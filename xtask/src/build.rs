//! `cargo xtask build`: compiles the loader, the kernel and the user
//! programs, which it packs into `system.img`.

use crate::config::{KERNEL_TARGET, PROFILE, UEFI_TARGET};
use crate::userland;
use crate::util::{cargo, root};
use anyhow::Result;
use std::path::PathBuf;

pub struct Artifacts {
    pub bootx64: PathBuf,
    pub kernel: PathBuf,
    pub system_img: PathBuf,
}

pub fn build() -> Result<Artifacts> {
    for (package, target) in [("relay-boot", UEFI_TARGET), ("relay-kernel", KERNEL_TARGET)] {
        let status = cargo()
            .args([
                "build",
                "--profile",
                PROFILE,
                "--package",
                package,
                "--target",
                target,
            ])
            .status()?;
        anyhow::ensure!(status.success(), "building {package} failed");
    }
    let system_img = userland::build_system_image()?;
    let t = root().join("target");
    Ok(Artifacts {
        bootx64: t.join(UEFI_TARGET).join(PROFILE).join("relay-boot.efi"),
        kernel: t.join(KERNEL_TARGET).join(PROFILE).join("relay-kernel"),
        system_img,
    })
}
