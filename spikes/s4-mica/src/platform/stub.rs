//! Non-Windows stub: no DWM. The spike runs with the solid fallback chrome so the layout,
//! tokens and text scaling can still be previewed (`cargo run` on Linux).

use crate::appearance::{text_scale_from_factor, ChromeMaterial, Config, Rgb, SystemAppearance};
use crate::PanelWindow;

/// Preview knobs for checking the fallback layout off Windows:
/// `S4_PREVIEW_DARK=1`, `S4_PREVIEW_TEXT_SCALE=2.25`.
pub fn read_appearance() -> SystemAppearance {
    let dark = std::env::var("S4_PREVIEW_DARK").is_ok_and(|v| v == "1");
    let text_scale = std::env::var("S4_PREVIEW_TEXT_SCALE")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .map_or(1.0, text_scale_from_factor);
    SystemAppearance {
        apps_use_light_theme: !dark,
        text_scale,
        sources: "non-Windows stub (S4_PREVIEW_* env)".into(),
        ..Default::default()
    }
}

pub fn configure_backend(
    selector: slint::BackendSelector,
    _cfg: Config,
    _initial: ChromeMaterial,
) -> slint::BackendSelector {
    selector
}

pub fn on_window_created(_app: &PanelWindow, _cfg: &Config) {}

pub fn apply_window_material(
    _window: &slint::Window,
    _appearance: &SystemAppearance,
    wanted: ChromeMaterial,
    _cfg: &Config,
    _border: Rgb,
) -> (ChromeMaterial, String) {
    (wanted, "not available on this platform".into())
}
