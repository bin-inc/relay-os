fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    // Every program's build script passes this to the linker (see
    // `userland/tests/build.rs`).
    println!("cargo:script={dir}/user.ld");
    println!("cargo:rerun-if-changed=user.ld");
}
