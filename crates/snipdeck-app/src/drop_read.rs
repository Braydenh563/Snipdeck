//! Reading the files of a drop off the UI thread. A drop of large files used to be read where the
//! window is drawn, so the Panel froze until the last byte was in. Now the read is a [`ReadJob`]
//! (no controller, no store: it may run on any thread), and the controller stores what came back
//! (`Controller::drop_read_done`, on the UI thread). Slint-free except for the hand-back.
//!
//! The files are read in chunks of about [`CHUNK_BYTES`], in the order they were dropped, each
//! under the same size limits as a file chosen in the editor ([`crate::item_editor::read_source`]); only
//! one chunk is in memory at a time, because the controller stores a chunk before the next is
//! asked for. [`spawn`] says where a job runs: on the calling thread until [`use_threads`] is
//! called, which the app does once at start.

use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::HashMap,
    path::PathBuf,
    rc::Rc,
    time::Duration,
};

use crate::{controller::DropRun, item_editor::Source};

/// About how many bytes one job reads before it hands them over (it always reads at least one
/// file, and finishes the file it is on).
pub const CHUNK_BYTES: u64 = 64 * 1024 * 1024;

/// How long to wait before offering a result again to a controller that was busy.
const RETRY: Duration = Duration::from_millis(50);

/// The slow half of a drop: reading files. `Send`.
#[derive(Debug)]
pub struct ReadJob {
    paths: Vec<PathBuf>,
    budget: u64,
}

/// What a [`ReadJob`] read: one entry per file it got to, in order (`None` for a file that
/// cannot be used: missing, a folder, empty, too large, unreadable), and the files it left for
/// the next job.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReadDone {
    /// The files read, in drop order.
    pub read: Vec<Option<Source>>,
    /// The files not yet read, in drop order.
    pub rest: Vec<PathBuf>,
}

impl ReadJob {
    /// A job for `paths`, reading about [`CHUNK_BYTES`].
    #[must_use]
    pub fn new(paths: Vec<PathBuf>) -> Self {
        Self::with_budget(paths, CHUNK_BYTES)
    }

    /// A job that stops once it has read `budget` bytes (for tests).
    #[must_use]
    pub fn with_budget(paths: Vec<PathBuf>, budget: u64) -> Self {
        Self { paths, budget }
    }

    /// Reads the files, in order, until the budget is used up.
    #[must_use]
    pub fn run(self) -> ReadDone {
        let mut done = ReadDone::default();
        let mut used = 0_u64;
        let mut paths = self.paths.into_iter();
        for path in paths.by_ref() {
            let source = crate::item_editor::read_source(
                snipdeck_core::ItemKind::File,
                &path.to_string_lossy(),
            )
            .ok();
            used = used.saturating_add(
                source
                    .as_ref()
                    .map_or(0, |s| u64::try_from(s.bytes.len()).unwrap_or(u64::MAX)),
            );
            done.read.push(source);
            if used >= self.budget {
                break;
            }
        }
        done.rest = paths.collect();
        done
    }

    /// `run`, but a panic while reading counts every file of the job as unusable, so the drop is
    /// always settled.
    fn run_guarded(self) -> ReadDone {
        let count = self.paths.len();
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.run())).unwrap_or_else(|_| {
            ReadDone {
                read: (0..count).map(|_| None).collect(),
                rest: Vec::new(),
            }
        })
    }
}

/// What to do with a job's result, on the UI thread.
type Finish = Box<dyn FnOnce(ReadDone)>;

thread_local! {
    static THREADS: Cell<bool> = const { Cell::new(false) };
    /// Results waiting for the UI thread, by ticket. Only the UI thread reads this.
    static WAITING: RefCell<HashMap<u64, Finish>> = RefCell::new(HashMap::new());
    static NEXT_TICKET: Cell<u64> = const { Cell::new(0) };
}

/// From now on the jobs of this (UI) thread run on a worker thread. The app calls this once.
pub fn use_threads() {
    THREADS.with(|t| t.set(true));
}

fn deliver(ticket: u64, done: Box<dyn Any + Send>) {
    let finish = WAITING.with(|w| w.borrow_mut().remove(&ticket));
    if let (Some(finish), Ok(done)) = (finish, done.downcast::<ReadDone>()) {
        finish(*done);
    }
}

/// Runs `job` and calls `finish` with its result on the UI thread: straight away, or (after
/// [`use_threads`]) once a worker thread has read the files.
pub fn spawn(job: ReadJob, finish: Finish) {
    if !THREADS.with(Cell::get) {
        finish(job.run_guarded());
        return;
    }
    let ticket = NEXT_TICKET.with(|n| {
        n.set(n.get() + 1);
        n.get()
    });
    let count = job.paths.len();
    WAITING.with(|w| w.borrow_mut().insert(ticket, finish));
    let spawned = std::thread::Builder::new()
        .name("snipdeck-drop".to_owned())
        .spawn(move || {
            let done: Box<dyn Any + Send> = Box::new(job.run_guarded());
            // The event loop being gone means the app is closing; there is nobody to tell.
            let _ = slint::invoke_from_event_loop(move || deliver(ticket, done));
        });
    if spawned.is_err() {
        let failed: Box<dyn Any + Send> = Box::new(ReadDone {
            read: (0..count).map(|_| None).collect(),
            rest: Vec::new(),
        });
        deliver(ticket, failed);
    }
}

/// Offers a result to the controller: `Ok(next)` when it took it (`next` is the drop again if
/// there are more files to read), `Err` with both handed back (boxed: they are large) when the
/// controller was busy.
pub type Apply = Rc<dyn Fn(DropRun, ReadDone) -> Result<Option<DropRun>, Busy>>;

/// A result the controller could not take yet: the drop and what was read, to offer again.
pub type Busy = Box<(DropRun, ReadDone)>;

/// Reads the rest of `run` chunk by chunk, each result applied on the UI thread by `apply`.
pub fn drive(mut run: DropRun, apply: Apply) {
    let job = run.next_job();
    spawn(job, Box::new(move |done| settle(run, done, &apply)));
}

fn settle(run: DropRun, done: ReadDone, apply: &Apply) {
    match apply(run, done) {
        Ok(Some(next)) => drive(next, Rc::clone(apply)),
        Ok(None) => {}
        Err(back) => {
            let (run, done) = *back;
            let apply = Rc::clone(apply);
            slint::Timer::single_shot(RETRY, move || settle(run, done, &apply));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(dir: &tempfile::TempDir, specs: &[(&str, usize)]) -> Vec<PathBuf> {
        specs
            .iter()
            .map(|(name, len)| {
                let path = dir.path().join(name);
                std::fs::write(&path, vec![b'x'; *len]).expect("write");
                path
            })
            .collect()
    }

    #[test]
    fn files_are_read_in_drop_order_and_unusable_ones_are_none() {
        let dir = tempfile::tempdir().expect("dir");
        let mut paths = files(&dir, &[("b.txt", 3), ("a.txt", 5)]);
        paths.insert(1, dir.path().join("missing.txt"));
        paths.push(dir.path().to_path_buf());
        let done = ReadJob::new(paths).run();
        let names: Vec<_> = done
            .read
            .iter()
            .map(|s| s.as_ref().map(|s| s.name.as_str()))
            .collect();
        assert_eq!(names, [Some("b.txt"), None, Some("a.txt"), None]);
        assert_eq!(done.rest, Vec::<std::path::PathBuf>::new());
    }

    #[test]
    fn a_job_stops_at_its_budget_and_leaves_the_rest_in_order() {
        let dir = tempfile::tempdir().expect("dir");
        let paths = files(&dir, &[("1", 10), ("2", 10), ("3", 10), ("4", 10)]);
        let done = ReadJob::with_budget(paths.clone(), 15).run();
        assert_eq!(
            done.read.len(),
            2,
            "the file that crosses the budget is read whole"
        );
        assert_eq!(done.rest, paths[2..]);
        let more = ReadJob::with_budget(done.rest, 15).run();
        assert_eq!(more.read.len(), 2);
        assert_eq!(more.rest, Vec::<std::path::PathBuf>::new());
    }

    #[test]
    fn a_job_always_reads_at_least_one_file() {
        let dir = tempfile::tempdir().expect("dir");
        let paths = files(&dir, &[("1", 10), ("2", 10)]);
        let done = ReadJob::with_budget(paths, 0).run();
        assert_eq!(done.read.len(), 1);
        assert_eq!(done.rest.len(), 1);
    }

    #[test]
    fn a_file_over_the_size_limit_is_none_without_reading_it() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("big.bin");
        let big = std::fs::File::create(&path).expect("file");
        big.set_len(crate::item_editor::max_attachment_bytes() + 1)
            .expect("sparse");
        let done = ReadJob::new(vec![path]).run();
        assert_eq!(done.read, [None]);
    }
}
