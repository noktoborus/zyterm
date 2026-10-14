//! Writes the pictures of `img/` as Markdown link definitions of `data:` URLs.
//!
//! The README names each picture by a label and defines the label as
//! `img/<name>.png`, which is where GitHub finds it. Rustdoc copies no file, so
//! the crate documentation puts these definitions before the README: Markdown
//! takes the first definition of a label, and the picture is in the page
//! itself.

use base64::Engine;
use std::fmt::Write;
use std::path::Path;

/// The directory of the pictures, beside the README.
const PICTURES: &str = "img";

fn main() {
    println!("cargo:rerun-if-changed={PICTURES}");

    let mut names: Vec<_> = std::fs::read_dir(PICTURES)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| path.extension().is_some_and(|extension| extension == "png"))
                .filter(|path| !is_left_by_a_failed_test(path))
                .collect()
        })
        .unwrap_or_default();
    names.sort();

    let mut definitions = String::new();
    for path in names {
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let bytes = std::fs::read(&path).expect("a picture of img/ is readable");
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        let _ = writeln!(
            definitions,
            "[picture-{stem}]: data:image/png;base64,{encoded}"
        );
    }

    let out = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    std::fs::write(Path::new(&out).join("pictures.md"), definitions)
        .expect("the definitions are written");
}

/// True for the new picture, the difference and the backup a picture test
/// leaves beside the one it compared against.
fn is_left_by_a_failed_test(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    [".new.png", ".diff.png", ".old.png"]
        .iter()
        .any(|suffix| name.ends_with(suffix))
}
