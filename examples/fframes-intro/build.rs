//! Reads the fframes history with `git log` so the origin scene always shows
//! the repository as it is when the video is built.

use std::path::Path;
use std::process::Command;

fn main() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("commits.txt");

    // a new commit moves HEAD's reflog
    println!(
        "cargo:rerun-if-changed={}",
        repo.join(".git/logs/HEAD").display()
    );

    let log = Command::new("git")
        .current_dir(&repo)
        .args(["log", "--reverse", "--format=%h|%ad|%s", "--date=short"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_else(|| {
            println!("cargo:warning=git log failed, the origin scene will have no commits");
            String::new()
        });

    std::fs::write(out, log).unwrap();
}
