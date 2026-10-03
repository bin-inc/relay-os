//! Host-testable parts of the Relay OS UEFI loader.
#![cfg_attr(not(test), no_std)]

pub mod cmdline;
#[cfg(test)]
mod code;
pub mod elf;
pub mod memmap;
pub mod mode;

#[cfg(test)]
mod tests {
    use crate::code::code;

    /// Every source file of the loader but this one, which names the banned
    /// words itself, and `code.rs`, whose tests do.
    const SOURCES: [(&str, &str); 8] = [
        ("cmdline.rs", include_str!("cmdline.rs")),
        ("elf.rs", include_str!("elf.rs")),
        ("main.rs", include_str!("main.rs")),
        ("memmap.rs", include_str!("memmap.rs")),
        ("mode.rs", include_str!("mode.rs")),
        ("paging.rs", include_str!("paging.rs")),
        ("proto.rs", include_str!("proto.rs")),
        ("video.rs", include_str!("video.rs")),
    ];

    #[test]
    fn every_source_file_is_checked() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|f| f != "lib.rs" && f != "code.rs")
            .collect();
        files.sort();
        let checked: Vec<&str> = SOURCES.iter().map(|(f, _)| *f).collect();
        assert_eq!(files, checked);
    }

    /// On the NUC 12 firmware, exclusive opens stop the firmware's own
    /// drivers (the text console stops drawing once the GOP is held) and a
    /// later ExitBootServices never returns. Like the Linux EFI stub, the
    /// loader only uses non-exclusive GET_PROTOCOL opens (`proto::get`).
    #[test]
    fn loader_avoids_firmware_features_linux_does_not_use() {
        for (file, src) in SOURCES {
            let src = code(src);
            // OS-defined memory types (0x8000_0000+) are legal but never used
            // by the Linux EFI stub; the loader sticks to standard types.
            for banned in [
                "open_protocol_exclusive",
                "get_image_file_system",
                "Exclusive",
                "MemoryType::custom",
            ] {
                assert!(!src.contains(banned), "{file} uses {banned}");
            }
        }
    }
}
