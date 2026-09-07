use relay_loader::{
    ELF_PF_R, ELF_PF_W, ELF_PF_X, ElfLoadError, LoadPlan, LoadSegment, SegmentFlags,
    parse_load_plan,
};

const HIGH_HALF_START: u64 = 0xffff_8000_0000_0000;
const SEGMENT_OFFSET: usize = 0x1000;
const SEGMENT_ADDRESS: u64 = HIGH_HALF_START + 0x1000;

#[test]
fn elf_produces_an_exact_plan_for_a_fixed_high_half_load_segment() {
    let image = fixture_with_segment_flags(ELF_PF_R | ELF_PF_X);

    assert_eq!(
        parse_load_plan(&image),
        Ok(LoadPlan {
            entry: SEGMENT_ADDRESS + 4,
            segments: vec![LoadSegment {
                file_range: SEGMENT_OFFSET..SEGMENT_OFFSET + 16,
                virtual_start: SEGMENT_ADDRESS,
                memory_len: 0x20,
                flags: SegmentFlags::READ | SegmentFlags::EXECUTE,
            }],
        })
    );
}

#[test]
fn elf_rejects_writable_executable_segment() {
    let image = fixture_with_segment_flags(ELF_PF_R | ELF_PF_W | ELF_PF_X);
    assert_eq!(
        parse_load_plan(&image),
        Err(ElfLoadError::WritableExecutable)
    );
}

#[test]
fn elf_rejects_low_memory_overlapping_and_dynamic_loads() {
    let mut low_memory = fixture_with_segment_flags(ELF_PF_R | ELF_PF_X);
    put_u64(&mut low_memory, 64 + 16, 0x1000);
    assert_eq!(parse_load_plan(&low_memory), Err(ElfLoadError::NotHighHalf));

    let mut overlapping = fixture_with_segment_flags(ELF_PF_R | ELF_PF_X);
    put_u64(&mut overlapping, 104, 0x2000);
    add_load_segment(
        &mut overlapping,
        SEGMENT_OFFSET + 0x1000,
        SEGMENT_ADDRESS + 0x1000,
        ELF_PF_R,
    );
    assert_eq!(
        parse_load_plan(&overlapping),
        Err(ElfLoadError::OverlappingSegments)
    );

    let mut dynamic = fixture_with_segment_flags(ELF_PF_R | ELF_PF_X);
    put_u32(&mut dynamic, 64, 2);
    put_u64(&mut dynamic, SEGMENT_OFFSET, 7);
    assert_eq!(
        parse_load_plan(&dynamic),
        Err(ElfLoadError::DynamicRelocations)
    );
}

#[test]
fn elf_rejects_terminated_dynamic_relr_requirement() {
    let mut dynamic = fixture_with_segment_flags(ELF_PF_R | ELF_PF_X);
    dynamic.resize(SEGMENT_OFFSET + 32, 0);
    put_u32(&mut dynamic, 64, 2);
    put_u64(&mut dynamic, 96, 32);
    put_u64(&mut dynamic, SEGMENT_OFFSET, 36);

    assert_eq!(
        parse_load_plan(&dynamic),
        Err(ElfLoadError::DynamicRelocations)
    );
}

#[test]
fn elf_returns_typed_errors_without_panicking_for_truncated_or_fuzz_sized_inputs() {
    let valid = fixture_with_segment_flags(ELF_PF_R | ELF_PF_X);
    for length in 0..valid.len() {
        let bytes = &valid[..length];
        let result = std::panic::catch_unwind(|| parse_load_plan(&bytes[..length]));
        assert!(result.is_ok(), "parser panicked for {length} bytes");
        assert!(result.unwrap().is_err(), "parser accepted {length} bytes");
    }

    for length in [0, 1, 7, 63, 64, 65, 127, 511, 4096] {
        let bytes = (0..length)
            .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
            .collect::<Vec<_>>();
        let result = std::panic::catch_unwind(|| parse_load_plan(&bytes));
        assert!(
            result.is_ok(),
            "parser panicked for fuzz corpus size {length}"
        );
        assert!(
            result.unwrap().is_err(),
            "parser accepted fuzz corpus size {length}"
        );
    }
}

fn fixture_with_segment_flags(flags: u32) -> Vec<u8> {
    let mut image = vec![0; SEGMENT_OFFSET + 16];
    image[..16].copy_from_slice(b"\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00");
    put_u16(&mut image, 16, 2);
    put_u16(&mut image, 18, 62);
    put_u32(&mut image, 20, 1);
    put_u64(&mut image, 24, SEGMENT_ADDRESS + 4);
    put_u64(&mut image, 32, 64);
    put_u16(&mut image, 52, 64);
    put_u16(&mut image, 54, 56);
    put_u16(&mut image, 56, 1);
    put_u32(&mut image, 64, 1);
    put_u32(&mut image, 68, flags);
    put_u64(&mut image, 72, SEGMENT_OFFSET as u64);
    put_u64(&mut image, 80, SEGMENT_ADDRESS);
    put_u64(&mut image, 96, 16);
    put_u64(&mut image, 104, 0x20);
    put_u64(&mut image, 112, 0x1000);
    image[SEGMENT_OFFSET..SEGMENT_OFFSET + 16].copy_from_slice(b"relay-kernel-v1!");
    image
}

fn add_load_segment(image: &mut Vec<u8>, offset: usize, address: u64, flags: u32) {
    image.resize(offset + 16, 0);
    put_u16(image, 56, 2);
    let header = 64 + 56;
    put_u32(image, header, 1);
    put_u32(image, header + 4, flags);
    put_u64(image, header + 8, offset as u64);
    put_u64(image, header + 16, address);
    put_u64(image, header + 32, 16);
    put_u64(image, header + 40, 0x20);
    put_u64(image, header + 48, 0x1000);
}

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
