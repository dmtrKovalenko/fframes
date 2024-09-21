use std::fs;
use std::path::Path;

fn main() {
    // Get the path to the "files" directory
    let files_dir = Path::new("files");

    // Check if the directory exists
    if files_dir.exists() && files_dir.is_dir() {
        // Iterate over the entries in the directory
        for entry in fs::read_dir(files_dir)
            .expect("Failed to read files directory")
            .flatten()
        {
            let path = entry.path();
            if path.is_file() {
                // Emit a rerun-if-changed instruction for each file
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }
}
