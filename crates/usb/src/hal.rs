//! The hardware abstraction (spec §6.1): the only way this crate reaches
//! hardware.

use core::fmt;
use core::ptr::NonNull;
use core::time::Duration;

/// Memory a device can read and write: a virtual address for the CPU, a
/// physical address for the device, zeroed when allocated.
///
/// Every access goes through volatile reads and writes, because the device
/// changes the memory behind the compiler's back. A `DmaBuf` is not `Clone`:
/// [`Hal::free_dma`] consumes it, so no access can outlive the memory.
#[derive(Debug)]
pub struct DmaBuf {
    virt: NonNull<u8>,
    phys: u64,
    size: usize,
}

impl DmaBuf {
    /// # Safety
    /// `virt` must point to `size` bytes of memory that stay valid, and that
    /// nothing else writes to except the device at `phys`, until the buffer
    /// is given back to [`Hal::free_dma`].
    pub unsafe fn new(virt: NonNull<u8>, phys: u64, size: usize) -> DmaBuf {
        DmaBuf { virt, phys, size }
    }

    /// The physical address of byte 0, for the device.
    pub fn phys(&self) -> u64 {
        self.phys
    }

    /// The physical address of byte `offset`.
    pub fn phys_at(&self, offset: usize) -> u64 {
        assert!(
            offset <= self.size,
            "DMA offset {offset} beyond {}",
            self.size
        );
        self.phys + offset as u64
    }

    pub fn size(&self) -> usize {
        self.size
    }

    /// The CPU's address of byte 0 (for the `Hal` that frees it).
    pub fn virt(&self) -> NonNull<u8> {
        self.virt
    }

    fn check(&self, offset: usize, len: usize) {
        assert!(
            offset.checked_add(len).is_some_and(|end| end <= self.size),
            "DMA access {offset}+{len} beyond {}",
            self.size
        );
    }

    pub fn read32(&self, offset: usize) -> u32 {
        self.check(offset, 4);
        assert!(offset.is_multiple_of(4), "unaligned DMA read at {offset}");
        // SAFETY: inside the buffer (checked), aligned, valid until freed.
        unsafe { self.virt.add(offset).cast::<u32>().read_volatile() }
    }

    pub fn write32(&self, offset: usize, value: u32) {
        self.check(offset, 4);
        assert!(offset.is_multiple_of(4), "unaligned DMA write at {offset}");
        // SAFETY: as in `read32`.
        unsafe { self.virt.add(offset).cast::<u32>().write_volatile(value) }
    }

    pub fn read64(&self, offset: usize) -> u64 {
        self.read32(offset) as u64 | (self.read32(offset + 4) as u64) << 32
    }

    pub fn write64(&self, offset: usize, value: u64) {
        self.write32(offset, value as u32);
        self.write32(offset + 4, (value >> 32) as u32);
    }

    /// Copies `out.len()` bytes starting at `offset` out of the buffer.
    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) {
        self.check(offset, out.len());
        for (i, b) in out.iter_mut().enumerate() {
            // SAFETY: inside the buffer (checked).
            *b = unsafe { self.virt.add(offset + i).read_volatile() };
        }
    }

    /// Copies `data` into the buffer starting at `offset`.
    pub fn write_bytes(&self, offset: usize, data: &[u8]) {
        self.check(offset, data.len());
        for (i, &b) in data.iter().enumerate() {
            // SAFETY: inside the buffer (checked).
            unsafe { self.virt.add(offset + i).write_volatile(b) };
        }
    }

    /// Sets `len` bytes from `offset` to zero.
    pub fn zero(&self, offset: usize, len: usize) {
        self.check(offset, len);
        for i in 0..len {
            // SAFETY: inside the buffer (checked).
            unsafe { self.virt.add(offset + i).write_volatile(0) };
        }
    }
}

/// What the USB stack needs from the machine. The kernel implements it over
/// `mm::map_mmio`, the frame allocator, the timer and the kernel log; tests
/// implement it over a fake controller.
pub trait Hal {
    /// Maps `len` bytes of device registers at physical `phys`, uncached, and
    /// returns their virtual address. `None` if they cannot be mapped.
    fn map_mmio(&self, phys: u64, len: usize) -> Option<usize>;
    /// Reads the 32-bit device register at virtual address `addr`.
    ///
    /// # Safety
    /// `addr` is 4-byte aligned and inside a range `map_mmio` returned.
    unsafe fn read32(&self, addr: usize) -> u32;
    /// Writes the 32-bit device register at virtual address `addr`.
    ///
    /// # Safety
    /// As for [`Hal::read32`].
    unsafe fn write32(&self, addr: usize, value: u32);
    /// `size` bytes of zeroed, physically contiguous memory starting on a
    /// 4 KiB page boundary, or on `align` bytes (a power of two) if that is
    /// larger. So a buffer of at most 4 KiB never crosses a page. `None`
    /// when memory has run out.
    fn alloc_dma(&self, size: usize, align: usize) -> Option<DmaBuf>;
    /// Gives memory from `alloc_dma` back.
    fn free_dma(&self, buf: DmaBuf);
    /// Monotonic time since boot.
    fn now(&self) -> Duration;
    /// Waits at least `d`, busy (there are no device interrupts).
    fn sleep(&self, d: Duration);
    /// Adds one line to the kernel log (`dmesg`).
    fn log(&self, args: fmt::Arguments);
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::alloc::{Layout, alloc_zeroed, dealloc};

    fn buf(size: usize) -> (DmaBuf, Layout) {
        let layout = Layout::from_size_align(size, 64).unwrap();
        let p = NonNull::new(unsafe { alloc_zeroed(layout) }).unwrap();
        (unsafe { DmaBuf::new(p, 0x1000, size) }, layout)
    }

    #[test]
    fn reads_and_writes_little_endian_words_and_bytes() {
        let (b, layout) = buf(64);
        b.write32(4, 0x1122_3344);
        b.write64(8, 0x5566_7788_99AA_BBCC);
        let mut out = [0u8; 4];
        b.read_bytes(4, &mut out);
        assert_eq!(out, [0x44, 0x33, 0x22, 0x11]);
        assert_eq!(b.read32(8), 0x99AA_BBCC);
        assert_eq!(b.read64(8), 0x5566_7788_99AA_BBCC);
        b.write_bytes(62, &[1, 2]);
        assert_eq!(b.read32(60), 0x0201_0000);
        b.zero(60, 4);
        assert_eq!(b.read32(60), 0);
        assert_eq!(b.phys_at(16), 0x1010);
        unsafe { dealloc(b.virt().as_ptr(), layout) };
    }

    #[test]
    #[should_panic(expected = "beyond 64")]
    fn accesses_past_the_end_panic() {
        let (b, _layout) = buf(64);
        b.read32(62);
    }

    #[test]
    #[should_panic(expected = "beyond 64")]
    fn byte_copies_past_the_end_panic() {
        let (b, _layout) = buf(64);
        b.write_bytes(60, &[0; 5]);
    }
}
