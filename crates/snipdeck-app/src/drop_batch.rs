//! Files dropped on a window arrive as one event each. This gathers the events of one drop, so
//! several files become one history step, and hands them over once the events stop.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    time::Duration,
};

/// How long after the last file event the drop counts as complete.
const SETTLE: Duration = Duration::from_millis(60);

/// The paths of one drop, in the order they came.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Pending(Vec<String>);

impl Pending {
    /// A file of the drop arrived.
    pub fn push(&mut self, path: String) {
        self.0.push(path);
    }

    /// Everything gathered so far; the batch is empty after.
    pub fn take(&mut self) -> Vec<String> {
        std::mem::take(&mut self.0)
    }

    /// `paths` could not be handed over yet: they go back in front of anything that came since.
    pub fn put_back(&mut self, mut paths: Vec<String>) {
        paths.append(&mut self.0);
        self.0 = paths;
    }
}

/// What a flush callback does with a drop: `true` when it took it, `false` to be asked again
/// shortly (the controller was busy).
type Flush = Rc<dyn Fn(&[String]) -> bool>;

struct Inner {
    pending: RefCell<Pending>,
    timer: slint::Timer,
}

/// The paths of one drop, collected until the events stop.
#[derive(Clone)]
pub struct DropBatch(Rc<Inner>);

impl Default for DropBatch {
    fn default() -> Self {
        Self(Rc::new(Inner {
            pending: RefCell::default(),
            timer: slint::Timer::default(),
        }))
    }
}

impl DropBatch {
    /// A file of the drop arrived; once no more follow, `flush` gets all the paths in the order
    /// they came. If `flush` returns `false` the paths are kept and offered again later.
    pub fn push(&self, path: String, flush: impl Fn(&[String]) -> bool + 'static) {
        self.0.pending.borrow_mut().push(path);
        arm(&self.0, Rc::new(flush));
    }
}

fn arm(inner: &Rc<Inner>, flush: Flush) {
    let weak: Weak<Inner> = Rc::downgrade(inner);
    inner
        .timer
        .start(slint::TimerMode::SingleShot, SETTLE, move || {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let paths = inner.pending.borrow_mut().take();
            if paths.is_empty() || flush(&paths) {
                return;
            }
            inner.pending.borrow_mut().put_back(paths);
            arm(&inner, Rc::clone(&flush));
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drop_is_taken_whole_and_in_order() {
        let mut pending = Pending::default();
        for p in ["a", "b", "c"] {
            pending.push(p.into());
        }
        assert_eq!(pending.take(), ["a", "b", "c"]);
        assert_eq!(pending.take(), Vec::<String>::new(), "a take empties it");
    }

    #[test]
    fn paths_put_back_come_before_newer_ones() {
        let mut pending = Pending::default();
        pending.push("a".into());
        pending.push("b".into());
        let taken = pending.take();
        pending.push("c".into());
        pending.put_back(taken);
        assert_eq!(pending.take(), ["a", "b", "c"]);
    }
}
