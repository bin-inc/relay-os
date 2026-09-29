//! [`check`]: the file header, the program headers, the loadable segments,
//! the entry point and the ABI note, in that order.

use alloc::vec::Vec;
use core::fmt;

/// The largest program the kernel loads (spec §5.2).
pub const MAX_SIZE: usize = 16 << 20;
/// Where a program's segments may lie (spec §5.1): above the unmapped
/// first 4 MiB, below the `mem_map` area.
pub const PROGRAM_BASE: u64 = 0x40_0000;
pub const PROGRAM_END: u64 = 0x1000_0000_0000;
/// `e_machine` of x86_64 programs.
pub const EM_X86_64: u16 = 62;

const HEADER_LEN: usize = 64;
const PHDR_LEN: usize = 56;
const ET_EXEC: u16 = 2;
const PT_LOAD: u32 = 1;
const PT_NOTE: u32 = 4;
const PT_GNU_STACK: u32 = 0x6474_e551;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PAGE: u64 = 4096;

/// What the program may do with a segment's pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    ReadExec,
    Read,
    ReadWrite,
}

/// A loadable segment: `memsz` bytes at `vaddr`, the first `filesz` of
/// them from the file at `offset`, the rest zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub vaddr: u64,
    pub memsz: u64,
    pub offset: u64,
    pub filesz: u64,
    pub access: Access,
}

impl Segment {
    /// The first address after it.
    pub fn end(&self) -> u64 {
        self.vaddr + self.memsz
    }
}

/// A program that passed every check: its entry point and its loadable
/// segments, in address order, on pages of their own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub entry: u64,
    pub segments: Vec<Segment>,
}

/// Why a file is not a program the kernel runs (`ENOEXEC`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElfError {
    TooBig(usize),
    NotElf,
    Not64Bit,
    NotLittleEndian,
    NotExec(u16),
    Machine(u16),
    ProgramHeaders,
    SegmentKind(u32),
    NoLoad,
    Outside { vaddr: u64 },
    WritableAndExecutable { vaddr: u64 },
    MoreFileThanMemory { vaddr: u64 },
    NotCongruent { vaddr: u64 },
    Overlap { vaddr: u64 },
    PastTheFile { vaddr: u64 },
    Entry(u64),
    NoteOutside,
    NotePastItsSegment,
    NoteType(u32),
    NoteSize(u32),
    NoNote,
    Abi(u32),
}

impl fmt::Display for ElfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ElfError::TooBig(n) => write!(f, "{n} bytes, more than 16 MiB"),
            ElfError::NotElf => write!(f, "not an ELF file"),
            ElfError::Not64Bit => write!(f, "not ELF64"),
            ElfError::NotLittleEndian => write!(f, "not little-endian"),
            ElfError::NotExec(t) => write!(f, "type {t}, not EXEC"),
            ElfError::Machine(m) => write!(f, "machine {m}"),
            ElfError::ProgramHeaders => write!(f, "the program headers lie outside the file"),
            ElfError::SegmentKind(k) => write!(
                f,
                "has a segment of type {k:#x}; only LOAD, NOTE and GNU_STACK are allowed"
            ),
            ElfError::NoLoad => write!(f, "no loadable segment"),
            ElfError::Outside { vaddr } => write!(
                f,
                "segment at {vaddr:#x} is outside {PROGRAM_BASE:#x}..{PROGRAM_END:#x}"
            ),
            ElfError::WritableAndExecutable { vaddr } => {
                write!(f, "segment at {vaddr:#x} is writable and executable")
            }
            ElfError::MoreFileThanMemory { vaddr } => {
                write!(f, "segment at {vaddr:#x} has more file than memory")
            }
            ElfError::NotCongruent { vaddr } => write!(
                f,
                "segment at {vaddr:#x}: offset and address are not congruent modulo 4 KiB"
            ),
            ElfError::Overlap { vaddr } => {
                write!(f, "segment at {vaddr:#x} overlaps the one before it")
            }
            ElfError::PastTheFile { vaddr } => {
                write!(f, "segment at {vaddr:#x} runs past the end of the file")
            }
            ElfError::Entry(e) => write!(f, "entry point {e:#x} is not in an executable segment"),
            ElfError::NoteOutside => write!(f, "a PT_NOTE segment lies outside the file"),
            ElfError::NotePastItsSegment => write!(f, "a note runs past its segment"),
            ElfError::NoteType(t) => write!(f, "the Relay note has type {t}, not 1"),
            ElfError::NoteSize(n) => write!(f, "the Relay note holds {n} bytes, not 4"),
            ElfError::NoNote => write!(f, "no PT_NOTE segment holds the Relay note"),
            ElfError::Abi(v) => write!(f, "built for ABI {v}"),
        }
    }
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from(u32_at(b, at)) | u64::from(u32_at(b, at + 4)) << 32
}

/// The bytes `offset..offset + len` of `file`, if they are all in it.
fn range(file: &[u8], offset: u64, len: u64) -> Option<&[u8]> {
    let start = usize::try_from(offset).ok()?;
    let end = start.checked_add(usize::try_from(len).ok()?)?;
    file.get(start..end)
}

/// Checks `file` against spec §5.2 for a machine of type `machine` and ABI
/// version `abi`, and returns what the kernel needs to load it.
pub fn check(file: &[u8], machine: u16, abi: u32) -> Result<Program, ElfError> {
    if file.len() > MAX_SIZE {
        return Err(ElfError::TooBig(file.len()));
    }
    if file.len() < HEADER_LEN || file[..4] != *b"\x7fELF" {
        return Err(ElfError::NotElf);
    }
    if file[4] != 2 {
        return Err(ElfError::Not64Bit);
    }
    if file[5] != 1 {
        return Err(ElfError::NotLittleEndian);
    }
    let kind = u16_at(file, 16);
    if kind != ET_EXEC {
        return Err(ElfError::NotExec(kind));
    }
    let m = u16_at(file, 18);
    if m != machine {
        return Err(ElfError::Machine(m));
    }
    let entry = u64_at(file, 24);
    let phoff = u64_at(file, 32);
    let phentsize = usize::from(u16_at(file, 54));
    let phnum = u64::from(u16_at(file, 56));
    if phentsize != PHDR_LEN {
        return Err(ElfError::ProgramHeaders);
    }
    let phdrs = range(file, phoff, phnum * PHDR_LEN as u64).ok_or(ElfError::ProgramHeaders)?;

    let mut segments = Vec::new();
    let mut notes = Vec::new();
    for ph in phdrs.as_chunks::<PHDR_LEN>().0 {
        let (kind, flags) = (u32_at(ph, 0), u32_at(ph, 4));
        let (offset, vaddr) = (u64_at(ph, 8), u64_at(ph, 16));
        let (filesz, memsz) = (u64_at(ph, 32), u64_at(ph, 40));
        match kind {
            PT_LOAD => {
                let access = if flags & PF_X != 0 {
                    if flags & PF_W != 0 {
                        return Err(ElfError::WritableAndExecutable { vaddr });
                    }
                    Access::ReadExec
                } else if flags & PF_W != 0 {
                    Access::ReadWrite
                } else {
                    Access::Read
                };
                segments.push(Segment {
                    vaddr,
                    memsz,
                    offset,
                    filesz,
                    access,
                });
            }
            PT_NOTE => notes.push(range(file, offset, filesz).ok_or(ElfError::NoteOutside)?),
            PT_GNU_STACK => {}
            other => return Err(ElfError::SegmentKind(other)),
        }
    }
    if segments.is_empty() {
        return Err(ElfError::NoLoad);
    }
    segments.sort_by_key(|s| s.vaddr);
    let mut previous_end = 0;
    for s in &segments {
        let vaddr = s.vaddr;
        match vaddr.checked_add(s.memsz) {
            Some(end) if vaddr >= PROGRAM_BASE && end <= PROGRAM_END => {}
            _ => return Err(ElfError::Outside { vaddr }),
        }
        if s.filesz > s.memsz {
            return Err(ElfError::MoreFileThanMemory { vaddr });
        }
        if s.offset % PAGE != vaddr % PAGE {
            return Err(ElfError::NotCongruent { vaddr });
        }
        if range(file, s.offset, s.filesz).is_none() {
            return Err(ElfError::PastTheFile { vaddr });
        }
        // Segments may not share a page: each page gets one permission.
        if vaddr & !(PAGE - 1) < previous_end {
            return Err(ElfError::Overlap { vaddr });
        }
        previous_end = s.end();
    }
    let executable = |s: &&Segment| s.access == Access::ReadExec;
    if !segments
        .iter()
        .filter(executable)
        .any(|s| (s.vaddr..s.end()).contains(&entry))
    {
        return Err(ElfError::Entry(entry));
    }
    let mut version = None;
    for n in notes {
        if let Some(v) = relay_note(n)? {
            version = Some(v);
        }
    }
    match version {
        None => Err(ElfError::NoNote),
        Some(v) if v != abi => Err(ElfError::Abi(v)),
        Some(_) => Ok(Program { entry, segments }),
    }
}

/// The ABI version in the `Relay` note among `notes` (a `PT_NOTE`
/// segment's bytes), if it has one: each note is `namesz`, `descsz`,
/// `type`, then the name and the descriptor, each padded to 4 bytes.
fn relay_note(notes: &[u8]) -> Result<Option<u32>, ElfError> {
    let mut at = 0usize;
    while at + 12 <= notes.len() {
        let namesz = u32_at(notes, at) as usize;
        let descsz = u32_at(notes, at + 4);
        let kind = u32_at(notes, at + 8);
        let name_at = at + 12;
        let desc_at = name_at
            .checked_add(namesz.next_multiple_of(4))
            .ok_or(ElfError::NotePastItsSegment)?;
        let next = desc_at
            .checked_add((descsz as usize).next_multiple_of(4))
            .filter(|&next| next <= notes.len())
            .ok_or(ElfError::NotePastItsSegment)?;
        let name = &notes[name_at..name_at + namesz];
        if name.strip_suffix(&[0]).unwrap_or(name) == relay_abi::NOTE_NAME {
            if kind != relay_abi::NOTE_TYPE {
                return Err(ElfError::NoteType(kind));
            }
            if descsz != 4 {
                return Err(ElfError::NoteSize(descsz));
            }
            return Ok(Some(u32_at(notes, desc_at)));
        }
        at = next;
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_abi::VERSION;

    /// A program header: type, flags, offset, vaddr, filesz, memsz.
    type Phdr = (u32, u32, u64, u64, u64, u64);

    const NOTE_AT: u64 = 0x2000;

    /// The `Relay` note with ABI `v`, as `relay-rt` lays it out.
    fn note(v: u32) -> Vec<u8> {
        let mut n = Vec::new();
        n.extend_from_slice(&6u32.to_le_bytes());
        n.extend_from_slice(&4u32.to_le_bytes());
        n.extend_from_slice(&1u32.to_le_bytes());
        n.extend_from_slice(b"Relay\0\0\0");
        n.extend_from_slice(&v.to_le_bytes());
        n
    }

    /// A program shaped like `t-args`: code at 0x401000, read-only data
    /// holding the note at 0x402000, data and bss at 0x403000.
    fn phdrs() -> Vec<Phdr> {
        vec![
            (PT_LOAD, 5, 0x1000, 0x40_1000, 0x800, 0x800),
            (PT_LOAD, 4, NOTE_AT, 0x40_2000, 0x40, 0x40),
            (PT_LOAD, 6, 0x3000, 0x40_3000, 0x10, 0x2000),
            (PT_NOTE, 4, NOTE_AT, 0x40_2000, 0x18, 0x18),
            (PT_GNU_STACK, 6, 0, 0, 0, 0),
        ]
    }

    fn elf(phdrs: &[Phdr], entry: u64) -> Vec<u8> {
        let mut f = vec![0u8; 0x3010];
        f[..4].copy_from_slice(b"\x7fELF");
        f[4] = 2;
        f[5] = 1;
        f[6] = 1;
        f[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
        f[18..20].copy_from_slice(&EM_X86_64.to_le_bytes());
        f[24..32].copy_from_slice(&entry.to_le_bytes());
        f[32..40].copy_from_slice(&64u64.to_le_bytes());
        f[54..56].copy_from_slice(&(PHDR_LEN as u16).to_le_bytes());
        f[56..58].copy_from_slice(&(phdrs.len() as u16).to_le_bytes());
        for (i, &(kind, flags, offset, vaddr, filesz, memsz)) in phdrs.iter().enumerate() {
            let at = 64 + i * PHDR_LEN;
            f[at..at + 4].copy_from_slice(&kind.to_le_bytes());
            f[at + 4..at + 8].copy_from_slice(&flags.to_le_bytes());
            f[at + 8..at + 16].copy_from_slice(&offset.to_le_bytes());
            f[at + 16..at + 24].copy_from_slice(&vaddr.to_le_bytes());
            f[at + 32..at + 40].copy_from_slice(&filesz.to_le_bytes());
            f[at + 40..at + 48].copy_from_slice(&memsz.to_le_bytes());
        }
        let n = note(VERSION);
        f[NOTE_AT as usize..NOTE_AT as usize + n.len()].copy_from_slice(&n);
        f
    }

    fn good() -> Vec<u8> {
        elf(&phdrs(), 0x40_1000)
    }

    fn check_x86(f: &[u8]) -> Result<Program, ElfError> {
        check(f, EM_X86_64, VERSION)
    }

    /// `good()` with program header `i` changed by `edit`.
    fn with(i: usize, edit: impl Fn(&mut Phdr)) -> Result<Program, ElfError> {
        let mut p = phdrs();
        edit(&mut p[i]);
        check_x86(&elf(&p, 0x40_1000))
    }

    #[test]
    fn a_program_like_t_args_is_accepted() {
        let p = check_x86(&good()).unwrap();
        assert_eq!(p.entry, 0x40_1000);
        let access: Vec<Access> = p.segments.iter().map(|s| s.access).collect();
        assert_eq!(access, [Access::ReadExec, Access::Read, Access::ReadWrite]);
        assert_eq!(
            p.segments[2],
            Segment {
                vaddr: 0x40_3000,
                memsz: 0x2000,
                offset: 0x3000,
                filesz: 0x10,
                access: Access::ReadWrite
            }
        );
        assert_eq!(p.segments[2].end(), 0x40_5000);
    }

    #[test]
    fn the_file_header_is_checked() {
        let edit = |at: usize, v: u8| {
            let mut f = good();
            f[at] = v;
            check_x86(&f)
        };
        assert_eq!(edit(0, 0x7e), Err(ElfError::NotElf));
        assert_eq!(edit(4, 1), Err(ElfError::Not64Bit));
        assert_eq!(edit(5, 2), Err(ElfError::NotLittleEndian));
        assert_eq!(edit(16, 3), Err(ElfError::NotExec(3)), "DYN");
        assert_eq!(edit(18, 0xB7), Err(ElfError::Machine(0xB7)), "aarch64");
        assert_eq!(check(&good(), 0xB7, VERSION), Err(ElfError::Machine(62)));
        assert_eq!(edit(54, 64), Err(ElfError::ProgramHeaders));
        assert_eq!(edit(57, 1), Err(ElfError::ProgramHeaders), "too many");
        assert_eq!(check_x86(&good()[..63]), Err(ElfError::NotElf));
        assert_eq!(check_x86(b""), Err(ElfError::NotElf));
        // OS/ABI and the ident version are not rules (plan 1's ruling).
        assert!(edit(7, 3).is_ok());
        assert!(edit(6, 0).is_ok());
    }

    #[test]
    fn at_most_16_mib() {
        let mut f = good();
        f.resize(MAX_SIZE, 0);
        assert!(check_x86(&f).is_ok());
        f.push(0);
        assert_eq!(check_x86(&f), Err(ElfError::TooBig(MAX_SIZE + 1)));
    }

    #[test]
    fn only_load_note_and_gnu_stack_segments() {
        for kind in [0, 2, 3, 6, 7, 0x6474_e550, 0x6474_e552] {
            assert_eq!(with(4, |p| p.0 = kind), Err(ElfError::SegmentKind(kind)));
        }
        assert_eq!(
            check_x86(&elf(&phdrs()[3..], 0x40_1000)),
            Err(ElfError::NoLoad)
        );
    }

    #[test]
    fn segments_stay_in_the_program_area() {
        assert_eq!(
            with(0, |p| p.3 = 0x3F_F000),
            Err(ElfError::Outside { vaddr: 0x3F_F000 })
        );
        assert_eq!(
            with(2, |p| p.3 = PROGRAM_END - 0x1000),
            Err(ElfError::Outside {
                vaddr: PROGRAM_END - 0x1000
            }),
            "its bss runs past the end"
        );
        assert_eq!(
            with(2, |p| p.5 = u64::MAX),
            Err(ElfError::Outside { vaddr: 0x40_3000 }),
            "overflows"
        );
        // Right at both ends is fine.
        let mut p = phdrs();
        p[0] = (PT_LOAD, 5, 0, PROGRAM_BASE, 0x800, 0x800);
        p[2] = (PT_LOAD, 6, 0x3000, PROGRAM_END - 0x2000, 0x10, 0x2000);
        assert!(check_x86(&elf(&p, PROGRAM_BASE)).is_ok());
    }

    #[test]
    fn each_segment_rule() {
        assert_eq!(
            with(0, |p| p.1 = 7),
            Err(ElfError::WritableAndExecutable { vaddr: 0x40_1000 })
        );
        assert_eq!(
            with(2, |p| p.4 = 0x2001),
            Err(ElfError::MoreFileThanMemory { vaddr: 0x40_3000 })
        );
        assert_eq!(
            with(0, |p| p.2 = 0x1800),
            Err(ElfError::NotCongruent { vaddr: 0x40_1000 })
        );
        assert_eq!(
            with(2, |p| (p.4, p.5) = (0x1000, 0x1000)),
            Err(ElfError::PastTheFile { vaddr: 0x40_3000 })
        );
        assert_eq!(
            with(2, |p| p.2 = u64::MAX - 0xFFF),
            Err(ElfError::PastTheFile { vaddr: 0x40_3000 }),
            "an offset that overflows"
        );
        // On the same page as the one before, even without sharing a byte.
        assert_eq!(
            with(1, |p| (p.2, p.3) = (0x1900, 0x40_1900)),
            Err(ElfError::Overlap { vaddr: 0x40_1900 })
        );
        assert_eq!(
            with(1, |p| p.3 = 0x40_1000),
            Err(ElfError::Overlap { vaddr: 0x40_1000 })
        );
        // Out of order in the file is fine: they are sorted.
        let mut p = phdrs();
        p.swap(0, 2);
        assert_eq!(
            check_x86(&elf(&p, 0x40_1000)).unwrap().segments[0].vaddr,
            0x40_1000
        );
        // Writable-only and execute-only flags.
        assert_eq!(
            with(2, |p| p.1 = 2).unwrap().segments[2].access,
            Access::ReadWrite
        );
        assert_eq!(
            with(0, |p| p.1 = 1).unwrap().segments[0].access,
            Access::ReadExec
        );
    }

    #[test]
    fn the_entry_point_is_in_an_executable_segment() {
        let p = phdrs();
        assert!(check_x86(&elf(&p, 0x40_17FF)).is_ok());
        for entry in [0x40_1800, 0x40_0FFF, 0x40_2000, 0x40_3000, 0] {
            assert_eq!(check_x86(&elf(&p, entry)), Err(ElfError::Entry(entry)));
        }
    }

    #[test]
    fn the_relay_note_names_this_abi() {
        let at = NOTE_AT as usize;
        let edit = |edit: &dyn Fn(&mut Vec<u8>)| {
            let mut f = good();
            edit(&mut f);
            check_x86(&f)
        };
        assert_eq!(
            edit(&|f| f[at + 20] = f[at + 20].wrapping_add(1)),
            Err(ElfError::Abi(VERSION + 1))
        );
        assert_eq!(
            check(&good(), EM_X86_64, VERSION + 1),
            Err(ElfError::Abi(VERSION))
        );
        assert_eq!(edit(&|f| f[at + 12] = b'X'), Err(ElfError::NoNote));
        assert_eq!(edit(&|f| f[at + 8] = 2), Err(ElfError::NoteType(2)));
        assert_eq!(edit(&|f| f[at + 4] = 8), Err(ElfError::NotePastItsSegment));
        assert_eq!(
            edit(&|f| f[at + 3] = 0x80),
            Err(ElfError::NotePastItsSegment)
        );
        // A name without its NUL is the same name.
        assert!(edit(&|f| f[at] = 5).is_ok());
        // Another owner's note first, then ours.
        let mut p = phdrs();
        p[3].2 = NOTE_AT - 0x14;
        p[3].4 = 0x14 + 0x18;
        let mut f = elf(&p, 0x40_1000);
        let other = [4u32.to_le_bytes(), 4u32.to_le_bytes(), 1u32.to_le_bytes()].concat();
        f[at - 0x14..at - 8].copy_from_slice(&other[..12]);
        f[at - 8..at - 4].copy_from_slice(b"GNU\0");
        assert!(check_x86(&f).is_ok());
        // Only a PT_NOTE segment counts, and it must lie in the file.
        assert_eq!(with(3, |p| p.0 = PT_GNU_STACK), Err(ElfError::NoNote));
        assert_eq!(with(3, |p| p.2 = 0x10_0000), Err(ElfError::NoteOutside));
        // A descriptor of another size.
        let mut p = phdrs();
        p[3].4 = 0x1C;
        let mut f = elf(&p, 0x40_1000);
        f[at + 4] = 8;
        assert_eq!(check_x86(&f), Err(ElfError::NoteSize(8)));
    }

    /// A small deterministic generator (xorshift64*).
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
    }

    /// Damages the header, program headers and note 20,000 times: `check`
    /// must never panic, and what it accepts must follow every rule.
    #[test]
    fn random_damage_never_panics_and_what_passes_follows_the_rules() {
        let base = good();
        let mut rng = Rng(0x5eed_1234_abcd_ef01);
        let mut accepted = 0;
        for _ in 0..20_000 {
            let mut f = base.clone();
            for _ in 0..1 + rng.next() % 4 {
                let at = match rng.next() % 3 {
                    0 => rng.next() % 64,
                    1 => 64 + rng.next() % (5 * PHDR_LEN as u64),
                    _ => NOTE_AT + rng.next() % 0x18,
                } as usize;
                f[at] = match rng.next() % 4 {
                    0 => 0,
                    1 => 0xFF,
                    _ => rng.next() as u8,
                };
            }
            if rng.next().is_multiple_of(8) {
                let len = rng.next() as usize % f.len();
                f.truncate(len);
            }
            let Ok(p) = check_x86(&f) else { continue };
            accepted += 1;
            let mut previous_end = 0;
            for s in &p.segments {
                assert!(s.vaddr >= PROGRAM_BASE && s.end() <= PROGRAM_END);
                assert!(s.filesz <= s.memsz);
                assert!(range(&f, s.offset, s.filesz).is_some());
                assert!(s.vaddr & !(PAGE - 1) >= previous_end);
                previous_end = s.end();
            }
            assert!(
                p.segments
                    .iter()
                    .any(|s| s.access == Access::ReadExec && (s.vaddr..s.end()).contains(&p.entry))
            );
        }
        assert!(
            accepted > 1000,
            "only {accepted} accepted: the damage is too coarse"
        );
    }

    #[test]
    fn the_messages_name_what_is_wrong() {
        assert_eq!(ElfError::NotExec(3).to_string(), "type 3, not EXEC");
        assert_eq!(
            ElfError::WritableAndExecutable { vaddr: 0x40_1000 }.to_string(),
            "segment at 0x401000 is writable and executable"
        );
        assert_eq!(
            ElfError::Outside { vaddr: 0x1000 }.to_string(),
            "segment at 0x1000 is outside 0x400000..0x100000000000"
        );
        assert_eq!(ElfError::Abi(2).to_string(), "built for ABI 2");
        assert_eq!(
            ElfError::TooBig(16777217).to_string(),
            "16777217 bytes, more than 16 MiB"
        );
    }
}
