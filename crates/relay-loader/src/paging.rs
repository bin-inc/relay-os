use crate::{
    SegmentFlags,
    memory::{Allocation, MemoryError, PAGE_SIZE, allocate_zeroed},
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
    root: Allocation,
    tables: Vec<Allocation>,
    next_pci_bar: u64,
}

impl PageTables {
    // Every raw page-table pointer below comes from `allocate_zeroed`, remains owned by
    // this builder until CR3 is loaded, and is accessed exclusively through its 512-entry
    // identity-mapped page. Callers supply only checked, page-aligned map addresses.
    pub fn new() -> Result<Self, PagingError> {
        let root = allocate().map_err(|_| PagingError::Allocation)?;
        Ok(Self {
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
            // The trampoline itself remains identity-mapped while it loads CR3.
            self.map_2m(address, address, PRESENT | WRITABLE)?;
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
        let l4 = self.entry_table(self.root.physical_start, index(virtual_address, 39));
        let l3 = self.next_table(l4)?;
        let l2 = self.entry_table(l3, index(virtual_address, 30));
        let l1 = self.next_table(l2)?;
        let entry = self.entry_table(l1, index(virtual_address, 21));
        let l0 = self.next_table(entry)?;
        let leaf = self.entry_table(l0, index(virtual_address, 12));
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
        let l4 = self.entry_table(self.root.physical_start, index(virtual_address, 39));
        let l3 = self.next_table(l4)?;
        let l2 = self.entry_table(l3, index(virtual_address, 30));
        let l1 = self.next_table(l2)?;
        let leaf = self.entry_table(l1, index(virtual_address, 21));
        // SAFETY: `leaf` is a bounded entry in an exclusively owned page-table page.
        if unsafe { leaf.read_volatile() } & PRESENT != 0 {
            return Err(PagingError::AlreadyMapped);
        }
        // SAFETY: the same exclusive table ownership makes this leaf update race-free.
        unsafe { leaf.write_volatile(physical_address | flags | PAGE_SIZE_2M) };
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

const fn index(address: u64, shift: u64) -> usize {
    ((address >> shift) & 0x1ff) as usize
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
    use super::{PagingError, bar_mapping_range};

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
}
