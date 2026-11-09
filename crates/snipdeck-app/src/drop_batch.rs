//! Files dropped on a window arrive as one event each. This gathers the events of one drop, so
//! several files become one history step.

use std::{cell::RefCell, rc::Rc, time::Duration};

/// How long after the last file event the drop counts as complete.
const SETTLE: Duration = Duration::from_millis(60);

/// The paths of one drop, collected until the events stop.
#[derive(Default)]
pub struct DropBatch {
    paths: Rc<RefCell<Vec<String>>>,
    timer: slint::Timer,
}

impl DropBatch {
    /// A file of the drop arrived; once no more follow, `flush` gets all the paths in the order
    /// they came.
    pub fn push(&self, path: String, flush: impl Fn(Vec<String>) + 'static) {
        self.paths.borrow_mut().push(path);
        let paths = Rc::clone(&self.paths);
        self.timer
            .start(slint::TimerMode::SingleShot, SETTLE, move || {
                let all = std::mem::take(&mut *paths.borrow_mut());
                if !all.is_empty() {
                    flush(all);
                }
            });
    }
}
