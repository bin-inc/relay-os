//! Host-side build tool for Relay OS. Run `cargo xtask --help`.

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
    /// Regenerate crates/term/src/font.rs from a Spleen BDF file.
    GenFont { bdf: PathBuf },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::GenFont { bdf } => font::gen_font(&bdf)?,
    }
    Ok(())
}
