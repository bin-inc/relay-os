//! Host-testable parts of the Relay OS UEFI loader.
#![cfg_attr(not(test), no_std)]

pub mod cmdline;
pub mod elf;
pub mod memmap;
pub mod mode;

#[cfg(test)]
mod tests {
    /// On the NUC 12 firmware, exclusive opens stop the firmware's own
    /// drivers (the text console stops drawing once the GOP is held) and a
    /// later ExitBootServices never returns. Like the Linux EFI stub, the
    /// loader only uses non-exclusive GET_PROTOCOL opens (`proto::get`).
    #[test]
    fn loader_never_opens_firmware_protocols_exclusively() {
        for (file, src) in [
            ("main.rs", include_str!("main.rs")),
            ("video.rs", include_str!("video.rs")),
            ("paging.rs", include_str!("paging.rs")),
        ] {
            for banned in [
                "open_protocol_exclusive",
                "get_image_file_system",
                "Exclusive",
            ] {
                assert!(!src.contains(banned), "{file} uses {banned}");
            }
        }
    }
}
