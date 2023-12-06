#[cfg(test)]
mod svgr_spec;
use std::{
    fs::File,
    io::{self, BufRead, Read},
};

use fframes::{usvgr::svgtree::Document, Svgr};

pub use futures;

#[cfg(feature = "compile-time-svgtree")]
fn resolve_maybe_precompiled_tree(svgr: Svgr) -> Document {
    svgr.svg_tree.try_into().unwrap()
}

#[cfg(not(feature = "compile-time-svgtree"))]
fn resolve_maybe_precompiled_tree(svgr: Svgr) -> Document {
    use fframes::usvgr::roxmltree::ParsingOptions;

    fframes::usvgr::svgtree::Document::parse(
        &fframes::usvgr::roxmltree::Document::parse_with_options(
            svgr.value.as_str(),
            ParsingOptions { allow_dtd: true },
        )
        .unwrap(),
    )
    .unwrap()
}

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
    let svgtree: Document = resolve_maybe_precompiled_tree(svgr);

    let snapshot = format!("{svgtree:?}");
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

    if !existing_file {
        std::fs::create_dir_all(snapshot_path.parent().unwrap()).unwrap();
        std::fs::write(snapshot_path, prefixed_snapshot).unwrap();
    } else {
        let is_eq = snapshot == read_snapshot(snapshot_path).unwrap();

        if !is_eq {
            let actual_path = if cfg!(feature = "compile-time-svgtree") {
                format!("_svgr_snapshots/${name}.runtime-actual.txt")
            } else {
                format!("_svgr_snapshots/${name}.inlined-actual.txt")
            };

            let actual_path = std::path::Path::new(actual_path.as_str());
            std::fs::write(actual_path, prefixed_snapshot).unwrap();

            if cfg!(feature = "compile-time-svgtree") {
                panic!("Compile-time svgtree is not equal to base snapshot for test {name}. See diff at {} for more details.", actual_path.display())
            } else {
                panic!("Runtime svgtree is not equal to base snapshot for test {name}. See diff at {} for more details.", actual_path.display())
            }
        }
    }
}
