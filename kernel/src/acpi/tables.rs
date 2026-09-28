//! ACPI table parsing (spec §5.4): RSDP → XSDT, MCFG, HPET and FADT. No AML
//! interpreter. Physical memory is read through `PhysRead`, so the parsers
//! are tested on the host with real tables from QEMU and the NUC.

use alloc::vec::Vec;
use core::fmt;

pub const HEADER_LEN: usize = 36;

/// Read access to physical memory.
pub trait PhysRead {
    /// The `len` bytes at `phys`, or `None` if they cannot be read.
    fn read(&mut self, phys: u64, len: usize) -> Option<&[u8]>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcpiError {
    /// The firmware passed no RSDP address.
    NoRsdp,
    BadRsdp,
    /// The RSDP has no XSDT address (ACPI 1.0 only).
    NoXsdt,
    Unreadable(u64),
    /// A table's header says it is shorter than the header itself.
    TooShort([u8; 4]),
    /// A table's header says it is longer than `MAX_TABLE`.
    TooLong([u8; 4], usize),
    BadChecksum([u8; 4]),
    /// Expected one signature, found another.
    WrongTable {
        expected: [u8; 4],
        found: [u8; 4],
    },
}

fn sig(s: &[u8; 4]) -> &str {
    core::str::from_utf8(s).unwrap_or("????")
}

impl fmt::Display for AcpiError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AcpiError::NoRsdp => write!(f, "firmware passed no RSDP"),
            AcpiError::BadRsdp => write!(f, "RSDP signature or checksum is wrong"),
            AcpiError::NoXsdt => write!(f, "no XSDT (ACPI 1.0 firmware)"),
            AcpiError::Unreadable(p) => write!(f, "cannot read table at {p:#x}"),
            AcpiError::TooShort(s) => write!(f, "{} is too short", sig(s)),
            AcpiError::TooLong(s, len) => write!(f, "{} claims {len} bytes", sig(s)),
            AcpiError::BadChecksum(s) => write!(f, "{} checksum is wrong", sig(s)),
            AcpiError::WrongTable { expected, found } => {
                write!(f, "expected {}, found {}", sig(expected), sig(found))
            }
        }
    }
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

pub fn checksum_ok(b: &[u8]) -> bool {
    b.iter().fold(0u8, |s, &x| s.wrapping_add(x)) == 0
}

/// The longest table read: 16 MiB, far above any real one (the NUC's
/// DSDT is 469,477 bytes), so a corrupt length is never mapped or summed.
pub const MAX_TABLE: usize = 16 << 20;

/// Reads the table at `phys` and checks its length and checksum.
pub fn read_table(mem: &mut impl PhysRead, phys: u64) -> Result<&[u8], AcpiError> {
    let header = mem
        .read(phys, HEADER_LEN)
        .ok_or(AcpiError::Unreadable(phys))?;
    let signature: [u8; 4] = header[..4].try_into().unwrap();
    let len = u32_at(header, 4) as usize;
    if len < HEADER_LEN {
        return Err(AcpiError::TooShort(signature));
    }
    if len > MAX_TABLE {
        return Err(AcpiError::TooLong(signature, len));
    }
    let table = mem.read(phys, len).ok_or(AcpiError::Unreadable(phys))?;
    if !checksum_ok(table) {
        return Err(AcpiError::BadChecksum(signature));
    }
    Ok(table)
}

/// `read_table`, and the signature must be `expected`.
pub fn read_table_as<'m>(
    mem: &'m mut impl PhysRead,
    phys: u64,
    expected: &[u8; 4],
) -> Result<&'m [u8], AcpiError> {
    let t = read_table(mem, phys)?;
    let found: [u8; 4] = t[..4].try_into().unwrap();
    if &found != expected {
        return Err(AcpiError::WrongTable {
            expected: *expected,
            found,
        });
    }
    Ok(t)
}

/// The tables the firmware lists in its XSDT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableList {
    pub entries: Vec<([u8; 4], u64)>,
}

impl TableList {
    /// Follows the RSDP (ACPI 2.0 or later) to the XSDT and reads the
    /// signature of every table it lists. Individual tables are checked
    /// when they are read; an entry that cannot be read at all is left out,
    /// so one bad pointer does not cost the other tables.
    pub fn load(mem: &mut impl PhysRead, rsdp_phys: u64) -> Result<TableList, AcpiError> {
        if rsdp_phys == 0 {
            return Err(AcpiError::NoRsdp);
        }
        let rsdp = mem
            .read(rsdp_phys, 36)
            .ok_or(AcpiError::Unreadable(rsdp_phys))?;
        if &rsdp[..8] != b"RSD PTR " || !checksum_ok(&rsdp[..20]) {
            return Err(AcpiError::BadRsdp);
        }
        if rsdp[15] < 2 {
            return Err(AcpiError::NoXsdt);
        }
        if !checksum_ok(&rsdp[..36]) {
            return Err(AcpiError::BadRsdp);
        }
        let xsdt_phys = u64_at(rsdp, 24);
        if xsdt_phys == 0 {
            return Err(AcpiError::NoXsdt);
        }
        let xsdt = read_table_as(mem, xsdt_phys, b"XSDT")?;
        let addrs: Vec<u64> = xsdt[HEADER_LEN..]
            .as_chunks::<8>()
            .0
            .iter()
            .map(|c| u64::from_le_bytes(*c))
            .collect();
        let mut entries = Vec::with_capacity(addrs.len());
        for phys in addrs {
            if let Some(header) = mem.read(phys, 4) {
                entries.push((header.try_into().unwrap(), phys));
            }
        }
        Ok(TableList { entries })
    }

    /// Physical address of the first table with this signature.
    pub fn find(&self, signature: &[u8; 4]) -> Option<u64> {
        self.entries
            .iter()
            .find(|(s, _)| s == signature)
            .map(|&(_, p)| p)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressSpace {
    Memory,
    Io,
    Other(u8),
}

/// An ACPI Generic Address Structure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GenericAddress {
    pub space: AddressSpace,
    pub bit_width: u8,
    pub bit_offset: u8,
    pub access_size: u8,
    pub address: u64,
}

impl fmt::Display for GenericAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.space {
            AddressSpace::Memory => write!(f, "mem {:#x}", self.address),
            AddressSpace::Io => write!(f, "io {:#x}", self.address),
            AddressSpace::Other(n) => write!(f, "space {n} {:#x}", self.address),
        }
    }
}

impl GenericAddress {
    fn parse(b: &[u8]) -> GenericAddress {
        GenericAddress {
            space: match b[0] {
                0 => AddressSpace::Memory,
                1 => AddressSpace::Io,
                n => AddressSpace::Other(n),
            },
            bit_width: b[1],
            bit_offset: b[2],
            access_size: b[3],
            address: u64_at(b, 4),
        }
    }

    fn io(port: u32, bytes: u8) -> GenericAddress {
        GenericAddress {
            space: AddressSpace::Io,
            bit_width: bytes.saturating_mul(8),
            bit_offset: 0,
            access_size: 0,
            address: port as u64,
        }
    }
}

/// One MCFG entry: the ECAM window of a PCI segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EcamRegion {
    /// Physical address of bus 0's configuration space (even if
    /// `start_bus` is higher).
    pub base: u64,
    pub segment: u16,
    pub start_bus: u8,
    pub end_bus: u8,
}

/// The ECAM regions of a validated MCFG table.
pub fn parse_mcfg(t: &[u8]) -> Vec<EcamRegion> {
    t.get(44..)
        .unwrap_or(&[])
        .as_chunks::<16>()
        .0
        .iter()
        .map(|e| EcamRegion {
            base: u64_at(e, 0),
            segment: u16_at(e, 8),
            start_bus: e[10],
            end_bus: e[11],
        })
        .collect()
}

/// The HPET's register base from a validated HPET table, if it is in
/// memory space.
pub fn parse_hpet(t: &[u8]) -> Option<u64> {
    let gas = GenericAddress::parse(t.get(40..52)?);
    (gas.space == AddressSpace::Memory && gas.address != 0).then_some(gas.address)
}

/// What the kernel needs from the FADT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fadt {
    pub dsdt: u64,
    pub pm1a_cnt: Option<GenericAddress>,
    pub pm1b_cnt: Option<GenericAddress>,
    /// The reset register and the value to write, if the firmware
    /// supports it (flag RESET_REG_SUP).
    pub reset: Option<(GenericAddress, u8)>,
    /// CMOS index of the RTC century register, or 0 if there is none.
    pub century: u8,
}

/// Parses a validated FADT of any revision. The 64-bit `X_` fields win
/// over the old 32-bit ones when the table is long enough to have them and
/// they are set.
pub fn parse_fadt(t: &[u8]) -> Fadt {
    let len = t.len();
    let dsdt = match len >= 148 && u64_at(t, 140) != 0 {
        true => u64_at(t, 140),
        false => t.get(40..44).map_or(0, |_| u32_at(t, 40) as u64),
    };
    let cnt_len = if len > 89 { t[89] } else { 2 };
    let pm1 = |x_off: usize, old_off: usize| {
        if len >= x_off + 12 {
            let gas = GenericAddress::parse(&t[x_off..x_off + 12]);
            if gas.address != 0 {
                return Some(gas);
            }
        }
        let port = if len >= old_off + 4 {
            u32_at(t, old_off)
        } else {
            0
        };
        (port != 0).then(|| GenericAddress::io(port, cnt_len))
    };
    let flags = if len >= 116 { u32_at(t, 112) } else { 0 };
    let reset = (len >= 129 && flags & (1 << 10) != 0)
        .then(|| (GenericAddress::parse(&t[116..128]), t[128]))
        .filter(|(gas, _)| gas.address != 0);
    Fadt {
        dsdt,
        pm1a_cnt: pm1(172, 64),
        pm1b_cnt: pm1(184, 68),
        reset,
        century: if len > 108 { t[108] } else { 0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Physical memory holding a few tables at their real addresses.
    #[derive(Default)]
    struct FakeMem(BTreeMap<u64, Vec<u8>>);

    impl FakeMem {
        fn put(&mut self, phys: u64, bytes: &[u8]) {
            self.0.insert(phys, bytes.to_vec());
        }
    }

    impl PhysRead for FakeMem {
        fn read(&mut self, phys: u64, len: usize) -> Option<&[u8]> {
            let (&start, bytes) = self.0.range(..=phys).next_back()?;
            let off = (phys - start) as usize;
            bytes.get(off..off + len)
        }
    }

    macro_rules! fixture {
        ($dir:literal, $name:literal) => {
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/fixtures/acpi/",
                $dir,
                "/",
                $name,
                ".bin"
            ))
        };
    }

    const QEMU_RSDP: u64 = 0x3fb7e014;

    fn qemu() -> FakeMem {
        let mut m = FakeMem::default();
        m.put(QEMU_RSDP, fixture!("qemu", "RSDP"));
        m.put(0x3fb7d0e8, fixture!("qemu", "XSDT"));
        m.put(0x3fb79000, fixture!("qemu", "FACP"));
        m.put(0x3fb7a000, fixture!("qemu", "DSDT"));
        m.put(0x3fb78000, fixture!("qemu", "APIC"));
        m.put(0x3fb77000, fixture!("qemu", "HPET"));
        m.put(0x3fb76000, fixture!("qemu", "MCFG"));
        m.put(0x3fb75000, fixture!("qemu", "WAET"));
        m.put(0x3fb74000, fixture!("qemu", "BGRT"));
        m
    }

    fn with_checksum(mut t: Vec<u8>, at: usize) -> Vec<u8> {
        t[at] = 0;
        let sum = t.iter().fold(0u8, |s, &x| s.wrapping_add(x));
        t[at] = sum.wrapping_neg();
        t
    }

    /// The NUC's tables behind an RSDP and XSDT built here (Linux does not
    /// expose the firmware's).
    fn nuc() -> (FakeMem, u64) {
        let mut m = FakeMem::default();
        m.put(0x56d45000, fixture!("nuc", "FACP"));
        m.put(0x56d46000, fixture!("nuc", "HPET"));
        m.put(0x56cb4000, fixture!("nuc", "MCFG"));
        let mut xsdt = b"XSDT".to_vec();
        xsdt.extend_from_slice(&(36u32 + 24).to_le_bytes());
        xsdt.extend_from_slice(&[1, 0]); // revision, checksum
        xsdt.extend_from_slice(&[0; 26]);
        for p in [0x56d45000u64, 0x56d46000, 0x56cb4000] {
            xsdt.extend_from_slice(&p.to_le_bytes());
        }
        m.put(0x56de3728, &with_checksum(xsdt, 9));
        let mut rsdp = b"RSD PTR \0INTEL \x02".to_vec();
        rsdp.extend_from_slice(&[0; 4]); // RSDT
        rsdp.extend_from_slice(&36u32.to_le_bytes());
        rsdp.extend_from_slice(&0x56de3728u64.to_le_bytes());
        rsdp.extend_from_slice(&[0; 4]);
        // The first checksum covers 20 bytes, the extended one all 36.
        let mut rsdp = [with_checksum(rsdp[..20].to_vec(), 8), rsdp[20..].to_vec()].concat();
        rsdp = with_checksum(rsdp, 32);
        m.put(0x56e47014, &rsdp);
        (m, 0x56e47014)
    }

    #[test]
    fn qemu_xsdt_lists_its_tables() {
        let t = TableList::load(&mut qemu(), QEMU_RSDP).unwrap();
        let sigs: Vec<&[u8; 4]> = t.entries.iter().map(|(s, _)| s).collect();
        assert_eq!(sigs, [b"FACP", b"APIC", b"HPET", b"MCFG", b"WAET", b"BGRT"]);
        assert_eq!(t.find(b"MCFG"), Some(0x3fb76000));
        assert_eq!(t.find(b"SSDT"), None);
    }

    #[test]
    fn qemu_tables_decode() {
        let mut m = qemu();
        let t = TableList::load(&mut m, QEMU_RSDP).unwrap();
        let mcfg = read_table_as(&mut m, t.find(b"MCFG").unwrap(), b"MCFG").unwrap();
        assert_eq!(
            parse_mcfg(mcfg),
            [EcamRegion {
                base: 0xE000_0000,
                segment: 0,
                start_bus: 0,
                end_bus: 255
            }]
        );
        let hpet = read_table_as(&mut m, t.find(b"HPET").unwrap(), b"HPET").unwrap();
        assert_eq!(parse_hpet(hpet), Some(0xFED0_0000));
        let fadt = parse_fadt(read_table_as(&mut m, t.find(b"FACP").unwrap(), b"FACP").unwrap());
        assert_eq!(fadt.dsdt, 0x3fb7a000);
        assert_eq!(
            fadt.pm1a_cnt.map(|g| (g.space, g.address, g.bit_width)),
            Some((AddressSpace::Io, 0x604, 16))
        );
        assert_eq!(fadt.pm1b_cnt, None);
        let (reset, value) = fadt.reset.unwrap();
        assert_eq!(
            (reset.space, reset.address, value),
            (AddressSpace::Io, 0xCF9, 0x0F)
        );
        assert_eq!(fadt.century, 0x32);
    }

    #[test]
    fn nuc_tables_decode() {
        let (mut m, rsdp) = nuc();
        let t = TableList::load(&mut m, rsdp).unwrap();
        let mcfg = read_table_as(&mut m, t.find(b"MCFG").unwrap(), b"MCFG").unwrap();
        assert_eq!(
            parse_mcfg(mcfg),
            [EcamRegion {
                base: 0xC000_0000,
                segment: 0,
                start_bus: 0,
                end_bus: 255
            }]
        );
        let hpet = read_table_as(&mut m, t.find(b"HPET").unwrap(), b"HPET").unwrap();
        assert_eq!(parse_hpet(hpet), Some(0xFED0_0000));
        let fadt = parse_fadt(read_table_as(&mut m, t.find(b"FACP").unwrap(), b"FACP").unwrap());
        assert_eq!(fadt.dsdt, 0x56cd2000);
        assert_eq!(
            fadt.pm1a_cnt.map(|g| (g.space, g.address)),
            Some((AddressSpace::Io, 0x1804))
        );
        assert_eq!(fadt.pm1b_cnt, None, "X_PM1b_CNT_BLK is present but zero");
        let (reset, value) = fadt.reset.unwrap();
        assert_eq!(
            (reset.space, reset.address, reset.bit_width, value),
            (AddressSpace::Io, 0xCF9, 8, 6)
        );
        assert_eq!(fadt.century, 0x32);
    }

    #[test]
    fn generic_addresses_print_their_space() {
        let io = GenericAddress::io(0xCF9, 1);
        assert_eq!(io.to_string(), "io 0xcf9");
        let mem = GenericAddress {
            space: AddressSpace::Memory,
            ..io
        };
        assert_eq!(mem.to_string(), "mem 0xcf9");
        let other = GenericAddress {
            space: AddressSpace::Other(3),
            ..io
        };
        assert_eq!(other.to_string(), "space 3 0xcf9");
    }

    #[test]
    fn an_acpi_1_fadt_uses_the_32_bit_fields() {
        // Revision 1: 116 bytes, no X_ fields, no reset register.
        let mut t = fixture!("qemu", "FACP")[..116].to_vec();
        t[4..8].copy_from_slice(&116u32.to_le_bytes());
        let fadt = parse_fadt(&with_checksum(t, 9));
        assert_eq!(fadt.dsdt, 0x3fb7a000);
        assert_eq!(fadt.pm1a_cnt.map(|g| g.address), Some(0x604));
        assert_eq!(fadt.reset, None);
    }

    #[test]
    fn bad_checksums_and_signatures_are_errors() {
        let mut m = qemu();
        m.0.get_mut(&0x3fb76000).unwrap()[50] ^= 1; // MCFG body
        assert_eq!(
            read_table(&mut m, 0x3fb76000),
            Err(AcpiError::BadChecksum(*b"MCFG"))
        );
        assert_eq!(
            read_table_as(&mut m, 0x3fb77000, b"MCFG"),
            Err(AcpiError::WrongTable {
                expected: *b"MCFG",
                found: *b"HPET"
            })
        );
        assert_eq!(
            read_table(&mut m, 0x1000),
            Err(AcpiError::Unreadable(0x1000))
        );

        let mut m = qemu();
        m.0.get_mut(&QEMU_RSDP).unwrap()[10] ^= 1; // OEM ID
        assert_eq!(TableList::load(&mut m, QEMU_RSDP), Err(AcpiError::BadRsdp));
        assert_eq!(TableList::load(&mut qemu(), 0), Err(AcpiError::NoRsdp));
    }

    #[test]
    fn acpi_1_rsdp_has_no_xsdt() {
        let mut m = qemu();
        let mut rsdp = fixture!("qemu", "RSDP").to_vec();
        rsdp[15] = 0; // revision 0: only the first 20 bytes count
        let rsdp = [with_checksum(rsdp[..20].to_vec(), 8), rsdp[20..].to_vec()].concat();
        m.put(QEMU_RSDP, &rsdp);
        assert_eq!(TableList::load(&mut m, QEMU_RSDP), Err(AcpiError::NoXsdt));
    }

    #[test]
    fn an_unreadable_xsdt_entry_is_left_out() {
        let mut m = qemu();
        m.0.remove(&0x3fb75000); // WAET
        let t = TableList::load(&mut m, QEMU_RSDP).unwrap();
        assert_eq!(t.entries.len(), 5);
        assert_eq!(t.find(b"WAET"), None);
        assert_eq!(t.find(b"BGRT"), Some(0x3fb74000));
    }

    #[test]
    fn a_garbage_pm1_control_length_does_not_overflow() {
        let mut t = fixture!("qemu", "FACP")[..116].to_vec();
        t[4..8].copy_from_slice(&116u32.to_le_bytes());
        t[89] = 0xFF; // PM1_CNT_LEN
        let fadt = parse_fadt(&with_checksum(t, 9));
        assert_eq!(fadt.pm1a_cnt.map(|g| g.bit_width), Some(255));
    }

    #[test]
    fn a_table_longer_than_any_real_one_is_not_read() {
        // A length of 4 GiB would be mapped and summed byte by byte.
        let mut m = FakeMem::default();
        let mut t = vec![0u8; 36];
        t[..4].copy_from_slice(b"DSDT");
        t[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        m.put(0x1000, &t);
        assert_eq!(
            read_table(&mut m, 0x1000),
            Err(AcpiError::TooLong(*b"DSDT", u32::MAX as usize))
        );
        assert_eq!(
            AcpiError::TooLong(*b"DSDT", u32::MAX as usize).to_string(),
            "DSDT claims 4294967295 bytes"
        );
        // The NUC's DSDT, the biggest table seen, is 469,477 bytes.
        t[4..8].copy_from_slice(&(MAX_TABLE as u32).to_le_bytes());
        m.put(0x1000, &t);
        assert_eq!(
            read_table(&mut m, 0x1000),
            Err(AcpiError::Unreadable(0x1000))
        );
    }

    #[test]
    fn a_fadt_too_short_for_its_dsdt_field_has_none() {
        // A valid 36-byte table: header only.
        let mut t = fixture!("qemu", "FACP")[..36].to_vec();
        t[4..8].copy_from_slice(&36u32.to_le_bytes());
        let fadt = parse_fadt(&with_checksum(t, 9));
        assert_eq!(fadt.dsdt, 0);
        assert_eq!((fadt.pm1a_cnt, fadt.reset, fadt.century), (None, None, 0));
    }

    #[test]
    fn a_table_shorter_than_its_header_is_rejected() {
        let mut m = FakeMem::default();
        let mut t = vec![0u8; 36];
        t[..4].copy_from_slice(b"HPET");
        t[4] = 20;
        m.put(0x1000, &t);
        assert_eq!(
            read_table(&mut m, 0x1000),
            Err(AcpiError::TooShort(*b"HPET"))
        );
    }
}
