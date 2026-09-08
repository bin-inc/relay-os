use relay_core::memory::{FrameAllocator, MemoryRegion, RegionKind, ReservedRange};

#[test]
fn frame_allocator_skips_reserved_pages_and_exhausts_usable_regions() {
    let regions = [
        MemoryRegion::new(0x1000, 0x5000, RegionKind::Usable),
        MemoryRegion::new(0x5000, 0x7000, RegionKind::Reserved),
    ];
    let reserved = [ReservedRange::new(0x2000, 0x3000)];
    let mut allocator = FrameAllocator::new(&regions, &reserved).unwrap();

    assert_eq!(allocator.allocate_frame(), Some(0x1000));
    assert_eq!(allocator.allocate_frame(), Some(0x3000));
    assert_eq!(allocator.allocate_frame(), Some(0x4000));
    assert_eq!(allocator.allocate_frame(), None);
}

#[test]
fn frame_allocator_rejects_unaligned_and_overlapping_regions() {
    assert!(
        FrameAllocator::new(
            &[MemoryRegion::new(0x1001, 0x2000, RegionKind::Usable)],
            &[],
        )
        .is_err()
    );
    assert!(
        FrameAllocator::new(
            &[
                MemoryRegion::new(0x1000, 0x3000, RegionKind::Usable),
                MemoryRegion::new(0x2000, 0x4000, RegionKind::Usable),
            ],
            &[],
        )
        .is_err()
    );
}
