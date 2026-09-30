//! The kernel-stack area (user-space gate §5.4): the stacks the kernel runs
//! on while it serves a program (its system calls, interrupts and faults).
//! One slot per process-table entry, each a 64 KiB stack above an unmapped
//! guard page, so a kernel stack overflow faults instead of running into
//! the next stack. The area has PML4 entry 509 to itself, which every
//! address space shares.

use super::paging::{Cache, PAGE, PageTables, PhysMem};

/// The area's first address: PML4 entry 509, between the loader's kernel
/// stack (508) and the kernel image (511).
pub const AREA: u64 = 0xFFFF_FE80_0000_0000;
/// Slots, one per process-table entry (user-space gate §5.4).
pub const SLOTS: usize = 64;
/// Bytes of stack in each slot.
pub const SIZE: u64 = 64 * 1024;
/// Each slot: the guard page, then the stack; the rest stays unmapped.
const SLOT_SIZE: u64 = 128 * 1024;

/// A kernel stack in one slot; `top` is where it starts (it grows down).
#[derive(Debug, PartialEq, Eq)]
pub struct KernelStack {
    slot: usize,
}

impl KernelStack {
    /// Its slot, 0 to `SLOTS - 1`: one per process at most.
    pub fn slot(&self) -> usize {
        self.slot
    }

    /// The lowest address of the stack; the page below is the guard.
    pub fn bottom(&self) -> u64 {
        AREA + self.slot as u64 * SLOT_SIZE + PAGE
    }

    /// The first address above the stack, 16-byte aligned.
    pub fn top(&self) -> u64 {
        self.bottom() + SIZE
    }

    /// The address of each of its pages.
    pub fn pages(&self) -> impl Iterator<Item = u64> + use<> {
        let bottom = self.bottom();
        (0..SIZE / PAGE).map(move |i| bottom + i * PAGE)
    }
}

/// Why there is no kernel stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackError {
    /// Every slot is in use: one per process-table entry, so the table is
    /// full.
    NoSlot,
    /// No frames for it.
    NoMemory,
}

impl StackError {
    /// What `spawn` says: `EAGAIN` for a full table, `ENOMEM` when the
    /// memory ran out.
    pub fn errno(self) -> vfs::Errno {
        match self {
            StackError::NoSlot => vfs::Errno::EAGAIN,
            StackError::NoMemory => vfs::Errno::ENOMEM,
        }
    }
}

/// Which slots are in use.
#[derive(Default)]
pub struct KernelStacks {
    used: u64,
}

impl KernelStacks {
    pub const fn new() -> KernelStacks {
        KernelStacks { used: 0 }
    }

    /// Maps a stack in a free slot; `NoSlot` when every slot is in use,
    /// `NoMemory` when there are no frames (what was mapped is given back).
    pub fn alloc(
        &mut self,
        tables: &mut PageTables,
        mem: &mut impl PhysMem,
    ) -> Result<KernelStack, StackError> {
        let slot = (0..SLOTS)
            .find(|&s| self.used & (1 << s) == 0)
            .ok_or(StackError::NoSlot)?;
        let stack = KernelStack { slot };
        for (n, virt) in stack.pages().enumerate() {
            let mapped = mem.alloc_table().map(|frame| {
                let r = tables.map(mem, virt, frame, PAGE, Cache::WriteBack);
                if r.is_err() {
                    mem.free_frame(frame);
                }
                r
            });
            if !matches!(mapped, Some(Ok(()))) {
                release(&stack, n, tables, mem);
                return Err(StackError::NoMemory);
            }
        }
        self.used |= 1 << slot;
        Ok(stack)
    }

    /// Gives back the stack's frames and its slot. The caller flushes the
    /// TLB for its pages.
    pub fn free(&mut self, stack: KernelStack, tables: &mut PageTables, mem: &mut impl PhysMem) {
        release(&stack, (SIZE / PAGE) as usize, tables, mem);
        self.used &= !(1 << stack.slot);
    }
}

/// Unmaps and frees the first `pages` pages of `stack`.
fn release(stack: &KernelStack, pages: usize, tables: &mut PageTables, mem: &mut impl PhysMem) {
    for virt in stack.pages().take(pages) {
        if let Some(frame) = tables.unmap(mem, virt) {
            mem.free_frame(frame);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mm::testing::FakeMem;
    use boot_info::{HEAP_BASE, KERNEL_BASE, KERNEL_STACK_BOTTOM, PHYS_OFFSET};

    fn pml4_entry(virt: u64) -> u64 {
        (virt >> 39) & 0x1FF
    }

    fn setup() -> (FakeMem, PageTables) {
        let mut m = FakeMem::new();
        let mut t = PageTables::new(&mut m).unwrap();
        t.fill_upper_half(&mut m).unwrap();
        (m, t)
    }

    #[test]
    fn the_area_has_a_pml4_entry_of_its_own() {
        assert_eq!(pml4_entry(AREA), 509);
        let last = KernelStack { slot: SLOTS - 1 };
        assert_eq!(pml4_entry(last.top() - 1), 509);
        for other in [PHYS_OFFSET, HEAP_BASE, KERNEL_STACK_BOTTOM, KERNEL_BASE] {
            assert_ne!(pml4_entry(other), 509, "{other:#x}");
        }
    }

    #[test]
    fn a_stack_is_64_kib_above_an_unmapped_guard_page() {
        let (mut m, mut t) = setup();
        let mut stacks = KernelStacks::new();
        let a = stacks.alloc(&mut t, &mut m).unwrap();
        let b = stacks.alloc(&mut t, &mut m).unwrap();
        assert_eq!(a.bottom(), AREA + PAGE);
        assert_eq!(a.top(), AREA + PAGE + SIZE);
        assert_eq!(b.bottom(), AREA + SLOT_SIZE + PAGE);
        assert_eq!(a.top() % 16, 0);
        for s in [&a, &b] {
            assert_eq!(t.translate(&mut m, s.bottom() - PAGE), None, "guard");
            assert_eq!(t.translate(&mut m, s.top()), None, "above");
            for v in s.pages() {
                let (_, cache, size) = t.translate(&mut m, v).expect("mapped");
                assert_eq!((cache, size), (Cache::WriteBack, PAGE));
                assert_eq!(t.user_page(&mut m, v), None, "not for ring 3");
            }
        }
        assert_eq!(a.pages().count(), 16);
    }

    #[test]
    fn freed_stacks_give_back_their_frames_and_slot() {
        let (mut m, mut t) = setup();
        let mut stacks = KernelStacks::new();
        let first = stacks.alloc(&mut t, &mut m).unwrap();
        stacks.free(first, &mut t, &mut m);
        // The page tables of the slot stay; its 16 frames do not.
        let before = m.frames();
        let s = stacks.alloc(&mut t, &mut m).unwrap();
        assert_eq!(s, KernelStack { slot: 0 }, "the slot is used again");
        assert_eq!(m.frames(), before + 16);
        let bottom = s.bottom();
        stacks.free(s, &mut t, &mut m);
        assert_eq!(m.frames(), before);
        assert_eq!(t.translate(&mut m, bottom), None);
    }

    #[test]
    fn every_slot_can_be_used_and_no_more() {
        let (mut m, mut t) = setup();
        let mut stacks = KernelStacks::new();
        let all: Vec<KernelStack> = (0..SLOTS)
            .map(|_| stacks.alloc(&mut t, &mut m).unwrap())
            .collect();
        assert_eq!(all.last().unwrap().slot, SLOTS - 1);
        assert_eq!(stacks.alloc(&mut t, &mut m), Err(StackError::NoSlot));
        assert_eq!(
            StackError::NoSlot.errno(),
            vfs::Errno::EAGAIN,
            "a full table"
        );
        for s in all {
            stacks.free(s, &mut t, &mut m);
        }
        assert!(stacks.alloc(&mut t, &mut m).is_ok());
    }

    #[test]
    fn running_out_of_frames_leaks_nothing() {
        let (mut m, mut t) = setup();
        let mut stacks = KernelStacks::new();
        // A frame for the first page, but none for the tables it needs.
        let cold = m.frames();
        m.limit = cold + 1;
        assert_eq!(stacks.alloc(&mut t, &mut m), Err(StackError::NoMemory));
        assert_eq!(StackError::NoMemory.errno(), vfs::Errno::ENOMEM);
        assert_eq!(m.frames(), cold);
        m.limit = usize::MAX;
        // Tables for the first slot, so only the stack's own frames vary.
        let warm = stacks.alloc(&mut t, &mut m).unwrap();
        stacks.free(warm, &mut t, &mut m);
        let before = m.frames();
        for extra in [0, 1, 7, 15] {
            m.limit = before + extra;
            assert_eq!(
                stacks.alloc(&mut t, &mut m),
                Err(StackError::NoMemory),
                "{extra} frames"
            );
            assert_eq!(m.frames(), before);
        }
        m.limit = usize::MAX;
        assert_eq!(stacks.alloc(&mut t, &mut m), Ok(KernelStack { slot: 0 }));
    }
}
