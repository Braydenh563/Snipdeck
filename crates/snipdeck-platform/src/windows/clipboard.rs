//! `WinClipboard`: [`crate::pipeline::Clipboard`] on the Win32 clipboard, with delayed rendering.
//!
//! # Thread model
//!
//! [`crate::pipeline::use_item`] blocks its thread while it polls
//! [`Clipboard::render_requested`], so the clipboard owner cannot live on that thread: a Target's
//! `GetClipboardData` sends `WM_RENDERFORMAT` to the owner window and waits for it. `WinClipboard`
//! therefore starts one thread with a hidden message-only window that pumps messages. That window
//! is the clipboard owner, and **every** clipboard call runs on its thread: the public methods
//! post a job and wait for its reply (capped at [`REPLY_TIMEOUT`]). A job the caller gave up on
//! before it started is never run. One thread means the window's state needs no lock and the
//! order of our own calls and the notifications they cause (our `EmptyClipboard` sends us
//! `WM_DESTROYCLIPBOARD` synchronously) is plain program order.
//!
//! The owner state lives in a thread-local `RefCell`, borrowed only for Rust work and never across
//! a Win32 call that can call back into the window procedure (`EmptyClipboard`,
//! `GetClipboardData`, `PeekMessage`, ...). While waiting for a busy clipboard, the thread keeps
//! answering sent messages, so a Target that holds the clipboard open while asking for several
//! formats is never stalled by our own retry loop. A snapshot is skipped (the paste goes ahead
//! without a restore) when the current owner is a hung window, whose renders would wedge the
//! thread.
//!
//! # Delayed rendering and the restore signal
//!
//! A transient write offers every representation with `SetClipboardData(format, NULL)` and sets
//! the three privacy markers with real data. The first `WM_RENDERFORMAT` stores the write's
//! generation in an atomic that [`Clipboard::render_requested`] reads. The rendered bytes are
//! prepared before the write, so rendering only copies them into an `HGLOBAL` (the `CF_DIB` made
//! from a PNG is decoded on demand, within [`crate::win_formats::DIB_DECODE_LIMIT`]).
//!
//! **Limitation:** any reader's request counts, not only the Target's. Windows does not say who
//! asked (`GetOpenClipboardWindow` names a window, but the Target's process is not known here),
//! so a clipboard monitor that ignores `ExcludeClipboardContentFromMonitorProcessing` and reads
//! the item first ends the wait early, and the restore may then beat the Target's own read.
//!
//! # Privacy
//!
//! Snipdeck's own copies of content are zeroed before they are freed: the bytes prepared for
//! rendering (as soon as ownership is lost, on `WM_DESTROYCLIPBOARD`), snapshots, decoded images,
//! text read back, and `HGLOBAL`s that Windows refused. Not covered: the caller's
//! [`crate::payload::Payload`] (the pipeline clones it; its owner drops it), memory handed to
//! Windows, and pasted files on disk (see [`crate::win_formats::KEPT_PASTE_DIRS`]). Nothing here
//! logs content. A private payload carries the markers on every write, kept copies included.
//!
//! # When a restore is allowed (sequence numbers)
//!
//! `GetClipboardSequenceNumber` is documented to increase whenever the contents change, and "if
//! clipboard rendering is delayed, the sequence number is not incremented until the changes are
//! rendered". So answering `WM_RENDERFORMAT` with `SetClipboardData` **does** bump it: the
//! spike's check (sequence number unchanged since the write) would refuse every restore after a
//! delayed render. Instead [`crate::win_formats::OwnerLedger`] records the numbers caused by our
//! own changes (the write, each render, and one possible bump when the requester closes the
//! clipboard, which the documentation neither states nor rules out). A restore opens the
//! clipboard, handles pending `WM_CLIPBOARDUPDATE`s, and puts the snapshot back only if it is
//! still the latest write, nobody emptied the clipboard (no `WM_DESTROYCLIPBOARD`), the window
//! is still the owner, and the sequence number is the last one recorded as ours. Replacing
//! content requires `EmptyClipboard`, which changes the owner, so ownership catches every
//! replacement; the sequence number catches a format added without emptying. Any doubt refuses
//! the restore: the item stays on the clipboard and newer user content is never overwritten.
//!
//! The restore writes with the privacy markers so clipboard managers and history do not record
//! the old content a second time. If a transient write empties the clipboard and Windows then
//! refuses every format, the snapshot taken just before (when the caller still holds it and the
//! clipboard has not changed since) is put back instead of leaving the clipboard empty.
//!
//! None of this has run on Windows yet (see `tools/windows-test/`).

use std::cell::RefCell;
use std::ffi::c_void;
use std::fs;
use std::io::Write as _;
use std::os::windows::ffi::OsStrExt as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering, compiler_fence};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, SetLastError,
    WIN32_ERROR, WPARAM,
};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EmptyClipboard, EnumClipboardFormats,
    GetClipboardData, GetClipboardOwner, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
    OpenClipboard, RegisterClipboardFormatW, RemoveClipboardFormatListener, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GMEM_ZEROINIT, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::{CF_DIB, CF_HDROP, CF_UNICODETEXT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, HWND_MESSAGE, IsHungAppWindow,
    MSG, PM_NOREMOVE, PM_QS_SENDMESSAGE, PM_REMOVE, PeekMessageW, PostMessageW, PostQuitMessage,
    RegisterClassExW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_CLIPBOARDUPDATE, WM_CLOSE,
    WM_DESTROY, WM_DESTROYCLIPBOARD, WM_RENDERALLFORMATS, WM_RENDERFORMAT, WNDCLASSEXW,
};
use windows::core::{HRESULT, PCWSTR, w};

use crate::cf_html;
use crate::clipboard_watch::{self, ChangeSink, ClipboardChange, WatchGate};
use crate::error::Error as ServiceError;
use crate::payload::{Data, FilePayload, Payload};
use crate::pipeline::{Clipboard, Error};
use crate::win_formats::{self, DROPEFFECT_COPY, MARKER_DWORD_ZERO, OwnerLedger, PASTE_DIR_PREFIX};

/// How long a caller waits for the clipboard thread to answer. Generous because a snapshot makes
/// other apps render their delayed formats (a large Excel range takes a while).
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(5);
/// How long to retry `OpenClipboard` while another app holds the clipboard, per operation.
const WRITE_OPEN_BUDGET: Duration = Duration::from_millis(50);
const READ_OPEN_BUDGET: Duration = Duration::from_millis(50);
const SNAPSHOT_OPEN_BUDGET: Duration = Duration::from_millis(100);
/// Longer: right after a render the Target may still hold the clipboard to read other formats.
const RESTORE_OPEN_BUDGET: Duration = Duration::from_millis(250);
const OPEN_RETRY_STEP: Duration = Duration::from_millis(5);
/// The most a snapshot keeps; formats that would exceed it are left out of the restore.
pub const SNAPSHOT_LIMIT: usize = 64 * 1024 * 1024;
/// The most text [`WinClipboard::read_text`] reads (bytes of UTF-16).
const READ_TEXT_LIMIT: usize = 32 * 1024 * 1024;
/// How long `Drop` waits for the thread to render outstanding formats and exit.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(2);
/// Posted to the owner window when jobs are queued.
const WM_RUN_JOBS: u32 = WM_APP + 1;
const CLASS_NAME: PCWSTR = w!("SnipdeckClipboardOwner");

/// Job states: queued, picked up by the thread, or abandoned by a caller that timed out.
const JOB_PENDING: u8 = 0;
const JOB_STARTED: u8 = 1;
const JOB_CANCELLED: u8 = 2;

type Job = Box<dyn FnOnce() + Send>;

/// The Windows clipboard, owned by a dedicated thread (see the module documentation).
pub struct WinClipboard {
    /// The owner window, as an address: `HWND` is not `Send`. Only used to post messages.
    window: usize,
    formats: Formats,
    jobs: Sender<Job>,
    /// The generation of the latest write a render was requested for.
    rendered: Arc<AtomicU64>,
    /// The latest snapshot, while its holder keeps it: the fallback for a refused write.
    latest_snapshot: Weak<SnapshotData>,
    thread: Option<JoinHandle<()>>,
    exited: Receiver<()>,
}

impl std::fmt::Debug for WinClipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WinClipboard")
            .field("window", &format_args!("{:#x}", self.window))
            .finish_non_exhaustive()
    }
}

/// The clipboard contents before a paste: every memory-backed format, bytes copied.
///
/// Zeroed when the last copy is dropped: it is the user's previous clipboard and may be a
/// password.
pub struct Snapshot {
    data: Arc<SnapshotData>,
}

struct SnapshotData {
    formats: Vec<(u32, Vec<u8>)>,
    /// `GetClipboardSequenceNumber` when the snapshot was taken.
    sequence: u32,
}

impl Snapshot {
    /// How many formats were captured.
    pub fn format_count(&self) -> usize {
        self.data.formats.len()
    }
}

impl Drop for SnapshotData {
    fn drop(&mut self) {
        for (_, bytes) in &mut self.formats {
            wipe(bytes);
        }
    }
}

// Shapes only, never bytes.
impl std::fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let formats = &self.data.formats;
        let bytes: usize = formats.iter().map(|(_, b)| b.len()).sum();
        write!(f, "Snapshot({} formats, {bytes} bytes)", formats.len())
    }
}

/// Identifies one write; see [`Clipboard::Written`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Written {
    generation: u64,
}

impl WinClipboard {
    /// Starts the clipboard thread and its owner window.
    ///
    /// # Errors
    ///
    /// [`ServiceError`] if the thread or the window cannot be created.
    pub fn new() -> Result<Self, ServiceError> {
        let formats = Formats::register();
        let (jobs, job_queue) = mpsc::channel::<Job>();
        let (ready_tx, ready) = mpsc::channel();
        let (exited_tx, exited) = mpsc::channel();
        let rendered = Arc::new(AtomicU64::new(0));
        let thread_rendered = Arc::clone(&rendered);
        let thread = std::thread::Builder::new()
            .name("snipdeck-clipboard".into())
            .spawn(move || {
                owner_thread(thread_rendered, &job_queue, &ready_tx);
                let _ = exited_tx.send(());
            })
            .map_err(|_| ServiceError::Unavailable("could not start the clipboard thread"))?;
        match ready.recv_timeout(REPLY_TIMEOUT) {
            Ok(Ok(window)) => Ok(Self {
                window,
                formats,
                jobs,
                rendered,
                latest_snapshot: Weak::new(),
                thread: Some(thread),
                exited,
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => Err(ServiceError::Unavailable(
                "the clipboard thread did not start",
            )),
        }
    }

    /// The clipboard's text (`CF_UNICODETEXT`), for the `{clipboard}` template variable.
    ///
    /// `None` if there is no text or the clipboard stays busy. Writes nothing, so no markers.
    pub fn read_text(&mut self) -> Option<String> {
        self.call(read_text_job).ok().flatten()
    }

    /// Reports every clipboard change to `sink` (on the clipboard thread) while `gate` is open.
    /// While the gate is closed nothing is read. Replaces an earlier listener.
    pub fn watch(&self, gate: WatchGate, sink: ChangeSink) {
        let _ = self.call(move || {
            with_owner(|owner| owner.watch = Some(Watch { gate, sink }));
        });
    }

    /// Empties the clipboard if it holds exactly `text` as `CF_UNICODETEXT`. Whether it did.
    pub fn clear_if_text(&mut self, text: &str) -> bool {
        let expected = text.to_owned();
        self.call(move || clear_if_text_job(&expected))
            .unwrap_or(false)
    }

    fn call<R: Send + 'static>(
        &self,
        job: impl FnOnce() -> R + Send + 'static,
    ) -> Result<R, Error> {
        let (reply, answer) = mpsc::sync_channel(1);
        let state = Arc::new(AtomicU8::new(JOB_PENDING));
        let job_state = Arc::clone(&state);
        self.jobs
            .send(Box::new(move || {
                let start = job_state.compare_exchange(
                    JOB_PENDING,
                    JOB_STARTED,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
                if start.is_ok() {
                    let _ = reply.send(job());
                }
            }))
            .map_err(|_| stopped())?;
        // SAFETY: PostMessageW only reads its arguments; posting to a window that no longer
        // exists fails cleanly instead of touching memory.
        unsafe { PostMessageW(Some(self.hwnd()), WM_RUN_JOBS, WPARAM(0), LPARAM(0)) }
            .map_err(|_| stopped())?;
        match answer.recv_timeout(REPLY_TIMEOUT) {
            Ok(result) => Ok(result),
            Err(RecvTimeoutError::Timeout) => {
                let cancel = state.compare_exchange(
                    JOB_PENDING,
                    JOB_CANCELLED,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
                Err(Error::Clipboard(if cancel.is_ok() {
                    "the clipboard thread is busy; nothing was done".into()
                } else {
                    // Already running (stuck in another app's render): it may still finish.
                    "the clipboard thread did not answer in time".into()
                }))
            }
            Err(RecvTimeoutError::Disconnected) => Err(stopped()),
        }
    }

    fn hwnd(&self) -> HWND {
        HWND(std::ptr::with_exposed_provenance_mut::<c_void>(self.window))
    }

    fn write(&mut self, payload: &Payload, mode: WriteMode) -> Result<u64, Error> {
        let entries = prepare(payload, &self.formats)?;
        let formats = self.formats;
        // Only a paste has a snapshot taken just before its write.
        let fallback = mode
            .delayed
            .then(|| self.latest_snapshot.upgrade())
            .flatten();
        self.call(move || write_job(entries, mode, formats, fallback))?
    }
}

/// How a write places its formats.
#[derive(Debug, Clone, Copy)]
struct WriteMode {
    /// Offered for delayed rendering (a paste) rather than rendered now (a kept copy).
    delayed: bool,
    /// Carries the history, monitor and cloud markers.
    markers: bool,
}

impl Clipboard for WinClipboard {
    type Snapshot = Snapshot;
    type Written = Written;

    fn snapshot(&mut self) -> Result<Snapshot, Error> {
        let snapshot = self.call(snapshot_job)??;
        self.latest_snapshot = Arc::downgrade(&snapshot.data);
        Ok(snapshot)
    }

    fn write_transient(&mut self, payload: &Payload) -> Result<Written, Error> {
        let mode = WriteMode {
            delayed: true,
            markers: true,
        };
        self.write(payload, mode)
            .map(|generation| Written { generation })
    }

    fn write_persistent(&mut self, payload: &Payload) -> Result<(), Error> {
        // A sensitive item stays out of history even when the user keeps it (SPEC §5.7).
        let mode = WriteMode {
            delayed: false,
            markers: payload.is_private(),
        };
        self.write(payload, mode).map(|_| ())
    }

    /// Whether anyone asked for a format of `written`. Any reader counts, not only the Target:
    /// see the limitation in the module documentation.
    fn render_requested(&mut self, written: &Written) -> bool {
        self.rendered.load(Ordering::Acquire) == written.generation
    }

    fn restore(&mut self, snapshot: Snapshot, written: Written) -> Result<bool, Error> {
        let formats = self.formats;
        self.call(move || restore_job(&snapshot, written, formats))?
    }
}

impl Drop for WinClipboard {
    fn drop(&mut self) {
        // WM_CLOSE -> DestroyWindow, which sends WM_RENDERALLFORMATS (outstanding formats stay
        // available after we exit) and WM_DESTROY, which ends the message loop.
        // SAFETY: as in `call`.
        let _ = unsafe { PostMessageW(Some(self.hwnd()), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        // A thread stuck in another app's render is left behind rather than hanging the caller.
        match self.exited.recv_timeout(SHUTDOWN_WAIT) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                if let Some(thread) = self.thread.take() {
                    let _ = thread.join();
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

fn stopped() -> Error {
    Error::Clipboard("the clipboard thread has stopped".into())
}

fn busy() -> Error {
    Error::Clipboard("another app is holding the clipboard".into())
}

fn os_error(op: &str, error: &windows::core::Error) -> Error {
    // The HRESULT bits, not the message: messages are localised and say nothing more.
    Error::Clipboard(format!(
        "{op} failed ({:#010x})",
        error.code().0.cast_unsigned()
    ))
}

/// Registered clipboard format ids (0 where registration failed: that format is skipped).
#[derive(Debug, Clone, Copy)]
struct Formats {
    html: u32,
    rtf: u32,
    png: u32,
    drop_effect: u32,
    exclude_monitor: u32,
    history: u32,
    cloud: u32,
}

impl Formats {
    fn register() -> Self {
        fn register(name: PCWSTR) -> u32 {
            // SAFETY: `name` is a NUL-terminated static string from `w!`.
            unsafe { RegisterClipboardFormatW(name) }
        }
        Self {
            html: register(w!("HTML Format")),
            rtf: register(w!("Rich Text Format")),
            png: register(w!("PNG")),
            drop_effect: register(w!("Preferred DropEffect")),
            exclude_monitor: register(w!("ExcludeClipboardContentFromMonitorProcessing")),
            history: register(w!("CanIncludeInClipboardHistory")),
            cloud: register(w!("CanUploadToCloudClipboard")),
        }
    }

    fn markers(self) -> [u32; 3] {
        [self.exclude_monitor, self.history, self.cloud]
    }
}

/// Bytes of one format, prepared before the write.
enum Source {
    Bytes(Vec<u8>),
    /// `CF_DIB` decoded from these PNG bytes when asked for.
    DibOf(Vec<u8>),
}

impl Source {
    /// Calls `f` with the rendered bytes; `None` if they cannot be produced (a bad PNG).
    fn with_bytes<R>(&self, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
        match self {
            Self::Bytes(bytes) => Some(f(bytes)),
            Self::DibOf(png) => {
                let mut dib = win_formats::png_to_dib(png)?;
                let result = f(&dib);
                wipe(&mut dib);
                Some(result)
            }
        }
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        match self {
            Self::Bytes(bytes) | Self::DibOf(bytes) => wipe(bytes),
        }
    }
}

struct Entry {
    format: u32,
    source: Source,
}

fn add(entries: &mut Vec<Entry>, format: u32, source: Source) {
    // The first representation of a format wins; 0 is a format that failed to register.
    if format != 0 && !entries.iter().any(|entry| entry.format == format) {
        entries.push(Entry { format, source });
    }
}

/// The clipboard formats for `payload`, primary first. Writes pasted files to disk.
fn prepare(payload: &Payload, formats: &Formats) -> Result<Vec<Entry>, Error> {
    let mut entries = Vec::new();
    for data in payload.data() {
        match data {
            Data::Text(text) => add(
                &mut entries,
                u32::from(CF_UNICODETEXT.0),
                Source::Bytes(win_formats::unicode_text(text)),
            ),
            Data::Html(fragment) => {
                if let Some(bytes) = cf_html::build(fragment) {
                    add(&mut entries, formats.html, Source::Bytes(bytes));
                }
            }
            Data::Rtf(rtf) => add(&mut entries, formats.rtf, Source::Bytes(rtf.clone())),
            Data::Png(png) => {
                add(&mut entries, formats.png, Source::Bytes(png.clone()));
                add(
                    &mut entries,
                    u32::from(CF_DIB.0),
                    Source::DibOf(png.clone()),
                );
            }
            Data::Files(files) if !files.is_empty() => {
                let paths = write_paste_files(files)?;
                let hdrop = win_formats::drop_files(&paths)
                    .ok_or_else(|| Error::Clipboard("a pasted file path is invalid".into()))?;
                add(&mut entries, u32::from(CF_HDROP.0), Source::Bytes(hdrop));
                add(
                    &mut entries,
                    formats.drop_effect,
                    Source::Bytes(DROPEFFECT_COPY.to_vec()),
                );
            }
            Data::Files(_) => {}
        }
    }
    if entries.is_empty() {
        return Err(Error::Clipboard("nothing to put on the clipboard".into()));
    }
    Ok(entries)
}

/// Writes `files` into a fresh `%TEMP%\Snipdeck\paste-<n>\` and returns their UTF-16 paths.
/// Older paste folders are removed first, except the newest ones kept by
/// [`win_formats::stale_paste_dirs`] (best effort: a file still open stays until a later paste).
fn write_paste_files(files: &[FilePayload]) -> Result<Vec<Vec<u16>>, Error> {
    let io = |error: std::io::Error| {
        Error::Clipboard(format!(
            "could not write the pasted files ({})",
            error.kind()
        ))
    };
    if !files
        .iter()
        .all(|file| win_formats::is_plain_file_name(&file.name))
    {
        return Err(Error::Clipboard("a pasted file name is invalid".into()));
    }
    let root = std::env::temp_dir().join("Snipdeck");
    fs::create_dir_all(&root).map_err(io)?;
    let existing: Vec<u64> = fs::read_dir(&root)
        .map_err(io)?
        .flatten()
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(win_formats::paste_dir_number)
        })
        .collect();
    for number in win_formats::stale_paste_dirs(&existing) {
        let _ = fs::remove_dir_all(root.join(format!("{PASTE_DIR_PREFIX}{number}")));
    }
    let highest = existing.iter().copied().max().unwrap_or(0);
    let dir = claim_paste_dir(&root, highest.saturating_add(1)).map_err(io)?;
    let mut paths = Vec::with_capacity(files.len());
    for file in files {
        let path = dir.join(&file.name);
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(io)?;
        out.write_all(&file.bytes).map_err(io)?;
        paths.push(path.as_os_str().encode_wide().collect());
    }
    Ok(paths)
}

fn claim_paste_dir(root: &std::path::Path, first: u64) -> std::io::Result<PathBuf> {
    let mut number = first;
    loop {
        let dir = root.join(format!("{PASTE_DIR_PREFIX}{number}"));
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                // Bounded: give up after 1000 taken names rather than loop forever.
                if number - first >= 1000 {
                    return Err(error);
                }
                number += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

/// Overwrites `bytes` with zeros in a way the optimiser may not remove.
fn wipe(bytes: &mut [u8]) {
    for byte in bytes.iter_mut() {
        // SAFETY: `byte` is a valid, aligned, exclusive reference to an initialised `u8`.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    compiler_fence(Ordering::SeqCst);
}

// ---------------------------------------------------------------------------------------------
// Owner thread
// ---------------------------------------------------------------------------------------------

/// The owner window's state; only touched on the clipboard thread.
struct Owner {
    hwnd: HWND,
    rendered: Arc<AtomicU64>,
    ledger: OwnerLedger,
    /// Formats of the latest transient write still to render; empty once ownership is lost.
    delayed: Vec<Entry>,
    /// The Recent inbox's listener, once installed with [`WinClipboard::watch`].
    watch: Option<Watch>,
}

/// A listener for clipboard changes and the switch that turns it on.
struct Watch {
    gate: WatchGate,
    sink: ChangeSink,
}

thread_local! {
    static OWNER: RefCell<Option<Owner>> = const { RefCell::new(None) };
}

/// Runs `f` on the owner state; `None` off the clipboard thread or while it is borrowed (which
/// the borrow rules in the module documentation rule out).
fn with_owner<R>(f: impl FnOnce(&mut Owner) -> R) -> Option<R> {
    OWNER.with(|cell| {
        let mut borrowed = cell.try_borrow_mut().ok()?;
        borrowed.as_mut().map(f)
    })
}

fn owner_thread(
    rendered: Arc<AtomicU64>,
    jobs: &Receiver<Job>,
    ready: &Sender<Result<usize, ServiceError>>,
) {
    let hwnd = match create_owner_window() {
        Ok(hwnd) => hwnd,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    OWNER.with(|cell| {
        *cell.borrow_mut() = Some(Owner {
            hwnd,
            rendered,
            ledger: OwnerLedger::default(),
            delayed: Vec::new(),
            watch: None,
        });
    });
    // Without the listener a render's possible bump on close goes unrecorded and the restore
    // after it is refused (the safe side), so a failure here is not fatal.
    // SAFETY: `hwnd` is the live window this thread just created.
    let _ = unsafe { AddClipboardFormatListener(hwnd) };
    let _ = ready.send(Ok(hwnd.0.expose_provenance()));

    let mut msg = MSG::default();
    loop {
        // SAFETY: `msg` is a valid MSG to write into; a None filter reads this thread's queue.
        let got = unsafe { GetMessageW(&raw mut msg, None, 0, 0) };
        if got.0 == 0 || got.0 == -1 {
            break; // WM_QUIT, or an error that would repeat forever.
        }
        if msg.message == WM_RUN_JOBS && msg.hwnd == hwnd {
            while let Ok(job) = jobs.try_recv() {
                // A panicking job drops its reply sender: the caller gets an error.
                let _ = catch_unwind(AssertUnwindSafe(job));
            }
            continue;
        }
        // SAFETY: `msg` was filled by GetMessageW.
        unsafe { DispatchMessageW(&raw const msg) };
    }
    // Dropping the state zeroes any payload copy still held.
    OWNER.with(|cell| drop(cell.borrow_mut().take()));
}

fn create_owner_window() -> Result<HWND, ServiceError> {
    // SAFETY: None asks for the handle of the running executable; nothing is freed later.
    let module = unsafe { GetModuleHandleW(None) }
        .map_err(|e| ServiceError::from_windows("GetModuleHandleW", &e))?;
    let class = WNDCLASSEXW {
        cbSize: u32::try_from(size_of::<WNDCLASSEXW>())
            .map_err(|_| ServiceError::Unavailable("WNDCLASSEXW size"))?,
        lpfnWndProc: Some(owner_wndproc),
        hInstance: module.into(),
        lpszClassName: CLASS_NAME,
        ..WNDCLASSEXW::default()
    };
    // SAFETY: `class` is fully initialised and its class name is a static string.
    if unsafe { RegisterClassExW(&raw const class) } == 0 {
        let error = windows::core::Error::from_thread();
        // A second WinClipboard in the same process reuses the class.
        if error.code() != HRESULT::from_win32(ERROR_CLASS_ALREADY_EXISTS.0) {
            return Err(ServiceError::from_windows("RegisterClassExW", &error));
        }
    }
    // SAFETY: the class is registered with `owner_wndproc`; HWND_MESSAGE makes a message-only
    // window that is never shown, has no parent to outlive and is destroyed on this thread.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS_NAME,
            w!("Snipdeck clipboard"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(module.into()),
            None,
        )
    }
    .map_err(|e| ServiceError::from_windows("CreateWindowExW", &e))
}

/// The owner window procedure. It must not unwind into Windows: every handler runs under
/// `catch_unwind`, and a panic falls back to `DefWindowProcW`.
unsafe extern "system" fn owner_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let handled = catch_unwind(AssertUnwindSafe(|| match msg {
        WM_RENDERFORMAT => {
            // Never OpenClipboard here: the requester holds it open.
            render_format(u32::try_from(wparam.0).unwrap_or(0));
            true
        }
        WM_RENDERALLFORMATS => {
            render_all_formats(hwnd);
            true
        }
        WM_DESTROYCLIPBOARD => {
            with_owner(|owner| {
                owner.ledger.destroyed();
                owner.delayed.clear(); // Source::drop zeroes the bytes.
            });
            true
        }
        WM_CLIPBOARDUPDATE => {
            clipboard_updated(hwnd);
            true
        }
        WM_DESTROY => {
            // SAFETY: `hwnd` is this live window; removing a listener that was never added fails
            // harmlessly. PostQuitMessage only posts to this thread's queue.
            unsafe {
                let _ = RemoveClipboardFormatListener(hwnd);
                PostQuitMessage(0);
            }
            true
        }
        _ => false,
    }));
    if matches!(handled, Ok(true)) {
        LRESULT(0)
    } else {
        // SAFETY: forwarding the unmodified message to the default procedure.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }
}

/// Answers a request for one delayed `format` of the latest transient write.
fn render_format(format: u32) {
    let global = with_owner(|owner| {
        let entry = owner.delayed.iter().find(|entry| entry.format == format)?;
        // Someone has asked: this is the signal the pipeline waits for.
        owner
            .rendered
            .store(owner.ledger.generation(), Ordering::Release);
        entry.source.with_bytes(Global::copy_of).flatten()
    })
    .flatten();
    let Some(global) = global else {
        return;
    };
    if global.into_clipboard(format) {
        // SAFETY: no preconditions.
        let sequence = unsafe { GetClipboardSequenceNumber() };
        with_owner(|owner| owner.ledger.rendered(sequence));
    }
}

/// Renders every outstanding format before the owner window goes away.
fn render_all_formats(hwnd: HWND) {
    let Some(_open) = OpenGuard::open(hwnd, WRITE_OPEN_BUDGET) else {
        return;
    };
    // SAFETY: no preconditions; we hold the clipboard open.
    if unsafe { GetClipboardOwner() }.ok() != Some(hwnd) {
        return;
    }
    let formats: Vec<u32> =
        with_owner(|owner| owner.delayed.iter().map(|e| e.format).collect()).unwrap_or_default();
    for format in formats {
        render_format(format);
    }
}

/// `WM_CLIPBOARDUPDATE`: lets the ledger take a requester's bump on close as ours.
fn clipboard_updated(hwnd: HWND) {
    report_change(hwnd);
    if with_owner(|owner| owner.ledger.wants_update()) != Some(true) {
        return;
    }
    // SAFETY: no preconditions.
    let still_owner = unsafe { GetClipboardOwner() }.ok() == Some(hwnd);
    // SAFETY: no preconditions.
    let sequence = unsafe { GetClipboardSequenceNumber() };
    with_owner(|owner| owner.ledger.clipboard_updated(still_owner, sequence));
}

/// Hands the change to the Recent inbox's listener, if one is installed and switched on.
fn report_change(hwnd: HWND) {
    // Taken out for the call, so the listener runs with no borrow of the owner state held.
    let Some(watch) = with_owner(|owner| owner.watch.take()).flatten() else {
        return;
    };
    if watch.gate.is_open() {
        (watch.sink)(observe_change(hwnd));
    }
    with_owner(|owner| {
        if owner.watch.is_none() {
            owner.watch = Some(watch);
        }
    });
}

/// Reads who changed the clipboard and what it holds, unless the source asked to be left out
/// (`ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory` = 0): then
/// the text is never read. Our own writes are reported as `own` and not read either.
fn observe_change(hwnd: HWND) -> ClipboardChange {
    let formats = Formats::register();
    // SAFETY: no preconditions.
    let owner = unsafe { GetClipboardOwner() }.ok();
    if owner == Some(hwnd) {
        return ClipboardChange {
            own: true,
            ..ClipboardChange::default()
        };
    }
    let mut change = ClipboardChange {
        source_app: owner.and_then(super::target::window_exe_name),
        ..ClipboardChange::default()
    };
    // SAFETY: no preconditions; a format id of 0 is not asked about.
    let monitor_marker = formats.exclude_monitor != 0
        && unsafe { IsClipboardFormatAvailable(formats.exclude_monitor) }.is_ok();
    if monitor_marker {
        change.excluded_format = true;
        return change;
    }
    let Some(_open) = OpenGuard::open(hwnd, READ_OPEN_BUDGET) else {
        return change;
    };
    let history = if formats.history == 0 {
        None
    } else {
        // SAFETY: the clipboard is open by this thread; the handle stays owned by the system.
        unsafe { GetClipboardData(formats.history) }
            .ok()
            .and_then(|handle| read_global(HGLOBAL(handle.0), 16))
            .and_then(|bytes| clipboard_watch::dword(&bytes))
    };
    if clipboard_watch::excluded_by_markers(false, history) {
        change.excluded_format = true;
        return change;
    }
    // SAFETY: no preconditions.
    if unsafe { IsClipboardFormatAvailable(u32::from(CF_UNICODETEXT.0)) }.is_ok() {
        // SAFETY: the clipboard is open by this thread; the handle stays owned by the system.
        if let Ok(handle) = unsafe { GetClipboardData(u32::from(CF_UNICODETEXT.0)) } {
            if let Some(mut bytes) = read_global(HGLOBAL(handle.0), READ_TEXT_LIMIT) {
                change.text = Some(win_formats::from_unicode_text(&bytes));
                wipe(&mut bytes);
            }
        }
    }
    change
}

fn owner_hwnd() -> Option<HWND> {
    with_owner(|owner| owner.hwnd)
}

fn write_job(
    entries: Vec<Entry>,
    mode: WriteMode,
    formats: Formats,
    fallback: Option<Arc<SnapshotData>>,
) -> Result<u64, Error> {
    let hwnd = owner_hwnd().ok_or_else(stopped)?;
    let open = OpenGuard::open(hwnd, WRITE_OPEN_BUDGET).ok_or_else(busy)?;
    // The snapshot is a valid fallback only if nothing changed since it was taken.
    // SAFETY: no preconditions.
    let sequence_now = unsafe { GetClipboardSequenceNumber() };
    let fallback = fallback.filter(|snapshot| snapshot.sequence == sequence_now);
    // Makes us the owner. If we already were, this sends us WM_DESTROYCLIPBOARD right now, which
    // zeroes the previous write's bytes.
    // SAFETY: the clipboard is open by this thread with our window as the owner.
    unsafe { EmptyClipboard() }.map_err(|e| os_error("EmptyClipboard", &e))?;
    let generation = with_owner(|owner| owner.ledger.begin_write()).ok_or_else(stopped)?;

    let mut placed = 0usize;
    if mode.delayed {
        for entry in &entries {
            if set_delayed(entry.format) {
                placed += 1;
            }
        }
    } else {
        for entry in &entries {
            if entry
                .source
                .with_bytes(|bytes| set_now(entry.format, bytes))
                == Some(true)
            {
                placed += 1;
            }
        }
    }
    if placed == 0 {
        // Do not leave the clipboard empty: put the user's previous content back if we have it.
        if let Some(snapshot) = fallback {
            put_back(&snapshot, formats);
        }
        drop(open);
        return Err(Error::Clipboard(
            "Windows refused every clipboard format".into(),
        ));
    }
    if mode.markers {
        set_markers(formats);
    }
    if mode.delayed {
        // Rendering happens later, from the window procedure.
        with_owner(|owner| owner.delayed = entries);
    } else {
        drop(entries);
    }
    drop(open);
    // Read after CloseClipboard, so a bump on close counts as ours. If another app changed the
    // clipboard in between, it emptied it first and we have lost ownership, which the restore
    // check sees.
    // SAFETY: no preconditions.
    let sequence = unsafe { GetClipboardSequenceNumber() };
    with_owner(|owner| owner.ledger.wrote(sequence));
    Ok(generation)
}

fn snapshot_job() -> Result<Snapshot, Error> {
    let hwnd = owner_hwnd().ok_or_else(stopped)?;
    // A hung owner would block every GetClipboardData below, and with it this thread.
    // SAFETY: no preconditions.
    let owner = unsafe { GetClipboardOwner() }
        .ok()
        .filter(|&owner| owner != hwnd);
    // SAFETY: IsHungAppWindow only inspects the window; any handle value is accepted.
    if owner.is_some_and(|owner| unsafe { IsHungAppWindow(owner) }.as_bool()) {
        return Err(Error::Clipboard(
            "the app that owns the clipboard is not responding".into(),
        ));
    }
    let open = OpenGuard::open(hwnd, SNAPSHOT_OPEN_BUDGET).ok_or_else(busy)?;
    let mut formats = Vec::new();
    let mut total = 0usize;
    let mut format = 0;
    loop {
        // SAFETY: the clipboard is open by this thread.
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            break;
        }
        if !win_formats::is_memory_format(format) {
            continue;
        }
        // Makes the owner render a delayed format (it may be our own window: same thread, so
        // the window procedure runs inside this call; no state is borrowed here).
        // SAFETY: the clipboard is open by this thread; the handle stays owned by the system.
        let Ok(handle) = (unsafe { GetClipboardData(format) }) else {
            continue;
        };
        if let Some(bytes) = read_global(HGLOBAL(handle.0), SNAPSHOT_LIMIT - total) {
            total += bytes.len();
            formats.push((format, bytes));
        }
    }
    // After the renders above, which bump it; nobody else can change it while we hold it open.
    // SAFETY: no preconditions.
    let sequence = unsafe { GetClipboardSequenceNumber() };
    drop(open);
    Ok(Snapshot {
        data: Arc::new(SnapshotData { formats, sequence }),
    })
}

fn restore_job(snapshot: &Snapshot, written: Written, formats: Formats) -> Result<bool, Error> {
    let hwnd = owner_hwnd().ok_or_else(stopped)?;
    let open = OpenGuard::open(hwnd, RESTORE_OPEN_BUDGET).ok_or_else(busy)?;
    // While we hold the clipboard nobody can change it; first take in the updates already posted
    // for earlier closes (see `OwnerLedger::clipboard_updated`).
    drain_clipboard_updates(hwnd);
    // SAFETY: no preconditions; the clipboard is open by this thread.
    let still_owner = unsafe { GetClipboardOwner() }.ok() == Some(hwnd);
    // SAFETY: no preconditions.
    let sequence = unsafe { GetClipboardSequenceNumber() };
    let check = with_owner(|owner| {
        owner
            .ledger
            .check(written.generation, still_owner, sequence)
    })
    .ok_or_else(stopped)?;
    if !check.unchanged() {
        return Ok(false);
    }
    // SAFETY: the clipboard is open by this thread with our window as the owner. This sends us
    // WM_DESTROYCLIPBOARD, which zeroes the item's bytes.
    unsafe { EmptyClipboard() }.map_err(|e| os_error("EmptyClipboard", &e))?;
    // The restore is a write of its own: the old Written no longer matches anything.
    with_owner(|owner| owner.ledger.begin_write());
    put_back(&snapshot.data, formats);
    drop(open);
    Ok(true)
}

/// Places `snapshot`'s formats now, with the markers (unless it is empty). The clipboard must be
/// open by this thread and emptied.
fn put_back(snapshot: &SnapshotData, formats: Formats) {
    let markers = formats.markers();
    for (format, bytes) in &snapshot.formats {
        if !markers.contains(format) {
            set_now(*format, bytes);
        }
    }
    if !snapshot.formats.is_empty() {
        set_markers(formats);
    }
}

fn set_markers(formats: Formats) {
    for marker in formats.markers() {
        if marker != 0 {
            set_now(marker, &MARKER_DWORD_ZERO);
        }
    }
}

fn read_text_job() -> Option<String> {
    let hwnd = owner_hwnd()?;
    let _open = OpenGuard::open(hwnd, READ_OPEN_BUDGET)?;
    // SAFETY: the clipboard is open by this thread; the handle stays owned by the system.
    let handle = unsafe { GetClipboardData(u32::from(CF_UNICODETEXT.0)) }.ok()?;
    let mut bytes = read_global(HGLOBAL(handle.0), READ_TEXT_LIMIT)?;
    let text = win_formats::from_unicode_text(&bytes);
    wipe(&mut bytes);
    Some(text)
}

fn clear_if_text_job(expected: &str) -> bool {
    let Some(hwnd) = owner_hwnd() else {
        return false;
    };
    let Some(_open) = OpenGuard::open(hwnd, WRITE_OPEN_BUDGET) else {
        return false;
    };
    // SAFETY: the clipboard is open by this thread; the handle stays owned by the system.
    let Ok(handle) = (unsafe { GetClipboardData(u32::from(CF_UNICODETEXT.0)) }) else {
        return false;
    };
    let Some(mut bytes) = read_global(HGLOBAL(handle.0), READ_TEXT_LIMIT) else {
        return false;
    };
    let same = win_formats::from_unicode_text(&bytes) == expected;
    wipe(&mut bytes);
    if !same {
        return false;
    }
    // SAFETY: the clipboard is open by this thread with our window as the owner.
    if unsafe { EmptyClipboard() }.is_err() {
        return false;
    }
    // The clear is a write of its own: an older Written no longer matches anything.
    with_owner(|owner| owner.ledger.begin_write());
    true
}

/// Dispatches the `WM_CLIPBOARDUPDATE`s already queued for our window.
fn drain_clipboard_updates(hwnd: HWND) {
    let mut msg = MSG::default();
    // SAFETY: `msg` is a valid MSG to write into; the filter only removes WM_CLIPBOARDUPDATE
    // for our window, and dispatching it runs `owner_wndproc`, which borrows no state we hold.
    while unsafe {
        PeekMessageW(
            &raw mut msg,
            Some(hwnd),
            WM_CLIPBOARDUPDATE,
            WM_CLIPBOARDUPDATE,
            PM_REMOVE,
        )
    }
    .as_bool()
    {
        // SAFETY: `msg` was filled by PeekMessageW.
        unsafe { DispatchMessageW(&raw const msg) };
    }
}

/// Lets Windows deliver messages *sent* to this thread (a Target's `WM_RENDERFORMAT`) without
/// touching posted ones.
fn answer_sent_messages() {
    let mut msg = MSG::default();
    // SAFETY: `msg` is a valid MSG to write into; PM_NOREMOVE | PM_QS_SENDMESSAGE only
    // dispatches sent messages and leaves the queue as it is.
    let _ = unsafe { PeekMessageW(&raw mut msg, None, 0, 0, PM_NOREMOVE | PM_QS_SENDMESSAGE) };
}

/// The clipboard, open by this thread with our window as the owner-to-be; closed on drop.
struct OpenGuard;

impl OpenGuard {
    fn open(hwnd: HWND, budget: Duration) -> Option<Self> {
        let started = Instant::now();
        loop {
            // SAFETY: `hwnd` is our live window. It must not be NULL: after
            // OpenClipboard(NULL), EmptyClipboard leaves no owner and SetClipboardData fails.
            if unsafe { OpenClipboard(Some(hwnd)) }.is_ok() {
                return Some(Self);
            }
            if started.elapsed() >= budget {
                return None;
            }
            answer_sent_messages();
            std::thread::sleep(OPEN_RETRY_STEP);
        }
    }
}

impl Drop for OpenGuard {
    fn drop(&mut self) {
        // SAFETY: this thread opened the clipboard (the guard only exists after a successful
        // OpenClipboard) and every GlobalLock taken meanwhile has been unlocked.
        let _ = unsafe { CloseClipboard() };
    }
}

/// Offers `format` for delayed rendering. The clipboard must be open by this thread.
fn set_delayed(format: u32) -> bool {
    // SAFETY: SetLastError only writes this thread's last-error value.
    unsafe { SetLastError(WIN32_ERROR(0)) };
    // SAFETY: the clipboard is open by this thread and our window is the owner; a NULL handle
    // transfers nothing.
    match unsafe { SetClipboardData(format, None) } {
        Ok(_) => true,
        // For delayed rendering NULL is also the success value; only a last error is a failure.
        Err(error) => error.code().is_ok(),
    }
}

/// Places a copy of `bytes` as `format` now. The clipboard must be open by this thread.
fn set_now(format: u32, bytes: &[u8]) -> bool {
    Global::copy_of(bytes).is_some_and(|global| global.into_clipboard(format))
}

/// A movable global memory block we own until the clipboard takes it.
struct Global(HGLOBAL);

impl Global {
    /// A `GMEM_MOVEABLE` block (required by `SetClipboardData`) holding `bytes`, unlocked.
    fn copy_of(bytes: &[u8]) -> Option<Self> {
        // An empty block would be 0 bytes, which GlobalAlloc treats as discarded; use 1 zero.
        let flags = if bytes.is_empty() {
            GMEM_MOVEABLE | GMEM_ZEROINIT
        } else {
            GMEM_MOVEABLE
        };
        // SAFETY: GlobalAlloc has no memory-safety preconditions.
        let block = Self(unsafe { GlobalAlloc(flags, bytes.len().max(1)) }.ok()?);
        // SAFETY: `block.0` is a live movable block we own.
        let target = unsafe { GlobalLock(block.0) };
        if target.is_null() {
            return None; // `block` drops and is freed.
        }
        // SAFETY: the locked block is at least `bytes.len()` bytes and cannot overlap `bytes`,
        // which lives in Rust-owned memory.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), target.cast::<u8>(), bytes.len()) };
        // SAFETY: balances the GlobalLock above. It reports "error" when the lock count reaches
        // zero, which is the expected outcome.
        let _ = unsafe { GlobalUnlock(block.0) };
        Some(block)
    }

    /// Hands the block to the clipboard as `format`. On success the system owns it and we must
    /// not touch or free it; on failure it is still ours and is zeroed and freed here.
    fn into_clipboard(self, format: u32) -> bool {
        // SAFETY: the clipboard is open by this thread, or we are answering WM_RENDERFORMAT for
        // it; the block is movable, unlocked and ours.
        let placed = unsafe { SetClipboardData(format, Some(HANDLE(self.0.0))) }.is_ok();
        if placed {
            std::mem::forget(self);
        }
        placed
    }
}

impl Drop for Global {
    fn drop(&mut self) {
        // The block is ours: never accepted by SetClipboardData. Zero it, then free it.
        // SAFETY: `self.0` is a live movable block we own and have not locked.
        let size = unsafe { GlobalSize(self.0) };
        // SAFETY: as above.
        let data = unsafe { GlobalLock(self.0) };
        if !data.is_null() {
            // SAFETY: the locked block is `size` writable bytes until GlobalUnlock below.
            unsafe { std::ptr::write_bytes(data.cast::<u8>(), 0, size) };
            compiler_fence(Ordering::SeqCst);
            // SAFETY: balances the GlobalLock above.
            let _ = unsafe { GlobalUnlock(self.0) };
        }
        // SAFETY: the block is ours and unlocked; it is not used after this.
        let _ = unsafe { GlobalFree(Some(self.0)) };
    }
}

/// Copies a clipboard memory handle's bytes; `None` if it is empty, larger than `limit`, or
/// cannot be locked. The handle stays owned by the system.
fn read_global(handle: HGLOBAL, limit: usize) -> Option<Vec<u8>> {
    // SAFETY: `handle` came from GetClipboardData for a memory format while the clipboard is
    // open by this thread, so it is a live global block.
    let size = unsafe { GlobalSize(handle) };
    if size == 0 || size > limit {
        return None;
    }
    // SAFETY: as above.
    let source = unsafe { GlobalLock(handle) };
    if source.is_null() {
        return None;
    }
    // SAFETY: the locked block is `size` readable bytes until GlobalUnlock below.
    let bytes = unsafe { std::slice::from_raw_parts(source.cast::<u8>(), size) }.to_vec();
    // SAFETY: balances the GlobalLock above.
    let _ = unsafe { GlobalUnlock(handle) };
    Some(bytes)
}

#[cfg(test)]
mod tests {
    //! These use the real clipboard, so they are ignored by default and serialised. Run on
    //! Windows with `cargo test -p snipdeck-platform -- --ignored clipboard`.

    use std::sync::{Mutex, PoisonError};

    use super::*;

    static CLIPBOARD: Mutex<()> = Mutex::new(());

    fn text(s: &str) -> Payload {
        Payload::new(vec![Data::Text(s.into())]).unwrap()
    }

    fn has_format(clipboard: &WinClipboard, format: u32) -> bool {
        clipboard
            .call(move || {
                let hwnd = owner_hwnd()?;
                let _open = OpenGuard::open(hwnd, READ_OPEN_BUDGET)?;
                let mut current = 0;
                loop {
                    // SAFETY: the clipboard is open by this thread.
                    current = unsafe { EnumClipboardFormats(current) };
                    if current == 0 || current == format {
                        return Some(current == format);
                    }
                }
            })
            .ok()
            .flatten()
            .unwrap_or(false)
    }

    #[test]
    #[ignore = "uses the real Windows clipboard"]
    fn persistent_text_reads_back_without_markers() {
        let _lock = CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner);
        let mut clipboard = WinClipboard::new().unwrap();
        clipboard.write_persistent(&text("hello ü")).unwrap();
        assert_eq!(clipboard.read_text().as_deref(), Some("hello ü"));
        assert!(!has_format(&clipboard, clipboard.formats.history));
    }

    #[test]
    #[ignore = "uses the real Windows clipboard"]
    fn a_private_kept_copy_carries_the_markers() {
        let _lock = CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner);
        let mut clipboard = WinClipboard::new().unwrap();
        clipboard
            .write_persistent(&text("secret").private())
            .unwrap();
        for marker in clipboard.formats.markers() {
            assert!(has_format(&clipboard, marker));
        }
    }

    #[test]
    #[ignore = "uses the real Windows clipboard"]
    fn a_read_of_transient_text_is_a_render_request() {
        let _lock = CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner);
        let mut clipboard = WinClipboard::new().unwrap();
        let written = clipboard.write_transient(&text("item")).unwrap();
        assert!(!clipboard.render_requested(&written));
        assert_eq!(clipboard.read_text().as_deref(), Some("item"));
        assert!(clipboard.render_requested(&written));
    }

    #[test]
    #[ignore = "uses the real Windows clipboard"]
    fn restore_after_a_render_puts_the_old_content_back() {
        let _lock = CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner);
        let mut clipboard = WinClipboard::new().unwrap();
        clipboard.write_persistent(&text("before")).unwrap();
        let snapshot = clipboard.snapshot().unwrap();
        let written = clipboard.write_transient(&text("item")).unwrap();
        assert_eq!(clipboard.read_text().as_deref(), Some("item"));
        assert_eq!(clipboard.restore(snapshot, written), Ok(true));
        assert_eq!(clipboard.read_text().as_deref(), Some("before"));
    }

    #[test]
    #[ignore = "uses the real Windows clipboard"]
    fn restore_never_overwrites_newer_content() {
        let _lock = CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner);
        let mut clipboard = WinClipboard::new().unwrap();
        let mut other_app = WinClipboard::new().unwrap();
        clipboard.write_persistent(&text("before")).unwrap();
        let snapshot = clipboard.snapshot().unwrap();
        let written = clipboard.write_transient(&text("item")).unwrap();
        other_app.write_persistent(&text("newer")).unwrap();
        assert_eq!(clipboard.restore(snapshot, written), Ok(false));
        assert_eq!(clipboard.read_text().as_deref(), Some("newer"));
    }

    #[test]
    #[ignore = "uses the real Windows clipboard"]
    fn dropping_the_owner_renders_outstanding_formats() {
        let _lock = CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner);
        let mut clipboard = WinClipboard::new().unwrap();
        clipboard.write_transient(&text("kept")).unwrap();
        drop(clipboard);
        let mut reader = WinClipboard::new().unwrap();
        assert_eq!(reader.read_text().as_deref(), Some("kept"));
    }

    #[test]
    #[ignore = "uses the real Windows clipboard"]
    fn paste_folders_keep_the_previous_one_only() {
        let _lock = CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner);
        let mut clipboard = WinClipboard::new().unwrap();
        let files = |name: &str| {
            Payload::new(vec![Data::Files(vec![FilePayload {
                name: name.into(),
                bytes: b"x".to_vec(),
            }])])
            .unwrap()
        };
        for name in ["a.txt", "b.txt", "c.txt"] {
            clipboard.write_persistent(&files(name)).unwrap();
        }
        let root = std::env::temp_dir().join("Snipdeck");
        let mut numbers: Vec<u64> = fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter_map(|e| {
                e.file_name()
                    .to_str()
                    .and_then(win_formats::paste_dir_number)
            })
            .collect();
        numbers.sort_unstable();
        assert_eq!(numbers.len(), win_formats::KEPT_PASTE_DIRS);
        let newest = root.join(format!("{PASTE_DIR_PREFIX}{}", numbers[1]));
        assert!(newest.join("c.txt").is_file());
    }
}
