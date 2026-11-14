//! The words and swatches of a colour item outside the Panel: the Main window's Colour block and
//! its "Paste as…", and the Picker's. Pure functions over data, so they are tested without a
//! window. The translated word for the `Name` format comes from the window (`ColourTexts`).

use slint::SharedString;
use snipdeck_colour::Format;
use snipdeck_core::ItemKind;

/// The label of a format row: `HEX`, `RGB` … and the window's own word for `Name`.
#[must_use]
pub fn format_word(f: Format, name: &SharedString) -> SharedString {
    if f == Format::Name {
        name.clone()
    } else {
        f.label().into()
    }
}

/// A "Paste as" choice of a colour: the format and the colour written that way, as in the
/// Panel's chooser.
#[must_use]
pub fn choice_text(f: Format, name: &SharedString, value: &str) -> String {
    format!("{}  {value}", format_word(f, name))
}

/// The swatch of a row: the colour its text is, for a plain Text or Code item (the kinds that
/// are colour items), as the colour and whether there is one.
#[must_use]
pub fn swatch_of_kind(kind: ItemKind, preview: &str, sensitive: bool) -> (slint::Color, bool) {
    if matches!(kind, ItemKind::Text | ItemKind::Code) {
        crate::binding::swatch_fields(preview, sensitive)
    } else {
        (slint::Color::default(), false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_name_row_takes_the_windows_word() {
        let name = SharedString::from("Nom");
        assert_eq!(format_word(Format::Hex, &name), "HEX");
        assert_eq!(format_word(Format::Oklch, &name), "OKLCH");
        assert_eq!(format_word(Format::Name, &name), "Nom");
        assert_eq!(
            choice_text(Format::Name, &name, "steelblue"),
            "Nom  steelblue"
        );
        assert_eq!(
            choice_text(Format::Rgb, &name, "rgb(1, 2, 3)"),
            "RGB  rgb(1, 2, 3)"
        );
    }

    #[test]
    fn a_swatch_is_for_text_and_code_that_are_one_colour_and_never_a_secret() {
        assert!(swatch_of_kind(ItemKind::Text, "#3a7bd5", false).1);
        assert!(swatch_of_kind(ItemKind::Code, "#3a7bd5", false).1);
        assert!(!swatch_of_kind(ItemKind::Template, "#3a7bd5", false).1);
        assert!(!swatch_of_kind(ItemKind::Link, "#3a7bd5", false).1);
        assert!(!swatch_of_kind(ItemKind::Text, "#3a7bd5", true).1);
        assert!(!swatch_of_kind(ItemKind::Text, "hello", false).1);
    }
}
