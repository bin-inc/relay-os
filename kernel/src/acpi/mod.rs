//! ACPI (spec §5.4): tables only, no AML interpreter. `init` reads what the
//! rest of the kernel needs once: the ECAM windows (PCI), the HPET (timer
//! calibration), the FADT (reset, power-off, RTC century) and `\_S5`.

pub mod aml;
pub mod tables;

use crate::klogln;
use crate::mm::{self, paging::Cache};
use alloc::vec::Vec;
use aml::SleepType;
use core::fmt;
use spin::Once;
use tables::{AcpiError, EcamRegion, Fadt, PhysRead, TableList};

pub struct Acpi {
    pub tables: TableList,
    pub ecam: Vec<EcamRegion>,
    pub hpet: Option<u64>,
    pub fadt: Option<Fadt>,
    pub s5: Option<SleepType>,
}

static ACPI: Once<Acpi> = Once::new();

/// The tables read by `init`, if it succeeded.
pub fn get() -> Option<&'static Acpi> {
    ACPI.get()
}

/// Firmware tables through `map_mmio` (write-back): they may lie in
/// reserved memory, which the linear map does not cover.
struct Mapped;

impl PhysRead for Mapped {
    fn read(&mut self, phys: u64, len: usize) -> Option<&[u8]> {
        let p = mm::map_mmio(phys, len as u64, Cache::WriteBack).ok()?;
        // SAFETY: just mapped; firmware tables stay put.
        Some(unsafe { core::slice::from_raw_parts(p, len) })
    }
}

/// A table the kernel can do without: a missing or broken one is logged
/// and skipped.
fn optional<'m>(mem: &'m mut Mapped, list: &TableList, sig: &[u8; 4]) -> Option<&'m [u8]> {
    let phys = list.find(sig)?;
    match tables::read_table_as(mem, phys, sig) {
        Ok(t) => Some(t),
        Err(e) => {
            klogln!("acpi: ignoring table at {phys:#x}: {e}");
            None
        }
    }
}

pub fn init(rsdp: u64) -> Result<&'static Acpi, AcpiError> {
    let mut mem = Mapped;
    let list = TableList::load(&mut mem, rsdp)?;
    let ecam = optional(&mut mem, &list, b"MCFG")
        .map(tables::parse_mcfg)
        .unwrap_or_default();
    let hpet = optional(&mut mem, &list, b"HPET").and_then(tables::parse_hpet);
    let fadt = optional(&mut mem, &list, b"FACP").map(tables::parse_fadt);
    let s5 = fadt.and_then(|f| match tables::read_table_as(&mut mem, f.dsdt, b"DSDT") {
        Ok(dsdt) => aml::find_s5(dsdt),
        Err(e) => {
            klogln!("acpi: ignoring DSDT at {:#x}: {e}", f.dsdt);
            None
        }
    });
    let acpi = ACPI.call_once(|| Acpi {
        tables: list,
        ecam,
        hpet,
        fadt,
        s5,
    });
    acpi.log_details();
    Ok(acpi)
}

impl Acpi {
    fn log_details(&self) {
        let sigs: Vec<&str> = self
            .tables
            .entries
            .iter()
            .map(|(sig, _)| core::str::from_utf8(sig).unwrap_or("????"))
            .collect();
        klogln!("acpi: tables {}", sigs.join(" "));
        if let Some(f) = &self.fadt {
            let none = || alloc::string::String::from("none");
            klogln!(
                "acpi: FADT reset {}, PM1a_CNT {}, PM1b_CNT {}, century register {:#x}",
                f.reset
                    .map_or_else(none, |(g, v)| alloc::format!("{g} <- {v:#x}")),
                f.pm1a_cnt.map_or_else(none, |g| alloc::format!("{g}")),
                f.pm1b_cnt.map_or_else(none, |g| alloc::format!("{g}")),
                f.century
            );
        }
    }
}

/// The one-line summary for the `[ ok ] acpi` status line.
impl fmt::Display for Acpi {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} tables", self.tables.entries.len())?;
        match self.ecam.first() {
            Some(e) => write!(
                f,
                ", ECAM {:#x} buses {}-{}",
                e.base, e.start_bus, e.end_bus
            )?,
            None => write!(f, ", no ECAM")?,
        }
        match self.hpet {
            Some(h) => write!(f, ", HPET {h:#x}")?,
            None => write!(f, ", no HPET")?,
        }
        match self.s5 {
            Some(s) => write!(f, ", S5 {}/{}", s.a, s.b),
            None => write!(f, ", no S5"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_names_what_was_found_and_what_is_missing() {
        let mut a = Acpi {
            tables: TableList {
                entries: alloc::vec![(*b"FACP", 0x1000), (*b"MCFG", 0x2000)],
            },
            ecam: alloc::vec![EcamRegion {
                base: 0xC000_0000,
                segment: 0,
                start_bus: 0,
                end_bus: 255
            }],
            hpet: Some(0xFED0_0000),
            fadt: None,
            s5: Some(SleepType { a: 7, b: 0 }),
        };
        assert_eq!(
            a.to_string(),
            "2 tables, ECAM 0xc0000000 buses 0-255, HPET 0xfed00000, S5 7/0"
        );
        a.ecam.clear();
        a.hpet = None;
        a.s5 = None;
        assert_eq!(a.to_string(), "2 tables, no ECAM, no HPET, no S5");
    }
}
