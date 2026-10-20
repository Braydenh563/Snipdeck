//! The `testing` feature of `snipdeck-store` lowers the Argon2 cost floor for stored key records, so
//! no manifest may switch it on for a shipped build. `src/lib.rs` also refuses to compile it in a
//! release profile; these checks catch the mistake earlier, in a debug `cargo test`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]

use std::path::Path;

/// The manifest text of a workspace member or of the workspace root.
fn manifest(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The part of a manifest before its `[dev-dependencies]` table: what a release build links.
fn before_dev_dependencies(text: &str) -> &str {
    text.split("[dev-dependencies]").next().unwrap()
}

fn enables_store_testing(text: &str) -> bool {
    text.lines()
        .filter(|line| line.contains("snipdeck-store"))
        .any(|line| line.contains("testing"))
}

#[test]
fn no_shipped_dependency_enables_the_store_testing_feature() {
    for member in [
        "../snipdeck-app/Cargo.toml",
        "../snipdeck-portable/Cargo.toml",
        "../snipdeck-ui/Cargo.toml",
        "../snipdeck-platform/Cargo.toml",
        "../snipdeck-core/Cargo.toml",
        "../../tools/snipdeck-licence-issuer/Cargo.toml",
        "../../Cargo.toml",
    ] {
        let text = manifest(member);
        assert!(
            !enables_store_testing(before_dev_dependencies(&text)),
            "{member}: only [dev-dependencies] may enable snipdeck-store/testing"
        );
    }
}

#[test]
fn the_app_enables_it_only_as_a_dev_dependency() {
    let text = manifest("../snipdeck-app/Cargo.toml");
    assert!(
        enables_store_testing(&text),
        "the app tests do use the feature; if that changed, update this guard"
    );
}

#[test]
fn the_feature_is_not_a_default_feature() {
    let text = manifest("Cargo.toml");
    assert!(!text.contains("default ="), "no default features at all");
}
