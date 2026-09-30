//! Memory a program names in a system call (user-space gate §11.1): a
//! range of its own address space, checked page by page against its page
//! tables before a byte is copied in or out. A pointer into the upper half, past the
//! lower half's end, onto a page the program does not have, or one that
//! wraps around is `EFAULT`, never a kernel fault. The bytes are copied
//! through the frames the tables name, so the kernel never touches a
//! program's addresses itself, and nothing can unmap them during the call
//! (§6.1).

use super::paging::{LOWER_HALF_END, PAGE, Perm, PhysMem};
use super::space::AddressSpace;
use alloc::vec::Vec;
use vfs::Errno;

/// `len` bytes at `addr` in a program's memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserSlice {
    addr: u64,
    len: u64,
}

impl UserSlice {
    /// A range that lies in the lower half; `EFAULT` otherwise. Whether
    /// its pages are mapped is checked when it is copied.
    pub fn new(addr: u64, len: u64) -> Result<UserSlice, Errno> {
        match addr.checked_add(len) {
            Some(end) if end <= LOWER_HALF_END => Ok(UserSlice { addr, len }),
            _ => Err(Errno::EFAULT),
        }
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Copies `buf.len()` bytes from `offset` into the range to `buf`.
    /// `EFAULT` if they run past the range or any of their pages is not
    /// one of the program's; `buf` may then hold part of them.
    pub fn read(
        &self,
        space: &AddressSpace,
        mem: &mut impl PhysMem,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<(), Errno> {
        let end = offset
            .checked_add(buf.len() as u64)
            .filter(|&end| end <= self.len)
            .ok_or(Errno::EFAULT)?;
        let mut virt = self.addr + offset;
        let mut done = 0;
        while virt < self.addr + end {
            let (frame, _) = space.user_page(mem, virt).ok_or(Errno::EFAULT)?;
            let at = (virt % PAGE) as usize;
            let n = (PAGE as usize - at).min(buf.len() - done);
            buf[done..done + n].copy_from_slice(&mem.bytes(frame)[at..at + n]);
            done += n;
            virt += n as u64;
        }
        Ok(())
    }

    /// Copies `bytes` into the range from `offset`. Every page they cover
    /// must be one of the program's and writable, or nothing is written
    /// and the answer is `EFAULT`: a `WaitStatus` or a `Time` arrives
    /// whole or not at all.
    pub fn write(
        &self,
        space: &AddressSpace,
        mem: &mut impl PhysMem,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), Errno> {
        let end = offset
            .checked_add(bytes.len() as u64)
            .filter(|&end| end <= self.len)
            .ok_or(Errno::EFAULT)?;
        let (start, end) = (self.addr + offset, self.addr + end);
        writable(space, mem, start, end)?;
        let mut virt = start;
        let mut done = 0;
        while virt < end {
            let (frame, _) = space.user_page(mem, virt).ok_or(Errno::EFAULT)?;
            let at = (virt % PAGE) as usize;
            let n = (PAGE as usize - at).min(bytes.len() - done);
            mem.bytes(frame)[at..at + n].copy_from_slice(&bytes[done..done + n]);
            done += n;
            virt += n as u64;
        }
        Ok(())
    }

    /// Whether every page of the range is one of the program's and
    /// writable; `EFAULT` otherwise. Nothing is written.
    pub fn check_writable(
        &self,
        space: &AddressSpace,
        mem: &mut impl PhysMem,
    ) -> Result<(), Errno> {
        writable(space, mem, self.addr, self.addr + self.len)
    }
}

/// `EFAULT` unless every page from `start` up to `end` is the program's
/// and writable.
fn writable(
    space: &AddressSpace,
    mem: &mut impl PhysMem,
    start: u64,
    end: u64,
) -> Result<(), Errno> {
    let mut page = start - start % PAGE;
    while page < end {
        match space.user_page(mem, page) {
            Some((_, Perm::ReadWrite)) => page += PAGE,
            _ => return Err(Errno::EFAULT),
        }
    }
    Ok(())
}

/// A byte string a program passes (a path, its children's arguments):
/// `len` bytes at `addr`, at most `max` of them. Paths and arguments are
/// bytes, not necessarily UTF-8, and carry their length (spec §7.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserStr {
    slice: UserSlice,
}

impl UserStr {
    /// `too_long` (`ENAMETOOLONG` for a path, `E2BIG` for arguments) if
    /// `len` is over `max`, before anything is read or allocated; `EFAULT`
    /// if the range is not in the lower half.
    pub fn new(addr: u64, len: u64, max: usize, too_long: Errno) -> Result<UserStr, Errno> {
        if len > max as u64 {
            return Err(too_long);
        }
        Ok(UserStr {
            slice: UserSlice::new(addr, len)?,
        })
    }

    /// The bytes, copied into the kernel; `EFAULT` if any of them is not
    /// the program's.
    pub fn read(&self, space: &AddressSpace, mem: &mut impl PhysMem) -> Result<Vec<u8>, Errno> {
        let mut buf = alloc::vec![0; self.slice.len() as usize];
        self.slice.read(space, mem, 0, &mut buf)?;
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mm::paging::{PageTables, Perm};
    use crate::mm::testing::FakeMem;

    const U: u64 = 0x40_0000;

    /// Two readable pages at `U` (code, then read-only data), a hole, then
    /// two writable pages; and the last page of the lower half.
    fn program(m: &mut FakeMem) -> AddressSpace {
        let mut k = PageTables::new(m).unwrap();
        k.fill_upper_half(m).unwrap();
        let mut s = AddressSpace::new(m, &k).unwrap();
        s.map_zeroed(m, U, 1, Perm::ReadExec).unwrap();
        s.map_zeroed(m, U + PAGE, 1, Perm::Read).unwrap();
        s.map_zeroed(m, U + 3 * PAGE, 2, Perm::ReadWrite).unwrap();
        s.map_zeroed(m, LOWER_HALF_END - PAGE, 1, Perm::ReadWrite)
            .unwrap();
        let text: Vec<u8> = (0..2 * PAGE).map(|i| i as u8).collect();
        s.fill(m, U, &text).unwrap();
        s
    }

    fn read(m: &mut FakeMem, s: &AddressSpace, addr: u64, len: u64) -> Result<Vec<u8>, Errno> {
        let slice = UserSlice::new(addr, len)?;
        let mut buf = vec![0; len as usize];
        slice.read(s, m, 0, &mut buf).map(|()| buf)
    }

    #[test]
    fn a_range_is_read_across_pages_of_any_permission() {
        let mut m = FakeMem::new();
        let s = program(&mut m);
        let want: Vec<u8> = (PAGE - 3..PAGE + 5).map(|i| i as u8).collect();
        assert_eq!(read(&mut m, &s, U + PAGE - 3, 8), Ok(want));
        assert_eq!(read(&mut m, &s, U + 3 * PAGE, 4), Ok(vec![0; 4]));
        let whole = read(&mut m, &s, U, 2 * PAGE).unwrap();
        assert_eq!(whole[4097], 1);
        // In pieces, at an offset.
        let slice = UserSlice::new(U + 10, 100).unwrap();
        let mut buf = [0; 5];
        slice.read(&s, &mut m, 90, &mut buf).unwrap();
        assert_eq!(buf, [100, 101, 102, 103, 104]);
        assert_eq!(
            slice.read(&s, &mut m, 96, &mut buf),
            Err(Errno::EFAULT),
            "past its end"
        );
        assert_eq!(
            slice.read(&s, &mut m, u64::MAX, &mut buf),
            Err(Errno::EFAULT)
        );
    }

    #[test]
    fn a_page_the_program_does_not_have_is_efault() {
        let mut m = FakeMem::new();
        let s = program(&mut m);
        assert_eq!(
            read(&mut m, &s, U + 2 * PAGE, 1),
            Err(Errno::EFAULT),
            "the hole"
        );
        assert_eq!(
            read(&mut m, &s, U + 2 * PAGE - 1, 2),
            Err(Errno::EFAULT),
            "into the hole"
        );
        assert_eq!(read(&mut m, &s, 0, 1), Err(Errno::EFAULT), "null");
        assert_eq!(read(&mut m, &s, U - 1, 2), Err(Errno::EFAULT), "from below");
    }

    #[test]
    fn nothing_outside_the_lower_half_is_a_program_s() {
        let mut m = FakeMem::new();
        let s = program(&mut m);
        let last = LOWER_HALF_END - PAGE;
        assert_eq!(read(&mut m, &s, last, PAGE), Ok(vec![0; PAGE as usize]));
        assert_eq!(
            read(&mut m, &s, last, PAGE + 1),
            Err(Errno::EFAULT),
            "across the end"
        );
        assert_eq!(UserSlice::new(LOWER_HALF_END, 1), Err(Errno::EFAULT));
        assert_eq!(
            UserSlice::new(0xFFFF_8000_0000_0000, 8),
            Err(Errno::EFAULT),
            "the kernel"
        );
        assert_eq!(UserSlice::new(0xFFFF_FFFF_8000_0000, 8), Err(Errno::EFAULT));
        assert_eq!(
            UserSlice::new(u64::MAX, 2),
            Err(Errno::EFAULT),
            "wraps around"
        );
        assert_eq!(UserSlice::new(U, u64::MAX), Err(Errno::EFAULT));
    }

    #[test]
    fn an_empty_range_reads_nothing_anywhere_below_the_end() {
        let mut m = FakeMem::new();
        let s = program(&mut m);
        assert_eq!(read(&mut m, &s, 0, 0), Ok(vec![]));
        assert_eq!(read(&mut m, &s, LOWER_HALF_END, 0), Ok(vec![]));
        assert!(UserSlice::new(U, 0).unwrap().is_empty());
        assert_eq!(UserSlice::new(U, 7).unwrap().len(), 7);
    }

    fn write(m: &mut FakeMem, s: &AddressSpace, addr: u64, bytes: &[u8]) -> Result<(), Errno> {
        UserSlice::new(addr, bytes.len() as u64)?.write(s, m, 0, bytes)
    }

    #[test]
    fn a_range_is_written_across_writable_pages() {
        let mut m = FakeMem::new();
        let s = program(&mut m);
        let at = U + 4 * PAGE - 3;
        assert_eq!(write(&mut m, &s, at, b"abcdefgh"), Ok(()));
        assert_eq!(read(&mut m, &s, at, 8), Ok(b"abcdefgh".to_vec()));
        // At an offset in a longer range.
        let slice = UserSlice::new(U + 3 * PAGE, 100).unwrap();
        slice.write(&s, &mut m, 90, b"xyz").unwrap();
        assert_eq!(read(&mut m, &s, U + 3 * PAGE + 90, 3), Ok(b"xyz".to_vec()));
        assert_eq!(
            slice.write(&s, &mut m, 98, b"xyz"),
            Err(Errno::EFAULT),
            "past its end"
        );
        assert_eq!(slice.write(&s, &mut m, u64::MAX, b"x"), Err(Errno::EFAULT));
        assert_eq!(write(&mut m, &s, LOWER_HALF_END - 2, b"ok"), Ok(()));
    }

    #[test]
    fn a_write_that_touches_a_page_it_may_not_write_writes_nothing() {
        let mut m = FakeMem::new();
        let s = program(&mut m);
        for (addr, why) in [
            (U, "code"),
            (U + PAGE, "read-only data"),
            (U + 2 * PAGE, "the hole"),
            (U + 5 * PAGE - 2, "past the writable pages"),
            (0, "null"),
        ] {
            assert_eq!(
                write(&mut m, &s, addr, b"abcd"),
                Err(Errno::EFAULT),
                "{why}"
            );
        }
        // The last write began on a writable page: its bytes there are not
        // written either.
        assert_eq!(read(&mut m, &s, U + 5 * PAGE - 2, 2), Ok(vec![0; 2]));
        assert_eq!(
            write(&mut m, &s, 0xFFFF_8000_0000_0000, b"k"),
            Err(Errno::EFAULT),
            "the kernel"
        );
        let code = read(&mut m, &s, U, 4).unwrap();
        assert_eq!(code, [0, 1, 2, 3], "the code is as it was");
    }

    #[test]
    fn a_string_is_copied_whole_and_no_longer_than_its_limit() {
        let mut m = FakeMem::new();
        let s = program(&mut m);
        let str = UserStr::new(U + PAGE - 2, 4, 4, Errno::ENAMETOOLONG).unwrap();
        assert_eq!(str.read(&s, &mut m), Ok(vec![254, 255, 0, 1]));
        assert_eq!(
            UserStr::new(U, 5, 4, Errno::ENAMETOOLONG),
            Err(Errno::ENAMETOOLONG),
            "one byte too long"
        );
        assert_eq!(
            UserStr::new(U, u64::MAX, 4, Errno::E2BIG),
            Err(Errno::E2BIG),
            "the length decides before the address"
        );
        assert_eq!(
            UserStr::new(u64::MAX, 2, 4, Errno::E2BIG),
            Err(Errno::EFAULT)
        );
        let hole = UserStr::new(U + 2 * PAGE - 1, 2, 4, Errno::E2BIG).unwrap();
        assert_eq!(hole.read(&s, &mut m), Err(Errno::EFAULT));
        let empty = UserStr::new(0, 0, 4, Errno::E2BIG).unwrap();
        assert_eq!(empty.read(&s, &mut m), Ok(vec![]));
    }
}
