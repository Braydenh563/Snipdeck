//! The welcome tour's half of the [`Controller`] (V1-PLAN 7a): which step the tour is on, Back
//! and Next, and what finishing or skipping records (`Settings::onboarded`). Slint-free like the
//! rest of the controller: `onboarding_binding` calls these from the window's callbacks. The
//! choices the steps offer (shortcuts, dock, start-up) go through the Settings code paths.

use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};

use super::{Controller, Effect, Library};
use crate::{host::ClipboardText, tour::TourStep};

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// Whether the tour opens by itself now: it has not been seen and the library holds no item
    /// (see `Settings::tour_due`). The host asks once at start-up.
    #[must_use]
    pub fn tour_due(&self) -> bool {
        let empty = self.store.all_items().is_ok_and(|items| items.is_empty());
        self.settings.tour_due(empty)
    }

    /// Starts the tour at its first step, for Help's "Show the welcome tour again" and for the
    /// first run. The window is opened by
    /// [`crate::onboarding_binding::request_open`].
    pub fn show_welcome_tour(&mut self) {
        self.session.tour = Some(TourStep::Welcome);
    }

    /// The step the tour is on; `None` when it is not running.
    #[must_use]
    pub fn tour_step(&self) -> Option<TourStep> {
        self.session.tour
    }

    /// Next: the following step, or, on the last, finishes the tour.
    pub fn tour_next(&mut self) {
        match self.session.tour.map(TourStep::next) {
            Some(Some(step)) => self.session.tour = Some(step),
            Some(None) => self.finish_tour(),
            None => {}
        }
    }

    /// Back: the previous step; the first step stays.
    pub fn tour_back(&mut self) {
        if let Some(step) = self.session.tour.and_then(TourStep::back) {
            self.session.tour = Some(step);
        }
    }

    /// Skip (also Esc and closing the window): ends the tour from any step. Whatever the earlier
    /// steps changed stays, and the tour does not open by itself again.
    pub fn tour_skip(&mut self) {
        self.finish_tour();
    }

    fn finish_tour(&mut self) {
        self.session.tour = None;
        self.update_settings(|s| s.onboarded = true);
    }
}
