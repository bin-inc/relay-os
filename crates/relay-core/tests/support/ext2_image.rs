use std::{
    env,
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

use super::file_device::FileDevice;

const SUPERBLOCK_OFFSET: usize = 1024;
const GROUP_DESCRIPTOR_OFFSET: usize = 4096;
const MAGIC_OFFSET: usize = 56;
const STATE_OFFSET: usize = 58;
const REVISION_OFFSET: usize = 76;
const FIRST_DATA_BLOCK_OFFSET: usize = 20;
const LOG_BLOCK_SIZE_OFFSET: usize = 24;
const BLOCKS_COUNT_OFFSET: usize = 4;
const BLOCKS_PER_GROUP_OFFSET: usize = 32;
const INODES_COUNT_OFFSET: usize = 0;
const INODE_SIZE_OFFSET: usize = 88;
const FEATURE_COMPAT_OFFSET: usize = 92;
const FEATURE_INCOMPAT_OFFSET: usize = 96;
const FEATURE_RO_COMPAT_OFFSET: usize = 100;
const BLOCK_BITMAP_OFFSET: usize = 0;
const INODE_BITMAP_OFFSET: usize = 4;
const INODE_TABLE_OFFSET: usize = 8;

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy)]
pub enum FeatureField {
    Compatible,
    Incompatible,
    ReadOnlyCompatible,
}

#[derive(Debug)]
pub enum FixtureError {
    Io(std::io::Error),
    InvalidName,
    CommandFailed(String),
}

impl core::fmt::Display for FixtureError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "fixture I/O failed: {error}"),
            Self::InvalidName => formatter.write_str("fixture name is not a valid path component"),
            Self::CommandFailed(error) => write!(formatter, "fixture command failed: {error}"),
        }
    }
}

impl std::error::Error for FixtureError {}

impl From<std::io::Error> for FixtureError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl FeatureField {
    pub const ALL: [Self; 3] = [
        Self::Compatible,
        Self::Incompatible,
        Self::ReadOnlyCompatible,
    ];

    const fn offset(self) -> usize {
        match self {
            Self::Compatible => FEATURE_COMPAT_OFFSET,
            Self::Incompatible => FEATURE_INCOMPAT_OFFSET,
            Self::ReadOnlyCompatible => FEATURE_RO_COMPAT_OFFSET,
        }
    }

    const fn approved_mask(self) -> u32 {
        match self {
            Self::Incompatible => 2,
            Self::Compatible | Self::ReadOnlyCompatible => 0,
        }
    }
}

pub fn unsupported_feature_bits(field: FeatureField) -> impl Iterator<Item = u32> {
    (0..32).filter_map(move |index| {
        let bit = 1_u32 << index;
        (!field.approved_mask() & bit != 0).then_some(bit)
    })
}

pub struct Ext2Fixture {
    directory: PathBuf,
    image: PathBuf,
}

pub fn fixture_with_files(files: &[(&str, &[u8])]) -> Result<Ext2Fixture, FixtureError> {
    if files.iter().any(|(name, _)| !valid_component(name)) {
        return Err(FixtureError::InvalidName);
    }
    let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
    let directory = env::temp_dir().join(format!("relay-core-ext2-{}-{id}", std::process::id()));
    fs::create_dir(&directory)?;
    let image = directory.join("root.ext2");
    File::create(&image)?.set_len(128 * 1024 * 1024)?;

    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("mke2fs")
        .current_dir(&repository)
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
            "lazy_itable_init=0,nodiscard,root_owner=0:0",
            "-U",
            "52454c41-5900-4000-8000-000000000004",
        ])
        .arg(&image)
        .arg("32768")
        .env("MKE2FS_CONFIG", repository.join("tools/mke2fs.conf"))
        .env("E2FSPROGS_FAKE_TIME", "1788739200")
        .output()?;
    if !output.status.success() {
        return Err(FixtureError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }

    for (name, contents) in files {
        let source = directory.join(format!("source-{name}"));
        fs::write(&source, contents)?;
        let output = Command::new("debugfs")
            .args(["-w", "-R"])
            .arg(format!(
                "write \"{}\" \"/{}\"",
                debugfs_string(&source.display().to_string()),
                debugfs_string(name)
            ))
            .arg(&image)
            .env("E2FSPROGS_FAKE_TIME", "1788739200")
            .output()?;
        if !output.status.success() {
            return Err(FixtureError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ));
        }
    }

    Ok(Ext2Fixture { directory, image })
}

pub fn fixture_with_feature(field: FeatureField, bit: u32) -> Result<Ext2Fixture, FixtureError> {
    let mut fixture = fixture_with_files(&[])?;
    fixture.set_feature(field, field.approved_mask() | bit);
    Ok(fixture)
}

impl Ext2Fixture {
    pub fn open(&self) -> std::io::Result<FileDevice> {
        FileDevice::open(&self.image)
    }

    pub fn open_read_only(&self) -> std::io::Result<FileDevice> {
        FileDevice::open_read_only(&self.image)
    }

    pub fn contains_file(&self, name: &str) -> Result<bool, FixtureError> {
        if !valid_component(name) {
            return Err(FixtureError::InvalidName);
        }
        let output = Command::new("debugfs")
            .args(["-R"])
            .arg(format!("stat \"/{}\"", debugfs_string(name)))
            .arg(&self.image)
            .output()?;
        Ok(output.status.success())
    }

    pub fn mark_dirty(&mut self) {
        self.set_superblock_u16(STATE_OFFSET, 0);
    }

    pub fn mark_error(&mut self) {
        self.set_superblock_u16(STATE_OFFSET, 2);
    }

    pub fn set_magic(&mut self, value: u16) {
        self.set_superblock_u16(MAGIC_OFFSET, value);
    }

    pub fn set_revision(&mut self, value: u32) {
        self.set_superblock_u32(REVISION_OFFSET, value);
    }

    pub fn set_log_block_size(&mut self, value: u32) {
        self.set_superblock_u32(LOG_BLOCK_SIZE_OFFSET, value);
    }

    pub fn set_inode_size(&mut self, value: u16) {
        self.set_superblock_u16(INODE_SIZE_OFFSET, value);
    }

    pub fn set_block_count(&mut self, value: u32) {
        self.set_superblock_u32(BLOCKS_COUNT_OFFSET, value);
    }

    pub fn set_inode_count(&mut self, value: u32) {
        self.set_superblock_u32(INODES_COUNT_OFFSET, value);
    }

    pub fn set_blocks_per_group(&mut self, value: u32) {
        self.set_superblock_u32(BLOCKS_PER_GROUP_OFFSET, value);
    }

    pub fn set_first_data_block(&mut self, value: u32) {
        self.set_superblock_u32(FIRST_DATA_BLOCK_OFFSET, value);
    }

    pub fn set_feature(&mut self, field: FeatureField, value: u32) {
        self.set_superblock_u32(field.offset(), value);
    }

    pub fn set_block_bitmap_block(&mut self, value: u32) {
        self.set_group_descriptor_u32(BLOCK_BITMAP_OFFSET, value);
    }

    pub fn set_inode_bitmap_block(&mut self, value: u32) {
        self.set_group_descriptor_u32(INODE_BITMAP_OFFSET, value);
    }

    pub fn set_inode_table_block(&mut self, value: u32) {
        self.set_group_descriptor_u32(INODE_TABLE_OFFSET, value);
    }

    fn set_superblock_u16(&mut self, offset: usize, value: u16) {
        self.with_bytes_mut(|bytes| {
            bytes[SUPERBLOCK_OFFSET + offset..SUPERBLOCK_OFFSET + offset + 2]
                .copy_from_slice(&value.to_le_bytes());
        });
    }

    fn set_superblock_u32(&mut self, offset: usize, value: u32) {
        self.with_bytes_mut(|bytes| {
            bytes[SUPERBLOCK_OFFSET + offset..SUPERBLOCK_OFFSET + offset + 4]
                .copy_from_slice(&value.to_le_bytes());
        });
    }

    fn set_group_descriptor_u32(&mut self, offset: usize, value: u32) {
        self.with_bytes_mut(|bytes| {
            bytes[GROUP_DESCRIPTOR_OFFSET + offset..GROUP_DESCRIPTOR_OFFSET + offset + 4]
                .copy_from_slice(&value.to_le_bytes());
        });
    }

    fn with_bytes_mut(&mut self, mutate: impl FnOnce(&mut [u8])) {
        let mut file = File::options()
            .read(true)
            .write(true)
            .open(&self.image)
            .unwrap();
        let len = usize::try_from(file.metadata().unwrap().len()).unwrap();
        let mut bytes = vec![0; len];
        file.read_exact(&mut bytes).unwrap();
        mutate(&mut bytes);
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(&bytes).unwrap();
        file.sync_all().unwrap();
    }
}

fn valid_component(name: &str) -> bool {
    name.len() <= 255
        && !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|byte| (b' '..=b'~').contains(&byte) && byte != b'/')
}

fn debugfs_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

impl Drop for Ext2Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}
