#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PagingDepth {
    FourLevel,
    FiveLevel,
}

#[cfg(any(target_os = "uefi", test))]
pub(crate) fn is_canonical(address: u64, depth: PagingDepth) -> bool {
    match depth {
        PagingDepth::FourLevel => {
            if address & (1 << 47) == 0 {
                address >> 48 == 0
            } else {
                address >> 48 == 0xffff
            }
        }
        PagingDepth::FiveLevel => {
            if address & (1 << 56) == 0 {
                address >> 57 == 0
            } else {
                address >> 57 == 0x7f
            }
        }
    }
}

pub fn paging_depth_from_cr4(cr4: u64) -> PagingDepth {
    if cr4 & (1 << 12) == 0 {
        PagingDepth::FourLevel
    } else {
        PagingDepth::FiveLevel
    }
}

pub fn page_indices(address: u64, _depth: PagingDepth) -> [usize; 5] {
    [
        ((address >> 48) & 0x1ff) as usize,
        ((address >> 39) & 0x1ff) as usize,
        ((address >> 30) & 0x1ff) as usize,
        ((address >> 21) & 0x1ff) as usize,
        ((address >> 12) & 0x1ff) as usize,
    ]
}

#[cfg(target_os = "uefi")]
pub fn firmware_paging_depth() -> PagingDepth {
    let cr4: u64;
    // SAFETY: this runs only in the x86_64 UEFI loader after firmware has entered long mode.
    unsafe {
        core::arch::asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags));
    }
    paging_depth_from_cr4(cr4)
}
