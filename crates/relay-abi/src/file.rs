//! What the file calls pass (spec §7.3): `open`'s flags, `seek`'s whence,
//! `stat`'s flag, what `stat`, `fstat` and `statfs` fill in, and the
//! records `read_dir` writes.

/// `open`'s flags: read, write, or both.
pub const OPEN_READ: u32 = 1;
pub const OPEN_WRITE: u32 = 2;
/// Create the file if it does not exist.
pub const OPEN_CREATE: u32 = 4;
/// Empty the file (only with [`OPEN_WRITE`]).
pub const OPEN_TRUNCATE: u32 = 8;
/// Every write goes to the end (only with [`OPEN_WRITE`]).
pub const OPEN_APPEND: u32 = 16;
/// With [`OPEN_CREATE`]: `EEXIST` if the file exists.
pub const OPEN_EXCLUSIVE: u32 = 32;
/// `ENOTDIR` unless the path is a directory.
pub const OPEN_DIRECTORY: u32 = 64;
/// Every flag there is.
pub const OPEN_FLAGS: u32 = 127;

/// `seek`'s whence: from the start, from the current offset, from the end.
pub const SEEK_START: u32 = 0;
pub const SEEK_CURRENT: u32 = 1;
pub const SEEK_END: u32 = 2;

/// `stat`'s flag: a symbolic link's own status, not its target's.
pub const STAT_NOFOLLOW: u32 = 1;

/// What kind of file a [`Stat`] or a directory entry is.
pub const KIND_UNKNOWN: u8 = 0;
pub const KIND_REGULAR: u8 = 1;
pub const KIND_DIRECTORY: u8 = 2;
pub const KIND_SYMLINK: u8 = 3;
pub const KIND_CHAR_DEVICE: u8 = 4;
pub const KIND_BLOCK_DEVICE: u8 = 5;
pub const KIND_FIFO: u8 = 6;
pub const KIND_SOCKET: u8 = 7;

/// `stat`'s and `fstat`'s answer, `#[repr(C)]` with no padding. Times are
/// seconds since 1970, UTC.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stat {
    pub ino: u64,
    pub size: u64,
    /// Space used, in 512-byte units.
    pub blocks: u64,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
    /// One of the `KIND_*` numbers.
    pub kind: u32,
    /// The permission bits, with set-user-ID, set-group-ID and sticky.
    pub perm: u32,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub block_size: u32,
}

impl Stat {
    pub const SIZE: usize = core::mem::size_of::<Stat>();

    /// Its bytes, as a program's memory holds the struct.
    pub fn to_bytes(&self) -> [u8; Stat::SIZE] {
        let mut b = [0u8; Stat::SIZE];
        let longs = [
            self.ino,
            self.size,
            self.blocks,
            self.atime,
            self.mtime,
            self.ctime,
        ];
        for (i, v) in longs.iter().enumerate() {
            b[8 * i..8 * i + 8].copy_from_slice(&v.to_ne_bytes());
        }
        let words = [
            self.kind,
            self.perm,
            self.nlink,
            self.uid,
            self.gid,
            self.block_size,
        ];
        for (i, v) in words.iter().enumerate() {
            b[48 + 4 * i..52 + 4 * i].copy_from_slice(&v.to_ne_bytes());
        }
        b
    }
}

/// `statfs`'s answer, `#[repr(C)]` with no padding. Counts are in
/// `block_size` units.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatFs {
    pub block_size: u64,
    pub blocks: u64,
    pub free_blocks: u64,
    /// Free blocks an unprivileged user could use.
    pub avail_blocks: u64,
    pub files: u64,
    pub free_files: u64,
}

impl StatFs {
    pub const SIZE: usize = core::mem::size_of::<StatFs>();

    pub fn to_bytes(&self) -> [u8; StatFs::SIZE] {
        let mut b = [0u8; StatFs::SIZE];
        let longs = [
            self.block_size,
            self.blocks,
            self.free_blocks,
            self.avail_blocks,
            self.files,
            self.free_files,
        ];
        for (i, v) in longs.iter().enumerate() {
            b[8 * i..8 * i + 8].copy_from_slice(&v.to_ne_bytes());
        }
        b
    }
}

/// The header of each record `read_dir` writes, `#[repr(C)]` with no
/// padding. The name follows it, and the record is padded with zeros to
/// a multiple of 8 bytes, which `len` includes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirEntry {
    pub ino: u64,
    /// The whole record's length.
    pub len: u16,
    pub name_len: u16,
    /// One of the `KIND_*` numbers.
    pub kind: u8,
    pub reserved: [u8; 3],
}

impl DirEntry {
    pub const SIZE: usize = core::mem::size_of::<DirEntry>();

    /// The length of the record for a name of `name_len` bytes.
    pub const fn record_len(name_len: usize) -> usize {
        (DirEntry::SIZE + name_len).next_multiple_of(8)
    }
}

/// Writes the record for `name` at the start of `buf`: how many bytes it
/// took, or `None` (and nothing written) if it does not fit or the name is
/// too long for a record.
pub fn put_dir_entry(buf: &mut [u8], ino: u64, kind: u8, name: &[u8]) -> Option<usize> {
    let len = DirEntry::record_len(name.len());
    let (Ok(len16), Ok(name_len)) = (u16::try_from(len), u16::try_from(name.len())) else {
        return None;
    };
    let rec = buf.get_mut(..len)?;
    rec.fill(0);
    rec[..8].copy_from_slice(&ino.to_ne_bytes());
    rec[8..10].copy_from_slice(&len16.to_ne_bytes());
    rec[10..12].copy_from_slice(&name_len.to_ne_bytes());
    rec[12] = kind;
    rec[DirEntry::SIZE..DirEntry::SIZE + name.len()].copy_from_slice(name);
    Some(len)
}

/// One record `read_dir` wrote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirRecord<'a> {
    pub ino: u64,
    pub kind: u8,
    pub name: &'a [u8],
}

/// The records in the bytes `read_dir` returned. A record that does not
/// hold together ends them.
pub fn dir_entries(buf: &[u8]) -> impl Iterator<Item = DirRecord<'_>> {
    let mut rest = buf;
    core::iter::from_fn(move || {
        let head = rest.get(..DirEntry::SIZE)?;
        let ino = u64::from_ne_bytes(head[..8].try_into().ok()?);
        let len = usize::from(u16::from_ne_bytes([head[8], head[9]]));
        let name_len = usize::from(u16::from_ne_bytes([head[10], head[11]]));
        if len < DirEntry::record_len(name_len) || len > rest.len() {
            return None;
        }
        let rec = DirRecord {
            ino,
            kind: head[12],
            name: &rest[DirEntry::SIZE..DirEntry::SIZE + name_len],
        };
        rest = &rest[len..];
        Some(rec)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    #[test]
    fn the_layouts_are_fixed() {
        assert_eq!(Stat::SIZE, 72);
        assert_eq!(offset_of!(Stat, ino), 0);
        assert_eq!(offset_of!(Stat, size), 8);
        assert_eq!(offset_of!(Stat, blocks), 16);
        assert_eq!(offset_of!(Stat, atime), 24);
        assert_eq!(offset_of!(Stat, mtime), 32);
        assert_eq!(offset_of!(Stat, ctime), 40);
        assert_eq!(offset_of!(Stat, kind), 48);
        assert_eq!(offset_of!(Stat, perm), 52);
        assert_eq!(offset_of!(Stat, nlink), 56);
        assert_eq!(offset_of!(Stat, uid), 60);
        assert_eq!(offset_of!(Stat, gid), 64);
        assert_eq!(offset_of!(Stat, block_size), 68);
        assert_eq!(StatFs::SIZE, 48);
        assert_eq!(offset_of!(StatFs, block_size), 0);
        assert_eq!(offset_of!(StatFs, blocks), 8);
        assert_eq!(offset_of!(StatFs, free_blocks), 16);
        assert_eq!(offset_of!(StatFs, avail_blocks), 24);
        assert_eq!(offset_of!(StatFs, files), 32);
        assert_eq!(offset_of!(StatFs, free_files), 40);
        assert_eq!(size_of::<DirEntry>(), 16);
        assert_eq!(offset_of!(DirEntry, ino), 0);
        assert_eq!(offset_of!(DirEntry, len), 8);
        assert_eq!(offset_of!(DirEntry, name_len), 10);
        assert_eq!(offset_of!(DirEntry, kind), 12);
        assert_eq!(offset_of!(DirEntry, reserved), 13);
    }

    #[test]
    fn the_numbers_are_fixed() {
        let flags = [
            OPEN_READ,
            OPEN_WRITE,
            OPEN_CREATE,
            OPEN_TRUNCATE,
            OPEN_APPEND,
            OPEN_EXCLUSIVE,
            OPEN_DIRECTORY,
        ];
        assert_eq!(flags, [1, 2, 4, 8, 16, 32, 64]);
        assert_eq!(flags.iter().fold(0, |a, f| a | f), OPEN_FLAGS);
        assert_eq!((SEEK_START, SEEK_CURRENT, SEEK_END), (0, 1, 2));
        assert_eq!(STAT_NOFOLLOW, 1);
        assert_eq!(
            [
                KIND_UNKNOWN,
                KIND_REGULAR,
                KIND_DIRECTORY,
                KIND_SYMLINK,
                KIND_CHAR_DEVICE,
                KIND_BLOCK_DEVICE,
                KIND_FIFO,
                KIND_SOCKET
            ],
            [0, 1, 2, 3, 4, 5, 6, 7]
        );
    }

    #[test]
    fn a_struct_s_bytes_are_its_memory() {
        let s = Stat {
            ino: 1,
            size: 2,
            blocks: 3,
            atime: 4,
            mtime: 5,
            ctime: 6,
            kind: 7,
            perm: 8,
            nlink: 9,
            uid: 10,
            gid: 11,
            block_size: 12,
        };
        // SAFETY: both are `repr(C)` of integers with no padding.
        let mem: [u8; Stat::SIZE] = unsafe { core::mem::transmute(s) };
        assert_eq!(s.to_bytes(), mem);
        let f = StatFs {
            block_size: 1,
            blocks: 2,
            free_blocks: 3,
            avail_blocks: 4,
            files: 5,
            free_files: 6,
        };
        let mem: [u8; StatFs::SIZE] = unsafe { core::mem::transmute(f) };
        assert_eq!(f.to_bytes(), mem);
    }

    #[test]
    fn records_are_padded_to_8_bytes_and_read_back() {
        let mut buf = [0xAAu8; 64];
        assert_eq!(put_dir_entry(&mut buf, 7, KIND_REGULAR, b"a"), Some(24));
        assert_eq!(buf[17..24], [0; 7], "padded with zeros");
        let n = put_dir_entry(&mut buf[24..], 1 << 40, KIND_DIRECTORY, b"12345678");
        assert_eq!(n, Some(24), "16 + 8 needs no padding");
        let got: Vec<_> = dir_entries(&buf[..48]).collect();
        assert_eq!(
            got,
            [
                DirRecord {
                    ino: 7,
                    kind: KIND_REGULAR,
                    name: b"a"
                },
                DirRecord {
                    ino: 1 << 40,
                    kind: KIND_DIRECTORY,
                    name: b"12345678"
                }
            ]
        );
        // The header as the struct: the kernel and the programs agree.
        let head = DirEntry {
            ino: 7,
            len: 24,
            name_len: 1,
            kind: KIND_REGULAR,
            reserved: [0; 3],
        };
        let mem: [u8; 16] = unsafe { core::mem::transmute(head) };
        assert_eq!(buf[..16], mem);
    }

    #[test]
    fn a_record_that_does_not_fit_is_not_written() {
        let mut buf = [0xAAu8; 23];
        assert_eq!(put_dir_entry(&mut buf, 7, KIND_REGULAR, b"a"), None);
        assert_eq!(buf, [0xAA; 23], "nothing written");
        assert_eq!(put_dir_entry(&mut [0; 16], 1, 0, b""), Some(16));
        let long = vec![b'x'; 70_000];
        assert_eq!(put_dir_entry(&mut vec![0; 80_000], 1, 0, &long), None);
    }

    #[test]
    fn records_that_do_not_hold_together_end_the_list() {
        let mut buf = [0u8; 48];
        put_dir_entry(&mut buf, 1, KIND_REGULAR, b"one").unwrap();
        put_dir_entry(&mut buf[24..], 2, KIND_REGULAR, b"two").unwrap();
        // The second record's length says more than there is.
        buf[32..34].copy_from_slice(&200u16.to_ne_bytes());
        assert_eq!(dir_entries(&buf).count(), 1);
        // A length shorter than its name.
        buf[8..10].copy_from_slice(&8u16.to_ne_bytes());
        assert_eq!(dir_entries(&buf).count(), 0);
        assert_eq!(dir_entries(&buf[..10]).count(), 0, "half a header");
        assert_eq!(dir_entries(&[]).count(), 0);
    }
}
