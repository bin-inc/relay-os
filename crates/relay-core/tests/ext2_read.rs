#![allow(dead_code)] // Shared fixture helpers are used by distinct integration-test crates.

mod support;

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    rc::Rc,
};

use relay_core::{
    block::{BlockDevice, BlockError, BlockGeometry},
    ext2::{Ext2, Ext2Error, MountMode},
    fs::{Name, NameError, NodeKind},
};
use support::{ext2_image::fixture_with_files, file_device::FileDevice};

thread_local! {
    static FAIL_NAME_ALLOCATION: Cell<bool> = const { Cell::new(false) };
}

struct FailNameAllocator;

#[global_allocator]
static ALLOCATOR: FailNameAllocator = FailNameAllocator;

unsafe impl GlobalAlloc for FailNameAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() <= 255 && FAIL_NAME_ALLOCATION.get() {
            core::ptr::null_mut()
        } else {
            // SAFETY: This allocator delegates all non-test allocations to System unchanged.
            unsafe { System.alloc(layout) }
        }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: Pointers passed to dealloc were allocated by System.
        unsafe { System.dealloc(pointer, layout) };
    }
}

fn fail_name_allocation(fail: bool) {
    FAIL_NAME_ALLOCATION.set(fail);
}

#[test]
fn name_reports_an_allocation_failure() {
    fail_name_allocation(true);
    let result = Name::new(b"file");
    fail_name_allocation(false);

    assert_eq!(result, Err(NameError::Allocation));
}

#[test]
fn listing_maps_name_allocation_failure_to_ext2_allocation() {
    let image = fixture_with_files(&[("file", b"relay")]).unwrap();
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();

    fail_name_allocation(true);
    let result = fs.read_dir(fs.root());
    fail_name_allocation(false);

    assert!(matches!(result, Err(Ext2Error::Allocation)));
}

#[test]
fn root_listing_returns_validated_entries_in_disk_order() {
    let image = fixture_with_files(&[("alpha", b"a"), ("beta", b"bb")]).unwrap();
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();

    assert_eq!(
        fs.read_dir(fs.root())
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>(),
        vec![Name::new(b"alpha").unwrap(), Name::new(b"beta").unwrap()],
    );
}

#[test]
fn listing_rejects_malformed_directory_records_before_returning_entries() {
    let mutations: &[fn(&mut support::ext2_image::Ext2Fixture)] = &[
        |image| image.set_root_entry_record_length("file", 0),
        |image| image.set_root_entry_record_length("file", 10),
        |image| image.set_root_entry_record_length("file", 4096),
        |image| image.set_root_entry_name_length("file", 255),
    ];

    for mutation in mutations {
        let mut image = fixture_with_files(&[("file", b"relay")]).unwrap();
        mutation(&mut image);
        let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();

        assert!(matches!(
            fs.read_dir(fs.root()),
            Err(Ext2Error::CorruptMetadata { .. }),
        ));
    }
}

#[test]
fn listing_rejects_invalid_live_directory_entries() {
    let mutations: &[fn(&mut support::ext2_image::Ext2Fixture)] = &[
        |image| image.set_root_entry_inode("file", 4_097),
        |image| image.set_root_entry_file_type("file", 7),
        |image| image.set_root_entry_file_type("file", 2),
        |image| image.set_root_entry_name_byte("file", 0, b'\n'),
    ];

    for mutation in mutations {
        let mut image = fixture_with_files(&[("file", b"relay")]).unwrap();
        mutation(&mut image);
        let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();

        assert!(matches!(
            fs.read_dir(fs.root()),
            Err(Ext2Error::CorruptMetadata { .. }),
        ));
    }
}

#[test]
fn listing_skips_valid_dot_dotdot_and_deleted_records() {
    let mut image = fixture_with_files(&[("live", b"relay"), ("gone", b"deleted")]).unwrap();
    image.set_root_entry_inode("gone", 0);
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();

    assert_eq!(
        fs.read_dir(fs.root())
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>(),
        vec![Name::new(b"live").unwrap()],
    );
}

#[test]
fn listing_validates_later_records_after_collecting_an_entry() {
    let mut image = fixture_with_files(&[("first", b"one"), ("later", b"two")]).unwrap();
    image.set_root_entry_record_length("later", 4);
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();

    assert!(matches!(
        fs.read_dir(fs.root()),
        Err(Ext2Error::CorruptMetadata {
            field: "directory_record_length"
        }),
    ));
}

#[test]
fn listing_rejects_regular_files_as_directories() {
    let image = fixture_with_files(&[("file", b"relay")]).unwrap();
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();
    let file = fs.lookup(fs.root(), &Name::new(b"file").unwrap()).unwrap();

    assert!(matches!(fs.read_dir(file), Err(Ext2Error::WrongNodeKind)));
}

#[test]
fn generated_root_readme_is_listed_and_read_without_mutation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/root.ext2");
    if !root.exists() {
        return;
    }
    let expected = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/root/README.txt"),
    )
    .unwrap();
    let mut fs = Ext2::mount(
        FileDevice::open_read_only(&root).unwrap(),
        MountMode::ReadOnly,
    )
    .unwrap();
    let entry = fs
        .read_dir(fs.root())
        .unwrap()
        .into_iter()
        .find(|entry| entry.name.as_bytes() == b"README.txt")
        .unwrap();
    let mut actual = vec![0; expected.len()];

    assert_eq!(
        fs.read_at(entry.node, 0, &mut actual).unwrap(),
        expected.len()
    );
    assert_eq!(actual, expected);

    let status = std::process::Command::new("e2fsck")
        .args(["-fn", root.to_str().unwrap()])
        .env("LC_ALL", "C")
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn metadata_reports_root_and_regular_file_details() {
    let image = fixture_with_files(&[("hello", b"relay")]).unwrap();
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();
    let hello = fs.lookup(fs.root(), &Name::new(b"hello").unwrap()).unwrap();

    let root = fs.metadata(fs.root()).unwrap();
    assert_eq!(root.kind, NodeKind::Directory);
    assert_eq!(root.len, 4096);
    assert_eq!(root.mode & 0o777, 0o755);

    let metadata = fs.metadata(hello).unwrap();
    assert_eq!(metadata.kind, NodeKind::Regular);
    assert_eq!(metadata.len, 5);
    assert_eq!(metadata.mode & 0o777, 0o644);
}

#[test]
fn lookup_returns_not_found_only_after_a_valid_directory_scan() {
    let image = fixture_with_files(&[("hello", b"relay")]).unwrap();
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();

    assert!(matches!(
        fs.lookup(fs.root(), &Name::new(b"missing").unwrap()),
        Err(Ext2Error::NotFound),
    ));
}

#[test]
fn lookup_rejects_a_regular_file_as_the_directory() {
    let image = fixture_with_files(&[("hello", b"relay")]).unwrap();
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();
    let hello = fs.lookup(fs.root(), &Name::new(b"hello").unwrap()).unwrap();

    assert!(matches!(
        fs.lookup(hello, &Name::new(b"missing").unwrap()),
        Err(Ext2Error::WrongNodeKind),
    ));
}

#[test]
fn read_crosses_an_ordinary_block_boundary() {
    let bytes = pattern(2 * 4096);
    let image = fixture_with_files(&[("blocks", &bytes)]).unwrap();
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();
    let node = fs
        .lookup(fs.root(), &Name::new(b"blocks").unwrap())
        .unwrap();
    let mut actual = [0; 2];

    assert_eq!(fs.read_at(node, 4095, &mut actual).unwrap(), 2);
    assert_eq!(actual, [0xff, 0x00]);
}

#[test]
fn read_crosses_direct_to_single_indirect() {
    let bytes = pattern(13 * 4096);
    let image = fixture_with_files(&[("boundary", &bytes)]).unwrap();
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();
    let node = fs
        .lookup(fs.root(), &Name::new(b"boundary").unwrap())
        .unwrap();
    let mut actual = [0; 2];

    assert_eq!(fs.read_at(node, 12 * 4096 - 1, &mut actual).unwrap(), 2);
    assert_eq!(actual, [0xff, 0x00]);
}

#[test]
fn read_at_eof_returns_zero_without_changing_the_destination() {
    let image = fixture_with_files(&[("hello", b"relay")]).unwrap();
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();
    let node = fs.lookup(fs.root(), &Name::new(b"hello").unwrap()).unwrap();
    let mut bytes = [0xaa; 4];

    assert_eq!(fs.read_at(node, 5, &mut bytes).unwrap(), 0);
    assert_eq!(bytes, [0xaa; 4]);
}

#[test]
fn read_rejects_directories() {
    let image = fixture_with_files(&[]).unwrap();
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();

    assert!(matches!(
        fs.read_at(fs.root(), 0, &mut [0; 1]),
        Err(Ext2Error::WrongNodeKind),
    ));
}

#[test]
fn empty_destination_returns_without_accessing_the_inode() {
    let image = fixture_with_files(&[("file", b"relay")]).unwrap();
    let reads = Rc::new(Cell::new(0));
    let device = TrackingDevice {
        inner: image.open().unwrap(),
        reads: reads.clone(),
        writes: Rc::new(Cell::new(0)),
        flushes: Rc::new(Cell::new(0)),
    };
    let mut fs = Ext2::mount(device, MountMode::ReadOnly).unwrap();
    let reads_before = reads.get();

    assert_eq!(fs.read_at(fs.root(), 0, &mut []), Ok(0));
    assert_eq!(reads.get(), reads_before);
}

#[test]
fn read_caps_the_final_copy_at_eof() {
    let bytes = pattern(4100);
    let image = fixture_with_files(&[("tail", &bytes)]).unwrap();
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();
    let node = fs.lookup(fs.root(), &Name::new(b"tail").unwrap()).unwrap();
    let mut actual = [0xaa; 8];

    assert_eq!(fs.read_at(node, 4095, &mut actual).unwrap(), 5);
    assert_eq!(&actual[..5], &[0xff, 0x00, 0x01, 0x02, 0x03]);
    assert_eq!(&actual[5..], &[0xaa; 3]);
}

#[test]
fn read_rejects_a_hole_below_file_length() {
    let mut image = fixture_with_files(&[("hole", &[0xff; 4096])]).unwrap();
    image.clear_first_data_pointer("hole");
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();
    let node = fs.lookup(fs.root(), &Name::new(b"hole").unwrap()).unwrap();

    assert!(matches!(
        fs.read_at(node, 0, &mut [0; 1]),
        Err(Ext2Error::SparseFile),
    ));
}

#[test]
fn read_rejects_an_out_of_range_indirect_pointer() {
    let bytes = pattern(13 * 4096);
    let mut image = fixture_with_files(&[("boundary", &bytes)]).unwrap();
    image.set_first_indirect_data_pointer("boundary", 32_768);
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();
    let node = fs
        .lookup(fs.root(), &Name::new(b"boundary").unwrap())
        .unwrap();

    assert!(matches!(
        fs.read_at(node, 12 * 4096, &mut [0; 1]),
        Err(Ext2Error::CorruptMetadata { .. }),
    ));
}

#[test]
fn read_rejects_structural_metadata_blocks_from_all_pointer_locations() {
    let direct_contents = pattern(4096);
    let indirect_contents = pattern(13 * 4096);
    let metadata_blocks = fixture_with_files(&[])
        .unwrap()
        .structural_metadata_blocks();

    for block in metadata_blocks {
        let mut direct = fixture_with_files(&[("direct", &direct_contents)]).unwrap();
        direct.set_first_data_pointer("direct", block);
        let mut fs = Ext2::mount(direct.open().unwrap(), MountMode::ReadOnly).unwrap();
        let node = fs
            .lookup(fs.root(), &Name::new(b"direct").unwrap())
            .unwrap();
        assert!(matches!(
            fs.read_at(node, 0, &mut [0; 1]),
            Err(Ext2Error::SparseFile
                | Ext2Error::CorruptMetadata {
                    field: "block_pointer"
                }),
        ));

        let mut indirect = fixture_with_files(&[("indirect", &indirect_contents)]).unwrap();
        indirect.set_indirect_pointer("indirect", block);
        let mut fs = Ext2::mount(indirect.open().unwrap(), MountMode::ReadOnly).unwrap();
        let node = fs
            .lookup(fs.root(), &Name::new(b"indirect").unwrap())
            .unwrap();
        assert!(matches!(
            fs.read_at(node, 12 * 4096, &mut [0; 1]),
            Err(Ext2Error::SparseFile
                | Ext2Error::CorruptMetadata {
                    field: "block_pointer"
                }),
        ));

        let mut indirect_data = fixture_with_files(&[("indirect", &indirect_contents)]).unwrap();
        indirect_data.set_first_indirect_data_pointer("indirect", block);
        let mut fs = Ext2::mount(indirect_data.open().unwrap(), MountMode::ReadOnly).unwrap();
        let node = fs
            .lookup(fs.root(), &Name::new(b"indirect").unwrap())
            .unwrap();
        assert!(matches!(
            fs.read_at(node, 12 * 4096, &mut [0; 1]),
            Err(Ext2Error::SparseFile
                | Ext2Error::CorruptMetadata {
                    field: "block_pointer"
                }),
        ));
    }
}

#[test]
fn metadata_rejects_oversized_unsupported_or_indirect_inodes() {
    let mutations: &[fn(&mut support::ext2_image::Ext2Fixture)] = &[
        |image| image.set_inode_file_size("file", 4_243_457),
        |image| image.set_inode_file_size_high("file", 1),
        |image| image.set_inode_mode("file", 0xa000),
        |image| image.set_inode_flags("file", 1),
        |image| image.set_double_indirect_pointer("file", 1),
    ];

    for mutation in mutations {
        let mut image = fixture_with_files(&[("file", b"relay")]).unwrap();
        mutation(&mut image);
        let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();
        assert!(matches!(
            fs.lookup(fs.root(), &Name::new(b"file").unwrap()),
            Err(Ext2Error::UnsupportedFile),
        ));
    }
}

#[test]
fn mount_lookup_and_reads_never_write_or_flush_the_device() {
    let image = fixture_with_files(&[("file", b"relay")]).unwrap();
    let writes = Rc::new(Cell::new(0));
    let flushes = Rc::new(Cell::new(0));
    let device = TrackingDevice {
        inner: image.open().unwrap(),
        reads: Rc::new(Cell::new(0)),
        writes: writes.clone(),
        flushes: flushes.clone(),
    };
    let mut fs = Ext2::mount(device, MountMode::ReadOnly).unwrap();
    let node = fs.lookup(fs.root(), &Name::new(b"file").unwrap()).unwrap();
    let mut bytes = [0; 5];

    let _ = fs.read_dir(fs.root()).unwrap();
    assert_eq!(fs.read_at(node, 0, &mut bytes), Ok(5));
    assert_eq!(bytes, *b"relay");
    assert_eq!(writes.get(), 0);
    assert_eq!(flushes.get(), 0);
}

#[test]
fn eof_read_does_not_access_a_data_block() {
    let image = fixture_with_files(&[("file", b"relay")]).unwrap();
    let reads = Rc::new(Cell::new(0));
    let device = TrackingDevice {
        inner: image.open().unwrap(),
        reads: reads.clone(),
        writes: Rc::new(Cell::new(0)),
        flushes: Rc::new(Cell::new(0)),
    };
    let mut fs = Ext2::mount(device, MountMode::ReadOnly).unwrap();
    let node = fs.lookup(fs.root(), &Name::new(b"file").unwrap()).unwrap();
    let reads_before = reads.get();

    assert_eq!(fs.read_at(node, 5, &mut [0; 1]), Ok(0));
    assert_eq!(reads.get(), reads_before + 1); // The inode is loaded but no data block is read.
}

#[test]
fn lookup_rejects_invalid_child_inodes_and_malformed_record_lengths() {
    let mutations: &[fn(&mut support::ext2_image::Ext2Fixture)] = &[
        |image| image.set_root_entry_inode("file", 4_097),
        |image| image.set_root_entry_record_length("file", 2),
        |image| image.set_root_entry_record_length("file", 4),
    ];

    for mutation in mutations {
        let mut image = fixture_with_files(&[("file", b"relay")]).unwrap();
        mutation(&mut image);
        let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();

        assert!(matches!(
            fs.lookup(fs.root(), &Name::new(b"file").unwrap()),
            Err(Ext2Error::CorruptMetadata { .. }),
        ));
    }
}

#[test]
fn lookup_validates_later_records_after_finding_the_requested_name() {
    let mut image = fixture_with_files(&[("first", b"relay"), ("later", b"relay")]).unwrap();
    image.set_root_entry_record_length("later", 4);
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();

    assert!(matches!(
        fs.lookup(fs.root(), &Name::new(b"first").unwrap()),
        Err(Ext2Error::CorruptMetadata {
            field: "directory_record_length"
        }),
    ));
}

#[test]
fn lookup_returns_the_first_of_duplicate_valid_names() {
    let mut image = fixture_with_files(&[("first", b"first"), ("later", b"later")]).unwrap();
    image.rename_root_entry("later", "first");
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();
    let node = fs.lookup(fs.root(), &Name::new(b"first").unwrap()).unwrap();
    let mut bytes = [0; 5];

    assert_eq!(fs.read_at(node, 0, &mut bytes), Ok(5));
    assert_eq!(bytes, *b"first");
}

#[test]
fn lookup_rejects_a_triple_indirect_inode() {
    let mut image = fixture_with_files(&[("file", b"relay")]).unwrap();
    image.set_triple_indirect_pointer("file", 1);
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();

    assert!(matches!(
        fs.lookup(fs.root(), &Name::new(b"file").unwrap()),
        Err(Ext2Error::UnsupportedFile),
    ));
}

#[test]
fn lookup_rejects_a_directory_file_type_that_disagrees_with_the_inode() {
    let mut image = fixture_with_files(&[("file", b"relay")]).unwrap();
    image.set_root_entry_file_type("file", 2);
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();

    assert!(matches!(
        fs.lookup(fs.root(), &Name::new(b"file").unwrap()),
        Err(Ext2Error::CorruptMetadata {
            field: "directory_file_type"
        }),
    ));
}

#[test]
fn lookup_validates_dot_entries_before_skipping_them() {
    let mut image = fixture_with_files(&[("file", b"relay")]).unwrap();
    image.set_root_entry_file_type(".", 1);
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();

    assert!(matches!(
        fs.lookup(fs.root(), &Name::new(b"file").unwrap()),
        Err(Ext2Error::CorruptMetadata {
            field: "directory_file_type"
        }),
    ));
}

#[test]
fn lookup_and_listing_ignore_shape_valid_deleted_records() {
    let mut image = fixture_with_files(&[("file", b"relay")]).unwrap();
    image.set_root_entry_inode("file", 0);
    image.set_root_entry_file_type("file", 0);
    let mut fs = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();

    assert!(matches!(
        fs.lookup(fs.root(), &Name::new(b"missing").unwrap()),
        Err(Ext2Error::NotFound),
    ));
    assert!(fs.read_dir(fs.root()).unwrap().is_empty());
}

fn pattern(length: usize) -> Vec<u8> {
    (0..length).map(|index| index as u8).collect()
}

struct TrackingDevice<D> {
    inner: D,
    reads: Rc<Cell<usize>>,
    writes: Rc<Cell<usize>>,
    flushes: Rc<Cell<usize>>,
}

impl<D: BlockDevice> BlockDevice for TrackingDevice<D> {
    fn geometry(&self) -> BlockGeometry {
        self.inner.geometry()
    }

    fn read_sectors(&mut self, first_lba: u64, dst: &mut [u8]) -> Result<(), BlockError> {
        self.reads.set(self.reads.get() + 1);
        self.inner.read_sectors(first_lba, dst)
    }

    fn write_sectors(&mut self, first_lba: u64, src: &[u8]) -> Result<(), BlockError> {
        self.writes.set(self.writes.get() + 1);
        self.inner.write_sectors(first_lba, src)
    }

    fn flush(&mut self) -> Result<(), BlockError> {
        self.flushes.set(self.flushes.get() + 1);
        self.inner.flush()
    }
}
