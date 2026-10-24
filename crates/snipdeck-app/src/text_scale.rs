//! The text size in force (WCAG 1.4.4): the Settings > Appearance choice over Windows' own
//! "Text size". It is one number for the whole process, kept here because every window has its
//! own `SystemLook` global to set and the windows are redrawn from different places. Whoever
//! learns of a change ([`configure`] for the setting, [`set_system`] for Windows) calls it; the
//! next redraw of each window runs [`apply`].
//!
//! UI thread only: the state is thread-local.

use std::cell::Cell;

use snipdeck_ui as ui;

use crate::settings::TextSize;

#[derive(Clone, Copy)]
struct State {
    setting: TextSize,
    /// Windows' factor, `1.0 ..= 2.25`.
    system: f32,
}

thread_local! {
    static STATE: Cell<State> = const {
        Cell::new(State { setting: TextSize::System, system: 1.0 })
    };
}

/// Remembers the Settings > Appearance choice.
pub fn configure(setting: TextSize) {
    STATE.with(|s| s.set(State { setting, ..s.get() }));
}

/// Remembers Windows' text scale (see `SystemLookSource::text_scale`).
pub fn set_system(factor: f32) {
    STATE.with(|s| {
        s.set(State {
            system: factor,
            ..s.get()
        });
    });
}

/// The factor the UI is laid out with now, `1.0 ..= 2.25`.
#[must_use]
pub fn current() -> f32 {
    let State { setting, system } = STATE.with(Cell::get);
    setting.scale(system)
}

/// Gives a window's `SystemLook` global the text scale in force. Cheap; does nothing when the
/// window already has it, so it can run on every redraw.
pub fn apply(look: &ui::SystemLook<'_>) {
    let wanted = current();
    if (look.get_text_scale() - wanted).abs() > f32::EPSILON {
        look.set_text_scale(wanted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setting_wins_over_windows_and_system_follows_windows() {
        configure(TextSize::System);
        set_system(1.5);
        assert!((current() - 1.5).abs() < 1e-6);
        configure(TextSize::P125);
        assert!(
            (current() - 1.25).abs() < 1e-6,
            "a fixed size ignores Windows"
        );
        configure(TextSize::System);
        set_system(7.0);
        assert!(
            (current() - 2.25).abs() < 1e-6,
            "held to the supported range"
        );
        set_system(1.0);
    }
}
