//! The archive format (spec §4.2): a 64-byte header, a table of 88-byte
//! entries sorted by name, then each entry's data on a 4 KiB boundary.
//! Little-endian throughout. The CRC-32 covers everything after the header.

use alloc::vec::Vec;
use core::fmt;

pub const MAGIC: [u8; 8] = *b"RELAYSYS";
pub const FORMAT_VERSION: u32 = 1;
pub const HEADER_LEN: usize = 64;
pub const ENTRY_LEN: usize = 88;
/// The longest entry name, in bytes.
pub const NAME_MAX: usize = 64;
/// Every entry's data starts at a multiple of this.
pub const DATA_ALIGN: usize = 4096;
/// Unix permission bits; a mode may have no others.
pub const MODE_MASK: u16 = 0o7777;

// Header fields (offsets). 20..24, 36..40 and 48..64 are zero.
const H_MAGIC: usize = 0;
const H_FORMAT: usize = 8;
const H_ABI: usize = 12;
const H_COUNT: usize = 16;
const H_LEN: usize = 24;
const H_CRC: usize = 32;
const H_TIME: usize = 40;

// Entry fields (offsets). 65 and 68..72 are zero.
const E_NAME: usize = 0;
const E_NAME_LEN: usize = 64;
const E_MODE: usize = 66;
const E_OFFSET: usize = 72;
const E_LEN: usize = 80;

/// One file of the archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry<'a> {
    pub name: &'a [u8],
    /// Permission bits (`MODE_MASK`).
    pub mode: u16,
    pub data: &'a [u8],
}

/// Why an archive was refused. Entry numbers count from 0 in table order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SysImgError {
    TooShort(usize),
    BadMagic,
    FormatVersion(u32),
    /// The header's length is not the length that was read.
    Length {
        header: u64,
        actual: u64,
    },
    Crc,
    /// The entry table does not fit in the archive.
    TableOutside(u32),
    BadName(usize),
    /// A name not greater than the one before it (so also a duplicate).
    OutOfOrder(usize),
    BadMode(usize),
    DataOutside(usize),
    Misaligned(usize),
}

impl fmt::Display for SysImgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SysImgError::TooShort(n) => write!(f, "only {n} bytes"),
            SysImgError::BadMagic => write!(f, "not a system archive"),
            SysImgError::FormatVersion(v) => {
                write!(f, "format version {v}, expected {FORMAT_VERSION}")
            }
            SysImgError::Length { header, actual } => {
                write!(f, "header says {header} bytes, {actual} were read")
            }
            SysImgError::Crc => write!(f, "checksum mismatch"),
            SysImgError::TableOutside(n) => write!(f, "{n} entries do not fit"),
            SysImgError::BadName(i) => write!(f, "entry {i}: invalid name"),
            SysImgError::OutOfOrder(i) => write!(f, "entry {i}: name out of order or repeated"),
            SysImgError::BadMode(i) => write!(f, "entry {i}: invalid mode"),
            SysImgError::DataOutside(i) => write!(f, "entry {i}: data outside the archive"),
            SysImgError::Misaligned(i) => write!(f, "entry {i}: data not on a 4 KiB boundary"),
        }
    }
}

/// A name the archive can hold: 1 to `NAME_MAX` bytes, no `/`, no NUL, not
/// `.` or `..`.
pub fn valid_name(name: &[u8]) -> bool {
    (1..=NAME_MAX).contains(&name.len())
        && !name.contains(&b'/')
        && !name.contains(&0)
        && name != b"."
        && name != b".."
}

fn put_u16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

fn put_u32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn put_u64(b: &mut [u8], at: usize, v: u64) {
    b[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    let mut v = [0; 4];
    v.copy_from_slice(&b[at..at + 4]);
    u32::from_le_bytes(v)
}

fn u64_at(b: &[u8], at: usize) -> u64 {
    let mut v = [0; 8];
    v.copy_from_slice(&b[at..at + 8]);
    u64::from_le_bytes(v)
}

/// Builds an archive of `files` (in any order; the table is sorted by
/// name) for ABI `abi`, built at `build_time` (seconds since 1970).
pub fn write(abi: u32, build_time: u64, files: &[Entry<'_>]) -> Result<Vec<u8>, SysImgError> {
    let mut sorted: Vec<Entry<'_>> = files.to_vec();
    sorted.sort_by(|a, b| a.name.cmp(b.name));
    for (i, e) in sorted.iter().enumerate() {
        if !valid_name(e.name) {
            return Err(SysImgError::BadName(i));
        }
        if i > 0 && sorted[i - 1].name == e.name {
            return Err(SysImgError::OutOfOrder(i));
        }
        if e.mode & !MODE_MASK != 0 {
            return Err(SysImgError::BadMode(i));
        }
    }
    let table_end = HEADER_LEN + sorted.len() * ENTRY_LEN;
    let mut out = alloc::vec![0u8; table_end];
    for (i, e) in sorted.iter().enumerate() {
        let offset = out.len().next_multiple_of(DATA_ALIGN);
        out.resize(offset, 0);
        out.extend_from_slice(e.data);
        let at = HEADER_LEN + i * ENTRY_LEN;
        out[at + E_NAME..at + E_NAME + e.name.len()].copy_from_slice(e.name);
        out[at + E_NAME_LEN] = e.name.len() as u8;
        put_u16(&mut out, at + E_MODE, e.mode);
        put_u64(&mut out, at + E_OFFSET, offset as u64);
        put_u64(&mut out, at + E_LEN, e.data.len() as u64);
    }
    out[H_MAGIC..H_MAGIC + 8].copy_from_slice(&MAGIC);
    put_u32(&mut out, H_FORMAT, FORMAT_VERSION);
    put_u32(&mut out, H_ABI, abi);
    put_u32(&mut out, H_COUNT, sorted.len() as u32);
    let len = out.len() as u64;
    put_u64(&mut out, H_LEN, len);
    put_u64(&mut out, H_TIME, build_time);
    let crc = crc32::crc32(&out[HEADER_LEN..]);
    put_u32(&mut out, H_CRC, crc);
    Ok(out)
}

/// A checked archive: every entry's name, mode and data range were
/// checked by `parse`, so reading it cannot fail.
#[derive(Clone, Copy, Debug)]
pub struct Archive<'a> {
    bytes: &'a [u8],
    count: usize,
}

impl<'a> Archive<'a> {
    /// Checks `bytes`, all of what was read, in this order: size, magic,
    /// format version, length, CRC, the table's size, then each entry.
    pub fn parse(bytes: &'a [u8]) -> Result<Archive<'a>, SysImgError> {
        if bytes.len() < HEADER_LEN {
            return Err(SysImgError::TooShort(bytes.len()));
        }
        if bytes[H_MAGIC..H_MAGIC + 8] != MAGIC {
            return Err(SysImgError::BadMagic);
        }
        let format = u32_at(bytes, H_FORMAT);
        if format != FORMAT_VERSION {
            return Err(SysImgError::FormatVersion(format));
        }
        let header = u64_at(bytes, H_LEN);
        let actual = bytes.len() as u64;
        if header != actual {
            return Err(SysImgError::Length { header, actual });
        }
        if crc32::crc32(&bytes[HEADER_LEN..]) != u32_at(bytes, H_CRC) {
            return Err(SysImgError::Crc);
        }
        let count = u32_at(bytes, H_COUNT);
        let table_end = (count as usize)
            .checked_mul(ENTRY_LEN)
            .and_then(|t| t.checked_add(HEADER_LEN))
            .filter(|&end| end <= bytes.len())
            .ok_or(SysImgError::TableOutside(count))?;
        let archive = Archive {
            bytes,
            count: count as usize,
        };
        let mut previous: &[u8] = &[];
        for i in 0..archive.count {
            let at = HEADER_LEN + i * ENTRY_LEN;
            let name_len = bytes[at + E_NAME_LEN] as usize;
            if name_len > NAME_MAX || !valid_name(&bytes[at..at + name_len]) {
                return Err(SysImgError::BadName(i));
            }
            let name = &bytes[at..at + name_len];
            if i > 0 && name <= previous {
                return Err(SysImgError::OutOfOrder(i));
            }
            previous = name;
            if u16_at(bytes, at + E_MODE) & !MODE_MASK != 0 {
                return Err(SysImgError::BadMode(i));
            }
            let offset = u64_at(bytes, at + E_OFFSET);
            let len = u64_at(bytes, at + E_LEN);
            if !offset.is_multiple_of(DATA_ALIGN as u64) {
                return Err(SysImgError::Misaligned(i));
            }
            match offset.checked_add(len) {
                Some(end) if offset >= table_end as u64 && end <= actual => {}
                _ => return Err(SysImgError::DataOutside(i)),
            }
        }
        Ok(archive)
    }

    pub fn abi(&self) -> u32 {
        u32_at(self.bytes, H_ABI)
    }

    /// Seconds since 1970, UTC, when xtask built the archive.
    pub fn build_time(&self) -> u64 {
        u64_at(self.bytes, H_TIME)
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// The whole archive's size in bytes.
    pub fn total_len(&self) -> u64 {
        self.bytes.len() as u64
    }

    /// Entry `i` in name order.
    pub fn entry(&self, i: usize) -> Option<Entry<'a>> {
        if i >= self.count {
            return None;
        }
        let b = self.bytes;
        let at = HEADER_LEN + i * ENTRY_LEN;
        // `parse` checked the name and the data range.
        let name = &b[at..at + b[at + E_NAME_LEN] as usize];
        let offset = u64_at(b, at + E_OFFSET) as usize;
        let len = u64_at(b, at + E_LEN) as usize;
        Some(Entry {
            name,
            mode: u16_at(b, at + E_MODE),
            data: &b[offset..offset + len],
        })
    }

    /// The index of the entry called `name`.
    pub fn find(&self, name: &[u8]) -> Option<usize> {
        let (mut lo, mut hi) = (0, self.count);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let e = self.entry(mid)?;
            match e.name.cmp(name) {
                core::cmp::Ordering::Less => lo = mid + 1,
                core::cmp::Ordering::Greater => hi = mid,
                core::cmp::Ordering::Equal => return Some(mid),
            }
        }
        None
    }

    pub fn entries(&self) -> impl Iterator<Item = Entry<'a>> + '_ {
        (0..self.count).filter_map(|i| self.entry(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file<'a>(name: &'a str, mode: u16, data: &'a [u8]) -> Entry<'a> {
        Entry {
            name: name.as_bytes(),
            mode,
            data,
        }
    }

    fn sample() -> Vec<u8> {
        let big = vec![0xAB; 5000];
        let big: &'static [u8] = Box::leak(big.into_boxed_slice());
        write(
            7,
            1_790_000_000,
            &[
                file("t-args", 0o755, big),
                file("empty", 0o644, b""),
                file("cat", 0o755, b"\x7fELF cat"),
            ],
        )
        .unwrap()
    }

    /// Recomputes the CRC after a test has changed the table or the data.
    fn fix_crc(b: &mut [u8]) {
        let crc = crc32::crc32(&b[HEADER_LEN..]);
        put_u32(b, H_CRC, crc);
    }

    fn entry_field(i: usize, field: usize) -> usize {
        HEADER_LEN + i * ENTRY_LEN + field
    }

    #[test]
    fn what_is_written_reads_back_sorted_by_name() {
        let bytes = sample();
        let a = Archive::parse(&bytes).unwrap();
        assert_eq!(a.abi(), 7);
        assert_eq!(a.build_time(), 1_790_000_000);
        assert_eq!(a.len(), 3);
        assert_eq!(a.total_len(), bytes.len() as u64);
        let names: Vec<&[u8]> = a.entries().map(|e| e.name).collect();
        assert_eq!(names, [&b"cat"[..], b"empty", b"t-args"]);
        assert_eq!(a.entry(0).unwrap().data, b"\x7fELF cat");
        assert_eq!(a.entry(0).unwrap().mode, 0o755);
        assert_eq!(a.entry(1).unwrap().data, b"");
        assert_eq!(a.entry(1).unwrap().mode, 0o644);
        assert_eq!(a.entry(2).unwrap().data, &[0xAB; 5000][..]);
        assert_eq!(a.entry(3), None);
        assert_eq!(a.find(b"t-args"), Some(2));
        assert_eq!(a.find(b"cat"), Some(0));
        assert_eq!(a.find(b"empty"), Some(1));
        assert_eq!(a.find(b"ls"), None);
        assert_eq!(a.find(b""), None);
    }

    #[test]
    fn the_layout_is_the_spec_s() {
        let b = sample();
        assert_eq!(&b[0..8], b"RELAYSYS");
        assert_eq!(u32_at(&b, 8), 1, "format version");
        assert_eq!(u32_at(&b, 12), 7, "ABI version");
        assert_eq!(u32_at(&b, 16), 3, "entry count");
        assert_eq!(u64_at(&b, 24), b.len() as u64, "total length");
        assert_eq!(
            u32_at(&b, 32),
            crc32::crc32(&b[64..]),
            "CRC of all after the header"
        );
        assert_eq!(u64_at(&b, 40), 1_790_000_000, "build time");
        assert!(
            b[20..24]
                .iter()
                .chain(&b[36..40])
                .chain(&b[48..64])
                .all(|&x| x == 0)
        );
        // Entry 0 ("cat") right after the header.
        assert_eq!(&b[64..67], b"cat");
        assert!(b[67..128].iter().all(|&x| x == 0), "name padded with zeros");
        assert_eq!(b[128], 3, "name length");
        assert_eq!(u16_at(&b, 130), 0o755, "mode");
        let offsets: Vec<u64> = (0..3)
            .map(|i| u64_at(&b, entry_field(i, E_OFFSET)))
            .collect();
        assert_eq!(
            offsets,
            [4096, 8192, 8192],
            "each on a 4 KiB boundary; empty takes none"
        );
        assert_eq!(u64_at(&b, entry_field(2, E_LEN)), 5000);
        assert_eq!(b.len(), 8192 + 5000, "no padding after the last data");
    }

    #[test]
    fn an_empty_archive_is_just_a_header() {
        let b = write(1, 0, &[]).unwrap();
        assert_eq!(b.len(), HEADER_LEN);
        let a = Archive::parse(&b).unwrap();
        assert!(a.is_empty());
        assert_eq!(a.entries().count(), 0);
    }

    #[test]
    fn the_writer_refuses_what_the_reader_would() {
        let long = "x".repeat(65);
        for name in ["", "a/b", "a\0b", ".", "..", long.as_str()] {
            assert_eq!(
                write(1, 0, &[file(name, 0o644, b"")]),
                Err(SysImgError::BadName(0)),
                "{name:?}"
            );
        }
        assert!(write(1, 0, &[file(&"x".repeat(64), 0o644, b"")]).is_ok());
        assert_eq!(
            write(1, 0, &[file("a", 0o644, b""), file("a", 0o755, b"x")]),
            Err(SysImgError::OutOfOrder(1))
        );
        assert_eq!(
            write(1, 0, &[file("a", 0o10644, b"")]),
            Err(SysImgError::BadMode(0))
        );
    }

    #[test]
    fn a_damaged_header_is_refused() {
        let good = sample();
        assert_eq!(
            Archive::parse(&good[..63]).err(),
            Some(SysImgError::TooShort(63))
        );
        assert_eq!(Archive::parse(&[]).err(), Some(SysImgError::TooShort(0)));
        let mut b = good.clone();
        b[0] = b'X';
        assert_eq!(Archive::parse(&b).err(), Some(SysImgError::BadMagic));
        let mut b = good.clone();
        put_u32(&mut b, H_FORMAT, 2);
        assert_eq!(
            Archive::parse(&b).err(),
            Some(SysImgError::FormatVersion(2))
        );
        let n = good.len() as u64;
        assert_eq!(
            Archive::parse(&good[..good.len() - 1]).err(),
            Some(SysImgError::Length {
                header: n,
                actual: n - 1
            })
        );
        let mut b = good.clone();
        b.push(0);
        assert_eq!(
            Archive::parse(&b).err(),
            Some(SysImgError::Length {
                header: n,
                actual: n + 1
            })
        );
        let mut b = good.clone();
        *b.last_mut().unwrap() ^= 1;
        assert_eq!(Archive::parse(&b).err(), Some(SysImgError::Crc));
        let mut b = good.clone();
        b[HEADER_LEN] ^= 1;
        assert_eq!(
            Archive::parse(&b).err(),
            Some(SysImgError::Crc),
            "the table is covered"
        );
        let mut b = good.clone();
        put_u32(&mut b, H_COUNT, 1000);
        assert_eq!(
            Archive::parse(&b).err(),
            Some(SysImgError::TableOutside(1000))
        );
        let mut b = good.clone();
        put_u32(&mut b, H_COUNT, u32::MAX);
        assert_eq!(
            Archive::parse(&b).err(),
            Some(SysImgError::TableOutside(u32::MAX))
        );
    }

    #[test]
    fn a_damaged_entry_is_refused() {
        let good = sample();
        let damaged = |edit: &dyn Fn(&mut Vec<u8>)| {
            let mut b = good.clone();
            edit(&mut b);
            fix_crc(&mut b);
            Archive::parse(&b).err()
        };
        assert_eq!(
            damaged(&|b| b[entry_field(1, E_NAME_LEN)] = 0),
            Some(SysImgError::BadName(1))
        );
        assert_eq!(
            damaged(&|b| b[entry_field(1, E_NAME_LEN)] = 65),
            Some(SysImgError::BadName(1))
        );
        assert_eq!(
            damaged(&|b| b[entry_field(0, E_NAME + 1)] = b'/'),
            Some(SysImgError::BadName(0))
        );
        assert_eq!(
            damaged(&|b| b[entry_field(0, E_NAME + 1)] = 0),
            Some(SysImgError::BadName(0))
        );
        // "cat" -> "fat" sorts after "empty".
        assert_eq!(
            damaged(&|b| b[entry_field(0, E_NAME)] = b'f'),
            Some(SysImgError::OutOfOrder(1))
        );
        // "empty" -> "cat\0\0" with length 3: a repeat.
        assert_eq!(
            damaged(&|b| {
                b[entry_field(1, E_NAME)..entry_field(1, E_NAME) + 3].copy_from_slice(b"cat");
                b[entry_field(1, E_NAME_LEN)] = 3;
            }),
            Some(SysImgError::OutOfOrder(1))
        );
        assert_eq!(
            damaged(&|b| put_u16(b, entry_field(2, E_MODE), 0o100755)),
            Some(SysImgError::BadMode(2))
        );
        assert_eq!(
            damaged(&|b| put_u64(b, entry_field(0, E_OFFSET), 4097)),
            Some(SysImgError::Misaligned(0))
        );
        assert_eq!(
            damaged(&|b| put_u64(b, entry_field(0, E_OFFSET), 0)),
            Some(SysImgError::DataOutside(0))
        );
        assert_eq!(
            damaged(&|b| put_u64(b, entry_field(2, E_LEN), 5001)),
            Some(SysImgError::DataOutside(2))
        );
        assert_eq!(
            damaged(&|b| put_u64(b, entry_field(2, E_LEN), u64::MAX)),
            Some(SysImgError::DataOutside(2))
        );
        assert_eq!(
            damaged(&|b| put_u64(b, entry_field(2, E_OFFSET), u64::MAX - 4095)),
            Some(SysImgError::DataOutside(2))
        );
    }

    /// With 232 empty files the table ends exactly at a 4 KiB boundary, so
    /// the archive ends with the table: a name length of 255 in the last
    /// entry points past the end.
    #[test]
    fn a_name_length_past_the_end_is_refused() {
        let names: Vec<String> = (0..232).map(|i| format!("f{i:03}")).collect();
        let files: Vec<Entry<'_>> = names.iter().map(|n| file(n, 0o644, b"")).collect();
        let mut b = write(1, 0, &files).unwrap();
        assert_eq!(
            b.len(),
            HEADER_LEN + 232 * ENTRY_LEN,
            "nothing after the table"
        );
        b[entry_field(231, E_NAME_LEN)] = 255;
        fix_crc(&mut b);
        assert_eq!(Archive::parse(&b).err(), Some(SysImgError::BadName(231)));
    }

    /// Seeded random damage to a valid archive, with the CRC fixed half of
    /// the time so the checks after it run too: `parse` never panics, and
    /// whatever it accepts reads without panicking.
    #[test]
    fn random_damage_never_panics() {
        let good = sample();
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut accepted = 0;
        for _ in 0..20_000 {
            let mut b = good.clone();
            for _ in 0..1 + rnd() % 4 {
                // Mostly the header and the table, where the fields are.
                let at = if rnd() % 4 == 0 {
                    rnd() as usize % b.len()
                } else {
                    rnd() as usize % (HEADER_LEN + 3 * ENTRY_LEN)
                };
                b[at] = rnd() as u8;
            }
            if rnd() % 8 == 0 {
                b.truncate(rnd() as usize % b.len());
            }
            if b.len() >= HEADER_LEN && rnd() % 2 == 0 {
                let len = b.len() as u64;
                if rnd() % 2 == 0 {
                    put_u64(&mut b, H_LEN, len);
                }
                fix_crc(&mut b);
            }
            if let Ok(a) = Archive::parse(&b) {
                accepted += 1;
                for e in a.entries() {
                    assert!(valid_name(e.name));
                    assert_eq!(a.find(e.name).and_then(|i| a.entry(i)), Some(e));
                }
            }
        }
        assert!(
            accepted > 0,
            "some damage (a data byte with the CRC fixed) is harmless"
        );
    }

    #[test]
    fn errors_read_as_boot_messages() {
        assert_eq!(SysImgError::Crc.to_string(), "checksum mismatch");
        assert_eq!(SysImgError::BadMagic.to_string(), "not a system archive");
        assert_eq!(
            SysImgError::Length {
                header: 10,
                actual: 9
            }
            .to_string(),
            "header says 10 bytes, 9 were read"
        );
        assert_eq!(
            SysImgError::OutOfOrder(3).to_string(),
            "entry 3: name out of order or repeated"
        );
    }
}
