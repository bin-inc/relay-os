//! Small helpers shared by the xtask subcommands.

use anyhow::{Context, Result, bail};
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

/// `target/relay` — where xtask puts everything it produces.
pub fn out_dir() -> PathBuf {
    root().join("target").join("relay")
}

/// Runs a command, failing with its stderr if it exits non-zero.
pub fn run(cmd: &mut Command) -> Result<()> {
    let out = cmd
        .output()
        .with_context(|| format!("failed to start {cmd:?}"))?;
    if !out.status.success() {
        bail!(
            "{cmd:?} failed ({}):\n{}{}",
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

/// Runs a command and returns its stdout as a string.
pub fn run_stdout(cmd: &mut Command) -> Result<String> {
    let out = cmd
        .output()
        .with_context(|| format!("failed to start {cmd:?}"))?;
    if !out.status.success() {
        bail!(
            "{cmd:?} failed ({}):\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8(out.stdout)?)
}

/// A `Command` for an mtools program with the config check disabled.
pub fn mtools(program: &str) -> Command {
    let mut c = Command::new(program);
    c.env("MTOOLS_SKIP_CHECK", "1");
    c
}
