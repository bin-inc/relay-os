fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None | Some("doctor") if args.next().is_none() => doctor(),
        Some("image") => image(args),
        Some("verify-image") => verify_image(args),
        Some("qemu") => unavailable("qemu is unavailable until Task 4"),
        Some(command) => unavailable(&format!(
            "unknown command `{command}`; available commands are `doctor`, `image`, and `verify-image`"
        )),
        None => unreachable!(),
    }
}

fn image(mut args: impl Iterator<Item = String>) {
    let Some(flag) = args.next() else {
        unavailable("image requires `--output PATH`");
    };
    let Some(output) = args.next() else {
        unavailable("image requires `--output PATH`");
    };
    if flag != "--output" || args.next().is_some() {
        unavailable("image requires exactly `--output PATH`");
    }
    if let Err(error) = relay_xtask::image::build(std::path::Path::new(&output)) {
        eprintln!("cargo xtask image: {error}");
        std::process::exit(1);
    }
}

fn verify_image(mut args: impl Iterator<Item = String>) {
    let Some(image) = args.next() else {
        unavailable("verify-image requires IMAGE");
    };
    if args.next().is_some() {
        unavailable("verify-image requires exactly one IMAGE");
    }
    match relay_xtask::verify::verify_image(std::path::Path::new(&image)) {
        Ok(report) => {
            print!("{}{}", report.sgdisk, report.e2fsck);
        }
        Err(error) => {
            eprintln!("cargo xtask verify-image: {error}");
            std::process::exit(1);
        }
    }
}

fn doctor() {
    let report = relay_xtask::doctor::inspect();
    relay_xtask::doctor::print_versions();

    if !report.ready() {
        eprintln!(
            "missing required tools or targets: {}",
            report.missing.join(", ")
        );
        std::process::exit(1);
    }
}

fn unavailable(message: &str) -> ! {
    eprintln!("cargo xtask: {message}");
    std::process::exit(2);
}
