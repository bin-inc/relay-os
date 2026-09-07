use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::str::FromStr;

use crc::{CRC_32_ISO_HDLC, Crc};
use relay_abi::GptGuid;

use crate::image::{ImageError, Result};
use crate::layout::{
    DISK_GUID, DISK_SECTORS, ESP_GUID, ESP_RANGE, EXT2_UUID, ROOT_GUID, ROOT_RANGE, SECTOR_SIZE,
    sectors_to_bytes,
};

const CRC32: Crc<u32> = Crc::<u32>::new(&CRC_32_ISO_HDLC);
const EFI_SYSTEM_PARTITION: &str = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b";
const LINUX_FILESYSTEM: &str = "0fc63daf-8483-4772-8e79-3d69d8477de4";
const GPT_HEADER_SIZE: usize = 92;
const GPT_ENTRY_SIZE: usize = 128;
const GPT_ENTRY_COUNT: usize = 128;

pub struct VerifyReport {
    pub sgdisk: String,
    pub e2fsck: String,
}

pub fn verify_image(path: &Path) -> Result<VerifyReport> {
    let bytes = fs::read(path).map_err(io_error)?;
    let root = parse_gpt(&bytes)?;
    verify_ext2(&bytes, &root)?;
    verify_boot_config(&bytes, path)?;
    let extracted = extracted_root_path(path)?;
    extract_root(&bytes, root, &extracted)?;
    let sgdisk = command_output(
        Command::new("sgdisk").args(["--verify", path_string(path)?.as_str()]),
        "sgdisk --verify",
    )?;
    let e2fsck = command_output(
        Command::new("e2fsck")
            .args(["-fn", path_string(&extracted)?.as_str()])
            .env("LC_ALL", "C"),
        "e2fsck -fn",
    )?;
    Ok(VerifyReport { sgdisk, e2fsck })
}

fn parse_gpt(bytes: &[u8]) -> Result<std::ops::Range<u64>> {
    let expected_len = usize::try_from(
        sectors_to_bytes(DISK_SECTORS)
            .ok_or_else(|| ImageError::new("disk byte length overflow"))?,
    )
    .map_err(|_| ImageError::new("disk byte length does not fit usize"))?;
    if bytes.len() != expected_len {
        return Err(ImageError::new(
            "disk image does not have the fixed geometry",
        ));
    }
    let primary = header(bytes, 1, DISK_SECTORS - 1, 2)?;
    let backup_entries_lba = DISK_SECTORS - 33;
    let backup = header(bytes, DISK_SECTORS - 1, 1, backup_entries_lba)?;
    let entries_len = GPT_ENTRY_COUNT * GPT_ENTRY_SIZE;
    let primary_entries = slice_at_lba(bytes, primary.entries_lba, entries_len)?;
    let backup_entries = slice_at_lba(bytes, backup.entries_lba, entries_len)?;
    if primary_entries != backup_entries {
        return Err(ImageError::new(
            "primary and backup GPT entry arrays differ",
        ));
    }
    if CRC32.checksum(primary_entries) != primary.entries_crc
        || CRC32.checksum(backup_entries) != backup.entries_crc
    {
        return Err(ImageError::new("GPT entry array checksum mismatch"));
    }
    partition_range(
        primary_entries,
        0,
        EFI_SYSTEM_PARTITION,
        ESP_GUID,
        ESP_RANGE.clone(),
    )?;
    let root = partition_range(
        primary_entries,
        1,
        LINUX_FILESYSTEM,
        ROOT_GUID,
        ROOT_RANGE.clone(),
    )?;
    if primary_entries[2 * GPT_ENTRY_SIZE..]
        .iter()
        .any(|byte| *byte != 0)
    {
        return Err(ImageError::new(
            "GPT contains an unexpected partition entry",
        ));
    }
    Ok(root)
}

struct Header {
    entries_lba: u64,
    entries_crc: u32,
}

fn header(bytes: &[u8], lba: u64, expected_backup: u64, expected_entries: u64) -> Result<Header> {
    let raw = slice_at_lba(bytes, lba, SECTOR_SIZE as usize)?;
    if raw.get(..8) != Some(b"EFI PART")
        || read_u32(raw, 8)? != 0x0001_0000
        || read_u32(raw, 12)? != GPT_HEADER_SIZE as u32
        || read_u64(raw, 24)? != lba
        || read_u64(raw, 32)? != expected_backup
        || read_u64(raw, 40)? != 34
        || read_u64(raw, 48)? != DISK_SECTORS - 34
        || read_u64(raw, 72)? != expected_entries
        || read_u32(raw, 80)? != GPT_ENTRY_COUNT as u32
        || read_u32(raw, 84)? != GPT_ENTRY_SIZE as u32
    {
        return Err(ImageError::new("GPT header violates the fixed profile"));
    }
    let mut header = [0_u8; GPT_HEADER_SIZE];
    header.copy_from_slice(
        raw.get(..GPT_HEADER_SIZE)
            .ok_or_else(|| ImageError::new("truncated GPT header"))?,
    );
    let expected_crc = read_u32(&header, 16)?;
    header[16..20].fill(0);
    if CRC32.checksum(&header) != expected_crc {
        return Err(ImageError::new("GPT header checksum mismatch"));
    }
    if GptGuid::from_gpt_bytes(read_guid(raw, 56)?) != guid(DISK_GUID)? {
        return Err(ImageError::new("GPT disk GUID is not the fixed GUID"));
    }
    Ok(Header {
        entries_lba: expected_entries,
        entries_crc: read_u32(raw, 88)?,
    })
}

fn partition_range(
    entries: &[u8],
    index: usize,
    expected_type: &str,
    expected_guid: &str,
    expected: std::ops::Range<u64>,
) -> Result<std::ops::Range<u64>> {
    let offset = index
        .checked_mul(GPT_ENTRY_SIZE)
        .ok_or_else(|| ImageError::new("GPT entry offset overflow"))?;
    let entry = entries
        .get(offset..offset + GPT_ENTRY_SIZE)
        .ok_or_else(|| ImageError::new("truncated GPT entry"))?;
    if GptGuid::from_gpt_bytes(read_guid(entry, 0)?) != guid(expected_type)? {
        return Err(ImageError::new(
            "GPT partition type does not match the fixed profile",
        ));
    }
    let actual_guid = GptGuid::from_gpt_bytes(read_guid(entry, 16)?);
    if actual_guid != guid(expected_guid)? {
        return Err(ImageError::new(
            "GPT unique partition GUID does not match the fixed profile",
        ));
    }
    let first = read_u64(entry, 32)?;
    let last = read_u64(entry, 40)?;
    let end = last
        .checked_add(1)
        .ok_or_else(|| ImageError::new("GPT partition end overflow"))?;
    if first != expected.start || end != expected.end {
        return Err(ImageError::new(
            "GPT partition range does not match the fixed profile",
        ));
    }
    Ok(first..end)
}

fn verify_ext2(bytes: &[u8], root: &std::ops::Range<u64>) -> Result<()> {
    let root_offset =
        sectors_to_bytes(root.start).ok_or_else(|| ImageError::new("root offset overflow"))?;
    let super_offset = root_offset
        .checked_add(1024)
        .ok_or_else(|| ImageError::new("ext2 superblock offset overflow"))?;
    let super_offset = usize::try_from(super_offset)
        .map_err(|_| ImageError::new("ext2 superblock offset does not fit usize"))?;
    let superblock = bytes
        .get(super_offset..super_offset + 1024)
        .ok_or_else(|| ImageError::new("truncated ext2 superblock"))?;
    if read_u16(superblock, 56)? != 0xef53
        || read_u32(superblock, 0)? != 4_096
        || read_u32(superblock, 4)? != 32_768
        || read_u32(superblock, 20)? != 0
        || read_u32(superblock, 24)? != 2
        || read_u32(superblock, 32)? != 32_768
        || read_u32(superblock, 40)? != 4_096
        || read_u32(superblock, 76)? != 1
        || read_u16(superblock, 88)? != 256
        || read_u32(superblock, 92)? != 0
        || read_u32(superblock, 96)? != 2
        || read_u32(superblock, 100)? != 0
        || GptGuid(read_guid(superblock, 104)?) != guid(EXT2_UUID)?
    {
        return Err(ImageError::new(
            "ext2 filesystem violates the fixed profile",
        ));
    }
    Ok(())
}

fn verify_boot_config(bytes: &[u8], path: &Path) -> Result<()> {
    let esp = extracted_esp_path(path)?;
    extract_partition(bytes, ESP_RANGE.clone(), &esp)?;
    let output = Command::new("mtype")
        .args(["-i", path_string(&esp)?.as_str(), "::/EFI/RELAY/relay.cfg"])
        .output()
        .map_err(io_error)?;
    if !output.status.success() || output.stdout != crate::layout::root_config() {
        return Err(ImageError::new(
            "ESP relay.cfg does not contain the canonical root GUID",
        ));
    }
    Ok(())
}

fn extracted_esp_path(image: &Path) -> Result<std::path::PathBuf> {
    let parent = image.parent().unwrap_or(Path::new("."));
    let name = image
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ImageError::new("image name is not UTF-8"))?;
    Ok(parent.join(format!("{name}.esp.fat")))
}

fn extracted_root_path(image: &Path) -> Result<std::path::PathBuf> {
    let parent = image.parent().unwrap_or(Path::new("."));
    let name = image
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ImageError::new("image name is not UTF-8"))?;
    Ok(parent.join(format!("{name}.root.ext2")))
}

fn extract_root(bytes: &[u8], root: std::ops::Range<u64>, output: &Path) -> Result<()> {
    extract_partition(bytes, root, output)
}

fn extract_partition(bytes: &[u8], range: std::ops::Range<u64>, output: &Path) -> Result<()> {
    let start = usize::try_from(
        sectors_to_bytes(range.start).ok_or_else(|| ImageError::new("partition start overflow"))?,
    )
    .map_err(|_| ImageError::new("partition start does not fit usize"))?;
    let len = usize::try_from(
        sectors_to_bytes(
            range
                .end
                .checked_sub(range.start)
                .ok_or_else(|| ImageError::new("partition range underflow"))?,
        )
        .ok_or_else(|| ImageError::new("partition length overflow"))?,
    )
    .map_err(|_| ImageError::new("partition length does not fit usize"))?;
    let root_bytes = bytes
        .get(
            start
                ..start
                    .checked_add(len)
                    .ok_or_else(|| ImageError::new("partition extraction end overflow"))?,
        )
        .ok_or_else(|| ImageError::new("partition is truncated"))?;
    File::create(output)
        .map_err(io_error)?
        .write_all(root_bytes)
        .map_err(io_error)
}

fn slice_at_lba(bytes: &[u8], lba: u64, len: usize) -> Result<&[u8]> {
    let start = usize::try_from(
        sectors_to_bytes(lba).ok_or_else(|| ImageError::new("LBA offset overflow"))?,
    )
    .map_err(|_| ImageError::new("LBA offset does not fit usize"))?;
    let end = start
        .checked_add(len)
        .ok_or_else(|| ImageError::new("slice end overflow"))?;
    bytes
        .get(start..end)
        .ok_or_else(|| ImageError::new("image is truncated"))
}

fn read_guid(bytes: &[u8], offset: usize) -> Result<[u8; 16]> {
    let source = bytes
        .get(offset..offset + 16)
        .ok_or_else(|| ImageError::new("GUID read exceeds buffer"))?;
    let mut guid = [0; 16];
    guid.copy_from_slice(source);
    Ok(guid)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let source = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| ImageError::new("integer read exceeds buffer"))?;
    Ok(u16::from_le_bytes([source[0], source[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let source = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| ImageError::new("integer read exceeds buffer"))?;
    Ok(u32::from_le_bytes([
        source[0], source[1], source[2], source[3],
    ]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64> {
    let source = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| ImageError::new("integer read exceeds buffer"))?;
    Ok(u64::from_le_bytes([
        source[0], source[1], source[2], source[3], source[4], source[5], source[6], source[7],
    ]))
}

fn guid(value: &str) -> Result<GptGuid> {
    GptGuid::from_str(value).map_err(|_| ImageError::new(format!("invalid fixed GUID `{value}`")))
}

fn path_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| ImageError::new(format!("path is not UTF-8: {}", path.display())))
}

fn command_output(command: &mut Command, action: &str) -> Result<String> {
    let output = command.output().map_err(io_error)?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if output.status.success() {
        Ok(text)
    } else {
        Err(ImageError::new(format!(
            "{action} failed with {}:\n{text}",
            output.status
        )))
    }
}

fn io_error(error: io::Error) -> ImageError {
    ImageError::new(error.to_string())
}
