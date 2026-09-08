use crate::{
    SegmentFlags,
    cpu::{PagingDepth, is_canonical, page_indices},
    memory::{Allocation, MemoryError, PAGE_SIZE, allocate_zeroed},
    transition::TransitionPage,
};
use alloc::vec::Vec;

const ENTRIES: usize = 512;
const PRESENT: u64 = 1;
const WRITABLE: u64 = 1 << 1;
const PAGE_SIZE_2M: u64 = 1 << 7;
const NO_EXECUTE: u64 = 1 << 63;
const CACHE_DISABLE: u64 = 1 << 4;
const WRITE_THROUGH: u64 = 1 << 3;
const IDENTITY_BOOTSTRAP_END: u64 = 1 << 32;
const MAX_DIRECT_MAPPED_PHYSICAL: u64 = 0x7fff_ffff_ffff;
pub const PCI_BAR_VIRTUAL_START: u64 = 0xffff_c000_0000_0000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PagingError {
    AddressOverflow,
    Allocation,
    AlreadyMapped,
    HugePageConflict,
}

pub struct PageTables {
    depth: PagingDepth,
    root: Allocation,
    tables: Vec<Allocation>,
    next_pci_bar: u64,
}

impl PageTables {
    // Every raw page-table pointer below comes from `allocate_zeroed`, remains owned by
    // this builder until CR3 is loaded, and is accessed exclusively through its 512-entry
    // identity-mapped page. Callers supply only checked, page-aligned map addresses.
    pub fn new(depth: PagingDepth) -> Result<Self, PagingError> {
        let root = allocate().map_err(|_| PagingError::Allocation)?;
        Ok(Self {
            depth,
            root,
            tables: Vec::new(),
            next_pci_bar: PCI_BAR_VIRTUAL_START,
        })
    }

    pub const fn root_physical_address(&self) -> u64 {
        self.root.physical_start
    }

    pub fn map_kernel_segment(
        &mut self,
        virtual_start: u64,
        physical_start: u64,
        pages: usize,
        flags: SegmentFlags,
    ) -> Result<(), PagingError> {
        let mut virtual_address = virtual_start;
        let mut physical_address = physical_start;
        for _ in 0..pages {
            let mut bits = PRESENT;
            if flags.contains(SegmentFlags::WRITE) {
                bits |= WRITABLE;
            }
            if !flags.contains(SegmentFlags::EXECUTE) {
                bits |= NO_EXECUTE;
            }
            self.map_4k(virtual_address, physical_address, bits)?;
            virtual_address = virtual_address
                .checked_add(PAGE_SIZE)
                .ok_or(PagingError::AddressOverflow)?;
            physical_address = physical_address
                .checked_add(PAGE_SIZE)
                .ok_or(PagingError::AddressOverflow)?;
        }
        Ok(())
    }

    pub fn map_identity_first_4g(&mut self) -> Result<(), PagingError> {
        for address in (0..IDENTITY_BOOTSTRAP_END).step_by((2 * 1024 * 1024) as usize) {
            match self.map_2m(address, address, PRESENT | WRITABLE) {
                // A prior transition-page leaf splits only this bootstrap region into 4 KiB leaves.
                Err(PagingError::AlreadyMapped) => self.map_identity_2m_as_4k(address)?,
                result => result?,
            }
        }
        Ok(())
    }

    pub fn map_identity_allocation(&mut self, allocation: Allocation) -> Result<(), PagingError> {
        let end = allocation.end().map_err(|_| PagingError::AddressOverflow)?;
        let mut physical = allocation.physical_start.max(IDENTITY_BOOTSTRAP_END);
        while physical < end {
            self.map_4k(physical, physical, PRESENT | WRITABLE | NO_EXECUTE)?;
            physical = physical
                .checked_add(PAGE_SIZE)
                .ok_or(PagingError::AddressOverflow)?;
        }
        Ok(())
    }

    pub fn map_transition_page(&mut self, page: TransitionPage) -> Result<(), PagingError> {
        if !is_canonical(page.physical_start, self.depth)
            || !page.physical_start.is_multiple_of(PAGE_SIZE)
        {
            return Err(PagingError::AddressOverflow);
        }
        match self.map_4k(page.physical_start, page.physical_start, PRESENT) {
            Err(PagingError::HugePageConflict)
                if self.has_executable_identity_huge_page(page.physical_start) =>
            {
                Ok(())
            }
            result => result,
        }
    }

    pub fn map_identity_execution_range(
        &mut self,
        physical_start: u64,
        bytes: u64,
    ) -> Result<(), PagingError> {
        if bytes == 0 {
            return Err(PagingError::AddressOverflow);
        }
        let end = physical_start
            .checked_add(bytes)
            .ok_or(PagingError::AddressOverflow)?;
        if end - 1 > MAX_DIRECT_MAPPED_PHYSICAL {
            return Err(PagingError::AddressOverflow);
        }
        let mapped_end = end
            .checked_add(PAGE_SIZE - 1)
            .ok_or(PagingError::AddressOverflow)?
            & !(PAGE_SIZE - 1);
        let mut physical = (physical_start & !(PAGE_SIZE - 1)).max(IDENTITY_BOOTSTRAP_END);
        while physical < mapped_end {
            self.map_4k(physical, physical, PRESENT | WRITABLE)?;
            physical = physical
                .checked_add(PAGE_SIZE)
                .ok_or(PagingError::AddressOverflow)?;
        }
        Ok(())
    }

    pub fn map_usable_ram(
        &mut self,
        physical_start: u64,
        physical_end: u64,
    ) -> Result<(), PagingError> {
        if physical_start > physical_end {
            return Err(PagingError::AddressOverflow);
        }
        if physical_end > 0 && physical_end - 1 > MAX_DIRECT_MAPPED_PHYSICAL {
            return Err(PagingError::AddressOverflow);
        }
        let mut physical = physical_start;
        while physical < physical_end {
            let virtual_address = crate::memory::PHYSICAL_MEMORY_OFFSET
                .checked_add(physical)
                .ok_or(PagingError::AddressOverflow)?;
            if physical.is_multiple_of(2 * 1024 * 1024)
                && physical_end - physical >= 2 * 1024 * 1024
            {
                self.map_2m(virtual_address, physical, PRESENT | WRITABLE | NO_EXECUTE)?;
                physical += 2 * 1024 * 1024;
            } else {
                self.map_4k(virtual_address, physical, PRESENT | WRITABLE | NO_EXECUTE)?;
                physical = physical
                    .checked_add(PAGE_SIZE)
                    .ok_or(PagingError::AddressOverflow)?;
            }
        }
        Ok(())
    }

    pub fn map_framebuffer(&mut self, physical_start: u64, bytes: u64) -> Result<(), PagingError> {
        let end = physical_start
            .checked_add(bytes)
            .ok_or(PagingError::AddressOverflow)?;
        if end > 0 && end - 1 > MAX_DIRECT_MAPPED_PHYSICAL {
            return Err(PagingError::AddressOverflow);
        }
        let mut physical = physical_start & !(PAGE_SIZE - 1);
        while physical < end {
            let virtual_address = crate::memory::PHYSICAL_MEMORY_OFFSET
                .checked_add(physical)
                .ok_or(PagingError::AddressOverflow)?;
            self.map_4k(
                virtual_address,
                physical,
                PRESENT | WRITABLE | NO_EXECUTE | CACHE_DISABLE | WRITE_THROUGH,
            )?;
            physical = physical
                .checked_add(PAGE_SIZE)
                .ok_or(PagingError::AddressOverflow)?;
        }
        Ok(())
    }

    pub fn map_pci_bar(&mut self, physical_start: u64, bytes: u64) -> Result<u64, PagingError> {
        let (physical, pages, offset) = bar_mapping_range(physical_start, bytes)?;
        let virtual_start = self.next_pci_bar;
        let virtual_end = virtual_start
            .checked_add(
                pages
                    .checked_mul(PAGE_SIZE)
                    .ok_or(PagingError::AddressOverflow)?,
            )
            .ok_or(PagingError::AddressOverflow)?;
        let mut physical = physical;
        let mut virtual_address = virtual_start;
        for _ in 0..pages {
            self.map_4k(
                virtual_address,
                physical,
                PRESENT | WRITABLE | NO_EXECUTE | CACHE_DISABLE | WRITE_THROUGH,
            )?;
            physical = physical
                .checked_add(PAGE_SIZE)
                .ok_or(PagingError::AddressOverflow)?;
            virtual_address = virtual_address
                .checked_add(PAGE_SIZE)
                .ok_or(PagingError::AddressOverflow)?;
        }
        self.next_pci_bar = virtual_end;
        virtual_start
            .checked_add(offset)
            .ok_or(PagingError::AddressOverflow)
    }

    fn map_4k(
        &mut self,
        virtual_address: u64,
        physical_address: u64,
        flags: u64,
    ) -> Result<(), PagingError> {
        if !virtual_address.is_multiple_of(PAGE_SIZE) || !physical_address.is_multiple_of(PAGE_SIZE)
        {
            return Err(PagingError::AddressOverflow);
        }
        let leaf_table = self.leaf_table(virtual_address, 4)?;
        let leaf = self.entry_table(leaf_table, page_indices(virtual_address, self.depth)[4]);
        // SAFETY: `leaf` is a bounded entry in an exclusively owned page-table page.
        if unsafe { leaf.read_volatile() } & PRESENT != 0 {
            return Err(PagingError::AlreadyMapped);
        }
        // SAFETY: the same exclusive table ownership makes this leaf update race-free.
        unsafe { leaf.write_volatile(physical_address | flags) };
        Ok(())
    }

    fn map_2m(
        &mut self,
        virtual_address: u64,
        physical_address: u64,
        flags: u64,
    ) -> Result<(), PagingError> {
        const TWO_MIB: u64 = 2 * 1024 * 1024;
        if !virtual_address.is_multiple_of(TWO_MIB) || !physical_address.is_multiple_of(TWO_MIB) {
            return Err(PagingError::AddressOverflow);
        }
        let leaf_table = self.leaf_table(virtual_address, 3)?;
        let leaf = self.entry_table(leaf_table, page_indices(virtual_address, self.depth)[3]);
        // SAFETY: `leaf` is a bounded entry in an exclusively owned page-table page.
        if unsafe { leaf.read_volatile() } & PRESENT != 0 {
            return Err(PagingError::AlreadyMapped);
        }
        // SAFETY: the same exclusive table ownership makes this leaf update race-free.
        unsafe { leaf.write_volatile(physical_address | flags | PAGE_SIZE_2M) };
        Ok(())
    }

    fn map_identity_2m_as_4k(&mut self, start: u64) -> Result<(), PagingError> {
        const TWO_MIB: u64 = 2 * 1024 * 1024;
        let end = start
            .checked_add(TWO_MIB)
            .ok_or(PagingError::AddressOverflow)?;
        let mut address = start;
        while address < end {
            match self.map_4k(address, address, PRESENT | WRITABLE) {
                Err(PagingError::AlreadyMapped)
                    if self.has_executable_identity_4k_page(address) => {}
                result => result?,
            }
            address = address
                .checked_add(PAGE_SIZE)
                .ok_or(PagingError::AddressOverflow)?;
        }
        Ok(())
    }

    fn next_table(&mut self, entry: *mut u64) -> Result<u64, PagingError> {
        // `entry` is an aligned entry in a loader-owned, zeroed page-table page. This
        // loader has exclusive access while building the hierarchy before loading CR3.
        let value = unsafe { entry.read_volatile() };
        if value & PRESENT != 0 {
            if value & PAGE_SIZE_2M != 0 {
                return Err(PagingError::HugePageConflict);
            }
            return Ok(value & 0x000f_ffff_ffff_f000);
        }
        let table = allocate().map_err(|_| PagingError::Allocation)?;
        // SAFETY: `entry` is exclusively owned and `table` is a page-aligned allocation.
        unsafe { entry.write_volatile(table.physical_start | PRESENT | WRITABLE) };
        self.tables.push(table);
        Ok(table.physical_start)
    }

    fn leaf_table(&mut self, virtual_address: u64, leaf_index: usize) -> Result<u64, PagingError> {
        let indices = page_indices(virtual_address, self.depth);
        let first_index = match self.depth {
            PagingDepth::FourLevel => 1,
            PagingDepth::FiveLevel => 0,
        };
        let mut table_physical = self.root.physical_start;
        for &entry_index in &indices[first_index..leaf_index] {
            let entry = self.entry_table(table_physical, entry_index);
            table_physical = self.next_table(entry)?;
        }
        Ok(table_physical)
    }

    fn has_executable_identity_huge_page(&self, address: u64) -> bool {
        const TWO_MIB: u64 = 2 * 1024 * 1024;
        let Some(entry) = self.mapped_leaf_entry(address) else {
            return false;
        };
        entry & (PRESENT | PAGE_SIZE_2M) == PRESENT | PAGE_SIZE_2M
            && entry & NO_EXECUTE == 0
            && entry & 0x000f_ffff_ffff_f000 == address & !(TWO_MIB - 1)
    }

    fn has_executable_identity_4k_page(&self, address: u64) -> bool {
        let Some(entry) = self.mapped_leaf_entry(address) else {
            return false;
        };
        entry & (PRESENT | PAGE_SIZE_2M | NO_EXECUTE) == PRESENT
            && entry & 0x000f_ffff_ffff_f000 == address
    }

    fn mapped_leaf_entry(&self, virtual_address: u64) -> Option<u64> {
        let indices = page_indices(virtual_address, self.depth);
        let first_index = match self.depth {
            PagingDepth::FourLevel => 1,
            PagingDepth::FiveLevel => 0,
        };
        let mut table_physical = self.root.physical_start;
        for (level, &entry_index) in indices[first_index..].iter().enumerate() {
            let level = first_index + level;
            // SAFETY: the table remains owned by this builder and its entry index is bounded.
            let entry = unsafe {
                self.entry_table(table_physical, entry_index)
                    .read_volatile()
            };
            if entry & PRESENT == 0 {
                return None;
            }
            if level == 3 && entry & PAGE_SIZE_2M != 0 {
                return Some(entry);
            }
            if level == 4 || entry & PAGE_SIZE_2M != 0 {
                return (level == 4).then_some(entry);
            }
            table_physical = entry & 0x000f_ffff_ffff_f000;
        }
        None
    }

    #[cfg(test)]
    fn transition_leaf_entry(&self, virtual_address: u64) -> Option<u64> {
        self.mapped_leaf_entry(virtual_address)
    }

    fn entry_table(&self, table_physical: u64, entry: usize) -> *mut u64 {
        debug_assert!(entry < ENTRIES);
        // SAFETY: every table address comes from `allocate_zeroed`, is page-aligned and
        // identity-accessible before CR3 changes; `entry` is bounded to 512 u64 entries.
        unsafe { (table_physical as *mut u64).add(entry) }
    }
}

fn allocate() -> Result<Allocation, MemoryError> {
    allocate_zeroed(1)
}

fn bar_mapping_range(physical_start: u64, bytes: u64) -> Result<(u64, u64, u64), PagingError> {
    if bytes == 0 {
        return Err(PagingError::AddressOverflow);
    }
    let offset = physical_start & (PAGE_SIZE - 1);
    let covered = offset
        .checked_add(bytes)
        .ok_or(PagingError::AddressOverflow)?;
    let end = physical_start
        .checked_add(bytes)
        .ok_or(PagingError::AddressOverflow)?;
    if end - 1 > MAX_DIRECT_MAPPED_PHYSICAL {
        return Err(PagingError::AddressOverflow);
    }
    let pages = covered
        .checked_add(PAGE_SIZE - 1)
        .ok_or(PagingError::AddressOverflow)?
        / PAGE_SIZE;
    Ok((physical_start - offset, pages, offset))
}

#[cfg(test)]
mod tests {
    use super::{NO_EXECUTE, PAGE_SIZE_2M, PRESENT, PageTables, PagingError, bar_mapping_range};
    use crate::{PagingDepth, transition::TransitionPage};

    const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;

    fn assert_executable_identity_leaf(entry: u64, physical_start: u64, huge: bool) {
        assert_ne!(entry & PRESENT, 0);
        assert_eq!(entry & NO_EXECUTE, 0);
        assert_eq!(entry & PAGE_SIZE_2M != 0, huge);
        assert_eq!(
            entry & ADDRESS_MASK,
            if huge {
                physical_start & !(2 * 1024 * 1024 - 1)
            } else {
                physical_start
            }
        );
    }

    #[test]
    fn bar_mapping_covers_an_unaligned_physical_range() {
        assert_eq!(bar_mapping_range(0x1234, 0x2000), Ok((0x1000, 3, 0x234)));
    }

    #[test]
    fn bar_mapping_rejects_physical_range_outside_the_direct_map() {
        assert_eq!(
            bar_mapping_range(0x8000_0000_0000, 1),
            Err(PagingError::AddressOverflow)
        );
    }

    #[test]
    fn fresh_transition_page_creates_an_executable_4k_leaf_in_each_depth() {
        for depth in [PagingDepth::FourLevel, PagingDepth::FiveLevel] {
            let mut tables = PageTables::new(depth).unwrap();
            let page = TransitionPage {
                physical_start: 0x1_0020_0000,
            };
            tables.map_transition_page(page).unwrap();
            assert_executable_identity_leaf(
                tables.transition_leaf_entry(page.physical_start).unwrap(),
                page.physical_start,
                false,
            );
        }
    }

    #[test]
    fn bootstrap_identity_map_preserves_a_prior_transition_page_in_each_depth() {
        for depth in [PagingDepth::FourLevel, PagingDepth::FiveLevel] {
            let mut tables = PageTables::new(depth).unwrap();
            let page = TransitionPage {
                physical_start: 0x0020_1000,
            };
            tables.map_transition_page(page).unwrap();
            tables.map_identity_first_4g().unwrap();
            assert_executable_identity_leaf(
                tables.transition_leaf_entry(page.physical_start).unwrap(),
                page.physical_start,
                false,
            );
            assert_executable_identity_leaf(
                tables
                    .transition_leaf_entry(page.physical_start + 0x1000)
                    .unwrap(),
                page.physical_start + 0x1000,
                false,
            );
        }
    }

    #[test]
    fn transition_page_accepts_an_executable_bootstrap_huge_leaf_in_each_depth() {
        for depth in [PagingDepth::FourLevel, PagingDepth::FiveLevel] {
            let mut tables = PageTables::new(depth).unwrap();
            tables.map_identity_first_4g().unwrap();
            let page = TransitionPage {
                physical_start: 0x0020_1000,
            };
            tables.map_transition_page(page).unwrap();
            assert_executable_identity_leaf(
                tables.transition_leaf_entry(page.physical_start).unwrap(),
                page.physical_start,
                true,
            );
        }
    }

    #[test]
    fn real_four_level_tables_reject_a_noncanonical_transition_page() {
        let mut tables = PageTables::new(PagingDepth::FourLevel).unwrap();
        assert_eq!(
            tables.map_transition_page(TransitionPage {
                physical_start: 0x0001_0000_0000_0000,
            }),
            Err(PagingError::AddressOverflow)
        );
    }
}
