fn main() {
    // The user linker script, from relay-rt's build script.
    let script = std::env::var("DEP_RELAY_RT_SCRIPT").unwrap();
    println!("cargo:rustc-link-arg-bins=-T{script}");
}
