//! OS-free model of the system appearance and the decisions the spike makes from it.
//! Kept free of Win32 types so the policy is unit-tested on any host.

/// First build where `DWMWA_USE_IMMERSIVE_DARK_MODE`, `DWMWA_WINDOW_CORNER_PREFERENCE` and
/// `DWMWA_BORDER_COLOR` are documented (Windows 11 21H2).
pub const MIN_BUILD_WIN11: u32 = 22000;
/// First build where `DWMWA_SYSTEMBACKDROP_TYPE` is documented (Windows 11 22H2).
pub const MIN_BUILD_SYSTEM_BACKDROP: u32 = 22621;

/// What the OS reports. Every field has a safe default for when a query fails.
#[derive(Clone, Debug, PartialEq)]
pub struct SystemAppearance {
    /// `RtlGetVersion` build number; `None` off Windows or if the call failed.
    pub os_build: Option<u32>,
    /// `HKCU\...\Themes\Personalize\AppsUseLightTheme` (missing value = light).
    pub apps_use_light_theme: bool,
    /// "Transparency effects": `UISettings.AdvancedEffectsEnabled`, else `EnableTransparency`.
    pub transparency_effects: bool,
    /// `SPI_GETHIGHCONTRAST` / `HCF_HIGHCONTRASTON`.
    pub high_contrast: bool,
    /// Windows "Text size", 1.0 ..= 2.25.
    pub text_scale: f32,
    /// System colours to use while `high_contrast` is on.
    pub hc_colors: Option<HighContrastColors>,
    /// Free-form notes from the platform layer (which source each value came from).
    pub sources: String,
}

impl Default for SystemAppearance {
    fn default() -> Self {
        Self {
            os_build: None,
            apps_use_light_theme: true,
            transparency_effects: true,
            high_contrast: false,
            text_scale: 1.0,
            hc_colors: None,
            sources: String::new(),
        }
    }
}

pub type Rgb = (u8, u8, u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HighContrastColors {
    pub window: Rgb,
    pub window_text: Rgb,
    pub highlight: Rgb,
    pub highlight_text: Rgb,
    pub gray_text: Rgb,
}

impl SystemAppearance {
    /// Dark tokens follow the app theme, but high contrast always wins (it brings its own colours).
    pub fn use_dark_tokens(&self) -> bool {
        !self.apps_use_light_theme && !self.high_contrast
    }

    pub fn is_win11(&self) -> bool {
        self.os_build.is_some_and(|b| b >= MIN_BUILD_WIN11)
    }
}

/// Which DWM system backdrop to request for the chrome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackdropKind {
    /// `DWMSBT_MAINWINDOW` (Mica) - what DESIGN.md specifies.
    Mica,
    /// `DWMSBT_TABBEDWINDOW` (Mica Alt).
    MicaAlt,
    /// `DWMSBT_TRANSIENTWINDOW` (Desktop Acrylic) - the Picker candidate.
    Acrylic,
}

/// How far to extend the DWM frame into the client area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtendFrame {
    /// Margins -1: "sheet of glass", backdrop may show behind the whole client area.
    Full,
    /// 1px top margin: the classic frameless-shadow trick, no backdrop in the client area.
    OnePixel,
    /// Margins 0.
    None,
}

/// Spike knobs, read from `S4_*` environment variables so the owner can run a test matrix
/// without rebuilding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub backdrop: BackdropKind,
    pub extend_frame: ExtendFrame,
    /// winit `with_undecorated_shadow` (1px non-client strip so DWM keeps the shadow).
    pub undecorated_shadow: bool,
    /// Add `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` like the real panel (ADR-0003).
    pub no_activate: bool,
    /// Experimental: keep the frame in its "active" state so Mica keeps its wallpaper tint.
    pub keep_active_look: bool,
    /// Paint the solid fallback even where Mica is available.
    pub force_solid: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            backdrop: BackdropKind::Mica,
            extend_frame: ExtendFrame::Full,
            undecorated_shadow: true,
            no_activate: false,
            keep_active_look: false,
            force_solid: false,
        }
    }
}

fn flag(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

impl Config {
    /// Builds a config from `(name, value)` pairs; unknown names and bad values are ignored.
    pub fn from_vars<I, K, V>(vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let mut c = Self::default();
        for (k, v) in vars {
            let v = v.as_ref();
            match k.as_ref() {
                "S4_BACKDROP" => match v.trim().to_ascii_lowercase().as_str() {
                    "mica" => c.backdrop = BackdropKind::Mica,
                    "mica-alt" | "tabbed" => c.backdrop = BackdropKind::MicaAlt,
                    "acrylic" | "transient" => c.backdrop = BackdropKind::Acrylic,
                    "none" | "solid" => c.force_solid = true,
                    _ => {}
                },
                "S4_EXTEND" => match v.trim().to_ascii_lowercase().as_str() {
                    "full" | "-1" => c.extend_frame = ExtendFrame::Full,
                    "1px" | "1" => c.extend_frame = ExtendFrame::OnePixel,
                    "none" | "0" => c.extend_frame = ExtendFrame::None,
                    _ => {}
                },
                "S4_SHADOW" => c.undecorated_shadow = flag(v).unwrap_or(c.undecorated_shadow),
                "S4_NOACTIVATE" => c.no_activate = flag(v).unwrap_or(c.no_activate),
                "S4_KEEP_ACTIVE_LOOK" => c.keep_active_look = flag(v).unwrap_or(c.keep_active_look),
                "S4_FORCE_SOLID" => c.force_solid = flag(v).unwrap_or(c.force_solid),
                _ => {}
            }
        }
        c
    }

    pub fn from_env() -> Self {
        Self::from_vars(std::env::vars())
    }
}

/// Why the chrome is painted solid instead of showing the DWM backdrop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolidReason {
    /// Not Windows, Windows 10, or Windows 11 before 22H2.
    UnsupportedOs,
    TransparencyOff,
    HighContrast,
    ForcedByConfig,
    /// `DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE)` returned an error.
    #[cfg_attr(not(windows), allow(dead_code))]
    BackdropCallFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChromeMaterial {
    Backdrop(BackdropKind),
    Solid(SolidReason),
}

/// DESIGN.md §5 Panel: "Falls back to solid `bg.app` on Windows 10, when Transparency effects
/// is off, or in high-contrast mode." High contrast is checked first because it also changes
/// the colours, not only the material.
pub fn choose_material(a: &SystemAppearance, cfg: &Config) -> ChromeMaterial {
    if a.high_contrast {
        ChromeMaterial::Solid(SolidReason::HighContrast)
    } else if !a.os_build.is_some_and(|b| b >= MIN_BUILD_SYSTEM_BACKDROP) {
        ChromeMaterial::Solid(SolidReason::UnsupportedOs)
    } else if !a.transparency_effects {
        ChromeMaterial::Solid(SolidReason::TransparencyOff)
    } else if cfg.force_solid {
        ChromeMaterial::Solid(SolidReason::ForcedByConfig)
    } else {
        ChromeMaterial::Backdrop(cfg.backdrop)
    }
}

/// `TextScaleFactor` registry DWORD (percent) to a factor, clamped to the documented 100..=225.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn text_scale_from_percent(percent: u32) -> f32 {
    percent.clamp(100, 225) as f32 / 100.0
}

/// `UISettings.TextScaleFactor` (documented range 1.0..=2.25) to a sanitised factor.
pub fn text_scale_from_factor(factor: f64) -> f32 {
    if factor.is_finite() {
        factor.clamp(1.0, 2.25) as f32
    } else {
        1.0
    }
}

/// `COLORREF` is 0x00BBGGRR.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn colorref_from_rgb((r, g, b): Rgb) -> u32 {
    u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn rgb_from_colorref(c: u32) -> Rgb {
    (
        (c & 0xFF) as u8,
        ((c >> 8) & 0xFF) as u8,
        ((c >> 16) & 0xFF) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win11_22h2() -> SystemAppearance {
        SystemAppearance {
            os_build: Some(22631),
            ..Default::default()
        }
    }

    #[test]
    fn mica_on_supported_build() {
        assert_eq!(
            choose_material(&win11_22h2(), &Config::default()),
            ChromeMaterial::Backdrop(BackdropKind::Mica)
        );
    }

    #[test]
    fn solid_on_windows_10_and_win11_21h2() {
        for build in [19045, 22000] {
            let a = SystemAppearance {
                os_build: Some(build),
                ..Default::default()
            };
            assert_eq!(
                choose_material(&a, &Config::default()),
                ChromeMaterial::Solid(SolidReason::UnsupportedOs),
                "build {build}"
            );
        }
        assert_eq!(
            choose_material(&SystemAppearance::default(), &Config::default()),
            ChromeMaterial::Solid(SolidReason::UnsupportedOs)
        );
    }

    #[test]
    fn solid_when_transparency_off() {
        let a = SystemAppearance {
            transparency_effects: false,
            ..win11_22h2()
        };
        assert_eq!(
            choose_material(&a, &Config::default()),
            ChromeMaterial::Solid(SolidReason::TransparencyOff)
        );
    }

    #[test]
    fn high_contrast_wins_and_forces_light_tokens() {
        let a = SystemAppearance {
            high_contrast: true,
            apps_use_light_theme: false,
            transparency_effects: false,
            ..win11_22h2()
        };
        assert_eq!(
            choose_material(&a, &Config::default()),
            ChromeMaterial::Solid(SolidReason::HighContrast)
        );
        assert!(!a.use_dark_tokens());
    }

    #[test]
    fn dark_tokens_follow_apps_theme() {
        let a = SystemAppearance {
            apps_use_light_theme: false,
            ..win11_22h2()
        };
        assert!(a.use_dark_tokens());
        assert!(!win11_22h2().use_dark_tokens());
    }

    #[test]
    fn config_from_vars() {
        let c = Config::from_vars([
            ("S4_BACKDROP", "acrylic"),
            ("S4_EXTEND", "1px"),
            ("S4_SHADOW", "0"),
            ("S4_NOACTIVATE", "yes"),
            ("S4_KEEP_ACTIVE_LOOK", "bogus"),
            ("UNRELATED", "1"),
        ]);
        assert_eq!(c.backdrop, BackdropKind::Acrylic);
        assert_eq!(c.extend_frame, ExtendFrame::OnePixel);
        assert!(!c.undecorated_shadow);
        assert!(c.no_activate);
        assert!(!c.keep_active_look);
        assert!(Config::from_vars([("S4_BACKDROP", "none")]).force_solid);
        assert_eq!(
            choose_material(&win11_22h2(), &Config::from_vars([("S4_FORCE_SOLID", "1")])),
            ChromeMaterial::Solid(SolidReason::ForcedByConfig)
        );
    }

    #[test]
    fn text_scale_is_clamped() {
        assert_eq!(text_scale_from_percent(0), 1.0);
        assert_eq!(text_scale_from_percent(150), 1.5);
        assert_eq!(text_scale_from_percent(400), 2.25);
        assert_eq!(text_scale_from_factor(f64::NAN), 1.0);
        assert_eq!(text_scale_from_factor(1.25), 1.25);
        assert_eq!(text_scale_from_factor(9.0), 2.25);
    }

    #[test]
    fn colorref_round_trip() {
        assert_eq!(colorref_from_rgb((0x12, 0x34, 0x56)), 0x0056_3412);
        assert_eq!(rgb_from_colorref(0x0056_3412), (0x12, 0x34, 0x56));
    }
}
