//! The line the startup screen shows for each device set up (spec §10:
//! the boot keeps going when a device fails, and says so): what was found
//! on it, keyboards and disks, or why a driver did not start. Also the
//! names the block layer gets for disks.

use crate::{Speed, UsbError};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// What happened when a device was set up, for the startup screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attached {
    pub port: u8,
    pub outcome: Result<Found, UsbError>,
}

/// Names one disk on one host; never reused, so a request for a stick that
/// was unplugged (and maybe plugged in again) fails instead of reaching
/// another device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiskId(pub u32);

/// A disk in use, for the block layer and the startup screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiskInfo {
    pub id: DiskId,
    pub port: u8,
    pub vendor: String,
    pub product: String,
    pub block_size: usize,
    pub block_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub vendor: u16,
    pub product: u16,
    pub speed: Speed,
    /// Boot-keyboard interfaces now in use.
    pub keyboards: usize,
    /// Why a boot-keyboard interface could not be started, if one could
    /// not: the screen must say so, because `dmesg` needs a keyboard.
    pub not_started: Option<UsbError>,
    /// Mass-storage interfaces now in use: `disk_info` has one entry for
    /// each.
    pub disks: usize,
    pub disk_info: Vec<DiskInfo>,
    /// Why a mass-storage interface could not be started, if one could not.
    pub disk_not_started: Option<UsbError>,
}

/// The boot line's parts after the speed, joined with ", ".
struct Parts<'a, 'b> {
    f: &'a mut fmt::Formatter<'b>,
    any: bool,
}

impl Parts<'_, '_> {
    fn add(&mut self, args: fmt::Arguments) -> fmt::Result {
        let sep = if self.any { ", " } else { "" };
        self.any = true;
        write!(self.f, "{sep}{args}")
    }
}

/// A disk's size: whole MiB under 1 GiB, else GiB with one decimal, both
/// rounded down.
struct Size(u64);

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        const MIB: u64 = 1 << 20;
        const GIB: u64 = 1 << 30;
        if self.0 < GIB {
            write!(f, "{} MiB", self.0 / MIB)
        } else {
            let tenths = self.0.saturating_mul(10) / GIB;
            write!(f, "{}.{} GiB", tenths / 10, tenths % 10)
        }
    }
}

impl fmt::Display for DiskInfo {
    /// "disk Kingston DataTraveler 3.0, 14.4 GiB".
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("disk")?;
        for name in [&self.vendor, &self.product] {
            if !name.is_empty() {
                write!(f, " {name}")?;
            }
        }
        let bytes = self.block_count.saturating_mul(self.block_size as u64);
        write!(f, ", {}", Size(bytes))
    }
}

impl fmt::Display for Attached {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "port {}: ", self.port)?;
        let d = match &self.outcome {
            Err(e) => return write!(f, "setup failed: {e}"),
            Ok(d) => d,
        };
        write!(f, "{:04x}:{:04x} {}, ", d.vendor, d.product, d.speed)?;
        let mut parts = Parts { f, any: false };
        match (d.keyboards, d.not_started) {
            (0, None) => {}
            (0, Some(e)) => parts.add(format_args!("keyboard not started: {e}"))?,
            (1, None) => parts.add(format_args!("keyboard"))?,
            (n, None) => parts.add(format_args!("{n} keyboards"))?,
            (n, Some(e)) => parts.add(format_args!("{n} keyboards, another not started: {e}"))?,
        }
        for disk in &d.disk_info {
            parts.add(format_args!("{disk}"))?;
        }
        match (d.disks, d.disk_not_started) {
            (_, None) => {}
            (0, Some(e)) => parts.add(format_args!("disk not started: {e}"))?,
            (_, Some(e)) => parts.add(format_args!("another disk not started: {e}"))?,
        }
        if !parts.any {
            parts.add(format_args!("not claimed"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    fn found() -> Found {
        Found {
            vendor: 0x0951,
            product: 0x1666,
            speed: Speed::Super,
            keyboards: 0,
            not_started: None,
            disks: 0,
            disk_info: Vec::new(),
            disk_not_started: None,
        }
    }

    fn disk(vendor: &str, product: &str, block_size: usize, block_count: u64) -> DiskInfo {
        DiskInfo {
            id: DiskId(0),
            port: 1,
            vendor: vendor.into(),
            product: product.into(),
            block_size,
            block_count,
        }
    }

    fn line(found: Found) -> String {
        Attached {
            port: 1,
            outcome: Ok(found),
        }
        .to_string()
    }

    #[test]
    fn boot_lines_name_what_was_found() {
        assert_eq!(line(found()), "port 1: 0951:1666 SuperSpeed, not claimed");
        let one = |f: Found| {
            line(f)
                .trim_start_matches("port 1: 0951:1666 SuperSpeed, ")
                .to_string()
        };
        let kingston = disk("Kingston", "DataTraveler 3.0", 512, 30_277_632);
        let with_disk = Found {
            disks: 1,
            disk_info: vec![kingston.clone()],
            ..found()
        };
        assert_eq!(
            one(with_disk.clone()),
            "disk Kingston DataTraveler 3.0, 14.4 GiB"
        );
        let gone = Some(UsbError::Timeout);
        assert_eq!(
            one(Found {
                disk_not_started: gone,
                ..found()
            }),
            "disk not started: timed out"
        );
        assert_eq!(
            one(Found {
                disk_not_started: gone,
                ..with_disk.clone()
            }),
            "disk Kingston DataTraveler 3.0, 14.4 GiB, another disk not started: timed out"
        );
        assert_eq!(
            one(Found {
                keyboards: 2,
                not_started: gone,
                ..with_disk.clone()
            }),
            "2 keyboards, another not started: timed out, disk Kingston DataTraveler 3.0, 14.4 GiB"
        );
        assert_eq!(
            one(Found {
                not_started: gone,
                disk_not_started: Some(UsbError::Stall),
                ..found()
            }),
            "keyboard not started: timed out, disk not started: stalled"
        );
        // A disk without names.
        assert_eq!(
            one(Found {
                disks: 1,
                disk_info: vec![disk("", "", 512, 2048)],
                ..found()
            }),
            "disk, 1 MiB"
        );
        assert_eq!(
            one(Found {
                disks: 1,
                disk_info: vec![disk("", "Stick", 512, 2048)],
                ..found()
            }),
            "disk Stick, 1 MiB"
        );
    }

    #[test]
    fn disk_sizes_are_whole_mib_then_gib_with_one_decimal_rounded_down() {
        let size = |block_size, blocks| {
            let f = Found {
                disks: 1,
                disk_info: vec![disk("V", "P", block_size, blocks)],
                ..found()
            };
            line(f).rsplit(", ").next().unwrap().to_string()
        };
        assert_eq!(size(512, 524_288), "256 MiB");
        assert_eq!(size(512, 2047), "0 MiB");
        assert_eq!(size(512, 2 * 1024 * 1024 - 1), "1023 MiB");
        assert_eq!(size(512, 2 * 1024 * 1024), "1.0 GiB");
        assert_eq!(size(4096, 262_144 + 26_214), "1.0 GiB");
        assert_eq!(size(4096, 262_144 + 26_215), "1.1 GiB");
        assert_eq!(size(512, 30_277_632), "14.4 GiB");
        // 512 bytes under 200 GiB (integer arithmetic, no rounding up).
        assert_eq!(size(512, 419_430_399), "199.9 GiB");
        // The largest disk: 2^32 blocks of 4096 bytes, 16 TiB.
        assert_eq!(size(4096, 1 << 32), "16384.0 GiB");
    }
}
