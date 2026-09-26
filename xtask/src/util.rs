//! Small helpers shared by the xtask subcommands.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Repository root (the parent of the `xtask` directory).
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// A `cargo` command (the same cargo that runs xtask) in the repository root.
pub fn cargo() -> Command {
    let mut c = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    c.current_dir(root());
    c
}
