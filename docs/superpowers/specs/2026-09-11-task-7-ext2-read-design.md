# Task 7 Ext2 Profile Validation And Read Path Design

## Status

Approved for implementation on 2026-09-11.

## Objective

Task 7 adds a safe, host-tested reader for Relay OS's fixed ext2 root
filesystem profile. It mounts a `BlockDevice` selected by Task 6, validates
all ext2 metadata before use, and exposes opaque nodes, metadata, name lookup,
directory listing, and offset reads for Task 8's VFS and read-only shell.

The reader accepts only the exact filesystem format created by the project's
image tooling. It does not try to offer best-effort compatibility with general
ext2 images.

## Module Boundaries

`relay-core::fs` defines the filesystem-facing types shared with future VFS
work:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeId(pub(crate) u32);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Name(alloc::vec::Vec<u8>);

pub struct Metadata {
    pub kind: NodeKind,
    pub len: u64,
    pub mode: u16,
}

pub enum NodeKind {
    Regular,
    Directory,
}

pub struct DirEntry {
    pub name: Name,
    pub node: NodeId,
    pub kind: NodeKind,
}
```

`Name::new` accepts nonempty printable ASCII names no longer than 255 bytes;
it rejects NUL, slash, and nonprintable bytes. Ext2's on-disk `.` and `..`
entries are parsed internally but are not accepted as names for ordinary
external lookup. `Name::as_bytes` exposes the validated bytes for comparisons
and rendering. `NodeId` is opaque and always represents an inode number in the
mounted filesystem's range `2..=4,096`; callers cannot construct it directly.

`relay-core::ext2` contains `Ext2<D>` and its public mount/read API.
`on_disk` contains checked byte-slice readers and ext2 record offsets only;
it never creates packed references. `validate` reads the superblock and group
descriptor, verifies the exact supported profile, and produces validated
geometry. `inode` translates valid inode IDs and logical file blocks into
checked device ranges. `directory` validates and iterates directory records.
Each module returns typed `Ext2Error` values rather than panicking for media
input.

```rust
pub enum MountMode {
    ReadOnly,
    ReadWrite,
}

impl<D: BlockDevice> Ext2<D> {
    pub fn mount(device: D, mode: MountMode) -> Result<Self, Ext2Error>;
    pub fn root(&self) -> NodeId;
    pub fn metadata(&mut self, node: NodeId) -> Result<Metadata, Ext2Error>;
    pub fn lookup(&mut self, dir: NodeId, name: &Name) -> Result<NodeId, Ext2Error>;
    pub fn read_dir(&mut self, dir: NodeId) -> Result<alloc::vec::Vec<DirEntry>, Ext2Error>;
    pub fn read_at(
        &mut self,
        node: NodeId,
        offset: u64,
        dst: &mut [u8],
    ) -> Result<usize, Ext2Error>;
}
```

`read_dir` returns validated entry names, node IDs, and supported kinds in
on-disk order, omitting the `.` and `..` entries. `lookup` requires a directory
node, compares names byte-for-byte, and returns `NotFound` if no matching
entry exists. These operations do not mutate media.

## Fixed Ext2 Profile

Mount requires a `BlockDevice` with 512-byte logical sectors and reads ext2
blocks in exact eight-sector transfers. It validates the following profile:

- Ext2 magic `0xef53`, dynamic revision 1, and 256-byte inodes.
- 4 KiB block and fragment sizes, 32,768 blocks total, 32,768 blocks per group,
  one group, and 4,096 inodes.
- `s_first_data_block == 0`, root inode 2, and valid group descriptor metadata
  block locations within the sole block group.
- Compatible feature bits clear, incompatible features containing exactly
  `FILETYPE`, and read-only-compatible feature bits clear.
- No journal, no extents, no metadata checksums, no sparse-super variants, and
  no other unapproved feature or geometry.

The reader validates that the superblock, group descriptor table, block bitmap,
inode bitmap, and inode table are inside the filesystem's one block group and
do not overlap invalid ranges. All block, inode, byte-offset, sector-count,
and allocation arithmetic is checked before I/O.

Read-only mounts may inspect a dirty or error-marked filesystem. Read-write
mount requests must reject a filesystem lacking `EXT2_VALID_FS` or containing
an ext2 error state, because Task 9 must not mutate media requiring host
repair. Task 7 itself performs no writes in either mode.

## Inodes, Directories, And File Reads

Inode records are copied from checked byte slices and interpreted by explicit
little-endian accessors. Supported inode kinds are regular files and
directories. Unsupported modes, reserved inode IDs, invalid inode numbers,
and unsupported inode flags return an error. The reader rejects double- and
triple-indirect pointers, and rejects sparse holes below `i_size`; every logical
file block below the file length must resolve to a valid nonzero data block.

Regular-file reads support exactly twelve direct pointers and one singly
indirect pointer block, yielding the milestone maximum of 4,243,456 bytes.
`read_at` returns zero at or beyond EOF, otherwise copies at most the available
bytes and requested destination length. It may cross 4 KiB block boundaries and
the direct-to-indirect boundary. Each referenced data or indirect block is
validated against filesystem geometry before it is read.

Directories use linear `FILETYPE` entries. For each entry, the implementation
requires nonzero, four-byte-aligned `rec_len`; `rec_len` wholly contained in
the current 4 KiB directory block; `name_len` bounded by the record; a valid
nonzero inode for live entries; and a file type matching the referenced regular
file or directory inode. Deleted entries with inode zero are skipped only when
their record shape is valid. Unsupported file types and malformed records fail
the operation rather than being ignored. A live entry name must meet the same
`Name` printable-ASCII contract; the reader rejects unsupported names instead
of exposing names that later VFS and shell layers cannot represent.

## Errors And Resource Limits

`Ext2Error` distinguishes underlying `BlockError` from unsupported profile,
corrupt metadata, invalid node type, missing name, and read-only mount-state
errors. Media-controlled sizes are capped by the fixed profile: 4 KiB block
buffers, 256-byte inode records, one 4 KiB indirect block, and directory
results bounded by the maximum directory data the profile can address. All
fallible `Vec` growth uses reservation errors mapped to a typed resource error.

No raw media references are retained after a device read, no unsafe code is
required, and expected filesystem errors never invoke allocation panics or
unbounded scans. Directory traversal and singly-indirect pointer iteration are
bounded by known per-block and file-size limits.

## Testing

Test support uses a hybrid fixture strategy:

- `file_device` wraps disposable host files as exact 512-byte `BlockDevice`
  instances for integration tests.
- `ext2_image` builds valid disposable images with the repository's
  `tools/mke2fs.conf` and the same `mke2fs` profile used by `xtask`, then
  populates fixture files with `debugfs`.
- Byte-level fixture mutations target exact superblock, group descriptor,
  inode, indirect-block, and directory-record corruption cases.

Mount tests cover every unapproved feature bit, invalid profile dimensions,
metadata range errors, dirty/error-marked read-write rejection, and valid
read-only mounting. Read tests cover root lookup and listing, regular and
directory metadata, direct blocks, direct-to-singly-indirect reads, EOF and
cross-block offsets, bad pointers, sparse holes, bad inode modes, malformed
directory records, and file-type mismatches. Valid generated fixtures and the
project `target/root.ext2` must pass host `e2fsck -fn` unchanged after reader
operations.

Verification runs:

```bash
cargo test -p relay-core --test ext2_mount --test ext2_read --locked
LC_ALL=C e2fsck -fn target/root.ext2
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

## Non-Goals

Task 7 does not mutate ext2 metadata, allocate blocks or inodes, create or
remove directory entries, implement VFS paths or shell commands, support links
or special files, cache blocks, repair filesystems, or integrate the reader
with USB/kernel runtime storage. Those are later tasks.
