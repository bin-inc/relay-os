//! One disk behind a mass-storage interface, LUN 0 (spec §6.4): claiming
//! the interface and setting the disk up. Setup is GET_MAX_LUN (BOT 3.2),
//! INQUIRY (SPC-4 §6.4), TEST UNIT READY until the stick is ready (SPC-4
//! §6.37), and READ CAPACITY(10), or (16) for a big disk (SBC-3 §5.15,
//! §5.16).

use super::scsi::{self, Capacity, Inquiry, NOT_READY};
use super::transport::{Data, Need, Transport, is_fatal};
use crate::UsbError;
use crate::bus::{Bus, DIR_IN, RECIPIENT_INTERFACE, Setup, TYPE_CLASS};
use crate::descriptor::{EndpointKind, Interface};
use alloc::string::String;
use core::time::Duration;

/// How long setup waits for TEST UNIT READY to pass (spec §6.4).
pub const READY_TIMEOUT: Duration = Duration::from_secs(5);
/// The pause between two TEST UNIT READYs.
pub const READY_POLL: Duration = Duration::from_millis(100);
/// GET_MAX_LUN (BOT 3.2).
const GET_MAX_LUN: u8 = 0xFE;
/// ASC "medium not present" (SPC-4 annex D).
const MEDIUM_NOT_PRESENT: u8 = 0x3A;

/// The first bulk IN and bulk OUT endpoints of `iface`.
fn bulk_endpoints(iface: &Interface) -> Option<(u8, u8)> {
    let bulk = |is_in: bool| {
        iface
            .endpoints
            .iter()
            .find(|e| e.kind == EndpointKind::Bulk && e.is_in() == is_in && e.packet_size() > 0)
            .map(|e| e.address)
    };
    Some((bulk(true)?, bulk(false)?))
}

/// Class 8 (mass storage), subclass 6 (SCSI transparent), protocol 0x50
/// (Bulk-Only), with a bulk IN and a bulk OUT endpoint.
pub fn is_mass_storage(iface: &Interface) -> bool {
    (iface.class, iface.subclass, iface.protocol) == (8, 6, 0x50) && bulk_endpoints(iface).is_some()
}

/// One disk behind a mass-storage interface (LUN 0).
pub struct MassStorage {
    pub(super) transport: Transport,
    pub(super) block_size: usize,
    pub(super) block_count: u64,
    vendor: String,
    product: String,
    /// SYNCHRONIZE CACHE was refused: there is no cache to flush.
    pub(super) no_cache: bool,
}

impl MassStorage {
    /// Setup (spec §6.4): GET_MAX_LUN (a STALL means 0; only LUN 0 is
    /// used), INQUIRY (logged as vendor, product, revision; a peripheral
    /// type other than 0 is `Unsupported("not a disk")`), TEST UNIT READY
    /// retried for up to 5 s with REQUEST SENSE after each failure and 100
    /// ms between tries, READ CAPACITY(10), then (16) if the answer is
    /// 0xFFFFFFFF. A block size other than 512, 1024, 2048 or 4096 is
    /// `Unsupported("block size")`; more than 2^32 blocks is
    /// `Unsupported("over 2^32 blocks")` (READ(10) cannot reach further).
    pub fn start(bus: &mut dyn Bus, slot: u8, iface: &Interface) -> Result<MassStorage, UsbError> {
        let Some((bulk_in, bulk_out)) = bulk_endpoints(iface) else {
            return Err(UsbError::Unsupported("no bulk IN and OUT endpoints"));
        };
        let mut t = Transport::new(slot, iface.number, bulk_in, bulk_out);
        max_lun(bus, slot, iface.number)?;
        let inquiry = inquiry(bus, &mut t)?;
        wait_until_ready(bus, &mut t)?;
        let (block_size, block_count) = capacity(bus, &mut t)?;
        slog!(bus, slot, "{block_count} blocks of {block_size} bytes");
        Ok(MassStorage {
            transport: t,
            block_size,
            block_count,
            vendor: inquiry.vendor,
            product: inquiry.product,
            no_cache: false,
        })
    }

    pub fn slot(&self) -> u8 {
        self.transport.slot()
    }

    /// Bytes in a block: 512, 1024, 2048 or 4096.
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    pub fn block_count(&self) -> u64 {
        self.block_count
    }

    /// INQUIRY's vendor identification, without padding.
    pub fn vendor(&self) -> &str {
        &self.vendor
    }

    /// INQUIRY's product identification, without padding.
    pub fn product(&self) -> &str {
        &self.product
    }
}

/// GET_MAX_LUN (BOT 3.2): only LUN 0 is used, so the answer is only
/// logged. Many single-LUN devices stall it; any failure but a gone device
/// means one LUN, as in Linux.
fn max_lun(bus: &mut dyn Bus, slot: u8, interface: u8) -> Result<(), UsbError> {
    let setup = Setup {
        request_type: DIR_IN | TYPE_CLASS | RECIPIENT_INTERFACE,
        request: GET_MAX_LUN,
        value: 0,
        index: interface as u16,
        length: 1,
    };
    let mut lun = [0u8];
    match bus.control(slot, setup, &mut lun) {
        Ok(1) if lun[0] > 0 => slog!(bus, slot, "{} LUNs, only LUN 0 is used", lun[0] as u32 + 1),
        Ok(_) => {}
        Err(UsbError::Stall) => slog!(bus, slot, "GET_MAX_LUN stalled: one LUN"),
        Err(e) if is_fatal(e) => return Err(e),
        Err(e) => slog!(bus, slot, "GET_MAX_LUN failed ({e}): one LUN"),
    }
    Ok(())
}

/// Runs a command that reads into `buf` (tried three times; each failure is
/// logged) and returns what came.
fn read_data<'a>(
    bus: &mut dyn Bus,
    t: &mut Transport,
    cdb: &[u8],
    buf: &'a mut [u8],
) -> Result<&'a [u8], UsbError> {
    let n = t.command(bus, cdb, Data::In(buf), Need::UpTo)?;
    Ok(&buf[..n.min(buf.len())])
}

/// INQUIRY: the names, and whether it is a disk at all.
fn inquiry(bus: &mut dyn Bus, t: &mut Transport) -> Result<Inquiry, UsbError> {
    let slot = t.slot();
    let mut buf = [0u8; scsi::INQUIRY_LEN];
    let cdb = scsi::inquiry(scsi::INQUIRY_LEN as u16);
    let data = read_data(bus, t, &cdb, &mut buf)?;
    let i = Inquiry::parse(data).inspect_err(|e| slog!(bus, slot, "INQUIRY: {e}"))?;
    slog!(
        bus,
        slot,
        "vendor \"{}\", product \"{}\", revision \"{}\"{}",
        i.vendor,
        i.product,
        i.revision,
        if i.removable { ", removable" } else { "" }
    );
    if i.peripheral_type != 0 {
        slog!(
            bus,
            slot,
            "peripheral type {} is not a disk",
            i.peripheral_type
        );
        return Err(UsbError::Unsupported("not a disk"));
    }
    Ok(i)
}

/// TEST UNIT READY until it passes, for up to [`READY_TIMEOUT`]: a stick
/// may take a while after power-on, and the first command may meet a unit
/// attention (power on, reset), which REQUEST SENSE (in `execute`) clears.
/// A sense seen again is not logged again. No medium does not wait.
fn wait_until_ready(bus: &mut dyn Bus, t: &mut Transport) -> Result<(), UsbError> {
    let slot = t.slot();
    let start = bus.now();
    let mut last = None;
    let mut tries = 0u32;
    loop {
        tries = tries.saturating_add(1);
        let e = match t.execute(bus, &scsi::test_unit_ready(), Data::None, Need::UpTo) {
            Ok(_) => {
                if tries > 1 {
                    slog!(bus, slot, "ready after {tries} tries");
                }
                return Ok(());
            }
            Err(e) if is_fatal(e) => return Err(e),
            Err(e) => e,
        };
        if last != Some(e) {
            slog!(bus, slot, "TEST UNIT READY: {e}");
        }
        if let UsbError::Sense(s) = e
            && (s.key, s.asc) == (NOT_READY, MEDIUM_NOT_PRESENT)
        {
            slog!(bus, slot, "no medium");
            return Err(e);
        }
        if bus.now().saturating_sub(start) >= READY_TIMEOUT {
            slog!(bus, slot, "not ready after 5 s: {e}");
            return Err(e);
        }
        last = Some(e);
        bus.sleep(READY_POLL);
    }
}

/// READ CAPACITY(10), then (16) if the disk is too big for it: the block
/// size and the number of blocks.
fn capacity(bus: &mut dyn Bus, t: &mut Transport) -> Result<(usize, u64), UsbError> {
    let slot = t.slot();
    let mut buf = [0u8; scsi::CAPACITY_16_LEN];
    let cdb = scsi::read_capacity_10();
    let data = read_data(bus, t, &cdb, &mut buf[..scsi::CAPACITY_10_LEN])?;
    let r = Capacity::parse_10(data);
    let mut cap = r.inspect_err(|e| slog!(bus, slot, "READ CAPACITY(10): {e}"))?;
    if cap.last_lba == 0xFFFF_FFFF {
        let cdb = scsi::read_capacity_16(scsi::CAPACITY_16_LEN as u32);
        let r = Capacity::parse_16(read_data(bus, t, &cdb, &mut buf)?);
        cap = r.inspect_err(|e| slog!(bus, slot, "READ CAPACITY(16): {e}"))?;
    }
    if !matches!(cap.block_size, 512 | 1024 | 2048 | 4096) {
        slog!(bus, slot, "block size {} not supported", cap.block_size);
        return Err(UsbError::Unsupported("block size"));
    }
    // READ(10) addresses blocks with 32 bits: the last LBA must fit.
    if cap.last_lba > 0xFFFF_FFFF {
        slog!(bus, slot, "last LBA {:#x}: over 2^32 blocks", cap.last_lba);
        return Err(UsbError::Unsupported("over 2^32 blocks"));
    }
    Ok((cap.block_size as usize, cap.last_lba + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::Endpoint;
    use crate::storage::Sense;
    use crate::testing::{Event, FakeConfig, FakeHal, FakeStorage, TamperBus, configured, op};
    use crate::xhci::Xhci;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Stick = Rc<RefCell<FakeStorage>>;

    fn start(
        config: FakeConfig,
        port: u8,
        stick: &Stick,
    ) -> (FakeHal, Xhci<FakeHal>, Result<MassStorage, UsbError>) {
        let (hal, mut xhci, d) = configured(config, port, stick);
        let r = MassStorage::start(&mut xhci, d.slot, &d.configuration.interfaces[0]);
        (hal, xhci, r)
    }

    fn kingston_with(
        knobs: impl FnOnce(&mut FakeStorage),
    ) -> (FakeHal, Stick, Result<MassStorage, UsbError>) {
        let stick = FakeStorage::kingston();
        knobs(&mut stick.borrow_mut());
        let (hal, _xhci, r) = start(FakeConfig::intel(), 13, &stick);
        (hal, stick, r)
    }

    fn sense(key: u8, asc: u8, ascq: u8) -> UsbError {
        UsbError::Sense(Sense { key, asc, ascq })
    }

    #[test]
    fn the_kingston_stick_starts_on_the_nuc_controller() {
        let stick = FakeStorage::kingston();
        let (hal, _xhci, r) = start(FakeConfig::intel(), 13, &stick);
        let disk = r.unwrap();
        assert_eq!(disk.slot(), 1);
        assert_eq!(
            (disk.vendor(), disk.product()),
            ("Kingston", "DataTraveler 3.0")
        );
        assert_eq!((disk.block_size(), disk.block_count()), (512, 30_277_632));
        assert_eq!(
            stick.borrow().opcodes(),
            [op::INQUIRY, op::TEST_UNIT_READY, op::READ_CAPACITY_10]
        );
        let log = hal.log_text();
        assert!(log.contains(
            "storage: slot 1: vendor \"Kingston\", product \"DataTraveler 3.0\", revision \"PMAP\", removable"
        ));
        assert!(log.contains("storage: slot 1: 30277632 blocks of 512 bytes"));
        assert!(!log.contains("ready after"));
    }

    /// The device's events as GET_MAX_LUN, opcodes, resets and cleared
    /// halts, for comparing sequences.
    fn trace(stick: &Stick) -> Vec<String> {
        let name = |e: &Event| match e {
            Event::GetMaxLun => "GET_MAX_LUN".into(),
            Event::Command(c) => format!("{:02x}", c.opcode),
            Event::Reset => "reset".into(),
            Event::ClearHalt(ep) => format!("clear {ep:02x}"),
        };
        stick.borrow().events().iter().map(name).collect()
    }

    #[test]
    fn the_qemu_stick_starts_with_its_first_test_unit_ready() {
        let stick = FakeStorage::qemu(524_288);
        let (hal, _xhci, r) = start(FakeConfig::qemu(), 2, &stick);
        let disk = r.unwrap();
        assert_eq!((disk.vendor(), disk.product()), ("QEMU", "QEMU HARDDISK"));
        assert_eq!((disk.block_size(), disk.block_count()), (512, 524_288));
        // GET_MAX_LUN, then INQUIRY, TEST UNIT READY (which passes: no
        // REQUEST SENSE) and READ CAPACITY(10), as real QEMU 8.2 answers.
        assert_eq!(trace(&stick), ["GET_MAX_LUN", "12", "00", "25"]);
        // What the boot log of the real run shows, and nothing else.
        let log = hal.log_text();
        let lines: Vec<&str> = log.lines().filter(|l| l.starts_with("storage: ")).collect();
        assert_eq!(
            lines,
            [
                "storage: slot 1: vendor \"QEMU\", product \"QEMU HARDDISK\", revision \"2.5+\"",
                "storage: slot 1: 524288 blocks of 512 bytes",
            ]
        );
    }

    #[test]
    fn a_power_on_unit_attention_is_cleared_by_request_sense() {
        let (hal, stick, r) = kingston_with(|s| s.unit_attention());
        assert!(r.is_ok());
        // The first TEST UNIT READY meets the unit attention; REQUEST
        // SENSE clears it and the next one passes.
        assert_eq!(trace(&stick), ["GET_MAX_LUN", "12", "00", "03", "00", "25"]);
        let log = hal.log_text();
        assert!(
            log.contains("storage: slot 1: TEST UNIT READY: UNIT ATTENTION (asc 0x29, ascq 0x00)")
        );
        assert!(log.contains("storage: slot 1: ready after 2 tries"));
    }

    #[test]
    fn a_stick_that_is_not_ready_for_a_few_tries_becomes_ready() {
        let (hal, stick, r) = kingston_with(|s| s.not_ready_for(3));
        assert!(r.is_ok());
        let turs = stick
            .borrow()
            .opcodes()
            .iter()
            .filter(|&&o| o == op::TEST_UNIT_READY)
            .count();
        assert_eq!(turs, 4);
        let log = hal.log_text();
        // The same sense three times is logged once.
        assert_eq!(
            log.matches("TEST UNIT READY: NOT READY (asc 0x04, ascq 0x01)")
                .count(),
            1
        );
        assert!(log.contains("storage: slot 1: ready after 4 tries"));
    }

    #[test]
    fn a_stick_that_never_becomes_ready_fails_after_5_s_with_its_sense() {
        let stick = FakeStorage::kingston();
        stick.borrow_mut().never_ready();
        let (hal, mut xhci, d) = configured(FakeConfig::intel(), 13, &stick);
        let before = hal.clock();
        let r = MassStorage::start(&mut xhci, d.slot, &d.configuration.interfaces[0]);
        let took = hal.clock() - before;
        assert_eq!(r.err(), Some(sense(2, 4, 1)));
        assert!(
            took >= READY_TIMEOUT && took < READY_TIMEOUT + Duration::from_secs(1),
            "{took:?}"
        );
        assert!(
            hal.log_text()
                .contains("storage: slot 1: not ready after 5 s: NOT READY (asc 0x04, ascq 0x01)")
        );
        // No capacity was asked of a disk that is not ready, and the tries
        // were 100 ms apart.
        let opcodes = stick.borrow().opcodes();
        assert!(!opcodes.contains(&op::READ_CAPACITY_10));
        let turs = opcodes
            .iter()
            .filter(|&&o| o == op::TEST_UNIT_READY)
            .count();
        assert!((50..=52).contains(&turs), "{turs} tries");
    }

    #[test]
    fn a_stick_that_goes_while_it_is_waited_for_fails_at_once() {
        let stick = FakeStorage::kingston();
        stick.borrow_mut().never_ready();
        let (hal, mut xhci, d) = configured(FakeConfig::intel(), 13, &stick);
        let mut bus = TamperBus::new(&mut xhci);
        // INQUIRY and a TEST UNIT READY with its REQUEST SENSE (3 bulk
        // transfers each), then the device is gone.
        bus.fail_bulk = Some((9, UsbError::Disconnected));
        let before = hal.clock();
        let r = MassStorage::start(&mut bus, d.slot, &d.configuration.interfaces[0]);
        assert_eq!(r.err(), Some(UsbError::Disconnected));
        assert!(hal.clock() - before < Duration::from_secs(1));
        assert_eq!(bus.bulk, 10);
    }

    #[test]
    fn a_stick_gone_before_setup_fails_at_once() {
        let stick = FakeStorage::kingston();
        let (hal, mut xhci, d) = configured(FakeConfig::intel(), 13, &stick);
        hal.fake().unplug(13);
        xhci.detach(d.slot);
        let r = MassStorage::start(&mut xhci, d.slot, &d.configuration.interfaces[0]);
        assert_eq!(r.err(), Some(UsbError::Disconnected));
        assert!(!hal.log_text().contains("GET_MAX_LUN"));
    }

    #[test]
    fn a_reader_without_a_medium_fails_at_once() {
        let stick = FakeStorage::kingston();
        stick.borrow_mut().no_medium();
        let (hal, mut xhci, d) = configured(FakeConfig::intel(), 13, &stick);
        let before = hal.clock();
        let r = MassStorage::start(&mut xhci, d.slot, &d.configuration.interfaces[0]);
        assert_eq!(r.err(), Some(sense(2, 0x3A, 0)));
        assert!(hal.clock() - before < Duration::from_secs(1));
        assert!(hal.log_text().contains("storage: slot 1: no medium"));
    }

    #[test]
    fn get_max_lun_stalled_means_one_lun_and_more_luns_still_use_lun_0() {
        let (hal, _, r) = kingston_with(|s| s.set_max_lun(None));
        assert!(r.is_ok());
        assert!(
            hal.log_text()
                .contains("storage: slot 1: GET_MAX_LUN stalled: one LUN")
        );
        // The fake panics on a CBW for another LUN.
        let (hal, _, r) = kingston_with(|s| s.set_max_lun(Some(3)));
        assert!(r.is_ok());
        assert!(
            hal.log_text()
                .contains("storage: slot 1: 4 LUNs, only LUN 0 is used")
        );
        let (hal, stick, r) = kingston_with(|_| {});
        assert!(r.is_ok());
        assert!(!hal.log_text().contains("LUN"));
        let get_max_lun = Setup {
            request_type: 0xA1,
            request: 0xFE,
            value: 0,
            index: 0,
            length: 1,
        };
        assert!(
            stick
                .borrow_mut()
                .usb()
                .requests()
                .iter()
                .any(|r| r.setup == get_max_lun)
        );
    }

    #[test]
    fn a_device_that_is_not_a_disk_is_refused_after_inquiry() {
        let (hal, stick, r) = kingston_with(|s| s.set_peripheral_type(5));
        assert_eq!(r.err(), Some(UsbError::Unsupported("not a disk")));
        assert_eq!(stick.borrow().opcodes(), [op::INQUIRY]);
        assert!(
            hal.log_text()
                .contains("storage: slot 1: peripheral type 5 is not a disk")
        );
    }

    #[test]
    fn a_capacity_of_0xffffffff_blocks_asks_read_capacity_16() {
        // Exactly 2^32 blocks: READ(10) still reaches the last one.
        let (_, stick, r) = kingston_with(|s| s.set_capacity(1 << 32, 512));
        assert_eq!(r.unwrap().block_count(), 1 << 32);
        assert_eq!(
            stick.borrow().opcodes()[2..],
            [op::READ_CAPACITY_10, op::SERVICE_ACTION_IN_16]
        );
        let (_, stick, r) = kingston_with(|s| s.set_capacity(0xFFFF_FFFF, 512));
        assert_eq!(r.unwrap().block_count(), 0xFFFF_FFFF);
        assert!(!stick.borrow().opcodes().contains(&op::SERVICE_ACTION_IN_16));
    }

    #[test]
    fn more_than_2_pow_32_blocks_are_refused() {
        let (hal, _, r) = kingston_with(|s| s.set_capacity((1 << 32) + 1, 512));
        assert_eq!(r.err(), Some(UsbError::Unsupported("over 2^32 blocks")));
        assert!(
            hal.log_text()
                .contains("storage: slot 1: last LBA 0x100000000: over 2^32 blocks")
        );
    }

    #[test]
    fn a_disk_of_4096_byte_blocks_starts_and_odd_block_sizes_are_refused() {
        let (_, _, r) = kingston_with(|s| s.set_capacity(1000, 4096));
        let disk = r.unwrap();
        assert_eq!((disk.block_size(), disk.block_count()), (4096, 1000));
        for size in [2048, 1024] {
            assert!(kingston_with(|s| s.set_capacity(1000, size)).2.is_ok());
        }
        for size in [520, 256, 8192, 0] {
            let (hal, _, r) = kingston_with(|s| s.set_capacity(1000, size));
            assert_eq!(r.err(), Some(UsbError::Unsupported("block size")));
            assert!(
                hal.log_text()
                    .contains(&format!("block size {size} not supported"))
            );
        }
    }

    #[test]
    fn a_setup_command_that_fails_is_tried_again() {
        let (hal, stick, r) = kingston_with(|s| s.phase_error_next());
        assert!(r.is_ok());
        assert!(
            hal.log_text()
                .contains("storage: slot 1: INQUIRY: protocol error: phase error; reset recovery")
        );
        assert_eq!(
            trace(&stick),
            [
                "GET_MAX_LUN",
                "12",
                "reset",
                "clear 81",
                "clear 02",
                "12",
                "00",
                "25"
            ]
        );
        // READ CAPACITY too.
        let (_hal, stick, r) = kingston_with(|s| {
            s.stall_csw_reads(2);
            s.set_capacity(1000, 512);
        });
        assert_eq!(r.map(|d| d.block_count()), Ok(1000));
        assert_eq!(stick.borrow().resets(), 1);
    }

    fn storage_interface(endpoints: Vec<Endpoint>) -> Interface {
        Interface {
            number: 0,
            class: 8,
            subclass: 6,
            protocol: 0x50,
            endpoints,
        }
    }

    fn bulk(address: u8) -> Endpoint {
        Endpoint {
            address,
            kind: EndpointKind::Bulk,
            max_packet: 512,
            interval: 0,
            max_burst: 0,
        }
    }

    #[test]
    fn mass_storage_interfaces_need_bot_scsi_and_two_bulk_endpoints() {
        assert!(is_mass_storage(&storage_interface(vec![
            bulk(0x81),
            bulk(0x02)
        ])));
        assert!(is_mass_storage(&storage_interface(vec![
            bulk(0x02),
            bulk(0x83)
        ])));
        assert!(!is_mass_storage(&storage_interface(vec![bulk(0x81)])));
        assert!(!is_mass_storage(&storage_interface(vec![
            bulk(0x81),
            bulk(0x82)
        ])));
        let mut interrupt = bulk(0x02);
        interrupt.kind = EndpointKind::Interrupt;
        assert!(!is_mass_storage(&storage_interface(vec![
            bulk(0x81),
            interrupt
        ])));
        let mut empty = bulk(0x02);
        empty.max_packet = 0;
        assert!(!is_mass_storage(&storage_interface(vec![
            bulk(0x81),
            empty
        ])));
        for (class, subclass, protocol) in [(8, 6, 0x62), (8, 2, 0x50), (3, 6, 0x50), (8, 6, 0)] {
            let mut i = storage_interface(vec![bulk(0x81), bulk(0x02)]);
            (i.class, i.subclass, i.protocol) = (class, subclass, protocol);
            assert!(!is_mass_storage(&i), "{class}/{subclass}/{protocol}");
        }
    }

    #[test]
    fn a_stick_without_bulk_endpoints_is_refused_by_start() {
        let stick = FakeStorage::kingston();
        let (_hal, mut xhci, d) = configured(FakeConfig::intel(), 13, &stick);
        let mut iface = d.configuration.interfaces[0].clone();
        iface.endpoints.truncate(1);
        assert_eq!(
            MassStorage::start(&mut xhci, d.slot, &iface).err(),
            Some(UsbError::Unsupported("no bulk IN and OUT endpoints"))
        );
        assert!(stick.borrow().events().is_empty());
    }
}
