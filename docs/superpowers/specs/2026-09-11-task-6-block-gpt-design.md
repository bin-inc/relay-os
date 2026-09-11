# Task 6 Block Device, GUID, And GPT Selection Design

## Status

Approved for implementation on 2026-09-11.

## Objective

Task 6 adds the host-testable storage boundary needed by the ext2 read path.
It provides exact synchronous sector I/O, a bounded view of a selected
partition, and GPT parsing that selects the root partition by the canonical
unique GUID passed from the loader. The implementation remains independent of
USB transport; Task 14 will supply a BOT/SCSI-backed `BlockDevice`.

## Scope And Boundaries

`relay-core::block` defines the only generic sector I/O interface consumed by
partitioning and filesystems:

```rust
pub struct BlockGeometry {
    pub logical_sector_size: u32,
    pub sector_count: u64,
}

pub trait BlockDevice {
    fn geometry(&self) -> BlockGeometry;
    fn read_sectors(&mut self, first_lba: u64, dst: &mut [u8]) -> Result<(), BlockError>;
    fn write_sectors(&mut self, first_lba: u64, src: &[u8]) -> Result<(), BlockError>;
    fn flush(&mut self) -> Result<(), BlockError>;
}
```

Each read and write has exact-I/O semantics. A buffer must have a nonzero
length that is a multiple of the device logical sector size. The requested
sector count is derived from that length, and checked arithmetic proves that
`first_lba + count` does not exceed `geometry().sector_count` before the
device operation begins. The public error type distinguishes invalid requests,
out-of-range access, short transfer, timeout, transport failure, read-only
media, and flush failure.

`PartitionDevice<D>` owns a device and exposes a `PartitionRange` of
`first_lba` plus `sector_count`. Its constructor validates that the range is
nonempty and lies within the inner geometry. Every partition-relative request
is fully validated, including the relative end calculation, before translating
to an inner LBA and forwarding. Consequently an overflowing or out-of-range
partition request performs no inner I/O. Partition geometry keeps the inner
logical-sector size and reports only the partition sector count.

Task 6 supports only the milestone's 512-byte logical-sector profile. GPT
selection rejects any other sector size before reading or interpreting GPT
records. Later USB storage work must expose the same sector profile through
the generic interface.

## GPT Parsing And Selection

`relay-core::gpt` parses both the primary header at LBA 1 and the backup
header at the final device LBA independently. It reads each header and its
declared entry array through exact block I/O, without packed struct references
or unchecked casts. Parser helpers use byte slices and checked offsets for all
little-endian integer and GUID fields.

For each copy, validation requires:

- The `EFI PART` signature, revision 1.0, and a header size from 92 through
  the logical sector size.
- A valid CRC32 over exactly the declared header size, with the stored CRC
  field zeroed for calculation.
- Correct current and alternate LBAs for the copy, and an entry-array LBA
  that is outside the header sector and within the device.
- A valid first/last usable range that is ordered and inside the device.
- A nonzero entry count and entry size of at least 128 bytes, plus checked
  multiplication and sector rounding for the complete entry array.
- A valid CRC32 over exactly `entry_count * entry_size` bytes of the complete
  entry array.
- For every non-empty entry, a valid `first_lba..=last_lba` range contained in
  the header's usable range. Empty entries have an all-zero type GUID and are
  ignored.

Each valid copy becomes an owned semantic table of non-empty entries containing
the entry's type GUID, unique GUID, and half-open sector range. If both copies
are valid, their disk GUID, usable range, entry count, entry size, and semantic
tables must be exactly equal. Their current/alternate LBA pair and entry-array
location are intentionally allowed to differ because they identify the primary
and backup copies. Any other disagreement between valid copies is a conflict
and selection fails; the implementation never guesses which valid-looking copy
is current.

If only one copy is valid, that copy may be used and the successful result is
explicitly marked degraded. If neither copy is valid, selection fails with an
error retaining primary and backup validation causes for deterministic
diagnostics and tests.

The public selection function takes ownership of the device and a
`relay_abi::GptGuid` target. It searches the selected valid table by *unique*
GUID, never by partition type GUID. Zero matches report `GuidNotFound`; two or
more matches report `DuplicateGuid`; exactly one produces a bounded
`PartitionDevice<D>`.

```rust
pub enum GptHealth {
    Redundant,
    Degraded { invalid_copy: GptCopy },
}

pub struct SelectedPartition<D> {
    pub device: PartitionDevice<D>,
    pub health: GptHealth,
}

pub fn find_partition_by_unique_guid<D: BlockDevice>(
    device: D,
    target: GptGuid,
) -> Result<SelectedPartition<D>, GptError>;
```

The explicit `GptHealth` result makes a missing redundant copy observable to
later kernel diagnostics without introducing a callback or logging dependency
into host-testable core logic.

## Error Handling

Malformed media is ordinary input, not a panic condition. All length, sector,
offset, addition, multiplication, conversion, and allocation calculations are
checked. Block-layer errors retain their meaning when propagated from GPT
reads; GPT errors identify the copy and failed invariant when parsing fails.
No repair or writeback is attempted in Task 6. A degraded copy is reported but
not reconstructed; repair remains a host-side responsibility.

## Testing

`crates/relay-core/tests/support/memory_device.rs` supplies a deterministic
in-memory `BlockDevice` that records reads, writes, and flushes. Focused block
tests verify exact sector-length validation, range overflow rejection before
forwarding, relative-to-absolute translation, read-only propagation, and
flush behavior.

GPT tests build byte-level 512-byte-sector fixtures and cover canonical mixed
endian GUID handling, valid mirrored tables, a valid primary with invalid
backup, a valid backup with invalid primary, invalid-both diagnostics,
conflicting valid tables, duplicate target GUIDs, wrong target GUIDs, header
and entry CRC failures, truncated entry arrays, invalid header/entry fields,
and arithmetic overflow paths. The generated `target/relay-os.img` is also
opened through the test block device so its root GUID resolves to the expected
half-open range `133120..395264`.

Verification runs:

```bash
cargo test -p relay-core --test block --test gpt --locked
cargo xtask verify-image target/relay-os.img
```

## Non-Goals

Task 6 does not implement USB, BOT/SCSI, caching, asynchronous I/O, MBR
support, GPT repair, arbitrary sector sizes, filesystem mounting, or runtime
kernel integration. It is a host-tested abstraction and parser foundation for
Tasks 7 through 10.
