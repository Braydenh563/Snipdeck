//! UX-30, UX-31: the Slint sources use the design tokens. A literal radius is allowed only on a hairline bar
//! (2px or less: a focus bar, a drag pip); every other radius comes from `Radius`. Font weights come from `Type`.

#![allow(clippy::expect_used, clippy::unwrap_used, reason = "test code")]

use std::{fs, path::Path};

fn slint_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(dir).expect("read dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            slint_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "slint") {
            out.push(path);
        }
    }
}

/// Every `property: <number>px;` line for `property` in the UI sources, as `(file:line, number)`.
fn literals(property: &str) -> Vec<(String, f32)> {
    let mut files = Vec::new();
    slint_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("ui"),
        &mut files,
    );
    let prefix = format!("{property}:");
    let mut found = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file).expect("read");
        for (i, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("").trim();
            let Some(rest) = code.strip_prefix(&prefix) else {
                continue;
            };
            let value = rest.trim().trim_end_matches(';').trim();
            let digits = value.trim_end_matches("px");
            if let Ok(n) = digits.parse::<f32>()
                && (value.ends_with("px") || value.parse::<f32>().is_ok())
            {
                found.push((format!("{}:{}", file.display(), i + 1), n));
            }
        }
    }
    found
}

#[test]
fn radii_come_from_the_tokens_except_on_hairline_bars() {
    let bad: Vec<_> = literals("border-radius")
        .into_iter()
        .filter(|(_, n)| *n > 2.0)
        .collect();
    assert_eq!(
        bad.len(),
        0,
        "literal radii over 2px (use Radius.*): {bad:?}"
    );
}

#[test]
fn font_weights_come_from_the_type_tokens() {
    let bad = literals("font-weight");
    assert_eq!(bad.len(), 0, "literal font weights (use Type.*): {bad:?}");
}
