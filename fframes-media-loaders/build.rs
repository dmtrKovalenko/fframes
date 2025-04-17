fn main() {
    // Check if we're targeting macOS or iOS
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    // If we're on an Apple platform but videotoolbox is not enabled, emit a warning
    if (target_os == "macos" || target_os == "ios")
        && !std::env::var("CARGO_FEATURE_VIDEOTOOLBOX").is_ok()
    {
        println!(
            "cargo:warning=\x1B[1m⚠️  WARNING: Apple target arch without videotoolbox detected ⚠️\x1B[0m"
        );
        println!(
            "cargo:warning=To leverage hardware acceleration of encodging/decodring it is strongly recommended to enable the 'videotoolbox' feature."
        );
        println!(
            "cargo:warning=To enable VideoToolbox, add --features videotoolbox to your cargo command or add videotoolbox to the features list in your Cargo.toml."
        );
    }
}
