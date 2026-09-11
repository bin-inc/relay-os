# Task 6 Block Device And GPT Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add host-tested exact sector I/O, bounded partition devices, and redundant GPT root-partition selection by unique GUID.

**Architecture:** `relay-core::block` defines synchronous, exact-I/O storage semantics; `block::partition` translates validated partition-relative requests into the owned device. `relay-core::gpt` independently validates primary and backup GPT copies into semantic tables, rejects valid-copy disagreement, and returns the selected `PartitionDevice` with explicit redundancy health. All parsing remains safe `no_std` Rust and is exercised through an in-memory block device.

**Tech Stack:** Rust 1.98.1, edition 2024, `no_std` plus `alloc`, `crc` 3.4.0 with `CRC_32_ISO_HDLC`, existing `relay-abi::GptGuid`.

**Spec:** `docs/superpowers/specs/2026-09-11-task-6-block-gpt-design.md`

## Global Constraints

- `relay-core` remains `#![no_std]`; host tests may use `std` only from integration-test support.
- Support exactly 512-byte logical sectors for GPT selection; reject all other logical-sector sizes before GPT I/O.
- Every external sector count, byte length, offset, arithmetic operation, and conversion must be bounds checked; malformed media returns typed errors and never panics.
- `BlockDevice` operations use exact nonempty sector-multiple buffers and must prove ranges before forwarding an operation.
- `PartitionDevice` must reject overflow or out-of-range relative requests before performing inner I/O.
- Parse primary and backup GPT copies independently. Accept one valid copy with typed degradation status; reject two valid copies whose disk GUID, usable range, entry shape, or semantic non-empty entries differ.
- Select exactly one partition by `relay_abi::GptGuid` unique GUID, never by type GUID. Do not repair or write GPT data.
- Do not add a runtime dependency or kernel/USB integration in this task.
- Maintain `cargo fmt`, Clippy with warnings denied, and locked workspace test compatibility.

---

## File Structure

```text
crates/relay-core/src/lib.rs                 Exposes the new block and GPT modules.
crates/relay-core/Cargo.toml                 Adds the existing ABI crate for canonical GUID types.
crates/relay-core/src/block/mod.rs           Exact-I/O trait, geometry, errors, and shared request validation.
crates/relay-core/src/block/partition.rs     Owning bounded partition view over any block device.
crates/relay-core/src/gpt.rs                 Safe GPT copy parsing, comparison, GUID selection, and typed results.
crates/relay-core/tests/support/mod.rs       Shares test-only device support between integration-test crates.
crates/relay-core/tests/support/memory_device.rs
                                                Deterministic instrumented in-memory BlockDevice.
crates/relay-core/tests/block.rs             Exact-I/O and partition-boundary integration tests.
crates/relay-core/tests/gpt.rs               Byte-level GPT fixture and redundancy/selection tests.
```

### Task 1: Exact Block Contract And Bounded Partition Device

**Files:**
- Create: `crates/relay-core/src/block/mod.rs`
- Create: `crates/relay-core/src/block/partition.rs`
- Modify: `crates/relay-core/src/lib.rs`
- Create: `crates/relay-core/tests/support/mod.rs`
- Create: `crates/relay-core/tests/support/memory_device.rs`
- Create: `crates/relay-core/tests/block.rs`

**Interfaces:**
- Consumes: no new runtime dependencies.
- Produces: `BlockGeometry`, `BlockDevice`, `BlockError`, `PartitionRange`, and `PartitionDevice<D>` for `gpt` and future filesystems.

- [ ] **Step 1: Write failing exact-I/O and partition-boundary tests**

Create `tests/support/mod.rs` containing `mod memory_device;` and create an initial instrumented `MemoryDevice` in `tests/support/memory_device.rs`. In `tests/block.rs`, add these tests before the production modules exist:

```rust
mod support;

use relay_core::block::{BlockDevice, BlockError, PartitionDevice, PartitionRange};
use support::memory_device::MemoryDevice;

#[test]
fn partition_rejects_end_overflow_without_io() {
    let inner = MemoryDevice::new(512, 16);
    let mut part = PartitionDevice::new(
        inner,
        PartitionRange { first_lba: 8, sector_count: 8 },
    )
    .unwrap();

    assert_eq!(part.read_sectors(7, &mut [0; 1024]), Err(BlockError::OutOfRange));
    assert_eq!(part.inner().read_count(), 0);
}

#[test]
fn partition_translates_valid_relative_requests() {
    let mut inner = MemoryDevice::new(512, 16);
    inner.fill_sector(9, 0x5a);
    let mut part = PartitionDevice::new(
        inner,
        PartitionRange { first_lba: 8, sector_count: 4 },
    )
    .unwrap();
    let mut bytes = [0; 512];

    part.read_sectors(1, &mut bytes).unwrap();

    assert_eq!(bytes, [0x5a; 512]);
    assert_eq!(part.inner().read_lbas(), &[9]);
}

#[test]
fn partition_rejects_empty_or_non_sector_buffers_without_io() {
    let inner = MemoryDevice::new(512, 16);
    let mut part = PartitionDevice::new(
        inner,
        PartitionRange { first_lba: 8, sector_count: 8 },
    )
    .unwrap();

    assert_eq!(part.write_sectors(0, &[]), Err(BlockError::InvalidRequest));
    assert_eq!(part.write_sectors(0, &[0; 1]), Err(BlockError::InvalidRequest));
    assert_eq!(part.inner().write_count(), 0);
}
```

- [ ] **Step 2: Run the new test target to verify it fails**

Run: `cargo test -p relay-core --test block --locked`

Expected: FAIL because `relay_core::block`, `PartitionDevice`, and the test support device are absent.

- [ ] **Step 3: Implement the exact block and test-device contracts**

In `src/block/mod.rs`, define the common contract and expose the partition module:

```rust
mod partition;

pub use partition::{PartitionDevice, PartitionRange};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockGeometry {
    pub logical_sector_size: u32,
    pub sector_count: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockError {
    InvalidRequest,
    OutOfRange,
    ShortTransfer,
    Timeout,
    Transport,
    ReadOnly,
    Flush,
}

pub trait BlockDevice {
    fn geometry(&self) -> BlockGeometry;
    fn read_sectors(&mut self, first_lba: u64, dst: &mut [u8]) -> Result<(), BlockError>;
    fn write_sectors(&mut self, first_lba: u64, src: &[u8]) -> Result<(), BlockError>;
    fn flush(&mut self) -> Result<(), BlockError>;
}
```

Keep a crate-visible validation helper in this module. It rejects zero sector sizes, empty buffers, nonmultiples, conversion failures, `first_lba.checked_add(count)` overflow, and end LBAs beyond the supplied sector count. `PartitionDevice` invokes it with partition geometry before computing `range.first_lba.checked_add(first_lba)`, then delegates exactly once to the inner device. Its constructor rejects zero-length partitions, zero inner sector size, and `first_lba + sector_count` outside the inner geometry. Expose `inner(&self) -> &D`, `inner_mut(&mut self) -> &mut D`, and `into_inner(self) -> D`.

Implement `MemoryDevice` as test-only infrastructure backed by `Vec<u8>`, with `new(sector_size, sector_count)`, `from_bytes(sector_size, bytes)`, `fill_sector`, `read_count`, `write_count`, and `read_lbas`. `from_bytes` rejects a byte length that is not an exact sector multiple. Its trait methods validate through the same expected semantics, copy exact ranges, record forwarded LBAs, and return configurable read-only or flush errors where requested by test helpers.

Add `pub mod block;` to `src/lib.rs`.

- [ ] **Step 4: Extend tests for constructor, write, and flush behavior**

Add the following assertions to `tests/block.rs`:

```rust
#[test]
fn partition_constructor_rejects_an_inner_range_overflow() {
    assert!(matches!(
        PartitionDevice::new(
            MemoryDevice::new(512, 16),
            PartitionRange { first_lba: 15, sector_count: 2 },
        ),
        Err(BlockError::OutOfRange),
    ));
}

#[test]
fn partition_forwards_flush_and_write_errors_unchanged() {
    let inner = MemoryDevice::new(512, 16).with_read_only().with_flush_error();
    let mut part = PartitionDevice::new(
        inner,
        PartitionRange { first_lba: 8, sector_count: 8 },
    )
    .unwrap();

    assert_eq!(part.write_sectors(0, &[0; 512]), Err(BlockError::ReadOnly));
    assert_eq!(part.flush(), Err(BlockError::Flush));
}
```

- [ ] **Step 5: Run formatting, focused tests, and lint**

Run:

```bash
cargo fmt --all --check
cargo test -p relay-core --test block --locked
cargo clippy -p relay-core --all-targets --locked -- -D warnings
```

Expected: all commands pass; invalid partition requests leave the memory device operation counters unchanged.

- [ ] **Step 6: Commit the block abstraction**

```bash
git add crates/relay-core/src/lib.rs crates/relay-core/src/block crates/relay-core/tests/support crates/relay-core/tests/block.rs
git commit -m "feat: add exact block partitions"
```

### Task 2: Redundant GPT Validation And Unique-GUID Selection

**Files:**
- Create: `crates/relay-core/src/gpt.rs`
- Modify: `crates/relay-core/Cargo.toml`
- Modify: `crates/relay-core/src/lib.rs`
- Modify: `crates/relay-core/tests/support/memory_device.rs`
- Create: `crates/relay-core/tests/gpt.rs`

**Interfaces:**
- Consumes: `BlockDevice`, `BlockError`, `PartitionDevice`, `PartitionRange`, `crc::Crc`, `crc::CRC_32_ISO_HDLC`, and `relay_abi::GptGuid`.
- Produces: `GptCopy`, `GptHealth`, `SelectedPartition<D>`, `GptError`, and `find_partition_by_unique_guid(device, target)` for Task 7.

- [ ] **Step 1: Write failing GPT endian, redundancy, and selection tests**

Add `relay-abi = { path = "../relay-abi" }` under `relay-core`'s dependencies. Create `tests/gpt.rs`. In `tests/support/mod.rs`, make the memory-device module public with `pub mod memory_device;`. Extend `MemoryDevice` with an externally retained, shared `OperationLog` containing read/write/flush counters; `operations(&self) -> OperationLog` returns a clone suitable for assertions after the device is moved into selection. Add helpers that construct a 512-byte-sector disk, write 92-byte revision-1 GPT headers with calculated header CRCs, write 128-byte entries with calculated entry-array CRCs, and mirror the entries to primary and backup locations. Start with these contract tests:

```rust
mod support;

use core::str::FromStr;

use relay_abi::GptGuid;
use relay_core::gpt::{find_partition_by_unique_guid, GptCopy, GptHealth};
use support::memory_device::MemoryDevice;

#[test]
fn guid_decodes_gpt_mixed_endian_fields() {
    let guid = GptGuid::from_gpt_bytes([
        0x41, 0x4c, 0x45, 0x52, 0x59, 0x00, 0x00, 0x40,
        0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03,
    ]);
    assert_eq!(guid.to_string(), "52454c41-5900-4000-8000-000000000003");
}

#[test]
fn matching_copies_select_the_unique_guid_with_redundant_health() {
    let target = GptGuid::from_str("52454c41-5900-4000-8000-000000000003").unwrap();
    let disk = mirrored_disk_with_entry(target, 133_120, 395_263);

    let selected = find_partition_by_unique_guid(disk, target).unwrap();

    assert_eq!(selected.health, GptHealth::Redundant);
    assert_eq!(selected.device.geometry().sector_count, 262_144);
}

#[test]
fn one_valid_copy_returns_typed_degradation() {
    let target = GptGuid::from_str("52454c41-5900-4000-8000-000000000003").unwrap();
    let mut disk = mirrored_disk_with_entry(target, 133_120, 395_263);
    corrupt_backup_header_crc(&mut disk);

    let selected = find_partition_by_unique_guid(disk, target).unwrap();

    assert_eq!(selected.health, GptHealth::Degraded { invalid_copy: GptCopy::Backup });
}
```

- [ ] **Step 2: Run GPT tests to verify the public API is absent**

Run: `cargo test -p relay-core --test gpt --locked`

Expected: FAIL because `relay_core::gpt` and `find_partition_by_unique_guid` do not exist.

- [ ] **Step 3: Implement safe per-copy GPT parsing and public types**

In `src/gpt.rs`, use these public types and signature:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GptCopy {
    Primary,
    Backup,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GptHealth {
    Redundant,
    Degraded { invalid_copy: GptCopy },
}

pub struct SelectedPartition<D> {
    pub device: PartitionDevice<D>,
    pub health: GptHealth,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GptError {
    UnsupportedSectorSize(u32),
    Device(BlockError),
    BothCopiesInvalid { primary: GptCopyError, backup: GptCopyError },
    ConflictingCopies,
    GuidNotFound,
    DuplicateGuid,
}

pub fn find_partition_by_unique_guid<D: BlockDevice>(
    device: D,
    target: GptGuid,
) -> Result<SelectedPartition<D>, GptError>;
```

Define a public `GptCopyError` enum for signature/header/CRC/range/entry-shape/entry-CRC/entry-range validation failures, carrying only bounded scalar context such as LBA or field value. Read headers through a one-sector scratch buffer, read the complete rounded-up entry array into an `alloc::vec::Vec<u8>`, and calculate CRC32 using `Crc::<u32>::new(&CRC_32_ISO_HDLC)`.

Require exact primary `current_lba == 1` and backup `current_lba == geometry.sector_count - 1`; each copy's `alternate_lba` must be the other's location. Validate signature, revision `0x0001_0000`, header size `92..=512`, header CRC, usable range, nonzero count, entry size at least 128, entry byte length/sector rounding, array placement, entry CRC, and nonempty entry ranges. Decode GUIDs only with `GptGuid::from_gpt_bytes`; empty entries are identified only by an all-zero type GUID. Store type GUID, unique GUID, and half-open range for every nonempty entry in a private semantic table.

Add `pub mod gpt;` in `src/lib.rs`.

- [ ] **Step 4: Implement copy resolution and exact unique-GUID selection**

Resolve parser results as follows:

```rust
match (parse_primary, parse_backup) {
    (Ok(primary), Ok(backup)) if primary.matches(&backup) => (primary, GptHealth::Redundant),
    (Ok(_), Ok(_)) => return Err(GptError::ConflictingCopies),
    (Ok(primary), Err(_)) => (primary, GptHealth::Degraded { invalid_copy: GptCopy::Backup }),
    (Err(_), Ok(backup)) => (backup, GptHealth::Degraded { invalid_copy: GptCopy::Primary }),
    (Err(primary), Err(backup)) => return Err(GptError::BothCopiesInvalid { primary, backup }),
};
```

`matches` compares disk GUID, usable first/last LBA, entry count, entry size, and the ordered semantic nonempty entries. It intentionally does not compare copy-local current/alternate LBA, header CRC, entry-array LBA, or entry-array CRC fields. Count selected table entries matching `target`; reject zero or more than one, then construct `PartitionDevice::new(device, range)` and map its error through `GptError::Device`.

- [ ] **Step 5: Add complete negative GPT coverage**

Extend `tests/gpt.rs` with fixtures and assertions for each outcome below. Each test must mutate precisely one fixture field and assert the named error or health result:

```rust
#[test]
fn conflicting_valid_copy_is_rejected() {
    let target = test_guid(3);
    let mut disk = mirrored_disk_with_entry(target, 133_120, 395_263);
    rewrite_backup_entry_range_and_crcs(&mut disk, 133_121, 395_263);

    assert!(matches!(
        find_partition_by_unique_guid(disk, target),
        Err(GptError::ConflictingCopies),
    ));
}

#[test]
fn duplicate_unique_guid_is_rejected() {
    let target = test_guid(3);
    let disk = mirrored_disk_with_duplicate_unique_guid(target);

    assert!(matches!(
        find_partition_by_unique_guid(disk, target),
        Err(GptError::DuplicateGuid),
    ));
}

#[test]
fn non_512_byte_devices_are_rejected_without_reads() {
    let disk = MemoryDevice::new(4_096, 64);
    let operations = disk.operations();

    assert!(matches!(
        find_partition_by_unique_guid(disk, test_guid(3)),
        Err(GptError::UnsupportedSectorSize(4_096)),
    ));
    assert_eq!(operations.read_count(), 0);
}
```

Also cover `GuidNotFound`, both-copy corruption retaining `GptCopyError` for primary and backup, header signature/revision/size/current-or-alternate-LBA/usable-range failures, header CRC failure, zero entry count, entry size below 128, overflowing entry count-times-size, truncated or out-of-device entry array, entry-array CRC failure, and nonempty entry range outside usable LBAs. Include one test that proves a matching type GUID but different unique GUID returns `GuidNotFound`.

- [ ] **Step 6: Add a generated-image round-trip test**

In `tests/gpt.rs`, add a test that reads `target/relay-os.img` only when it exists. If absent, it returns without passing the selection assertion so the dedicated command below remains authoritative. When present, load it into `MemoryDevice::from_bytes(512, bytes)`, select `ROOT_GUID` parsed from `relay_xtask`'s documented value, and assert `GptHealth::Redundant` plus a selected geometry of 262,144 sectors. Do not add a dependency from `relay-core` to `relay-xtask`; retain the canonical GUID string in the test.

- [ ] **Step 7: Run focused, generated-image, workspace, and image verification gates**

Run:

```bash
cargo fmt --all --check
cargo test -p relay-core --test block --test gpt --locked
cargo xtask image --output target/relay-os.img
cargo test -p relay-core --test gpt --locked
cargo xtask verify-image target/relay-os.img
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Expected: all commands pass. The second GPT run exercises the generated-image round trip, resolving GUID `52454c41-5900-4000-8000-000000000003` to 262,144 sectors beginning at LBA 133120; `verify-image` reports valid GPT and ext2 output.

- [ ] **Step 8: Commit Task 6**

```bash
git add crates/relay-core/Cargo.toml crates/relay-core/src/lib.rs crates/relay-core/src/gpt.rs crates/relay-core/tests/support/memory_device.rs crates/relay-core/tests/gpt.rs
git commit -m "feat: add block and GPT abstractions"
```
