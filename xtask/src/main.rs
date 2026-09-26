//! Host-side build tool for Relay OS. Run `cargo xtask --help`.

mod ci;
mod config;
mod font;
mod util;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Relay OS build, image, test and flash tool")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Formatting check and clippy (warnings are errors) on every crate.
    Lint,
    /// Host unit tests.
    Unit,
    /// Everything a pull request must pass: lint, unit tests, QEMU scenarios.
    Ci,
    /// Regenerate crates/term/src/font.rs from a Spleen BDF file.
    GenFont { bdf: PathBuf },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Lint => ci::lint()?,
        Cmd::Unit => ci::unit_tests()?,
        Cmd::Ci => {
            ci::lint()?;
            ci::unit_tests()?;
        }
        Cmd::GenFont { bdf } => font::gen_font(&bdf)?,
    }
    Ok(())
}
