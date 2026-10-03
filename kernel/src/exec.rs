//! A program in its address space (user-space gate §5.1–§5.3): its
//! segments on pages with their permissions, a 1 MiB stack below the top
//! of the lower half with an unmapped guard page beneath it, and its
//! arguments at the top of the stack with its environment just below them
//! (programmable shell gate §8.1). Architecture-neutral; the registers the
//! entry state goes into are the `arch` module's business.

use crate::mm::paging::{MapError, PAGE, Perm, PhysMem};
use crate::mm::space::AddressSpace;
use elf::{Access, Program};
use vfs::Errno;

/// The first address above the stack; the page from here up is never
/// mapped (spec §5.1).
pub const STACK_TOP: u64 = 0x7FFF_FFFF_F000;
/// The stack's pages: 1 MiB less one page.
pub const STACK_PAGES: u64 = 255;
/// The lowest address of the stack; the page below it is the guard page.
pub const STACK_BOTTOM: u64 = STACK_TOP - STACK_PAGES * PAGE;
/// Whether `address` is in the stack's guard page: a page fault there is
/// a stack overflow.
pub fn in_guard_page(address: u64) -> bool {
    (STACK_BOTTOM - PAGE..STACK_BOTTOM).contains(&address)
}

/// The most argument bytes a program gets (spec §5.3).
pub const ARGS_MAX: usize = 64 * 1024;
/// The most environment bytes a program gets, counted apart from the
/// arguments (programmable shell gate §8.1).
pub const ENV_MAX: usize = 64 * 1024;

/// Where a program starts: its first instruction, its stack pointer (16-byte
/// aligned, below the arguments and the environment), its arguments'
/// address, length and count (spec §5.3), and its environment's, all 0
/// for none (programmable shell gate §8.2). `arch::user::enter` reads it
/// (`repr(C)`) and puts each where its architecture says.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub ip: u64,
    pub sp: u64,
    pub args: u64,
    pub args_len: u64,
    pub argc: u64,
    pub env: u64,
    pub env_len: u64,
    pub envc: u64,
}

fn perm(access: Access) -> Perm {
    match access {
        Access::ReadExec => Perm::ReadExec,
        Access::Read => Perm::Read,
        Access::ReadWrite => Perm::ReadWrite,
    }
}

/// `ENOMEM` when the frames ran out; anything else cannot happen to a
/// program `elf::check` accepted.
fn errno(e: MapError) -> Errno {
    match e {
        MapError::OutOfMemory => Errno::ENOMEM,
        _ => Errno::ENOEXEC,
    }
}

/// Strings for the new stack, the arguments or the environment: `count` of
/// them, each followed by a NUL, in `bytes`.
#[derive(Clone, Copy, Debug)]
pub struct Strings<'a> {
    pub bytes: &'a [u8],
    pub count: u64,
}

impl Strings<'_> {
    /// No strings: no environment.
    pub const NONE: Strings<'static> = Strings {
        bytes: &[],
        count: 0,
    };
}

/// Maps `program` (read from `file`, checked by `elf::check`) and its stack
/// into `space` with `args` at the top of the stack and `env` just below
/// them. On an error the caller destroys `space`, which gives back whatever
/// was mapped.
pub fn load(
    space: &mut AddressSpace,
    mem: &mut impl PhysMem,
    file: &[u8],
    program: &Program,
    args: Strings<'_>,
    env: Strings<'_>,
) -> Result<Entry, Errno> {
    for s in &program.segments {
        let first = s.vaddr & !(PAGE - 1);
        let pages = (s.end().next_multiple_of(PAGE) - first) / PAGE;
        space
            .map_zeroed(mem, first, pages, perm(s.access))
            .map_err(errno)?;
        let bytes = usize::try_from(s.offset)
            .ok()
            .zip(usize::try_from(s.filesz).ok())
            .and_then(|(at, len)| file.get(at..at.checked_add(len)?))
            .ok_or(Errno::ENOEXEC)?;
        space.fill(mem, s.vaddr, bytes).map_err(errno)?;
    }
    space
        .map_zeroed(mem, STACK_BOTTOM, STACK_PAGES, Perm::ReadWrite)
        .map_err(errno)?;
    if args.bytes.len() > ARGS_MAX || env.bytes.len() > ENV_MAX {
        return Err(Errno::E2BIG);
    }
    let at = STACK_TOP - args.bytes.len() as u64;
    space.fill(mem, at, args.bytes).map_err(errno)?;
    let env_at = at - env.bytes.len() as u64;
    space.fill(mem, env_at, env.bytes).map_err(errno)?;
    Ok(Entry {
        ip: program.entry,
        sp: env_at & !15,
        args: at,
        args_len: args.bytes.len() as u64,
        argc: args.count,
        env: if env.bytes.is_empty() { 0 } else { env_at },
        env_len: env.bytes.len() as u64,
        envc: env.count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mm::paging::PageTables;
    use crate::mm::testing::FakeMem;
    use alloc::vec::Vec;
    use elf::Segment;

    /// The argument bytes for `args`: each argument, then a NUL.
    fn arg_bytes(args: &[&[u8]]) -> Vec<u8> {
        args.iter()
            .flat_map(|a| a.iter().copied().chain([0]))
            .collect()
    }

    fn strings(bytes: &[u8], count: u64) -> Strings<'_> {
        Strings { bytes, count }
    }

    fn space(m: &mut FakeMem) -> AddressSpace {
        let mut k = PageTables::new(m).unwrap();
        k.fill_upper_half(m).unwrap();
        AddressSpace::new(m, &k).unwrap()
    }

    fn read(m: &mut FakeMem, s: &AddressSpace, virt: u64, len: usize) -> Vec<u8> {
        (0..len as u64)
            .map(|i| {
                let (frame, _) = s.user_page(m, virt + i).expect("mapped");
                m.bytes(frame)[((virt + i) % PAGE) as usize]
            })
            .collect()
    }

    /// A file of 0x4000 bytes, each its offset's low byte, and a program
    /// shaped like `t-args` over it: code, read-only data, data and bss.
    fn program() -> (Vec<u8>, Program) {
        let file: Vec<u8> = (0..0x4000u32).map(|i| i as u8).collect();
        let seg = |vaddr, memsz, offset, filesz, access| Segment {
            vaddr,
            memsz,
            offset,
            filesz,
            access,
        };
        let program = Program {
            entry: 0x40_1010,
            segments: vec![
                seg(0x40_1010, 0x7F0, 0x1010, 0x7F0, Access::ReadExec),
                seg(0x40_2000, 0x1800, 0x2000, 0x1800, Access::Read),
                seg(0x40_4800, 0x2000, 0x2800, 0x100, Access::ReadWrite),
            ],
        };
        (file, program)
    }

    #[test]
    fn segments_get_their_bytes_permissions_and_zeroes() {
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let (file, p) = program();
        let args = arg_bytes(&[b"/bin/t"]);
        load(&mut s, &mut m, &file, &p, strings(&args, 1), Strings::NONE).unwrap();
        // Code from 0x401010, zeroes before it on its page.
        assert_eq!(read(&mut m, &s, 0x40_1000, 0x10), vec![0; 0x10]);
        assert_eq!(read(&mut m, &s, 0x40_1010, 4), [0x10, 0x11, 0x12, 0x13]);
        assert_eq!(s.user_page(&mut m, 0x40_17FF).unwrap().1, Perm::ReadExec);
        // Read-only data over two pages.
        assert_eq!(read(&mut m, &s, 0x40_37FE, 2), [0xFE, 0xFF]);
        assert_eq!(s.user_page(&mut m, 0x40_3000).unwrap().1, Perm::Read);
        // Data: 0x100 bytes from the file, then bss to the end of its pages.
        assert_eq!(read(&mut m, &s, 0x40_4800, 2), [0x00, 0x01]);
        assert_eq!(read(&mut m, &s, 0x40_48FF, 2), [0xFF, 0]);
        assert_eq!(read(&mut m, &s, 0x40_6000, 0x800), vec![0; 0x800]);
        assert_eq!(s.user_page(&mut m, 0x40_67FF).unwrap().1, Perm::ReadWrite);
        // Nothing in between or around.
        for v in [0x40_0000, 0x40_7000, 0x3F_F000] {
            assert_eq!(s.user_page(&mut m, v), None, "{v:#x}");
        }
        s.destroy(&mut m);
    }

    #[test]
    fn the_stack_is_1_mib_less_a_page_above_a_guard_page() {
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let (file, p) = program();
        load(
            &mut s,
            &mut m,
            &file,
            &p,
            strings(&arg_bytes(&[b"x"]), 1),
            Strings::NONE,
        )
        .unwrap();
        assert_eq!(STACK_BOTTOM, 0x7FFF_FFF0_0000);
        assert_eq!(
            s.user_page(&mut m, STACK_BOTTOM).unwrap().1,
            Perm::ReadWrite
        );
        assert_eq!(
            s.user_page(&mut m, STACK_TOP - 1).unwrap().1,
            Perm::ReadWrite
        );
        assert_eq!(s.user_page(&mut m, STACK_BOTTOM - 1), None, "guard page");
        assert_eq!(s.user_page(&mut m, STACK_TOP), None, "the top page");
        s.destroy(&mut m);
    }

    #[test]
    fn only_the_guard_page_is_an_overflow() {
        assert!(in_guard_page(STACK_BOTTOM - 8));
        assert!(in_guard_page(STACK_BOTTOM - PAGE));
        assert!(!in_guard_page(STACK_BOTTOM));
        assert!(!in_guard_page(STACK_BOTTOM - PAGE - 1));
        assert!(!in_guard_page(0));
    }

    #[test]
    fn the_arguments_are_at_the_top_with_the_stack_below_them() {
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let (file, p) = program();
        let args = arg_bytes(&[b"/bin/t-args", b"a", b"b c", b""]);
        assert_eq!(args, b"/bin/t-args\0a\0b c\0\0");
        let e = load(&mut s, &mut m, &file, &p, strings(&args, 4), Strings::NONE).unwrap();
        assert_eq!(e.ip, 0x40_1010);
        assert_eq!((e.args_len, e.argc), (19, 4));
        assert_eq!(e.args + e.args_len, STACK_TOP);
        assert_eq!(read(&mut m, &s, e.args, 19), args);
        assert_eq!(e.sp % 16, 0);
        assert!(e.sp <= e.args && e.args - e.sp < 16);
        // No environment: its address, length and count are all 0.
        assert_eq!((e.env, e.env_len, e.envc), (0, 0, 0));
        s.destroy(&mut m);
    }

    #[test]
    fn the_environment_is_just_below_the_arguments() {
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let (file, p) = program();
        let args = arg_bytes(&[b"/bin/t-env", b"x"]);
        let env = arg_bytes(&[b"HOME=/root", "A=\u{e9}t\u{e9}".as_bytes(), b""]);
        let e = load(
            &mut s,
            &mut m,
            &file,
            &p,
            strings(&args, 2),
            strings(&env, 3),
        )
        .unwrap();
        assert_eq!(e.args + e.args_len, STACK_TOP, "the arguments keep the top");
        assert_eq!((e.env_len, e.envc), (env.len() as u64, 3));
        assert_eq!(e.env + e.env_len, e.args, "just below them");
        assert_eq!(read(&mut m, &s, e.env, env.len()), env);
        assert_eq!(read(&mut m, &s, e.args, args.len()), args);
        assert_eq!(e.sp % 16, 0);
        assert!(e.sp <= e.env && e.env - e.sp < 16);
        s.destroy(&mut m);
    }

    #[test]
    fn arguments_up_to_64_kib() {
        let long = vec![b'x'; ARGS_MAX - 3];
        let args = arg_bytes(&[b"p", &long]);
        assert_eq!(args.len(), ARGS_MAX);
        // The most still fits, and lands at the top.
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let (file, p) = program();
        let e = load(&mut s, &mut m, &file, &p, strings(&args, 2), Strings::NONE).unwrap();
        assert_eq!(e.args, STACK_TOP - ARGS_MAX as u64);
        assert_eq!(read(&mut m, &s, STACK_TOP - 2, 2), b"x\0");
        let too_many = vec![1; ARGS_MAX + 1];
        let mut s2 = space(&mut m);
        assert_eq!(
            load(
                &mut s2,
                &mut m,
                &file,
                &p,
                strings(&too_many, 1),
                Strings::NONE
            ),
            Err(Errno::E2BIG)
        );
        s.destroy(&mut m);
        s2.destroy(&mut m);
    }

    #[test]
    fn both_blocks_at_64_kib_leave_the_stack_its_room() {
        let long = vec![b'x'; ARGS_MAX - 3];
        let args = arg_bytes(&[b"p", &long]);
        let entry = [&b"A="[..], &vec![b'v'; ENV_MAX - 3]].concat();
        let env = arg_bytes(&[&entry]);
        assert_eq!((args.len(), env.len()), (ARGS_MAX, ENV_MAX));
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let (file, p) = program();
        let e = load(
            &mut s,
            &mut m,
            &file,
            &p,
            strings(&args, 2),
            strings(&env, 1),
        )
        .unwrap();
        assert_eq!(e.args, STACK_TOP - ARGS_MAX as u64);
        assert_eq!(e.env, e.args - ENV_MAX as u64);
        assert_eq!(read(&mut m, &s, e.env, 3), b"A=v");
        assert_eq!(read(&mut m, &s, e.args - 2, 4), b"v\0p\0");
        // 892 KiB below them: /bin/sh at its nesting bound used 40 KiB
        // (programmable shell gate §15 item 6).
        assert_eq!(e.sp, e.env);
        assert_eq!(e.sp - STACK_BOTTOM, 892 * 1024);
        let mut s2 = space(&mut m);
        let too_much = vec![1; ENV_MAX + 1];
        assert_eq!(
            load(
                &mut s2,
                &mut m,
                &file,
                &p,
                strings(&args, 2),
                strings(&too_much, 0)
            ),
            Err(Errno::E2BIG)
        );
        s.destroy(&mut m);
        s2.destroy(&mut m);
    }

    #[test]
    fn a_program_that_does_not_fit_its_file_is_enoexec() {
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let (file, p) = program();
        let args = arg_bytes(&[b"p"]);
        assert_eq!(
            load(
                &mut s,
                &mut m,
                &file[..0x3000],
                &p,
                strings(&args, 1),
                Strings::NONE
            ),
            Err(Errno::ENOEXEC)
        );
        s.destroy(&mut m);
    }

    #[test]
    fn running_out_of_memory_is_enomem_and_leaks_nothing() {
        let mut m = FakeMem::new();
        let (file, p) = program();
        let args = arg_bytes(&[b"p"]);
        let mut k = PageTables::new(&mut m).unwrap();
        k.fill_upper_half(&mut m).unwrap();
        let before = m.frames();
        // Every limit from nothing up to what the whole program needs.
        let mut s = AddressSpace::new(&mut m, &k).unwrap();
        load(&mut s, &mut m, &file, &p, strings(&args, 1), Strings::NONE).unwrap();
        let needed = m.frames() - before;
        s.destroy(&mut m);
        for limit in (1..needed).step_by(7) {
            m.limit = before + limit;
            let mut s = AddressSpace::new(&mut m, &k).unwrap();
            assert_eq!(
                load(&mut s, &mut m, &file, &p, strings(&args, 1), Strings::NONE),
                Err(Errno::ENOMEM)
            );
            s.destroy(&mut m);
            assert_eq!(m.frames(), before, "limit {limit}");
        }
    }
}
