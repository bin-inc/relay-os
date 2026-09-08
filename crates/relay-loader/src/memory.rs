use alloc::vec::Vec;
use core::{mem::size_of, ptr};

use relay_abi::{BootInfo, MemoryMap, MemoryRegion};
use uefi::{
    boot::{self, AllocateType},
    mem::memory_map::{MemoryMap as UefiMemoryMap, MemoryType},
};

use crate::elf::LoadPlan;

pub const PAGE_SIZE: u64 = 4096;
pub const PHYSICAL_MEMORY_OFFSET: u64 = 0xffff_8000_0000_0000;
pub const MEMORY_REGION_RESERVED: u32 = 0;
pub const MEMORY_REGION_USABLE: u32 = 1;
pub const MAX_MEMORY_REGIONS: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryError {
    AddressOverflow,
    Allocation,
    TooManyRegions,
    SegmentOutOfBounds,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Allocation {
    pub physical_start: u64,
    pub pages: usize,
}

impl Allocation {
    pub fn byte_len(self) -> Result<usize, MemoryError> {
        self.pages
            .checked_mul(PAGE_SIZE as usize)
            .ok_or(MemoryError::AddressOverflow)
    }

    pub fn end(self) -> Result<u64, MemoryError> {
        let byte_len = self.byte_len()?;
        self.physical_start
            .checked_add(u64::try_from(byte_len).map_err(|_| MemoryError::AddressOverflow)?)
            .ok_or(MemoryError::AddressOverflow)
    }
}

pub fn allocate_zeroed(pages: usize) -> Result<Allocation, MemoryError> {
    if pages == 0 || pages.checked_mul(PAGE_SIZE as usize).is_none() {
        return Err(MemoryError::AddressOverflow);
    }
    let address = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages)
        .map_err(|_| MemoryError::Allocation)?;
    // SAFETY: UEFI returned `pages` writable, page-aligned pages; they are zeroed before use.
    unsafe { ptr::write_bytes(address.as_ptr(), 0, pages * PAGE_SIZE as usize) };
    Ok(Allocation {
        physical_start: address.as_ptr() as u64,
        pages,
    })
}

pub struct LoadedSegment {
    pub virtual_start: u64,
    pub physical_start: u64,
    pub page_virtual_start: u64,
    pub pages: usize,
    pub flags: crate::SegmentFlags,
}

pub struct LoadedKernel {
    pub entry: u64,
    pub segments: Vec<LoadedSegment>,
}

pub fn load_kernel(plan: &LoadPlan, image: &[u8]) -> Result<LoadedKernel, MemoryError> {
    let mut segments = Vec::with_capacity(plan.segments.len());
    for segment in &plan.segments {
        let page_virtual_start = segment.virtual_start & !(PAGE_SIZE - 1);
        let page_offset = usize::try_from(segment.virtual_start - page_virtual_start)
            .map_err(|_| MemoryError::AddressOverflow)?;
        let bytes = page_offset
            .checked_add(
                usize::try_from(segment.memory_len).map_err(|_| MemoryError::AddressOverflow)?,
            )
            .ok_or(MemoryError::AddressOverflow)?;
        let pages = bytes
            .checked_add(PAGE_SIZE as usize - 1)
            .ok_or(MemoryError::AddressOverflow)?
            / PAGE_SIZE as usize;
        let allocation = allocate_zeroed(pages)?;
        let source = image
            .get(segment.file_range.clone())
            .ok_or(MemoryError::SegmentOutOfBounds)?;
        // SAFETY: `allocation` was zeroed above and `source.len()` was validated by the ELF plan.
        unsafe {
            ptr::copy_nonoverlapping(
                source.as_ptr(),
                (allocation.physical_start as *mut u8).add(page_offset),
                source.len(),
            );
        }
        segments.push(LoadedSegment {
            virtual_start: segment.virtual_start,
            physical_start: allocation.physical_start,
            page_virtual_start,
            pages,
            flags: segment.flags,
        });
    }
    Ok(LoadedKernel {
        entry: plan.entry,
        segments,
    })
}

#[repr(C)]
pub struct BootData {
    pub info: BootInfo,
    pub regions: [MemoryRegion; MAX_MEMORY_REGIONS],
}

pub fn allocate_boot_data() -> Result<Allocation, MemoryError> {
    let bytes = size_of::<BootData>();
    let pages = bytes
        .checked_add(PAGE_SIZE as usize - 1)
        .ok_or(MemoryError::AddressOverflow)?
        / PAGE_SIZE as usize;
    allocate_zeroed(pages)
}

/// # Safety
/// `data` must point to a writable, initialized `BootData` allocation that remains
/// valid after boot services exit; `map` must be the final UEFI memory map.
pub unsafe fn normalize_memory_map(
    data: *mut BootData,
    map: &impl UefiMemoryMap,
) -> Result<(), MemoryError> {
    if map.len() > MAX_MEMORY_REGIONS {
        return Err(MemoryError::TooManyRegions);
    }
    let regions = unsafe { &mut (*data).regions };
    let mut previous_end = 0_u64;
    for (index, descriptor) in map.entries().enumerate() {
        let length = descriptor
            .page_count
            .checked_mul(PAGE_SIZE)
            .ok_or(MemoryError::AddressOverflow)?;
        let end = descriptor
            .phys_start
            .checked_add(length)
            .ok_or(MemoryError::AddressOverflow)?;
        if descriptor.phys_start < previous_end {
            return Err(MemoryError::AddressOverflow);
        }
        previous_end = end;
        regions[index] = MemoryRegion {
            start: descriptor.phys_start,
            end,
            kind: if descriptor.ty == MemoryType::CONVENTIONAL {
                MEMORY_REGION_USABLE
            } else {
                MEMORY_REGION_RESERVED
            },
            reserved: 0,
        };
    }
    unsafe {
        (*data).info.memory_map = MemoryMap {
            entries_address: core::ptr::addr_of!((*data).regions) as u64,
            entry_count: map.len() as u64,
        };
    }
    Ok(())
}
