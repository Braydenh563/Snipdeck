//! The Windows subsystem of the exe does not depend on the build profile (PLATFORM-35): a dev
//! build is a GUI program like the release, so a console window never opens with it.

const MAIN: &str = include_str!("../src/main.rs");

fn subsystem_attribute() -> &'static str {
    MAIN.lines()
        .find(|l| l.contains("windows_subsystem"))
        .expect("main.rs sets the windows subsystem")
}

#[test]
fn the_subsystem_is_not_tied_to_debug_assertions() {
    let line = subsystem_attribute();
    assert!(!line.contains("debug_assertions"), "{line}");
    assert!(line.contains(r#"windows_subsystem = "windows""#), "{line}");
}

#[test]
fn only_the_console_feature_turns_the_gui_subsystem_off() {
    let line = subsystem_attribute();
    assert!(line.contains(r#"not(feature = "console")"#), "{line}");
    let manifest = include_str!("../Cargo.toml");
    assert!(manifest.contains("console = []"));
}
