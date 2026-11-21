//! Which system family renders CJK text for a UI language (DESIGN.md §3, SPEC §13a).
//!
//! The Slint side takes one family name (`Fonts.cjk-family`, empty = no change). This module picks it: the host
//! passes the language setting and a check for installed families (fontique/fontconfig on the host side), and
//! sets the global to the answer. Pure, so it is tested without any font installed.

/// System families to try for `language` (a locale tag such as `ja`, `zh-Hans`, `zh_CN`, `pt-BR`), best first.
/// Empty for a language that needs none.
#[must_use]
pub fn cjk_font_candidates(language: &str) -> &'static [&'static str] {
    let tag = language.replace('_', "-").to_ascii_lowercase();
    let mut parts = tag.split('-');
    let lang = parts.next().unwrap_or("");
    let traditional = parts.any(|p| matches!(p, "hant" | "tw" | "hk" | "mo"));
    match lang {
        "ja" => &[
            "Yu Gothic UI",
            "Meiryo UI",
            "Noto Sans CJK JP",
            "IPAexGothic",
            "IPAGothic",
        ],
        "zh" if traditional => &["Microsoft JhengHei UI", "Noto Sans CJK TC", "PingFang TC"],
        "zh" => &[
            "Microsoft YaHei UI",
            "Noto Sans CJK SC",
            "PingFang SC",
            "WenQuanYi Micro Hei",
        ],
        "ko" => &["Malgun Gothic", "Noto Sans CJK KR", "Apple SD Gothic Neo"],
        _ => &[],
    }
}

/// The first of [`cjk_font_candidates`] that `installed` accepts, or an empty string (leave `Fonts.cjk-family`
/// empty: the renderer's own fallback applies).
#[must_use]
pub fn cjk_font_family(language: &str, installed: impl Fn(&str) -> bool) -> String {
    cjk_font_candidates(language)
        .iter()
        .find(|family| installed(family))
        .map(|family| (*family).to_owned())
        .unwrap_or_default()
}
