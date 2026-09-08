use relay_loader::{PagingDepth, page_indices, paging_depth_from_cr4};

#[test]
fn paging_depth_follows_the_la57_bit() {
    assert_eq!(paging_depth_from_cr4(0), PagingDepth::FourLevel);
    assert_eq!(paging_depth_from_cr4(1 << 12), PagingDepth::FiveLevel);
}

#[test]
fn five_level_indices_preserve_the_extra_top_level() {
    let address = 0x00a5_8123_4567_8000;
    assert_eq!(
        page_indices(address, PagingDepth::FiveLevel),
        [0x0a5, 0x102, 0x08d, 0x02b, 0x078]
    );
}

#[test]
fn four_level_indices_retain_the_qemu_top_level() {
    let address = 0xffff_8000_0000_0000;
    assert_eq!(
        page_indices(address, PagingDepth::FourLevel),
        [0x1ff, 0x100, 0, 0, 0]
    );
}
