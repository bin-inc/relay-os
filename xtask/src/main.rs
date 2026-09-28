//! Host-side build tool for Relay OS. Run `cargo xtask --help`.

mod build;
mod ci;
mod config;
mod e2e;
mod flash;
mod font;
mod host_shell;
mod image;
mod keys;
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
    /// Run the shell on this machine over an image's ext2 root partition.
    HostShell {
        /// Image with a GPT and an ext2 root partition, e.g.
        /// target/relay/relay-os.img (changed in place).
        img: PathBuf,
    },
    /// Write to the Kingston test stick.
    Flash {
        /// Replace loader and kernel only; keep files on /.
        #[arg(long, conflicts_with = "full", required_unless_present = "full")]
        kernel: bool,
        /// Repartition and reformat the whole stick.
        #[arg(long)]
        full: bool,
        /// Skip the typed confirmation for --full.
        #[arg(long)]
        yes: bool,
        /// Kernel command line to put on the stick (e.g. `video=1280x720`).
        #[arg(long, default_value = "")]
        cmdline: String,
    },
    /// e2fsck the stick's root filesystem and list its files.
    VerifyUsb,
    /// Write a udev rule giving you access to the test stick.
    SetupUdev,
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
        Cmd::HostShell { img } => host_shell::run(&img)?,
        Cmd::Flash {
            kernel,
            full,
            yes,
            cmdline,
        } => {
            let a = build::build()?;
            if full {
                flash::flash_full(&a, &cmdline, yes)?;
            } else if kernel {
                flash::flash_kernel(&a, &cmdline)?;
            }
        }
        Cmd::VerifyUsb => flash::verify_usb()?,
        Cmd::SetupUdev => flash::setup_udev()?,
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
