//! `cargo xtask lint | unit | ci`: the checks every pull request must pass.
//!
//! `.github/workflows/ci.yml` only installs tools and calls these
//! subcommands, so CI and a local run check exactly the same things. New
//! crates and scenarios are picked up without touching the workflow.

use crate::config::{KERNEL_TARGET, PROFILE, UEFI_TARGET};
use crate::util::{cargo, root};
use anyhow::{Result, bail};

/// Which bare-metal packages exist (they appear part-way through plan 1).
#[derive(Clone, Copy, Debug)]
pub struct Packages {
    pub boot: bool,
    pub kernel: bool,
}

impl Packages {
    pub fn detect() -> Packages {
        let has = |dir: &str| root().join(dir).join("Cargo.toml").exists();
        Packages {
            boot: has("boot"),
            kernel: has("kernel"),
        }
    }
}

fn cmd(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

/// `cargo` argument lists that make up `lint`. Warnings are errors.
pub fn lint_commands(p: Packages) -> Vec<Vec<String>> {
    let mut v = vec![
        cmd(&["fmt", "--all", "--check"]),
        // Default members: every host crate.
        cmd(&["clippy", "--all-targets", "--", "-D", "warnings"]),
    ];
    for (present, package, target) in [
        (p.boot, "relay-boot", UEFI_TARGET),
        (p.kernel, "relay-kernel", KERNEL_TARGET),
    ] {
        if present {
            v.push(cmd(&[
                "clippy",
                "--package",
                package,
                "--lib",
                "--tests",
                "--",
                "-D",
                "warnings",
            ]));
            v.push(cmd(&[
                "clippy",
                "--profile",
                PROFILE,
                "--package",
                package,
                "--target",
                target,
                "--",
                "-D",
                "warnings",
            ]));
        }
    }
    v
}

/// `cargo` argument lists for the host unit tests: the default members plus
/// the libraries of the loader and the kernel (their binaries only build
/// for bare metal).
pub fn unit_commands(p: Packages) -> Vec<Vec<String>> {
    let mut v = vec![cmd(&["test"])];
    for (present, package) in [(p.boot, "relay-boot"), (p.kernel, "relay-kernel")] {
        if present {
            v.push(cmd(&["test", "--package", package, "--lib"]));
        }
    }
    v
}

fn run_all(commands: Vec<Vec<String>>) -> Result<()> {
    for args in commands {
        println!("== cargo {}", args.join(" "));
        if !cargo().args(&args).status()?.success() {
            bail!("`cargo {}` failed", args.join(" "));
        }
    }
    Ok(())
}

pub fn lint() -> Result<()> {
    run_all(lint_commands(Packages::detect()))
}

pub fn unit_tests() -> Result<()> {
    run_all(unit_commands(Packages::detect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Packages = Packages {
        boot: false,
        kernel: false,
    };
    const ALL: Packages = Packages {
        boot: true,
        kernel: true,
    };

    #[test]
    fn lint_grows_with_the_bare_metal_packages() {
        assert_eq!(lint_commands(NONE).len(), 2);
        let all = lint_commands(ALL);
        assert_eq!(all.len(), 6);
        assert!(
            all.iter()
                .all(|c| c[0] == "fmt" || c.ends_with(&cmd(&["-D", "warnings"])))
        );
        assert!(
            all.iter()
                .any(|c| c.contains(&"x86_64-unknown-uefi".to_string()))
        );
        assert!(
            all.iter()
                .any(|c| c.contains(&"x86_64-unknown-none".to_string()))
        );
    }

    #[test]
    fn unit_tests_include_the_bare_metal_libraries() {
        assert_eq!(unit_commands(NONE), vec![cmd(&["test"])]);
        let all = unit_commands(ALL);
        assert!(all.contains(&cmd(&["test", "--package", "relay-boot", "--lib"])));
        assert!(all.contains(&cmd(&["test", "--package", "relay-kernel", "--lib"])));
    }

    /// The workflow installs the toolchain explicitly; it must be the one
    /// `rust-toolchain.toml` pins, and it must run the gates.
    #[test]
    fn workflow_matches_toolchain_and_runs_the_gates() {
        let toolchain = std::fs::read_to_string(root().join("rust-toolchain.toml")).unwrap();
        let channel = toolchain
            .lines()
            .find_map(|l| l.strip_prefix("channel = "))
            .map(|c| c.trim_matches('"'))
            .unwrap();
        let workflow = std::fs::read_to_string(root().join(".github/workflows/ci.yml")).unwrap();
        assert!(workflow.contains(&format!("rustup toolchain install {channel} ")));
        assert!(workflow.contains("run: cargo xtask lint"));
        assert!(workflow.contains("run: cargo xtask unit"));
    }
}
