//! Which window opened which, so that closing a window closes what it opened (O53).
//!
//! A secondary window (Settings, Troubleshoot, an editor, the Tag manager) belongs to the window
//! the user opened it from. Closing Main closes everything opened from Main; closing the Panel
//! closes what was opened from the Panel and leaves Main's windows alone. [`Registry`] is the
//! bookkeeping and has no windowing in it; the functions below it connect it to the host's
//! windows: each window registers a closer once, says [`opened`] when it is shown, and the
//! owner's close path calls [`owner_closed`].

use std::{cell::RefCell, rc::Rc};

/// Who opened whom. `W` identifies a window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registry<W> {
    /// `(child, opener)` pairs, in the order the children were opened.
    owned: Vec<(W, W)>,
}

impl<W> Default for Registry<W> {
    fn default() -> Self {
        Self { owned: Vec::new() }
    }
}

impl<W: Copy + Eq> Registry<W> {
    /// An empty registry.
    #[must_use]
    pub const fn new() -> Self {
        Self { owned: Vec::new() }
    }

    /// `child` was opened from `opener`. A child opened again belongs to its new opener; a window
    /// cannot own itself.
    pub fn open(&mut self, child: W, opener: W) {
        self.owned.retain(|(c, _)| *c != child);
        if child != opener {
            self.owned.push((child, opener));
        }
    }

    /// `child` went away by itself: it is nobody's any more.
    pub fn forget(&mut self, child: W) {
        self.owned.retain(|(c, _)| *c != child);
    }

    /// `opener` is closing: the windows to close with it (what it opened, and what those opened),
    /// removed from the registry. The opener's own entry stays.
    pub fn close(&mut self, opener: W) -> Vec<W> {
        let mut out = Vec::new();
        let mut queue = vec![opener];
        while let Some(parent) = queue.pop() {
            let mut i = 0;
            while i < self.owned.len() {
                if self.owned[i].1 == parent {
                    let (child, _) = self.owned.remove(i);
                    if child != opener && !out.contains(&child) {
                        out.push(child);
                        queue.push(child);
                    }
                } else {
                    i += 1;
                }
            }
        }
        out
    }
}

/// The windows that can own or be owned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Win {
    /// The Main window.
    Main,
    /// The Panel.
    Panel,
    /// The Settings page.
    Settings,
    /// The Troubleshoot / Guide / About window.
    Help,
    /// The Tag manager.
    TagManager,
    /// The list editor.
    ListEditor,
    /// The board editor.
    BoardEditor,
    /// The item editor.
    ItemEditor,
}

type Closer = Rc<dyn Fn()>;

thread_local! {
    static REGISTRY: RefCell<Registry<Win>> = const { RefCell::new(Registry::new()) };
    static CLOSERS: RefCell<Vec<(Win, Closer)>> = const { RefCell::new(Vec::new()) };
}

/// Tells the registry how to close `win` (without asking: a draft is kept). Replaces an earlier
/// closer of the same window.
pub fn register_closer(win: Win, close: impl Fn() + 'static) {
    CLOSERS.with(|all| {
        let mut all = all.borrow_mut();
        all.retain(|(w, _)| *w != win);
        all.push((win, Rc::new(close)));
    });
}

/// `win` was just shown. It belongs to the surface the user last acted on (see
/// [`crate::native_window::note_opener`]); with none noted it belongs to nobody.
pub fn opened(win: Win) {
    use crate::native_window::{Surface, last_opener};
    let opener = match last_opener() {
        Some(Surface::Main) => Win::Main,
        Some(Surface::Panel) => Win::Panel,
        None => return,
    };
    REGISTRY.with(|r| r.borrow_mut().open(win, opener));
}

/// `owner` was closed: closes every window opened from it. No borrow is held while a closer
/// runs, so a closer may open or close windows itself.
pub fn owner_closed(owner: Win) {
    let children = REGISTRY.with(|r| r.borrow_mut().close(owner));
    for child in children {
        let closer = CLOSERS.with(|all| {
            all.borrow()
                .iter()
                .find(|(w, _)| *w == child)
                .map(|(_, c)| Rc::clone(c))
        });
        if let Some(closer) = closer {
            closer();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum T {
        Main,
        Panel,
        Settings,
        Editor,
        Dialog,
    }

    #[test]
    fn closing_an_opener_returns_what_it_opened() {
        let mut r = Registry::new();
        r.open(T::Settings, T::Main);
        r.open(T::Editor, T::Main);
        assert_eq!(r.close(T::Main), vec![T::Settings, T::Editor]);
    }

    #[test]
    fn closing_main_leaves_the_panels_children() {
        let mut r = Registry::new();
        r.open(T::Settings, T::Main);
        r.open(T::Editor, T::Panel);
        assert_eq!(r.close(T::Main), vec![T::Settings]);
        assert_eq!(r.close(T::Panel), vec![T::Editor]);
    }

    #[test]
    fn what_a_child_opened_closes_with_it() {
        let mut r = Registry::new();
        r.open(T::Editor, T::Main);
        r.open(T::Dialog, T::Editor);
        assert_eq!(r.close(T::Main), vec![T::Editor, T::Dialog]);
        assert_eq!(r.close(T::Editor), Vec::<T>::new());
    }

    #[test]
    fn a_window_is_closed_once() {
        let mut r = Registry::new();
        r.open(T::Settings, T::Main);
        assert_eq!(r.close(T::Main), vec![T::Settings]);
        assert_eq!(r.close(T::Main), Vec::<T>::new());
    }

    #[test]
    fn a_window_opened_again_belongs_to_its_new_opener() {
        let mut r = Registry::new();
        r.open(T::Settings, T::Main);
        r.open(T::Settings, T::Panel);
        assert_eq!(r.close(T::Main), Vec::<T>::new());
        assert_eq!(r.close(T::Panel), vec![T::Settings]);
    }

    #[test]
    fn a_window_that_closed_itself_is_forgotten() {
        let mut r = Registry::new();
        r.open(T::Settings, T::Main);
        r.forget(T::Settings);
        assert_eq!(r.close(T::Main), Vec::<T>::new());
    }

    #[test]
    fn a_window_cannot_own_itself_and_a_cycle_ends() {
        let mut r = Registry::new();
        r.open(T::Main, T::Main);
        assert_eq!(r.close(T::Main), Vec::<T>::new());
        r.open(T::Editor, T::Dialog);
        r.open(T::Dialog, T::Editor);
        assert_eq!(r.close(T::Editor), vec![T::Dialog]);
    }

    #[test]
    fn owner_closed_runs_the_closers_of_the_children_only() {
        use std::cell::Cell;
        thread_local! { static LOG: Cell<u8> = const { Cell::new(0) }; }
        register_closer(Win::Settings, || LOG.with(|l| l.set(l.get() | 1)));
        register_closer(Win::Help, || LOG.with(|l| l.set(l.get() | 2)));
        REGISTRY.with(|r| {
            let mut r = r.borrow_mut();
            r.open(Win::Settings, Win::Main);
            r.open(Win::Help, Win::Panel);
        });
        owner_closed(Win::Main);
        assert_eq!(LOG.with(Cell::get), 1);
        owner_closed(Win::Panel);
        assert_eq!(LOG.with(Cell::get), 3);
    }
}
