fn main() {
    // We only need to link the C library for tests and benchmarks,
    // but building it is handled by the Justfile.
    // This tells Cargo where to find the library so it only links it to OUR crate,
    // and doesn't break external dependencies' build scripts (like libc or serde).
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-search=native={}/Turbo-Base64", dir);
    println!("cargo:rustc-link-lib=static=tb64");
    println!("cargo:rerun-if-changed=Turbo-Base64/libtb64.a");
}
