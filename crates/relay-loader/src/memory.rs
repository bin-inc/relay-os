use alloc::vec::Vec;
use core::{mem::size_of, ptr};

use relay_abi::{BootInfo, MemoryMap, MemoryRegion};
#[cfg(not(all(test, not(target_os = "uefi"))))]
use uefi::boot::{self, AllocateType};
use uefi::mem::memory_map::{
    MemoryMap as UefiMemoryMap, MemoryMapMut as UefiMemoryMapMut, MemoryType,
};

use crate::{
    elf::LoadPlan,
    post_ebs::{self, FinalMap},
    transition::{TransitionPage, trampoline_bytes},
};

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
    UnsortedOrOverlapping,
    DescriptorArithmetic,
    SegmentOutOfBounds,
    TransitionTooLarge,
}

impl<T: UefiMemoryMapMut> FinalMap for T {
    fn len(&self) -> usize {
        UefiMemoryMap::len(self)
    }

    fn sort(&mut self) {
        UefiMemoryMapMut::sort(self);
    }
}

pub fn allocate_transition_page() -> Result<TransitionPage, MemoryError> {
    let trampoline = trampoline_bytes();
    if trampoline.len() > PAGE_SIZE as usize {
        return Err(MemoryError::TransitionTooLarge);
    }
    let allocation = allocate_zeroed(1)?;
    // SAFETY: `allocation` owns one writable page and the fixed trampoline fits within it.
    unsafe {
        ptr::copy_nonoverlapping(
            trampoline.as_ptr(),
            allocation.physical_start as *mut u8,
            trampoline.len(),
        );
    }
    Ok(TransitionPage {
        physical_start: allocation.physical_start,
    })
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
    #[cfg(all(test, not(target_os = "uefi")))]
    {
        let layout =
            core::alloc::Layout::from_size_align(pages * PAGE_SIZE as usize, PAGE_SIZE as usize)
                .map_err(|_| MemoryError::AddressOverflow)?;
        // SAFETY: `layout` describes the requested page-aligned test allocation.
        let address = unsafe { alloc::alloc::alloc_zeroed(layout) };
        if address.is_null() {
            return Err(MemoryError::Allocation);
        }
        Ok(Allocation {
            physical_start: address as u64,
            pages,
        })
    }
    #[cfg(not(all(test, not(target_os = "uefi"))))]
    {
        let address = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages)
            .map_err(|_| MemoryError::Allocation)?;
        // SAFETY: UEFI returned `pages` writable, page-aligned pages; they are zeroed before use.
        unsafe { ptr::write_bytes(address.as_ptr(), 0, pages * PAGE_SIZE as usize) };
        Ok(Allocation {
            physical_start: address.as_ptr() as u64,
            pages,
        })
    }
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
#[cfg_attr(test, allow(dead_code))]
pub(crate) unsafe fn normalize_memory_map(
    data: *mut BootData,
    map: &impl UefiMemoryMap,
) -> Result<(), MemoryError> {
    if map.len() > MAX_MEMORY_REGIONS {
        return Err(MemoryError::TooManyRegions);
    }
    let regions = unsafe { &mut (*data).regions };
    let mut previous_end = 0_u64;
    for (index, descriptor) in map.entries().enumerate() {
        let descriptor = unsafe { ptr::read_volatile(descriptor) };
        let length = descriptor
            .page_count
            .checked_mul(PAGE_SIZE)
            .ok_or(MemoryError::DescriptorArithmetic)?;
        let end = descriptor
            .phys_start
            .checked_add(length)
            .ok_or(MemoryError::DescriptorArithmetic)?;
        if descriptor.phys_start < previous_end {
            return Err(MemoryError::UnsortedOrOverlapping);
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

/// # Safety
/// `data` must point to a writable, initialized `BootData` allocation that remains
/// valid after boot services exit; `map` must be the final UEFI memory map.
pub(crate) unsafe fn normalize_final_map(
    data: *mut BootData,
    map: &mut impl UefiMemoryMapMut,
) -> Result<(), MemoryError> {
    post_ebs::sort_final_map(map, MAX_MEMORY_REGIONS).map_err(|()| MemoryError::TooManyRegions)?;
    unsafe { normalize_memory_map(data, map) }
}

#[cfg(test)]
mod tests {
    use core::mem::{MaybeUninit, size_of, size_of_val};

    use uefi::mem::memory_map::{
        MemoryAttribute, MemoryDescriptor, MemoryMapMeta, MemoryMapRefMut, MemoryType,
    };

    use super::*;

    fn descriptor(ty: MemoryType, phys_start: u64, page_count: u64) -> MemoryDescriptor {
        MemoryDescriptor {
            ty,
            padding: 0,
            phys_start,
            virt_start: 0,
            page_count,
            att: MemoryAttribute::empty(),
        }
    }

    fn memory_map(descriptors: &mut [MemoryDescriptor]) -> MemoryMapRefMut<'_> {
        let map_size = size_of_val(descriptors);
        // SAFETY: `MemoryDescriptor` is the exact backing layout of the test map.
        let buffer = unsafe {
            core::slice::from_raw_parts_mut(descriptors.as_mut_ptr().cast::<u8>(), map_size)
        };
        MemoryMapRefMut::new(
            buffer,
            MemoryMapMeta {
                map_size,
                desc_size: size_of::<MemoryDescriptor>(),
                map_key: Default::default(),
                desc_version: MemoryDescriptor::VERSION,
            },
        )
        .unwrap()
    }

    #[test]
    fn unordered_disjoint_map_normalizes_to_sorted_abi_regions() {
        let mut descriptors = [
            descriptor(MemoryType::CONVENTIONAL, 0x9000, 2),
            descriptor(MemoryType::RESERVED, 0x2000, 1),
            descriptor(MemoryType::CONVENTIONAL, 0x5000, 3),
        ];
        let mut map = memory_map(&mut descriptors);
        let mut data = MaybeUninit::<BootData>::zeroed();

        unsafe {
            normalize_final_map(data.as_mut_ptr(), &mut map).unwrap();
        }

        let data = unsafe { data.assume_init() };
        assert_eq!(data.info.memory_map.entry_count, 3);
        assert_eq!(data.regions[0].start, 0x2000);
        assert_eq!(data.regions[0].end, 0x3000);
        assert_eq!(data.regions[0].kind, MEMORY_REGION_RESERVED);
        assert_eq!(data.regions[1].start, 0x5000);
        assert_eq!(data.regions[1].end, 0x8000);
        assert_eq!(data.regions[1].kind, MEMORY_REGION_USABLE);
        assert_eq!(data.regions[2].start, 0x9000);
        assert_eq!(data.regions[2].end, 0xb000);
        assert_eq!(data.regions[2].kind, MEMORY_REGION_USABLE);
    }

    #[test]
    fn unordered_nonadjacent_overlap_is_rejected_after_sorting() {
        let mut descriptors = [
            descriptor(MemoryType::RESERVED, 0x5000, 1),
            descriptor(MemoryType::CONVENTIONAL, 0x9000, 1),
            descriptor(MemoryType::RESERVED, 0x1000, 1),
            descriptor(MemoryType::CONVENTIONAL, 0x4000, 2),
        ];
        let mut map = memory_map(&mut descriptors);
        let mut data = MaybeUninit::<BootData>::zeroed();
        let result = unsafe { normalize_final_map(data.as_mut_ptr(), &mut map) };

        assert_eq!(result, Err(MemoryError::UnsortedOrOverlapping));
    }
}
