//! Starting the controller (spec §6.2): the memory it reads from the
//! start (scratchpad buffers, DCBAA, command and event rings), programming
//! and running it, port power, and `Xhci::new`, which puts it all together
//! after the BIOS handoff, halt and reset of `init`.

use super::caps::{CapList, PortProtocol, Protocol, port_map, protocols};
use super::init::{REGISTER_TIMEOUT, bios_handoff, halt, reset, wait_for};
use super::regs::{
    CONFIG, CRCR, DCBAAP, ERDP, ERSTBA, ERSTSZ, HCH, IE, IMAN, INTE, PAGESIZE, PP, Params, RCS,
    RUN, Regs, USBCMD, USBSTS, portsc_neutral,
};
use super::ring::{EventRing, ProducerRing};
use super::{ControllerInfo, Xhci};
use crate::{DmaBuf, Hal, UsbError};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Write;
use core::time::Duration;

/// How long a device may take to signal its attach after its port was
/// powered or reset (USB 2.0 7.1.7.3, TSIGATT), so devices present at boot
/// are connected when the first `port_changes` looks.
const ATTACH_TIME: Duration = Duration::from_millis(100);
/// How long `Xhci::new` waits at most, in all, for USB 3 links present at
/// boot to finish training.
const SETTLE_LIMIT: Duration = Duration::from_secs(1);
/// The only page size this driver sets up scratchpad buffers for.
const PAGE: usize = 4096;

/// The scratchpad buffers (xHCI 4.20): pages the controller keeps its own
/// state in, named by an array DCBAA[0] points to.
#[derive(Debug)]
pub struct Scratchpads {
    array: DmaBuf,
    pages: Vec<DmaBuf>,
}

impl Scratchpads {
    /// `count` pages and the array naming them; `None` for 0.
    pub fn new<H: Hal>(hal: &H, count: u16) -> Result<Option<Scratchpads>, UsbError> {
        if count == 0 {
            return Ok(None);
        }
        let array = hal
            .alloc_dma(8 * count as usize, 64)
            .ok_or(UsbError::NoMemory)?;
        let mut s = Scratchpads {
            array,
            pages: Vec::new(),
        };
        for i in 0..count as usize {
            let Some(page) = hal.alloc_dma(PAGE, PAGE) else {
                s.free(hal);
                return Err(UsbError::NoMemory);
            };
            s.array.write64(8 * i, page.phys());
            s.pages.push(page);
        }
        Ok(Some(s))
    }

    pub fn free<H: Hal>(self, hal: &H) {
        for page in self.pages {
            hal.free_dma(page);
        }
        hal.free_dma(self.array);
    }
}

/// What the controller reads from memory from the start.
struct Memory {
    dcbaa: DmaBuf,
    scratchpads: Option<Scratchpads>,
    commands: ProducerRing,
    events: EventRing,
}

impl Memory {
    /// Allocates all of it, or nothing.
    fn new<H: Hal>(hal: &H, params: &Params) -> Result<Memory, UsbError> {
        let scratchpads = Scratchpads::new(hal, params.scratchpads)?;
        let dcbaa = hal.alloc_dma(8 * (params.max_slots as usize + 1), 64);
        let commands = ProducerRing::new(hal).ok();
        let events = EventRing::new(hal).ok();
        match (dcbaa, commands, events) {
            (Some(dcbaa), Some(commands), Some(events)) => {
                if let Some(s) = &scratchpads {
                    dcbaa.write64(0, s.array.phys());
                }
                Ok(Memory {
                    dcbaa,
                    scratchpads,
                    commands,
                    events,
                })
            }
            (dcbaa, commands, events) => {
                if let Some(b) = dcbaa {
                    hal.free_dma(b);
                }
                if let Some(r) = commands {
                    r.free(hal);
                }
                if let Some(r) = events {
                    r.free(hal);
                }
                if let Some(s) = scratchpads {
                    s.free(hal);
                }
                Err(UsbError::NoMemory)
            }
        }
    }

    fn free<H: Hal>(self, hal: &H) {
        hal.free_dma(self.dcbaa);
        self.commands.free(hal);
        self.events.free(hal);
        if let Some(s) = self.scratchpads {
            s.free(hal);
        }
    }
}

/// Programs the rings and starts the controller (xHCI 4.2): slots, DCBAA,
/// command ring, the primary interrupter with ERSTBA last (xHCI 4.9.4),
/// interrupts off (milestone 1 polls), then R/S and up to 1 s for HCH = 0.
fn run<H: Hal>(
    hal: &H,
    regs: &Regs,
    params: &Params,
    mem: &Memory,
    name: &str,
) -> Result<(), UsbError> {
    let config = regs.op_read(hal, CONFIG);
    regs.op_write(hal, CONFIG, config & !0xFF | params.max_slots as u32);
    regs.op_write64(hal, DCBAAP, mem.dcbaa.phys());
    regs.op_write64(hal, CRCR, mem.commands.phys() | RCS as u64);
    regs.intr_write(hal, ERSTSZ, 1);
    regs.intr_write64(hal, ERDP, mem.events.dequeue_pointer());
    regs.intr_write64(hal, ERSTBA, mem.events.erst_phys());
    let iman = regs.intr_read(hal, IMAN);
    regs.intr_write(hal, IMAN, iman & !IE);
    let cmd = regs.op_read(hal, USBCMD) & !INTE;
    regs.op_write(hal, USBCMD, cmd | RUN);
    if wait_for(hal, REGISTER_TIMEOUT, || {
        regs.op_read(hal, USBSTS) & HCH == 0
    })
    .is_none()
    {
        xlog!(
            hal,
            name,
            "USBSTS.HCH still 1 {} ms after R/S = 1",
            REGISTER_TIMEOUT.as_millis()
        );
        regs.op_write(hal, USBCMD, cmd);
        return Err(UsbError::Timeout);
    }
    Ok(())
}

/// With Port Power Control, ports start unpowered: powers each one that
/// is off. Returns whether any was.
fn power_ports<H: Hal>(hal: &H, regs: &Regs, params: &Params) -> bool {
    if !params.ppc {
        return false;
    }
    let mut powered = false;
    for port in 1..=params.ports {
        let sc = regs.portsc(hal, port);
        if sc & PP == 0 {
            regs.set_portsc(hal, port, portsc_neutral(sc) | PP);
            powered = true;
        }
    }
    powered
}

/// "USB 2.0 ports 1-12, USB 3.1 ports 13-16".
fn describe(protocols: &[Protocol]) -> String {
    let mut line = String::new();
    for (i, p) in protocols.iter().enumerate() {
        let sep = if i == 0 { "" } else { ", " };
        let last = (p.first as u16 + p.count as u16).saturating_sub(1);
        let _ = write!(
            line,
            "{sep}USB {}.{:x} ports {}-{last}",
            p.major,
            p.minor >> 4,
            p.first
        );
    }
    if line.is_empty() {
        line.push_str("no supported protocol capabilities");
    }
    line
}

impl<H: Hal> Xhci<H> {
    /// Brings the controller up (spec §6.2): maps `mmio_len` bytes of
    /// registers at `mmio_phys`, BIOS handoff, halt, reset, scratchpad
    /// buffers, DCBAA, command and event rings, run, port power. `name`
    /// (the PCI address, "00:14.0") starts every log line. On an error
    /// everything allocated is freed again.
    pub fn new(hal: H, mmio_phys: u64, mmio_len: usize, name: &str) -> Result<Xhci<H>, UsbError> {
        let started = hal.now();
        let Some(base) = hal.map_mmio(mmio_phys, mmio_len) else {
            xlog!(
                &hal,
                name,
                "cannot map {mmio_len:#x} bytes of registers at {mmio_phys:#x}"
            );
            return Err(UsbError::NoMemory);
        };
        let regs = Regs::new(&hal, base, mmio_len)
            .inspect_err(|e| xlog!(&hal, name, "registers at {mmio_phys:#x}: {e}"))?;
        let params = Params::read(&hal, &regs);
        let page = regs.op_read(&hal, PAGESIZE);
        xlog!(
            &hal,
            name,
            "xHCI {:x}.{:02x}, {} slots, {} ports, {} interrupters, {}-byte contexts, page size {} KiB",
            params.version >> 8,
            params.version & 0xFF,
            params.max_slots,
            params.ports,
            params.interrupters,
            params.context_size,
            4u64 << page.trailing_zeros().min(20),
        );
        if !params.ac64 {
            xlog!(&hal, name, "no 64-bit addressing (HCCPARAMS1.AC64 = 0)");
            return Err(UsbError::Unsupported("32-bit DMA only"));
        }
        if page & 1 == 0 {
            xlog!(
                &hal,
                name,
                "PAGESIZE {page:#x}: only 4 KiB pages are supported"
            );
            return Err(UsbError::Unsupported("page size other than 4 KiB"));
        }
        let caps = CapList::walk(&hal, &regs, params.xecp);
        if let Some(problem) = caps.problem {
            xlog!(&hal, name, "extended capabilities: {problem}");
        }
        bios_handoff(&hal, &regs, &caps, name);
        halt(&hal, &regs, name)?;
        reset(&hal, &regs, name)?;
        // HCRST reset the root ports too.
        let mut settle_from = hal.now();
        let found = protocols(&hal, &regs, &caps);
        xlog!(&hal, name, "{}", describe(&found.usable));
        for p in &found.empty {
            xlog!(
                &hal,
                name,
                "USB {}.{:x} protocol capability names no ports (first {}, count {}); ignored",
                p.major,
                p.minor >> 4,
                p.first,
                p.count
            );
        }
        let ports = port_map(params.ports, &found.usable);
        for (i, _) in ports.iter().enumerate().filter(|(_, p)| p.is_none()) {
            xlog!(
                &hal,
                name,
                "port {}: no supported protocol, not used",
                i + 1
            );
        }
        let mem =
            Memory::new(&hal, &params).inspect_err(|_| xlog!(&hal, name, "out of DMA memory"))?;
        match &mem.scratchpads {
            Some(s) => xlog!(
                &hal,
                name,
                "{} scratchpad buffers, array {:#x}",
                s.pages.len(),
                s.array.phys()
            ),
            None => xlog!(&hal, name, "no scratchpad buffers"),
        }
        xlog!(
            &hal,
            name,
            "DCBAA {:#x}, command ring {:#x}, event ring {:#x}",
            mem.dcbaa.phys(),
            mem.commands.phys(),
            mem.events.phys()
        );
        if let Err(e) = run(&hal, &regs, &params, &mem, name) {
            mem.free(&hal);
            return Err(e);
        }
        let powered = power_ports(&hal, &regs, &params);
        if powered {
            settle_from = hal.now();
        }
        xlog!(
            &hal,
            name,
            "running{}",
            if powered { ", ports powered" } else { "" }
        );
        let count = |kind| ports.iter().filter(|&&p| p == Some(kind)).count() as u8;
        let info: ControllerInfo =
            params.info(count(PortProtocol::Usb2), count(PortProtocol::Usb3));
        let mut xhci = Xhci {
            hal,
            name: name.to_string(),
            regs,
            info,
            ports,
            dcbaa: mem.dcbaa,
            scratchpads: mem.scratchpads,
            commands: mem.commands,
            events: mem.events,
            pending: None,
            ring_stopped: false,
            dead: false,
            port_flags: alloc::vec![false; params.ports as usize],
            first_scan: true,
        };
        xhci.settle_ports(started, settle_from);
        Ok(xhci)
    }

    /// Waits until 100 ms after the ports were last powered or reset, so
    /// that devices present at boot have signalled their attach, then,
    /// polling, up to 1 s after `started` in all while a USB 3 link still
    /// trains (in Polling it shows no connection yet).
    fn settle_ports(&mut self, started: Duration, since: Duration) {
        let start = self.hal.now();
        self.hal.sleep((since + ATTACH_TIME).saturating_sub(start));
        while self.usb3_link_training().is_some() && self.hal.now() - started < SETTLE_LIMIT {
            self.poll();
            self.hal.sleep(Duration::from_millis(1));
        }
        let waited = (self.hal.now() - start).as_millis();
        match self.usb3_link_training() {
            Some(port) => xlog!(
                &self.hal,
                &self.name,
                "port {port}: USB 3 link still training after {waited} ms"
            ),
            None => xlog!(&self.hal, &self.name, "ports settled after {waited} ms"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{ExtCap, FAKE_BAR, FAKE_BAR_LEN, FakeConfig, FakeHal, start};

    fn new(hal: &FakeHal) -> Result<Xhci<FakeHal>, UsbError> {
        Xhci::new(hal.clone(), FAKE_BAR, FAKE_BAR_LEN, "00:14.0")
    }

    #[test]
    fn qemu_and_intel_controllers_start() {
        for config in [FakeConfig::basic(), FakeConfig::intel()] {
            let (hal, xhci) = start(config);
            let fake = hal.fake();
            assert!(fake.running());
            assert_eq!(fake.dcbaap(), xhci.dcbaa.phys());
            let ring = fake.command_ring().unwrap();
            assert_eq!((ring.dequeue, ring.cycle), (xhci.commands.phys(), true));
            assert_eq!(fake.event_ring().unwrap().base, xhci.events.phys());
            assert_eq!(xhci.name(), "00:14.0");
        }
        let (_, xhci) = start(FakeConfig::intel());
        assert_eq!(
            xhci.info().to_string(),
            "xHCI 1.20, 16 ports (12 USB 2, 4 USB 3), 64-byte contexts, 2 scratchpads"
        );
    }

    #[test]
    fn interrupts_stay_off() {
        let (hal, _xhci) = start(FakeConfig::intel());
        assert_eq!(hal.fake().iman() & IE, 0);
        assert_eq!(hal.fake().usbcmd() & (INTE | RUN), RUN);
    }

    #[test]
    fn scratchpad_pages_are_where_dcbaa_0_says() {
        for n in [2, 33] {
            let mut config = FakeConfig::intel();
            config.scratchpads = n;
            let (hal, xhci) = start(config);
            let s = xhci.scratchpads.as_ref().unwrap();
            assert_eq!(xhci.dcbaa.read64(0), s.array.phys());
            let pages: Vec<u64> = s.pages.iter().map(|p| p.phys()).collect();
            assert_eq!(hal.fake().scratchpad_pages(), &pages[..]);
            assert_eq!(pages.len(), n as usize);
            assert!(
                hal.log_text()
                    .contains(&alloc::format!("{n} scratchpad buffers, array 0x"))
            );
        }
    }

    #[test]
    fn without_scratchpads_nothing_is_allocated_for_them() {
        let (hal, xhci) = start(FakeConfig::basic());
        assert!(xhci.scratchpads.is_none());
        assert_eq!(xhci.dcbaa.read64(0), 0);
        // The DCBAA, the command ring, the event ring and its ERST.
        assert_eq!(hal.outstanding_dma(), 4);
        assert!(hal.log_text().contains("no scratchpad buffers"));
    }

    #[test]
    fn page_sizes_other_than_4_kib_are_refused() {
        let mut config = FakeConfig::intel();
        config.page_size = 2;
        let hal = FakeHal::with_controller(config);
        assert_eq!(
            new(&hal).err(),
            Some(UsbError::Unsupported("page size other than 4 KiB"))
        );
        assert_eq!(hal.outstanding_dma(), 0);
        assert!(hal.log_text().contains("page size 8 KiB"));
        assert!(
            hal.log_text()
                .contains("PAGESIZE 0x2: only 4 KiB pages are supported")
        );
    }

    #[test]
    fn controllers_without_64_bit_addressing_are_refused() {
        let mut config = FakeConfig::basic();
        config.ac64 = false;
        let hal = FakeHal::with_controller(config);
        assert_eq!(
            new(&hal).err(),
            Some(UsbError::Unsupported("32-bit DMA only"))
        );
        assert_eq!(hal.outstanding_dma(), 0);
    }

    #[test]
    fn a_controller_reading_all_ones_does_not_start() {
        let mut config = FakeConfig::intel();
        config.all_ones = true;
        let hal = FakeHal::with_controller(config);
        assert_eq!(new(&hal).err(), Some(UsbError::NotResponding));
        assert!(hal.log_text().contains("controller not responding"));
    }

    #[test]
    fn a_start_that_fails_at_any_allocation_frees_every_dma_buffer() {
        // Intel's start allocates 7 buffers: 2 scratchpad pages and their
        // array, the DCBAA, the command ring, the event ring and its ERST.
        for n in 0..7 {
            let hal = FakeHal::with_controller(FakeConfig::intel());
            hal.fail_alloc_after(n);
            assert_eq!(new(&hal).err(), Some(UsbError::NoMemory), "allocation {n}");
            assert_eq!(hal.outstanding_dma(), 0, "allocation {n}");
            assert!(!hal.fake().running());
        }
        let hal = FakeHal::with_controller(FakeConfig::intel());
        hal.fail_alloc_after(7);
        assert!(new(&hal).is_ok());
        // One allocation failing while the later ones succeed.
        for n in 0..7 {
            let hal = FakeHal::with_controller(FakeConfig::intel());
            hal.fail_one_alloc(n);
            assert_eq!(new(&hal).err(), Some(UsbError::NoMemory), "allocation {n}");
            assert_eq!(hal.outstanding_dma(), 0, "allocation {n}");
        }
    }

    #[test]
    fn a_controller_that_never_runs_is_stopped_and_freed() {
        let mut config = FakeConfig::intel();
        config.run_time = None;
        let hal = FakeHal::with_controller(config);
        assert_eq!(new(&hal).err(), Some(UsbError::Timeout));
        assert_eq!(hal.outstanding_dma(), 0);
        assert_eq!(hal.fake().usbcmd() & RUN, 0);
        assert!(
            hal.log_text()
                .contains("USBSTS.HCH still 1 1000 ms after R/S = 1")
        );
    }

    #[test]
    fn a_failed_halt_or_reset_allocates_nothing() {
        let mut config = FakeConfig::intel();
        config.halt_time = None;
        let hal = FakeHal::with_controller(config);
        assert_eq!(new(&hal).err(), Some(UsbError::Timeout));
        assert_eq!(hal.outstanding_dma(), 0);
        let mut config = FakeConfig::intel();
        config.cnr_time = None;
        let hal = FakeHal::with_controller(config);
        assert_eq!(new(&hal).err(), Some(UsbError::Timeout));
        assert_eq!(hal.outstanding_dma(), 0);
    }

    #[test]
    fn the_log_has_what_a_photo_of_the_nuc_screen_needs() {
        let (hal, _xhci) = start(FakeConfig::intel());
        let log = hal.log_text();
        for line in [
            "xhci 00:14.0: xHCI 1.20, 64 slots, 16 ports, 8 interrupters, 64-byte contexts, page size 4 KiB",
            "xhci 00:14.0: legacy support at 0x8000: BIOS owned, handed over after 5 ms; SMI enables 0x0000e011 cleared",
            "xhci 00:14.0: halted after 1 ms",
            "xhci 00:14.0: reset after 12 ms",
            "xhci 00:14.0: USB 2.0 ports 1-12, USB 3.1 ports 13-16",
            "xhci 00:14.0: 2 scratchpad buffers, array 0x",
            "xhci 00:14.0: DCBAA 0x",
            "xhci 00:14.0: running, ports powered",
        ] {
            assert!(log.contains(line), "missing {line:?} in\n{log}");
        }
    }

    #[test]
    fn ports_get_power_when_the_controller_controls_it() {
        let (hal, _xhci) = start(FakeConfig::intel());
        for port in 1..=16 {
            assert_eq!(hal.fake().portsc(port) & PP, PP, "port {port}");
        }
        let powered = hal.fake().powered_at().unwrap();
        assert!(
            hal.clock() - powered >= ATTACH_TIME,
            "devices get time to signal their attach"
        );
        let (hal, _xhci) = start(FakeConfig::basic());
        assert_eq!(
            hal.fake().port_writes(),
            0,
            "QEMU's ports are always powered"
        );
        assert!(hal.log_text().contains("xhci 00:14.0: running\n"));
    }

    #[test]
    fn a_port_no_protocol_covers_is_logged_and_counted_nowhere() {
        let mut config = FakeConfig::basic();
        config
            .caps
            .retain(|c| !matches!(c.cap, ExtCap::Protocol { major: 3, .. }));
        config.caps[0].next = 0;
        let (hal, xhci) = start(config);
        assert_eq!((xhci.info().usb2_ports, xhci.info().usb3_ports), (4, 0));
        assert_eq!(xhci.ports[4], None);
        assert!(
            hal.log_text()
                .contains("xhci 00:14.0: port 5: no supported protocol, not used")
        );
    }

    #[test]
    fn an_idle_controller_waits_only_the_attach_time() {
        let (hal, _xhci) = start(FakeConfig::intel());
        let waited = hal.clock() - hal.fake().powered_at().unwrap();
        assert!(waited >= ATTACH_TIME && waited < ATTACH_TIME + Duration::from_millis(1));
        assert!(
            hal.log_text()
                .ends_with("xhci 00:14.0: ports settled after 100 ms")
        );
        // Without port power control the reset is what the ports settle
        // from.
        let (hal, _xhci) = start(FakeConfig::basic());
        assert!(hal.clock() >= ATTACH_TIME && hal.clock() < ATTACH_TIME + Duration::from_millis(5));
    }

    #[test]
    fn an_empty_protocol_capability_does_not_panic() {
        let mut config = FakeConfig::qemu();
        if let ExtCap::Protocol { first, count, .. } = &mut config.caps[1].cap {
            (*first, *count) = (0, 0);
        }
        let (hal, xhci) = start(config);
        assert_eq!((xhci.info().usb2_ports, xhci.info().usb3_ports), (4, 0));
        assert!(hal.log_text().contains(
            "xhci 00:14.0: USB 3.0 protocol capability names no ports (first 0, count 0); ignored"
        ));
    }
}
