use core::mem::{align_of, size_of};
use core::str::FromStr;

use relay_abi::{BootInfo, FramebufferInfo, GptGuid, MemoryRegion};

#[test]
fn boot_info_has_stable_layout() {
    assert_eq!(relay_abi::BOOT_INFO_MAGIC, 0x5245_4c41_5942_4f4f);
    assert_eq!(relay_abi::BOOT_ABI_VERSION, 1);
    assert_eq!(align_of::<BootInfo>(), 8);
    assert_eq!(size_of::<GptGuid>(), 16);
    assert_eq!(size_of::<MemoryRegion>(), 24);
    assert_eq!(size_of::<FramebufferInfo>(), 56);
    assert_eq!(size_of::<BootInfo>(), 120);
}

#[test]
fn guid_round_trips_canonical_text_and_gpt_bytes() {
    let guid = GptGuid::from_str("52454c41-5900-4000-8000-000000000003").unwrap();

    assert_eq!(guid.to_string(), "52454c41-5900-4000-8000-000000000003");
    assert_eq!(
        guid.to_gpt_bytes(),
        [
            0x41, 0x4c, 0x45, 0x52, 0x00, 0x59, 0x00, 0x40, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x03,
        ]
    );
    assert_eq!(GptGuid::from_gpt_bytes(guid.to_gpt_bytes()), guid);
}

#[test]
fn guid_rejects_noncanonical_text() {
    assert!(GptGuid::from_str("52454C41-5900-4000-8000-000000000003").is_err());
    assert!(GptGuid::from_str("52454c41-5900-4000-8000-00000000003").is_err());
}
