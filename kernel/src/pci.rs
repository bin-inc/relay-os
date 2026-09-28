//! PCI enumeration through ECAM (spec §5.5): every bus in the MCFG range,
//! multi-function devices, and 32/64-bit BARs sized by writing all ones.
//! Configuration space is reached through `ConfigSpace`, so enumeration is
//! tested on the host against a fake modelled on the NUC.

use alloc::vec::Vec;
use core::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PciAddress {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

impl PciAddress {
    /// Offset of this function's 4 KiB configuration space in an ECAM
    /// window that starts at bus 0.
    pub fn ecam_offset(&self) -> u64 {
        ((self.bus as u64) << 20) | ((self.device as u64) << 15) | ((self.function as u64) << 12)
    }
}

impl fmt::Display for PciAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:02x}:{:02x}.{}", self.bus, self.device, self.function)
    }
}

/// 32-bit configuration space access.
pub trait ConfigSpace {
    fn read32(&mut self, a: PciAddress, offset: u16) -> u32;
    fn write32(&mut self, a: PciAddress, offset: u16, value: u32);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarKind {
    Memory32,
    Memory64,
    Io,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bar {
    pub index: u8,
    pub kind: BarKind,
    pub address: u64,
    pub size: u64,
    pub prefetchable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PciDevice {
    pub address: PciAddress,
    pub vendor: u16,
    pub device: u16,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub revision: u8,
    /// Header layout: 0 device, 1 PCI bridge, 2 CardBus bridge.
    pub header_type: u8,
    pub bars: Vec<Bar>,
}

impl PciDevice {
    /// A USB xHCI controller (class 0x0C, subclass 0x03, prog-if 0x30).
    pub fn is_xhci(&self) -> bool {
        (self.class, self.subclass, self.prog_if) == (0x0C, 0x03, 0x30)
    }
}

const COMMAND: u16 = 0x04;
const IO_SPACE: u32 = 1 << 0;
const MEMORY_SPACE: u32 = 1 << 1;
const BUS_MASTER: u32 = 1 << 2;
const BAR0: u16 = 0x10;
const HEADER_MULTI_FUNCTION: u8 = 0x80;
/// Status register bit 4 (the upper half of dword 1): a capability list.
const STATUS_CAPABILITIES: u32 = 1 << 20;
const CAPABILITIES_POINTER: u16 = 0x34;
const CAP_POWER_MANAGEMENT: u8 = 0x01;
/// PMCSR bit 3: the function keeps its configuration from D3hot to D0.
const NO_SOFT_RESET: u32 = 1 << 3;

/// Reads one function's header and sizes its BARs. `None` if nothing
/// answers at `a`.
fn probe(cfg: &mut impl ConfigSpace, a: PciAddress) -> Option<PciDevice> {
    let id = cfg.read32(a, 0x00);
    if id & 0xFFFF == 0xFFFF {
        return None;
    }
    let class = cfg.read32(a, 0x08);
    let header_type = (cfg.read32(a, 0x0C) >> 16) as u8 & 0x7F;
    let host_bridge = class >> 16 == 0x0600;
    let bar_count = match header_type {
        // On some chipsets a host bridge's memory-decode bit gates the
        // CPU's own memory accesses; Linux never turns it off either.
        _ if host_bridge => 0,
        0 => 6,
        1 => 2,
        _ => 0,
    };
    Some(PciDevice {
        address: a,
        vendor: id as u16,
        device: (id >> 16) as u16,
        class: (class >> 24) as u8,
        subclass: (class >> 16) as u8,
        prog_if: (class >> 8) as u8,
        revision: class as u8,
        header_type,
        bars: size_bars(cfg, a, bar_count),
    })
}

/// BAR `i`'s value and the bits that stick when all ones are written to it
/// (the original value is written back).
fn bar_value_and_mask(cfg: &mut impl ConfigSpace, a: PciAddress, i: u8) -> (u32, u32) {
    let off = BAR0 + 4 * i as u16;
    let original = cfg.read32(a, off);
    cfg.write32(a, off, 0xFFFF_FFFF);
    let mask = cfg.read32(a, off);
    cfg.write32(a, off, original);
    (original, mask)
}

/// Decodes and sizes the first `count` BARs. Memory and I/O decoding are
/// turned off while each BAR briefly holds all ones, then everything is
/// restored.
pub fn size_bars(cfg: &mut impl ConfigSpace, a: PciAddress, count: u8) -> Vec<Bar> {
    let mut bars = Vec::new();
    if count == 0 {
        return bars;
    }
    // Only the low half is the command register. Writing zeros to the
    // status half leaves its write-one-to-clear bits alone.
    let command = cfg.read32(a, COMMAND) & 0xFFFF;
    cfg.write32(a, COMMAND, command & !(IO_SPACE | MEMORY_SPACE));
    let mut i = 0;
    while i < count {
        let (original, lo_mask) = bar_value_and_mask(cfg, a, i);
        let (kind, address, mask) = if original & 1 == 1 {
            (
                BarKind::Io,
                (original & !0x3) as u64,
                (lo_mask & !0x3) as u64,
            )
        } else if (original >> 1) & 0x3 == 0x2 && i + 1 < count {
            let (original_hi, hi_mask) = bar_value_and_mask(cfg, a, i + 1);
            let address = ((original_hi as u64) << 32) | (original & !0xF) as u64;
            (
                BarKind::Memory64,
                address,
                ((hi_mask as u64) << 32) | (lo_mask & !0xF) as u64,
            )
        } else {
            (
                BarKind::Memory32,
                (original & !0xF) as u64,
                (lo_mask & !0xF) as u64,
            )
        };
        if mask != 0 {
            bars.push(Bar {
                index: i,
                kind,
                address,
                // The lowest writable address bit is the size (for I/O
                // BARs this also ignores upper bits that read as zero).
                size: mask.isolate_lowest_one(),
                prefetchable: kind != BarKind::Io && original & 0x8 != 0,
            });
        }
        i += if kind == BarKind::Memory64 { 2 } else { 1 };
    }
    cfg.write32(a, COMMAND, command);
    bars
}

/// Every function on buses `start_bus..=end_bus`, in address order.
/// Functions 1-7 are only probed when function 0 says the device is
/// multi-function.
pub fn enumerate(cfg: &mut impl ConfigSpace, start_bus: u8, end_bus: u8) -> Vec<PciDevice> {
    let mut found = Vec::new();
    for bus in start_bus..=end_bus {
        for device in 0..32 {
            let f0 = PciAddress {
                bus,
                device,
                function: 0,
            };
            if cfg.read32(f0, 0x00) & 0xFFFF == 0xFFFF {
                continue;
            }
            let multi = (cfg.read32(f0, 0x0C) >> 16) as u8 & HEADER_MULTI_FUNCTION != 0;
            let functions = if multi { 8 } else { 1 };
            for function in 0..functions {
                if let Some(d) = probe(
                    cfg,
                    PciAddress {
                        bus,
                        device,
                        function,
                    },
                ) {
                    found.push(d);
                }
            }
        }
    }
    found
}

/// Turns on memory-space decoding and bus mastering (a driver needs both
/// before it touches a controller's registers or lets it do DMA).
pub fn enable_memory_and_bus_master(cfg: &mut impl ConfigSpace, a: PciAddress) {
    let command = cfg.read32(a, COMMAND) & 0xFFFF;
    cfg.write32(a, COMMAND, command | MEMORY_SPACE | BUS_MASTER);
}

/// What `wake_to_d0` did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wake {
    /// The power state the function was in (0 is D0, 3 is D3hot).
    pub from: u8,
    /// Leaving D3hot reset the function (PMCSR's No_Soft_Reset was 0, PCI
    /// PM 1.2 §5.4.1): its BARs and command register are cleared.
    pub reset: bool,
}

/// Puts the function into power state D0 through its PCI Power Management
/// capability (firmware may leave an unused controller in D3hot, where its
/// registers read as all ones). `None` without the capability. After a
/// change from D3hot the device needs 10 ms before it is used (PCI PM 1.2
/// §5.6.1), and if it was reset, its BARs back (`restore_bars`).
pub fn wake_to_d0(cfg: &mut impl ConfigSpace, a: PciAddress) -> Option<Wake> {
    if cfg.read32(a, COMMAND) & STATUS_CAPABILITIES == 0 {
        return None;
    }
    let mut ptr = (cfg.read32(a, CAPABILITIES_POINTER) & 0xFC) as u16;
    // Capabilities live in 0x40..0x100; a list longer than fits there
    // loops, so the walk is bounded.
    for _ in 0..48 {
        if ptr < 0x40 {
            return None;
        }
        let header = cfg.read32(a, ptr);
        if header as u8 == CAP_POWER_MANAGEMENT {
            let pmcsr = cfg.read32(a, ptr + 4);
            let from = (pmcsr & 0x3) as u8;
            if from != 0 {
                // Power state 0; bit 15 (PME status) is write-one-to-clear
                // and the upper half is read-only, so both are written as 0.
                cfg.write32(a, ptr + 4, pmcsr & 0x7FFC);
            }
            return Some(Wake {
                from,
                reset: from == 3 && pmcsr & NO_SOFT_RESET == 0,
            });
        }
        ptr = ((header >> 8) & 0xFC) as u16;
    }
    None
}

/// Writes the BAR addresses enumeration found back, after a wake that
/// reset the function. Only the address bits are writable, so the type
/// bits come back by themselves.
pub fn restore_bars(cfg: &mut impl ConfigSpace, a: PciAddress, bars: &[Bar]) {
    for b in bars {
        let off = BAR0 + 4 * b.index as u16;
        cfg.write32(a, off, b.address as u32);
        if b.kind == BarKind::Memory64 {
            cfg.write32(a, off + 4, (b.address >> 32) as u32);
        }
    }
}

/// A short name for common classes.
pub fn class_name(class: u8, subclass: u8, prog_if: u8) -> &'static str {
    match (class, subclass, prog_if) {
        (0x01, 0x06, _) => "SATA",
        (0x01, 0x08, _) => "NVMe",
        (0x02, _, _) => "network",
        (0x03, _, _) => "display",
        (0x04, _, _) => "multimedia",
        (0x05, _, _) => "memory",
        (0x06, 0x00, _) => "host bridge",
        (0x06, 0x01, _) => "ISA bridge",
        (0x06, 0x04, _) => "PCI bridge",
        (0x07, _, _) => "communication",
        (0x08, _, _) => "system peripheral",
        (0x0C, 0x03, 0x00) => "USB UHCI",
        (0x0C, 0x03, 0x10) => "USB OHCI",
        (0x0C, 0x03, 0x20) => "USB EHCI",
        (0x0C, 0x03, 0x30) => "USB xHCI",
        (0x0C, 0x03, _) => "USB",
        (0x0C, 0x05, _) => "SMBus",
        (0x0C, _, _) => "serial bus",
        (0x11, _, _) => "signal processing",
        _ => "other",
    }
}

/// Byte count with a binary unit: 32, 4K, 64K, 16M, 256M, 4G.
pub struct Size(pub u64);

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let n = self.0;
        match n {
            _ if n >= 1 << 30 && n.is_multiple_of(1 << 30) => write!(f, "{}G", n >> 30),
            _ if n >= 1 << 20 && n.is_multiple_of(1 << 20) => write!(f, "{}M", n >> 20),
            _ if n >= 1 << 10 && n.is_multiple_of(1 << 10) => write!(f, "{}K", n >> 10),
            _ => write!(f, "{n}"),
        }
    }
}

/// One line per device: address, IDs, class code, class name and BARs.
impl fmt::Display for PciDevice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} {:04x}:{:04x} {:02x}{:02x}{:02x} {}",
            self.address,
            self.vendor,
            self.device,
            self.class,
            self.subclass,
            self.prog_if,
            class_name(self.class, self.subclass, self.prog_if)
        )?;
        for b in &self.bars {
            let kind = match b.kind {
                BarKind::Memory32 => "mem32",
                BarKind::Memory64 => "mem64",
                BarKind::Io => "io",
            };
            write!(
                f,
                ", bar{} {kind} {:#x} {}",
                b.index,
                b.address,
                Size(b.size)
            )?;
            if b.prefetchable {
                write!(f, " pf")?;
            }
        }
        Ok(())
    }
}

/// The `[ ok ] pci` status line: device count, buses in use and the xHCI
/// controllers.
pub struct Summary<'a>(pub &'a [PciDevice]);

impl fmt::Display for Summary<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut buses: Vec<u8> = self.0.iter().map(|d| d.address.bus).collect();
        buses.dedup(); // devices are in address order
        let plural = |n: usize, one, many| if n == 1 { one } else { many };
        let n = self.0.len();
        write!(
            f,
            "{n} {} on {}",
            plural(n, "device", "devices"),
            plural(buses.len(), "bus", "buses")
        )?;
        for b in buses {
            write!(f, " {b:02x}")?;
        }
        let mut xhci = self.0.iter().filter(|d| d.is_xhci()).peekable();
        if xhci.peek().is_none() {
            return write!(f, ", no xHCI");
        }
        write!(f, ", xHCI at")?;
        for d in xhci {
            write!(f, " {}", d.address)?;
        }
        Ok(())
    }
}

/// Configuration space through the ECAM window, mapped uncached.
struct Ecam {
    /// Virtual address of bus 0's configuration space.
    base: u64,
}

impl ConfigSpace for Ecam {
    fn read32(&mut self, a: PciAddress, offset: u16) -> u32 {
        let p = (self.base + a.ecam_offset() + offset as u64) as *const u32;
        // SAFETY: inside the mapped ECAM window of an enumerated bus.
        unsafe { core::ptr::read_volatile(p) }
    }

    fn write32(&mut self, a: PciAddress, offset: u16, value: u32) {
        let p = (self.base + a.ecam_offset() + offset as u64) as *mut u32;
        unsafe { core::ptr::write_volatile(p, value) }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum PciError {
    NoEcam,
    Map(crate::mm::paging::MapError),
    NoDevices,
}

impl fmt::Display for PciError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PciError::NoEcam => write!(f, "no ECAM window (ACPI has no MCFG table)"),
            PciError::Map(e) => write!(f, "cannot map ECAM: {e}"),
            PciError::NoDevices => write!(f, "no devices found"),
        }
    }
}

static DEVICES: spin::Once<Vec<PciDevice>> = spin::Once::new();
/// The ECAM windows `init` mapped: (physical base of bus 0, first bus, last bus).
static WINDOWS: spin::Once<Vec<(u64, u8, u8)>> = spin::Once::new();

/// The devices found by `init` (empty before it, or if it failed).
pub fn devices() -> &'static [PciDevice] {
    DEVICES.get().map_or(&[], |d| d.as_slice())
}

/// Enumerates every bus of every ECAM window of PCI segment 0 (the only
/// segment a PC like the NUC has).
pub fn init(ecam: &[crate::acpi::tables::EcamRegion]) -> Result<&'static [PciDevice], PciError> {
    use crate::mm::{self, paging::Cache};
    let mut found = Vec::new();
    let mut mapped = Vec::new();
    // A window whose bus range is backwards is firmware garbage.
    let mut windows = ecam
        .iter()
        .filter(|r| r.segment == 0 && r.start_bus <= r.end_bus)
        .peekable();
    if windows.peek().is_none() {
        return Err(PciError::NoEcam);
    }
    for r in windows {
        let first = r.base.saturating_add((r.start_bus as u64) << 20);
        let len = (r.end_bus as u64 - r.start_bus as u64 + 1) << 20;
        mm::map_mmio(first, len, Cache::Uncached).map_err(PciError::Map)?;
        let mut cfg = Ecam {
            base: boot_info::PHYS_OFFSET + r.base,
        };
        found.extend(enumerate(&mut cfg, r.start_bus, r.end_bus));
        mapped.push((r.base, r.start_bus, r.end_bus));
    }
    WINDOWS.call_once(|| mapped);
    if found.is_empty() {
        return Err(PciError::NoDevices);
    }
    Ok(DEVICES.call_once(|| found))
}

/// Readies `d` for its driver: power state D0 (waiting 10 ms if it had to
/// change, and restoring its BARs if that reset it), then memory decoding
/// and bus mastering. Returns what the wake did, if the function has power
/// management.
pub fn enable_device(d: &PciDevice) -> Option<Wake> {
    let a = d.address;
    let (base, _, _) = *WINDOWS
        .get()?
        .iter()
        .find(|&&(_, start, end)| (start..=end).contains(&a.bus))?;
    let mut cfg = Ecam {
        base: boot_info::PHYS_OFFSET + base,
    };
    let wake = wake_to_d0(&mut cfg, a);
    if let Some(w) = wake
        && w.from != 0
    {
        crate::timer::sleep(core::time::Duration::from_millis(10));
        if w.reset {
            restore_bars(&mut cfg, a, &d.bars);
        }
    }
    enable_memory_and_bus_master(&mut cfg, a);
    wake
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// One function: its first 64 dwords of configuration space and, per
    /// BAR, the bits that can be written (0 = BAR not implemented).
    struct Func {
        regs: [u32; 64],
        bar_masks: [u32; 6],
    }

    #[derive(Default)]
    struct FakeConfig {
        funcs: BTreeMap<PciAddress, Func>,
        /// Set when a BAR was written while its device still decoded.
        decoding_during_sizing: bool,
        /// Every configuration write, in order.
        writes: Vec<(PciAddress, u16)>,
    }

    fn addr(bus: u8, device: u8, function: u8) -> PciAddress {
        PciAddress {
            bus,
            device,
            function,
        }
    }

    impl FakeConfig {
        /// Adds a function. `bars` are (index, value, size mask) triples.
        fn add(
            &mut self,
            a: PciAddress,
            id: u32,
            class: u32,
            header: u8,
            bars: &[(usize, u32, u32)],
        ) {
            let mut regs = [0u32; 64];
            regs[0] = id;
            regs[1] = 0x0290_0007; // status bits set; command: io, mem, bus master
            regs[2] = class;
            regs[3] = (header as u32) << 16;
            let mut bar_masks = [0; 6];
            for &(i, value, mask) in bars {
                regs[4 + i] = value;
                bar_masks[i] = mask;
            }
            self.funcs.insert(a, Func { regs, bar_masks });
        }
    }

    impl ConfigSpace for FakeConfig {
        fn read32(&mut self, a: PciAddress, offset: u16) -> u32 {
            self.funcs
                .get(&a)
                .map_or(0xFFFF_FFFF, |f| f.regs[offset as usize / 4])
        }

        fn write32(&mut self, a: PciAddress, offset: u16, value: u32) {
            self.writes.push((a, offset));
            let Some(f) = self.funcs.get_mut(&a) else {
                return;
            };
            let i = offset as usize / 4;
            if i == 1 {
                // Command, and a status register whose bits clear when a
                // one is written to them.
                f.regs[1] = (value & 0xFFFF) | (f.regs[1] & 0xFFFF_0000 & !(value & 0xFFFF_0000));
            } else if (4..10).contains(&i) {
                if f.regs[1] & 0x3 != 0 && value == 0xFFFF_FFFF {
                    self.decoding_during_sizing = true;
                }
                let mask = f.bar_masks[i - 4];
                let flags = if f.regs[i] & 1 == 1 { 0x3 } else { 0xF };
                // Read-only type bits stay; unimplemented BARs read zero.
                f.regs[i] = if mask == 0 {
                    0
                } else {
                    (value & mask) | (f.regs[i] & flags & !mask)
                };
            } else {
                // Power management control at 0x84 (see
                // `with_power_management`): leaving D3hot without
                // No_Soft_Reset resets the function, clearing its BARs'
                // address bits and its command register.
                if offset == 0x84
                    && f.regs[i] & 0x3 == 3
                    && value & 0x3 == 0
                    && f.regs[i] & NO_SOFT_RESET == 0
                {
                    for b in 0..6 {
                        let mask = f.bar_masks[b];
                        f.regs[4 + b] &= !mask;
                    }
                    f.regs[1] &= 0xFFFF_0000;
                }
                f.regs[i] = value;
            }
        }
    }

    /// Enough of the NUC's `lspci` to cover every case: multi-function
    /// devices, devices behind bridges on other buses, 64-bit BARs above
    /// 4 GiB, a prefetchable framebuffer BAR and an I/O BAR.
    fn nuc() -> FakeConfig {
        let mut c = FakeConfig::default();
        let mf = 0x80;
        c.add(addr(0, 0x00, 0), 0x4621_8086, 0x0600_0002, 0, &[]);
        c.add(
            addr(0, 0x02, 0),
            0x46a6_8086,
            0x0300_000c,
            0,
            &[
                (0, 0x3c00_0004, 0xFF00_0000), // 16M mem64
                (1, 0x0000_0060, 0xFFFF_FFFF),
                (2, 0x0000_000c, 0xF000_0000), // 256M mem64 prefetchable
                (3, 0x0000_0040, 0xFFFF_FFFF),
                (4, 0x0000_3001, 0xFFFF_FFC0), // 64 bytes of I/O
            ],
        );
        c.add(
            addr(0, 0x0d, 0),
            0x461e_8086,
            0x0c03_3002,
            mf,
            &[(0, 0x3d19_0004, 0xFFFF_0000), (1, 0x60, !0)],
        );
        c.add(
            addr(0, 0x0d, 2),
            0x463e_8086,
            0x0c03_4002,
            0,
            &[(0, 0x3d10_0004, 0xFFFC_0000), (1, 0x60, !0)],
        );
        c.add(
            addr(0, 0x0d, 3),
            0x466d_8086,
            0x0c03_4002,
            0,
            &[(0, 0x3d14_0004, 0xFFFC_0000), (1, 0x60, !0)],
        );
        c.add(
            addr(0, 0x14, 0),
            0x51ed_8086,
            0x0c03_3001,
            mf,
            &[(0, 0x3d18_0004, 0xFFFF_0000), (1, 0x60, !0)],
        );
        c.add(
            addr(0, 0x14, 2),
            0x51ef_8086,
            0x0500_0001,
            0,
            &[(0, 0x3d1a_4004, 0xFFFF_C000), (1, 0x60, !0)],
        );
        c.add(addr(0, 0x1d, 0), 0x51b0_8086, 0x0604_0001, 1, &[]);
        c.add(addr(0, 0x1f, 0), 0x5182_8086, 0x0601_0001, mf, &[]);
        c.add(
            addr(0, 0x1f, 4),
            0x51a3_8086,
            0x0c05_0001,
            0,
            &[
                (0, 0x3d1a_8004, 0xFFFF_FF00),
                (1, 0x60, !0),
                (4, 0xefa1, 0xFFFF_FFE0),
            ],
        );
        c.add(
            addr(1, 0x00, 0),
            0xa809_144d,
            0x0108_0200,
            0,
            &[(0, 0x8000_0004, 0xFFFF_C000), (1, 0, !0)],
        );
        c.add(
            addr(0x72, 0x00, 0),
            0x15f3_8086,
            0x0200_0003,
            0,
            &[(0, 0x8020_0000, 0xFFF0_0000), (3, 0x8030_0000, 0xFFFF_C000)],
        );
        // Function 1 of a single-function device: firmware ghosts like this
        // exist, and must not be listed.
        c.add(addr(0, 0x16, 0), 0x51e0_8086, 0x0780_0001, 0, &[]);
        c.add(addr(0, 0x16, 1), 0xdead_8086, 0x0780_0001, 0, &[]);
        c
    }

    #[test]
    fn finds_every_function_in_address_order() {
        let found = enumerate(&mut nuc(), 0, 255);
        let addrs: Vec<String> = found.iter().map(|d| d.address.to_string()).collect();
        assert_eq!(
            addrs,
            [
                "00:00.0", "00:02.0", "00:0d.0", "00:0d.2", "00:0d.3", "00:14.0", "00:14.2",
                "00:16.0", "00:1d.0", "00:1f.0", "00:1f.4", "01:00.0", "72:00.0"
            ]
        );
    }

    #[test]
    fn both_xhci_controllers_are_found_in_address_order() {
        let found = enumerate(&mut nuc(), 0, 255);
        let xhci: Vec<String> = found
            .iter()
            .filter(|d| d.is_xhci())
            .map(|d| d.address.to_string())
            .collect();
        assert_eq!(xhci, ["00:0d.0", "00:14.0"]);
    }

    #[test]
    fn bars_are_decoded_and_sized() {
        let found = enumerate(&mut nuc(), 0, 255);
        let dev = |a: &str| found.iter().find(|d| d.address.to_string() == a).unwrap();
        let bar = |index, kind, address, size, prefetchable| Bar {
            index,
            kind,
            address,
            size,
            prefetchable,
        };
        assert_eq!(
            dev("00:14.0").bars,
            [bar(0, BarKind::Memory64, 0x60_3d18_0000, 0x1_0000, false)]
        );
        assert_eq!(
            dev("00:02.0").bars,
            [
                bar(0, BarKind::Memory64, 0x60_3c00_0000, 16 << 20, false),
                bar(2, BarKind::Memory64, 0x40_0000_0000, 256 << 20, true),
                bar(4, BarKind::Io, 0x3000, 64, false),
            ]
        );
        assert_eq!(
            dev("00:1f.4").bars[1],
            bar(4, BarKind::Io, 0xefa0, 32, false)
        );
        assert_eq!(
            dev("72:00.0").bars,
            [
                bar(0, BarKind::Memory32, 0x8020_0000, 1 << 20, false),
                bar(3, BarKind::Memory32, 0x8030_0000, 16 << 10, false)
            ]
        );
        assert!(dev("00:00.0").bars.is_empty());
        assert_eq!(dev("00:1d.0").header_type, 1);
    }

    #[test]
    fn sizing_restores_bars_command_and_status_and_never_decodes_all_ones() {
        let mut c = nuc();
        let before: Vec<[u32; 64]> = c.funcs.values().map(|f| f.regs).collect();
        enumerate(&mut c, 0, 255);
        let after: Vec<[u32; 64]> = c.funcs.values().map(|f| f.regs).collect();
        assert_eq!(before, after);
        assert!(!c.decoding_during_sizing);
    }

    #[test]
    fn host_bridges_are_left_alone() {
        let mut c = FakeConfig::default();
        c.add(
            addr(0, 0, 0),
            0x4621_8086,
            0x0600_0002,
            0,
            &[(0, 0xfed1_0000, 0xFFFF_8000)],
        );
        let found = enumerate(&mut c, 0, 0);
        assert!(found[0].bars.is_empty());
        assert_eq!(
            c.writes,
            [],
            "neither the command register nor a BAR is written"
        );
    }

    #[test]
    fn only_the_given_bus_range_is_scanned() {
        let found = enumerate(&mut nuc(), 1, 0x71);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].address, addr(1, 0, 0));
    }

    #[test]
    fn memory_and_bus_master_can_be_enabled() {
        let mut c = nuc();
        let a = addr(0, 0x14, 0);
        c.funcs.get_mut(&a).unwrap().regs[1] = 0x0290_0000;
        enable_memory_and_bus_master(&mut c, a);
        assert_eq!(
            c.funcs[&a].regs[1], 0x0290_0006,
            "status bits are not cleared"
        );
    }

    /// Gives the function at `a` a capability list: an MSI capability,
    /// then power management in `state`.
    fn with_power_management(c: &mut FakeConfig, a: PciAddress, state: u32) {
        let f = c.funcs.get_mut(&a).unwrap();
        f.regs[0x34 / 4] = 0x70;
        f.regs[0x70 / 4] = 0x0080_8005; // MSI, next at 0x80
        f.regs[0x80 / 4] = 0xC803_0001; // power management, last
        f.regs[0x84 / 4] = 0x0000_8100 | state; // PME status and enable set
    }

    #[test]
    fn a_function_in_d3_is_woken_to_d0() {
        let mut c = nuc();
        let a = addr(0, 0x0d, 0);
        with_power_management(&mut c, a, 3);
        assert_eq!(
            wake_to_d0(&mut c, a),
            Some(Wake {
                from: 3,
                reset: true
            })
        );
        assert_eq!(
            c.funcs[&a].regs[0x84 / 4],
            0x0000_0100,
            "D0, PME enable kept, PME status left alone"
        );
        assert_eq!(
            wake_to_d0(&mut c, a),
            Some(Wake {
                from: 0,
                reset: false
            })
        );
        assert_eq!(c.writes.iter().filter(|w| w.1 == 0x84).count(), 1);
    }

    #[test]
    fn a_function_reset_by_its_wake_gets_its_bars_back() {
        let mut c = nuc();
        let a = addr(0, 0x0d, 0);
        let bars = enumerate(&mut c, 0, 0)
            .into_iter()
            .find(|d| d.address == a)
            .unwrap()
            .bars;
        let before = (c.funcs[&a].regs[4], c.funcs[&a].regs[5]);
        with_power_management(&mut c, a, 3);
        assert!(wake_to_d0(&mut c, a).unwrap().reset);
        assert_eq!(c.funcs[&a].regs[4] & !0xF, 0, "the wake cleared BAR0");
        restore_bars(&mut c, a, &bars);
        assert_eq!((c.funcs[&a].regs[4], c.funcs[&a].regs[5]), before);
        // With No_Soft_Reset the function keeps its configuration.
        let mut c = nuc();
        with_power_management(&mut c, a, 3 | NO_SOFT_RESET);
        assert_eq!(
            wake_to_d0(&mut c, a),
            Some(Wake {
                from: 3,
                reset: false
            })
        );
        assert_eq!((c.funcs[&a].regs[4], c.funcs[&a].regs[5]), before);
    }

    #[test]
    fn functions_without_power_management_are_left_alone() {
        let mut c = nuc();
        let a = addr(0, 0x14, 0);
        assert_eq!(wake_to_d0(&mut c, a), None);
        // A capability list that loops ends too.
        let f = c.funcs.get_mut(&a).unwrap();
        f.regs[0x34 / 4] = 0x40;
        f.regs[0x40 / 4] = 0x0000_4005;
        assert_eq!(wake_to_d0(&mut c, a), None);
        // No capability list at all.
        c.funcs.get_mut(&a).unwrap().regs[1] = 0x0280_0007;
        with_power_management(&mut c, a, 3);
        c.funcs.get_mut(&a).unwrap().regs[1] = 0x0280_0007;
        assert_eq!(wake_to_d0(&mut c, a), None);
        assert!(c.writes.is_empty());
    }

    #[test]
    fn summary_line() {
        let found = enumerate(&mut nuc(), 0, 255);
        assert_eq!(
            Summary(&found).to_string(),
            "13 devices on buses 00 01 72, xHCI at 00:0d.0 00:14.0"
        );
        let host_bridge_only = &found[..1];
        assert_eq!(
            Summary(host_bridge_only).to_string(),
            "1 device on bus 00, no xHCI"
        );
    }

    #[test]
    fn device_lines() {
        let found = enumerate(&mut nuc(), 0, 255);
        assert_eq!(
            found[5].to_string(),
            "00:14.0 8086:51ed 0c0330 USB xHCI, bar0 mem64 0x603d180000 64K"
        );
        assert_eq!(
            found[1].to_string(),
            "00:02.0 8086:46a6 030000 display, bar0 mem64 0x603c000000 16M, bar2 mem64 0x4000000000 256M pf, bar4 io 0x3000 64"
        );
        assert_eq!(
            PciAddress {
                bus: 0x72,
                device: 0x1f,
                function: 7
            }
            .ecam_offset(),
            0x72 << 20 | 0x1f << 15 | 7 << 12
        );
    }
}
