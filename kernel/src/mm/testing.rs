//! A fake `PhysMem` for the host tests: frames in host memory at made-up
//! physical addresses, counted, so a test sees every frame come back.

use super::paging::{PAGE, PhysMem};
use std::collections::HashMap;

pub struct FakeMem {
    /// Every allocated frame, tables and pages alike.
    pub tables: HashMap<u64, Box<[u64; 512]>>,
    next: u64,
    /// No more frames than this may be allocated at once.
    pub limit: usize,
}

impl FakeMem {
    pub fn new() -> FakeMem {
        FakeMem {
            tables: HashMap::new(),
            next: 0x1000_0000,
            limit: usize::MAX,
        }
    }

    /// Frames allocated and not yet given back.
    pub fn frames(&self) -> usize {
        self.tables.len()
    }
}

impl Default for FakeMem {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysMem for FakeMem {
    fn table(&mut self, phys: u64) -> &mut [u64; 512] {
        self.tables.get_mut(&phys).expect("not a table frame")
    }
    fn alloc_table(&mut self) -> Option<u64> {
        if self.tables.len() >= self.limit {
            return None;
        }
        let p = self.next;
        self.next += PAGE;
        self.tables.insert(p, Box::new([0; 512]));
        Some(p)
    }
    fn free_frame(&mut self, phys: u64) {
        assert!(self.tables.remove(&phys).is_some(), "{phys:#x} freed twice");
    }
}
