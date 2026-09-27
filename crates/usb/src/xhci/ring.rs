//! Rings (xHCI 4.9): the producer rings this driver fills (the command
//! ring and one transfer ring per endpoint) and the event ring the
//! controller fills. Each is one 4 KiB segment of 256 TRBs; ownership of a
//! TRB passes with its cycle bit.

use super::trb::{CHAIN, Trb};
use crate::{DmaBuf, Hal, UsbError};
use core::sync::atomic::{Ordering, fence};

/// TRBs per segment: one 4 KiB page.
pub const TRBS: usize = 256;
const TRB_SIZE: usize = 16;
const SEGMENT_SIZE: usize = TRBS * TRB_SIZE;
/// Producer rings end in a Link TRB back to their start.
const LINK_INDEX: usize = TRBS - 1;

/// A ring this driver produces and the controller consumes: 255 usable
/// TRBs and a Link TRB with Toggle Cycle (xHCI 4.9.2).
#[derive(Debug)]
pub struct ProducerRing {
    buf: DmaBuf,
    /// Where the next TRB goes.
    index: usize,
    /// The Producer Cycle State: the cycle bit valid TRBs carry this lap.
    cycle: bool,
}

impl ProducerRing {
    pub fn new<H: Hal>(hal: &H) -> Result<ProducerRing, UsbError> {
        let buf = hal.alloc_dma(SEGMENT_SIZE, 64).ok_or(UsbError::NoMemory)?;
        let ring = ProducerRing {
            buf,
            index: 0,
            cycle: true,
        };
        // Cycle 0: the controller stops before the link until it is valid.
        ring.write(LINK_INDEX, Trb::link(ring.buf.phys()));
        Ok(ring)
    }

    /// The physical address of the segment (for CRCR and contexts).
    pub fn phys(&self) -> u64 {
        self.buf.phys()
    }

    /// Adds `trb` with the current cycle bit and returns its physical
    /// address, which the controller's events name.
    pub fn push(&mut self, trb: Trb) -> u64 {
        let addr = self.buf.phys_at(self.index * TRB_SIZE);
        self.write(self.index, trb.with_cycle(self.cycle));
        self.index += 1;
        if self.index == LINK_INDEX {
            // xHCI 4.11.5.1: a TD that spans the link carries its chain
            // bit through it.
            let mut link = Trb::link(self.buf.phys());
            if trb.chain() {
                link.0[3] |= CHAIN;
            }
            self.write(LINK_INDEX, link.with_cycle(self.cycle));
            self.cycle = !self.cycle;
            self.index = 0;
        }
        addr
    }

    /// Overwrites the TRB at `addr` (returned by `push`) with `trb`, keeping
    /// the cycle bit it has, so the controller treats it as it would have
    /// treated the old one.
    pub fn replace(&self, addr: u64, trb: Trb) {
        let offset = addr
            .checked_sub(self.buf.phys())
            .map(|o| o as usize)
            .filter(|&o| o < LINK_INDEX * TRB_SIZE && o % TRB_SIZE == 0)
            .expect("replace: not a TRB of this ring");
        let old = self.read(offset / TRB_SIZE);
        self.write(offset / TRB_SIZE, trb.with_cycle(old.cycle()));
    }

    /// The next TRB's address with the cycle state in bit 0: what Set TR
    /// Dequeue Pointer and CRCR take.
    pub fn enqueue_pointer(&self) -> u64 {
        self.buf.phys_at(self.index * TRB_SIZE) | self.cycle as u64
    }

    pub fn free<H: Hal>(self, hal: &H) {
        hal.free_dma(self.buf);
    }

    fn read(&self, index: usize) -> Trb {
        let offset = index * TRB_SIZE;
        Trb(core::array::from_fn(|i| self.buf.read32(offset + 4 * i)))
    }

    fn write(&self, index: usize, trb: Trb) {
        let offset = index * TRB_SIZE;
        for (i, &dword) in trb.0[..3].iter().enumerate() {
            self.buf.write32(offset + 4 * i, dword);
        }
        // The dword with the cycle bit hands the TRB over, so it goes last.
        fence(Ordering::Release);
        self.buf.write32(offset + 12, trb.0[3]);
    }
}

/// The primary interrupter's event ring: one segment and a one-entry Event
/// Ring Segment Table (xHCI 4.9.4, 6.5).
#[derive(Debug)]
pub struct EventRing {
    segment: DmaBuf,
    erst: DmaBuf,
    /// The next TRB to look at.
    index: usize,
    /// The Consumer Cycle State.
    cycle: bool,
}

impl EventRing {
    pub fn new<H: Hal>(hal: &H) -> Result<EventRing, UsbError> {
        let segment = hal.alloc_dma(SEGMENT_SIZE, 64).ok_or(UsbError::NoMemory)?;
        let Some(erst) = hal.alloc_dma(16, 64) else {
            hal.free_dma(segment);
            return Err(UsbError::NoMemory);
        };
        erst.write64(0, segment.phys());
        erst.write32(8, TRBS as u32);
        Ok(EventRing {
            segment,
            erst,
            index: 0,
            cycle: true,
        })
    }

    /// The ERST's physical address (for ERSTBA).
    pub fn erst_phys(&self) -> u64 {
        self.erst.phys()
    }

    /// The segment's physical address.
    pub fn phys(&self) -> u64 {
        self.segment.phys()
    }

    /// The next event, if the controller has written one.
    pub fn next(&mut self) -> Option<Trb> {
        let offset = self.index * TRB_SIZE;
        let control = self.segment.read32(offset + 12);
        if (control & 1 != 0) != self.cycle {
            return None;
        }
        // The rest of the TRB is read after its cycle bit said it is there.
        fence(Ordering::Acquire);
        let trb = Trb([
            self.segment.read32(offset),
            self.segment.read32(offset + 4),
            self.segment.read32(offset + 8),
            control,
        ]);
        self.index += 1;
        if self.index == TRBS {
            self.index = 0;
            self.cycle = !self.cycle;
        }
        Some(trb)
    }

    /// Where the next event goes: what ERDP must be told.
    pub fn dequeue_pointer(&self) -> u64 {
        self.segment.phys_at(self.index * TRB_SIZE)
    }

    pub fn free<H: Hal>(self, hal: &H) {
        hal.free_dma(self.segment);
        hal.free_dma(self.erst);
    }
}

#[cfg(test)]
mod tests {
    use super::super::trb::{LINK, TOGGLE_CYCLE};
    use super::*;
    use crate::testing::FakeHal;

    fn trb_at(ring: &ProducerRing, index: usize) -> Trb {
        ring.read(index)
    }

    #[test]
    fn a_new_ring_ends_in_a_link_that_is_not_yet_valid() {
        let hal = FakeHal::new();
        let ring = ProducerRing::new(&hal).unwrap();
        let link = trb_at(&ring, 255);
        assert_eq!(link.trb_type(), LINK);
        assert_eq!(link.pointer(), ring.phys());
        assert_eq!(link.0[3] & TOGGLE_CYCLE, TOGGLE_CYCLE);
        assert!(!link.cycle());
        assert_eq!(ring.enqueue_pointer(), ring.phys() | 1);
        ring.free(&hal);
    }

    #[test]
    fn a_lap_fills_255_trbs_then_validates_the_link_and_toggles() {
        let hal = FakeHal::new();
        let mut ring = ProducerRing::new(&hal).unwrap();
        for i in 0..255 {
            let addr = ring.push(Trb::no_op_command());
            assert_eq!(addr, ring.phys() + 16 * i as u64);
        }
        for i in 0..255 {
            assert!(trb_at(&ring, i).cycle(), "TRB {i} of the first lap");
        }
        assert!(trb_at(&ring, 255).cycle(), "the link is valid for lap 1");
        // Lap 2 starts at index 0 with cycle 0.
        assert_eq!(ring.enqueue_pointer(), ring.phys());
        assert_eq!(ring.push(Trb::no_op_command()), ring.phys());
        let first = trb_at(&ring, 0);
        assert!(!first.cycle());
        assert_eq!(first.trb_type(), Trb::no_op_command().trb_type());
        assert!(trb_at(&ring, 1).cycle(), "still lap 1's TRB");
        ring.free(&hal);
    }

    #[test]
    fn after_two_wraps_the_cycle_is_one_again() {
        let hal = FakeHal::new();
        let mut ring = ProducerRing::new(&hal).unwrap();
        for _ in 0..255 {
            ring.push(Trb::no_op_command());
        }
        assert!(trb_at(&ring, 255).cycle());
        for _ in 0..255 {
            ring.push(Trb::no_op_command());
        }
        assert!(!trb_at(&ring, 255).cycle(), "the link for lap 2");
        assert!(!trb_at(&ring, 254).cycle());
        assert_eq!(ring.enqueue_pointer() & 1, 1);
        ring.push(Trb::no_op_command());
        assert!(trb_at(&ring, 0).cycle());
        ring.free(&hal);
    }

    #[test]
    fn the_link_carries_the_chain_bit_of_the_trb_before_it() {
        let hal = FakeHal::new();
        let mut ring = ProducerRing::new(&hal).unwrap();
        for _ in 0..254 {
            ring.push(Trb::normal(0x1000, 8));
        }
        let mut chained = Trb::normal(0x1000, 8);
        chained.0[3] |= CHAIN;
        ring.push(chained);
        assert!(trb_at(&ring, 255).chain());
        for _ in 0..255 {
            ring.push(Trb::normal(0x1000, 8));
        }
        assert!(!trb_at(&ring, 255).chain(), "an unchained TD clears it");
        ring.free(&hal);
    }

    #[test]
    fn the_enqueue_pointer_carries_the_cycle_state() {
        let hal = FakeHal::new();
        let mut ring = ProducerRing::new(&hal).unwrap();
        ring.push(Trb::no_op_command());
        assert_eq!(ring.enqueue_pointer(), (ring.phys() + 16) | 1);
        for _ in 1..255 {
            ring.push(Trb::no_op_command());
        }
        assert_eq!(ring.enqueue_pointer(), ring.phys());
        ring.free(&hal);
    }

    #[test]
    fn a_replaced_trb_keeps_its_cycle_bit() {
        let hal = FakeHal::new();
        let mut ring = ProducerRing::new(&hal).unwrap();
        for _ in 0..255 {
            ring.push(Trb::no_op_command());
        }
        let old = ring.push(Trb::enable_slot());
        ring.replace(old, Trb::no_op_command());
        assert_eq!(trb_at(&ring, 0), Trb::no_op_command().with_cycle(false));
        ring.free(&hal);
    }

    fn produce(ring: &EventRing, index: usize, cycle: bool, tag: u32) {
        let o = index * TRB_SIZE;
        ring.segment.write32(o, tag);
        ring.segment.write32(o + 12, 33 << 10 | cycle as u32);
    }

    #[test]
    fn the_event_ring_describes_its_segment_in_the_erst() {
        let hal = FakeHal::new();
        let ring = EventRing::new(&hal).unwrap();
        assert_eq!(ring.erst.read64(0), ring.phys());
        assert_eq!(ring.erst.read32(8), 256);
        assert_eq!(ring.dequeue_pointer(), ring.phys());
        ring.free(&hal);
    }

    #[test]
    fn events_are_consumed_only_while_their_cycle_bit_matches() {
        let hal = FakeHal::new();
        let mut ring = EventRing::new(&hal).unwrap();
        assert_eq!(ring.next(), None, "a zeroed ring is empty");
        produce(&ring, 0, true, 7);
        produce(&ring, 1, true, 8);
        assert_eq!(ring.next().map(|t| t.0[0]), Some(7));
        assert_eq!(ring.dequeue_pointer(), ring.phys() + 16);
        assert_eq!(ring.next().map(|t| t.0[0]), Some(8));
        assert_eq!(ring.next(), None);
        assert_eq!(ring.dequeue_pointer(), ring.phys() + 32);
        ring.free(&hal);
    }

    #[test]
    fn the_event_ring_wraps_at_256_and_toggles_its_cycle() {
        let hal = FakeHal::new();
        let mut ring = EventRing::new(&hal).unwrap();
        for i in 0..256 {
            produce(&ring, i, true, i as u32);
        }
        for i in 0..256 {
            assert_eq!(ring.next().map(|t| t.0[0]), Some(i));
        }
        assert_eq!(ring.dequeue_pointer(), ring.phys());
        assert_eq!(ring.next(), None, "lap 1's TRBs are stale in lap 2");
        produce(&ring, 0, false, 1000);
        assert_eq!(ring.next().map(|t| t.0[0]), Some(1000));
        ring.free(&hal);
    }

    #[test]
    fn freeing_gives_all_dma_back() {
        let hal = FakeHal::new();
        let ring = ProducerRing::new(&hal).unwrap();
        let events = EventRing::new(&hal).unwrap();
        assert_eq!(hal.outstanding_dma(), 3);
        ring.free(&hal);
        events.free(&hal);
        assert_eq!(hal.outstanding_dma(), 0);
    }

    #[test]
    fn running_out_of_memory_is_an_error_that_leaks_nothing() {
        let hal = FakeHal::new();
        hal.fail_alloc_after(1);
        assert_eq!(EventRing::new(&hal).err(), Some(UsbError::NoMemory));
        assert_eq!(hal.outstanding_dma(), 0);
        assert_eq!(ProducerRing::new(&hal).err(), Some(UsbError::NoMemory));
    }
}
