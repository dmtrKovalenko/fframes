use std::{env, path::Path, process::Command};

fn main() {
    println!("cargo::rerun-if-changed=Cargo.toml");
    if env::var_os("CARGO_FEATURE_DYNAMIC_LINKING").is_none() {
        return;
    }
    let origin = match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => "@loader_path",
        Ok("linux") => "$ORIGIN",
        _ => return,
    };
    if env::var("CARGO_CFG_TARGET_FEATURE")
        .unwrap_or_default()
        .split(',')
        .any(|f| f == "crt-static")
    {
        return;
    }

    // Cargo sets the loader path for `cargo run`. Give a directly launched binary the
    // same access to the engine in target/<profile>/deps and the toolchain's libstd.
    let target = env::var("TARGET").unwrap();
    let libdir = Command::new(env::var_os("RUSTC").unwrap())
        .args(["--print", "target-libdir", "--target"])
        .arg(&target)
        .output()
        .expect("run rustc to locate the target's Rust libraries");
    assert!(
        libdir.status.success(),
        "rustc --print target-libdir failed"
    );
    let libdir = String::from_utf8(libdir.stdout).unwrap();
    let libdir = libdir.trim_end();
    let deps = format!("{origin}/deps");
    for dir in [deps.as_str(), origin, libdir] {
        println!("cargo::rustc-link-arg-bins=-Wl,-rpath");
        println!("cargo::rustc-link-arg-bins=-Xlinker");
        println!("cargo::rustc-link-arg-bins={dir}");
    }

    // Native GNU/Linux ARM64 uses the system linker by default. Rustup ships LLD next
    // to libstd. Use it for the video executable if the user hasn't selected a linker.
    if target == "aarch64-unknown-linux-gnu"
        && env::var("HOST").as_deref() == Ok(target.as_str())
        && env::var_os("RUSTC_LINKER").is_none()
        && !env::var("CARGO_ENCODED_RUSTFLAGS")
            .unwrap_or_default()
            .contains("link")
    {
        let linker_dir = Path::new(libdir).join("../bin/gcc-ld");
        if linker_dir.join("ld.lld").is_file() {
            println!("cargo::rustc-link-arg-bins=-B{}", linker_dir.display());
            println!("cargo::rustc-link-arg-bins=-fuse-ld=lld");
            // One worker kept warm Vulkan links fast even on hosts with many cores.
            println!("cargo::rustc-link-arg-bins=-Wl,--threads=1");
        }
    }
}
