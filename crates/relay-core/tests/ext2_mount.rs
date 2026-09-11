mod support;

use relay_core::{
    block::{BlockDevice, BlockError, BlockGeometry},
    ext2::{Ext2, Ext2Error, MountMode},
    fs::Name,
};
use support::ext2_image::{self, FeatureField, FixtureError, fixture_with_files};

#[test]
fn name_accepts_only_nonempty_printable_ascii_path_components() {
    assert_eq!(Name::new(b"README.txt").unwrap().as_bytes(), b"README.txt");
    assert!(Name::new(b"").is_err());
    assert!(Name::new(b"a/b").is_err());
    assert!(Name::new(b"a\0b").is_err());
    assert!(Name::new(&[0x7f]).is_err());
    assert!(Name::new(&[b'a'; 256]).is_err());
    assert!(Name::new(b".").is_err());
    assert!(Name::new(b"..").is_err());
}

#[test]
fn mount_accepts_the_controlled_ext2_profile() {
    let image = fixture_with_files(&[("hello", b"relay")]).unwrap();

    assert!(Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).is_ok());
}

#[test]
fn read_only_file_device_reads_and_rejects_writes() {
    let image = fixture_with_files(&[]).unwrap();
    let mut device = image.open_read_only().unwrap();
    let mut bytes = [0; 512];

    assert_eq!(device.read_sectors(0, &mut bytes), Ok(()));
    assert_eq!(
        device.write_sectors(0, &[0; 512]),
        Err(BlockError::ReadOnly)
    );
}

#[test]
fn fixture_file_names_follow_public_component_rules_and_quote_spaces() {
    let image =
        fixture_with_files(&[("hello world", b"relay"), ("quote\" file", b"relay")]).unwrap();

    assert!(image.contains_file("hello world").unwrap());
    assert!(image.contains_file("quote\" file").unwrap());
    let too_long = "a".repeat(256);
    for name in ["", ".", "..", "a/b", "a\0b", &too_long] {
        assert!(matches!(
            fixture_with_files(&[(name, b"relay")]),
            Err(FixtureError::InvalidName),
        ));
    }
}

#[test]
fn mount_rejects_any_unapproved_feature_bit() {
    for field in FeatureField::ALL {
        for bit in ext2_image::unsupported_feature_bits(field) {
            let image = ext2_image::fixture_with_feature(field, bit).unwrap();
            assert!(matches!(
                Ext2::mount(image.open().unwrap(), MountMode::ReadOnly),
                Err(Ext2Error::UnsupportedFeature { .. }),
            ));
        }
    }
}

#[test]
fn mount_rejects_a_missing_required_filetype_feature() {
    let mut image = fixture_with_files(&[]).unwrap();
    image.set_feature(FeatureField::Incompatible, 0);

    assert!(matches!(
        Ext2::mount(image.open().unwrap(), MountMode::ReadOnly),
        Err(Ext2Error::UnsupportedFeature { .. }),
    ));
}

#[test]
fn read_write_mount_rejects_dirty_or_error_marked_filesystems() {
    for mutation in [
        ext2_image::Ext2Fixture::mark_dirty,
        ext2_image::Ext2Fixture::mark_error,
    ] {
        let mut image = fixture_with_files(&[]).unwrap();
        mutation(&mut image);
        assert!(matches!(
            Ext2::mount(image.open().unwrap(), MountMode::ReadWrite),
            Err(Ext2Error::MountRequiresCleanFilesystem),
        ));
    }
}

#[test]
fn read_only_mount_allows_dirty_filesystems() {
    let mut image = fixture_with_files(&[]).unwrap();
    image.mark_dirty();

    assert!(Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).is_ok());
}

#[test]
fn mount_rejects_group_metadata_outside_the_only_block_group() {
    let mut image = fixture_with_files(&[]).unwrap();
    image.set_inode_table_block(32_768);

    assert!(matches!(
        Ext2::mount(image.open().unwrap(), MountMode::ReadOnly),
        Err(Ext2Error::CorruptMetadata { .. }),
    ));
}

#[test]
fn mount_rejects_invalid_profile_fields() {
    let mutations: &[fn(&mut ext2_image::Ext2Fixture)] = &[
        |image| image.set_magic(0),
        |image| image.set_revision(0),
        |image| image.set_log_block_size(1),
        |image| image.set_log_fragment_size(1),
        |image| image.set_inode_size(128),
        |image| image.set_block_count(32_767),
        |image| image.set_inode_count(4_095),
        |image| image.set_blocks_per_group(32_767),
        |image| image.set_fragments_per_group(32_767),
        |image| image.set_inodes_per_group(4_095),
        |image| image.set_first_data_block(1),
    ];
    for mutation in mutations {
        let mut image = fixture_with_files(&[]).unwrap();
        mutation(&mut image);
        assert!(matches!(
            Ext2::mount(image.open().unwrap(), MountMode::ReadOnly),
            Err(Ext2Error::UnsupportedProfile { .. }),
        ));
    }
}

#[test]
fn mount_rejects_invalid_bitmap_locations_and_metadata_overlap() {
    for mutation in [
        |image: &mut ext2_image::Ext2Fixture| image.set_block_bitmap_block(32_768),
        |image: &mut ext2_image::Ext2Fixture| image.set_inode_bitmap_block(32_768),
        |image: &mut ext2_image::Ext2Fixture| image.set_inode_table_block(1),
    ] {
        let mut image = fixture_with_files(&[]).unwrap();
        mutation(&mut image);
        assert!(matches!(
            Ext2::mount(image.open().unwrap(), MountMode::ReadOnly),
            Err(Ext2Error::CorruptMetadata { .. }),
        ));
    }
}

#[test]
fn mount_rejects_non_512_byte_sectors_without_reading() {
    let mut device = WrongSectorDevice { reads: 0 };

    assert!(matches!(
        Ext2::mount(&mut device, MountMode::ReadOnly),
        Err(Ext2Error::UnsupportedSector { sector_size: 4096 }),
    ));
    assert_eq!(device.reads, 0);
}

#[test]
fn mount_rejects_a_device_shorter_than_the_fixed_profile() {
    let image = fixture_with_files(&[]).unwrap();
    let device = ShortGeometryDevice {
        inner: image.open().unwrap(),
    };

    assert!(matches!(
        Ext2::mount(device, MountMode::ReadOnly),
        Err(Ext2Error::UnsupportedProfile {
            field: "device_length"
        }),
    ));
}

struct WrongSectorDevice {
    reads: usize,
}

impl BlockDevice for &mut WrongSectorDevice {
    fn geometry(&self) -> BlockGeometry {
        BlockGeometry {
            logical_sector_size: 4096,
            sector_count: 32_768,
        }
    }

    fn read_sectors(&mut self, _first_lba: u64, _dst: &mut [u8]) -> Result<(), BlockError> {
        self.reads += 1;
        Ok(())
    }

    fn write_sectors(&mut self, _first_lba: u64, _src: &[u8]) -> Result<(), BlockError> {
        unreachable!("mount must not write")
    }

    fn flush(&mut self) -> Result<(), BlockError> {
        unreachable!("mount must not flush")
    }
}

struct ShortGeometryDevice<D> {
    inner: D,
}

impl<D: BlockDevice> BlockDevice for ShortGeometryDevice<D> {
    fn geometry(&self) -> BlockGeometry {
        let mut geometry = self.inner.geometry();
        geometry.sector_count -= 1;
        geometry
    }

    fn read_sectors(&mut self, first_lba: u64, dst: &mut [u8]) -> Result<(), BlockError> {
        self.inner.read_sectors(first_lba, dst)
    }

    fn write_sectors(&mut self, first_lba: u64, src: &[u8]) -> Result<(), BlockError> {
        self.inner.write_sectors(first_lba, src)
    }

    fn flush(&mut self) -> Result<(), BlockError> {
        self.inner.flush()
    }
}
