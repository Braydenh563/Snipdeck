//! Snipdeck spike S2: IME / CJK input in Slint.
//!
//! Run on Windows:
//!   cargo run                       # default renderer (winit + FemtoVG/OpenGL)
//!   set SLINT_BACKEND=winit-software && cargo run
//!   cargo run --features skia       # then SLINT_BACKEND=winit-skia (MSVC toolchain)
//!   cargo run -- --cjk zh           # Han fallback order for Simplified Chinese
//!   cargo run -- --cjk system       # leave fallback to DirectWrite (baseline)
//!
//! The manual test script with expected results lives in docs/spikes/S2-ime.md.

use std::rc::Rc;
use std::time::Instant;

use slint::fontique_011::fontique;
use slint::platform::Key;
use slint::{Model, ModelRc, SharedString, VecModel};

slint::include_modules!();

/// Which Han font wins when IBM Plex Sans has no glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CjkMode {
    /// Han → Yu Gothic UI first (Japanese glyph forms).
    Ja,
    /// Han → Microsoft YaHei UI first (Simplified Chinese glyph forms).
    Zh,
    /// Han → Malgun Gothic first (Korean Hanja forms).
    Ko,
    /// Don't touch fontique; DirectWrite's system fallback decides.
    System,
}

impl CjkMode {
    fn from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let value = args
            .iter()
            .position(|a| a == "--cjk")
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
            .unwrap_or("ja");
        match value {
            "zh" | "zh-Hans" => Self::Zh,
            "ko" => Self::Ko,
            "system" => Self::System,
            _ => Self::Ja,
        }
    }
}

const JA: &str = "Yu Gothic UI";
const ZH: &str = "Microsoft YaHei UI";
const KO: &str = "Malgun Gothic";
/// Older/alternative system fonts, used only if the primary ones are missing.
const JA_ALT: &str = "Meiryo UI";
const ZH_ALT: &str = "SimSun";

/// ISO 15924 tag, e.g. "Hani".
fn script(tag: &str) -> fontique::Script {
    let bytes: [u8; 4] = tag.as_bytes().try_into().expect("4-letter ISO 15924 tag");
    fontique::Script::from_bytes(bytes)
}

fn family_names(coll: &mut fontique::Collection, ids: &[fontique::FamilyId]) -> String {
    let names: Vec<String> = ids
        .iter()
        .map(|id| coll.family_name(*id).unwrap_or("?").to_string())
        .collect();
    if names.is_empty() {
        "(none)".into()
    } else {
        names.join(", ")
    }
}

/// Pins the per-script fallback chain in Slint's process-wide fontique collection.
///
/// Slint 1.18 shapes every run with the stack [font-family, SansSerif, SystemUi]
/// (IBM Plex Sans, Arial, Segoe UI on Windows). None of these has CJK glyphs, so parley
/// asks fontique for `FallbackKey(script, locale=None)`; without this function, that is
/// answered by DirectWrite's system fallback for a sample character with an empty locale.
fn configure_cjk_fallback(mode: CjkMode) -> Vec<String> {
    let mut coll = slint::fontique_011::shared_collection();
    let mut report = Vec::new();

    for tag in ["Hani", "Hira", "Kana", "Hang", "Bopo"] {
        let ids: Vec<_> = coll
            .fallback_families(fontique::FallbackKey::new(script(tag), None))
            .collect();
        report.push(format!(
            "system fallback before override  {tag}: {}",
            family_names(&mut coll, &ids)
        ));
    }

    for name in [JA, ZH, KO, JA_ALT, ZH_ALT, "IBM Plex Sans"] {
        report.push(format!(
            "family {name:<20} installed/registered: {}",
            coll.family_id(name).is_some()
        ));
    }

    if mode == CjkMode::System {
        report.push("--cjk system: fallback left to DirectWrite".into());
        return report;
    }

    let han: &[&str] = match mode {
        CjkMode::Ja => &[JA, JA_ALT, ZH, KO],
        CjkMode::Zh => &[ZH, ZH_ALT, JA, KO],
        CjkMode::Ko => &[KO, JA, ZH],
        CjkMode::System => unreachable!(),
    };
    let chains: [(&str, &[&str]); 5] = [
        ("Hani", han),
        ("Hira", &[JA, JA_ALT]),
        ("Kana", &[JA, JA_ALT]),
        ("Hang", &[KO]),
        ("Bopo", &[ZH, ZH_ALT]),
    ];
    for (tag, names) in chains {
        let ids: Vec<_> = names.iter().filter_map(|n| coll.family_id(n)).collect();
        if ids.is_empty() {
            report.push(format!(
                "override {tag}: no candidate installed, kept system fallback"
            ));
            continue;
        }
        let key = fontique::FallbackKey::new(script(tag), None);
        coll.set_fallbacks(key, ids.iter().copied());
        report.push(format!("override {tag}: {}", family_names(&mut coll, &ids)));
    }
    report
}

/// Slint encodes non-printable keys as private-use characters; name the ones that matter
/// for IME testing so the log is readable.
fn describe_key(text: &str) -> String {
    let named = [
        (Key::Return, "Return"),
        (Key::Escape, "Escape"),
        (Key::Backspace, "Backspace"),
        (Key::Delete, "Delete"),
        (Key::Tab, "Tab"),
        (Key::Space, "Space"),
        (Key::LeftArrow, "Left"),
        (Key::RightArrow, "Right"),
        (Key::UpArrow, "Up"),
        (Key::DownArrow, "Down"),
        (Key::Home, "Home"),
        (Key::End, "End"),
        (Key::Shift, "Shift"),
        (Key::Control, "Control"),
        (Key::Alt, "Alt"),
    ];
    for (key, name) in named {
        if SharedString::from(key).as_str() == text {
            return format!("<{name}>");
        }
    }
    format!("{text:?}")
}

const CHECKLIST: &[&str] = &[
    "Setup: Windows 11 → Settings → Time & language → add Japanese (Microsoft IME), Chinese (Simplified, Microsoft Pinyin), Korean (Microsoft IME). Switch with Win+Space.",
    "1  JA, LineEdit: type nihongo → see にほんご highlighted inline, then Space → 日本語, Enter. Log: one 'edited' with 日本語, NO 'accepted', NO key-pressed <Return>.",
    "2  JA: candidate window appears directly below the caret of the field you type in (not window corner, not screen 0,0).",
    "3  ZH Pinyin, LineEdit: type zhongwen → 中文 via Space or 1. Candidate bar under the caret.",
    "4  KO, LineEdit: type gksrmf (2-set) → 한글; each syllable shows inline while composing.",
    "5  JA long composition: type watashihanihongonobenkyouwoshiteimasu, Space; use Left/Right to move between clauses, Space to re-convert, Enter.",
    "6  Arrow keys while composing do NOT move Slint's caret (log shows no key-pressed); after commit they do.",
    "7  Narrow LineEdit: long composition at the right edge — is the composition visible (field scrolls) before commit?",
    "8  TextEdit: click at the end of the first wrapped line, compose nihongo → wrap + candidate window follow the composition.",
    "9  PlexInput: same as 1/3/4; the red preedit line mirrors the composition; Latin in Plex, CJK in Yu Gothic/YaHei.",
    "10 Esc while composing cancels the composition; field text unchanged; no 'accepted'.",
    "11 Backspace while composing edits the composition only.",
    "12 Compose, then click another field: note whether the composition is committed, dropped, or moves to the new field.",
    "13 Compose, then Alt+Tab away and back: note whether the composition is lost (known upstream bug slint#10861).",
    "14 Font rows: compare 直骨角今海 across rows; default row should match Yu Gothic (--cjk ja) or YaHei (--cjk zh).",
    "15 Repeat 1, 3, 4 with SLINT_BACKEND=winit-software and (MSVC) --features skia + winit-skia.",
    "16 Repeat at 150% display scaling and on a second monitor: candidate window still at the caret.",
];

fn main() -> Result<(), slint::PlatformError> {
    let mode = CjkMode::from_args();
    let fallback_report = configure_cjk_fallback(mode);

    let ui = AppWindow::new()?;

    let backend =
        std::env::var("SLINT_BACKEND").unwrap_or_else(|_| "default (winit-femtovg)".into());
    let info = format!("backend: {backend} · --cjk {mode:?} · Win+Space switches IME");
    ui.set_info(info.clone().into());

    println!("== Snipdeck S2 IME spike ==\n{info}");
    for line in &fallback_report {
        println!("  {line}");
    }
    // IBM Plex Sans is registered when the component is created (import in app.slint).
    let mut coll = slint::fontique_011::shared_collection();
    println!(
        "  IBM Plex Sans registered after window creation: {}",
        coll.family_id("IBM Plex Sans").is_some()
    );
    println!("\nChecklist:");
    for line in CHECKLIST {
        println!("  {line}");
    }
    println!();

    let checklist: Vec<SharedString> = CHECKLIST.iter().map(|s| (*s).into()).collect();
    ui.set_checklist(ModelRc::from(Rc::new(VecModel::from(checklist))));

    let log = Rc::new(VecModel::<SharedString>::default());
    ui.set_log(ModelRc::from(log.clone()));

    let start = Instant::now();
    let log_for_events = log.clone();
    ui.on_event(move |source, kind, text| {
        let shown = if kind == "key-pressed" {
            describe_key(&text)
        } else {
            format!("{:?}", text.as_str())
        };
        let line = format!(
            "{:>7.3}s {source:<15} {kind:<11} {shown}",
            start.elapsed().as_secs_f64()
        );
        println!("{line}");
        log_for_events.insert(0, line.into());
        while log_for_events.row_count() > 300 {
            log_for_events.remove(log_for_events.row_count() - 1);
        }
    });

    let log_for_clear = log;
    ui.on_clear_log(move || log_for_clear.set_vec(Vec::new()));

    ui.run()
}
