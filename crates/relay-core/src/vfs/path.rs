use crate::fs::{Name, NodeId};
use alloc::vec::Vec;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cwd {
    pub(super) components: Vec<Name>,
}

impl Cwd {
    pub(super) fn root() -> Self {
        Self {
            components: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedPath {
    pub(super) node: NodeId,
    pub(super) components: Vec<Name>,
}
