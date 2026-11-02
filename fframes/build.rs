fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if (target_os == "macos" || target_os == "ios")
        && std::env::var_os("CARGO_FEATURE_VIDEOTOOLBOX").is_none()
        && std::env::var_os("CARGO_FEATURE_PREVIEW_VIDEOTOOLBOX").is_none()
    {
        println!(
            "cargo:warning=\x1B[1m⚠️  WARNING: Apple target arch without videotoolbox enabled ⚠️\x1B[0m"
        );
        println!(
            "cargo:warning=To leverage hardware acceleration of encoding/decoding it is strongly recommended to enable the \"videotoolbox\" feature. To enable videotoolbox, add \"videotoolbox\" to the `fframes` dependency features list in your Cargo.toml"
        );
    }
}
