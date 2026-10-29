use std::{env, process::Command};

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
    let libdir = Command::new(env::var_os("RUSTC").unwrap())
        .args(["--print", "target-libdir", "--target"])
        .arg(env::var_os("TARGET").unwrap())
        .output()
        .expect("run rustc to locate the target's Rust libraries");
    assert!(
        libdir.status.success(),
        "rustc --print target-libdir failed"
    );
    let libdir = String::from_utf8(libdir.stdout).unwrap();
    let deps = format!("{origin}/deps");
    for dir in [deps.as_str(), origin, libdir.trim_end()] {
        println!("cargo::rustc-link-arg-bins=-Wl,-rpath");
        println!("cargo::rustc-link-arg-bins=-Xlinker");
        println!("cargo::rustc-link-arg-bins={dir}");
    }
}
