//! Just enough ELF64 parsing to load the kernel: the file header and the
//! PT_LOAD program headers. Pure functions over a byte slice.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub vaddr: u64,
    pub file_offset: u64,
    pub file_size: u64,
    pub mem_size: u64,
    pub writable: bool,
    pub executable: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KernelElf {
    pub entry: u64,
    pub segments: [Option<Segment>; 8],
}

impl KernelElf {
    pub fn segments(&self) -> impl Iterator<Item = &Segment> {
        self.segments.iter().flatten()
    }

    /// Page-aligned [start, end) virtual range covering every segment.
    pub fn span(&self) -> (u64, u64) {
        let start = self.segments().map(|s| s.vaddr).min().unwrap_or(0) & !0xFFF;
        let end = self
            .segments()
            .map(|s| s.vaddr + s.mem_size)
            .max()
            .unwrap_or(0);
        (start, (end + 0xFFF) & !0xFFF)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ElfError {
    TooShort,
    NotElf64LittleEndian,
    NotExecutable,
    NotX86_64,
    TooManySegments,
    SegmentOutOfFile,
    BelowKernelBase,
}

const PT_LOAD: u32 = 1;
const PF_X: u32 = 1;
const PF_W: u32 = 2;

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

/// Parses a static x86_64 ELF executable linked at or above `min_vaddr`.
pub fn parse(file: &[u8], min_vaddr: u64) -> Result<KernelElf, ElfError> {
    if file.len() < 64 {
        return Err(ElfError::TooShort);
    }
    if &file[0..4] != b"\x7fELF" || file[4] != 2 || file[5] != 1 {
        return Err(ElfError::NotElf64LittleEndian);
    }
    if u16_at(file, 16) != 2 {
        return Err(ElfError::NotExecutable); // ET_EXEC only: no relocations to apply
    }
    if u16_at(file, 18) != 62 {
        return Err(ElfError::NotX86_64);
    }
    let entry = u64_at(file, 24);
    let phoff = u64_at(file, 32) as usize;
    let phentsize = u16_at(file, 54) as usize;
    let phnum = u16_at(file, 56) as usize;
    if phentsize < 56
        || phoff
            .checked_add(phnum * phentsize)
            .is_none_or(|end| end > file.len())
    {
        return Err(ElfError::TooShort);
    }
    let mut segments = [None; 8];
    let mut n = 0;
    for i in 0..phnum {
        let ph = &file[phoff + i * phentsize..];
        if u32_at(ph, 0) != PT_LOAD {
            continue;
        }
        let flags = u32_at(ph, 4);
        let seg = Segment {
            file_offset: u64_at(ph, 8),
            vaddr: u64_at(ph, 16),
            file_size: u64_at(ph, 32),
            mem_size: u64_at(ph, 40),
            writable: flags & PF_W != 0,
            executable: flags & PF_X != 0,
        };
        if seg
            .file_offset
            .checked_add(seg.file_size)
            .is_none_or(|e| e > file.len() as u64)
            || seg.file_size > seg.mem_size
        {
            return Err(ElfError::SegmentOutOfFile);
        }
        if seg.vaddr < min_vaddr {
            return Err(ElfError::BelowKernelBase);
        }
        *segments.get_mut(n).ok_or(ElfError::TooManySegments)? = Some(seg);
        n += 1;
    }
    Ok(KernelElf { entry, segments })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal ELF64 with the given program headers
    /// (type, flags, offset, vaddr, filesz, memsz).
    fn elf(etype: u16, machine: u16, phdrs: &[(u32, u32, u64, u64, u64, u64)]) -> Vec<u8> {
        let mut f = vec![0u8; 64 + 56 * phdrs.len() + 0x100];
        f[0..4].copy_from_slice(b"\x7fELF");
        f[4] = 2;
        f[5] = 1;
        f[16..18].copy_from_slice(&etype.to_le_bytes());
        f[18..20].copy_from_slice(&machine.to_le_bytes());
        f[24..32].copy_from_slice(&0xFFFF_FFFF_8000_0010u64.to_le_bytes());
        f[32..40].copy_from_slice(&64u64.to_le_bytes());
        f[54..56].copy_from_slice(&56u16.to_le_bytes());
        f[56..58].copy_from_slice(&(phdrs.len() as u16).to_le_bytes());
        for (i, &(ty, flags, off, vaddr, filesz, memsz)) in phdrs.iter().enumerate() {
            let p = 64 + i * 56;
            f[p..p + 4].copy_from_slice(&ty.to_le_bytes());
            f[p + 4..p + 8].copy_from_slice(&flags.to_le_bytes());
            f[p + 8..p + 16].copy_from_slice(&off.to_le_bytes());
            f[p + 16..p + 24].copy_from_slice(&vaddr.to_le_bytes());
            f[p + 32..p + 40].copy_from_slice(&filesz.to_le_bytes());
            f[p + 40..p + 48].copy_from_slice(&memsz.to_le_bytes());
        }
        f
    }

    const BASE: u64 = 0xFFFF_FFFF_8000_0000;

    #[test]
    fn parses_load_segments_and_span() {
        let f = elf(
            2,
            62,
            &[
                (1, 5, 0, BASE, 0x10, 0x10),
                (4, 4, 0, 0, 0, 0), // PT_NOTE ignored
                (1, 6, 0x20, BASE + 0x2000, 0x10, 0x3000),
            ],
        );
        let k = parse(&f, BASE).unwrap();
        assert_eq!(k.entry, BASE + 0x10);
        let segs: Vec<_> = k.segments().collect();
        assert_eq!(segs.len(), 2);
        assert!(segs[0].executable && !segs[0].writable);
        assert!(segs[1].writable && !segs[1].executable);
        assert_eq!(k.span(), (BASE, BASE + 0x5000));
    }

    #[test]
    fn rejects_bad_files() {
        assert_eq!(parse(&[0; 10], BASE), Err(ElfError::TooShort));
        assert_eq!(parse(&[0; 64], BASE), Err(ElfError::NotElf64LittleEndian));
        assert_eq!(parse(&elf(3, 62, &[]), BASE), Err(ElfError::NotExecutable));
        assert_eq!(parse(&elf(2, 40, &[]), BASE), Err(ElfError::NotX86_64));
        assert_eq!(
            parse(&elf(2, 62, &[(1, 5, 0, 0x1000, 0x10, 0x10)]), BASE),
            Err(ElfError::BelowKernelBase)
        );
        assert_eq!(
            parse(&elf(2, 62, &[(1, 5, 0, BASE, 0x10_0000, 0x10_0000)]), BASE),
            Err(ElfError::SegmentOutOfFile)
        );
        assert_eq!(
            parse(&elf(2, 62, &[(1, 5, 0, BASE, 0x20, 0x10)]), BASE),
            Err(ElfError::SegmentOutOfFile)
        );
    }
}
