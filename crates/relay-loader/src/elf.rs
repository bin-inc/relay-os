use alloc::vec::Vec;
use core::ops::{BitOr, Range};

use crate::ElfLoadError;

pub const ELF_PF_X: u32 = 1;
pub const ELF_PF_W: u32 = 2;
pub const ELF_PF_R: u32 = 4;

const ELF_HEADER_SIZE: usize = 64;
const PROGRAM_HEADER_SIZE: usize = 56;
const ELF_CLASS_64: u8 = 2;
const ELF_DATA_LITTLE_ENDIAN: u8 = 1;
const ELF_VERSION_CURRENT: u8 = 1;
const ELF_OSABI_SYSTEM_V: u8 = 0;
const ELF_TYPE_EXECUTABLE: u16 = 2;
const ELF_MACHINE_X86_64: u16 = 62;
const PROGRAM_TYPE_LOAD: u32 = 1;
const PROGRAM_TYPE_DYNAMIC: u32 = 2;
const PAGE_SIZE: u64 = 4096;
const HIGH_HALF_START: u64 = 0xffff_8000_0000_0000;
const DYNAMIC_ENTRY_SIZE: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SegmentFlags(u32);

impl SegmentFlags {
    pub const READ: Self = Self(ELF_PF_R);
    pub const WRITE: Self = Self(ELF_PF_W);
    pub const EXECUTE: Self = Self(ELF_PF_X);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for SegmentFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadPlan {
    pub entry: u64,
    pub segments: Vec<LoadSegment>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadSegment {
    pub file_range: Range<usize>,
    pub virtual_start: u64,
    pub memory_len: u64,
    pub flags: SegmentFlags,
}

pub fn parse_load_plan(bytes: &[u8]) -> Result<LoadPlan, ElfLoadError> {
    let header = bytes
        .get(..ELF_HEADER_SIZE)
        .ok_or(ElfLoadError::Truncated)?;
    if header.get(..4) != Some(b"\x7fELF") {
        return Err(ElfLoadError::InvalidMagic);
    }
    if header[4] != ELF_CLASS_64 {
        return Err(ElfLoadError::UnsupportedClass);
    }
    if header[5] != ELF_DATA_LITTLE_ENDIAN {
        return Err(ElfLoadError::UnsupportedEndianness);
    }
    if header[6] != ELF_VERSION_CURRENT || read_u32(header, 20)? != 1 {
        return Err(ElfLoadError::UnsupportedVersion);
    }
    if header[7] != ELF_OSABI_SYSTEM_V || header[8] != 0 || header[9..16] != [0; 7] {
        return Err(ElfLoadError::UnsupportedAbi);
    }
    if read_u16(header, 16)? != ELF_TYPE_EXECUTABLE {
        return Err(ElfLoadError::UnsupportedType);
    }
    if read_u16(header, 18)? != ELF_MACHINE_X86_64 {
        return Err(ElfLoadError::UnsupportedMachine);
    }
    if read_u16(header, 52)? as usize != ELF_HEADER_SIZE {
        return Err(ElfLoadError::InvalidHeaderSize);
    }
    if read_u16(header, 54)? as usize != PROGRAM_HEADER_SIZE {
        return Err(ElfLoadError::InvalidProgramHeaderSize);
    }

    let entry = read_u64(header, 24)?;
    let program_header_offset =
        usize::try_from(read_u64(header, 32)?).map_err(|_| ElfLoadError::IntegerOverflow)?;
    let program_header_count = usize::from(read_u16(header, 56)?);
    let program_header_len = program_header_count
        .checked_mul(PROGRAM_HEADER_SIZE)
        .ok_or(ElfLoadError::IntegerOverflow)?;
    let program_header_end = program_header_offset
        .checked_add(program_header_len)
        .ok_or(ElfLoadError::IntegerOverflow)?;
    let program_headers = bytes
        .get(program_header_offset..program_header_end)
        .ok_or(ElfLoadError::ProgramHeaderTableOutOfBounds)?;

    let mut segments = Vec::new();
    for index in 0..program_header_count {
        let offset = index
            .checked_mul(PROGRAM_HEADER_SIZE)
            .ok_or(ElfLoadError::IntegerOverflow)?;
        let end = offset
            .checked_add(PROGRAM_HEADER_SIZE)
            .ok_or(ElfLoadError::IntegerOverflow)?;
        let program_header = program_headers
            .get(offset..end)
            .ok_or(ElfLoadError::Truncated)?;

        match read_u32(program_header, 0)? {
            PROGRAM_TYPE_LOAD => add_load_segment(bytes, program_header, &mut segments)?,
            PROGRAM_TYPE_DYNAMIC => validate_dynamic_table(bytes, program_header)?,
            _ => {}
        }
    }

    let entry_in_executable_segment = segments.iter().any(|segment| {
        segment.flags.contains(SegmentFlags::EXECUTE)
            && segment
                .virtual_start
                .checked_add(segment.memory_len)
                .is_some_and(|end| segment.virtual_start <= entry && entry < end)
    });
    if !entry_in_executable_segment {
        return Err(ElfLoadError::EntryOutsideExecutableSegment);
    }

    Ok(LoadPlan { entry, segments })
}

fn add_load_segment(
    bytes: &[u8],
    program_header: &[u8],
    segments: &mut Vec<LoadSegment>,
) -> Result<(), ElfLoadError> {
    let raw_flags = read_u32(program_header, 4)?;
    if raw_flags & !(ELF_PF_R | ELF_PF_W | ELF_PF_X) != 0 {
        return Err(ElfLoadError::InvalidSegmentFlags);
    }
    let flags = SegmentFlags(raw_flags);
    if flags.contains(SegmentFlags::WRITE) && flags.contains(SegmentFlags::EXECUTE) {
        return Err(ElfLoadError::WritableExecutable);
    }

    let file_offset = read_u64(program_header, 8)?;
    let virtual_start = read_u64(program_header, 16)?;
    let file_len = read_u64(program_header, 32)?;
    let memory_len = read_u64(program_header, 40)?;
    if file_len > memory_len {
        return Err(ElfLoadError::FileSizeExceedsMemorySize);
    }
    if read_u64(program_header, 48)? != PAGE_SIZE {
        return Err(ElfLoadError::InvalidSegmentAlignment);
    }
    if file_offset % PAGE_SIZE != virtual_start % PAGE_SIZE {
        return Err(ElfLoadError::OffsetNotPageCongruent);
    }
    if virtual_start < HIGH_HALF_START {
        return Err(ElfLoadError::NotHighHalf);
    }

    let file_start = usize::try_from(file_offset).map_err(|_| ElfLoadError::IntegerOverflow)?;
    let file_len = usize::try_from(file_len).map_err(|_| ElfLoadError::IntegerOverflow)?;
    let file_end = file_start
        .checked_add(file_len)
        .ok_or(ElfLoadError::IntegerOverflow)?;
    if bytes.get(file_start..file_end).is_none() {
        return Err(ElfLoadError::SegmentOutOfBounds);
    }
    let virtual_end = virtual_start
        .checked_add(memory_len)
        .ok_or(ElfLoadError::IntegerOverflow)?;

    for segment in segments.iter() {
        let segment_end = segment
            .virtual_start
            .checked_add(segment.memory_len)
            .ok_or(ElfLoadError::IntegerOverflow)?;
        if virtual_start < segment_end && segment.virtual_start < virtual_end {
            return Err(ElfLoadError::OverlappingSegments);
        }
    }

    segments.push(LoadSegment {
        file_range: file_start..file_end,
        virtual_start,
        memory_len,
        flags,
    });
    Ok(())
}

fn validate_dynamic_table(bytes: &[u8], program_header: &[u8]) -> Result<(), ElfLoadError> {
    let file_offset =
        usize::try_from(read_u64(program_header, 8)?).map_err(|_| ElfLoadError::IntegerOverflow)?;
    let file_len = usize::try_from(read_u64(program_header, 32)?)
        .map_err(|_| ElfLoadError::IntegerOverflow)?;
    if file_len % DYNAMIC_ENTRY_SIZE != 0 {
        return Err(ElfLoadError::InvalidDynamicTable);
    }
    let file_end = file_offset
        .checked_add(file_len)
        .ok_or(ElfLoadError::IntegerOverflow)?;
    let table = bytes
        .get(file_offset..file_end)
        .ok_or(ElfLoadError::InvalidDynamicTable)?;

    for entry in table.as_chunks::<DYNAMIC_ENTRY_SIZE>().0 {
        match read_u64(entry, 0)? {
            0 => return Ok(()),
            2 | 7 | 8 | 9 | 17 | 18 | 19 | 23 | 35 | 36 | 37 => {
                return Err(ElfLoadError::DynamicRelocations);
            }
            _ => {}
        }
    }
    Err(ElfLoadError::InvalidDynamicTable)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, ElfLoadError> {
    let end = offset.checked_add(2).ok_or(ElfLoadError::IntegerOverflow)?;
    let value = bytes.get(offset..end).ok_or(ElfLoadError::Truncated)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ElfLoadError> {
    let end = offset.checked_add(4).ok_or(ElfLoadError::IntegerOverflow)?;
    let value = bytes.get(offset..end).ok_or(ElfLoadError::Truncated)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, ElfLoadError> {
    let end = offset.checked_add(8).ok_or(ElfLoadError::IntegerOverflow)?;
    let value = bytes.get(offset..end).ok_or(ElfLoadError::Truncated)?;
    Ok(u64::from_le_bytes([
        value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
    ]))
}
