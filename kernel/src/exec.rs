//! A program in its address space (user-space gate §5.1–§5.3): its
//! segments on pages with their permissions, a 1 MiB stack below the top
//! of the lower half with an unmapped guard page beneath it, and its
//! arguments at the top of the stack. Architecture-neutral; the registers
//! the entry state goes into are the `arch` module's business.

use crate::mm::paging::{MapError, PAGE, Perm, PhysMem};
use crate::mm::space::AddressSpace;
use alloc::vec::Vec;
use elf::{Access, Program};
use vfs::Errno;

/// The first address above the stack; the page from here up is never
/// mapped (spec §5.1).
pub const STACK_TOP: u64 = 0x7FFF_FFFF_F000;
/// The stack's pages: 1 MiB less one page.
pub const STACK_PAGES: u64 = 255;
/// The lowest address of the stack; the page below it is the guard page.
pub const STACK_BOTTOM: u64 = STACK_TOP - STACK_PAGES * PAGE;
/// The most argument bytes a program gets (spec §5.3).
pub const ARGS_MAX: usize = 64 * 1024;

/// Where a program starts: its first instruction, its stack pointer (16-byte
/// aligned, below the arguments), and its arguments' address, length and
/// count (spec §5.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub ip: u64,
    pub sp: u64,
    pub args: u64,
    pub args_len: u64,
    pub argc: u64,
}

/// The argument bytes for `args` (argument 0 first): each argument, then a
/// NUL. An argument holding a NUL is `EINVAL`; more than `ARGS_MAX` bytes
/// is `E2BIG`.
pub fn arg_bytes(args: &[&[u8]]) -> Result<Vec<u8>, Errno> {
    let len: usize = args.iter().map(|a| a.len() + 1).sum();
    if len > ARGS_MAX {
        return Err(Errno::E2BIG);
    }
    let mut bytes = Vec::with_capacity(len);
    for a in args {
        if a.contains(&0) {
            return Err(Errno::EINVAL);
        }
        bytes.extend_from_slice(a);
        bytes.push(0);
    }
    Ok(bytes)
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

/// Maps `program` (read from `file`, checked by `elf::check`) and its stack
/// into `space` with `args` (from `arg_bytes`, `argc` of them) at the top
/// of the stack. On an error the caller destroys `space`, which gives
/// back whatever was mapped.
pub fn load(
    space: &mut AddressSpace,
    mem: &mut impl PhysMem,
    file: &[u8],
    program: &Program,
    args: &[u8],
    argc: u64,
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
    if args.len() > ARGS_MAX {
        return Err(Errno::E2BIG);
    }
    let at = STACK_TOP - args.len() as u64;
    space.fill(mem, at, args).map_err(errno)?;
    Ok(Entry {
        ip: program.entry,
        sp: at & !15,
        args: at,
        args_len: args.len() as u64,
        argc,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mm::paging::PageTables;
    use crate::mm::testing::FakeMem;
    use elf::Segment;

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
        let args = arg_bytes(&[b"/bin/t"]).unwrap();
        load(&mut s, &mut m, &file, &p, &args, 1).unwrap();
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
        load(&mut s, &mut m, &file, &p, &arg_bytes(&[b"x"]).unwrap(), 1).unwrap();
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
    fn the_arguments_are_at_the_top_with_the_stack_below_them() {
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let (file, p) = program();
        let args = arg_bytes(&[b"/bin/t-args", b"a", b"b c", b""]).unwrap();
        assert_eq!(args, b"/bin/t-args\0a\0b c\0\0");
        let e = load(&mut s, &mut m, &file, &p, &args, 4).unwrap();
        assert_eq!(e.ip, 0x40_1010);
        assert_eq!((e.args_len, e.argc), (19, 4));
        assert_eq!(e.args + e.args_len, STACK_TOP);
        assert_eq!(read(&mut m, &s, e.args, 19), args);
        assert_eq!(e.sp % 16, 0);
        assert!(e.sp <= e.args && e.args - e.sp < 16);
        s.destroy(&mut m);
    }

    #[test]
    fn arguments_up_to_64_kib() {
        let long = vec![b'x'; ARGS_MAX - 3];
        let args = arg_bytes(&[b"p", &long]).unwrap();
        assert_eq!(args.len(), ARGS_MAX);
        assert_eq!(arg_bytes(&[b"pp", &long]), Err(Errno::E2BIG));
        assert_eq!(arg_bytes(&[b"p", b"a\0b"]), Err(Errno::EINVAL));
        // The most still fits, and lands at the top.
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let (file, p) = program();
        let e = load(&mut s, &mut m, &file, &p, &args, 2).unwrap();
        assert_eq!(e.args, STACK_TOP - ARGS_MAX as u64);
        assert_eq!(read(&mut m, &s, STACK_TOP - 2, 2), b"x\0");
        let too_many = vec![1; ARGS_MAX + 1];
        let mut s2 = space(&mut m);
        assert_eq!(
            load(&mut s2, &mut m, &file, &p, &too_many, 1),
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
        let args = arg_bytes(&[b"p"]).unwrap();
        assert_eq!(
            load(&mut s, &mut m, &file[..0x3000], &p, &args, 1),
            Err(Errno::ENOEXEC)
        );
        s.destroy(&mut m);
    }

    #[test]
    fn running_out_of_memory_is_enomem_and_leaks_nothing() {
        let mut m = FakeMem::new();
        let (file, p) = program();
        let args = arg_bytes(&[b"p"]).unwrap();
        let mut k = PageTables::new(&mut m).unwrap();
        k.fill_upper_half(&mut m).unwrap();
        let before = m.frames();
        // Every limit from nothing up to what the whole program needs.
        let mut s = AddressSpace::new(&mut m, &k).unwrap();
        load(&mut s, &mut m, &file, &p, &args, 1).unwrap();
        let needed = m.frames() - before;
        s.destroy(&mut m);
        for limit in (1..needed).step_by(7) {
            m.limit = before + limit;
            let mut s = AddressSpace::new(&mut m, &k).unwrap();
            assert_eq!(
                load(&mut s, &mut m, &file, &p, &args, 1),
                Err(Errno::ENOMEM)
            );
            s.destroy(&mut m);
            assert_eq!(m.frames(), before, "limit {limit}");
        }
    }
}
