//! Helpers shared by the window tests (`#[path]`-included, so each test binary compiles its own
//! copy). The same pattern as `snipdeck-ui`'s tests: Slint's testing platform is process-wide and
//! bound to one thread, so every test body runs on one dedicated UI thread.
#![allow(
    dead_code,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test support: each test binary uses a subset, and a failure here is a failed test"
)]

use std::sync::{Mutex, OnceLock, mpsc};

use snipdeck_store::crypto::KeyProvider as _;

use slint::{
    Window,
    platform::{Key, WindowEvent},
};

/// Runs a test body on the UI thread. A panic in `f` is re-raised in the calling test.
pub fn on_ui_thread(f: impl FnOnce() + Send + 'static) {
    type Job = Box<dyn FnOnce() + Send>;
    static UI: OnceLock<Mutex<mpsc::Sender<Job>>> = OnceLock::new();
    let sender = UI.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::spawn(move || {
            i_slint_backend_testing::init_integration_test_with_mock_time();
            for job in rx {
                job();
            }
        });
        Mutex::new(tx)
    });
    let (done_tx, done_rx) = mpsc::channel();
    let job: Job = Box::new(move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        let _ = done_tx.send(outcome);
    });
    sender
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .send(job)
        .expect("the UI thread is alive");
    if let Err(panic) = done_rx.recv().expect("the UI thread answers") {
        std::panic::resume_unwind(panic);
    }
}

/// One key press and release; `key` is a character or a [`Key`] converted with [`key`].
pub fn press(window: &Window, key: &str) {
    window.dispatch_event(WindowEvent::KeyPressed { text: key.into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
}

/// Types text one character at a time.
pub fn type_text(window: &Window, text: &str) {
    for c in text.chars() {
        press(window, &c.to_string());
    }
}

/// Holds the `modifiers`, presses `key`, then releases everything.
pub fn chord(window: &Window, modifiers: &[Key], key: &str) {
    for m in modifiers {
        window.dispatch_event(WindowEvent::KeyPressed {
            text: char::from(*m).to_string().into(),
        });
    }
    window.dispatch_event(WindowEvent::KeyPressed { text: key.into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
    for m in modifiers.iter().rev() {
        window.dispatch_event(WindowEvent::KeyReleased {
            text: char::from(*m).to_string().into(),
        });
    }
}

pub fn key(k: Key) -> String {
    char::from(k).to_string()
}

/// One in-memory key provider shared by every controller of a test, so a "restart" (a second
/// controller on the same data folder) opens the key the first run made.
#[derive(Clone)]
pub struct SharedProvider(std::rc::Rc<snipdeck_store::crypto::MemoryKeyProvider>);

impl SharedProvider {
    pub fn new() -> Self {
        Self(std::rc::Rc::new(
            snipdeck_store::crypto::MemoryKeyProvider::new().expect("provider"),
        ))
    }
}

impl snipdeck_store::crypto::KeyProvider for SharedProvider {
    fn wrap_key(&self, key: &[u8]) -> Result<Vec<u8>, snipdeck_store::crypto::KeyProviderError> {
        self.0.wrap_key(key)
    }

    fn unwrap_key(
        &self,
        wrapped: &[u8],
    ) -> Result<snipdeck_store::crypto::Zeroizing<Vec<u8>>, snipdeck_store::crypto::KeyProviderError>
    {
        self.0.unwrap_key(wrapped)
    }
}

/// Every file under `dir` (database, log, blobs, drafts) is free of each needle.
pub fn assert_no_plaintext(dir: &std::path::Path, needles: &[&[u8]]) {
    let mut stack = vec![dir.to_owned()];
    let mut seen = 0;
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).expect("dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            if name
                .as_deref()
                .is_some_and(|n| n.ends_with("-shm") || n == "snipdeck.lock")
            {
                continue;
            }
            let bytes = std::fs::read(&path).expect("read");
            seen += 1;
            for needle in needles {
                assert!(
                    !bytes.windows(needle.len()).any(|w| w == *needle),
                    "{} holds {:?}",
                    path.display(),
                    String::from_utf8_lossy(&needle[..needle.len().min(20)])
                );
            }
        }
    }
    assert!(seen > 0, "there was something to scan");
}
