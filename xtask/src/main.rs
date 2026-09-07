fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None | Some("doctor") if args.next().is_none() => doctor(),
        Some("qemu") => unavailable("qemu is unavailable until Task 4"),
        Some(command) => unavailable(&format!(
            "unknown command `{command}`; only `doctor` is available"
        )),
        None => unreachable!(),
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
