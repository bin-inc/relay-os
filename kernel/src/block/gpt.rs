//! GPT parsing (spec §6.5, UEFI 2.10 §5.3): the primary header at LBA 1 and
//! its partition entry array, both CRC32-checked, and the backup header when
//! either is bad. Everything read from the disk is untrusted (spec §10): a
//! corrupt or hostile table is refused or skipped, never a panic.

use ::crc32::crc32;
use alloc::vec::Vec;
use core::fmt;
use vfs::{BlockDevice, IoError};

/// A GUID in on-disk byte order (the first three fields little-endian), as GPT and BootInfo store it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Guid(pub [u8; 16]);

impl Guid {
    /// From the canonical text form's fields: 0FC63DAF-8483-4772-8E79-3D69D8477DE4 is
    /// from_fields(0x0FC63DAF, 0x8483, 0x4772, [0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D, 0xE4]).
    pub const fn from_fields(a: u32, b: u16, c: u16, d: [u8; 8]) -> Guid {
        let a = a.to_le_bytes();
        let b = b.to_le_bytes();
        let c = c.to_le_bytes();
        Guid([
            a[0], a[1], a[2], a[3], b[0], b[1], c[0], c[1], d[0], d[1], d[2], d[3], d[4], d[5],
            d[6], d[7],
        ])
    }

    /// The zero GUID marks an unused entry.
    pub fn is_zero(&self) -> bool {
        self.0 == [0; 16]
    }
}

impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let g = &self.0;
        let a = u32::from_le_bytes([g[0], g[1], g[2], g[3]]);
        let b = u16::from_le_bytes([g[4], g[5]]);
        let c = u16::from_le_bytes([g[6], g[7]]);
        write!(f, "{a:08X}-{b:04X}-{c:04X}-{:02X}{:02X}-", g[8], g[9])?;
        for byte in &g[10..] {
            write!(f, "{byte:02X}")?;
        }
        Ok(())
    }
}

/// The EFI System Partition: C12A7328-F81F-11D2-BA4B-00A0C93EC93B.
pub const ESP_TYPE: Guid = Guid::from_fields(
    0xC12A_7328,
    0xF81F,
    0x11D2,
    [0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B],
);

/// A Linux filesystem: 0FC63DAF-8483-4772-8E79-3D69D8477DE4.
pub const LINUX_FS_TYPE: Guid = Guid::from_fields(
    0x0FC6_3DAF,
    0x8483,
    0x4772,
    [0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D, 0xE4],
);

/// A used entry of the partition array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GptPartition {
    /// 1-based index in the entry array (Linux's partition number).
    pub number: u32,
    pub type_guid: Guid,
    pub unique_guid: Guid,
    pub first_lba: u64,
    /// Inclusive, as GPT stores it.
    pub last_lba: u64,
}

/// A disk's partition table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gpt {
    pub disk_guid: Guid,
    /// Used entries (type GUID not zero) in entry order.
    pub partitions: Vec<GptPartition>,
    /// The primary header or its entry array was bad and the backup was used.
    pub used_backup: bool,
}

/// Why `read_gpt` found no table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GptError {
    /// Reading the disk failed.
    Io(IoError),
    /// Neither header is a valid GPT header with a valid entry array.
    NoGpt,
}

impl fmt::Display for GptError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GptError::Io(_) => write!(f, "read error"),
            GptError::NoGpt => write!(f, "no valid GPT"),
        }
    }
}

const SIGNATURE: &[u8; 8] = b"EFI PART";
/// The header fields of UEFI 2.10 end here; `header_size` may be larger.
const MIN_HEADER_SIZE: usize = 92;
const MIN_ENTRY_SIZE: u32 = 128;
/// sfdisk writes 16 KiB; this leaves room for any real table, and a corrupt
/// entry count cannot make us allocate gigabytes.
const MAX_ARRAY_BYTES: u64 = 1 << 20;

/// Offsets in the header (UEFI 2.10 table 5.5).
const H_SIZE: usize = 12;
const H_CRC: usize = 16;
const H_MY_LBA: usize = 24;
const H_ALTERNATE: usize = 32;
const H_FIRST_USABLE: usize = 40;
const H_LAST_USABLE: usize = 48;
const H_DISK_GUID: usize = 56;
const H_ENTRIES_LBA: usize = 72;
const H_ENTRY_COUNT: usize = 80;
const H_ENTRY_SIZE: usize = 84;
const H_ENTRIES_CRC: usize = 88;

/// Offsets in a partition entry (UEFI 2.10 table 5.6).
const E_TYPE: usize = 0;
const E_UNIQUE: usize = 16;
const E_FIRST: usize = 32;
const E_LAST: usize = 40;

/// Callers check that `b` is long enough first.
fn u32_at(b: &[u8], o: usize) -> u32 {
    let mut v = [0; 4];
    v.copy_from_slice(&b[o..o + 4]);
    u32::from_le_bytes(v)
}

fn u64_at(b: &[u8], o: usize) -> u64 {
    let mut v = [0; 8];
    v.copy_from_slice(&b[o..o + 8]);
    u64::from_le_bytes(v)
}

fn guid_at(b: &[u8], o: usize) -> Guid {
    let mut g = [0; 16];
    g.copy_from_slice(&b[o..o + 16]);
    Guid(g)
}

/// A header that passed every check of `parse_header`.
struct Header {
    alternate_lba: u64,
    first_usable: u64,
    last_usable: u64,
    disk_guid: Guid,
    entries_lba: u64,
    entry_count: u32,
    entry_size: u32,
    entries_crc: u32,
}

impl Header {
    /// Bytes in the entry array; bounded by `MAX_ARRAY_BYTES` once parsed.
    fn array_bytes(&self) -> u64 {
        self.entry_count as u64 * self.entry_size as u64
    }
}

/// Checks the header in `block`, read from `lba` of a disk of `blocks`
/// blocks of `block.len()` bytes.
fn parse_header(block: &[u8], lba: u64, blocks: u64) -> Option<Header> {
    if block.len() < MIN_HEADER_SIZE || block[..8] != *SIGNATURE {
        return None;
    }
    let size = u32_at(block, H_SIZE) as usize;
    if !(MIN_HEADER_SIZE..=block.len()).contains(&size) {
        return None;
    }
    let mut copy = block[..size].to_vec();
    copy[H_CRC..H_CRC + 4].fill(0);
    if crc32(&copy) != u32_at(block, H_CRC) || u64_at(block, H_MY_LBA) != lba {
        return None;
    }
    let h = Header {
        alternate_lba: u64_at(block, H_ALTERNATE),
        first_usable: u64_at(block, H_FIRST_USABLE),
        last_usable: u64_at(block, H_LAST_USABLE),
        disk_guid: guid_at(block, H_DISK_GUID),
        entries_lba: u64_at(block, H_ENTRIES_LBA),
        entry_count: u32_at(block, H_ENTRY_COUNT),
        entry_size: u32_at(block, H_ENTRY_SIZE),
        entries_crc: u32_at(block, H_ENTRIES_CRC),
    };
    if h.first_usable > h.last_usable || h.last_usable >= blocks {
        return None;
    }
    if h.entry_size < MIN_ENTRY_SIZE || !h.entry_size.is_multiple_of(8) {
        return None;
    }
    if h.array_bytes() > MAX_ARRAY_BYTES {
        return None;
    }
    let array_blocks = h.array_bytes().div_ceil(block.len() as u64);
    match h.entries_lba.checked_add(array_blocks) {
        Some(end) if end <= blocks => Some(h),
        _ => None,
    }
}

/// Reads `count` whole blocks at `lba`; `count` is bounded by the caller.
fn read_blocks(dev: &mut dyn BlockDevice, lba: u64, count: u64) -> Result<Vec<u8>, IoError> {
    let len = usize::try_from(count)
        .ok()
        .and_then(|n| n.checked_mul(dev.block_size()))
        .ok_or(IoError::OutOfRange)?;
    let mut buf = alloc::vec![0; len];
    dev.read(lba, &mut buf)?;
    Ok(buf)
}

/// The header at `lba`, `Ok(None)` if it is not a valid one.
fn read_header(dev: &mut dyn BlockDevice, lba: u64) -> Result<Option<Header>, IoError> {
    let block = read_blocks(dev, lba, 1)?;
    Ok(parse_header(&block, lba, dev.block_count()))
}

/// The used, mountable entries of `h`'s array, `Ok(None)` if its CRC is wrong.
fn read_entries(
    dev: &mut dyn BlockDevice,
    h: &Header,
) -> Result<Option<Vec<GptPartition>>, IoError> {
    // parse_header bounded the array to MAX_ARRAY_BYTES inside the disk.
    let bytes = h.array_bytes() as usize;
    let blocks = bytes.div_ceil(dev.block_size()) as u64;
    let buf = read_blocks(dev, h.entries_lba, blocks)?;
    let array = &buf[..bytes];
    if crc32(array) != h.entries_crc {
        return Ok(None);
    }
    let mut partitions = Vec::new();
    for (i, e) in array.chunks_exact(h.entry_size as usize).enumerate() {
        let type_guid = guid_at(e, E_TYPE);
        let (first_lba, last_lba) = (u64_at(e, E_FIRST), u64_at(e, E_LAST));
        // An entry we could not mount is skipped, not the whole table.
        if type_guid.is_zero()
            || first_lba > last_lba
            || first_lba < h.first_usable
            || last_lba > h.last_usable
        {
            continue;
        }
        partitions.push(GptPartition {
            // At most MAX_ARRAY_BYTES / MIN_ENTRY_SIZE entries, so no overflow.
            number: i as u32 + 1,
            type_guid,
            unique_guid: guid_at(e, E_UNIQUE),
            first_lba,
            last_lba,
        });
    }
    Ok(Some(partitions))
}

/// Reads the GPT of `dev` (spec §6.5): the primary header at LBA 1 and its entry array,
/// both CRC32-checked; if either is bad, the backup header at the disk's last LBA (or at
/// the primary's alternate LBA when the primary header itself was valid) and its array.
///
/// A read error on the primary side is not final, since the backup may still
/// be readable; the result is `Io` only if reading the backup fails too.
pub fn read_gpt(dev: &mut dyn BlockDevice) -> Result<Gpt, GptError> {
    let blocks = dev.block_count();
    let backup_lba = match read_header(dev, 1) {
        Ok(Some(h)) => match read_entries(dev, &h) {
            Ok(Some(partitions)) => return Ok(gpt(&h, partitions, false)),
            // Only a header that passed its checks can say where the
            // backup is.
            Ok(None) | Err(_) => Some(h.alternate_lba).filter(|&lba| lba < blocks),
        },
        Ok(None) | Err(_) => blocks.checked_sub(1),
    };
    let Some(backup_lba) = backup_lba else {
        return Err(GptError::NoGpt);
    };
    let h = read_header(dev, backup_lba)
        .map_err(GptError::Io)?
        .ok_or(GptError::NoGpt)?;
    match read_entries(dev, &h).map_err(GptError::Io)? {
        Some(partitions) => Ok(gpt(&h, partitions, true)),
        None => Err(GptError::NoGpt),
    }
}

fn gpt(h: &Header, partitions: Vec<GptPartition>, used_backup: bool) -> Gpt {
    Gpt {
        disk_guid: h.disk_guid,
        partitions,
        used_backup,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::testing::{MemDisk, expected_gpt, fdisk_image, guid, sfdisk_image};
    use alloc::string::ToString;

    /// `xtask/src/image.rs`'s `sfdisk_script(true)` with the fixed GUIDs of
    /// `xtask/src/config.rs`: the QEMU image's layout.
    const OUR_LAYOUT: &str = "label: gpt\n\
        label-id: 52454C41-5900-4000-8000-000000000001\n\
        first-lba: 2048\n\
        start=2048, size=131072, type=C12A7328-F81F-11D2-BA4B-00A0C93EC93B, \
        uuid=52454C41-5900-4000-8000-000000000002, name=\"RELAYESP\"\n\
        start=133120, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4, \
        uuid=52454C41-5900-4000-8000-000000000003, name=\"relayroot\"\n";
    const IMAGE_BYTES: u64 = 256 << 20;

    /// Two partitions on 8 MiB, for the corruption tests.
    const SMALL: &str = "label: gpt\n\
        start=2048, size=4096, type=C12A7328-F81F-11D2-BA4B-00A0C93EC93B\n\
        start=6144, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4\n";
    const SMALL_BYTES: u64 = 8 << 20;
    const SMALL_BLOCKS: u64 = SMALL_BYTES / 512;

    fn get32(b: &[u8], o: usize) -> u32 {
        u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
    }
    fn get64(b: &[u8], o: usize) -> u64 {
        u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
    }
    fn put32(b: &mut [u8], o: usize, v: u32) {
        b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn put64(b: &mut [u8], o: usize, v: u64) {
        b[o..o + 8].copy_from_slice(&v.to_le_bytes());
    }

    fn block(img: &mut [u8], bs: usize, lba: u64) -> &mut [u8] {
        let at = lba as usize * bs;
        &mut img[at..at + bs]
    }

    /// Recomputes the header CRC at `lba`, as a writer of a hostile header
    /// would, over `header_size` bytes (at most the block).
    fn reseal(img: &mut [u8], bs: usize, lba: u64) {
        let h = block(img, bs, lba);
        let size = (get32(h, H_SIZE) as usize).min(bs);
        put32(h, H_CRC, 0);
        let crc = crc32(&h[..size]);
        put32(h, H_CRC, crc);
    }

    /// Recomputes the entry array CRC of the header at `lba` from its
    /// fields, then the header CRC.
    fn reseal_array(img: &mut [u8], bs: usize, lba: u64) {
        let h = block(img, bs, lba);
        let at = get64(h, H_ENTRIES_LBA) as usize * bs;
        let len = get32(h, H_ENTRY_COUNT) as usize * get32(h, H_ENTRY_SIZE) as usize;
        let crc = crc32(&img[at..at + len]);
        put32(block(img, bs, lba), H_ENTRIES_CRC, crc);
        reseal(img, bs, lba);
    }

    /// Byte offset of entry `index` (0-based) of the header at `lba`.
    fn entry(img: &mut [u8], bs: usize, lba: u64, index: usize) -> usize {
        let h = block(img, bs, lba);
        get64(h, H_ENTRIES_LBA) as usize * bs + index * get32(h, H_ENTRY_SIZE) as usize
    }

    fn read(img: &[u8], bs: usize) -> Result<Gpt, GptError> {
        read_gpt(&mut MemDisk::new(img.to_vec(), bs))
    }

    /// A fresh small image and what reading it gives.
    fn small() -> (Vec<u8>, Gpt) {
        let img = sfdisk_image(SMALL, SMALL_BYTES);
        let gpt = read(&img, 512).unwrap();
        assert_eq!(gpt, expected_gpt(&img, 512));
        assert_eq!(gpt.partitions.len(), 2);
        (img, gpt)
    }

    fn from_backup(gpt: &Gpt) -> Result<Gpt, GptError> {
        Ok(Gpt {
            used_backup: true,
            ..gpt.clone()
        })
    }

    /// Applies `change` to the primary header, reseals it and expects the
    /// backup to be used.
    fn refused(change: impl FnOnce(&mut [u8])) {
        let (mut img, good) = small();
        change(block(&mut img, 512, 1));
        reseal(&mut img, 512, 1);
        assert_eq!(read(&img, 512), from_backup(&good));
    }

    #[test]
    fn guid_from_fields_matches_the_bytes_sfdisk_writes() {
        let img = sfdisk_image(OUR_LAYOUT, IMAGE_BYTES);
        let esp = &img[1024..1024 + 128];
        let root = &img[1024 + 128..1024 + 256];
        assert_eq!(esp[..16], ESP_TYPE.0);
        assert_eq!(root[..16], LINUX_FS_TYPE.0);
        let id = |n| Guid::from_fields(0x5245_4C41, 0x5900, 0x4000, [0x80, 0, 0, 0, 0, 0, 0, n]);
        assert_eq!(img[512 + H_DISK_GUID..512 + H_DISK_GUID + 16], id(1).0);
        assert_eq!(esp[16..32], id(2).0);
        assert_eq!(root[16..32], id(3).0);
        assert_eq!(
            LINUX_FS_TYPE.0,
            [
                0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47, 0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47,
                0x7D, 0xE4
            ]
        );
    }

    #[test]
    fn guid_display_is_canonical_upper_case() {
        assert_eq!(
            LINUX_FS_TYPE.to_string(),
            "0FC63DAF-8483-4772-8E79-3D69D8477DE4"
        );
        assert_eq!(ESP_TYPE.to_string(), "C12A7328-F81F-11D2-BA4B-00A0C93EC93B");
        assert_eq!(
            Guid([0; 16]).to_string(),
            "00000000-0000-0000-0000-000000000000"
        );
        assert_eq!(guid("52454C41-5900-4000-8000-00000000000A").0[15], 0x0A);
    }

    #[test]
    fn only_the_all_zero_guid_is_zero() {
        assert!(Guid([0; 16]).is_zero());
        assert!(!LINUX_FS_TYPE.is_zero());
        let mut last = [0; 16];
        last[15] = 1;
        assert!(!Guid(last).is_zero());
    }

    #[test]
    fn errors_are_short() {
        assert_eq!(GptError::Io(IoError::Device).to_string(), "read error");
        assert_eq!(GptError::NoGpt.to_string(), "no valid GPT");
    }

    #[test]
    fn our_layout_as_sfdisk_writes_it() {
        let img = sfdisk_image(OUR_LAYOUT, IMAGE_BYTES);
        let gpt = read(&img, 512).unwrap();
        assert_eq!(gpt, expected_gpt(&img, 512));
        let id = |n| Guid::from_fields(0x5245_4C41, 0x5900, 0x4000, [0x80, 0, 0, 0, 0, 0, 0, n]);
        // sfdisk ends the last partition on a 1 MiB boundary.
        let last = (IMAGE_BYTES >> 9) - 2048 - 1;
        assert_eq!(
            gpt,
            Gpt {
                disk_guid: id(1),
                partitions: alloc::vec![
                    GptPartition {
                        number: 1,
                        type_guid: ESP_TYPE,
                        unique_guid: id(2),
                        first_lba: 2048,
                        last_lba: 133_119,
                    },
                    GptPartition {
                        number: 2,
                        type_guid: LINUX_FS_TYPE,
                        unique_guid: id(3),
                        first_lba: 133_120,
                        last_lba: last,
                    },
                ],
                used_backup: false,
            }
        );
    }

    #[test]
    fn more_partitions_with_gaps_in_the_numbering() {
        // Entries 4, 6-8 and 10-127 stay unused; entry 128 is the array's last.
        let script = "label: gpt\n\
            p1 : start=2048, size=2048, type=C12A7328-F81F-11D2-BA4B-00A0C93EC93B\n\
            p2 : start=4096, size=1, type=0657FD6D-A4AB-43C4-84E5-0933C84B4F4F\n\
            p3 : start=6144, size=2048, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4\n\
            p5 : start=8192, size=2048, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4\n\
            p9 : start=10240, size=2048, type=21686148-6449-6E6F-744E-656564454649\n\
            p128 : start=12288, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4\n";
        let img = sfdisk_image(script, 16 << 20);
        let gpt = read(&img, 512).unwrap();
        assert_eq!(gpt, expected_gpt(&img, 512));
        let numbers: Vec<u32> = gpt.partitions.iter().map(|p| p.number).collect();
        assert_eq!(numbers, [1, 2, 3, 5, 9, 128]);
        assert_eq!(gpt.partitions[1].first_lba, gpt.partitions[1].last_lba);
        assert!(!gpt.used_backup);
    }

    const FOUR_K: &str = "label: gpt\n\
        sector-size: 4096\n\
        start=256, size=1024, type=C12A7328-F81F-11D2-BA4B-00A0C93EC93B\n\
        start=1280, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4\n";

    #[test]
    fn sectors_of_4096_bytes() {
        let img = fdisk_image(FOUR_K, 16 << 20, 4096);
        let gpt = read(&img, 4096).unwrap();
        assert_eq!(gpt, expected_gpt(&img, 4096));
        let lbas: Vec<(u64, u64)> = gpt
            .partitions
            .iter()
            .map(|p| (p.first_lba, p.last_lba))
            .collect();
        // sfdisk ends the last partition on a 1 MiB boundary.
        assert_eq!(lbas, [(256, 1279), (1280, 4096 - 256 - 1)]);
        assert_eq!(gpt.partitions[1].type_guid, LINUX_FS_TYPE);
    }

    #[test]
    fn the_block_size_is_the_devices() {
        let four_k = fdisk_image(FOUR_K, 16 << 20, 4096);
        assert_eq!(read(&four_k, 512), Err(GptError::NoGpt));
        let (small, _) = small();
        assert_eq!(read(&small, 4096), Err(GptError::NoGpt));
    }

    #[test]
    fn corrupt_primary_signature_uses_the_backup() {
        let (mut img, good) = small();
        img[512] ^= 1;
        assert_eq!(read(&img, 512), from_backup(&good));
        // With the CRC recomputed, only the signature check catches it.
        refused(|h| h[7] ^= 1);
    }

    #[test]
    fn corrupt_byte_under_the_primary_crc_uses_the_backup() {
        let (mut img, good) = small();
        img[512 + H_DISK_GUID] ^= 1;
        assert_eq!(read(&img, 512), from_backup(&good));
    }

    #[test]
    fn primary_my_lba_must_be_where_it_was_read() {
        refused(|h| put64(h, H_MY_LBA, 2));
    }

    #[test]
    fn a_resealed_primary_is_accepted() {
        // The other tests change a field and reseal; this shows resealing
        // alone changes nothing.
        let (mut img, good) = small();
        reseal_array(&mut img, 512, 1);
        assert_eq!(read(&img, 512), Ok(good));
    }

    /// The small image with `extra` zero blocks after it, so the backup
    /// header is no longer at the last LBA.
    fn grown(extra: usize) -> (Vec<u8>, Gpt) {
        let (mut img, good) = small();
        img.resize(img.len() + extra * 512, 0);
        (img, good)
    }

    #[test]
    fn corrupt_primary_array_uses_the_alternate_lba() {
        let (mut img, good) = grown(64);
        let at = entry(&mut img, 512, 1, 0);
        img[at + 56] ^= 1; // a byte of the name
        assert_eq!(read(&img, 512), from_backup(&good));
    }

    #[test]
    fn a_bad_primary_header_looks_at_the_last_lba_only() {
        let (mut img, _) = grown(64);
        img[512] ^= 1;
        assert_eq!(read(&img, 512), Err(GptError::NoGpt));
    }

    #[test]
    fn the_alternate_lba_must_be_inside_the_disk() {
        for alternate in [SMALL_BLOCKS, u64::MAX] {
            let (mut img, _) = small();
            let at = entry(&mut img, 512, 1, 0);
            img[at + 56] ^= 1;
            put64(block(&mut img, 512, 1), H_ALTERNATE, alternate);
            reseal(&mut img, 512, 1);
            assert_eq!(read(&img, 512), Err(GptError::NoGpt), "{alternate}");
        }
    }

    #[test]
    fn both_headers_corrupt_is_no_gpt() {
        let (mut img, _) = small();
        img[512] ^= 1;
        let last = img.len() - 512;
        img[last] ^= 1;
        assert_eq!(read(&img, 512), Err(GptError::NoGpt));
    }

    #[test]
    fn both_arrays_corrupt_is_no_gpt() {
        let (mut img, _) = small();
        let primary = entry(&mut img, 512, 1, 0);
        let backup = entry(&mut img, 512, SMALL_BLOCKS - 1, 0);
        img[primary + 56] ^= 1;
        img[backup + 56] ^= 1;
        assert_eq!(read(&img, 512), Err(GptError::NoGpt));
    }

    #[test]
    fn a_disk_of_zeros_is_no_gpt() {
        assert_eq!(read(&[0; 64 * 512], 512), Err(GptError::NoGpt));
        assert_eq!(read(&[0; 8 * 4096], 4096), Err(GptError::NoGpt));
        assert_eq!(read(&[], 512), Err(GptError::NoGpt));
        assert_eq!(read(&[0; 512], 512), Err(GptError::NoGpt));
    }

    #[test]
    fn blocks_too_small_for_a_header_are_no_gpt() {
        assert_eq!(read(&[0; 64], 4), Err(GptError::NoGpt));
        let mut img = alloc::vec![0; 64 * 64];
        img[64..72].copy_from_slice(SIGNATURE);
        assert_eq!(read(&img, 64), Err(GptError::NoGpt));
    }

    fn with_bad(img: &[u8], bad: core::ops::Range<u64>) -> Result<Gpt, GptError> {
        let mut disk = MemDisk::new(img.to_vec(), 512);
        disk.bad = bad;
        read_gpt(&mut disk)
    }

    #[test]
    fn failing_reads_are_io_errors() {
        let (img, _) = small();
        assert_eq!(
            with_bad(&img, 0..u64::MAX),
            Err(GptError::Io(IoError::Device))
        );
    }

    #[test]
    fn an_unreadable_primary_uses_the_backup() {
        let (img, good) = small();
        assert_eq!(with_bad(&img, 1..2), from_backup(&good));
        // The primary array unreadable: the header is fine, so the backup
        // comes from its alternate LBA.
        let (img, good) = grown(64);
        assert_eq!(with_bad(&img, 2..3), from_backup(&good));
    }

    #[test]
    fn a_read_error_counts_only_if_the_backup_is_unreadable_too() {
        let (mut img, _) = small();
        let last = img.len() - 512;
        img[last] ^= 1;
        // Primary unreadable, backup readable but corrupt.
        assert_eq!(with_bad(&img, 1..2), Err(GptError::NoGpt));
        // Primary corrupt, backup unreadable.
        let (mut img, _) = small();
        img[512] ^= 1;
        assert_eq!(
            with_bad(&img, SMALL_BLOCKS - 1..SMALL_BLOCKS),
            Err(GptError::Io(IoError::Device))
        );
    }

    #[test]
    fn hostile_header_sizes_are_refused() {
        for size in [0, 91, 513, u32::MAX] {
            refused(|h| put32(h, H_SIZE, size));
        }
    }

    #[test]
    fn a_header_as_large_as_the_block_is_accepted() {
        let (mut img, good) = small();
        put32(block(&mut img, 512, 1), H_SIZE, 512);
        reseal(&mut img, 512, 1);
        assert_eq!(read(&img, 512), Ok(good));
    }

    #[test]
    fn hostile_usable_ranges_are_refused() {
        refused(|h| put64(h, H_LAST_USABLE, SMALL_BLOCKS));
        refused(|h| put64(h, H_LAST_USABLE, u64::MAX));
        refused(|h| {
            let last = get64(h, H_LAST_USABLE);
            put64(h, H_FIRST_USABLE, last + 1);
        });
    }

    #[test]
    fn a_usable_range_up_to_the_last_block_is_accepted() {
        let (mut img, good) = small();
        put64(block(&mut img, 512, 1), H_LAST_USABLE, SMALL_BLOCKS - 1);
        reseal(&mut img, 512, 1);
        assert_eq!(read(&img, 512), Ok(good));
    }

    #[test]
    fn an_entry_array_past_the_end_is_refused() {
        refused(|h| put64(h, H_ENTRIES_LBA, SMALL_BLOCKS - 1));
        refused(|h| put64(h, H_ENTRIES_LBA, u64::MAX));
        // In the backup it is refused before anything is read, so it is
        // not a read error.
        let (mut img, _) = small();
        img[512] ^= 1;
        put64(
            block(&mut img, 512, SMALL_BLOCKS - 1),
            H_ENTRIES_LBA,
            SMALL_BLOCKS - 1,
        );
        reseal(&mut img, 512, SMALL_BLOCKS - 1);
        assert_eq!(read(&img, 512), Err(GptError::NoGpt));
    }

    #[test]
    fn an_entry_array_up_to_the_last_block_is_accepted() {
        // Moved over the backup header, so only the primary can be read.
        // 128 entries take 32 blocks of 512 bytes, 4 of 4096.
        let four_k = fdisk_image(FOUR_K, 16 << 20, 4096);
        for (mut img, bs) in [(small().0, 512), (four_k, 4096)] {
            let good = read(&img, bs).unwrap();
            let array = 128 * 128;
            let to = img.len() - array;
            img.copy_within(2 * bs..2 * bs + array, to);
            put64(block(&mut img, bs, 1), H_ENTRIES_LBA, (to / bs) as u64);
            reseal(&mut img, bs, 1);
            assert_eq!(read(&img, bs), Ok(good), "{bs}");
        }
    }

    #[test]
    fn an_entry_array_ending_inside_a_block_is_accepted() {
        let (mut img, good) = small();
        put32(block(&mut img, 512, 1), H_ENTRY_COUNT, 127);
        reseal_array(&mut img, 512, 1);
        assert_eq!(read(&img, 512), Ok(good));
    }

    #[test]
    fn hostile_entry_sizes_are_refused() {
        for size in [0, 8, 100, 120, 132] {
            let (mut img, good) = small();
            put32(block(&mut img, 512, 1), H_ENTRY_SIZE, size);
            reseal_array(&mut img, 512, 1);
            assert_eq!(read(&img, 512), from_backup(&good), "{size}");
        }
    }

    #[test]
    fn an_entry_array_over_1_mib_is_refused() {
        // 8193 entries of 128 bytes end inside the 8 MiB disk, so only the
        // size bound refuses them.
        for count in [8193, u32::MAX] {
            let (mut img, good) = small();
            put32(block(&mut img, 512, 1), H_ENTRY_COUNT, count);
            if count == 8193 {
                reseal_array(&mut img, 512, 1);
            } else {
                reseal(&mut img, 512, 1);
            }
            assert_eq!(read(&img, 512), from_backup(&good), "{count}");
        }
    }

    #[test]
    fn an_entry_array_of_1_mib_is_accepted() {
        // The entries past sfdisk's 128 are the zeros before partition 1.
        let (mut img, good) = small();
        put32(block(&mut img, 512, 1), H_ENTRY_COUNT, 8192);
        reseal_array(&mut img, 512, 1);
        assert_eq!(read(&img, 512), Ok(good));
    }

    /// Changes entry `index` of the primary array and reseals it.
    fn change_entry(img: &mut [u8], index: usize, change: impl FnOnce(&mut [u8])) {
        let at = entry(img, 512, 1, index);
        change(&mut img[at..at + 128]);
        reseal_array(img, 512, 1);
    }

    #[test]
    fn entries_outside_the_usable_range_are_skipped() {
        let (mut img, good) = small();
        let last_usable = get64(block(&mut img, 512, 1), H_LAST_USABLE);
        change_entry(&mut img, 1, |e| put64(e, E_LAST, last_usable + 1));
        let gpt = read(&img, 512).unwrap();
        assert_eq!(gpt.partitions, good.partitions[..1]);
        assert!(!gpt.used_backup);

        let (mut img, good) = small();
        let first_usable = get64(block(&mut img, 512, 1), H_FIRST_USABLE);
        change_entry(&mut img, 0, |e| put64(e, E_FIRST, first_usable - 1));
        assert_eq!(read(&img, 512).unwrap().partitions, good.partitions[1..]);
    }

    #[test]
    fn entries_may_fill_the_usable_range() {
        // sfdisk starts partition 1 at the first usable LBA but ends the
        // last one on a 1 MiB boundary; move its end to the last usable one.
        let (mut img, mut good) = small();
        let h = block(&mut img, 512, 1);
        let (first_usable, last_usable) = (get64(h, H_FIRST_USABLE), get64(h, H_LAST_USABLE));
        assert_eq!(good.partitions[0].first_lba, first_usable);
        change_entry(&mut img, 1, |e| put64(e, E_LAST, last_usable));
        good.partitions[1].last_lba = last_usable;
        assert_eq!(read(&img, 512), Ok(good));
    }

    #[test]
    fn reversed_entries_are_skipped() {
        let (mut img, good) = small();
        change_entry(&mut img, 0, |e| {
            let first = get64(e, E_FIRST);
            put64(e, E_LAST, first - 1);
        });
        assert_eq!(read(&img, 512).unwrap().partitions, good.partitions[1..]);
    }

    #[test]
    fn entries_with_a_zero_type_are_unused() {
        let (mut img, good) = small();
        change_entry(&mut img, 0, |e| e[..16].fill(0));
        assert_eq!(read(&img, 512).unwrap().partitions, good.partitions[1..]);
    }
}
