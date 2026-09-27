//! The USB stack in the kernel (spec §6): the `Hal` it runs on.

use crate::mm::{self, paging::Cache};
use crate::{klogln, timer};
use core::fmt;
use core::time::Duration;
use usb::{DmaBuf, Hal};

/// Device registers through `mm::map_mmio` (uncached), DMA memory from the
/// frame allocator, the TSC for time and the kernel log.
#[derive(Clone, Copy, Debug, Default)]
pub struct KernelHal;

impl Hal for KernelHal {
    fn map_mmio(&self, phys: u64, len: usize) -> Option<usize> {
        match mm::map_mmio(phys, len as u64, Cache::Uncached) {
            Ok(p) => Some(p as usize),
            Err(e) => {
                klogln!("usb: cannot map {len:#x} bytes at {phys:#x}: {e}");
                None
            }
        }
    }

    unsafe fn read32(&self, addr: usize) -> u32 {
        // SAFETY: the caller keeps `addr` inside a range `map_mmio` mapped.
        unsafe { core::ptr::read_volatile(addr as *const u32) }
    }

    unsafe fn write32(&self, addr: usize, value: u32) {
        // SAFETY: as in `read32`.
        unsafe { core::ptr::write_volatile(addr as *mut u32, value) }
    }

    fn alloc_dma(&self, size: usize, align: usize) -> Option<DmaBuf> {
        let (virt, phys) = mm::alloc_dma(size, align)?;
        // SAFETY: fresh frames of at least `size` bytes, owned by this
        // buffer until `free_dma`.
        Some(unsafe { DmaBuf::new(virt, phys, size) })
    }

    fn free_dma(&self, buf: DmaBuf) {
        mm::free_dma(buf.phys(), buf.size());
    }

    fn now(&self) -> Duration {
        timer::tsc_time().unwrap_or_else(timer::uptime)
    }

    fn sleep(&self, d: Duration) {
        timer::sleep(d);
    }

    fn log(&self, args: fmt::Arguments) {
        klogln!("{args}");
    }
}
