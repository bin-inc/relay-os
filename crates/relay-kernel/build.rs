fn main() {
    println!("cargo::rerun-if-changed=x86_64-relay.ld");
    let script =
        std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("x86_64-relay.ld");
    println!("cargo::rustc-link-arg=-T{}", script.display());
    println!("cargo::rustc-link-arg=-no-pie");
}
