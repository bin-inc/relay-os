//! A fake xHCI controller. It is as strict as hardware: whatever the xHCI
//! specification forbids panics with a message starting "fake xhci: ", so
//! a driver bug fails its test instead of passing by luck. It decodes
//! registers, TRBs and contexts with its own constants, not the driver's.

mod regs;

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
        }
    }
}

/// Something the fake does at a set time.
type Action = Box<dyn FnOnce(&mut FakeXhci, &Dma)>;

/// The fake controller: registers, and (as tasks add them) rings, slots
/// and ports. `FakeHal` routes every register access and every tick of
/// virtual time here.
pub struct FakeXhci {
    config: FakeConfig,
    now: Duration,
    usbcmd: u32,
    usbsts: u32,
    dnctrl: u32,
    crcr: u64,
    dcbaap: u64,
    config_reg: u32,
    portsc: Vec<u32>,
    iman: u32,
    imod: u32,
    erstsz: u32,
    erstba: u64,
    erdp: u64,
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
            dcbaap: 0,
            config_reg: 0,
            portsc: vec![0; ports],
            iman: 0,
            imod: 0,
            erstsz: 0,
            erstba: 0,
            erdp: 0,
            legacy: [0; 2],
            cap_reads: 0,
            highest_read: 0,
            timers: Vec::new(),
            hcrst_at: None,
            cap_writes: 0,
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
