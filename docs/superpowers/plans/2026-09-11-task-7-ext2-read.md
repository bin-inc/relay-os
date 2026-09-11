# Task 7 Ext2 Profile Validation And Read Path Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a safe, host-tested read-only ext2 implementation for Relay OS's fixed root-filesystem profile.

**Architecture:** `relay-core::fs` owns filesystem-facing opaque names, nodes, and metadata. `relay-core::ext2` separates fixed-profile mount validation and safe on-disk byte decoding from inode block mapping and directory traversal. It consumes only `BlockDevice`, so later USB storage can supply the same interface without changing filesystem logic.

**Tech Stack:** Rust 1.98.1, edition 2024, `no_std` plus `alloc`, existing `relay-core::block`, host test support using `std`, `mke2fs`, `debugfs`, and `e2fsck` from e2fsprogs 1.47.2.

**Spec:** `docs/superpowers/specs/2026-09-11-task-7-ext2-read-design.md`

## Global Constraints

- `relay-core` remains `#![no_std]`; host filesystem/process access belongs only in integration-test support.
- The reader supports exactly 512-byte logical sectors and 4 KiB ext2 blocks, read as exact eight-sector transfers.
- The accepted ext2 profile is dynamic revision 1, 32,768 blocks, 32,768 blocks per group, one group, 4,096 inodes, 256-byte inodes, `s_first_data_block == 0`, and exact incompatible `FILETYPE` with all compatible and read-only-compatible feature bits clear.
- All media-provided lengths, offsets, block numbers, inode numbers, arithmetic operations, conversions, and allocations must be checked; malformed media returns typed errors and never panics.
- Do not create packed references to ext2 records. Read little-endian fields from checked byte slices and retain no raw media references after I/O.
- Read-only mounts may inspect dirty/error-marked filesystems. Read-write mount requests reject filesystems without `EXT2_VALID_FS` or with ext2 error state; Task 7 never writes either mount mode.
- Support only regular files and directories, twelve direct pointers plus one singly indirect pointer, non-sparse logical blocks below `i_size`, and a maximum regular-file size of 4,243,456 bytes.
- Directory records require nonzero aligned `rec_len`, in-block bounds, bounded `name_len`, supported and matching `FILETYPE`, and names compatible with the printable-ASCII `Name` contract.
- No ext2 mutation, allocation, links, special files, caching, VFS paths, shell commands, USB, or kernel runtime integration belongs in this task.
- Maintain `cargo fmt --all --check`, workspace Clippy with `-D warnings`, and locked workspace tests.

---

## File Structure

```text
crates/relay-core/src/lib.rs                 Exposes shared filesystem and ext2 modules.
crates/relay-core/src/fs.rs                  Opaque node/name values and filesystem metadata results.
crates/relay-core/src/ext2/mod.rs            Public Ext2 API, mount state, and typed errors.
crates/relay-core/src/ext2/on_disk.rs        Checked ext2 byte decoding and disk-record constants.
crates/relay-core/src/ext2/validate.rs       Fixed-profile superblock/group descriptor validation.
crates/relay-core/src/ext2/inode.rs          Inode loading and direct/singly-indirect block translation.
crates/relay-core/src/ext2/directory.rs      Validated directory-record iteration and lookup/listing.
crates/relay-core/tests/support/mod.rs       Exposes the Task 7 test helpers.
crates/relay-core/tests/support/file_device.rs
                                                Disposable host-file BlockDevice implementation.
crates/relay-core/tests/support/ext2_image.rs
                                                Builds and mutates controlled ext2 test images.
crates/relay-core/tests/ext2_mount.rs        Mount/profile and mount-mode integration tests.
crates/relay-core/tests/ext2_read.rs         Metadata, lookup, directory, and offset-read tests.
```

### Task 1: Filesystem Values And Checked Ext2 Mount Validation

**Files:**
- Create: `crates/relay-core/src/fs.rs`
- Create: `crates/relay-core/src/ext2/mod.rs`
- Create: `crates/relay-core/src/ext2/on_disk.rs`
- Create: `crates/relay-core/src/ext2/validate.rs`
- Modify: `crates/relay-core/src/lib.rs`
- Modify: `crates/relay-core/tests/support/mod.rs`
- Create: `crates/relay-core/tests/support/file_device.rs`
- Create: `crates/relay-core/tests/support/ext2_image.rs`
- Create: `crates/relay-core/tests/ext2_mount.rs`

**Interfaces:**
- Consumes: `crate::block::{BlockDevice, BlockError, BlockGeometry}`.
- Produces: `fs::{NodeId, Name, NameError, NodeKind, Metadata, DirEntry}`, `ext2::{Ext2, Ext2Error, MountMode}`, and a validated private ext2 geometry for Tasks 2 and 3.

- [ ] **Step 1: Write failing public-value and mount-profile tests**

Create `tests/ext2_mount.rs` with `mod support;`, imports for `Ext2`, `Ext2Error`, `MountMode`, `Name`, and `NodeKind`, and tests for the public contracts:

```rust
#[test]
fn name_accepts_only_nonempty_printable_ascii_path_components() {
    assert_eq!(Name::new(b"README.txt").unwrap().as_bytes(), b"README.txt");
    assert!(Name::new(b"").is_err());
    assert!(Name::new(b"a/b").is_err());
    assert!(Name::new(b"a\0b").is_err());
    assert!(Name::new(&[0x7f]).is_err());
}

#[test]
fn mount_accepts_the_controlled_ext2_profile() {
    let image = support::ext2_image::fixture_with_files(&[("hello", b"relay")]);
    let result = Ext2::mount(image.open(), MountMode::ReadOnly);

    assert!(result.is_ok());
}

#[test]
fn mount_rejects_any_unapproved_feature_bit() {
    for field in support::ext2_image::FeatureField::ALL {
        for bit in support::ext2_image::unsupported_feature_bits(field) {
            let image = support::ext2_image::fixture_with_feature(field, bit);
            assert!(matches!(
                Ext2::mount(image.open(), MountMode::ReadOnly),
                Err(Ext2Error::UnsupportedFeature { .. }),
            ));
        }
    }
}
```

Do not expose a production `NodeId` integer accessor merely for tests. The mount task proves successful construction; Task 2 extends integration coverage with public root metadata and lookup assertions.

- [ ] **Step 2: Run mount tests to verify they fail for missing APIs**

Run: `cargo test -p relay-core --test ext2_mount --locked`

Expected: FAIL because `fs`, `ext2`, and test fixture support do not exist.

- [ ] **Step 3: Implement `fs` values and safe on-disk readers**

Define exactly these public types in `src/fs.rs`:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeId(pub(crate) u32);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Name(Vec<u8>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NameError { Empty, TooLong, InvalidByte }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeKind { Regular, Directory }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Metadata { pub kind: NodeKind, pub len: u64, pub mode: u16 }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirEntry { pub name: Name, pub node: NodeId, pub kind: NodeKind }
```

Implement `Name::new(bytes: &[u8]) -> Result<Self, NameError>` and `as_bytes(&self) -> &[u8]`. Accept bytes in `b' '..=b'~'` except slash, and reject `.` and `..` because they are reserved traversal entries. In `ext2/on_disk.rs`, use only checked `get` ranges plus `u16::from_le_bytes`, `u32::from_le_bytes`, and `u64::from_le_bytes`; provide named offsets/constants for superblock, group descriptor, inode, and directory fields. Every short buffer must map to a typed corruption error.

- [ ] **Step 4: Build disposable file-backed image support**

Implement `tests/support/file_device.rs` as a `BlockDevice` backed by a temporary `std::fs::File`. Its `geometry`, reads, writes, and flush use exact sector validation and checked seek offsets; use `read_exact`, `write_all`, and `sync_all`, mapping host I/O to `BlockError::Transport` and read-only access to `BlockError::ReadOnly`. It is test-only and may use `std`.

Implement `tests/support/ext2_image.rs` with an owning `Ext2Fixture` that creates a unique directory under `std::env::temp_dir()` using the process ID plus an atomic counter, formats a 128 MiB image with the exact repository options below, populates requested printable-ASCII files using `debugfs`, and deletes its directory in `Drop`:

```text
mke2fs -F -t ext2 -b 4096 -g 32768 -I 256 -N 4096 -m 0 -O none,filetype \
  -E lazy_itable_init=0,nodiscard,root_owner=0:0 \
  -U 52454c41-5900-4000-8000-000000000004 IMAGE 32768
```

Set `MKE2FS_CONFIG` to the repository `tools/mke2fs.conf` and `E2FSPROGS_FAKE_TIME=1788739200`. Expose `open() -> FileDevice`, `bytes_mut() -> &mut [u8]` only through a read-modify-write helper that flushes before reopen, and named helpers for superblock feature/state fields and group-descriptor fields. Do not share fixed paths between parallel tests.

- [ ] **Step 5: Implement strict mount validation**

In `ext2/mod.rs`, define `MountMode::{ReadOnly, ReadWrite}`, `Ext2Error` variants for block failures, unsupported sector/profile/feature, dirty read-write mount, corrupt metadata, invalid node, wrong node kind, not found, unsupported file, sparse file, and allocation failure. Define `Ext2<D>` to own its device, mode, and validated geometry.

`Ext2::mount` first rejects a non-512-byte device geometry, then reads the 1,024-byte superblock from byte offset 1,024 through checked exact sector requests. Validate ext2 magic, revision 1, 4 KiB block/fragment logarithms, counts, one-group geometry, 256-byte inodes, first-data block, exact feature masks, and root inode 2. Read the sole group descriptor table at block 1, then verify bitmap and inode-table locations and inode-table length are in the sole group and do not overlap superblock/group-descriptor metadata. For `ReadWrite`, require `s_state == EXT2_VALID_FS` and `s_errors` does not indicate an error state. No mount operation writes media.

Keep all block-to-sector and byte-range calculations checked. Use fixed-size stack arrays for superblock/group-descriptor blocks and map no expected malformed input to a panic.

- [ ] **Step 6: Extend mount tests with precise profile and mode failures**

Use the byte mutation helpers to add these tests to `ext2_mount.rs`:

```rust
#[test]
fn read_write_mount_rejects_dirty_or_error_marked_filesystems() {
    for mutation in [mark_dirty, mark_error] {
        let mut image = fixture_with_files(&[]);
        mutation(&mut image);
        assert!(matches!(
            Ext2::mount(image.open(), MountMode::ReadWrite),
            Err(Ext2Error::MountRequiresCleanFilesystem),
        ));
    }
}

#[test]
fn mount_rejects_group_metadata_outside_the_only_block_group() {
    let mut image = fixture_with_files(&[]);
    image.set_inode_table_block(32_768);

    assert!(matches!(
        Ext2::mount(image.open(), MountMode::ReadOnly),
        Err(Ext2Error::CorruptMetadata { .. }),
    ));
}
```

Also mutate magic, revision, block size, inode size, block/inode counts, blocks-per-group, first-data block, each supported-feature mask, bitmap locations, and metadata overlap. Assert exact error variants where the field is independently represented; otherwise assert the corruption category. Include a test that `ReadOnly` mounts a dirty fixture successfully.

- [ ] **Step 7: Run formatting, focused tests, and lint**

Run:

```bash
cargo fmt --all --check
cargo test -p relay-core --test ext2_mount --locked
cargo clippy -p relay-core --all-targets --locked -- -D warnings
```

Expected: all commands pass, including a valid profile mount and every malformed-profile rejection without panic or warnings.

- [ ] **Step 8: Commit mount validation**

```bash
git add crates/relay-core/src/lib.rs crates/relay-core/src/fs.rs crates/relay-core/src/ext2 crates/relay-core/tests/support crates/relay-core/tests/ext2_mount.rs
git commit -m "feat: validate controlled ext2 filesystems"
```

### Task 2: Inode Mapping, Lookup, And Offset Reads

**Files:**
- Create: `crates/relay-core/src/ext2/inode.rs`
- Create: `crates/relay-core/src/ext2/directory.rs`
- Modify: `crates/relay-core/src/ext2/mod.rs`
- Modify: `crates/relay-core/tests/support/ext2_image.rs`
- Create: `crates/relay-core/tests/ext2_read.rs`

**Interfaces:**
- Consumes: Task 1 `Ext2<D>` validated geometry, `BlockDevice`, `NodeId`, `Metadata`, `NodeKind`, and `Ext2Error`.
- Produces: `Ext2::metadata(node)`, `Ext2::lookup(dir, name)`, and `Ext2::read_at(node, offset, dst)` for Task 3 and Task 8.

- [ ] **Step 1: Write failing inode metadata and block-boundary tests**

Create `tests/ext2_read.rs` with `mod support;`. Build a profile image containing a small regular file and a 53,248-byte pattern file (`13 * 4096`) whose last byte in direct block 11 is `0xff` and first byte in logical block 12 is `0x00`:

```rust
#[test]
fn read_crosses_direct_to_single_indirect() {
    let mut fs = fixture_with_pattern_file(13 * 4096).mount_read_only();
    let node = fs.lookup(fs.root(), &Name::new(b"boundary").unwrap()).unwrap();
    let mut bytes = [0; 2];

    assert_eq!(fs.read_at(node, 12 * 4096 - 1, &mut bytes).unwrap(), 2);
    assert_eq!(bytes, [0xff, 0x00]);
}

#[test]
fn read_at_eof_returns_zero_without_accessing_a_data_block() {
    let mut fs = fixture_with_file("hello", b"relay").mount_read_only();
    let node = fs.lookup(fs.root(), &Name::new(b"hello").unwrap()).unwrap();
    let mut bytes = [0xaa; 4];

    assert_eq!(fs.read_at(node, 5, &mut bytes).unwrap(), 0);
    assert_eq!(bytes, [0xaa; 4]);
}
```

- [ ] **Step 2: Run read tests to verify they fail for missing APIs**

Run: `cargo test -p relay-core --test ext2_read --locked`

Expected: FAIL because `metadata`, `lookup`, `read_at`, and inode block translation are absent.

- [ ] **Step 3: Implement checked inode loading and metadata**

Implement private inode loading in `ext2/inode.rs`. Validate `NodeId` is within `2..=4,096`; calculate `(inode - 1) * 256`, locate the inode in the validated inode table with checked arithmetic, and read the containing 4 KiB block. Copy exactly the 256 inode bytes before interpretation. Accept only `S_IFREG` and `S_IFDIR`, preserve permission bits in `Metadata.mode`, and return a typed error for every other file type or unsupported inode flag.

Expose:

```rust
pub fn metadata(&mut self, node: NodeId) -> Result<Metadata, Ext2Error>;
pub fn lookup(&mut self, dir: NodeId, name: &Name) -> Result<NodeId, Ext2Error>;
pub fn read_at(
    &mut self,
    node: NodeId,
    offset: u64,
    dst: &mut [u8],
) -> Result<usize, Ext2Error>;
```

`read_at` rejects directories with `WrongNodeKind`, returns zero without block I/O for empty destinations or offsets at/after EOF, and caps reads to EOF. Reject file sizes above 4,243,456 bytes, any double/triple indirect pointer, and any zero direct/indirect data pointer needed below EOF as `SparseFile`. Validate each data and indirect block number against the accepted filesystem block range before reading.

- [ ] **Step 4: Implement direct and singly-indirect copying**

Read one 4 KiB data block at a time into a fixed buffer. Calculate logical block number and in-block byte offset with checked conversions. Logical blocks 0 through 11 use inode pointers; blocks 12 through 1,035 read the one 4 KiB singly-indirect block and decode 1,024 little-endian `u32` data-block pointers. Copy only the required contiguous segment, repeat until destination or EOF is reached, and use checked output indexes and offset increments.

Do not cache blocks or expose raw inode/block pointers. Do not allocate based on file size or caller destination length.

- [ ] **Step 5: Implement minimal validated directory lookup**

Create `ext2/directory.rs` and implement the private directory-record iterator needed by public `lookup`. It loads directory metadata, rejects a non-directory `NodeId`, and reads each directory data block through the Task 2 logical-block resolver. Require each record's `rec_len` to be nonzero, four-byte aligned, and wholly contained within the current 4 KiB block; require `name_len <= rec_len - 8`; skip only valid deleted records and valid `.`/`..` records. For live records, require a nonzero in-range inode, convert its name through `Name::new`, and load inode metadata to verify ext2 `FILETYPE` values 1 and 2 match a regular file or directory. Return a corruption error for all malformed/unsupported records.

`lookup` compares each validated live name with the requested `Name`, returns the first matching `NodeId`, and returns `NotFound` only after complete successful iteration. It does not allocate a directory result vector; that public listing API stays in Task 3.

- [ ] **Step 6: Add corruption and boundary tests**

Use ext2-image byte mutators to add tests that each produce the named typed failure:

```rust
#[test]
fn read_rejects_a_hole_below_file_length() {
    let mut fixture = fixture_with_file("hole", &[0; 4096]);
    fixture.clear_first_data_pointer("hole");
    let mut fs = fixture.mount_read_only();
    let node = fs.lookup(fs.root(), &Name::new(b"hole").unwrap()).unwrap();

    assert!(matches!(fs.read_at(node, 0, &mut [0; 1]), Err(Ext2Error::SparseFile)));
}

#[test]
fn read_rejects_an_out_of_range_indirect_pointer() {
    let mut fixture = fixture_with_pattern_file(13 * 4096);
    fixture.set_first_indirect_data_pointer("boundary", 32_768);
    let mut fs = fixture.mount_read_only();
    let node = fs.lookup(fs.root(), &Name::new(b"boundary").unwrap()).unwrap();

    assert!(matches!(fs.read_at(node, 12 * 4096, &mut [0; 1]), Err(Ext2Error::CorruptMetadata { .. })));
}
```

Also test root and regular-file metadata, lookup of an absent name, lookup on a regular file, a read crossing an ordinary 4 KiB boundary, partial final-block reads, invalid node values obtained through corrupted directory entries, oversized files, unsupported inode modes, double-indirect use, malformed `rec_len` and mismatched `FILETYPE`, and no writes/flushes during all reads.

- [ ] **Step 7: Run focused inode/read verification**

Run:

```bash
cargo fmt --all --check
cargo test -p relay-core --test ext2_mount --test ext2_read --locked
cargo clippy -p relay-core --all-targets --locked -- -D warnings
```

Expected: all tests pass with direct, cross-block, indirect, EOF, and corrupt-pointer behavior verified against file-backed ext2 fixtures.

- [ ] **Step 8: Commit inode reads and lookup**

```bash
git add crates/relay-core/src/ext2/inode.rs crates/relay-core/src/ext2/directory.rs crates/relay-core/src/ext2/mod.rs crates/relay-core/tests/support/ext2_image.rs crates/relay-core/tests/ext2_read.rs
git commit -m "feat: read controlled ext2 inodes"
```

### Task 3: Directory Listing And Host Compatibility

**Files:**
- Create: `crates/relay-core/src/ext2/directory.rs`
- Modify: `crates/relay-core/src/ext2/mod.rs`
- Modify: `crates/relay-core/tests/support/ext2_image.rs`
- Modify: `crates/relay-core/tests/ext2_read.rs`

**Interfaces:**
- Consumes: Task 1 mount/profile state and Task 2 inode/block-read and validated-lookup API.
- Produces: `Ext2::read_dir(dir)` returning `Vec<DirEntry>` for Task 8 VFS.

- [ ] **Step 1: Write failing public directory-listing tests**

Add these public API tests:

```rust
#[test]
fn root_lookup_and_listing_return_validated_entries_in_disk_order() {
    let mut fs = fixture_with_files(&[("alpha", b"a"), ("beta", b"bb")]).mount_read_only();
    let root = fs.root();

    assert_eq!(fs.lookup(root, &Name::new(b"beta").unwrap()).unwrap(), fs.lookup(root, &Name::new(b"beta").unwrap()).unwrap());
    assert_eq!(
        fs.read_dir(root).unwrap().into_iter().map(|entry| entry.name).collect::<Vec<_>>(),
        vec![Name::new(b"alpha").unwrap(), Name::new(b"beta").unwrap()],
    );
}

#[test]
fn lookup_rejects_malformed_directory_record_before_returning_an_entry() {
    let mut fixture = fixture_with_file("hello", b"relay");
    fixture.set_root_first_record_length(0);
    let mut fs = fixture.mount_read_only();

    assert!(matches!(
        fs.lookup(fs.root(), &Name::new(b"hello").unwrap()),
        Err(Ext2Error::CorruptMetadata { .. }),
    ));
}
```

Use a deterministic fixture construction order. The assertion intentionally checks names and ordering rather than a constructible raw `NodeId` value.

- [ ] **Step 2: Run directory tests to verify they fail for missing APIs**

Run: `cargo test -p relay-core --test ext2_read --locked`

Expected: FAIL because `read_dir` is not implemented.

- [ ] **Step 3: Implement validated directory record iteration**

Extend `ext2/directory.rs` so its existing validated iterator can drive directory listing without duplicating parsing. It first loads metadata and rejects non-directory nodes. It reads directory contents only through the same validated logical-block resolver used by regular reads. For every record, validate all of these before inspecting or returning a name:

```text
inode: u32 at offset 0
rec_len: u16 at offset 4, nonzero and divisible by 4
name_len: u8 at offset 6, at most rec_len - 8
file_type: u8 at offset 7, regular (1) or directory (2) for live entries
record end: at most the current 4 KiB logical directory block
```

For a live entry, validate its inode range, load its inode metadata, and require that metadata kind matches the `FILETYPE` byte. Skip only valid `.` and `..` records and valid records with inode zero. Convert other live names through `Name::new`; reject names this reader cannot represent. Loop bounds are the validated directory length and 4 KiB block boundaries, never a media-controlled unbounded sentinel.

- [ ] **Step 4: Expose lookup and listing APIs**

Implement:

```rust
pub fn read_dir(&mut self, dir: NodeId) -> Result<Vec<DirEntry>, Ext2Error>;
```

`read_dir` reserves incrementally with `try_reserve` and maps allocation failures to `Ext2Error::Allocation`; it preserves on-disk order and does not return `.`/`..`. It must preserve Task 2's `lookup` behavior without reimplementing record parsing. Neither method writes or flushes the block device.

- [ ] **Step 5: Add directory and generated-root compatibility tests**

Add tests for non-directory lookup/listing rejection, absent names, malformed zero/unaligned/cross-block `rec_len`, oversized `name_len`, invalid live inode, unsupported file type, file-type/inode-mode mismatch, nonprintable live name, and skipping valid deleted/`.`/`..` entries. Add an integration test that opens `target/root.ext2` after `cargo xtask image --output target/relay-os.img`, mounts it through `FileDevice`, finds `README.txt`, reads it, and compares its contents with `assets/root/README.txt`.

The generated-root test may return early only if `target/root.ext2` is absent; the command in the final verification step builds it first. After all reader operations, execute `LC_ALL=C e2fsck -fn target/root.ext2` and require exit status 0, proving the reader performed no mutation.

- [ ] **Step 6: Run full Task 7 verification**

Run:

```bash
cargo fmt --all --check
cargo test -p relay-core --test ext2_mount --test ext2_read --locked
cargo xtask image --output target/relay-os.img
cargo test -p relay-core --test ext2_read --locked
LC_ALL=C e2fsck -fn target/root.ext2
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Expected: all commands pass without warnings. The generated root fixture mounts, `README.txt` resolves and reads exactly, and `e2fsck -fn` exits 0 after every read-only test path.

- [ ] **Step 7: Commit directory reader and Task 7**

```bash
git add crates/relay-core/src/ext2/directory.rs crates/relay-core/src/ext2/mod.rs crates/relay-core/tests/support/ext2_image.rs crates/relay-core/tests/ext2_read.rs
git commit -m "feat: read controlled ext2 filesystems"
```
