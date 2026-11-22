//! Handing a worker thread's answer back to the UI thread.
//!
//! The file dialogs and the packaged startup task answer from threads of their own (so the window
//! does not freeze while they wait), but what should happen next is a closure that holds the
//! UI's `Rc`s and cannot cross threads. [`back_on_ui`] keeps that closure on the UI thread and
//! gives the worker a `Send` closure that carries only the answer.

use std::{
    any::Any,
    cell::RefCell,
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
    thread::ThreadId,
};

type Pending = HashMap<u64, Box<dyn Any>>;

thread_local! {
    /// The closures waiting for their answer, on the UI thread.
    static PENDING: RefCell<Pending> = RefCell::new(HashMap::new());
}

static NEXT: AtomicU64 = AtomicU64::new(1);

/// Wraps `next` (which stays on the calling thread, the UI thread) in a closure a worker can
/// call with the answer. Called on the UI thread itself (a fake that answers at once) it runs
/// `next` there and then; from another thread it goes through
/// [`slint::invoke_from_event_loop`], and is dropped unrun if the event loop is gone.
pub fn back_on_ui<T: Send + 'static>(
    next: impl FnOnce(T) + 'static,
) -> impl FnOnce(T) + Send + 'static {
    let origin: ThreadId = std::thread::current().id();
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let next: Box<dyn FnOnce(T)> = Box::new(next);
    PENDING.with(|pending| pending.borrow_mut().insert(id, Box::new(next)));
    move |answer: T| {
        if std::thread::current().id() == origin {
            run(id, answer);
        } else {
            let _ = slint::invoke_from_event_loop(move || run(id, answer));
        }
    }
}

fn run<T: 'static>(id: u64, answer: T) {
    let waiting = PENDING.with(|pending| pending.borrow_mut().remove(&id));
    if let Some(next) = waiting.and_then(|any| any.downcast::<Box<dyn FnOnce(T)>>().ok()) {
        next(answer);
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::*;

    #[test]
    fn an_answer_on_the_calling_thread_runs_at_once() {
        let seen = Rc::new(Cell::new(0));
        let sink = Rc::clone(&seen);
        let hand_back = back_on_ui(move |n: i32| sink.set(n));
        hand_back(7);
        assert_eq!(seen.get(), 7);
    }

    #[test]
    fn an_answer_from_a_worker_runs_on_the_ui_thread_in_the_event_loop() {
        i_slint_backend_testing::init_integration_test_with_system_time();
        let seen = Rc::new(RefCell::new(None));
        let sink = Rc::clone(&seen);
        let ui = std::thread::current().id();
        let hand_back = back_on_ui(move |n: i32| {
            *sink.borrow_mut() = Some((n, std::thread::current().id()));
            let _ = slint::quit_event_loop();
        });
        std::thread::spawn(move || hand_back(41));
        slint::run_event_loop().expect("event loop");
        assert_eq!(*seen.borrow(), Some((41, ui)));
    }
}
