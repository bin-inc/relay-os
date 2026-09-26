//! Host-side build tool for Relay OS. Run `cargo xtask --help`.

mod build;
mod ci;
mod config;
mod e2e;
mod font;
mod image;
mod qemu;
mod qmp;
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
    /// Build BOOTX64.EFI and kernel.elf.
    Build,
    /// Build target/relay/relay-os.img.
    Image {
        /// Kernel command line written to the ESP.
        #[arg(long, default_value = "")]
        cmdline: String,
    },
    /// Boot the image in QEMU with a window (serial on this terminal).
    Qemu {
        /// No window; serial only.
        #[arg(long)]
        serial_only: bool,
        #[arg(long, default_value = "")]
        cmdline: String,
    },
    /// Host unit tests, then every QEMU end-to-end scenario.
    Test {
        /// Run only this scenario (file stem in tests/e2e).
        #[arg(long)]
        scenario: Option<String>,
        /// Skip the host unit tests.
        #[arg(long)]
        e2e_only: bool,
    },
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
        Cmd::Build => {
            let a = build::build()?;
            println!("{}\n{}", a.bootx64.display(), a.kernel.display());
        }
        Cmd::Image { cmdline } => {
            let a = build::build()?;
            println!("{}", image::build_image(&a, &cmdline)?.display());
        }
        Cmd::Qemu {
            serial_only,
            cmdline,
        } => {
            let a = build::build()?;
            let img = image::build_image(&a, &cmdline)?;
            qemu::run_interactive(&img, &util::out_dir().join("qemu"), serial_only)?;
        }
        Cmd::Test { scenario, e2e_only } => {
            if !e2e_only {
                ci::unit_tests()?;
            }
            e2e::run_all(scenario.as_deref())?;
        }
        Cmd::Lint => ci::lint()?,
        Cmd::Unit => ci::unit_tests()?,
        Cmd::Ci => {
            ci::lint()?;
            ci::unit_tests()?;
            e2e::run_all(None)?;
        }
        Cmd::GenFont { bdf } => font::gen_font(&bdf)?,
    }
    Ok(())
}
