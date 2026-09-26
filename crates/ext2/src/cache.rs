//! The write-back block cache between ext2 and the device (spec §8.3).
//!
//! It holds whole filesystem blocks and evicts the least recently used
//! clean one when full. Dirty blocks stay until [`BlockCache::sync`], or
//! until they would fill more than half the cache, when they are written
//! out early. A block that fails to write stays dirty, so a later `sync`
//! retries it.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use vfs::{BlockDevice, Errno};

/// The largest single device write `sync` issues.
const MAX_WRITE: usize = 64 << 10;
/// The fewest blocks a cache holds, whatever its byte capacity.
const MIN_BLOCKS: usize = 4;

struct Entry {
    data: Box<[u8]>,
    dirty: bool,
    /// When it was last used. A clean block's tick is its key in `lru`.
    tick: u64,
}

/// A write-back LRU cache of filesystem blocks over a [`BlockDevice`].
/// Filesystem block `b` is device blocks `b * (block_size / device block
/// size)` onwards.
pub struct BlockCache<D: BlockDevice> {
    dev: D,
    block_size: usize,
    /// Device blocks per filesystem block.
    scale: u64,
    /// Blocks held at most.
    capacity: usize,
    entries: BTreeMap<u64, Entry>,
    /// Clean blocks by last use, oldest first: the eviction candidates.
    lru: BTreeMap<u64, u64>,
    tick: u64,
    dirty: usize,
}

impl<D: BlockDevice> BlockCache<D> {
    /// A cache of `capacity_bytes` (at least four blocks) over `dev`.
    /// `EINVAL` unless the device's block size divides `block_size`.
    pub fn new(dev: D, block_size: usize, capacity_bytes: usize) -> Result<Self, Errno> {
        let dev_block = dev.block_size();
        if dev_block == 0 || block_size == 0 || !block_size.is_multiple_of(dev_block) {
            return Err(Errno::EINVAL);
        }
        Ok(BlockCache {
            dev,
            block_size,
            scale: (block_size / dev_block) as u64,
            capacity: (capacity_bytes / block_size).max(MIN_BLOCKS),
            entries: BTreeMap::new(),
            lru: BTreeMap::new(),
            tick: 0,
            dirty: 0,
        })
    }

    pub fn block_size(&self) -> usize {
        self.block_size
    }

    pub fn device(&self) -> &D {
        &self.dev
    }

    /// Gives the device back, dropping every cached block, dirty or not.
    pub fn into_inner(self) -> D {
        self.dev
    }

    /// Blocks changed but not yet written to the device.
    pub fn dirty_blocks(&self) -> usize {
        self.dirty
    }

    /// Block `block`, from the device unless cached.
    pub fn read(&mut self, block: u64) -> Result<&[u8], Errno> {
        self.load(block)?;
        Ok(&self.entry(block).data)
    }

    /// Block `block` for changing: loaded like [`read`](Self::read), then
    /// marked dirty.
    pub fn write(&mut self, block: u64) -> Result<&mut [u8], Errno> {
        self.load(block)?;
        self.mark_dirty(block);
        Ok(&mut self.entry_mut(block).data)
    }

    /// Block `block` filled with zeros and marked dirty, without reading it
    /// (for freshly allocated blocks).
    pub fn zeroed(&mut self, block: u64) -> Result<&mut [u8], Errno> {
        if let Some(entry) = self.entries.get_mut(&block) {
            entry.data.fill(0);
            self.touch(block);
        } else {
            self.make_room()?;
            self.insert(block, vec![0; self.block_size].into_boxed_slice());
        }
        self.mark_dirty(block);
        Ok(&mut self.entry_mut(block).data)
    }

    /// Writes every dirty block, then flushes the device. On a write error
    /// the other blocks are still written; the failed ones stay dirty and
    /// the result is `EIO`.
    pub fn sync(&mut self) -> Result<(), Errno> {
        let written = self.write_back();
        let flushed = self.dev.flush().map_err(Errno::from);
        written.and(flushed)
    }

    /// Writes every dirty block in ascending order, merging runs of
    /// consecutive blocks into single device writes of at most 64 KiB.
    fn write_back(&mut self) -> Result<(), Errno> {
        let dirty: Vec<u64> = self
            .entries
            .iter()
            .filter(|(_, e)| e.dirty)
            .map(|(&b, _)| b)
            .collect();
        let max_run = (MAX_WRITE / self.block_size).max(1);
        let mut result = Ok(());
        let mut rest = &dirty[..];
        while let Some(&first) = rest.first() {
            let mut len = 1;
            while len < rest.len().min(max_run) && rest[len] == first + len as u64 {
                len += 1;
            }
            let (run, tail) = rest.split_at(len);
            rest = tail;
            if self.write_run(run).is_ok() {
                continue;
            }
            if run.len() == 1 {
                result = Err(Errno::EIO);
                continue;
            }
            // Retry one by one, so only the failing blocks stay dirty.
            for &block in run {
                if self.write_run(&[block]).is_err() {
                    result = Err(Errno::EIO);
                }
            }
        }
        result
    }

    /// Writes consecutive dirty blocks with one device write and marks
    /// them clean.
    fn write_run(&mut self, run: &[u64]) -> Result<(), Errno> {
        let first = *run.first().expect("runs are never empty");
        let lba = self.lba(first)?;
        if let [block] = run {
            let data = &self
                .entries
                .get(block)
                .expect("dirty blocks are cached")
                .data;
            self.dev.write(lba, data)?;
        } else {
            let mut buf = Vec::with_capacity(run.len() * self.block_size);
            for block in run {
                buf.extend_from_slice(&self.entry(*block).data);
            }
            self.dev.write(lba, &buf)?;
        }
        for &block in run {
            let entry = self.entry_mut(block);
            entry.dirty = false;
            let tick = entry.tick;
            self.lru.insert(tick, block);
            self.dirty -= 1;
        }
        Ok(())
    }

    fn lba(&self, block: u64) -> Result<u64, Errno> {
        block.checked_mul(self.scale).ok_or(Errno::EIO)
    }

    /// Makes `block` present, reading it if needed. Read errors are not
    /// cached: the next access tries the device again.
    fn load(&mut self, block: u64) -> Result<(), Errno> {
        if self.entries.contains_key(&block) {
            self.touch(block);
            return Ok(());
        }
        let lba = self.lba(block)?;
        self.make_room()?;
        let mut data = vec![0; self.block_size].into_boxed_slice();
        self.dev.read(lba, &mut data)?;
        self.insert(block, data);
        Ok(())
    }

    /// Evicts the least recently used clean block if the cache is full. With
    /// only dirty blocks left they are written out first; if that fails
    /// too, `EIO` rather than growing without bound.
    fn make_room(&mut self) -> Result<(), Errno> {
        if self.entries.len() < self.capacity {
            return Ok(());
        }
        if self.lru.is_empty() {
            // Errors show up again at `sync`; here only room matters.
            let _ = self.write_back();
        }
        let (_, block) = self.lru.pop_first().ok_or(Errno::EIO)?;
        self.entries.remove(&block);
        Ok(())
    }

    fn insert(&mut self, block: u64, data: Box<[u8]>) {
        self.tick += 1;
        let tick = self.tick;
        self.entries.insert(
            block,
            Entry {
                data,
                dirty: false,
                tick,
            },
        );
        self.lru.insert(tick, block);
    }

    /// Records a use of a cached block.
    fn touch(&mut self, block: u64) {
        self.tick += 1;
        let tick = self.tick;
        let entry = self.entry_mut(block);
        let old = core::mem::replace(&mut entry.tick, tick);
        if !entry.dirty {
            self.lru.remove(&old);
            self.lru.insert(tick, block);
        }
    }

    /// Marks a cached block dirty. Dirty blocks past half the cache are
    /// written out first (spec §8.3), so eviction always finds clean ones.
    fn mark_dirty(&mut self, block: u64) {
        if self.entry(block).dirty {
            return;
        }
        if self.dirty + 1 > self.capacity / 2 {
            // Failed blocks stay dirty and are reported by `sync`.
            let _ = self.write_back();
        }
        let entry = self.entry_mut(block);
        entry.dirty = true;
        let tick = entry.tick;
        self.lru.remove(&tick);
        self.dirty += 1;
    }

    fn entry(&self, block: u64) -> &Entry {
        self.entries.get(&block).expect("block is cached")
    }

    fn entry_mut(&mut self, block: u64) -> &mut Entry {
        self.entries.get_mut(&block).expect("block is cached")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeSet;
    use vfs::IoError;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Op {
        /// First device block and number of device blocks.
        Read(u64, u64),
        Write(u64, u64),
        Flush,
    }

    /// A RAM disk of 512-byte blocks that records every request and fails
    /// the chosen blocks.
    struct Fake {
        data: Vec<u8>,
        ops: Vec<Op>,
        fail_reads: BTreeSet<u64>,
        fail_writes: BTreeSet<u64>,
    }

    const DEV_BLOCK: usize = 512;

    impl Fake {
        fn new(blocks: usize) -> Fake {
            let data = (0..blocks * DEV_BLOCK)
                .map(|i| (i / DEV_BLOCK) as u8)
                .collect();
            Fake {
                data,
                ops: Vec::new(),
                fail_reads: BTreeSet::new(),
                fail_writes: BTreeSet::new(),
            }
        }

        fn hits(set: &BTreeSet<u64>, lba: u64, n: u64) -> bool {
            set.range(lba..lba + n).next().is_some()
        }

        fn writes(&self) -> Vec<Op> {
            self.ops
                .iter()
                .copied()
                .filter(|op| !matches!(op, Op::Read(..)))
                .collect()
        }
    }

    impl BlockDevice for Fake {
        fn block_size(&self) -> usize {
            DEV_BLOCK
        }
        fn block_count(&self) -> u64 {
            (self.data.len() / DEV_BLOCK) as u64
        }
        fn read(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), IoError> {
            vfs::check_request(DEV_BLOCK, self.block_count(), lba, buf.len())?;
            let n = (buf.len() / DEV_BLOCK) as u64;
            self.ops.push(Op::Read(lba, n));
            if Self::hits(&self.fail_reads, lba, n) {
                return Err(IoError::Device);
            }
            let at = lba as usize * DEV_BLOCK;
            buf.copy_from_slice(&self.data[at..at + buf.len()]);
            Ok(())
        }
        fn write(&mut self, lba: u64, buf: &[u8]) -> Result<(), IoError> {
            vfs::check_request(DEV_BLOCK, self.block_count(), lba, buf.len())?;
            let n = (buf.len() / DEV_BLOCK) as u64;
            self.ops.push(Op::Write(lba, n));
            if Self::hits(&self.fail_writes, lba, n) {
                return Err(IoError::Device);
            }
            let at = lba as usize * DEV_BLOCK;
            self.data[at..at + buf.len()].copy_from_slice(buf);
            Ok(())
        }
        fn flush(&mut self) -> Result<(), IoError> {
            self.ops.push(Op::Flush);
            Ok(())
        }
    }

    /// 1 KiB blocks (two device blocks each) over a 64-block device.
    fn cache(capacity_blocks: usize) -> BlockCache<Fake> {
        BlockCache::new(Fake::new(128), 1024, capacity_blocks * 1024).unwrap()
    }

    fn ops(c: &mut BlockCache<Fake>) -> Vec<Op> {
        core::mem::take(&mut c.dev.ops)
    }

    #[test]
    fn the_device_block_size_must_divide_the_block_size() {
        assert!(BlockCache::new(Fake::new(8), 1024, 8192).is_ok());
        assert!(BlockCache::new(Fake::new(8), 512, 8192).is_ok());
        assert_eq!(
            BlockCache::new(Fake::new(8), 1000, 8192).err(),
            Some(Errno::EINVAL)
        );
        assert_eq!(
            BlockCache::new(Fake::new(8), 256, 8192).err(),
            Some(Errno::EINVAL)
        );
    }

    #[test]
    fn blocks_are_read_once_at_their_device_address() {
        let mut c = cache(8);
        assert_eq!(c.read(3).unwrap()[..2], [6, 6]);
        assert_eq!(c.read(3).unwrap()[1023], 7);
        assert_eq!(ops(&mut c), [Op::Read(6, 2)]);
        assert_eq!(c.read(64), Err(Errno::EIO), "past the device");
    }

    #[test]
    fn writes_stay_in_the_cache_until_sync() {
        let mut c = cache(16);
        c.write(2).unwrap()[0] = 0xAA;
        assert_eq!(c.dirty_blocks(), 1);
        assert_eq!(c.read(2).unwrap()[0], 0xAA);
        assert_eq!(ops(&mut c), [Op::Read(4, 2)]);
        c.sync().unwrap();
        assert_eq!(ops(&mut c), [Op::Write(4, 2), Op::Flush]);
        assert_eq!(c.device().data[4 * DEV_BLOCK], 0xAA);
        assert_eq!(c.dirty_blocks(), 0);
        c.sync().unwrap();
        assert_eq!(ops(&mut c), [Op::Flush], "nothing left to write");
    }

    #[test]
    fn zeroed_blocks_are_not_read() {
        let mut c = cache(8);
        assert!(c.zeroed(5).unwrap().iter().all(|&b| b == 0));
        c.read(6).unwrap();
        assert!(c.zeroed(6).unwrap().iter().all(|&b| b == 0));
        assert_eq!(ops(&mut c), [Op::Read(12, 2)]);
        assert_eq!(c.dirty_blocks(), 2);
    }

    #[test]
    fn sync_writes_in_ascending_order_merging_runs() {
        let mut c = cache(32);
        for b in [9, 4, 3, 5, 12, 11] {
            c.zeroed(b).unwrap();
        }
        c.sync().unwrap();
        assert_eq!(
            ops(&mut c),
            [
                Op::Write(6, 6),
                Op::Write(18, 2),
                Op::Write(22, 4),
                Op::Flush
            ]
        );
    }

    #[test]
    fn merged_writes_are_at_most_64_kib() {
        let mut c = BlockCache::new(Fake::new(256), 4096, 1 << 20).unwrap();
        for b in 0..20 {
            c.zeroed(b).unwrap();
        }
        c.sync().unwrap();
        assert_eq!(
            ops(&mut c),
            [Op::Write(0, 128), Op::Write(128, 32), Op::Flush]
        );
    }

    #[test]
    fn the_least_recently_used_clean_block_is_evicted() {
        let mut c = cache(4);
        for b in 0..4 {
            c.read(b).unwrap();
        }
        c.read(0).unwrap();
        c.read(4).unwrap(); // evicts 1
        ops(&mut c);
        c.read(0).unwrap();
        c.read(2).unwrap();
        assert_eq!(ops(&mut c), []);
        c.read(1).unwrap();
        assert_eq!(ops(&mut c), [Op::Read(2, 2)]);
    }

    #[test]
    fn dirty_blocks_are_never_evicted() {
        let mut c = cache(4);
        c.write(0).unwrap()[0] = 0xEE;
        for b in 1..10 {
            c.read(b).unwrap();
        }
        assert!(c.dev.writes().is_empty(), "no early write");
        assert_eq!(c.read(0).unwrap()[0], 0xEE);
        c.sync().unwrap();
        assert_eq!(c.device().data[0], 0xEE);
    }

    #[test]
    fn dirty_blocks_past_half_the_cache_are_written_early() {
        let mut c = cache(8);
        for b in 0..4 {
            c.zeroed(b).unwrap();
        }
        assert!(c.dev.writes().is_empty());
        c.zeroed(10).unwrap();
        assert_eq!(c.dev.writes(), [Op::Write(0, 8)], "no flush needed");
        assert_eq!(c.dirty_blocks(), 1);
        // Rewriting a block that is already dirty does not count again.
        c.write(10).unwrap();
        assert_eq!(c.dirty_blocks(), 1);
    }

    #[test]
    fn failed_writes_stay_dirty_and_are_retried() {
        let mut c = cache(16);
        for b in 1..4 {
            c.zeroed(b).unwrap();
        }
        c.dev.fail_writes.insert(4); // block 2
        assert_eq!(c.sync(), Err(Errno::EIO));
        assert_eq!(
            ops(&mut c),
            [
                Op::Write(2, 6),
                Op::Write(2, 2),
                Op::Write(4, 2),
                Op::Write(6, 2),
                Op::Flush
            ]
        );
        assert_eq!(c.dirty_blocks(), 1);
        assert_eq!(c.sync(), Err(Errno::EIO), "still broken");
        ops(&mut c);
        c.dev.fail_writes.clear();
        c.sync().unwrap();
        assert_eq!(ops(&mut c), [Op::Write(4, 2), Op::Flush]);
        assert_eq!(c.dirty_blocks(), 0);
    }

    #[test]
    fn read_errors_are_not_cached() {
        let mut c = cache(8);
        c.dev.fail_reads.insert(2);
        assert_eq!(c.read(1), Err(Errno::EIO));
        assert_eq!(c.write(1).err(), Some(Errno::EIO));
        c.dev.fail_reads.clear();
        assert_eq!(c.read(1).unwrap()[0], 2);
        assert_eq!(c.dirty_blocks(), 0);
    }

    #[test]
    fn a_cache_full_of_unwritable_blocks_refuses_more() {
        let mut c = cache(4);
        c.dev.fail_writes.extend(0..128);
        for b in 0..4 {
            c.zeroed(b).unwrap();
        }
        assert_eq!(c.dirty_blocks(), 4);
        assert_eq!(c.read(9), Err(Errno::EIO));
        assert_eq!(c.zeroed(9).err(), Some(Errno::EIO));
        assert_eq!(c.entries.len(), 4);
        c.dev.fail_writes.clear();
        assert_eq!(c.read(9).unwrap()[0], 18);
    }
}
