//! A fake [`Hal`]: virtual time, DMA memory from the host heap (its
//! physical address is its host address), captured log lines, and the
//! registers of a [`FakeXhci`], which sees every access and every tick.

use super::xhci::{FakeConfig, FakeXhci};
use crate::{DmaBuf, Hal};
use core::fmt;
use core::ptr::NonNull;
use core::time::Duration;
use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::cell::{Cell, RefCell, RefMut};
use std::collections::BTreeMap;
use std::rc::Rc;

/// The fake controller's BAR: the only MMIO `map_mmio` accepts.
pub const FAKE_BAR: u64 = 0xFE00_0000;
pub const FAKE_BAR_LEN: usize = 0x1_0000;
/// Where the BAR appears to be mapped.
const FAKE_MMIO: usize = 0xFFFF_9000_FE00_0000;

/// A shared handle: the driver owns one clone, the test another.
#[derive(Clone)]
pub struct FakeHal(Rc<Inner>);

struct Inner {
    clock: Cell<Duration>,
    dma: RefCell<Dma>,
    log: RefCell<Vec<String>>,
    xhci: RefCell<Option<FakeXhci>>,
}

/// Every DMA buffer handed out and not yet freed, by physical address.
#[derive(Default)]
pub struct Dma {
    buffers: BTreeMap<u64, (NonNull<u8>, Layout)>,
    /// Allocations that still succeed; `None` is no limit.
    allocs_left: Option<usize>,
    /// The one allocation (counting from 0) that fails, if any.
    failing: Option<usize>,
}

impl Dma {
    /// The host pointer to `len` bytes at `phys`, which must lie inside one
    /// allocated buffer: the controller never reaches other memory.
    fn find(&self, phys: u64, len: usize) -> *mut u8 {
        let hit = self.buffers.range(..=phys).next_back();
        match hit {
            Some((&start, &(ptr, layout))) if phys + len as u64 <= start + layout.size() as u64 => {
                // SAFETY: inside the allocation (checked just above).
                unsafe { ptr.as_ptr().add((phys - start) as usize) }
            }
            _ => panic!("fake xhci: access to unallocated DMA memory at {phys:#x}"),
        }
    }

    /// Whether `len` bytes at `phys` lie inside one allocated buffer.
    pub fn contains(&self, phys: u64, len: usize) -> bool {
        self.buffers
            .range(..=phys)
            .next_back()
            .is_some_and(|(&start, &(_, layout))| phys + len as u64 <= start + layout.size() as u64)
    }

    pub fn read32(&self, phys: u64) -> u32 {
        assert!(phys.is_multiple_of(4), "fake xhci: unaligned DMA read");
        // SAFETY: `find` checked the range; the address is aligned.
        unsafe { self.find(phys, 4).cast::<u32>().read_volatile() }
    }

    pub fn write32(&self, phys: u64, value: u32) {
        assert!(phys.is_multiple_of(4), "fake xhci: unaligned DMA write");
        // SAFETY: as in `read32`.
        unsafe { self.find(phys, 4).cast::<u32>().write_volatile(value) }
    }

    pub fn read64(&self, phys: u64) -> u64 {
        self.read32(phys) as u64 | (self.read32(phys + 4) as u64) << 32
    }

    pub fn write64(&self, phys: u64, value: u64) {
        self.write32(phys, value as u32);
        self.write32(phys + 4, (value >> 32) as u32);
    }

    pub fn read_bytes(&self, phys: u64, len: usize) -> Vec<u8> {
        let p = self.find(phys, len);
        // SAFETY: `find` checked the range.
        (0..len)
            .map(|i| unsafe { p.add(i).read_volatile() })
            .collect()
    }

    pub fn write_bytes(&self, phys: u64, data: &[u8]) {
        let p = self.find(phys, data.len());
        for (i, &b) in data.iter().enumerate() {
            // SAFETY: `find` checked the range.
            unsafe { p.add(i).write_volatile(b) };
        }
    }
}

impl Drop for Dma {
    fn drop(&mut self) {
        for (ptr, layout) in self.buffers.values() {
            // SAFETY: allocated with this layout and not freed yet.
            unsafe { dealloc(ptr.as_ptr(), *layout) };
        }
    }
}

impl Default for FakeHal {
    fn default() -> FakeHal {
        FakeHal::new()
    }
}

impl FakeHal {
    /// A machine without a controller (for rings and contexts).
    pub fn new() -> FakeHal {
        FakeHal(Rc::new(Inner {
            clock: Cell::new(Duration::ZERO),
            dma: RefCell::new(Dma::default()),
            log: RefCell::new(Vec::new()),
            xhci: RefCell::new(None),
        }))
    }

    /// A machine with a fake controller at [`FAKE_BAR`].
    pub fn with_controller(config: FakeConfig) -> FakeHal {
        let hal = FakeHal::new();
        *hal.0.xhci.borrow_mut() = Some(FakeXhci::new(config));
        hal
    }

    /// Runs `f` on the fake controller with the DMA memory it reaches (to
    /// post events or read what the driver wrote).
    pub fn act<R>(&self, f: impl FnOnce(&mut FakeXhci, &Dma) -> R) -> R {
        let mut x = self.fake();
        f(&mut x, &self.0.dma.borrow())
    }

    /// The fake controller, to plug devices, turn knobs and look inside.
    /// Drop the guard before calling the driver again.
    pub fn fake(&self) -> RefMut<'_, FakeXhci> {
        RefMut::map(self.0.xhci.borrow_mut(), |x| {
            x.as_mut().expect("fake hal: no controller")
        })
    }

    /// The virtual time, without advancing it (`now` advances it).
    pub fn clock(&self) -> Duration {
        self.0.clock.get()
    }

    /// DMA buffers allocated and not freed.
    pub fn outstanding_dma(&self) -> usize {
        self.0.dma.borrow().buffers.len()
    }

    /// Lets `n` more allocations succeed, then fails every one.
    pub fn fail_alloc_after(&self, n: usize) {
        self.0.dma.borrow_mut().allocs_left = Some(n);
    }

    /// Fails only the allocation `n` from now (0 is the next one).
    pub fn fail_one_alloc(&self, n: usize) {
        self.0.dma.borrow_mut().failing = Some(n);
    }

    /// Every log line so far, one per line.
    pub fn log_text(&self) -> String {
        self.0.log.borrow().join("\n")
    }

    fn advance(&self, d: Duration) {
        let now = self.0.clock.get() + d;
        self.0.clock.set(now);
        if let Some(x) = self.0.xhci.borrow_mut().as_mut() {
            x.advance_to(now, &self.0.dma.borrow());
        }
    }

    /// The BAR offset of a mapped register address.
    fn offset(&self, addr: usize) -> usize {
        match addr.checked_sub(FAKE_MMIO) {
            Some(o) if o < FAKE_BAR_LEN && o.is_multiple_of(4) => o,
            _ => panic!("fake hal: register access at {addr:#x} outside the BAR"),
        }
    }
}

impl Hal for FakeHal {
    fn map_mmio(&self, phys: u64, len: usize) -> Option<usize> {
        let present = self.0.xhci.borrow().is_some();
        (present && phys == FAKE_BAR && len <= FAKE_BAR_LEN).then_some(FAKE_MMIO)
    }

    unsafe fn read32(&self, addr: usize) -> u32 {
        let offset = self.offset(addr);
        self.fake().read(offset, &self.0.dma.borrow())
    }

    unsafe fn write32(&self, addr: usize, value: u32) {
        let offset = self.offset(addr);
        self.fake().write(offset, value, &self.0.dma.borrow());
    }

    fn alloc_dma(&self, size: usize, align: usize) -> Option<DmaBuf> {
        assert!(
            size > 0 && align.is_power_of_two(),
            "fake hal: bad DMA request"
        );
        let mut dma = self.0.dma.borrow_mut();
        if let Some(n) = dma.failing.as_mut() {
            if *n == 0 {
                dma.failing = None;
                return None;
            }
            *n -= 1;
        }
        if let Some(left) = dma.allocs_left.as_mut() {
            if *left == 0 {
                return None;
            }
            *left -= 1;
        }
        let layout = Layout::from_size_align(size, align.max(4096)).expect("DMA layout");
        // SAFETY: the layout has a non-zero size.
        let ptr = NonNull::new(unsafe { alloc_zeroed(layout) }).expect("host memory");
        let phys = ptr.as_ptr() as u64;
        dma.buffers.insert(phys, (ptr, layout));
        // SAFETY: `size` zeroed bytes, freed only through `free_dma`.
        Some(unsafe { DmaBuf::new(ptr, phys, size) })
    }

    fn free_dma(&self, buf: DmaBuf) {
        let mut dma = self.0.dma.borrow_mut();
        let Some((ptr, layout)) = dma.buffers.remove(&buf.phys()) else {
            panic!(
                "fake hal: freeing DMA memory at {:#x} that is not allocated",
                buf.phys()
            );
        };
        assert_eq!(
            buf.size(),
            layout.size(),
            "fake hal: freed with another size"
        );
        // SAFETY: allocated with this layout; removed from the map, so
        // nothing reaches it any more.
        unsafe { dealloc(ptr.as_ptr(), layout) };
    }

    fn now(&self) -> Duration {
        // Time moves on every look, so a wait that forgets to sleep ends.
        self.advance(Duration::from_micros(1));
        self.clock()
    }

    fn sleep(&self, d: Duration) {
        self.advance(d);
    }

    fn log(&self, args: fmt::Arguments) {
        self.0.log.borrow_mut().push(args.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_moves_only_when_asked() {
        let hal = FakeHal::new();
        assert_eq!(hal.clock(), Duration::ZERO);
        assert_eq!(hal.now(), Duration::from_micros(1));
        hal.sleep(Duration::from_secs(1));
        assert_eq!(hal.clock(), Duration::from_micros(1_000_001));
    }

    #[test]
    fn dma_is_zeroed_aligned_and_identity_mapped() {
        let hal = FakeHal::new();
        let a = hal.alloc_dma(64, 64).unwrap();
        let b = hal.alloc_dma(8192, 8192).unwrap();
        assert_eq!(a.phys() % 4096, 0);
        assert_eq!(b.phys() % 8192, 0);
        assert_eq!(a.phys(), a.virt().as_ptr() as u64);
        assert_eq!(a.read64(56), 0);
        a.write32(8, 0xDEAD_BEEF);
        assert_eq!(hal.0.dma.borrow().read32(a.phys() + 8), 0xDEAD_BEEF);
        assert_eq!(hal.outstanding_dma(), 2);
        hal.free_dma(a);
        hal.free_dma(b);
        assert_eq!(hal.outstanding_dma(), 0);
    }

    #[test]
    #[should_panic(expected = "access to unallocated DMA memory")]
    fn the_controller_cannot_reach_past_a_buffer() {
        let hal = FakeHal::new();
        let a = hal.alloc_dma(64, 64).unwrap();
        hal.0.dma.borrow().read32(a.phys() + 64);
    }

    #[test]
    #[should_panic(expected = "not allocated")]
    fn a_double_free_panics() {
        let hal = FakeHal::new();
        let a = hal.alloc_dma(64, 64).unwrap();
        // SAFETY: a second handle to the same memory, to free it twice.
        let twin = unsafe { DmaBuf::new(a.virt(), a.phys(), a.size()) };
        hal.free_dma(a);
        hal.free_dma(twin);
    }

    #[test]
    fn allocations_can_be_made_to_fail() {
        let hal = FakeHal::new();
        hal.fail_alloc_after(1);
        let a = hal.alloc_dma(64, 64).unwrap();
        assert!(hal.alloc_dma(64, 64).is_none());
        assert!(hal.alloc_dma(64, 64).is_none());
        hal.free_dma(a);
        let hal = FakeHal::new();
        hal.fail_one_alloc(1);
        let a = hal.alloc_dma(64, 64).unwrap();
        assert!(hal.alloc_dma(64, 64).is_none());
        let b = hal.alloc_dma(64, 64).unwrap();
        hal.free_dma(a);
        hal.free_dma(b);
    }

    #[test]
    fn only_the_fake_bar_can_be_mapped() {
        assert_eq!(FakeHal::new().map_mmio(FAKE_BAR, 4096), None);
        let hal = FakeHal::with_controller(FakeConfig::basic());
        assert_eq!(hal.map_mmio(FAKE_BAR + 0x1000, 4096), None);
        assert_eq!(hal.map_mmio(FAKE_BAR, FAKE_BAR_LEN + 1), None);
        let base = hal.map_mmio(FAKE_BAR, FAKE_BAR_LEN).unwrap();
        // SAFETY: the fake checks every register address.
        assert_eq!(unsafe { hal.read32(base) } >> 16, 0x0100);
    }

    #[test]
    #[should_panic(expected = "outside the BAR")]
    fn registers_past_the_bar_panic() {
        let hal = FakeHal::with_controller(FakeConfig::basic());
        let base = hal.map_mmio(FAKE_BAR, FAKE_BAR_LEN).unwrap();
        // SAFETY: the fake checks every register address.
        unsafe { hal.read32(base + FAKE_BAR_LEN) };
    }

    #[test]
    fn log_lines_are_captured() {
        let hal = FakeHal::new();
        hal.log(format_args!("one {}", 1));
        hal.log(format_args!("two"));
        assert_eq!(hal.log_text(), "one 1\ntwo");
    }
}
