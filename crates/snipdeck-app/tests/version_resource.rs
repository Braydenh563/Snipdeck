//! The exe's version resource names the company and the licence (Explorer's Details tab and
//! `SmartScreen` show them). The resource is built on Windows only, so this reads the build script.
#![allow(clippy::unwrap_used, reason = "test code")]

#[test]
fn the_version_resource_has_a_company_and_a_copyright() {
    let build = include_str!("../build.rs");
    for key in [
        "ProductName",
        "CompanyName",
        "LegalCopyright",
        "FileDescription",
        "OriginalFilename",
        "InternalName",
    ] {
        assert!(
            build.contains(&format!(".set(\"{key}\"")) || build.contains(&format!("\"{key}\"")),
            "{key}"
        );
    }
    assert!(build.contains("GPL-3.0-only"));
}
