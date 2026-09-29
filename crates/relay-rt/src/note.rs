//! The ELF note that names the ABI a program was built for (spec §5.2):
//! owner `Relay`, type 1, a 4-byte descriptor holding
//! `relay_abi::VERSION`. The user linker script keeps it in a `PT_NOTE`.

use relay_abi::{NOTE_NAME, NOTE_TYPE, VERSION};

/// An ELF note with a name of up to 7 bytes (8 with its NUL, a multiple
/// of 4) and a 4-byte descriptor.
#[repr(C, align(4))]
pub struct Note {
    namesz: u32,
    descsz: u32,
    kind: u32,
    name: [u8; 8],
    desc: u32,
}

pub const fn abi_note() -> Note {
    let mut name = [0; 8];
    let mut i = 0;
    while i < NOTE_NAME.len() {
        name[i] = NOTE_NAME[i];
        i += 1;
    }
    Note {
        namesz: NOTE_NAME.len() as u32 + 1,
        descsz: 4,
        kind: NOTE_TYPE,
        name,
        desc: VERSION,
    }
}

#[cfg(target_os = "none")]
#[used]
#[unsafe(link_section = ".note.relay")]
static ABI_NOTE: Note = abi_note();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_note_is_laid_out_as_elf_says() {
        let n = abi_note();
        assert_eq!(core::mem::size_of::<Note>(), 24);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&n.namesz.to_le_bytes());
        bytes.extend_from_slice(&n.descsz.to_le_bytes());
        bytes.extend_from_slice(&n.kind.to_le_bytes());
        bytes.extend_from_slice(&n.name);
        bytes.extend_from_slice(&n.desc.to_le_bytes());
        let mut want = vec![6, 0, 0, 0, 4, 0, 0, 0, 1, 0, 0, 0];
        want.extend_from_slice(b"Relay\0\0\0");
        want.extend_from_slice(&VERSION.to_le_bytes());
        assert_eq!(bytes, want);
    }
}
