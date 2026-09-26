//! Small helpers shared by the xtask subcommands.

use std::path::{Path, PathBuf};

/// Repository root (the parent of the `xtask` directory).
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}
