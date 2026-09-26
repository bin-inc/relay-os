fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    // Only the kernel binary uses the linker script; host tests of the
    // library link normally.
    println!("cargo:rustc-link-arg-bins=-T{dir}/linker.ld");
    println!("cargo:rerun-if-changed=linker.ld");
}
