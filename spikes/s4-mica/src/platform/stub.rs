//! Non-Windows stub: no DWM. The spike runs with the solid fallback chrome so the layout,
//! tokens and text scaling can still be previewed (`cargo run` on Linux).

use crate::appearance::{ChromeMaterial, Config, Rgb, SystemAppearance};
use crate::PanelWindow;

pub fn read_appearance() -> SystemAppearance {
    SystemAppearance {
        sources: "non-Windows stub (defaults)".into(),
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
