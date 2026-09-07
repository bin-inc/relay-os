use std::env;
use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::time::{Duration, SystemTime};

use crc::{CRC_32_ISO_HDLC, Crc};
use relay_abi::GptGuid;

use crate::layout::{
    DISK_GUID, DISK_SECTORS, ESP_GUID, ESP_RANGE, EXT2_UUID, ROOT_GUID, ROOT_RANGE, SECTOR_SIZE,
    sectors_to_bytes,
};

const EFI_SYSTEM_PARTITION: &str = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b";
const LINUX_FILESYSTEM: &str = "0fc63daf-8483-4772-8e79-3d69d8477de4";
const GPT_HEADER_SIZE: usize = 92;
const GPT_ENTRY_COUNT: usize = 128;
const GPT_ENTRY_SIZE: usize = 128;
const CRC32: Crc<u32> = Crc::<u32>::new(&CRC_32_ISO_HDLC);
// mke2fs creates the root inode with mode 0755 by default; `root_perms` is not
// available in ubuntu-latest's e2fsprogs.
const MKE2FS_EXTENDED_OPTIONS: &str = "lazy_itable_init=0,nodiscard,root_owner=0:0";

pub type Result<T> = std::result::Result<T, ImageError>;

#[derive(Debug)]
pub struct ImageError(String);

impl ImageError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ImageError {}

pub fn build(output: &Path) -> Result<()> {
    let root = workspace_root()?;
    let epoch = source_date_epoch()?;
    let target = root.join("target");
    let work = target.join("image-work");
    if work.exists() {
        fs::remove_dir_all(&work).map_err(io_error)?;
    }
    fs::create_dir_all(&work).map_err(io_error)?;
    fs::create_dir_all(output.parent().unwrap_or(Path::new("."))).map_err(io_error)?;

    build_artifacts(&root)?;
    let artifacts = stage_artifacts(&root, &work, epoch)?;
    let esp = target.join("esp.fat");
    let root_fs = target.join("root.ext2");
    build_esp(&esp, &artifacts, epoch)?;
    build_root_filesystem(&root, &root_fs, &artifacts.readme, epoch)?;
    assemble_disk(output, &esp, &root_fs)
}

fn workspace_root() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| ImageError("xtask must reside directly below the workspace root".into()))
}

fn source_date_epoch() -> Result<u64> {
    match env::var("SOURCE_DATE_EPOCH") {
        Ok(value) => value
            .parse()
            .map_err(|_| ImageError("SOURCE_DATE_EPOCH must be an unsigned integer".into())),
        Err(env::VarError::NotPresent) => Ok(1_788_739_200),
        Err(env::VarError::NotUnicode(_)) => {
            Err(ImageError("SOURCE_DATE_EPOCH is not UTF-8".into()))
        }
    }
}

fn build_artifacts(root: &Path) -> Result<()> {
    run(
        Command::new("cargo").current_dir(root).args([
            "build",
            "-p",
            "relay-loader",
            "--target",
            "x86_64-unknown-uefi",
            "--locked",
        ]),
        "build UEFI loader",
    )?;
    run(
        Command::new("cargo").current_dir(root).args([
            "build",
            "-p",
            "relay-kernel",
            "--target",
            "x86_64-unknown-none",
            "--locked",
        ]),
        "build kernel",
    )
}

struct Artifacts {
    loader: PathBuf,
    kernel: PathBuf,
    config: PathBuf,
    readme: PathBuf,
}

fn stage_artifacts(root: &Path, work: &Path, epoch: u64) -> Result<Artifacts> {
    let target = root.join("target");
    let loader = stage_file(
        &target.join("x86_64-unknown-uefi/debug/relay-loader.efi"),
        &work.join("BOOTX64.EFI"),
        epoch,
    )?;
    let kernel = stage_file(
        &target.join("x86_64-unknown-none/debug/relay-kernel"),
        &work.join("kernel.elf"),
        epoch,
    )?;
    let config = work.join("relay.cfg");
    fs::write(&config, crate::layout::root_config()).map_err(io_error)?;
    set_timestamp(&config, epoch)?;
    let readme = stage_file(
        &root.join("assets/root/README.txt"),
        &work.join("README.txt"),
        epoch,
    )?;
    Ok(Artifacts {
        loader,
        kernel,
        config,
        readme,
    })
}

fn stage_file(source: &Path, destination: &Path, epoch: u64) -> Result<PathBuf> {
    fs::copy(source, destination).map_err(|error| {
        ImageError(format!(
            "copy {} to {}: {error}",
            source.display(),
            destination.display()
        ))
    })?;
    set_timestamp(destination, epoch)?;
    Ok(destination.to_path_buf())
}

fn set_timestamp(path: &Path, epoch: u64) -> Result<()> {
    let time = SystemTime::UNIX_EPOCH
        .checked_add(Duration::from_secs(epoch))
        .ok_or_else(|| ImageError("SOURCE_DATE_EPOCH is outside the system clock range".into()))?;
    File::open(path)
        .map_err(io_error)?
        .set_times(FileTimes::new().set_modified(time))
        .map_err(io_error)
}

fn build_esp(path: &Path, artifacts: &Artifacts, epoch: u64) -> Result<()> {
    create_zeroed(path, range_len(&ESP_RANGE)?)?;
    run(
        Command::new("mformat")
            .args([
                "-i",
                path_string(path)?.as_str(),
                "-F",
                "-v",
                "RELAYESP",
                "-N",
                "52454c41",
                "::",
            ])
            .env("SOURCE_DATE_EPOCH", epoch.to_string()),
        "format ESP",
    )?;
    run(
        Command::new("mmd").args([
            "-i",
            path_string(path)?.as_str(),
            "::/EFI",
            "::/EFI/BOOT",
            "::/EFI/RELAY",
        ]),
        "create ESP directories",
    )?;
    copy_to_esp(path, &artifacts.loader, "::/EFI/BOOT/BOOTX64.EFI")?;
    copy_to_esp(path, &artifacts.kernel, "::/EFI/RELAY/kernel.elf")?;
    copy_to_esp(path, &artifacts.config, "::/EFI/RELAY/relay.cfg")?;
    normalize_fat_timestamps(path, epoch)
}

fn copy_to_esp(image: &Path, source: &Path, destination: &str) -> Result<()> {
    run(
        Command::new("mcopy").args([
            "-i",
            path_string(image)?.as_str(),
            path_string(source)?.as_str(),
            destination,
        ]),
        "populate ESP",
    )
}

fn build_root_filesystem(root: &Path, path: &Path, readme: &Path, epoch: u64) -> Result<()> {
    create_zeroed(path, range_len(&ROOT_RANGE)?)?;
    let config = root.join("tools/mke2fs.conf");
    run(
        Command::new("mke2fs")
            .current_dir(root)
            .args([
                "-F",
                "-t",
                "ext2",
                "-b",
                "4096",
                "-g",
                "32768",
                "-I",
                "256",
                "-N",
                "4096",
                "-m",
                "0",
                "-O",
                "none,filetype",
                "-E",
                MKE2FS_EXTENDED_OPTIONS,
                "-U",
                EXT2_UUID,
                "target/root.ext2",
                "32768",
            ])
            .env("MKE2FS_CONFIG", config)
            .env("E2FSPROGS_FAKE_TIME", epoch.to_string())
            .env("SOURCE_DATE_EPOCH", epoch.to_string()),
        "format root ext2",
    )?;
    run(
        Command::new("debugfs")
            .current_dir(root)
            .args([
                "-w",
                "-R",
                &format!("write {} /README.txt", readme.display()),
                "target/root.ext2",
            ])
            .env("E2FSPROGS_FAKE_TIME", epoch.to_string())
            .env("SOURCE_DATE_EPOCH", epoch.to_string()),
        "seed root filesystem",
    )?;
    normalize_ext2_hash_seed(path)?;
    if path.metadata().map_err(io_error)?.len() != range_len(&ROOT_RANGE)? {
        return Err(ImageError(
            "formatted root filesystem has an unexpected size".into(),
        ));
    }
    Ok(())
}

fn normalize_ext2_hash_seed(path: &Path) -> Result<()> {
    let mut filesystem = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(io_error)?;
    filesystem
        .seek(SeekFrom::Start(1024 + 236))
        .map_err(io_error)?;
    filesystem
        .write_all(b"relay-os-hashkey")
        .map_err(io_error)?;
    filesystem.sync_all().map_err(io_error)
}

fn assemble_disk(output: &Path, esp: &Path, root: &Path) -> Result<()> {
    let disk_bytes = sectors_to_bytes(DISK_SECTORS)
        .ok_or_else(|| ImageError("disk byte length overflow".into()))?;
    create_zeroed(output, disk_bytes)?;
    let mut disk = OpenOptions::new()
        .read(true)
        .write(true)
        .open(output)
        .map_err(io_error)?;
    let entries = partition_entries()?;
    write_gpt(&mut disk, &entries)?;
    copy_partition(&mut disk, esp, ESP_RANGE.start)?;
    copy_partition(&mut disk, root, ROOT_RANGE.start)?;
    disk.sync_all().map_err(io_error)
}

fn create_zeroed(path: &Path, len: u64) -> Result<()> {
    let mut file = File::create(path).map_err(io_error)?;
    let zeros = [0_u8; 1024 * 1024];
    let mut remaining = len;
    while remaining != 0 {
        let count = usize::try_from(remaining.min(zeros.len() as u64))
            .map_err(|_| ImageError("zero-fill write length does not fit usize".into()))?;
        file.write_all(&zeros[..count]).map_err(io_error)?;
        remaining -= count as u64;
    }
    file.sync_all().map_err(io_error)
}

fn partition_entries() -> Result<[u8; GPT_ENTRY_COUNT * GPT_ENTRY_SIZE]> {
    let mut entries = [0; GPT_ENTRY_COUNT * GPT_ENTRY_SIZE];
    write_partition(
        &mut entries[..GPT_ENTRY_SIZE],
        EFI_SYSTEM_PARTITION,
        ESP_GUID,
        ESP_RANGE.clone(),
        "Relay ESP",
    )?;
    write_partition(
        &mut entries[GPT_ENTRY_SIZE..2 * GPT_ENTRY_SIZE],
        LINUX_FILESYSTEM,
        ROOT_GUID,
        ROOT_RANGE.clone(),
        "Relay Root",
    )?;
    Ok(entries)
}

fn write_partition(
    entry: &mut [u8],
    kind: &str,
    unique: &str,
    range: std::ops::Range<u64>,
    name: &str,
) -> Result<()> {
    entry[..16].copy_from_slice(&guid(kind)?.to_gpt_bytes());
    entry[16..32].copy_from_slice(&guid(unique)?.to_gpt_bytes());
    put_u64(entry, 32, range.start)?;
    put_u64(
        entry,
        40,
        range
            .end
            .checked_sub(1)
            .ok_or_else(|| ImageError("empty partition range".into()))?,
    )?;
    for (index, unit) in name.encode_utf16().enumerate() {
        let offset = 56usize
            .checked_add(
                index
                    .checked_mul(2)
                    .ok_or_else(|| ImageError("GPT name offset overflow".into()))?,
            )
            .ok_or_else(|| ImageError("GPT name offset overflow".into()))?;
        put_u16(entry, offset, unit)?;
    }
    Ok(())
}

fn write_gpt(disk: &mut File, entries: &[u8; GPT_ENTRY_COUNT * GPT_ENTRY_SIZE]) -> Result<()> {
    let entries_crc = CRC32.checksum(entries);
    seek_to_lba(disk, 2)?;
    disk.write_all(entries).map_err(io_error)?;
    let backup_entries_lba = DISK_SECTORS
        .checked_sub(33)
        .ok_or_else(|| ImageError("backup GPT placement underflow".into()))?;
    seek_to_lba(disk, backup_entries_lba)?;
    disk.write_all(entries).map_err(io_error)?;

    let primary = gpt_header(1, DISK_SECTORS - 1, 2, entries_crc)?;
    let backup = gpt_header(DISK_SECTORS - 1, 1, backup_entries_lba, entries_crc)?;
    seek_to_lba(disk, 1)?;
    disk.write_all(&primary).map_err(io_error)?;
    seek_to_lba(disk, DISK_SECTORS - 1)?;
    disk.write_all(&backup).map_err(io_error)?;

    seek_to_lba(disk, 0)?;
    let mut protective_mbr = [0_u8; SECTOR_SIZE as usize];
    protective_mbr[446 + 4] = 0xee;
    protective_mbr[446 + 8..446 + 12].copy_from_slice(&1_u32.to_le_bytes());
    protective_mbr[446 + 12..446 + 16].copy_from_slice(&u32::MAX.to_le_bytes());
    protective_mbr[510..512].copy_from_slice(&[0x55, 0xaa]);
    disk.write_all(&protective_mbr).map_err(io_error)
}

fn gpt_header(
    current_lba: u64,
    backup_lba: u64,
    entries_lba: u64,
    entries_crc: u32,
) -> Result<[u8; SECTOR_SIZE as usize]> {
    let mut header = [0_u8; SECTOR_SIZE as usize];
    header[..8].copy_from_slice(b"EFI PART");
    put_u32(&mut header, 8, 0x0001_0000)?;
    put_u32(&mut header, 12, GPT_HEADER_SIZE as u32)?;
    put_u64(&mut header, 24, current_lba)?;
    put_u64(&mut header, 32, backup_lba)?;
    put_u64(&mut header, 40, 34)?;
    put_u64(&mut header, 48, DISK_SECTORS - 34)?;
    header[56..72].copy_from_slice(&guid(DISK_GUID)?.to_gpt_bytes());
    put_u64(&mut header, 72, entries_lba)?;
    put_u32(&mut header, 80, GPT_ENTRY_COUNT as u32)?;
    put_u32(&mut header, 84, GPT_ENTRY_SIZE as u32)?;
    put_u32(&mut header, 88, entries_crc)?;
    let checksum = CRC32.checksum(&header[..GPT_HEADER_SIZE]);
    put_u32(&mut header, 16, checksum)?;
    Ok(header)
}

fn copy_partition(disk: &mut File, source: &Path, first_lba: u64) -> Result<()> {
    let bytes = source.metadata().map_err(io_error)?.len();
    let offset = sectors_to_bytes(first_lba)
        .ok_or_else(|| ImageError("partition placement overflow".into()))?;
    disk.seek(SeekFrom::Start(offset)).map_err(io_error)?;
    let mut source = File::open(source).map_err(io_error)?;
    let copied = io::copy(&mut source, disk).map_err(io_error)?;
    if copied != bytes {
        return Err(ImageError("short partition copy".into()));
    }
    Ok(())
}

fn normalize_fat_timestamps(path: &Path, epoch: u64) -> Result<()> {
    let (year, month, day, hour, minute, second) = fat_date_time(epoch)?;
    let date = ((year - 1980) << 9) | (u16::from(month) << 5) | u16::from(day);
    let time = (u16::from(hour) << 11) | (u16::from(minute) << 5) | u16::from(second / 2);
    let mut image = fs::read(path).map_err(io_error)?;
    let bytes_per_sector = u64::from(read_u16(&image, 11)?);
    let sectors_per_cluster = u64::from(
        *image
            .get(13)
            .ok_or_else(|| ImageError("FAT BPB is truncated".into()))?,
    );
    let reserved = u64::from(read_u16(&image, 14)?);
    let fats = u64::from(
        *image
            .get(16)
            .ok_or_else(|| ImageError("FAT BPB is truncated".into()))?,
    );
    let sectors_per_fat = u64::from(read_u32(&image, 36)?);
    let root_cluster = read_u32(&image, 44)?;
    if bytes_per_sector != SECTOR_SIZE
        || sectors_per_cluster == 0
        || fats == 0
        || sectors_per_fat == 0
        || root_cluster < 2
    {
        return Err(ImageError(
            "ESP FAT32 BPB has an unsupported geometry".into(),
        ));
    }
    let fat_offset = checked_offset(reserved, bytes_per_sector)?;
    let data_sector = reserved
        .checked_add(
            fats.checked_mul(sectors_per_fat)
                .ok_or_else(|| ImageError("FAT data sector overflow".into()))?,
        )
        .ok_or_else(|| ImageError("FAT data sector overflow".into()))?;
    let data_offset = checked_offset(data_sector, bytes_per_sector)?;
    let geometry = FatGeometry {
        fat_offset,
        data_offset,
        bytes_per_sector,
        sectors_per_cluster,
        date,
        time,
    };
    normalize_directory(&mut image, &geometry, root_cluster)?;
    fs::write(path, image).map_err(io_error)
}

struct FatGeometry {
    fat_offset: usize,
    data_offset: usize,
    bytes_per_sector: u64,
    sectors_per_cluster: u64,
    date: u16,
    time: u16,
}

fn normalize_directory(image: &mut [u8], geometry: &FatGeometry, first_cluster: u32) -> Result<()> {
    let cluster_bytes = usize::try_from(
        geometry
            .bytes_per_sector
            .checked_mul(geometry.sectors_per_cluster)
            .ok_or_else(|| ImageError("FAT cluster size overflow".into()))?,
    )
    .map_err(|_| ImageError("FAT cluster size does not fit usize".into()))?;
    let mut cluster = first_cluster;
    let mut remaining = image.len() / cluster_bytes + 1;
    while (2..0x0fff_fff8).contains(&cluster) && remaining != 0 {
        remaining -= 1;
        let index = usize::try_from(cluster - 2)
            .map_err(|_| ImageError("FAT cluster index overflow".into()))?;
        let start = geometry
            .data_offset
            .checked_add(
                index
                    .checked_mul(cluster_bytes)
                    .ok_or_else(|| ImageError("FAT directory offset overflow".into()))?,
            )
            .ok_or_else(|| ImageError("FAT directory offset overflow".into()))?;
        let end = start
            .checked_add(cluster_bytes)
            .ok_or_else(|| ImageError("FAT directory end overflow".into()))?;
        let children = {
            let entries = image
                .get_mut(start..end)
                .ok_or_else(|| ImageError("FAT directory is outside the ESP".into()))?;
            let mut children = Vec::new();
            for entry in entries.as_chunks_mut::<32>().0 {
                if entry[0] == 0x00 {
                    break;
                }
                if entry[0] == 0xe5 || entry[11] == 0x0f {
                    continue;
                }
                entry[14..16].copy_from_slice(&geometry.time.to_le_bytes());
                entry[16..18].copy_from_slice(&geometry.date.to_le_bytes());
                entry[18..20].copy_from_slice(&geometry.date.to_le_bytes());
                entry[22..24].copy_from_slice(&geometry.time.to_le_bytes());
                entry[24..26].copy_from_slice(&geometry.date.to_le_bytes());
                if let Some(child) = directory_child_cluster(entry) {
                    children.push(child);
                }
            }
            children
        };
        for child in children {
            normalize_directory(image, geometry, child)?;
        }
        let next_offset = geometry
            .fat_offset
            .checked_add(
                usize::try_from(cluster)
                    .map_err(|_| ImageError("FAT entry offset overflow".into()))?
                    .checked_mul(4)
                    .ok_or_else(|| ImageError("FAT entry offset overflow".into()))?,
            )
            .ok_or_else(|| ImageError("FAT entry offset overflow".into()))?;
        cluster = read_u32(image, next_offset)? & 0x0fff_ffff;
    }
    if remaining == 0 {
        return Err(ImageError("FAT directory cluster chain loops".into()));
    }
    Ok(())
}

fn directory_child_cluster(entry: &[u8]) -> Option<u32> {
    if entry.len() != 32 || entry[11] & 0x10 == 0 || entry[0] == b'.' {
        return None;
    }
    let child = u32::from(u16::from_le_bytes([entry[26], entry[27]]))
        | (u32::from(u16::from_le_bytes([entry[20], entry[21]])) << 16);
    (child >= 2).then_some(child)
}

fn fat_date_time(epoch: u64) -> Result<(u16, u8, u8, u8, u8, u8)> {
    let days = epoch / 86_400;
    let time = epoch % 86_400;
    let mut year = 1970_u16;
    let mut remaining = days;
    while remaining >= u64::from(days_in_year(year)) {
        remaining -= u64::from(days_in_year(year));
        year = year.checked_add(1).ok_or_else(|| {
            ImageError("SOURCE_DATE_EPOCH is outside FAT's supported range".into())
        })?;
    }
    if !(1980..=2107).contains(&year) {
        return Err(ImageError(
            "SOURCE_DATE_EPOCH is outside FAT's supported range".into(),
        ));
    }
    let mut month = 1_u8;
    while remaining >= u64::from(days_in_month(year, month)) {
        remaining -= u64::from(days_in_month(year, month));
        month = month
            .checked_add(1)
            .ok_or_else(|| ImageError("SOURCE_DATE_EPOCH month overflow".into()))?;
    }
    Ok((
        year,
        month,
        u8::try_from(remaining + 1)
            .map_err(|_| ImageError("SOURCE_DATE_EPOCH day overflow".into()))?,
        u8::try_from(time / 3600)
            .map_err(|_| ImageError("SOURCE_DATE_EPOCH hour overflow".into()))?,
        u8::try_from((time % 3600) / 60)
            .map_err(|_| ImageError("SOURCE_DATE_EPOCH minute overflow".into()))?,
        u8::try_from(time % 60)
            .map_err(|_| ImageError("SOURCE_DATE_EPOCH second overflow".into()))?,
    ))
}

fn days_in_year(year: u16) -> u16 {
    if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) {
        366
    } else {
        365
    }
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if days_in_year(year) == 366 {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

fn range_len(range: &std::ops::Range<u64>) -> Result<u64> {
    let sectors = range
        .end
        .checked_sub(range.start)
        .ok_or_else(|| ImageError("partition range underflow".into()))?;
    sectors_to_bytes(sectors).ok_or_else(|| ImageError("partition size overflow".into()))
}

fn checked_offset(sectors: u64, bytes_per_sector: u64) -> Result<usize> {
    usize::try_from(
        sectors
            .checked_mul(bytes_per_sector)
            .ok_or_else(|| ImageError("byte offset overflow".into()))?,
    )
    .map_err(|_| ImageError("byte offset does not fit usize".into()))
}

fn seek_to_lba(file: &mut File, lba: u64) -> Result<()> {
    file.seek(SeekFrom::Start(
        sectors_to_bytes(lba).ok_or_else(|| ImageError("LBA byte offset overflow".into()))?,
    ))
    .map_err(io_error)?;
    Ok(())
}

fn guid(value: &str) -> Result<GptGuid> {
    GptGuid::from_str(value).map_err(|_| ImageError(format!("invalid fixed GUID `{value}`")))
}

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) -> Result<()> {
    let destination = bytes
        .get_mut(offset..offset + 2)
        .ok_or_else(|| ImageError("integer write exceeds buffer".into()))?;
    destination.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<()> {
    let destination = bytes
        .get_mut(offset..offset + 4)
        .ok_or_else(|| ImageError("integer write exceeds buffer".into()))?;
    destination.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) -> Result<()> {
    let destination = bytes
        .get_mut(offset..offset + 8)
        .ok_or_else(|| ImageError("integer write exceeds buffer".into()))?;
    destination.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| ImageError("integer read exceeds buffer".into()))?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| ImageError("integer read exceeds buffer".into()))?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn path_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| ImageError(format!("path is not UTF-8: {}", path.display())))
}

fn run(command: &mut Command, action: &str) -> Result<()> {
    let output = command
        .output()
        .map_err(|error| ImageError(format!("{action}: {error}")))?;
    if output.status.success() {
        return Ok(());
    }
    Err(ImageError(format!(
        "{action} failed with {}:\n{}{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )))
}

fn io_error(error: io::Error) -> ImageError {
    ImageError(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{MKE2FS_EXTENDED_OPTIONS, directory_child_cluster};

    #[test]
    fn mke2fs_options_avoid_nonportable_root_permissions() {
        assert!(MKE2FS_EXTENDED_OPTIONS.contains("root_owner=0:0"));
        assert!(!MKE2FS_EXTENDED_OPTIONS.contains("root_perms"));
    }

    #[test]
    fn directory_child_cluster_ignores_dot_entries() {
        let mut dot = [0; 32];
        dot[..11].fill(b' ');
        dot[0] = b'.';
        dot[11] = 0x10;
        dot[26..28].copy_from_slice(&3_u16.to_le_bytes());
        assert_eq!(directory_child_cluster(&dot), None);

        let mut child = [0; 32];
        child[..11].fill(b' ');
        child[..4].copy_from_slice(b"BOOT");
        child[11] = 0x10;
        child[26..28].copy_from_slice(&4_u16.to_le_bytes());
        assert_eq!(directory_child_cluster(&child), Some(4));
    }
}
