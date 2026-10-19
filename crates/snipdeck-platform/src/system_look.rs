//! What Windows says about how apps should look: light or dark, and high contrast.
//!
//! The decisions are portable and tested here; the Windows backend
//! (`windows::system_look`) reads the registry value `AppsUseLightTheme` and
//! `SystemParametersInfo(SPI_GETHIGHCONTRAST)` and watches for changes. The app combines this
//! with its own `theme` setting (`system | light | dark`), see `snipdeck_app::settings`.

/// An 8-bit RGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

impl Rgb {
    /// From a Win32 `COLORREF` (`0x00BBGGRR`).
    #[must_use]
    pub const fn from_colorref(colorref: u32) -> Self {
        Self {
            r: (colorref & 0xFF) as u8,
            g: ((colorref >> 8) & 0xFF) as u8,
            b: ((colorref >> 16) & 0xFF) as u8,
        }
    }
}

/// The system colours that apply while a high-contrast theme is on (`GetSysColor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighContrast {
    /// `COLOR_WINDOW`.
    pub window: Rgb,
    /// `COLOR_WINDOWTEXT`.
    pub window_text: Rgb,
    /// `COLOR_HIGHLIGHT`.
    pub highlight: Rgb,
    /// `COLOR_HIGHLIGHTTEXT`.
    pub highlight_text: Rgb,
    /// `COLOR_GRAYTEXT`.
    pub gray_text: Rgb,
}

impl Default for HighContrast {
    /// "High Contrast Black", the values the UI's `SystemLook` global starts with.
    fn default() -> Self {
        let rgb = |r, g, b| Rgb { r, g, b };
        Self {
            window: rgb(0, 0, 0),
            window_text: rgb(255, 255, 255),
            highlight: rgb(0x1a, 0xeb, 0xff),
            highlight_text: rgb(0, 0, 0),
            gray_text: rgb(0x3f, 0xf2, 0x3f),
        }
    }
}

/// How Windows is set up right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SystemLook {
    /// Apps should be dark ("Choose your default app mode" is Dark).
    pub dark: bool,
    /// A high-contrast theme is on, with its colours. Wins over everything else.
    pub high_contrast: Option<HighContrast>,
    /// The colour Windows uses as its accent, when it can be read.
    pub accent: Option<Rgb>,
}

/// Reads the current [`SystemLook`]. Cheap enough to call on every change notification, and
/// callable from any thread.
pub trait SystemLookSource: Send + Sync {
    /// The look now. A value that cannot be read gives the light default.
    fn read(&self) -> SystemLook;
}

// Window messages and `SPI_*` codes that can change the look. Written out so the filter is
// testable here; the Windows backend asserts they equal the `windows` crate's constants.
/// `WM_SYSCOLORCHANGE`.
pub const WM_SYSCOLORCHANGE: u32 = 0x0015;
/// `WM_SETTINGCHANGE`.
pub const WM_SETTINGCHANGE: u32 = 0x001A;
/// `WM_THEMECHANGED`.
pub const WM_THEMECHANGED: u32 = 0x031A;
/// `SPI_SETHIGHCONTRAST`.
pub const SPI_SETHIGHCONTRAST: usize = 0x0043;
/// The `lParam` string Windows sends with `WM_SETTINGCHANGE` when the app or system colour mode
/// changes.
pub const IMMERSIVE_COLOR_SET: &str = "ImmersiveColorSet";

/// Whether a window message can have changed the look, so the source should be read again.
///
/// `setting_area` is the `lParam` string of a `WM_SETTINGCHANGE` (the area that changed), if it
/// carried one. A false positive costs one registry read; a false negative means the app stays
/// in the wrong theme until restart, so unknown `WM_SETTINGCHANGE`s with the colour-set area or
/// the high-contrast code are the only ones filtered in.
#[must_use]
pub fn message_may_change_look(message: u32, wparam: usize, setting_area: Option<&str>) -> bool {
    match message {
        WM_SYSCOLORCHANGE | WM_THEMECHANGED => true,
        WM_SETTINGCHANGE => {
            wparam == SPI_SETHIGHCONTRAST || setting_area == Some(IMMERSIVE_COLOR_SET)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorref_is_blue_green_red() {
        let c = Rgb::from_colorref(0x00_33_22_11);
        assert_eq!((c.r, c.g, c.b), (0x11, 0x22, 0x33));
    }

    #[test]
    fn the_default_look_is_light_with_no_high_contrast() {
        let look = SystemLook::default();
        assert!(!look.dark);
        assert_eq!(look.high_contrast, None);
    }

    #[test]
    fn only_colour_related_changes_trigger_a_re_read() {
        assert!(message_may_change_look(
            WM_SETTINGCHANGE,
            0,
            Some("ImmersiveColorSet")
        ));
        assert!(message_may_change_look(
            WM_SETTINGCHANGE,
            SPI_SETHIGHCONTRAST,
            None
        ));
        assert!(message_may_change_look(WM_THEMECHANGED, 0, None));
        assert!(message_may_change_look(WM_SYSCOLORCHANGE, 0, None));
        // Other settings: wallpaper (0x14), the Environment area, a bare WM_SETTINGCHANGE.
        assert!(!message_may_change_look(WM_SETTINGCHANGE, 0x14, None));
        assert!(!message_may_change_look(
            WM_SETTINGCHANGE,
            0,
            Some("Environment")
        ));
        assert!(!message_may_change_look(WM_SETTINGCHANGE, 0, None));
        assert!(!message_may_change_look(0x0001, 0, None));
    }
}
