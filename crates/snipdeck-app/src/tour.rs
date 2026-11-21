//! The steps of the welcome tour (V1-PLAN 7a): a plain list with a first and a last, so the
//! controller's navigation is tested without a window.

/// One step of the tour, in the order it is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TourStep {
    /// What the Panel, the Picker and the shortcuts do.
    #[default]
    Welcome,
    /// The four global shortcuts, with their recorders.
    Shortcuts,
    /// Dock edge, "hide when it loses focus" and the taskbar button.
    Panel,
    /// Start with Windows and keep running in the tray.
    Startup,
}

impl TourStep {
    /// Every step, first to last.
    pub const ALL: [Self; 4] = [Self::Welcome, Self::Shortcuts, Self::Panel, Self::Startup];

    /// Where the step is in [`ALL`](Self::ALL), from 0.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Welcome => 0,
            Self::Shortcuts => 1,
            Self::Panel => 2,
            Self::Startup => 3,
        }
    }

    /// The step after this one; `None` after the last.
    #[must_use]
    pub fn next(self) -> Option<Self> {
        Self::ALL.get(self.index() + 1).copied()
    }

    /// The step before this one; `None` before the first.
    #[must_use]
    pub fn back(self) -> Option<Self> {
        self.index()
            .checked_sub(1)
            .and_then(|at| Self::ALL.get(at))
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_steps_run_in_order_with_no_step_before_the_first_or_after_the_last() {
        assert_eq!(TourStep::default(), TourStep::Welcome);
        assert_eq!(TourStep::Welcome.back(), None);
        assert_eq!(TourStep::Welcome.next(), Some(TourStep::Shortcuts));
        assert_eq!(TourStep::Panel.back(), Some(TourStep::Shortcuts));
        assert_eq!(TourStep::Panel.next(), Some(TourStep::Startup));
        assert_eq!(TourStep::Startup.next(), None);
        for (at, step) in TourStep::ALL.into_iter().enumerate() {
            assert_eq!(step.index(), at);
        }
    }
}
