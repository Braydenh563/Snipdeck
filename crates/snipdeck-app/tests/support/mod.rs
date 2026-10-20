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
pub struct SharedProvider {
    inner: std::sync::Arc<snipdeck_store::crypto::MemoryKeyProvider>,
    /// How many more `unwrap_key` calls work; `None` is no limit. At zero every one fails, as a
    /// key store that went away does.
    unwraps_left: std::sync::Arc<std::sync::Mutex<Option<u32>>>,
}

impl SharedProvider {
    pub fn new() -> Self {
        Self {
            inner: std::sync::Arc::new(
                snipdeck_store::crypto::MemoryKeyProvider::new().expect("provider"),
            ),
            unwraps_left: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// From now on `n` more unwraps work, then every one fails (`None` lifts the limit).
    pub fn fail_unwraps_after(&self, n: Option<u32>) {
        *self.unwraps_left.lock().unwrap() = n;
    }
}

impl snipdeck_store::crypto::KeyProvider for SharedProvider {
    fn wrap_key(&self, key: &[u8]) -> Result<Vec<u8>, snipdeck_store::crypto::KeyProviderError> {
        self.inner.wrap_key(key)
    }

    fn unwrap_key(
        &self,
        wrapped: &[u8],
    ) -> Result<snipdeck_store::crypto::Zeroizing<Vec<u8>>, snipdeck_store::crypto::KeyProviderError>
    {
        let mut limit = self.unwraps_left.lock().unwrap();
        if let Some(left) = *limit {
            if left == 0 {
                return Err(snipdeck_store::crypto::KeyProviderError::Unavailable(
                    "the key store is gone".to_owned(),
                ));
            }
            *limit = Some(left - 1);
        }
        drop(limit);
        self.inner.unwrap_key(wrapped)
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

/// Holds each master password job until the test releases it, so a window can be looked at while
/// the key is being derived. Installing it replaces the thread's offload; dropping the guard puts
/// the inline one back (the UI thread is shared by the tests of a file).
#[derive(Clone, Default)]
pub struct Deferred {
    held: std::rc::Rc<
        std::cell::RefCell<
            Vec<(
                snipdeck_app::master_job::MasterJob,
                snipdeck_app::master_job::Finish,
            )>,
        >,
    >,
}

impl snipdeck_app::master_job::Offload for Deferred {
    fn run(
        &self,
        job: snipdeck_app::master_job::MasterJob,
        finish: snipdeck_app::master_job::Finish,
    ) {
        self.held.borrow_mut().push((job, finish));
    }
}

/// Puts the inline offload back when dropped.
pub struct DeferredGuard;

impl Drop for DeferredGuard {
    fn drop(&mut self) {
        snipdeck_app::master_job::set_offload(std::rc::Rc::new(snipdeck_app::master_job::Inline));
    }
}

impl Deferred {
    /// Makes the windows of this thread hand their jobs to `self`.
    pub fn install(&self) -> DeferredGuard {
        snipdeck_app::master_job::set_offload(std::rc::Rc::new(self.clone()));
        DeferredGuard
    }

    /// How many jobs are waiting.
    pub fn held(&self) -> usize {
        self.held.borrow().len()
    }

    /// Runs the oldest waiting job and hands its outcome to the window, as the worker would.
    pub fn release(&self) {
        let (job, finish) = self.held.borrow_mut().remove(0);
        finish(job.run());
    }
}

/// A test password derived at run time from `label`: the same label always gives the same text, and
/// no password is written down in the test source. Leaks a few bytes per call; tests only.
pub fn pw(label: &str) -> &'static str {
    let key = blake3::derive_key("snipdeck test password", label.as_bytes());
    let hex = blake3::Hash::from_bytes(key).to_hex();
    Box::leak(hex.as_str()[..24].to_owned().into_boxed_str())
}

/// The published development licence seed (public by design; `docs/licensing.md`), assembled at
/// run time rather than written as a byte literal.
pub fn dev_seed() -> [u8; 32] {
    let text = ["snipdeck", "development", "licence", "key"].join(" ");
    text.as_bytes().try_into().unwrap()
}

/// A throwaway signing seed computed from `tag`, unrelated to the development key.
pub fn derived_seed(tag: u8) -> [u8; 32] {
    std::array::from_fn(|i| u8::try_from(i).unwrap().wrapping_mul(31).wrapping_add(tag))
}
