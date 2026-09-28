//! One host controller with its class drivers (spec §6): devices are set up
//! when they appear on a root port (at start and when plugged in later),
//! boot-keyboard interfaces go to the keyboard driver and mass-storage
//! interfaces to the storage driver, and a device's drivers are dropped when
//! it goes away. A setup that fails is tried again, three times in all. The
//! kernel keeps one `Host` per xHCI controller and polls it from the
//! console; disks are named by a [`DiskId`] that is never reused.

use crate::hid::{BootKeyboard, KeyEvent, is_boot_keyboard};
use crate::storage::{MassStorage, is_mass_storage};
use crate::xhci::{Device, Xhci};
use crate::{Hal, UsbError};
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::fmt;

/// Key events kept for the console at most; beyond this new ones are
/// dropped (nobody is reading).
pub const MAX_EVENTS: usize = 256;
/// Setups of one device: the first and two more (plan 4's finding M4).
pub const ATTACH_TRIES: u32 = 3;

mod boot_line;

pub use boot_line::{Attached, DiskId, DiskInfo, Found};

/// A disk in use: its id, the port of its device and the driver.
struct Disk {
    id: DiskId,
    port: u8,
    storage: MassStorage,
}

pub struct Host<H: Hal> {
    xhci: Xhci<H>,
    keyboards: Vec<BootKeyboard>,
    events: VecDeque<KeyEvent>,
    disks: Vec<Disk>,
    /// The id the next disk gets.
    next_disk: u32,
}

impl<H: Hal> Host<H> {
    pub fn new(xhci: Xhci<H>) -> Host<H> {
        Host {
            xhci,
            keyboards: Vec::new(),
            events: VecDeque::new(),
            disks: Vec::new(),
            next_disk: 0,
        }
    }

    pub fn xhci(&self) -> &Xhci<H> {
        &self.xhci
    }

    /// Boot keyboards in use.
    pub fn keyboards(&self) -> usize {
        self.keyboards.len()
    }

    /// Handles ports that changed: sets up new devices, drops those that
    /// went away, and lets the keyboards do their blocking work (LEDs,
    /// recovery). Waits while a device is set up (a few hundred ms), so it
    /// runs only where the caller may block. Returns one line per device
    /// set up.
    pub fn service(&mut self) -> Vec<Attached> {
        let mut attached = Vec::new();
        for change in self.xhci.port_changes() {
            if let Some(slot) = self.xhci.slot_of_port(change.port)
                && (!change.connected || change.reconnected)
            {
                self.drop_device(slot);
            }
            if change.connected && self.xhci.slot_of_port(change.port).is_none() {
                attached.push(self.attach(change.port));
            }
        }
        for k in &mut self.keyboards {
            k.service(&mut self.xhci);
        }
        attached
    }

    /// Processes events and keyboard reports. Never waits.
    pub fn poll(&mut self) {
        self.xhci.poll();
        for k in &mut self.keyboards {
            k.poll(&mut self.xhci, &mut self.events);
        }
        self.events.truncate(MAX_EVENTS);
    }

    /// The disks in use, in the order they were set up.
    pub fn disks(&self) -> Vec<DiskInfo> {
        self.disks.iter().map(Disk::info).collect()
    }

    /// Reads whole blocks of disk `disk` from `lba` (as
    /// `MassStorage::read`). `Err(UsbError::Disconnected)` for a disk that
    /// is gone.
    pub fn read(&mut self, disk: DiskId, lba: u64, buf: &mut [u8]) -> Result<(), UsbError> {
        storage(&mut self.disks, disk)?.read(&mut self.xhci, lba, buf)
    }

    /// Writes whole blocks to disk `disk` from `lba`, as `read`.
    pub fn write(&mut self, disk: DiskId, lba: u64, buf: &[u8]) -> Result<(), UsbError> {
        storage(&mut self.disks, disk)?.write(&mut self.xhci, lba, buf)
    }

    /// Flushes disk `disk`'s cache, as `MassStorage::flush`.
    pub fn flush(&mut self, disk: DiskId) -> Result<(), UsbError> {
        storage(&mut self.disks, disk)?.flush(&mut self.xhci)
    }

    /// The oldest key event not yet taken.
    pub fn next_key(&mut self) -> Option<KeyEvent> {
        self.events.pop_front()
    }

    /// Sets up the device on `port`, [`ATTACH_TRIES`] times at most (each
    /// try resets the port afresh), unless it is gone or the controller
    /// died. Only the final outcome is reported.
    fn attach(&mut self, port: u8) -> Attached {
        let mut tries = 1;
        let outcome = loop {
            match self.xhci.attach(port).and_then(|d| self.claim(d)) {
                Err(e)
                    if tries < ATTACH_TRIES
                        && !matches!(e, UsbError::Disconnected | UsbError::ControllerDead) =>
                {
                    self.log(format_args!("port {port}: setup failed: {e}, trying again"));
                    tries += 1;
                }
                outcome => break outcome,
            }
        };
        if let Err(e) = &outcome {
            self.log(format_args!("port {port}: setup failed: {e}"));
        }
        Attached { port, outcome }
    }

    /// Configures the device for the interfaces a driver wants (all in one
    /// Configure Endpoint) and starts the drivers. A device nothing claims
    /// stays addressed but unused. A driver that does not start is shown on
    /// the boot line; it does not fail the device's setup.
    fn claim(&mut self, d: Device) -> Result<Found, UsbError> {
        let interfaces: Vec<u8> = d
            .configuration
            .interfaces
            .iter()
            .filter(|i| is_boot_keyboard(i) || is_mass_storage(i))
            .map(|i| i.number)
            .collect();
        let mut found = Found {
            vendor: d.descriptor.vendor,
            product: d.descriptor.product,
            speed: d.speed,
            keyboards: 0,
            not_started: None,
            disks: 0,
            disk_info: Vec::new(),
            disk_not_started: None,
        };
        if interfaces.is_empty() {
            self.log(format_args!("slot {}: no driver for this device", d.slot));
            return Ok(found);
        }
        if let Err(e) = self.xhci.configure(&d, &interfaces) {
            self.xhci.detach(d.slot);
            return Err(e);
        }
        for iface in d
            .configuration
            .interfaces
            .iter()
            .filter(|i| interfaces.contains(&i.number))
        {
            if is_mass_storage(iface) {
                match MassStorage::start(&mut self.xhci, d.slot, iface) {
                    Ok(storage) => {
                        let disk = Disk {
                            id: DiskId(self.next_disk),
                            port: d.port,
                            storage,
                        };
                        self.next_disk = self.next_disk.wrapping_add(1);
                        found.disk_info.push(disk.info());
                        found.disks += 1;
                        self.disks.push(disk);
                    }
                    Err(e) => {
                        self.log(format_args!(
                            "slot {} interface {}: disk not started: {e}",
                            d.slot, iface.number
                        ));
                        found.disk_not_started = Some(e);
                    }
                }
                continue;
            }
            match BootKeyboard::start(&mut self.xhci, d.slot, iface) {
                Ok(k) => {
                    self.keyboards.push(k);
                    found.keyboards += 1;
                }
                Err(e) => {
                    self.log(format_args!(
                        "slot {} interface {}: keyboard not started: {e}",
                        d.slot, iface.number
                    ));
                    found.not_started = Some(e);
                }
            }
        }
        Ok(found)
    }

    fn log(&self, args: fmt::Arguments) {
        self.xhci
            .hal()
            .log(format_args!("xhci {}: {args}", self.xhci.name()));
    }

    /// The device in `slot` is gone: its keyboards stop (a held key stops
    /// repeating with them), its disks go (their ids fail from now on) and
    /// the controller forgets it.
    fn drop_device(&mut self, slot: u8) {
        self.keyboards.retain(|k| k.slot() != slot);
        self.disks.retain(|d| d.storage.slot() != slot);
        self.xhci.detach(slot);
    }
}

/// The driver of disk `id`, if its device is still there.
fn storage(disks: &mut [Disk], id: DiskId) -> Result<&mut MassStorage, UsbError> {
    let disk = disks.iter_mut().find(|d| d.id == id);
    disk.map(|d| &mut d.storage).ok_or(UsbError::Disconnected)
}

impl Disk {
    fn info(&self) -> DiskInfo {
        DiskInfo {
            id: self.id,
            port: self.port,
            vendor: self.storage.vendor().into(),
            product: self.storage.product().into(),
            block_size: self.storage.block_size(),
            block_count: self.storage.block_count(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hid::Key;
    use crate::testing::{FakeConfig, FakeHal, FakeStorage, FakeUsbDevice, op, start};
    use alloc::string::{String, ToString};
    use alloc::vec;
    use core::time::Duration;

    fn host(config: FakeConfig) -> (FakeHal, Host<FakeHal>) {
        let (hal, xhci) = start(config);
        (hal, Host::new(xhci))
    }

    /// Polls for `ms` milliseconds of fake time and returns the characters
    /// of the key presses that came out.
    fn typed(hal: &FakeHal, host: &mut Host<FakeHal>, ms: u64) -> String {
        let mut s = String::new();
        for _ in 0..ms {
            host.poll();
            while let Some(e) = host.next_key() {
                if let (true, Key::Char(c)) = (e.pressed, e.key) {
                    s.push(c as char);
                }
            }
            hal.sleep(Duration::from_millis(1));
        }
        s
    }

    fn key_report(usage: u8) -> [u8; 8] {
        [0, 0, usage, 0, 0, 0, 0, 0]
    }

    /// QEMU's e2e disk: 256 MiB.
    const QEMU_BLOCKS: u64 = 524_288;

    #[test]
    fn devices_present_at_start_are_set_up_and_reported() {
        // Laid out as QEMU's e2e machine: USB 3 ports first.
        let (hal, mut host) = host(FakeConfig::qemu());
        hal.fake().plug(2, FakeStorage::qemu(QEMU_BLOCKS));
        hal.fake().plug(5, FakeUsbDevice::qemu_keyboard());
        // The stick's USB 3 link trains before its port shows a connection.
        hal.sleep(Duration::from_millis(60));
        let lines: Vec<String> = host.service().iter().map(|a| a.to_string()).collect();
        assert_eq!(
            lines,
            [
                "port 2: 46f4:0001 SuperSpeed, disk QEMU QEMU HARDDISK, 256 MiB",
                "port 5: 0627:0001 high-speed, keyboard",
            ]
        );
        assert_eq!(host.keyboards(), 1);
        assert_eq!(
            host.disks(),
            [DiskInfo {
                id: DiskId(0),
                port: 2,
                vendor: "QEMU".into(),
                product: "QEMU HARDDISK".into(),
                block_size: 512,
                block_count: QEMU_BLOCKS,
            }]
        );
        // Nothing changed since: nothing more to do.
        assert!(host.service().is_empty());
    }

    #[test]
    fn the_nuc_stick_is_a_disk_of_14_4_gib() {
        let (hal, mut host) = host(FakeConfig::intel());
        hal.fake().plug(15, FakeStorage::kingston());
        hal.sleep(Duration::from_millis(60));
        let attached = host.service();
        assert_eq!(
            attached[0].to_string(),
            "port 15: 0951:1666 SuperSpeed, disk Kingston DataTraveler 3.0, 14.4 GiB"
        );
        assert_eq!(attached[0].outcome.as_ref().map(|f| f.disks), Ok(1));
    }

    #[test]
    fn reads_and_writes_reach_the_disk() {
        let (hal, mut host) = host(FakeConfig::qemu());
        let stick = FakeStorage::qemu(QEMU_BLOCKS);
        hal.fake().plug(2, stick.clone());
        hal.sleep(Duration::from_millis(60));
        host.service();
        let id = host.disks()[0].id;
        let data: Vec<u8> = (0..4096).map(|i| (i % 253) as u8).collect();
        host.write(id, 2048, &data).unwrap();
        assert_eq!(stick.borrow().read_blocks(2048, 8), data);
        let mut buf = vec![0u8; 4096];
        host.read(id, 2048, &mut buf).unwrap();
        assert_eq!(buf, data);
        host.flush(id).unwrap();
        assert_eq!(
            stick.borrow().opcodes().last(),
            Some(&op::SYNCHRONIZE_CACHE_10)
        );
        assert_eq!(
            host.read(DiskId(7), 0, &mut buf),
            Err(UsbError::Disconnected)
        );
    }

    #[test]
    fn an_unplugged_disk_is_gone_for_good_and_a_replug_gets_a_new_id() {
        let (hal, mut host) = host(FakeConfig::qemu());
        hal.sleep(Duration::from_millis(60));
        assert!(host.service().is_empty());
        let before = hal.outstanding_dma();
        hal.fake().plug(2, FakeStorage::qemu(QEMU_BLOCKS));
        hal.sleep(Duration::from_millis(60));
        host.service();
        let old = host.disks()[0].id;
        hal.fake().unplug(2);
        assert!(host.service().is_empty());
        assert!(host.disks().is_empty());
        assert_eq!(hal.outstanding_dma(), before, "the stick's memory is freed");
        let mut buf = vec![0u8; 512];
        assert_eq!(host.read(old, 0, &mut buf), Err(UsbError::Disconnected));
        assert_eq!(host.write(old, 0, &buf), Err(UsbError::Disconnected));
        assert_eq!(host.flush(old), Err(UsbError::Disconnected));
        // Plugged in again: another id, which works; the old one does not.
        hal.fake().plug(2, FakeStorage::qemu(QEMU_BLOCKS));
        hal.sleep(Duration::from_millis(60));
        host.service();
        let new = host.disks()[0].id;
        assert!(new > old);
        assert_eq!(host.read(new, 0, &mut buf), Ok(()));
        assert_eq!(host.read(old, 0, &mut buf), Err(UsbError::Disconnected));
    }

    #[test]
    fn a_request_for_an_unplugged_disk_never_reaches_the_stick_plugged_in_after_it() {
        // Another stick, or the same one again: either is a new device.
        for same in [false, true] {
            let (hal, mut host) = host(FakeConfig::qemu());
            let a = FakeStorage::qemu(QEMU_BLOCKS);
            hal.fake().plug(2, a.clone());
            hal.sleep(Duration::from_millis(60));
            host.service();
            let old = host.disks()[0].id;
            // Stick A out and B in before the console looks again.
            hal.fake().unplug(2);
            let b = if same {
                a.clone()
            } else {
                FakeStorage::qemu(QEMU_BLOCKS)
            };
            let seen = b.borrow().events().len();
            hal.fake().plug(2, b.clone());
            hal.sleep(Duration::from_millis(60));
            let data = vec![0xAB; 512];
            // B has no address yet: A's slot reaches nothing.
            assert!(host.write(old, 100, &data).is_err());
            assert!(host.flush(old).is_err());
            assert_eq!(b.borrow().events().len(), seen, "B saw A's requests");
            assert_eq!(b.borrow().read_blocks(100, 1), [0; 512]);
            // The next look replaces A with B, under a new id.
            assert_eq!(host.service().len(), 1);
            let disks = host.disks();
            assert_eq!(disks.len(), 1);
            let new = disks[0].id;
            assert_ne!(new, old);
            assert_eq!(host.write(new, 100, &data), Ok(()));
            assert_eq!(b.borrow().read_blocks(100, 1), data);
            assert_eq!(host.write(old, 100, &data), Err(UsbError::Disconnected));
        }
    }

    #[test]
    fn a_request_after_an_unplug_fails_at_once_where_the_controller_never_answers() {
        // QEMU's qemu-xhci never completes a TD for a device that has gone:
        // queued, each request would wait the whole bulk timeout, once for
        // every dirty block of every sync.
        let (hal, mut host) = host(FakeConfig::qemu());
        let stick = FakeStorage::qemu(QEMU_BLOCKS);
        hal.fake().plug(2, stick.clone());
        hal.sleep(Duration::from_millis(60));
        host.service();
        let id = host.disks()[0].id;
        hal.fake().unplug(2);
        let before = hal.clock();
        let mut buf = vec![0; 512];
        assert_eq!(host.read(id, 0, &mut buf), Err(UsbError::Disconnected));
        assert_eq!(host.write(id, 0, &buf), Err(UsbError::Disconnected));
        assert_eq!(host.flush(id), Err(UsbError::Disconnected));
        assert!(
            hal.clock() - before < Duration::from_millis(10),
            "took {:?}",
            hal.clock() - before
        );
    }

    #[test]
    fn a_request_after_a_replug_between_looks_fails_at_once_and_sends_nothing() {
        let (hal, mut host) = host(FakeConfig::qemu());
        let a = FakeStorage::qemu(QEMU_BLOCKS);
        hal.fake().plug(2, a.clone());
        hal.sleep(Duration::from_millis(60));
        host.service();
        let old = host.disks()[0].id;
        hal.fake().unplug(2);
        let b = FakeStorage::qemu(QEMU_BLOCKS);
        hal.fake().plug(2, b.clone());
        hal.sleep(Duration::from_millis(60));
        // The port shows a device again, but a connect change nobody has
        // looked at yet: it may not be A.
        let before = hal.clock();
        assert_eq!(
            host.write(old, 100, &[0xAB; 512]),
            Err(UsbError::Disconnected)
        );
        assert!(hal.clock() - before < Duration::from_millis(10));
        assert!(b.borrow().events().is_empty(), "B saw A's request");
    }

    #[test]
    fn a_disk_given_up_as_not_answering_works_again_when_plugged_in_again() {
        let (hal, mut host) = host(FakeConfig::qemu());
        let stick = FakeStorage::qemu(QEMU_BLOCKS);
        hal.fake().plug(2, stick.clone());
        hal.sleep(Duration::from_millis(60));
        host.service();
        let old = host.disks()[0].id;
        stick.borrow_mut().nak(true);
        let mut buf = vec![0u8; 512];
        assert_eq!(host.read(old, 0, &mut buf), Err(UsbError::Timeout));
        let before = hal.clock();
        assert_eq!(host.read(old, 0, &mut buf), Err(UsbError::Timeout));
        assert!(hal.clock() - before < Duration::from_millis(1));
        // The firmware came back after a replug: a new disk.
        hal.fake().unplug(2);
        host.service();
        stick.borrow_mut().nak(false);
        hal.fake().plug(2, stick);
        hal.sleep(Duration::from_millis(60));
        host.service();
        let new = host.disks()[0].id;
        assert_ne!(new, old);
        assert_eq!(host.read(new, 0, &mut buf), Ok(()));
    }

    #[test]
    fn a_disk_that_cannot_be_started_says_why_and_is_not_set_up_again() {
        let (hal, mut host) = host(FakeConfig::intel());
        let stick = FakeStorage::usb2(1 << 20);
        stick.borrow_mut().no_medium();
        hal.fake().plug(3, stick.clone());
        let attached = host.service();
        assert_eq!(
            attached[0].to_string(),
            "port 3: 0951:1665 high-speed, disk not started: NOT READY (asc 0x3a, ascq 0x00)"
        );
        assert!(host.disks().is_empty());
        let inquiries = stick
            .borrow()
            .opcodes()
            .iter()
            .filter(|&&o| o == op::INQUIRY)
            .count();
        assert_eq!(inquiries, 1);
        assert!(
            hal.log_text()
                .contains("xhci 00:14.0: slot 1 interface 0: disk not started: NOT READY")
        );
        assert!(!hal.log_text().contains("trying again"));
    }

    #[test]
    fn a_device_with_a_keyboard_and_a_disk_has_both() {
        let (hal, mut host) = host(FakeConfig::intel());
        let stick = FakeStorage::kingston();
        stick.borrow_mut().add_boot_keyboard();
        hal.fake().plug(13, stick.clone());
        hal.sleep(Duration::from_millis(60));
        let attached = host.service();
        assert_eq!(
            attached[0].to_string(),
            "port 13: 0951:1666 SuperSpeed, keyboard, disk Kingston DataTraveler 3.0, 14.4 GiB"
        );
        assert_eq!((host.keyboards(), host.disks().len()), (1, 1));
        // One Configure Endpoint for both interfaces.
        let configures = hal
            .fake()
            .executed()
            .iter()
            .filter(|e| e.kind == 12)
            .count();
        assert_eq!(configures, 1);
        let slot = host.xhci().slot_of_port(13).unwrap() as usize;
        for dci in [3, 4, 7] {
            assert!(hal.fake().endpoint(slot, dci).is_some(), "DCI {dci}");
        }
        stick.borrow_mut().usb().push_in(0x83, &key_report(0x0B));
        stick.borrow_mut().usb().push_in(0x83, &key_report(0));
        assert_eq!(typed(&hal, &mut host, 100), "h");
    }

    #[test]
    fn a_device_whose_first_setup_fails_is_set_up_again() {
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        // The first GET_DESCRIPTOR is never answered.
        k120.borrow_mut().ignore_requests(1);
        hal.fake().plug(3, k120);
        let attached = host.service();
        assert_eq!(attached.len(), 1);
        assert_eq!(
            attached[0].to_string(),
            "port 3: 046d:c31c low-speed, keyboard"
        );
        assert!(
            hal.log_text()
                .contains("xhci 00:14.0: port 3: setup failed: timed out, trying again")
        );
        assert_eq!(host.keyboards(), 1);
    }

    #[test]
    fn a_device_that_always_fails_is_tried_three_times_and_reported_once() {
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        k120.borrow_mut().stall_request(0x06, 0x0100);
        hal.fake().plug(3, k120.clone());
        let attached = host.service();
        assert_eq!(attached.len(), 1);
        assert_eq!(attached[0].to_string(), "port 3: setup failed: stalled");
        let log = hal.log_text();
        assert_eq!(
            log.matches("port 3: setup failed: stalled, trying again")
                .count(),
            2
        );
        let device_descriptors = k120
            .borrow()
            .requests()
            .iter()
            .filter(|r| r.setup.request == 0x06 && r.setup.value == 0x0100)
            .count();
        assert_eq!(device_descriptors, 3);
    }

    #[test]
    fn a_setup_that_kills_the_controller_is_not_tried_again() {
        let mut config = FakeConfig::intel();
        // Configure Endpoint hangs and so does its abort: the controller is
        // given up.
        config.hang_command = Some(12);
        config.abort_never_completes = true;
        let (hal, mut host) = host(config);
        hal.fake().plug(3, FakeUsbDevice::k120());
        let attached = host.service();
        assert_eq!(attached[0].outcome, Err(UsbError::ControllerDead));
        let log = hal.log_text();
        assert_eq!(log.matches("trying again").count(), 1, "{log}");
        assert!(log.contains("port 3: setup failed: timed out, trying again"));
    }

    #[test]
    fn a_device_that_is_unplugged_during_setup_is_not_tried_again() {
        let (hal, mut host) = host(FakeConfig::intel());
        hal.fake().plug(3, FakeUsbDevice::k120());
        // After the debounce, during the port reset.
        hal.fake()
            .after(Duration::from_millis(105), |x, _| x.unplug(3));
        let attached = host.service();
        assert_eq!(attached.len(), 1);
        assert_eq!(attached[0].outcome, Err(UsbError::Disconnected));
        assert!(!hal.log_text().contains("trying again"));
    }

    #[test]
    fn typing_on_a_keyboard_gives_key_events() {
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        hal.fake().plug(3, k120.clone());
        host.service();
        for usage in [0x0B, 0x00, 0x0C, 0x00] {
            k120.borrow_mut().push_in(0x81, &key_report(usage));
        }
        assert_eq!(typed(&hal, &mut host, 200), "hi");
    }

    #[test]
    fn only_the_receivers_keyboard_interface_is_claimed() {
        let (hal, mut host) = host(FakeConfig::intel());
        hal.fake().plug(1, FakeUsbDevice::unifying_receiver());
        let attached = host.service();
        assert_eq!(
            attached[0].to_string(),
            "port 1: 046d:c534 full-speed, keyboard"
        );
        let slot = host.xhci().slot_of_port(1).unwrap() as usize;
        // Interface 0's endpoint 0x81 (DCI 3) only; the mouse's 0x82 and
        // the vendor interface's 0x83 stay unconfigured.
        assert!(hal.fake().endpoint(slot, 3).is_some());
        assert!(hal.fake().endpoint(slot, 5).is_none());
        assert!(hal.fake().endpoint(slot, 7).is_none());
    }

    #[test]
    fn a_keyboard_plugged_in_later_is_picked_up_and_dropped_when_unplugged() {
        let (hal, mut host) = host(FakeConfig::intel());
        assert!(host.service().is_empty());
        let before = hal.outstanding_dma();
        let k120 = FakeUsbDevice::k120();
        hal.fake().plug(3, k120.clone());
        assert_eq!(host.service().len(), 1);
        assert_eq!(host.keyboards(), 1);
        hal.fake().unplug(3);
        assert!(host.service().is_empty());
        assert_eq!(host.keyboards(), 0);
        assert_eq!(host.xhci().slot_of_port(3), None);
        assert_eq!(
            hal.outstanding_dma(),
            before,
            "the device's memory is freed"
        );
        // Plugged in again, it works again.
        let k120 = FakeUsbDevice::k120();
        hal.fake().plug(3, k120.clone());
        assert_eq!(host.service().len(), 1);
        k120.borrow_mut().push_in(0x81, &key_report(0x04));
        k120.borrow_mut().push_in(0x81, &key_report(0x00));
        assert_eq!(typed(&hal, &mut host, 100), "a");
    }

    #[test]
    fn a_quick_replug_replaces_the_device() {
        let (hal, mut host) = host(FakeConfig::intel());
        hal.fake().plug(3, FakeUsbDevice::k120());
        host.service();
        // Unplugged and another keyboard plugged in before the console
        // looked again: the port is connected both times.
        hal.fake().unplug(3);
        let other = FakeUsbDevice::qemu_keyboard();
        hal.fake().plug(3, other.clone());
        let attached = host.service();
        assert_eq!(attached.len(), 1);
        assert_eq!(
            attached[0].to_string(),
            "port 3: 0627:0001 high-speed, keyboard"
        );
        assert_eq!(host.keyboards(), 1);
        other.borrow_mut().push_in(0x81, &key_report(0x05));
        assert_eq!(typed(&hal, &mut host, 100), "b");
    }

    #[test]
    fn a_held_key_stops_repeating_when_its_keyboard_is_unplugged() {
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        hal.fake().plug(3, k120.clone());
        host.service();
        k120.borrow_mut().push_in(0x81, &key_report(0x04));
        assert_eq!(typed(&hal, &mut host, 100), "a");
        hal.fake().unplug(3);
        host.service();
        assert_eq!(typed(&hal, &mut host, 2000), "");
        assert_eq!(host.keyboards(), 0);
    }

    #[test]
    fn a_keyboard_whose_transfers_just_stop_is_dropped_too() {
        // A controller that does not fail the queued report on unplug: the
        // keyboard never sees an error, so only the port change stops it.
        let mut config = FakeConfig::intel();
        config.fail_transfers_on_unplug = false;
        let (hal, mut host) = host(config);
        let k120 = FakeUsbDevice::k120();
        hal.fake().plug(3, k120.clone());
        host.service();
        k120.borrow_mut().push_in(0x81, &key_report(0x04));
        assert_eq!(typed(&hal, &mut host, 100), "a");
        hal.fake().unplug(3);
        host.service();
        assert_eq!(host.keyboards(), 0);
        assert_eq!(typed(&hal, &mut host, 2000), "");
    }

    #[test]
    fn a_keyboard_that_cannot_be_started_says_so_on_screen() {
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        k120.borrow_mut().stall_request(0x0B, 0); // SET_PROTOCOL(boot)
        hal.fake().plug(3, k120);
        let attached = host.service();
        assert_eq!(
            attached[0].to_string(),
            "port 3: 046d:c31c low-speed, keyboard not started: stalled"
        );
        assert_eq!(host.keyboards(), 0);
    }

    #[test]
    fn a_device_that_fails_setup_is_reported_once() {
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        k120.borrow_mut().stall_request(0x06, 0x0100);
        hal.fake().plug(3, k120);
        let attached = host.service();
        assert_eq!(attached[0].to_string(), "port 3: setup failed: stalled");
        assert!(host.service().is_empty());
        assert!(
            hal.log_text()
                .contains("xhci 00:14.0: port 3: setup failed: stalled")
        );
    }

    #[test]
    fn unread_key_events_are_bounded() {
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        hal.fake().plug(3, k120.clone());
        host.service();
        for _ in 0..MAX_EVENTS {
            k120.borrow_mut().push_in(0x81, &key_report(0x04));
            k120.borrow_mut().push_in(0x81, &key_report(0x00));
        }
        for _ in 0..4 * MAX_EVENTS {
            host.poll();
            hal.sleep(Duration::from_millis(1));
        }
        assert_eq!(host.events.len(), MAX_EVENTS);
    }

    #[test]
    fn a_keyboard_plugged_in_before_boot_is_found_by_the_first_service() {
        use crate::testing::{FAKE_BAR, FAKE_BAR_LEN};
        let hal = FakeHal::with_controller(FakeConfig::intel());
        hal.fake().plug(3, FakeUsbDevice::k120());
        let xhci = Xhci::new(hal.clone(), FAKE_BAR, FAKE_BAR_LEN, "00:14.0").unwrap();
        let mut host = Host::new(xhci);
        assert_eq!(host.service().len(), 1);
        assert_eq!(host.keyboards(), 1, "[ ok ] keyboard at boot");
    }
}
