//! A fake xHCI controller. It is as strict as hardware: whatever the xHCI
//! specification forbids panics with a message starting "fake xhci: ", so
//! a driver bug fails its test instead of passing by luck. It decodes
//! registers, TRBs and contexts with its own constants, not the driver's.

mod commands;
mod ports;
mod regs;
mod rings;

pub use commands::Executed;
use commands::FakeSlot;

use super::hal::Dma;
use core::time::Duration;

/// An extended capability of the fake.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtCap {
    /// USB Legacy Support (ID 1): USBLEGSUP and USBLEGCTLSTS.
    Legacy,
    /// Supported Protocol (ID 2): USB `major.minor` on `count` ports from
    /// `first`.
    Protocol {
        major: u8,
        minor: u8,
        first: u8,
        count: u8,
    },
    /// A capability the driver has no use for.
    Other(u8),
}

/// A capability at `offset` bytes into the BAR; `next` is the distance to
/// the next one in dwords (0 ends the list).
#[derive(Clone, Debug)]
pub struct FakeCap {
    pub offset: usize,
    pub next: u8,
    pub cap: ExtCap,
}

/// What the fake is: the capability values and the fault knobs.
#[derive(Clone, Debug)]
pub struct FakeConfig {
    pub version: u16,
    pub max_slots: u8,
    pub interrupters: u16,
    pub ports: u8,
    pub context_64: bool,
    pub scratchpads: u16,
    pub ac64: bool,
    /// Port Power Control: ports start unpowered and need PORTSC.PP.
    pub ppc: bool,
    /// The PAGESIZE register: bit n means pages of 2^(n+12) bytes.
    pub page_size: u32,
    pub cap_length: u8,
    pub rtsoff: u32,
    pub dboff: u32,
    /// Where the extended capability list starts, in bytes (0: none).
    pub xecp: usize,
    pub caps: Vec<FakeCap>,
    /// Dword 1 of every Supported Protocol capability.
    pub protocol_name: u32,
    /// Knob: every dword from `xecp` on reads as a capability whose next
    /// one is the following dword.
    pub endless_caps: bool,
    /// Knob: every register reads 0xFFFF_FFFF (powered down or gone).
    pub all_ones: bool,
    /// The legacy support capability says the BIOS owns the controller.
    pub bios_owned: bool,
    /// How long the BIOS takes to let go once asked; `None`: it never does.
    pub bios_release: Option<Duration>,
    /// USBLEGCTLSTS as the BIOS left it.
    pub legacy_ctlsts: u32,
    /// The controller is running when the driver finds it.
    pub running: bool,
    /// How long HCH takes to follow R/S = 0; `None`: it never halts.
    pub halt_time: Option<Duration>,
    /// How long HCRST takes to clear; `None`: it never does.
    pub reset_time: Option<Duration>,
    /// How long CNR stays set after HCRST cleared; `None`: forever.
    pub cnr_time: Option<Duration>,
    /// How long HCH takes to clear after R/S = 1; `None`: it never runs.
    pub run_time: Option<Duration>,
    /// Knob: commands of this TRB type never complete.
    pub hang_command: Option<u32>,
    /// Knob: CRCR.CA never stops the command ring (CRR stays 1).
    pub abort_never_completes: bool,
    /// Knob: an abort leaves the dequeue pointer on the aborted command, so
    /// the ring resumes on it (some controllers; xHCI 4.6.1.2 allows both).
    pub abort_keeps_dequeue: bool,
    /// How long a port reset takes; `None`: it never completes.
    pub port_reset_time: Option<Duration>,
    /// How long a USB 3 link trains (in Polling, CCS 0) after a connect.
    pub usb3_training: Duration,
    /// Knob: USB 3 links fail to train and end in SS.Inactive (CCS 1),
    /// where only a warm reset helps.
    pub usb3_link_fails: bool,
    /// How long a USB 2 device present when its port is reset or powered
    /// takes to signal its attach (at most 100 ms, USB 2.0 7.1.7.3).
    pub usb2_attach_delay: Duration,
}

/// Capabilities at `offsets`, each pointing to the next.
fn chain(caps: Vec<(usize, ExtCap)>) -> Vec<FakeCap> {
    let mut out: Vec<FakeCap> = Vec::new();
    for (i, (offset, cap)) in caps.iter().enumerate() {
        let next = caps.get(i + 1).map_or(0, |(o, _)| ((o - offset) / 4) as u8);
        out.push(FakeCap {
            offset: *offset,
            next,
            cap: cap.clone(),
        });
    }
    out
}

impl FakeConfig {
    /// A controller as simple as QEMU's `qemu-xhci` (xHCI 1.00, 32-byte
    /// contexts, no scratchpads, no legacy support), with USB 2 ports 1-4
    /// and USB 3 ports 5-8.
    pub fn basic() -> FakeConfig {
        FakeConfig {
            version: 0x0100,
            max_slots: 64,
            interrupters: 16,
            ports: 8,
            context_64: false,
            scratchpads: 0,
            ac64: true,
            ppc: false,
            page_size: 1,
            cap_length: 0x40,
            rtsoff: 0x1000,
            dboff: 0x2000,
            xecp: 0x20,
            caps: chain(vec![
                (
                    0x20,
                    ExtCap::Protocol {
                        major: 2,
                        minor: 0,
                        first: 1,
                        count: 4,
                    },
                ),
                (
                    0x30,
                    ExtCap::Protocol {
                        major: 3,
                        minor: 0,
                        first: 5,
                        count: 4,
                    },
                ),
            ]),
            protocol_name: u32::from_le_bytes(*b"USB "),
            endless_caps: false,
            all_ones: false,
            bios_owned: false,
            bios_release: None,
            legacy_ctlsts: 0,
            running: false,
            halt_time: Some(Duration::ZERO),
            reset_time: Some(Duration::ZERO),
            cnr_time: Some(Duration::ZERO),
            run_time: Some(Duration::ZERO),
            hang_command: None,
            abort_never_completes: false,
            abort_keeps_dequeue: false,
            port_reset_time: Some(ports::RESET_TIME),
            usb3_training: Duration::from_millis(50),
            usb3_link_fails: false,
            usb2_attach_delay: Duration::from_millis(30),
        }
    }

    /// QEMU 8.2's `qemu-xhci` as the e2e scenarios meet it: `basic`, but
    /// with USB 3 on ports 1-4 and USB 2 on ports 5-8 (QEMU lists the USB 2
    /// protocol capability first).
    pub fn qemu() -> FakeConfig {
        FakeConfig {
            caps: chain(vec![
                (
                    0x20,
                    ExtCap::Protocol {
                        major: 2,
                        minor: 0,
                        first: 5,
                        count: 4,
                    },
                ),
                (
                    0x30,
                    ExtCap::Protocol {
                        major: 3,
                        minor: 0,
                        first: 1,
                        count: 4,
                    },
                ),
            ]),
            ..FakeConfig::basic()
        }
    }

    /// The NUC's Alder Lake PCH xHCI (`8086:51ed`): xHCI 1.20, 64-byte
    /// contexts, 2 scratchpads, BIOS-owned legacy support, port power
    /// control, USB 2 ports 1-12 and USB 3 ports 13-16, with unrelated
    /// capabilities between the protocol ones.
    pub fn intel() -> FakeConfig {
        FakeConfig {
            version: 0x0120,
            max_slots: 64,
            interrupters: 8,
            ports: 16,
            context_64: true,
            scratchpads: 2,
            ac64: true,
            ppc: true,
            page_size: 1,
            cap_length: 0x80,
            rtsoff: 0x2000,
            dboff: 0x3000,
            xecp: 0x8000,
            caps: chain(vec![
                (0x8000, ExtCap::Legacy),
                (
                    0x8020,
                    ExtCap::Protocol {
                        major: 2,
                        minor: 0,
                        first: 1,
                        count: 12,
                    },
                ),
                (0x8040, ExtCap::Other(0xC0)),
                (0x8070, ExtCap::Other(0xC1)),
                (
                    0x8080,
                    ExtCap::Protocol {
                        major: 3,
                        minor: 0x10,
                        first: 13,
                        count: 4,
                    },
                ),
                (0x80A0, ExtCap::Other(10)),
            ]),
            protocol_name: u32::from_le_bytes(*b"USB "),
            endless_caps: false,
            all_ones: false,
            bios_owned: true,
            bios_release: Some(Duration::from_millis(5)),
            // SMIs on (bits 0, 4, 13-15) and pending (29-31); bit 8 is
            // reserved and must survive.
            legacy_ctlsts: 0xE000_E111,
            running: true,
            halt_time: Some(Duration::from_millis(1)),
            reset_time: Some(Duration::from_millis(2)),
            cnr_time: Some(Duration::from_millis(10)),
            run_time: Some(Duration::from_micros(500)),
            hang_command: None,
            abort_never_completes: false,
            abort_keeps_dequeue: false,
            port_reset_time: Some(ports::RESET_TIME),
            usb3_training: Duration::from_millis(50),
            usb3_link_fails: false,
            usb2_attach_delay: Duration::from_millis(30),
        }
    }
}

/// Something the fake does at a set time.
type Action = Box<dyn FnOnce(&mut FakeXhci, &Dma)>;

/// A ring the fake consumes: the command ring or a transfer ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Consumer {
    pub dequeue: u64,
    /// The Consumer Cycle State.
    pub cycle: bool,
}

/// The event ring the fake produces into (one segment).
#[derive(Clone, Copy, Debug)]
pub struct EventRing {
    pub base: u64,
    pub size: usize,
    pub enqueue: usize,
    pub cycle: bool,
}

/// The fake controller: registers, and (as tasks add them) rings, slots
/// and ports. `FakeHal` routes every register access and every tick of
/// virtual time here.
pub struct FakeXhci {
    config: FakeConfig,
    now: Duration,
    usbcmd: u32,
    usbsts: u32,
    dnctrl: u32,
    /// CRCR as written; the ring it names once the high half is written.
    crcr: u64,
    command_ring: Option<Consumer>,
    /// Command Ring Running.
    crr: bool,
    dcbaap: u64,
    config_reg: u32,
    portsc: Vec<u32>,
    port_writes: usize,
    portsc_writes: Vec<(u8, u32)>,
    /// Which ports are USB 3 (from the Supported Protocol capabilities).
    usb3: Vec<bool>,
    devices: Vec<Option<ports::Device>>,
    /// Bumped by every connect, disconnect and reset, so a timer set for
    /// an earlier state does nothing.
    port_generation: Vec<u64>,
    reset_done: Vec<Option<Duration>>,
    iman: u32,
    imod: u32,
    erstsz: u32,
    erstba: u64,
    event_ring: Option<EventRing>,
    /// ERDP without its flag bits, and Event Handler Busy.
    erdp: u64,
    ehb: bool,
    /// The scratchpad pages DCBAA[0] named when the controller started.
    scratchpad_pages: Vec<u64>,
    /// When a port was last powered on.
    powered_at: Option<Duration>,
    /// USBLEGSUP and USBLEGCTLSTS.
    legacy: [u32; 2],
    cap_reads: usize,
    highest_read: usize,
    /// Due actions, in the order they were scheduled.
    timers: Vec<(Duration, Action)>,
    /// When HCRST was last written.
    hcrst_at: Option<Duration>,
    /// Writes to the extended capability space.
    cap_writes: usize,
    /// Events waiting for the controller to run.
    pending_events: Vec<[u32; 4]>,
    events_posted: usize,
    erdp_writes: usize,
    /// Slots 1..=MaxSlots (index 0 unused).
    slots: Vec<Option<FakeSlot>>,
    /// Every command executed, in order.
    executed: Vec<Executed>,
    /// The command TRB the ring is stuck on.
    hung: Option<u64>,
    aborts: usize,
    /// CRCR's low half was written with CA; the abort happens once the
    /// high half follows.
    abort_requested: bool,
}

impl FakeXhci {
    pub fn new(config: FakeConfig) -> FakeXhci {
        let ports = config.ports as usize;
        let mut x = FakeXhci {
            config,
            now: Duration::ZERO,
            usbcmd: 0,
            usbsts: regs::HCH,
            dnctrl: 0,
            crcr: 0,
            command_ring: None,
            crr: false,
            dcbaap: 0,
            config_reg: 0,
            portsc: vec![0; ports],
            port_writes: 0,
            portsc_writes: Vec::new(),
            usb3: vec![false; ports],
            devices: vec![None; ports],
            port_generation: vec![0; ports],
            reset_done: vec![None; ports],
            iman: 0,
            imod: 0,
            erstsz: 0,
            erstba: 0,
            event_ring: None,
            erdp: 0,
            ehb: false,
            scratchpad_pages: Vec::new(),
            powered_at: None,
            legacy: [0; 2],
            cap_reads: 0,
            highest_read: 0,
            timers: Vec::new(),
            hcrst_at: None,
            cap_writes: 0,
            pending_events: Vec::new(),
            events_posted: 0,
            erdp_writes: 0,
            slots: Vec::new(),
            executed: Vec::new(),
            hung: None,
            aborts: 0,
            abort_requested: false,
        };
        x.legacy = [
            if x.config.bios_owned {
                regs::BIOS_OWNED
            } else {
                0
            },
            x.config.legacy_ctlsts,
        ];
        if x.config.running {
            x.usbcmd = regs::RUN;
            x.usbsts = 0;
        }
        x.slots = (0..=x.config.max_slots).map(|_| None).collect();
        for cap in &x.config.caps {
            if let ExtCap::Protocol {
                major: 3,
                first,
                count,
                ..
            } = cap.cap
            {
                for p in first..first.saturating_add(count) {
                    if let Some(u) = (p as usize).checked_sub(1).and_then(|i| x.usb3.get_mut(i)) {
                        *u = true;
                    }
                }
            }
        }
        x.power_on_ports();
        x
    }

    /// Lets the controller act on everything due by `now`.
    pub fn advance_to(&mut self, now: Duration, dma: &Dma) {
        while let Some(i) = self.next_due(now) {
            let (when, action) = self.timers.remove(i);
            self.now = self.now.max(when);
            action(self, dma);
        }
        self.now = now;
        self.process_commands(dma);
        self.flush_events(dma);
    }

    /// The earliest action due by `now` (the first scheduled among equals).
    fn next_due(&self, now: Duration) -> Option<usize> {
        let mut best: Option<usize> = None;
        for (i, (when, _)) in self.timers.iter().enumerate() {
            if *when <= now && best.is_none_or(|b| *when < self.timers[b].0) {
                best = Some(i);
            }
        }
        best
    }

    /// Runs `action` once the clock reaches `when`.
    pub fn at(&mut self, when: Duration, action: impl FnOnce(&mut FakeXhci, &Dma) + 'static) {
        self.timers.push((when, Box::new(action)));
    }

    /// Runs `action` `delay` from now: how tests unplug a device halfway
    /// through a driver call.
    pub fn after(&mut self, delay: Duration, action: impl FnOnce(&mut FakeXhci, &Dma) + 'static) {
        self.at(self.now + delay, action);
    }

    /// The knobs, to turn one while the driver runs (the controller
    /// falling off the bus, say).
    pub fn config_mut(&mut self) -> &mut FakeConfig {
        &mut self.config
    }

    pub fn usbcmd(&self) -> u32 {
        self.usbcmd
    }

    pub fn usbsts(&self) -> u32 {
        self.usbsts
    }

    /// USBLEGSUP and USBLEGCTLSTS.
    pub fn legacy(&self) -> [u32; 2] {
        self.legacy
    }

    pub fn cap_writes(&self) -> usize {
        self.cap_writes
    }

    /// Whether R/S is set and the controller has left the halted state.
    pub fn running(&self) -> bool {
        self.usbsts & regs::HCH == 0
    }

    pub fn dcbaap(&self) -> u64 {
        self.dcbaap
    }

    pub fn iman(&self) -> u32 {
        self.iman
    }

    pub fn event_ring(&self) -> Option<EventRing> {
        self.event_ring
    }

    pub fn command_ring(&self) -> Option<Consumer> {
        self.command_ring
    }

    pub fn scratchpad_pages(&self) -> &[u64] {
        &self.scratchpad_pages
    }

    pub fn portsc(&self, port: u8) -> u32 {
        self.portsc[port as usize - 1]
    }

    /// PORTSC writes so far.
    pub fn port_writes(&self) -> usize {
        self.port_writes
    }

    /// When software last powered a port on.
    pub fn powered_at(&self) -> Option<Duration> {
        self.powered_at
    }

    /// Event Handler Busy: set with the first event after software last
    /// cleared it through ERDP.
    pub fn ehb(&self) -> bool {
        self.ehb
    }

    /// ERDP as software last wrote it (without flags).
    pub fn erdp(&self) -> u64 {
        self.erdp
    }

    pub fn events_posted(&self) -> usize {
        self.events_posted
    }

    /// Writes of ERDP's high half: one per update.
    pub fn erdp_writes(&self) -> usize {
        self.erdp_writes
    }

    pub fn executed(&self) -> &[Executed] {
        &self.executed
    }

    /// The command TRB the ring hangs on.
    pub fn hung(&self) -> Option<u64> {
        self.hung
    }

    /// Command ring aborts (CRCR.CA) that took effect.
    pub fn aborts(&self) -> usize {
        self.aborts
    }

    pub fn slot_enabled(&self, slot: usize) -> bool {
        self.slots.get(slot).is_some_and(Option::is_some)
    }

    /// USBSTS.HSE (xHCI 4.10.2.6): the controller halts at once.
    pub fn host_system_error(&mut self) {
        self.usbsts |= regs::HSE | regs::HCH;
        self.usbcmd &= !regs::RUN;
        self.crr = false;
    }

    /// Reads of the extended capability area so far.
    pub fn cap_reads(&self) -> usize {
        self.cap_reads
    }

    /// The highest register offset read so far.
    pub fn highest_read(&self) -> usize {
        self.highest_read
    }

    pub fn config(&self) -> &FakeConfig {
        &self.config
    }

    /// Without port power control, ports are always powered (xHCI 5.4.8).
    fn power_on_ports(&mut self) {
        if !self.config.ppc {
            for p in &mut self.portsc {
                *p |= regs::PP;
            }
        }
    }
}
