use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeId(pub(crate) u32);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Name(Vec<u8>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NameError {
    Empty,
    TooLong,
    InvalidByte,
    Allocation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeKind {
    Regular,
    Directory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Metadata {
    pub kind: NodeKind,
    pub len: u64,
    pub mode: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirEntry {
    pub name: Name,
    pub node: NodeId,
    pub kind: NodeKind,
}

impl Name {
    pub fn new(bytes: &[u8]) -> Result<Self, NameError> {
        if bytes.is_empty() {
            return Err(NameError::Empty);
        }
        if bytes.len() > 255 {
            return Err(NameError::TooLong);
        }
        if bytes == b"."
            || bytes == b".."
            || bytes
                .iter()
                .any(|&byte| !(b' '..=b'~').contains(&byte) || byte == b'/')
        {
            return Err(NameError::InvalidByte);
        }
        let mut name = Vec::new();
        name.try_reserve_exact(bytes.len())
            .map_err(|_| NameError::Allocation)?;
        name.extend_from_slice(bytes);
        Ok(Self(name))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}
