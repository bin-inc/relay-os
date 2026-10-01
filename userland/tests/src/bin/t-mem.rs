//! `t-mem KIND`: memory from `mem_map` (spec §5.1, §7.3), each answer
//! printed as `<what>: <value or error name>`.
//!
//! - `t-mem map`: the first map at the area's start, zeroed and writable;
//!   `mem_unmap` of part of it, and of what it did not map; a hole filled
//!   first fit; too much refused; and the free frames the same after.
//! - `t-mem unmapped`: reads a page after giving it back, which must kill
//!   it (`killed (page fault at 0x100000000000, read, …)`); `t-mem
//!   unmapped-many` the last of 256 pages given back together.
//! - `t-mem grow N`: `N` MiB on the heap (spec §8.1) in blocks of every
//!   size, each checked, which `relay-rt` maps as it grows.
//! - `t-mem oom`: starts `t-mem hog`, which takes heap until there is
//!   none (`t-mem: out of memory`), and says how it ended: status 134.
//! - `t-mem churn`: maps and gives back nearly all free memory over and
//!   over for at most 20 s, so that it spends its time in the kernel,
//!   zeroing and freeing pages, and almost never in ring 3; then `churn:
//!   not stopped`. Ctrl-C must stop it all the same.
#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
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
        Some(b"grow") => match args.get(2).and_then(parse) {
            Some(n) => grow(n),
            None => return usage(),
        },
        Some(b"oom") => oom(),
        Some(b"hog") => hog(),
        Some(b"churn") => churn(),
        _ => return usage(),
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

fn usage() -> u8 {
    let _ = sys::write_all(
        2,
        b"usage: t-mem map|unmapped|unmapped-many|grow N|oom|churn\n",
    );
    2
}

/// A decimal number.
fn parse(s: &[u8]) -> Option<usize> {
    if s.is_empty() {
        return None;
    }
    s.iter().try_fold(0usize, |n, &c| {
        let d = c.checked_sub(b'0').filter(|d| *d <= 9)?;
        n.checked_mul(10)?.checked_add(usize::from(d))
    })
}

/// `mib` MiB in blocks of 16 bytes to 256 KiB, each filled with its own
/// byte and checked once all are there.
fn grow(mib: usize) -> Result<(), u16> {
    let mut blocks: Vec<Vec<u8>> = Vec::new();
    let mut total = 0;
    let mut size = 16;
    while total < mib << 20 {
        let tag = blocks.len() as u8;
        blocks.push(alloc::vec![tag; size]);
        total += size;
        size = if size >= 256 << 10 { 16 } else { size * 2 };
    }
    let all_there = blocks
        .iter()
        .enumerate()
        .all(|(i, b)| b.iter().all(|&x| x == i as u8));
    let _ = writeln!(
        Fd(1),
        "{} blocks, {} MiB, all there: {all_there}",
        blocks.len(),
        total >> 20
    );
    Ok(())
}

fn oom() -> Result<(), u16> {
    let fds = [
        relay_abi::FdMap {
            child: 1,
            parent: 1,
        },
        relay_abi::FdMap {
            child: 2,
            parent: 2,
        },
    ];
    let pid = sys::spawn(b"/bin/t-mem", b"t-mem\0hog\0", b"", &fds, 0, 0)?;
    if let Some((_, w)) = sys::wait(i64::from(pid), false)? {
        let _ = writeln!(Fd(1), "the hog: {w}");
    }
    Ok(())
}

/// How long `churn` goes on if nothing stops it.
const CHURN_NS: u64 = 20_000_000_000;

fn churn() -> Result<(), u16> {
    // As much as leaves 16 MiB (8 MiB above the kernel's reserve), at most
    // 1 GiB: each call zeroes or frees it all, for tens of milliseconds.
    let len = (free()? * 4096).saturating_sub(16 << 20).min(1 << 30) as usize & !(PAGE - 1);
    let start = sys::time()?.uptime_ns;
    loop {
        for _ in 0..16 {
            let a = sys::mem_map(len)?;
            // SAFETY: nothing uses it.
            unsafe { sys::mem_unmap(a, len)? };
        }
        if sys::time()?.uptime_ns - start >= CHURN_NS {
            break;
        }
    }
    let _ = writeln!(Fd(1), "churn: not stopped");
    Ok(())
}

/// Takes the heap a mebibyte at a time until `relay-rt` says it is out.
fn hog() -> Result<(), u16> {
    let mut blocks: Vec<Vec<u8>> = Vec::new();
    loop {
        blocks.push(alloc::vec![1; 1 << 20]);
    }
}
