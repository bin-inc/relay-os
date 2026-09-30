//! `t-mem KIND`: memory from `mem_map` (spec §5.1, §7.3), each answer
//! printed as `<what>: <value or error name>`.
//!
//! - `t-mem map`: the first map at the area's start, zeroed and writable;
//!   `mem_unmap` of part of it, and of what it did not map; a hole filled
//!   first fit; too much refused; and the free frames the same after.
//! - `t-mem unmapped`: reads a page after giving it back, which must kill
//!   it (`killed (page fault at 0x100000000000, read, …)`); `t-mem
//!   unmapped-many` the last of 256 pages given back together.
#![no_std]
#![no_main]

use core::fmt::Write;
use relay_abi::errno;
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

/// Where the `mem_map` area starts (spec §5.1).
const AREA: usize = 0x1000_0000_0000;
const PAGE: usize = 4096;

fn main(args: Args) -> u8 {
    let r = match args.get(1) {
        Some(b"map") => map(),
        Some(b"unmapped") => unmapped(1),
        Some(b"unmapped-many") => unmapped(256),
        _ => {
            let _ = sys::write_all(2, b"usage: t-mem map|unmapped|unmapped-many\n");
            return 2;
        }
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(Fd(2), "t-mem: {}", errno::name(e).unwrap_or("?"));
            1
        }
    }
}

fn show<T: core::fmt::Display>(what: &str, r: Result<T, u16>) {
    let _ = match r {
        Ok(v) => writeln!(Fd(1), "{what}: {v}"),
        Err(e) => writeln!(Fd(1), "{what}: {}", errno::name(e).unwrap_or("?")),
    };
}

fn free() -> Result<u64, u16> {
    Ok(sys::memory()?.ram_free / 4096)
}

fn map() -> Result<(), u16> {
    // A first map makes the page tables of the area's first 4 MiB, which
    // stay: count after it.
    let warm = sys::mem_map(4 << 20)?;
    // SAFETY: nothing uses it.
    unsafe { sys::mem_unmap(warm, 4 << 20)? };
    let before = free()?;
    let a = sys::mem_map(3 * PAGE)?;
    let _ = writeln!(Fd(1), "at the area's start: {}", a == AREA);
    // SAFETY: three pages `mem_map` just gave.
    let mem = unsafe { core::slice::from_raw_parts_mut(a as *mut u8, 3 * PAGE) };
    let _ = writeln!(Fd(1), "zeroed: {}", mem.iter().all(|&b| b == 0));
    mem.fill(0xA5);
    let _ = writeln!(Fd(1), "written: {}", mem[3 * PAGE - 1] == 0xA5);
    show("map nothing", sys::mem_map(0));
    show("map too much", sys::mem_map(1 << 46));
    // SAFETY: the middle page is not used any more; the others are not
    // this program's, or not the start of a page.
    unsafe {
        show(
            "unmap the middle",
            sys::mem_unmap(a + PAGE, 1).map(|()| "ok"),
        );
        show(
            "unmap it again",
            sys::mem_unmap(a + PAGE, PAGE).map(|()| "ok"),
        );
        show(
            "unmap half a page in",
            sys::mem_unmap(a + PAGE / 2, PAGE).map(|()| "ok"),
        );
        show(
            "unmap the program's code",
            sys::mem_unmap(0x40_0000, PAGE).map(|()| "ok"),
        );
    }
    let hole = sys::mem_map(PAGE)?;
    let _ = writeln!(Fd(1), "the hole is filled first: {}", hole == a + PAGE);
    // SAFETY: nothing uses them any more.
    unsafe {
        sys::mem_unmap(a, 3 * PAGE)?;
    }
    // Many pages, mapped and given back.
    let big = sys::mem_map(4 << 20)?;
    unsafe { sys::mem_unmap(big, 4 << 20)? };
    let lost = i128::from(before) - i128::from(free()?);
    let _ = writeln!(Fd(1), "frames lost: {lost}");
    Ok(())
}

/// Maps `pages` pages, writes the last, gives them back and reads it.
fn unmapped(pages: usize) -> Result<(), u16> {
    let a = sys::mem_map(pages * PAGE)?;
    let last = a + (pages - 1) * PAGE;
    // SAFETY: a page `mem_map` gave.
    unsafe { (last as *mut u8).write_volatile(1) };
    // SAFETY: the read after it is the test: it must fault.
    unsafe {
        sys::mem_unmap(a, pages * PAGE)?;
        let b = (last as *const u8).read_volatile();
        let _ = writeln!(Fd(1), "read an unmapped page: {b}");
    }
    Ok(())
}
