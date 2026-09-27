//! Device and input contexts (xHCI 6.2). A context is 32 or 64 bytes
//! (HCCPARAMS1.CSZ); the stride is part of every view, never assumed. An
//! input context holds the input control context, the slot context and 31
//! endpoint contexts at their DCI; a device (output) context the slot
//! context and the endpoints, one entry earlier.

use crate::DmaBuf;
use crate::Speed;
use crate::descriptor::EndpointKind;

/// Entries in a device context: the slot and 31 endpoints.
const DEVICE_ENTRIES: usize = 32;

// Endpoint states (xHCI 6.2.3, dword 0).
pub const EP_DISABLED: u8 = 0;
pub const EP_RUNNING: u8 = 1;
pub const EP_HALTED: u8 = 2;
pub const EP_STOPPED: u8 = 3;
pub const EP_ERROR: u8 = 4;

// Endpoint types (xHCI table 6-9).
pub const ISOCH_OUT: u8 = 1;
pub const BULK_OUT: u8 = 2;
pub const INTERRUPT_OUT: u8 = 3;
pub const CONTROL: u8 = 4;
pub const ISOCH_IN: u8 = 5;
pub const BULK_IN: u8 = 6;
pub const INTERRUPT_IN: u8 = 7;

/// The Device Context Index of an endpoint address: EP0 is 1, OUT n is
/// 2n, IN n is 2n + 1 (xHCI 4.5.1).
pub fn dci(endpoint_address: u8) -> usize {
    let number = (endpoint_address & 0x0F) as usize;
    if number == 0 {
        1
    } else {
        2 * number + (endpoint_address >> 7) as usize
    }
}

/// The context's endpoint type for an endpoint of `kind` and direction.
pub fn endpoint_type(kind: EndpointKind, is_in: bool) -> u8 {
    match (kind, is_in) {
        (EndpointKind::Control, _) => CONTROL,
        (EndpointKind::Isochronous, false) => ISOCH_OUT,
        (EndpointKind::Isochronous, true) => ISOCH_IN,
        (EndpointKind::Bulk, false) => BULK_OUT,
        (EndpointKind::Bulk, true) => BULK_IN,
        (EndpointKind::Interrupt, false) => INTERRUPT_OUT,
        (EndpointKind::Interrupt, true) => INTERRUPT_IN,
    }
}

/// The endpoint context's Interval: the service interval is 2^n × 125 µs
/// (xHCI 6.2.3.6). `b_interval` counts frames for low- and full-speed
/// interrupt endpoints and is an exponent for everything else periodic.
pub fn interval_exponent(speed: Speed, kind: EndpointKind, b_interval: u8) -> u8 {
    let exponent_minus_one = || b_interval.saturating_sub(1).min(15);
    match (kind, speed) {
        (EndpointKind::Control | EndpointKind::Bulk, _) => 0,
        (EndpointKind::Interrupt, Speed::Low | Speed::Full) => {
            // Frames to microframes, rounded down to a power of two.
            (b_interval.max(1) as u32 * 8).ilog2().clamp(3, 10) as u8
        }
        (EndpointKind::Isochronous, Speed::Full) => b_interval.clamp(1, 13) + 2,
        _ => exponent_minus_one(),
    }
}

/// The Protocol Speed ID of the default speed table (xHCI 7.2.2.1.1), as
/// PORTSC and the slot context carry it.
pub fn speed_id(speed: Speed) -> u8 {
    match speed {
        Speed::Full => 1,
        Speed::Low => 2,
        Speed::High => 3,
        Speed::Super => 4,
        Speed::SuperPlus => 5,
    }
}

pub fn speed_from_id(id: u8) -> Option<Speed> {
    match id {
        1 => Some(Speed::Full),
        2 => Some(Speed::Low),
        3 => Some(Speed::High),
        4 => Some(Speed::Super),
        5 => Some(Speed::SuperPlus),
        _ => None,
    }
}

/// Bytes for an input context: the control context and 32 more.
pub fn input_size(stride: usize) -> usize {
    (DEVICE_ENTRIES + 1) * stride
}

/// Bytes for a device (output) context.
pub fn device_size(stride: usize) -> usize {
    DEVICE_ENTRIES * stride
}

/// A slot context's fields (xHCI 6.2.2). Hubs and TTs are not used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlotContext {
    pub route_string: u32,
    /// Protocol Speed ID.
    pub speed: u8,
    /// The highest DCI in use.
    pub context_entries: u8,
    pub root_port: u8,
    pub interrupter: u16,
    /// Output only: the USB address the controller assigned.
    pub address: u8,
    /// Output only.
    pub state: u8,
}

impl SlotContext {
    fn write(&self, buf: &DmaBuf, at: usize) {
        buf.write32(
            at,
            self.route_string & 0xF_FFFF
                | (self.speed as u32 & 0xF) << 20
                | (self.context_entries as u32 & 0x1F) << 27,
        );
        buf.write32(at + 4, (self.root_port as u32) << 16);
        buf.write32(at + 8, (self.interrupter as u32 & 0x3FF) << 22);
        buf.write32(
            at + 12,
            self.address as u32 | (self.state as u32 & 0x1F) << 27,
        );
    }

    fn read(buf: &DmaBuf, at: usize) -> SlotContext {
        let d = [
            buf.read32(at),
            buf.read32(at + 4),
            buf.read32(at + 8),
            buf.read32(at + 12),
        ];
        SlotContext {
            route_string: d[0] & 0xF_FFFF,
            speed: (d[0] >> 20) as u8 & 0xF,
            context_entries: (d[0] >> 27) as u8,
            root_port: (d[1] >> 16) as u8,
            interrupter: (d[2] >> 22) as u16,
            address: d[3] as u8,
            state: (d[3] >> 27) as u8,
        }
    }
}

/// An endpoint context's fields (xHCI 6.2.3). Streams are not used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EndpointContext {
    /// Output only.
    pub state: u8,
    pub mult: u8,
    pub max_primary_streams: u8,
    pub interval: u8,
    /// Error count: retries before the endpoint halts (3 = the most).
    pub cerr: u8,
    pub ep_type: u8,
    pub max_burst: u8,
    pub max_packet: u16,
    /// The TR Dequeue Pointer with the Dequeue Cycle State in bit 0.
    pub dequeue: u64,
    pub average_trb_length: u16,
    /// Max ESIT Payload: bytes per service interval (24 bits).
    pub max_esit_payload: u32,
}

impl EndpointContext {
    fn write(&self, buf: &DmaBuf, at: usize) {
        buf.write32(
            at,
            self.state as u32 & 7
                | (self.mult as u32 & 3) << 8
                | (self.max_primary_streams as u32 & 0x1F) << 10
                | (self.interval as u32) << 16
                | (self.max_esit_payload >> 16 & 0xFF) << 24,
        );
        buf.write32(
            at + 4,
            (self.cerr as u32 & 3) << 1
                | (self.ep_type as u32 & 7) << 3
                | (self.max_burst as u32) << 8
                | (self.max_packet as u32) << 16,
        );
        buf.write64(at + 8, self.dequeue & !0xE);
        buf.write32(
            at + 16,
            self.average_trb_length as u32 | (self.max_esit_payload & 0xFFFF) << 16,
        );
    }

    fn read(buf: &DmaBuf, at: usize) -> EndpointContext {
        let d0 = buf.read32(at);
        let d1 = buf.read32(at + 4);
        let d4 = buf.read32(at + 16);
        EndpointContext {
            state: d0 as u8 & 7,
            mult: (d0 >> 8) as u8 & 3,
            max_primary_streams: (d0 >> 10) as u8 & 0x1F,
            interval: (d0 >> 16) as u8,
            cerr: (d1 >> 1) as u8 & 3,
            ep_type: (d1 >> 3) as u8 & 7,
            max_burst: (d1 >> 8) as u8,
            max_packet: (d1 >> 16) as u16,
            dequeue: buf.read64(at + 8) & !0xE,
            average_trb_length: d4 as u16,
            max_esit_payload: (d0 >> 24) << 16 | d4 >> 16,
        }
    }
}

/// An input context in `buf`, for Address Device, Evaluate Context and
/// Configure Endpoint (xHCI 6.2.5).
pub struct Input<'a> {
    buf: &'a DmaBuf,
    stride: usize,
}

impl<'a> Input<'a> {
    pub fn new(buf: &'a DmaBuf, stride: usize) -> Input<'a> {
        debug_assert!(buf.size() >= input_size(stride));
        Input { buf, stride }
    }

    /// Zeroes the whole context, so no flag or field of an earlier command
    /// is left over.
    pub fn clear(&self) {
        self.buf.zero(0, input_size(self.stride));
    }

    /// The Input Control Context's drop and add flags: bit n for DCI n,
    /// bit 0 (add only) for the slot context.
    pub fn set_flags(&self, drop: u32, add: u32) {
        self.buf.write32(0, drop);
        self.buf.write32(4, add);
    }

    pub fn set_slot(&self, slot: &SlotContext) {
        slot.write(self.buf, self.stride);
    }

    pub fn set_endpoint(&self, dci: usize, ep: &EndpointContext) {
        ep.write(self.buf, self.endpoint_offset(dci));
    }

    fn endpoint_offset(&self, dci: usize) -> usize {
        assert!((1..DEVICE_ENTRIES).contains(&dci), "DCI {dci}");
        (dci + 1) * self.stride
    }
}

/// A device (output) context in `buf`, which the controller writes.
pub struct Output<'a> {
    buf: &'a DmaBuf,
    stride: usize,
}

impl<'a> Output<'a> {
    pub fn new(buf: &'a DmaBuf, stride: usize) -> Output<'a> {
        debug_assert!(buf.size() >= device_size(stride));
        Output { buf, stride }
    }

    pub fn slot(&self) -> SlotContext {
        SlotContext::read(self.buf, 0)
    }

    pub fn endpoint(&self, dci: usize) -> EndpointContext {
        assert!((1..DEVICE_ENTRIES).contains(&dci), "DCI {dci}");
        EndpointContext::read(self.buf, dci * self.stride)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Hal;
    use crate::testing::FakeHal;

    fn sample_slot() -> SlotContext {
        SlotContext {
            route_string: 0x1_2345,
            speed: 4,
            context_entries: 31,
            root_port: 13,
            interrupter: 0x2AB,
            address: 7,
            state: 3,
        }
    }

    fn sample_endpoint() -> EndpointContext {
        EndpointContext {
            state: EP_HALTED,
            mult: 2,
            max_primary_streams: 0,
            interval: 6,
            cerr: 3,
            ep_type: INTERRUPT_IN,
            max_burst: 15,
            max_packet: 1024,
            dequeue: 0x12_3456_7890 | 1,
            average_trb_length: 3072,
            max_esit_payload: 0x12_3456,
        }
    }

    #[test]
    fn slot_context_fields_round_trip_at_the_slot_offset() {
        for stride in [32, 64] {
            let hal = FakeHal::new();
            let buf = hal.alloc_dma(4096, 64).unwrap();
            let input = Input::new(&buf, stride);
            input.set_slot(&sample_slot());
            assert_eq!(SlotContext::read(&buf, stride), sample_slot());
            // xHCI 6.2.2: the slot context is entry 1 of an input context.
            assert_eq!(buf.read32(stride), 0x1_2345 | 4 << 20 | 31 << 27);
            assert_eq!(buf.read32(stride + 4), 13 << 16);
            assert_eq!(buf.read32(stride + 8), 0x2AB << 22);
            assert_eq!(buf.read32(stride + 12), 7 | 3 << 27);
            assert_eq!(buf.read32(0), 0, "the control context is untouched");
            hal.free_dma(buf);
        }
    }

    #[test]
    fn endpoint_context_fields_round_trip_at_dci_plus_one() {
        for stride in [32, 64] {
            let hal = FakeHal::new();
            let buf = hal.alloc_dma(4096, 64).unwrap();
            let input = Input::new(&buf, stride);
            for dci in [1, 3, 31] {
                input.set_endpoint(dci, &sample_endpoint());
                let at = (dci + 1) * stride;
                assert_eq!(EndpointContext::read(&buf, at), sample_endpoint());
                assert_eq!(buf.read32(at), 2 | 2 << 8 | 6 << 16 | 0x12 << 24);
                assert_eq!(buf.read32(at + 4), 3 << 1 | 7 << 3 | 15 << 8 | 1024 << 16);
                assert_eq!(buf.read64(at + 8), 0x12_3456_7891);
                assert_eq!(buf.read32(at + 16), 3072 | 0x3456 << 16);
            }
            hal.free_dma(buf);
        }
    }

    #[test]
    fn output_contexts_start_with_the_slot() {
        for stride in [32, 64] {
            let hal = FakeHal::new();
            let buf = hal.alloc_dma(4096, 64).unwrap();
            let out = Output::new(&buf, stride);
            // As the controller writes them: the slot at 0, DCI n at n.
            sample_slot().write(&buf, 0);
            sample_endpoint().write(&buf, 3 * stride);
            assert_eq!(
                (out.slot(), out.endpoint(3)),
                (sample_slot(), sample_endpoint())
            );
            hal.free_dma(buf);
        }
    }

    #[test]
    fn the_input_control_context_holds_drop_and_add_flags_and_clears() {
        let hal = FakeHal::new();
        let buf = hal.alloc_dma(4096, 64).unwrap();
        let input = Input::new(&buf, 64);
        input.set_flags(1 << 4, 0b1011);
        assert_eq!((buf.read32(0), buf.read32(4)), (1 << 4, 0b1011));

        input.set_endpoint(31, &sample_endpoint());
        input.clear();
        assert_eq!((buf.read32(0), buf.read32(4)), (0, 0));
        assert_eq!(
            EndpointContext::read(&buf, 32 * 64),
            EndpointContext::default()
        );
        assert_eq!(input_size(64), 2112);
        assert_eq!(device_size(32), 1024);
        hal.free_dma(buf);
    }

    #[test]
    fn the_dequeue_pointer_keeps_its_cycle_state_but_not_stream_bits() {
        let hal = FakeHal::new();
        let buf = hal.alloc_dma(4096, 64).unwrap();
        let input = Input::new(&buf, 32);
        let ep = EndpointContext {
            dequeue: 0x5000 | 0xF,
            ..EndpointContext::default()
        };
        input.set_endpoint(1, &ep);
        assert_eq!(EndpointContext::read(&buf, 2 * 32).dequeue, 0x5001);
        hal.free_dma(buf);
    }

    #[test]
    fn device_context_indices() {
        assert_eq!(dci(0x00), 1);
        assert_eq!(dci(0x80), 1);
        assert_eq!(dci(0x01), 2);
        assert_eq!(dci(0x81), 3);
        assert_eq!(dci(0x02), 4);
        assert_eq!(dci(0x82), 5);
        assert_eq!(dci(0x8F), 31);
    }

    #[test]
    fn endpoint_types_follow_kind_and_direction() {
        assert_eq!(endpoint_type(EndpointKind::Interrupt, true), 7);
        assert_eq!(endpoint_type(EndpointKind::Interrupt, false), 3);
        assert_eq!(endpoint_type(EndpointKind::Bulk, true), 6);
        assert_eq!(endpoint_type(EndpointKind::Bulk, false), 2);
        assert_eq!(endpoint_type(EndpointKind::Control, true), 4);
    }

    #[test]
    fn intervals_follow_xhci_6_2_3_6() {
        use EndpointKind::*;
        use Speed::*;
        assert_eq!(interval_exponent(Low, Interrupt, 10), 6, "the K120");
        assert_eq!(interval_exponent(Full, Interrupt, 1), 3);
        assert_eq!(interval_exponent(Full, Interrupt, 255), 10);
        assert_eq!(interval_exponent(Full, Interrupt, 0), 3);
        assert_eq!(interval_exponent(High, Interrupt, 1), 0);
        assert_eq!(interval_exponent(High, Interrupt, 4), 3);
        assert_eq!(interval_exponent(High, Interrupt, 0), 0);
        assert_eq!(interval_exponent(Super, Interrupt, 16), 15);
        assert_eq!(interval_exponent(Super, Interrupt, 200), 15);
        assert_eq!(interval_exponent(Full, Isochronous, 1), 3);
        assert_eq!(interval_exponent(High, Isochronous, 4), 3);
        assert_eq!(interval_exponent(High, Bulk, 255), 0);
        assert_eq!(interval_exponent(Super, Bulk, 0), 0);
        assert_eq!(interval_exponent(Full, Control, 10), 0);
    }

    #[test]
    fn speed_ids_follow_the_default_table() {
        for s in [
            Speed::Low,
            Speed::Full,
            Speed::High,
            Speed::Super,
            Speed::SuperPlus,
        ] {
            assert_eq!(speed_from_id(speed_id(s)), Some(s));
        }
        assert_eq!(speed_id(Speed::Low), 2);
        assert_eq!(speed_from_id(0), None);
        assert_eq!(speed_from_id(6), None);
    }
}
