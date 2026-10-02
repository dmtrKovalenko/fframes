// Static hashes only exist when the macro emits compile-time trees.
#[cfg(all(test, feature = "compile-time-svgtree"))]
mod static_hash_spec;
#[cfg(test)]
mod svgr_spec;
use std::{
    fs::File,
    io::{self, BufRead, Read},
};

use fframes::{Svgr, usvgr};

pub use futures;

fn read_snapshot(path: &std::path::Path) -> io::Result<String> {
    let r = File::open(path)?;
    let mut reader = io::BufReader::new(r);
    reader.read_until(b'\n', &mut Vec::new())?;
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;

    String::from_utf8(buf).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid UTF-8 sequence: {e}"),
        )
    })
}

pub fn assert_compile_time_svgr_eq_runtime(name: &str, svgr: Svgr) {
    let svgtree = svgr
        .into_svg_tree(
            &usvgr::Options::default(),
            &mut usvgr::Cache::default(),
            &usvgr::fontdb::Database::default(),
        )
        .unwrap();

    let snapshot = svgtree.to_string(&usvgr::WriteOptions {
        preserve_text: true,
        ..Default::default()
    });
    let path = format!("_svgr_snapshots/${name}.snapshot.txt");
    let snapshot_path = std::path::Path::new(path.as_str());
    let existing_file = snapshot_path.exists();

    let prefixed_snapshot = format!(
        "{}\n{snapshot}",
        if cfg!(feature = "compile-time-svgtree") {
            "(Compile-time)"
        } else {
            "(Runtime)"
        },
    );

    if existing_file {
        let is_eq = snapshot == read_snapshot(snapshot_path).unwrap();

        if !is_eq {
            let actual_path = if cfg!(feature = "compile-time-svgtree") {
                format!("_svgr_snapshots/${name}.inlined-actual.txt")
            } else {
                format!("_svgr_snapshots/${name}.runtime-actual.txt")
            };

            let diff_path = std::path::Path::new(actual_path.as_str());

            std::fs::write(diff_path, prefixed_snapshot).unwrap();

            if cfg!(feature = "compile-time-svgtree") {
                panic!(
                    "Compile-time svgtree is not equal to base snapshot for test {name}. See diff at {} for more details.",
                    diff_path.display()
                )
            } else {
                panic!(
                    "Runtime svgtree is not equal to base snapshot for test {name}. See diff at {} for more details.",
                    diff_path.display()
                )
            }
        }
    } else {
        std::fs::create_dir_all(snapshot_path.parent().unwrap()).unwrap();
        std::fs::write(snapshot_path, prefixed_snapshot).unwrap();
    }
}
