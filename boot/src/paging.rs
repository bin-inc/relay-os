//! Builds the kernel's initial page tables while UEFI's identity mapping is
//! still active (so physical addresses can be used as pointers).

use boot_info::PHYS_OFFSET;
use uefi::boot::{self, AllocateType, MemoryType};
use x86_64::structures::paging::{
    FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags as F, PhysFrame,
    Size2MiB, Size4KiB,
};
use x86_64::{PhysAddr, VirtAddr};

/// Hands out zeroed 4 KiB frames from UEFI as LOADER_DATA.
pub struct UefiFrames;

unsafe impl FrameAllocator<Size4KiB> for UefiFrames {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let p = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1).ok()?;
        unsafe { core::ptr::write_bytes(p.as_ptr(), 0, 4096) };
        Some(PhysFrame::containing_address(PhysAddr::new(
            p.as_ptr() as u64
        )))
    }
}

pub struct Tables {
    pub pml4_phys: u64,
    mapper: OffsetPageTable<'static>,
}

impl Tables {
    pub fn new() -> Tables {
        let frame = UefiFrames.allocate_frame().expect("out of memory for PML4");
        let pml4_phys = frame.start_address().as_u64();
        let pml4 = unsafe { &mut *(pml4_phys as *mut PageTable) };
        let mapper = unsafe { OffsetPageTable::new(pml4, VirtAddr::new(0)) };
        Tables { pml4_phys, mapper }
    }

    /// Maps one 4 KiB page.
    pub fn map_4k(&mut self, virt: u64, phys: u64, flags: F) {
        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(virt));
        let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(phys));
        unsafe {
            self.mapper
                .map_to(page, frame, flags | F::PRESENT, &mut UefiFrames)
        }
        .expect("map_4k failed")
        .ignore();
    }

    /// Maps [phys_start, phys_end) at PHYS_OFFSET + phys with 2 MiB pages.
    pub fn map_linear(&mut self, phys_start: u64, phys_end: u64) {
        let flags = F::PRESENT | F::WRITABLE | F::NO_EXECUTE;
        let mut p = phys_start & !0x1F_FFFF;
        while p < phys_end {
            let page = Page::<Size2MiB>::containing_address(VirtAddr::new(PHYS_OFFSET + p));
            let frame = PhysFrame::<Size2MiB>::containing_address(PhysAddr::new(p));
            match unsafe { self.mapper.map_to(page, frame, flags, &mut UefiFrames) } {
                Ok(f) => f.ignore(),
                // Already mapped by an earlier, overlapping range.
                Err(x86_64::structures::paging::mapper::MapToError::PageAlreadyMapped(_)) => {}
                Err(e) => panic!("map_linear failed at {p:#x}: {e:?}"),
            }
            p += 0x20_0000;
        }
    }
}
