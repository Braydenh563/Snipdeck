//! Every edit of an item in the controllers is one history step: no code outside the recorded
//! paths may call `update_item` (LIBRARY-37).
#![allow(clippy::unwrap_used, reason = "test code")]

use std::{fs, path::Path};

/// The only non-test files that write an item: the editor's save (which records its own step)
/// and the history module (which applies a step's changes and `update_recorded`).
const ALLOWED: [&str; 2] = ["controller/editor_ctl/save.rs", "controller/history_ctl.rs"];

fn sources(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "tests") {
                continue;
            }
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_controller_code_writes_an_item_outside_the_recorded_paths() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src, &mut files);
    assert!(files.len() > 20, "the sources were found");
    let mut bypass = Vec::new();
    for file in files {
        let rel = file
            .strip_prefix(&src)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if rel == "controller/tests.rs" || ALLOWED.contains(&rel.as_str()) {
            continue;
        }
        let text = fs::read_to_string(&file).unwrap();
        // Unit-test modules sit at the end of a file.
        let live = text.split("#[cfg(test)]").next().unwrap();
        for (n, line) in live.lines().enumerate() {
            if line.contains(".update_item(") {
                bypass.push(format!("{rel}:{}", n + 1));
            }
        }
    }
    assert!(
        bypass.is_empty(),
        "these write an item without a history step: {bypass:?}"
    );
}
