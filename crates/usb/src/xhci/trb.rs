//! Transfer Request Blocks (xHCI 6.4): the 16-byte records of every ring.
//! Constructors build the TRBs this driver sends; accessors read the
//! events the controller sends back. The cycle bit is left to the ring.

use crate::Setup;

/// A TRB as four little-endian dwords.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trb(pub [u32; 4]);

// TRB types (xHCI table 6-91).
pub const NORMAL: u8 = 1;
pub const SETUP_STAGE: u8 = 2;
pub const DATA_STAGE: u8 = 3;
pub const STATUS_STAGE: u8 = 4;
pub const LINK: u8 = 6;
pub const ENABLE_SLOT: u8 = 9;
pub const DISABLE_SLOT: u8 = 10;
pub const ADDRESS_DEVICE: u8 = 11;
pub const CONFIGURE_ENDPOINT: u8 = 12;
pub const EVALUATE_CONTEXT: u8 = 13;
pub const RESET_ENDPOINT: u8 = 14;
pub const STOP_ENDPOINT: u8 = 15;
pub const SET_TR_DEQUEUE: u8 = 16;
pub const NO_OP_COMMAND: u8 = 23;
pub const TRANSFER_EVENT: u8 = 32;
pub const COMMAND_COMPLETION: u8 = 33;
pub const PORT_STATUS_CHANGE: u8 = 34;
pub const HOST_CONTROLLER_EVENT: u8 = 37;

// Completion codes (xHCI table 6-90).
pub const SUCCESS: u8 = 1;
pub const DATA_BUFFER_ERROR: u8 = 2;
pub const BABBLE: u8 = 3;
pub const USB_TRANSACTION_ERROR: u8 = 4;
pub const TRB_ERROR: u8 = 5;
pub const STALL: u8 = 6;
pub const RESOURCE_ERROR: u8 = 7;
pub const BANDWIDTH_ERROR: u8 = 8;
pub const NO_SLOTS: u8 = 9;
pub const SLOT_NOT_ENABLED: u8 = 11;
pub const ENDPOINT_NOT_ENABLED: u8 = 12;
pub const SHORT_PACKET: u8 = 13;
pub const PARAMETER_ERROR: u8 = 17;
pub const CONTEXT_STATE_ERROR: u8 = 19;
pub const EVENT_RING_FULL: u8 = 21;
pub const COMMAND_RING_STOPPED: u8 = 24;
pub const COMMAND_ABORTED: u8 = 25;
pub const STOPPED: u8 = 26;
pub const STOPPED_LENGTH_INVALID: u8 = 27;

// Dword 3 flags. Several bits mean different things per TRB type.
pub const CYCLE: u32 = 1 << 0;
/// Link TRB: toggle the consumer's cycle state.
pub const TOGGLE_CYCLE: u32 = 1 << 1;
/// Interrupt on Short Packet: an event with the residual when a transfer
/// ends short.
pub const ISP: u32 = 1 << 2;
pub const CHAIN: u32 = 1 << 4;
pub const IOC: u32 = 1 << 5;
/// Immediate data: the setup packet is in the TRB itself.
pub const IDT: u32 = 1 << 6;
/// Address Device: Block Set Address Request. Configure Endpoint:
/// Deconfigure.
pub const BSR: u32 = 1 << 9;
pub const DECONFIGURE: u32 = 1 << 9;
/// Data and Status Stage: the direction is IN.
pub const DIR_IN: u32 = 1 << 16;

// Setup Stage Transfer Type (xHCI 6.4.1.2.1).
const TRT_NO_DATA: u32 = 0;
const TRT_OUT: u32 = 2;
const TRT_IN: u32 = 3;

fn control(kind: u8) -> u32 {
    (kind as u32) << 10
}

fn slot_field(slot: u8) -> u32 {
    (slot as u32) << 24
}

fn endpoint_field(dci: usize) -> u32 {
    ((dci as u32) & 0x1F) << 16
}

impl Trb {
    fn with_pointer(pointer: u64, status: u32, control: u32) -> Trb {
        Trb([pointer as u32, (pointer >> 32) as u32, status, control])
    }

    /// A Normal TRB for `len` bytes at `buffer`, with an event when it
    /// completes or ends short (xHCI 6.4.1.1).
    pub fn normal(buffer: u64, len: u32) -> Trb {
        Trb::with_pointer(buffer, len & 0x1_FFFF, control(NORMAL) | IOC | ISP)
    }

    /// A Setup Stage TRB carrying the 8 setup bytes inline (xHCI 6.4.1.2.1).
    pub fn setup_stage(setup: &Setup) -> Trb {
        let b = setup.to_bytes();
        let trt = match (setup.length, setup.is_in()) {
            (0, _) => TRT_NO_DATA,
            (_, true) => TRT_IN,
            (_, false) => TRT_OUT,
        };
        Trb([
            u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            8,
            control(SETUP_STAGE) | IDT | trt << 16,
        ])
    }

    /// A Data Stage TRB for `len` bytes at `buffer` (xHCI 6.4.1.2.2). A
    /// short IN transfer produces an event with the residual.
    pub fn data_stage(buffer: u64, len: u32, dir_in: bool) -> Trb {
        let dir = if dir_in { DIR_IN } else { 0 };
        Trb::with_pointer(buffer, len & 0x1_FFFF, control(DATA_STAGE) | ISP | dir)
    }

    /// A Status Stage TRB with an event on completion (xHCI 6.4.1.2.3).
    pub fn status_stage(dir_in: bool) -> Trb {
        let dir = if dir_in { DIR_IN } else { 0 };
        Trb([0, 0, 0, control(STATUS_STAGE) | IOC | dir])
    }

    /// A Link TRB to `segment` that toggles the consumer's cycle state
    /// (xHCI 6.4.4.1).
    pub fn link(segment: u64) -> Trb {
        Trb::with_pointer(segment, 0, control(LINK) | TOGGLE_CYCLE)
    }

    pub fn no_op_command() -> Trb {
        Trb([0, 0, 0, control(NO_OP_COMMAND)])
    }

    /// Enable Slot for a USB device (slot type 0, xHCI 6.4.3.2).
    pub fn enable_slot() -> Trb {
        Trb([0, 0, 0, control(ENABLE_SLOT)])
    }

    pub fn disable_slot(slot: u8) -> Trb {
        Trb([0, 0, 0, control(DISABLE_SLOT) | slot_field(slot)])
    }

    /// Address Device with the input context at `input` (xHCI 6.4.3.4).
    /// With `bsr` the controller does not send SET_ADDRESS.
    pub fn address_device(input: u64, slot: u8, bsr: bool) -> Trb {
        let bsr = if bsr { BSR } else { 0 };
        Trb::with_pointer(input, 0, control(ADDRESS_DEVICE) | bsr | slot_field(slot))
    }

    /// Configure Endpoint (xHCI 6.4.3.5); `deconfigure` drops every
    /// endpoint but EP0.
    pub fn configure_endpoint(input: u64, slot: u8, deconfigure: bool) -> Trb {
        let dc = if deconfigure { DECONFIGURE } else { 0 };
        Trb::with_pointer(
            input,
            0,
            control(CONFIGURE_ENDPOINT) | dc | slot_field(slot),
        )
    }

    pub fn evaluate_context(input: u64, slot: u8) -> Trb {
        Trb::with_pointer(input, 0, control(EVALUATE_CONTEXT) | slot_field(slot))
    }

    /// Reset Endpoint: a Halted endpoint becomes Stopped (xHCI 4.6.8).
    pub fn reset_endpoint(slot: u8, dci: usize) -> Trb {
        Trb([
            0,
            0,
            0,
            control(RESET_ENDPOINT) | endpoint_field(dci) | slot_field(slot),
        ])
    }

    /// Stop Endpoint: a Running endpoint becomes Stopped (xHCI 4.6.9).
    pub fn stop_endpoint(slot: u8, dci: usize) -> Trb {
        Trb([
            0,
            0,
            0,
            control(STOP_ENDPOINT) | endpoint_field(dci) | slot_field(slot),
        ])
    }

    /// Set TR Dequeue Pointer to `dequeue`, whose bit 0 is the Dequeue
    /// Cycle State (xHCI 6.4.3.9).
    pub fn set_tr_dequeue(slot: u8, dci: usize, dequeue: u64) -> Trb {
        Trb::with_pointer(
            dequeue & !0xE,
            0,
            control(SET_TR_DEQUEUE) | endpoint_field(dci) | slot_field(slot),
        )
    }

    pub fn trb_type(&self) -> u8 {
        (self.0[3] >> 10) as u8 & 0x3F
    }

    pub fn cycle(&self) -> bool {
        self.0[3] & CYCLE != 0
    }

    pub fn chain(&self) -> bool {
        self.0[3] & CHAIN != 0
    }

    /// This TRB with its cycle bit set to `cycle`.
    pub fn with_cycle(self, cycle: bool) -> Trb {
        let mut t = self;
        t.0[3] = t.0[3] & !CYCLE | cycle as u32;
        t
    }

    /// Dwords 0-1: a buffer, a context or, in events, the TRB concerned.
    pub fn pointer(&self) -> u64 {
        self.0[0] as u64 | (self.0[1] as u64) << 32
    }

    pub fn completion_code(&self) -> u8 {
        (self.0[2] >> 24) as u8
    }

    /// Transfer events: the bytes not transferred (the residual).
    pub fn transfer_length(&self) -> u32 {
        self.0[2] & 0xFF_FFFF
    }

    pub fn slot_id(&self) -> u8 {
        (self.0[3] >> 24) as u8
    }

    /// Transfer events: the endpoint's DCI.
    pub fn endpoint_id(&self) -> usize {
        (self.0[3] >> 16) as usize & 0x1F
    }

    /// Port Status Change events: the root port number.
    pub fn port_id(&self) -> u8 {
        (self.0[0] >> 24) as u8
    }
}

/// A completion code's name for the log.
pub fn completion_name(code: u8) -> &'static str {
    match code {
        0 => "invalid",
        SUCCESS => "success",
        DATA_BUFFER_ERROR => "data buffer error",
        BABBLE => "babble",
        USB_TRANSACTION_ERROR => "USB transaction error",
        TRB_ERROR => "TRB error",
        STALL => "stall",
        RESOURCE_ERROR => "resource error",
        BANDWIDTH_ERROR => "bandwidth error",
        NO_SLOTS => "no slots available",
        SLOT_NOT_ENABLED => "slot not enabled",
        ENDPOINT_NOT_ENABLED => "endpoint not enabled",
        SHORT_PACKET => "short packet",
        PARAMETER_ERROR => "parameter error",
        CONTEXT_STATE_ERROR => "context state error",
        EVENT_RING_FULL => "event ring full",
        COMMAND_RING_STOPPED => "command ring stopped",
        COMMAND_ABORTED => "command aborted",
        STOPPED => "stopped",
        STOPPED_LENGTH_INVALID => "stopped, length invalid",
        _ => "other error",
    }
}

/// A command TRB type's name for the log.
pub fn command_name(kind: u8) -> &'static str {
    match kind {
        ENABLE_SLOT => "Enable Slot",
        DISABLE_SLOT => "Disable Slot",
        ADDRESS_DEVICE => "Address Device",
        CONFIGURE_ENDPOINT => "Configure Endpoint",
        EVALUATE_CONTEXT => "Evaluate Context",
        RESET_ENDPOINT => "Reset Endpoint",
        STOP_ENDPOINT => "Stop Endpoint",
        SET_TR_DEQUEUE => "Set TR Dequeue Pointer",
        NO_OP_COMMAND => "No Op",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_normal_trb_interrupts_on_completion_and_short_packets() {
        let t = Trb::normal(0x1_2345_6780, 8);
        // xHCI 6.4.1.1: type 1 in bits 15:10, ISP bit 2, IOC bit 5.
        assert_eq!(t.0, [0x2345_6780, 0x1, 8, 0x0000_0424]);
        assert_eq!(t.trb_type(), NORMAL);
        assert_eq!(t.pointer(), 0x1_2345_6780);
    }

    #[test]
    fn a_setup_stage_carries_the_packet_inline_with_its_transfer_type() {
        let get = Trb::setup_stage(&Setup::get_descriptor(1, 0, 18));
        // bmRequestType 0x80, bRequest 6, wValue 0x0100; wIndex 0, wLength 18.
        assert_eq!(get.0, [0x0100_0680, 0x0012_0000, 8, 0x0003_0840]);
        let set = Trb::setup_stage(&Setup::set_configuration(1));
        assert_eq!(set.0[3], 0x0000_0840, "no data stage: TRT 0");
        let out = Setup {
            request_type: 0x21,
            request: 9,
            value: 0x0200,
            index: 0,
            length: 1,
        };
        assert_eq!(Trb::setup_stage(&out).0[3] >> 16, 2, "OUT data: TRT 2");
    }

    #[test]
    fn data_and_status_stages_carry_the_direction() {
        let d = Trb::data_stage(0x8000, 18, true);
        assert_eq!(d.0, [0x8000, 0, 18, 0x0001_0C04]);
        assert_eq!(Trb::data_stage(0x8000, 1, false).0[3], 0x0000_0C04);
        assert_eq!(Trb::status_stage(false).0, [0, 0, 0, 0x0000_1020]);
        assert_eq!(Trb::status_stage(true).0[3], 0x0001_1020);
    }

    #[test]
    fn a_link_toggles_the_cycle() {
        let l = Trb::link(0xABC0_0000_1000);
        assert_eq!(l.0, [0x0000_1000, 0xABC0, 0, 0x0000_1802]);
        assert_eq!(l.trb_type(), LINK);
    }

    #[test]
    fn commands_name_their_slot_and_endpoint() {
        assert_eq!(Trb::no_op_command().0[3], 23 << 10);
        assert_eq!(Trb::enable_slot().0[3], 9 << 10);
        assert_eq!(Trb::disable_slot(3).0[3], 0x0300_2800);
        let a = Trb::address_device(0x7000, 5, false);
        assert_eq!(a.0, [0x7000, 0, 0, 0x0500_2C00]);
        assert_eq!(Trb::address_device(0x7000, 5, true).0[3], 0x0500_2E00);
        assert_eq!(Trb::configure_endpoint(0x7000, 1, false).0[3], 0x0100_3000);
        assert_eq!(Trb::configure_endpoint(0x7000, 1, true).0[3], 0x0100_3200);
        assert_eq!(Trb::evaluate_context(0x7000, 2).0[3], 0x0200_3400);
        // xHCI 6.4.3.6-6.4.3.9: the endpoint ID is in bits 20:16.
        assert_eq!(Trb::reset_endpoint(1, 3).0[3], 0x0103_3800);
        assert_eq!(Trb::stop_endpoint(2, 1).0[3], 0x0201_3C00);
        let s = Trb::set_tr_dequeue(1, 3, 0x9000_1231);
        assert_eq!(s.0, [0x9000_1231 & !0xE, 0, 0, 0x0103_4000]);
    }

    #[test]
    fn the_cycle_bit_is_set_and_cleared_without_touching_the_rest() {
        let t = Trb::status_stage(true).with_cycle(true);
        assert!(t.cycle());
        assert_eq!(t.0[3], 0x0001_1021);
        assert_eq!(t.with_cycle(false), Trb::status_stage(true));
        assert!(!t.chain());
        assert!(Trb([0, 0, 0, CHAIN]).chain());
    }

    #[test]
    fn event_fields_are_decoded() {
        // A transfer event: TRB 0x1_0000_2040, residual 5, Short Packet,
        // slot 2, DCI 3.
        let e = Trb([0x2040, 1, 13 << 24 | 5, 0x0203_8001]);
        assert_eq!(e.trb_type(), TRANSFER_EVENT);
        assert_eq!(e.pointer(), 0x1_0000_2040);
        assert_eq!(e.completion_code(), SHORT_PACKET);
        assert_eq!(e.transfer_length(), 5);
        assert_eq!((e.slot_id(), e.endpoint_id()), (2, 3));
        let p = Trb([7 << 24, 0, 1 << 24, 34 << 10]);
        assert_eq!((p.trb_type(), p.port_id()), (PORT_STATUS_CHANGE, 7));
    }

    #[test]
    fn completion_codes_and_commands_have_names() {
        assert_eq!(completion_name(STALL), "stall");
        assert_eq!(
            completion_name(USB_TRANSACTION_ERROR),
            "USB transaction error"
        );
        assert_eq!(completion_name(200), "other error");
        assert_eq!(command_name(ADDRESS_DEVICE), "Address Device");
    }
}
