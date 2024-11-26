fn main() {
    // Tell cargo to look for zlib
    println!("cargo:rustc-link-lib=z");

    // Tell cargo to invalidate the built crate whenever the build script changes
    println!("cargo:rerun-if-changed=build.rs");
}
