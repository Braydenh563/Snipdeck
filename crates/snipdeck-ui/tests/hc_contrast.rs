//! UX-34: in high contrast every text/background pair of the theme is a pair of system colours
//! that Windows guarantees to contrast (WindowText on Window, HighlightText on Highlight, GrayText
//! on Window). The first test reads the pairs from `ui/theme.slint`; the second measures their
//! contrast for the stock colour sets.
//!
//! Not verified here: that Windows hands the app these exact colours on a real desktop (the
//! `GetSysColor` call is Windows-only, see `tools/windows-test/`), and the Windows 11 contrast
//! themes (Aquatic, Desert, Dusk, Night sky), whose values this file does not carry.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use std::path::Path;

/// (foreground token, background token, expected foreground system colour, expected background).
const PAIRS: &[(&str, &str, &str, &str)] = &[
    ("text-primary", "bg-surface", "hc-window-text", "hc-window"),
    (
        "text-secondary",
        "bg-surface",
        "hc-window-text",
        "hc-window",
    ),
    ("text-tertiary", "bg-surface", "hc-gray-text", "hc-window"),
    (
        "text-on-selected",
        "bg-selected",
        "hc-highlight-text",
        "hc-highlight",
    ),
    (
        "text-secondary-on-selected",
        "bg-selected",
        "hc-highlight-text",
        "hc-highlight",
    ),
    (
        "accent-on-selected",
        "bg-selected",
        "hc-highlight-text",
        "hc-highlight",
    ),
    (
        "text-on-accent",
        "accent",
        "hc-highlight-text",
        "hc-highlight",
    ),
    ("text-on-ink", "ink", "hc-window", "hc-window-text"),
    (
        "search-mark-text",
        "search-mark",
        "hc-highlight-text",
        "hc-highlight",
    ),
    ("accent-text", "bg-surface", "hc-highlight", "hc-window"),
    ("focus-ring", "bg-surface", "hc-highlight", "hc-window"),
    ("danger-fg", "danger-bg", "hc-window-text", "hc-window"),
    ("warning-fg", "warning-bg", "hc-window-text", "hc-window"),
    ("success-fg", "success-bg", "hc-window-text", "hc-window"),
    ("info-fg", "info-bg", "hc-window-text", "hc-window"),
];

/// What a token is set to while `hc` is on: the system colour after `hc ? SystemLook.`.
fn hc_branch(source: &str, token: &str) -> String {
    let head = format!("out property <color> {token}: hc ? SystemLook.");
    let line = source
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with(&head))
        .unwrap_or_else(|| panic!("no hc branch for {token}"));
    line[head.len()..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect()
}

#[test]
fn every_high_contrast_pair_uses_the_system_colours_made_to_go_together() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/theme.slint");
    let source = std::fs::read_to_string(path).expect("theme.slint");
    let mut wrong = Vec::new();
    for (fg, bg, want_fg, want_bg) in PAIRS {
        let (got_fg, got_bg) = (hc_branch(&source, fg), hc_branch(&source, bg));
        if got_fg != *want_fg || got_bg != *want_bg {
            wrong.push(format!(
                "{fg} on {bg}: {got_fg} on {got_bg}, want {want_fg} on {want_bg}"
            ));
        }
    }
    assert_eq!(wrong.len(), 0, "{wrong:#?}");
}

fn lin(c: u8) -> f64 {
    let c = f64::from(c) / 255.0;
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let lum = |p: [u8; 3]| 0.2126 * lin(p[0]) + 0.7152 * lin(p[1]) + 0.0722 * lin(p[2]);
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// The stock "High Contrast Black" and "High Contrast White" colour sets (Window, WindowText,
/// Highlight, HighlightText, GrayText), as Windows ships them.
const STOCK: &[(&str, [[u8; 3]; 5])] = &[
    (
        "High Contrast Black",
        [
            [0, 0, 0],
            [255, 255, 255],
            [0x1A, 0xEB, 0xFF],
            [0, 0, 0],
            [0x3F, 0xF2, 0x3F],
        ],
    ),
    (
        "High Contrast White",
        [
            [255, 255, 255],
            [0, 0, 0],
            [0x37, 0x00, 0x6E],
            [255, 255, 255],
            [0x60, 0x00, 0x00],
        ],
    ),
];

#[test]
fn the_stock_high_contrast_sets_give_every_pair_at_least_4_5_to_1() {
    let mut low = Vec::new();
    for (name, [window, window_text, highlight, highlight_text, gray]) in STOCK {
        let colour = |sys: &str| match sys {
            "hc-window" => *window,
            "hc-window-text" => *window_text,
            "hc-highlight" => *highlight,
            "hc-highlight-text" => *highlight_text,
            "hc-gray-text" => *gray,
            other => panic!("unknown system colour {other}"),
        };
        for (fg, bg, sys_fg, sys_bg) in PAIRS {
            let ratio = contrast(colour(sys_fg), colour(sys_bg));
            if ratio < 4.5 {
                low.push(format!("{name}: {fg} on {bg} is {ratio:.2}:1"));
            }
        }
    }
    assert_eq!(low.len(), 0, "{low:#?}");
}
