//! A fake USB stick: Bulk-Only Transport (USB Mass Storage Class Bulk-Only
//! Transport 1.0) and the SCSI commands the driver sends (SPC-4, SBC-3),
//! over an in-memory disk. Its descriptors and standard requests are those
//! of a [`FakeUsbDevice`]; `bot` has the transport's state machine, `scsi`
//! the commands.
//!
//! It is as strict as a real device, and where a correct host has no
//! choice, stricter: whatever BOT or SCSI does not let a host do panics
//! with a message starting "fake storage: ". It decodes CBWs and command
//! blocks with its own constants, not the driver's. Its answers copy two
//! real devices: the NUC's Kingston DataTraveler 3.0 and QEMU 8.2's
//! `usb-storage` (with `scsi-disk` behind it).

mod bot;
mod scsi;

pub use scsi::{KINGSTON_BLOCKS, KINGSTON_INQUIRY, op};

use super::device::{FakeDevice, FakeUsbDevice};
use super::{FakeConfig, FakeHal, start};
use crate::xhci::{Device, Xhci};
use crate::{Hal, Setup};
use bot::Phase;
use core::time::Duration;
use scsi::op::*;
use scsi::{NO_SENSE, Sense};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

const BULK_IN: u8 = 0x81;
const BULK_OUT: u8 = 0x02;
const INTERFACE: u16 = 0;

/// Class requests (BOT 3.1, 3.2).
const RESET: u8 = 0xFF;
const GET_MAX_LUN: u8 = 0xFE;

const CBW_SIGNATURE: u32 = 0x4342_5355;
const CSW_SIGNATURE: u32 = 0x5342_5355;
const CBW_LEN: usize = 31;
const CSW_LEN: usize = 13;

/// bCSWStatus.
const PASSED: u8 = 0;
const FAILED: u8 = 1;
const PHASE_ERROR: u8 = 2;

/// A CBW as the device took it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    pub opcode: u8,
    pub tag: u32,
    pub data_length: u32,
    /// READ(10) and WRITE(10): the first block and the number of blocks.
    pub lba: u64,
    pub blocks: u32,
}

/// What the device saw, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// GET_MAX_LUN (answered or stalled).
    GetMaxLun,
    Command(Command),
    /// A Bulk-Only Mass Storage Reset.
    Reset,
    /// CLEAR_FEATURE(ENDPOINT_HALT) of this endpoint.
    ClearHalt(u8),
}

/// What a failed command does with the data phase the host announced
/// (BOT 6.7.2 and 6.7.3 allow both).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailedData {
    /// Stall the data endpoint (most sticks).
    Stall,
    /// Move the announced bytes (zeros in, discarded out), as QEMU does.
    Pad,
}

/// A mass-storage device with one LUN.
pub struct FakeStorage {
    usb: FakeUsbDevice,
    inquiry: Vec<u8>,
    blocks: u64,
    block_size: u32,
    /// Blocks written, by LBA; the others read as zeros.
    disk: BTreeMap<u64, Vec<u8>>,
    phase: Phase,
    last_tag: Option<u32>,
    sense: Sense,
    failed_data: FailedData,
    /// Halts the host must clear after a reset before the next CBW
    /// (BOT 5.3.4).
    uncleared: BTreeSet<u8>,
    events: Vec<Event>,
    /// Knob: GET_MAX_LUN's answer; `None` stalls it.
    max_lun: Option<u8>,
    /// Knob: a UNIT ATTENTION is pending (power on, reset).
    unit_attention: bool,
    /// Knob: this many more TEST UNIT READYs fail with NOT READY.
    not_ready: usize,
    /// Knob: NOT READY (becoming ready) for ever.
    never_ready: bool,
    /// Knob: NOT READY, medium not present.
    no_medium: bool,
    /// Knob: the next command ends with a phase error.
    phase_error: bool,
    /// Knob: the next data-IN or data-OUT phase stalls (the command then
    /// fails with ABORTED COMMAND).
    stall_data_in: bool,
    stall_data_out: bool,
    /// Knob: this many CSW reads stall.
    stall_csw: usize,
    /// Knob: what is wrong with the next CSW.
    bad_csw: Option<BadCsw>,
    /// Knob: the next data-IN phase keeps back this many bytes (its CSW
    /// reports them as the residue).
    short_data_in: u32,
    /// Knob: the residue the next CSW reports, whatever moved.
    residue: Option<u32>,
    /// Knob: every bulk transfer is NAKed: the device does not answer.
    nak: bool,
    /// Knob: reading this block fails with MEDIUM ERROR.
    medium_error: Option<u64>,
    /// Knob: writes fail with DATA PROTECT.
    write_protected: bool,
    /// Knob: SYNCHRONIZE CACHE fails with ILLEGAL REQUEST.
    no_cache_sync: bool,
}

/// A CSW that breaks BOT 6.3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadCsw {
    Signature,
    /// The tag of another CBW.
    Tag,
    /// 12 bytes.
    Short,
}

impl FakeStorage {
    fn new(
        usb: Rc<RefCell<FakeUsbDevice>>,
        inquiry: Vec<u8>,
        blocks: u64,
        failed_data: FailedData,
    ) -> Rc<RefCell<FakeStorage>> {
        let usb = Rc::try_unwrap(usb)
            .ok()
            .expect("a new device has one owner")
            .into_inner();
        Rc::new(RefCell::new(FakeStorage {
            usb,
            inquiry,
            blocks,
            block_size: 512,
            disk: BTreeMap::new(),
            phase: Phase::Cbw,
            last_tag: None,
            sense: NO_SENSE,
            failed_data,
            uncleared: BTreeSet::new(),
            events: Vec::new(),
            max_lun: Some(0),
            unit_attention: false,
            not_ready: 0,
            never_ready: false,
            no_medium: false,
            phase_error: false,
            stall_data_in: false,
            stall_data_out: false,
            stall_csw: 0,
            bad_csw: None,
            short_data_in: 0,
            residue: None,
            nak: false,
            medium_error: None,
            write_protected: false,
            no_cache_sync: false,
        }))
    }

    /// The NUC's Kingston DataTraveler 3.0: its descriptors, its INQUIRY
    /// data and its size (30,277,632 blocks of 512 bytes, about 14.4 GiB,
    /// stored sparsely). A failed command stalls its data phase.
    pub fn kingston() -> Rc<RefCell<FakeStorage>> {
        FakeStorage::new(
            FakeUsbDevice::kingston_stick(),
            KINGSTON_INQUIRY.to_vec(),
            KINGSTON_BLOCKS,
            FailedData::Stall,
        )
    }

    /// QEMU 8.2's usb-storage with a disk of `blocks` blocks of 512 bytes
    /// (the e2e image: 524,288). It is ready at once: in the real e2e run
    /// the first TEST UNIT READY passes (no unit attention is pending, as
    /// the driver sends no Bulk-Only reset during setup). A failed command
    /// pads its data phase.
    pub fn qemu(blocks: u64) -> Rc<RefCell<FakeStorage>> {
        FakeStorage::new(
            FakeUsbDevice::qemu_stick(),
            scsi::qemu_inquiry(),
            blocks,
            FailedData::Pad,
        )
    }

    /// The descriptors and standard requests, with their knobs.
    pub fn usb(&mut self) -> &mut FakeUsbDevice {
        &mut self.usb
    }

    /// Knob: the disk has `blocks` blocks of `block_size` bytes (what was
    /// written is forgotten).
    pub fn set_capacity(&mut self, blocks: u64, block_size: u32) {
        (self.blocks, self.block_size) = (blocks, block_size);
        self.disk.clear();
    }

    /// Knob: INQUIRY's peripheral device type (0 is a disk).
    pub fn set_peripheral_type(&mut self, kind: u8) {
        self.inquiry[0] = kind;
    }

    /// Knob: GET_MAX_LUN answers `answer`, or stalls for `None`.
    pub fn set_max_lun(&mut self, answer: Option<u8>) {
        self.max_lun = answer;
    }

    /// Knob: the next `n` TEST UNIT READYs fail with NOT READY, becoming
    /// ready (0x04/0x01), as a stick that is still powering up.
    pub fn not_ready_for(&mut self, n: usize) {
        self.not_ready = n;
    }

    /// Knob: NOT READY (becoming ready) for ever.
    pub fn never_ready(&mut self) {
        self.never_ready = true;
    }

    /// Knob: NOT READY, medium not present (0x3A/0x00): a card reader
    /// without a card.
    pub fn no_medium(&mut self) {
        self.no_medium = true;
    }

    /// Knob: a UNIT ATTENTION, power on or reset (0x29/0x00), is pending:
    /// the next command but INQUIRY and REQUEST SENSE fails with it (a
    /// device that reports its power-on, or one after a reset).
    pub fn unit_attention(&mut self) {
        self.unit_attention = true;
    }

    /// Knob: the next command ends with a phase error (CSW status 2) after
    /// its data phase.
    pub fn phase_error_next(&mut self) {
        self.phase_error = true;
    }

    /// Knob: the next data-IN phase stalls; once the host has cleared the
    /// halt, the CSW says the command failed (ABORTED COMMAND).
    pub fn stall_next_data_in(&mut self) {
        self.stall_data_in = true;
    }

    /// Knob: the next data-OUT phase stalls, as `stall_next_data_in`.
    pub fn stall_next_data_out(&mut self) {
        self.stall_data_out = true;
    }

    /// Knob: the next `n` CSW reads stall (the CSW stays pending).
    pub fn stall_csw_reads(&mut self, n: usize) {
        self.stall_csw = n;
    }

    /// Knob: the next CSW is broken this way.
    pub fn bad_csw_next(&mut self, bad: BadCsw) {
        self.bad_csw = Some(bad);
    }

    /// Knob: the next data-IN phase is `bytes` shorter than asked, and its
    /// CSW reports them as the residue.
    pub fn short_next_data_in(&mut self, bytes: u32) {
        self.short_data_in = bytes;
    }

    /// Knob: the next CSW reports a residue of `bytes`, whatever moved
    /// (Linux knows devices whose residue is wrong: US_FL_IGNORE_RESIDUE).
    pub fn residue_next(&mut self, bytes: u32) {
        self.residue = Some(bytes);
    }

    /// Knob: every bulk transfer is NAKed while `on`: transfers time out.
    pub fn nak(&mut self, on: bool) {
        self.nak = on;
    }

    /// Knob: reading block `lba` fails with MEDIUM ERROR, unrecovered read
    /// error (0x03/0x11/0x00).
    pub fn medium_error_at(&mut self, lba: u64) {
        self.medium_error = Some(lba);
    }

    /// Knob: writes fail with DATA PROTECT, write protected
    /// (0x07/0x27/0x00).
    pub fn write_protect(&mut self) {
        self.write_protected = true;
    }

    /// Knob: SYNCHRONIZE CACHE fails with ILLEGAL REQUEST, invalid command
    /// operation code (0x05/0x20/0x00), as many cheap sticks answer.
    pub fn no_synchronize_cache(&mut self) {
        self.no_cache_sync = true;
    }

    /// Everything the device saw.
    pub fn events(&self) -> Vec<Event> {
        self.events.clone()
    }

    /// The CBWs the device took.
    pub fn commands(&self) -> Vec<Command> {
        let command = |e: &Event| match e {
            Event::Command(c) => Some(*c),
            _ => None,
        };
        self.events.iter().filter_map(command).collect()
    }

    /// The operation codes of the CBWs the device took.
    pub fn opcodes(&self) -> Vec<u8> {
        self.commands().iter().map(|c| c.opcode).collect()
    }

    /// Bulk-Only Mass Storage Resets so far.
    pub fn resets(&self) -> usize {
        self.events.iter().filter(|e| **e == Event::Reset).count()
    }

    /// `count` blocks of the disk from `lba`.
    pub fn read_blocks(&self, lba: u64, count: u64) -> Vec<u8> {
        let size = self.block_size as usize;
        let mut out = Vec::new();
        for b in lba..lba + count {
            match self.disk.get(&b) {
                Some(block) => out.extend_from_slice(block),
                None => out.resize(out.len() + size, 0),
            }
        }
        out
    }

    /// Stores whole blocks of `data` from `lba`.
    pub fn write_blocks(&mut self, lba: u64, data: &[u8]) {
        let size = self.block_size as usize;
        assert!(
            data.len().is_multiple_of(size),
            "fake storage: part of a block"
        );
        for (i, block) in data.chunks(size).enumerate() {
            self.disk.insert(lba + i as u64, block.to_vec());
        }
    }
}

/// A controller made from `config` with `stick` plugged into `port`,
/// addressed and configured for its interface 0: what `MassStorage::start`
/// gets.
pub fn configured(
    config: FakeConfig,
    port: u8,
    stick: &Rc<RefCell<FakeStorage>>,
) -> (FakeHal, Xhci<FakeHal>, Device) {
    let (hal, mut xhci) = start(config);
    hal.fake().plug(port, stick.clone());
    // A USB 3 link trains for 50 ms before the port shows a connection.
    hal.sleep(Duration::from_millis(60));
    xhci.port_changes();
    let d = xhci
        .attach(port)
        .unwrap_or_else(|e| panic!("the stick was not attached: {e}\n{}", hal.log_text()));
    xhci.configure(&d, &[0]).unwrap();
    (hal, xhci, d)
}

// Helpers for the fake's own tests.

/// A CBW laid out by hand (BOT 5.1).
fn cbw_bytes(tag: u32, length: u32, dir_in: bool, cdb: &[u8]) -> Vec<u8> {
    let mut b = CBW_SIGNATURE.to_le_bytes().to_vec();
    b.extend_from_slice(&tag.to_le_bytes());
    b.extend_from_slice(&length.to_le_bytes());
    b.extend_from_slice(&[if dir_in { 0x80 } else { 0 }, 0, cdb.len() as u8]);
    b.extend_from_slice(cdb);
    b.resize(CBW_LEN, 0);
    b
}

/// Hands `bytes` to bulk OUT, which must take them.
fn send(dev: &mut FakeStorage, bytes: &[u8]) {
    assert_eq!(dev.data_out(BULK_OUT, bytes), Some(Ok(())));
}

/// Takes up to `len` bytes from bulk IN, which must have them.
fn receive(dev: &mut FakeStorage, len: usize) -> Vec<u8> {
    dev.data_in(BULK_IN, len).unwrap().unwrap()
}

/// The CSW's tag, residue and status.
fn read_csw(dev: &mut FakeStorage) -> (u32, u32, u8) {
    let b = receive(dev, CSW_LEN);
    assert_eq!(b.len(), CSW_LEN);
    assert_eq!(b[..4], CSW_SIGNATURE.to_le_bytes());
    let le32 = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    (le32(4), le32(8), b[12])
}

/// READ(10) of `blocks` blocks from `lba`, laid out by hand (SBC-3 §5.8).
fn read_10_cdb(lba: u32, blocks: u16) -> [u8; 10] {
    let [a, b, c, d] = lba.to_be_bytes();
    let [n, m] = blocks.to_be_bytes();
    [READ_10, 0, a, b, c, d, 0, n, m, 0]
}

/// A Kingston stick outside its `Rc`, to drive by hand.
fn kingston_device() -> FakeStorage {
    Rc::try_unwrap(FakeStorage::kingston())
        .ok()
        .unwrap()
        .into_inner()
}

/// A class request with `wValue` 0.
fn class_request(request_type: u8, request: u8, index: u16, length: u16) -> Setup {
    Setup {
        request_type,
        request,
        value: 0,
        index,
        length,
    }
}

/// Runs `f` on a fresh Kingston stick and returns its panic message, which
/// must be the fake's own.
fn panics_with(f: impl FnOnce(&mut FakeStorage)) -> String {
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&mut kingston_device())));
    let e = r.expect_err("the fake did not panic");
    let msg = e
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap();
    assert!(msg.starts_with("fake storage: "), "{msg}");
    msg
}
