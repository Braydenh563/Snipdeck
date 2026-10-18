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
        "zh" if traditional => &[
            "Microsoft JhengHei UI",
            "Noto Sans CJK TC",
            "PingFang TC",
        ],
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_locales_map_to_the_families_of_the_design() {
        assert_eq!(cjk_font_candidates("ja")[0], "Yu Gothic UI");
        assert_eq!(cjk_font_candidates("ja-JP")[0], "Yu Gothic UI");
        assert_eq!(cjk_font_candidates("zh-Hans")[0], "Microsoft YaHei UI");
        assert_eq!(cjk_font_candidates("zh_CN")[0], "Microsoft YaHei UI");
        assert_eq!(cjk_font_candidates("zh-Hant")[0], "Microsoft JhengHei UI");
        assert_eq!(cjk_font_candidates("ko")[0], "Malgun Gothic");
        for latin in ["en", "de", "fr", "es", "pt-BR", ""] {
            assert!(cjk_font_candidates(latin).is_empty(), "{latin}");
        }
    }

    #[test]
    fn the_first_installed_family_wins_and_none_means_no_change() {
        let only = |name: &'static str| move |f: &str| f == name;
        assert_eq!(cjk_font_family("ja", only("Noto Sans CJK JP")), "Noto Sans CJK JP");
        assert_eq!(cjk_font_family("ja", |_| true), "Yu Gothic UI");
        assert_eq!(cjk_font_family("ja", |_| false), "");
        assert_eq!(cjk_font_family("de", |_| true), "");
    }
}
