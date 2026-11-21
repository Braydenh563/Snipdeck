//! Which DWM window attributes Snipdeck sets on its windows, decided from the Windows build
//! (PLATFORM-19, PLATFORM-20). Portable, so the version gates are tested on every OS; the calls
//! themselves are in `windows::dwm` (unverified on Windows).
//!
//! - **Title bar theme.** The title bars of the regular windows follow the Windows app mode
//!   unless asked otherwise, so a dark Snipdeck under a light Windows got a light title bar. The
//!   attribute is `DWMWA_USE_IMMERSIVE_DARK_MODE`: number 20 from build 18985, number 19 on the
//!   older Windows 10 builds (1809 to 1909) that only know the undocumented first value.
//! - **Corners.** Windows 11 (build 22000 and up) rounds top-level windows by itself, but only
//!   when it chooses to; the frameless Panel, Picker, fill form and toast ask for
//!   `DWMWCP_ROUND` explicitly so their edges do not depend on that choice. Windows 10 draws
//!   square corners and knows no such attribute: nothing is set there.

/// The first Windows 10 build that has `DWMWA_USE_IMMERSIVE_DARK_MODE` as attribute 19 (1809).
pub const FIRST_DARK_TITLE_19_BUILD: u32 = 17763;
/// The first build where the dark title bar attribute is number 20 (Windows 10 20H1 preview).
pub const FIRST_DARK_TITLE_20_BUILD: u32 = 18985;
/// The first Windows 11 build (21H2): the first with rounded corners and
/// `DWMWA_WINDOW_CORNER_PREFERENCE`.
pub const FIRST_WINDOWS_11_BUILD: u32 = 22000;

/// `DWMWA_USE_IMMERSIVE_DARK_MODE` before build 18985.
pub const ATTRIBUTE_DARK_TITLE_OLD: u32 = 19;
/// `DWMWA_USE_IMMERSIVE_DARK_MODE` from build 18985.
pub const ATTRIBUTE_DARK_TITLE: u32 = 20;
/// `DWMWA_WINDOW_CORNER_PREFERENCE`.
pub const ATTRIBUTE_CORNER_PREFERENCE: u32 = 33;
/// `DWMWCP_ROUND`.
pub const CORNER_ROUND: u32 = 2;

/// A DWM attribute number and the `DWORD` to set it to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DwmSetting {
    /// The `DWMWINDOWATTRIBUTE` number.
    pub attribute: u32,
    /// The value (a `BOOL` or an enum, both four bytes).
    pub value: u32,
}

/// The title bar setting for an app theme on Windows `build`: dark or light (`dark`). None on a
/// Windows build too old to have the attribute (the title bar then stays as Windows draws it).
#[must_use]
pub fn dark_title_bar(build: u32, dark: bool) -> Option<DwmSetting> {
    let attribute = if build >= FIRST_DARK_TITLE_20_BUILD {
        ATTRIBUTE_DARK_TITLE
    } else if build >= FIRST_DARK_TITLE_19_BUILD {
        ATTRIBUTE_DARK_TITLE_OLD
    } else {
        return None;
    };
    Some(DwmSetting {
        attribute,
        value: u32::from(dark),
    })
}

/// The corner setting for the frameless windows on Windows `build`: round on Windows 11, none
/// on Windows 10 (which has no such attribute and square corners).
#[must_use]
pub fn round_corners(build: u32) -> Option<DwmSetting> {
    (build >= FIRST_WINDOWS_11_BUILD).then_some(DwmSetting {
        attribute: ATTRIBUTE_CORNER_PREFERENCE,
        value: CORNER_ROUND,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_title_uses_attribute_20_on_current_builds() {
        let on = dark_title_bar(26300, true).unwrap();
        assert_eq!((on.attribute, on.value), (20, 1));
        let off = dark_title_bar(19045, false).unwrap();
        assert_eq!((off.attribute, off.value), (20, 0));
    }

    #[test]
    fn dark_title_uses_attribute_19_on_old_windows_10() {
        for build in [17763, 18362, 18363, 18984] {
            assert_eq!(
                dark_title_bar(build, true).unwrap().attribute,
                19,
                "{build}"
            );
        }
        assert_eq!(dark_title_bar(18985, true).unwrap().attribute, 20);
    }

    #[test]
    fn dark_title_is_left_alone_before_windows_10_1809() {
        assert_eq!(dark_title_bar(17134, true), None);
        assert_eq!(dark_title_bar(0, false), None);
    }

    #[test]
    fn corners_are_round_from_windows_11_only() {
        assert_eq!(round_corners(21999), None);
        assert_eq!(round_corners(19045), None);
        let setting = round_corners(22000).unwrap();
        assert_eq!((setting.attribute, setting.value), (33, 2));
        assert!(round_corners(26300).is_some());
    }
}
