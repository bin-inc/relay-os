#![allow(dead_code)] // Shared fixture helpers are used by distinct integration-test crates.

mod support;

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

use relay_core::{
    ext2::{Ext2, MountMode},
    fs::{DirEntry, Metadata, Name, NodeId, NodeKind},
    vfs::{FsError, Vfs, VfsError},
};
use support::ext2_image::{fixture_with_directory, fixture_with_files};

thread_local! {
    static FAIL_CWD_PATH_ALLOCATION: Cell<bool> = const { Cell::new(false) };
}

struct FailCwdPathAllocator;

#[global_allocator]
static ALLOCATOR: FailCwdPathAllocator = FailCwdPathAllocator;

unsafe impl GlobalAlloc for FailCwdPathAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() == 5 && FAIL_CWD_PATH_ALLOCATION.get() {
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

fn fixture_vfs<'a>(files: &[(&'a str, &'a [u8])]) -> Vfs<Ext2<support::file_device::FileDevice>> {
    let image = fixture_with_files(files).unwrap();
    Vfs::new(Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap())
}

fn fixture_vfs_with_directory(
    directory: &str,
    file: &str,
    contents: &[u8],
) -> Vfs<Ext2<support::file_device::FileDevice>> {
    let image = fixture_with_directory(directory, file, contents).unwrap();
    Vfs::new(Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap())
}

#[test]
fn traversal_does_not_lexically_cancel_a_missing_component() {
    let mut vfs = fixture_vfs(&[("file", b"relay")]);
    let cwd = vfs.initial_cwd();

    assert!(matches!(
        vfs.resolve(&cwd, "missing/../file"),
        Err(VfsError::Fs(FsError::NotFound))
    ));
}

#[test]
fn resolution_normalizes_root_relative_dot_and_parent_components() {
    let mut vfs = fixture_vfs_with_directory("docs", "guide", b"read me");
    let cwd = vfs.initial_cwd();
    let docs = vfs.change_dir(&cwd, "/docs/./").unwrap();
    let root = vfs.change_dir(&docs, "../../").unwrap();
    let docs_again = vfs.change_dir(&root, "docs//").unwrap();

    assert_eq!(vfs.cwd_path(&docs).unwrap(), b"/docs");
    assert_eq!(vfs.cwd_path(&root).unwrap(), b"/");
    assert_eq!(vfs.cwd_path(&docs_again).unwrap(), b"/docs");
}

#[test]
fn trailing_slash_requires_a_directory() {
    let mut vfs = fixture_vfs(&[("file", b"relay")]);
    let cwd = vfs.initial_cwd();

    assert!(matches!(
        vfs.resolve(&cwd, "file/"),
        Err(VfsError::NotDirectory)
    ));
}

#[test]
fn resolution_ignores_repeated_separators_and_dots_at_every_position() {
    let mut vfs = fixture_vfs_with_directory("docs", "guide", b"read me");
    let cwd = vfs.initial_cwd();
    let docs = vfs.change_dir(&cwd, "./docs/./").unwrap();

    assert_eq!(vfs.cwd_path(&docs).unwrap(), b"/docs");
    assert_eq!(
        vfs.resolve(&cwd, "//./docs///./guide/.")
            .map(|_| ())
            .unwrap(),
        ()
    );
}

#[test]
fn parent_components_are_clamped_at_root() {
    let mut vfs = fixture_vfs_with_directory("docs", "guide", b"read me");
    let cwd = vfs.initial_cwd();
    let root = vfs.change_dir(&cwd, "../../..").unwrap();
    let docs = vfs.change_dir(&root, "docs").unwrap();

    assert_eq!(vfs.cwd_path(&root).unwrap(), b"/");
    assert_eq!(vfs.cwd_path(&docs).unwrap(), b"/docs");
}

#[test]
fn relative_paths_start_from_the_supplied_cwd() {
    let mut vfs = fixture_vfs_with_directory("docs", "guide", b"read me");
    let root = vfs.initial_cwd();
    let docs = vfs.change_dir(&root, "docs").unwrap();
    let mut contents = Vec::new();

    vfs.read_file(&docs, "guide", |bytes| {
        contents.extend_from_slice(bytes);
        Ok(())
    })
    .unwrap();

    assert_eq!(contents, b"read me");
}

#[test]
fn missing_final_and_intermediate_nodes_return_not_found() {
    let mut vfs = fixture_vfs_with_directory("docs", "guide", b"read me");
    let cwd = vfs.initial_cwd();

    for path in ["missing", "missing/guide", "docs/missing"] {
        assert!(matches!(
            vfs.resolve(&cwd, path),
            Err(VfsError::Fs(FsError::NotFound))
        ));
    }
}

#[test]
fn regular_files_cannot_be_intermediate_directories() {
    let mut vfs = fixture_vfs(&[("file", b"relay")]);
    let cwd = vfs.initial_cwd();

    for path in ["file/child", "file/.."] {
        assert!(matches!(
            vfs.resolve(&cwd, path),
            Err(VfsError::NotDirectory)
        ));
    }
}

#[test]
fn resolution_rejects_non_ascii_and_overlong_paths() {
    let mut vfs = fixture_vfs(&[]);
    let cwd = vfs.initial_cwd();

    assert_eq!(vfs.resolve(&cwd, "caf\u{e9}"), Err(VfsError::InvalidPath));
    assert_eq!(
        vfs.resolve(&cwd, &"a".repeat(4097)),
        Err(VfsError::InvalidPath)
    );
}

#[test]
fn slash_only_paths_resolve_to_root_and_list_the_current_directory() {
    let mut vfs = fixture_vfs(&[("file", b"relay")]);
    let cwd = vfs.initial_cwd();

    assert!(vfs.resolve(&cwd, "///").is_ok());
    assert_eq!(
        vfs.list(&cwd, Some("///"))
            .unwrap()
            .into_iter()
            .map(|entry| entry.name.as_bytes().to_vec())
            .collect::<Vec<_>>(),
        vec![b"file".to_vec()]
    );
    assert_eq!(
        vfs.list(&cwd, None)
            .unwrap()
            .into_iter()
            .map(|entry| entry.name.as_bytes().to_vec())
            .collect::<Vec<_>>(),
        vec![b"file".to_vec()]
    );
}

#[test]
fn read_file_preserves_binary_bytes() {
    let bytes = [0, 9, 10, 13, 127, 255];
    let mut vfs = fixture_vfs(&[("binary", &bytes)]);
    let cwd = vfs.initial_cwd();
    let mut contents = Vec::new();

    vfs.read_file(&cwd, "binary", |chunk| {
        contents.extend_from_slice(chunk);
        Ok(())
    })
    .unwrap();

    assert_eq!(contents, bytes);
}

#[test]
fn filesystem_errors_are_preserved_by_vfs() {
    for error in [
        FsError::NotFound,
        FsError::WrongNodeKind,
        FsError::Corrupt,
        FsError::Unsupported,
        FsError::Allocation,
        FsError::Io,
    ] {
        let mut vfs = Vfs::new(ErrorFileSystem {
            root: fixture_node(),
            error,
        });
        let cwd = vfs.initial_cwd();

        assert_eq!(vfs.list(&cwd, None), Err(VfsError::Fs(error)));
    }
}

#[test]
fn list_and_read_file_require_their_respective_node_kinds() {
    let mut vfs = fixture_vfs_with_directory("docs", "guide", b"read me");
    let cwd = vfs.initial_cwd();

    assert_eq!(
        vfs.list(&cwd, Some("docs/guide")),
        Err(VfsError::NotDirectory)
    );
    assert_eq!(
        vfs.read_file(&cwd, "docs", |_| Ok(())),
        Err(VfsError::NotRegularFile)
    );
}

#[test]
fn read_file_stops_when_the_output_reports_an_error() {
    let mut vfs = fixture_vfs(&[("file", b"relay")]);
    let cwd = vfs.initial_cwd();

    assert_eq!(
        vfs.read_file(&cwd, "file", |_| Err(VfsError::Allocation)),
        Err(VfsError::Allocation)
    );
}

#[test]
fn read_file_uses_at_most_a_four_kib_buffer_per_callback() {
    let bytes = (0..8193).map(|index| index as u8).collect::<Vec<_>>();
    let (root, file) = fixture_nodes();
    let mut vfs = Vfs::new(StreamingFileSystem {
        root,
        file,
        contents: bytes.clone(),
    });
    let cwd = vfs.initial_cwd();
    let mut chunks = Vec::new();
    let mut contents = Vec::new();

    vfs.read_file(&cwd, "file", |chunk| {
        chunks.push(chunk.len());
        contents.extend_from_slice(chunk);
        Ok(())
    })
    .unwrap();

    assert_eq!(chunks, vec![4096, 4096, 1]);
    assert_eq!(contents, bytes);
}

#[test]
fn read_file_rejects_an_overreported_read_without_panicking() {
    let (root, file) = fixture_nodes();
    let mut vfs = Vfs::new(OverreportingFileSystem { root, file });
    let cwd = vfs.initial_cwd();

    assert_eq!(
        vfs.read_file(&cwd, "file", |_| Ok(())),
        Err(VfsError::Fs(FsError::Corrupt))
    );
}

#[test]
fn cwd_path_reports_output_allocation_failure() {
    let mut vfs = fixture_vfs_with_directory("docs", "guide", b"read me");
    let root = vfs.initial_cwd();
    let docs = vfs.change_dir(&root, "docs").unwrap();

    FAIL_CWD_PATH_ALLOCATION.set(true);
    let result = vfs.cwd_path(&docs);
    FAIL_CWD_PATH_ALLOCATION.set(false);

    assert_eq!(result, Err(VfsError::Allocation));
}

#[test]
fn change_dir_rejects_canonical_cwds_longer_than_four_kib() {
    let root = fixture_node();
    let mut vfs = Vfs::new(DirectoryFileSystem { root });
    let mut cwd = vfs.initial_cwd();
    let component = "a".repeat(255);

    for _ in 0..16 {
        cwd = vfs.change_dir(&cwd, &component).unwrap();
    }
    assert_eq!(vfs.cwd_path(&cwd).unwrap().len(), 4096);
    assert_eq!(vfs.change_dir(&cwd, &component), Err(VfsError::InvalidPath));
}

struct StreamingFileSystem {
    root: NodeId,
    file: NodeId,
    contents: Vec<u8>,
}

impl relay_core::vfs::FileSystem for StreamingFileSystem {
    fn root(&self) -> NodeId {
        self.root
    }

    fn metadata(&mut self, node: NodeId) -> Result<Metadata, FsError> {
        Ok(Metadata {
            kind: if node == self.root {
                NodeKind::Directory
            } else {
                NodeKind::Regular
            },
            len: self.contents.len() as u64,
            mode: 0,
        })
    }

    fn lookup(&mut self, _: NodeId, name: &Name) -> Result<NodeId, FsError> {
        if name.as_bytes() == b"file" {
            Ok(self.file)
        } else {
            Err(FsError::NotFound)
        }
    }

    fn read_dir(&mut self, _: NodeId) -> Result<Vec<DirEntry>, FsError> {
        Err(FsError::WrongNodeKind)
    }

    fn read_at(&mut self, _: NodeId, offset: u64, dst: &mut [u8]) -> Result<usize, FsError> {
        let start = usize::try_from(offset).unwrap();
        let bytes = &self.contents[start..];
        let read = bytes.len().min(dst.len());
        dst[..read].copy_from_slice(&bytes[..read]);
        Ok(read)
    }
}

struct OverreportingFileSystem {
    root: NodeId,
    file: NodeId,
}

struct DirectoryFileSystem {
    root: NodeId,
}

impl relay_core::vfs::FileSystem for DirectoryFileSystem {
    fn root(&self) -> NodeId {
        self.root
    }

    fn metadata(&mut self, _: NodeId) -> Result<Metadata, FsError> {
        Ok(Metadata {
            kind: NodeKind::Directory,
            len: 0,
            mode: 0,
        })
    }

    fn lookup(&mut self, _: NodeId, _: &Name) -> Result<NodeId, FsError> {
        Ok(self.root)
    }

    fn read_dir(&mut self, _: NodeId) -> Result<Vec<DirEntry>, FsError> {
        Ok(Vec::new())
    }

    fn read_at(&mut self, _: NodeId, _: u64, _: &mut [u8]) -> Result<usize, FsError> {
        Err(FsError::WrongNodeKind)
    }
}

impl relay_core::vfs::FileSystem for OverreportingFileSystem {
    fn root(&self) -> NodeId {
        self.root
    }

    fn metadata(&mut self, node: NodeId) -> Result<Metadata, FsError> {
        Ok(Metadata {
            kind: if node == self.root {
                NodeKind::Directory
            } else {
                NodeKind::Regular
            },
            len: 1,
            mode: 0,
        })
    }

    fn lookup(&mut self, _: NodeId, name: &Name) -> Result<NodeId, FsError> {
        if name.as_bytes() == b"file" {
            Ok(self.file)
        } else {
            Err(FsError::NotFound)
        }
    }

    fn read_dir(&mut self, _: NodeId) -> Result<Vec<DirEntry>, FsError> {
        Err(FsError::WrongNodeKind)
    }

    fn read_at(&mut self, _: NodeId, _: u64, dst: &mut [u8]) -> Result<usize, FsError> {
        Ok(dst.len() + 1)
    }
}

fn fixture_node() -> NodeId {
    let image = fixture_with_files(&[]).unwrap();
    Ext2::mount(image.open().unwrap(), MountMode::ReadOnly)
        .unwrap()
        .root()
}

fn fixture_nodes() -> (NodeId, NodeId) {
    let image = fixture_with_files(&[("file", b"")]).unwrap();
    let mut filesystem = Ext2::mount(image.open().unwrap(), MountMode::ReadOnly).unwrap();
    let root = filesystem.root();
    let file = filesystem
        .lookup(root, &Name::new(b"file").unwrap())
        .unwrap();
    (root, file)
}

struct ErrorFileSystem {
    root: NodeId,
    error: FsError,
}

impl relay_core::vfs::FileSystem for ErrorFileSystem {
    fn root(&self) -> NodeId {
        self.root
    }

    fn metadata(&mut self, _: NodeId) -> Result<Metadata, FsError> {
        Err(self.error)
    }

    fn lookup(&mut self, _: NodeId, _: &Name) -> Result<NodeId, FsError> {
        Err(self.error)
    }

    fn read_dir(&mut self, _: NodeId) -> Result<Vec<DirEntry>, FsError> {
        Err(self.error)
    }

    fn read_at(&mut self, _: NodeId, _: u64, _: &mut [u8]) -> Result<usize, FsError> {
        Err(self.error)
    }
}
