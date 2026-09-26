//! Directory blocks (spec §8.2): the entries of one block, parsed and
//! validated in full on every read. An entry is inode (u32), rec_len
//! (u16), name_len (u8), file type (u8), then the name; records tile the
//! block exactly. Without the `filetype` feature, name_len is 16 bits and
//! there is no type byte.

use crate::le::{set_u16, set_u32, u16_at, u32_at};
use alloc::vec::Vec;
use vfs::FileType;

/// Bytes before an entry's name.
pub const HEADER: usize = 8;

/// One entry of a directory block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Where it starts in the block.
    pub offset: usize,
    /// 0 for an unused entry.
    pub inode: u32,
    pub rec_len: usize,
    pub name_len: usize,
}

impl Entry {
    pub fn name<'a>(&self, block: &'a [u8]) -> &'a [u8] {
        &block[self.offset + HEADER..][..self.name_len]
    }
}

/// The smallest record holding a name of `name_len` bytes.
pub fn rec_len_for(name_len: usize) -> usize {
    (HEADER + name_len + 3) & !3
}

/// The file type byte of an entry for an inode of type `kind` (with the
/// `filetype` feature).
pub fn type_byte(kind: FileType) -> u8 {
    match kind {
        FileType::Regular => 1,
        FileType::Directory => 2,
        FileType::CharDev => 3,
        FileType::BlockDev => 4,
        FileType::Fifo => 5,
        FileType::Socket => 6,
        FileType::Symlink => 7,
    }
}

/// Writes an entry at `offset`. Without `filetype` the type byte is the
/// high half of the name length, so it is not written.
fn put(
    block: &mut [u8],
    offset: usize,
    rec_len: usize,
    name: &[u8],
    inode: u32,
    file_type: Option<u8>,
) {
    set_u32(block, offset, inode);
    set_u16(block, offset + 4, rec_len as u16);
    match file_type {
        Some(t) => {
            block[offset + 6] = name.len() as u8;
            block[offset + 7] = t;
        }
        None => set_u16(block, offset + 6, name.len() as u16),
    }
    block[offset + HEADER..][..name.len()].copy_from_slice(name);
}

/// Sets the record length of the entry at `offset`.
fn set_rec_len(block: &mut [u8], offset: usize, rec_len: usize) {
    set_u16(block, offset + 4, rec_len as u16);
}

/// An entry with room for a new name of `name_len` bytes: an unused one
/// large enough, or a used one whose record has that much to spare after
/// its own name.
pub fn room(entries: &[Entry], name_len: usize) -> Option<Entry> {
    let need = rec_len_for(name_len);
    entries.iter().copied().find(|e| {
        if e.inode == 0 {
            e.rec_len >= need
        } else {
            e.rec_len >= rec_len_for(e.name_len) + need
        }
    })
}

/// Adds `name` → `inode` in the room [`room`] found at `at`: reusing an
/// unused entry, or splitting the spare space off a used one.
/// `file_type` is `None` without the `filetype` feature.
pub fn insert(block: &mut [u8], at: Entry, name: &[u8], inode: u32, file_type: Option<u8>) {
    if at.inode == 0 {
        put(block, at.offset, at.rec_len, name, inode, file_type);
    } else {
        let keep = rec_len_for(at.name_len);
        set_rec_len(block, at.offset, keep);
        put(
            block,
            at.offset + keep,
            at.rec_len - keep,
            name,
            inode,
            file_type,
        );
    }
}

/// Removes `entry` from `block`: its record joins the previous entry's in
/// the block, or, first in the block, it just becomes unused.
pub fn remove(block: &mut [u8], entry: Entry, prev: Option<Entry>) {
    match prev {
        Some(prev) => set_rec_len(block, prev.offset, prev.rec_len + entry.rec_len),
        None => set_u32(block, entry.offset, 0),
    }
}

/// Points `entry` at another inode, of type `file_type` (`None` without
/// the `filetype` feature).
pub fn retarget(block: &mut [u8], entry: Entry, inode: u32, file_type: Option<u8>) {
    set_u32(block, entry.offset, inode);
    if let Some(t) = file_type {
        block[entry.offset + 7] = t;
    }
}

/// Makes `block` an empty directory block: one unused entry covering it.
pub fn init_empty(block: &mut [u8]) {
    block.fill(0);
    set_rec_len(block, 0, block.len());
}

/// Makes `block` a new directory's first block: `.` and `..`.
pub fn init_dir(block: &mut [u8], inode: u32, parent: u32, filetype: bool) {
    let t = filetype.then(|| type_byte(FileType::Directory));
    block.fill(0);
    put(block, 0, 12, b".", inode, t);
    put(block, 12, block.len() - 12, b"..", parent, t);
}

/// Why a directory block does not parse: what is wrong, and where.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Corrupt {
    pub offset: usize,
    pub what: &'static str,
}

/// Parses every entry of `block`. Used entries must name inodes up to
/// `max_inode`.
pub fn parse(block: &[u8], filetype: bool, max_inode: u32) -> Result<Vec<Entry>, Corrupt> {
    let mut entries = Vec::new();
    let mut offset = 0;
    while offset < block.len() {
        let bad = |what| Corrupt { offset, what };
        if block.len() - offset < HEADER {
            return Err(bad("entry header past the end of the block"));
        }
        let inode = u32_at(block, offset);
        let rec_len = u16_at(block, offset + 4) as usize;
        let name_len = if filetype {
            block[offset + 6] as usize
        } else {
            u16_at(block, offset + 6) as usize
        };
        if !rec_len.is_multiple_of(4) || rec_len < HEADER {
            return Err(bad("bad rec_len"));
        }
        if rec_len > block.len() - offset {
            return Err(bad("rec_len past the end of the block"));
        }
        if name_len > 255 || rec_len_for(name_len) > rec_len {
            return Err(bad("name longer than its record"));
        }
        if inode > max_inode {
            return Err(bad("inode number out of range"));
        }
        entries.push(Entry {
            offset,
            inode,
            rec_len,
            name_len,
        });
        offset += rec_len;
    }
    Ok(entries)
}

/// Checks the names of the used entries of a block: none is empty or holds
/// `/` or NUL, and `.` and `..` are only the first two entries of a
/// directory's first block. Anything else would let a caller that joins
/// names into paths wander out of the directory, or loop.
pub fn check_names(block: &[u8], entries: &[Entry], first_block: bool) -> Result<(), Corrupt> {
    for (i, e) in entries.iter().enumerate().filter(|(_, e)| e.inode != 0) {
        let name = e.name(block);
        let bad = |what| {
            Err(Corrupt {
                offset: e.offset,
                what,
            })
        };
        if name.is_empty() {
            return bad("empty name");
        }
        if name.contains(&b'/') || name.contains(&0) {
            return bad("name with / or NUL");
        }
        let place = match name {
            b"." => Some(0),
            b".." => Some(1),
            _ => None,
        };
        if place.is_some_and(|p| !first_block || p != i) {
            return bad("misplaced . or ..");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Packs `(inode, rec_len, name)` records (type byte 1, a file).
    fn block(entries: &[(u32, usize, &[u8])], filetype: bool) -> Vec<u8> {
        let mut b = Vec::new();
        for &(inode, rec_len, name) in entries {
            let start = b.len();
            b.extend_from_slice(&inode.to_le_bytes());
            b.extend_from_slice(&(rec_len as u16).to_le_bytes());
            if filetype {
                b.extend_from_slice(&[name.len() as u8, 1]);
            } else {
                b.extend_from_slice(&(name.len() as u16).to_le_bytes());
            }
            b.extend_from_slice(name);
            b.resize(start + rec_len, 0);
        }
        b
    }

    #[test]
    fn records_round_names_up_to_four_bytes() {
        assert_eq!(rec_len_for(1), 12);
        assert_eq!(rec_len_for(4), 12);
        assert_eq!(rec_len_for(5), 16);
        assert_eq!(rec_len_for(255), 264);
    }

    #[test]
    fn a_block_parses_into_its_entries() {
        for filetype in [true, false] {
            let b = block(
                &[
                    (2, 12, b"."),
                    (2, 12, b".."),
                    (0, 20, b"gone"),
                    (12, 20, b"file"),
                ],
                filetype,
            );
            let e = parse(&b, filetype, 100).unwrap();
            assert_eq!(e.len(), 4);
            assert_eq!((e[2].offset, e[2].inode, e[2].rec_len), (24, 0, 20));
            assert_eq!(e[3].name(&b), b"file");
            assert_eq!(e[1].name(&b), b"..");
        }
    }

    #[test]
    fn corrupt_blocks_are_refused_not_panicked_on() {
        let good = block(&[(2, 12, b"."), (2, 52, b"..")], true);
        let bad = |patch: &dyn Fn(&mut Vec<u8>)| {
            let mut b = good.clone();
            patch(&mut b);
            parse(&b, true, 100).unwrap_err()
        };
        assert_eq!(bad(&|b| b[4] = 0).what, "bad rec_len");
        assert_eq!(bad(&|b| b[4] = 13).what, "bad rec_len");
        assert_eq!(
            bad(&|b| b[16] = 56).what,
            "rec_len past the end of the block"
        );
        assert_eq!(bad(&|b| b[6] = 5).what, "name longer than its record");
        assert_eq!(bad(&|b| b[12] = 101).offset, 12);
        assert_eq!(
            bad(&|b| b.truncate(60)).what,
            "rec_len past the end of the block"
        );
        let tail = block(&[(2, 60, b".")], true);
        let mut short = tail.clone();
        short.extend_from_slice(&[0; 4]);
        assert_eq!(
            parse(&short, true, 100).unwrap_err().what,
            "entry header past the end of the block"
        );
        // Without `filetype` the name length is 16 bits wide.
        let mut wide = block(&[(2, 64, b"x")], false);
        wide[7] = 1;
        assert_eq!(
            parse(&wide, false, 100).unwrap_err().what,
            "name longer than its record"
        );
    }

    #[test]
    fn names_are_checked() {
        let names = |entries: &[(u32, usize, &[u8])], first| {
            let b = block(entries, true);
            let e = parse(&b, true, 100).unwrap();
            check_names(&b, &e, first).map_err(|c| (c.offset, c.what))
        };
        let dir: [(u32, usize, &[u8]); 3] = [(2, 12, b"."), (2, 12, b".."), (12, 40, b"f")];
        assert_eq!(names(&dir, true), Ok(()));
        assert_eq!(names(&dir, false), Err((0, "misplaced . or ..")));
        assert_eq!(
            names(&[(12, 12, b"a/b")], false),
            Err((0, "name with / or NUL"))
        );
        assert_eq!(
            names(&[(12, 12, b"a\x00")], false),
            Err((0, "name with / or NUL"))
        );
        assert_eq!(names(&[(12, 12, b"")], false), Err((0, "empty name")));
        assert_eq!(
            names(&[(0, 12, b"")], false),
            Ok(()),
            "unused entries may be anything"
        );
        let swapped: [(u32, usize, &[u8]); 2] = [(2, 12, b".."), (2, 52, b".")];
        assert_eq!(names(&swapped, true), Err((0, "misplaced . or ..")));
        let late: [(u32, usize, &[u8]); 3] = [(2, 12, b"."), (2, 12, b".."), (5, 40, b"..")];
        assert_eq!(names(&late, true), Err((24, "misplaced . or ..")));
    }

    #[test]
    fn new_names_split_spare_space_or_reuse_unused_entries() {
        for filetype in [true, false] {
            let t = filetype.then_some(1);
            let mut b = vec![0u8; 64];
            init_dir(&mut b, 12, 2, filetype);
            let e = parse(&b, filetype, 100).unwrap();
            assert_eq!((e[0].rec_len, e[1].rec_len, e[1].inode), (12, 52, 2));
            // `..` needs 12 of its 52 bytes: a 40-byte record splits off.
            let at = room(&e, 20).unwrap();
            insert(&mut b, at, b"twenty-characters-xx", 13, t);
            let e = parse(&b, filetype, 100).unwrap();
            assert_eq!(e.len(), 3);
            assert_eq!((e[1].rec_len, e[2].offset, e[2].rec_len), (12, 24, 40));
            assert_eq!(e[2].name(&b), b"twenty-characters-xx");
            assert_eq!(room(&e, 20), None, "full");
            assert_eq!(room(&e, 4).map(|e| e.offset), Some(24), "12 spare bytes");
            if filetype {
                assert_eq!(b[24 + 7], 1);
            }
        }
        let mut b = vec![0u8; 32];
        init_empty(&mut b);
        let e = parse(&b, true, 100).unwrap();
        assert_eq!((e.len(), e[0].inode, e[0].rec_len), (1, 0, 32));
        insert(&mut b, room(&e, 3).unwrap(), b"new", 7, Some(2));
        let e = parse(&b, true, 100).unwrap();
        assert_eq!(
            (e[0].inode, e[0].rec_len, e[0].name(&b)),
            (7, 32, &b"new"[..])
        );
        assert_eq!(room(&e, 12), Some(e[0]), "the rest of the reused entry");
    }

    #[test]
    fn file_type_bytes_are_ext2s() {
        assert_eq!(type_byte(FileType::Regular), 1);
        assert_eq!(type_byte(FileType::Directory), 2);
        assert_eq!(type_byte(FileType::Symlink), 7);
    }

    #[test]
    fn removed_entries_merge_into_the_previous_one() {
        let mut b = block(&[(12, 12, b"a"), (13, 12, b"b"), (14, 40, b"c")], true);
        let e = parse(&b, true, 100).unwrap();
        remove(&mut b, e[1], Some(e[0]));
        let e = parse(&b, true, 100).unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!((e[0].rec_len, e[1].name(&b)), (24, &b"c"[..]));
        // The first entry of a block has nothing to merge into.
        remove(&mut b, e[0], None);
        let e = parse(&b, true, 100).unwrap();
        assert_eq!((e[0].inode, e[0].rec_len), (0, 24));
        assert_eq!(room(&e, 12).map(|e| e.offset), Some(0), "reusable");
    }

    #[test]
    fn an_entry_can_point_elsewhere() {
        let mut b = block(&[(12, 12, b"a"), (13, 52, b"b")], true);
        let e = parse(&b, true, 100).unwrap();
        retarget(&mut b, e[1], 40, Some(7));
        let e = parse(&b, true, 100).unwrap();
        assert_eq!((e[1].inode, e[1].name(&b), b[12 + 7]), (40, &b"b"[..], 7));
        let mut b = block(&[(12, 64, b"a")], false);
        retarget(&mut b, e[0], 41, None);
        assert_eq!(parse(&b, false, 100).unwrap()[0].inode, 41);
        assert_eq!(b[7], 0, "no type byte to write");
    }
}
