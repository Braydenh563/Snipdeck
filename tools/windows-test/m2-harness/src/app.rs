//! Start-up report, hotkeys and the message loop.

use std::path::Path;
use std::time::{Duration, Instant};

use snipdeck_core::UseMode;
use snipdeck_platform::chord::Chord;
use snipdeck_platform::pipeline::{self, Env, SETTLE, SystemTiming, Timing};
use snipdeck_platform::route::{self, CopyReason, Policy, Route};
use snipdeck_platform::windows::autostart;
use snipdeck_platform::windows::clipboard::WinClipboard;
use snipdeck_platform::windows::dpapi;
use snipdeck_platform::windows::hotkey::{HotkeyError, HotkeyService};
use snipdeck_platform::windows::keyboard::WinKeyboard;
use snipdeck_platform::windows::single_instance::{Acquired, SingleInstance};
use snipdeck_platform::windows::target::TargetTracker;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PostQuitMessage, PostThreadMessageW, TranslateMessage,
    WM_APP,
};

use crate::items::{self, Item};
use crate::sys;

/// Application id of the single-instance claim.
const APP_ID: &str = "m2-harness";
/// Hotkey id of the `--probe` chord.
const PROBE_ID: u32 = 99;

#[derive(Clone, Copy)]
enum Action {
    Use(Item, UseMode),
    Info,
    Quit,
}

struct Hotkey {
    id: u32,
    chord: &'static str,
    what: &'static str,
    action: Action,
}

const HOTKEYS: [Hotkey; 9] = [
    Hotkey {
        id: 1,
        chord: "Ctrl+Alt+1",
        what: "paste plain text item",
        action: Action::Use(Item::Text, UseMode::Paste),
    },
    Hotkey {
        id: 2,
        chord: "Ctrl+Alt+2",
        what: "paste HTML item (html + text)",
        action: Action::Use(Item::Html, UseMode::Paste),
    },
    Hotkey {
        id: 3,
        chord: "Ctrl+Alt+3",
        what: "plain paste of the HTML item (text only)",
        action: Action::Use(Item::Html, UseMode::PastePlain),
    },
    Hotkey {
        id: 4,
        chord: "Ctrl+Alt+4",
        what: "copy mode (text item stays on the clipboard)",
        action: Action::Use(Item::Text, UseMode::Copy),
    },
    Hotkey {
        id: 5,
        chord: "Ctrl+Alt+5",
        what: "paste 64x64 PNG with a transparent corner",
        action: Action::Use(Item::Png, UseMode::Paste),
    },
    Hotkey {
        id: 6,
        chord: "Ctrl+Alt+6",
        what: "paste two small text files",
        action: Action::Use(Item::Files, UseMode::Paste),
    },
    Hotkey {
        id: 7,
        chord: "Ctrl+Alt+7",
        what: "print Target info only",
        action: Action::Info,
    },
    Hotkey {
        id: 8,
        chord: "Ctrl+Alt+8",
        what: "paste long multi-line unicode text",
        action: Action::Use(Item::Unicode, UseMode::Paste),
    },
    Hotkey {
        id: 9,
        chord: "Ctrl+Alt+Q",
        what: "quit",
        action: Action::Quit,
    },
];

#[derive(Default)]
struct Args {
    autostart_test: bool,
    hold_secs: u64,
    clobber: bool,
    probe: Option<String>,
}

impl Args {
    fn parse() -> Self {
        let mut args = Self::default();
        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--autostart-test" => args.autostart_test = true,
                "--hold" => args.hold_secs = it.next().and_then(|v| v.parse().ok()).unwrap_or(0),
                "--clobber" => args.clobber = true,
                "--probe" => args.probe = it.next(),
                other => println!("[m2] ignoring unknown argument {other}"),
            }
        }
        args
    }
}

/// Runs the harness; the return value is the process exit code.
pub fn run() -> i32 {
    let args = Args::parse();
    if args.autostart_test {
        return autostart_test(args.hold_secs);
    }
    sys::ensure_message_queue();

    // The single-instance claim comes first so a second launch exits without touching hotkeys.
    let _instance = match SingleInstance::acquire(APP_ID, || println!("[m2] raised")) {
        Ok(Acquired::First(guard)) => {
            println!("[m2] instance=first");
            guard
        }
        Ok(Acquired::Second) => {
            println!("[m2] second instance");
            return 0;
        }
        Err(e) => {
            println!("[m2] instance=ERROR {e}");
            return 1;
        }
    };

    report_system();
    report_dpapi();
    report_chords();

    let thread_id = unsafe { GetCurrentThreadId() };
    let hotkey_service = match HotkeyService::start(move |id| {
        // SAFETY: plain integers; the target thread has a queue (ensure_message_queue).
        let _ = unsafe { PostThreadMessageW(thread_id, WM_APP, WPARAM(id as usize), LPARAM(0)) };
    }) {
        Ok(service) => service,
        Err(e) => {
            println!("[m2] hotkeys=ERROR service did not start: {e}");
            return 1;
        }
    };
    // The tracker needs this thread's message loop (run below), and is not Send.
    let tracker = TargetTracker::install(|| {});
    let mut clipboard = match WinClipboard::new() {
        Ok(c) => c,
        Err(e) => {
            println!("[m2] clipboard=ERROR {e}");
            return 1;
        }
    };
    let mut keyboard = WinKeyboard::new();
    let mut timing = ClobberTiming::new(args.clobber);

    report_hotkeys(&hotkey_service, args.probe.as_deref());
    if args.clobber {
        println!(
            "[m2] --clobber: another clipboard owner overwrites the clipboard during each paste"
        );
    }
    println!(
        "[m2] ready: focus the target app, then press a chord (the chord keys stay held, which exercises the Alt mask)"
    );

    loop {
        let mut msg = MSG::default();
        // SAFETY: `msg` is a valid out-pointer.
        let got = unsafe { GetMessageW(&raw mut msg, None, 0, 0) };
        if got.0 <= 0 {
            break;
        }
        if msg.hwnd.0.is_null() && msg.message == WM_APP {
            let id = msg.wParam.0 as u32;
            let action = if id == PROBE_ID {
                println!("[m2] probe hotkey pressed");
                continue;
            } else {
                HOTKEYS.iter().find(|h| h.id == id).map(|h| h.action)
            };
            match action {
                Some(Action::Use(item, mode)) => {
                    use_item(
                        &tracker,
                        &mut clipboard,
                        &mut keyboard,
                        &mut timing,
                        item,
                        mode,
                    );
                }
                Some(Action::Info) => target_info(&tracker),
                Some(Action::Quit) => {
                    // SAFETY: no preconditions.
                    unsafe { PostQuitMessage(0) };
                }
                None => println!("[m2] unknown hotkey id {id}"),
            }
            continue;
        }
        // SAFETY: `msg` came from GetMessageW.
        unsafe {
            let _ = TranslateMessage(&raw const msg);
            DispatchMessageW(&raw const msg);
        }
    }

    drop(hotkey_service);
    println!("[m2] quit: hotkeys released");
    drop(tracker);
    0
}

fn report_system() {
    match sys::windows_version() {
        Some((major, minor, build)) => println!("[m2] windows={major}.{minor} build={build}"),
        None => println!("[m2] windows=unknown"),
    }
    match sys::is_elevated() {
        Some(true) => println!("[m2] integrity=elevated"),
        Some(false) => println!("[m2] integrity=not elevated"),
        None => println!("[m2] integrity=unknown"),
    }
}

fn report_dpapi() {
    let secret = b"m2 secret value";
    let entropy = b"m2 entropy A";
    let outcome = (|| -> Result<(bool, bool, bool), String> {
        let blob = dpapi::protect(secret, entropy).map_err(|e| format!("protect: {e}"))?;
        let hides_plaintext = !blob.windows(secret.len()).any(|w| w == secret);
        let back = dpapi::unprotect(&blob, entropy).map_err(|e| format!("unprotect: {e}"))?;
        let round_trip = back.as_slice() == secret;
        let wrong_rejected = dpapi::unprotect(&blob, b"m2 entropy B").is_err();
        Ok((hides_plaintext, round_trip, wrong_rejected))
    })();
    match outcome {
        Ok((hidden, round_trip, rejected)) => {
            let ok = hidden && round_trip && rejected;
            println!(
                "[m2] dpapi={} round-trip={round_trip} blob-hides-plaintext={hidden} wrong-entropy-rejected={rejected}",
                if ok { "ok" } else { "FAIL" }
            );
        }
        Err(e) => println!("[m2] dpapi=FAIL {e}"),
    }
}

fn report_chords() {
    let samples = [" ctrl + ALT + 1 ", "Win+Shift+F5", "Shift+A", "A"];
    let parts: Vec<String> = samples
        .iter()
        .map(|s| match s.parse::<Chord>() {
            Ok(chord) => {
                let (mods, vk) = chord.to_win32();
                format!("{s:?} -> {chord} (mods={mods:#06x} vk={vk:#04x})")
            }
            Err(e) => format!("{s:?} -> rejected ({e})"),
        })
        .collect();
    println!("[m2] chords: {}", parts.join("; "));
}

fn report_hotkeys(service: &HotkeyService, probe: Option<&str>) {
    let mut lines = Vec::new();
    let mut registered = 0;
    let mut in_use = 0;
    let mut other_errors = 0;
    let mut register = |id: u32, text: &str, what: &str| match text.parse::<Chord>() {
        Ok(chord) => match service.register(id, chord) {
            Ok(()) => {
                registered += 1;
                lines.push(format!("[m2] hotkey {chord} registered: {what}"));
            }
            Err(HotkeyError::InUse) => {
                in_use += 1;
                lines.push(format!(
                    "[m2] hotkey {chord} InUse (another app owns it): {what}"
                ));
            }
            Err(e) => {
                other_errors += 1;
                lines.push(format!("[m2] hotkey {chord} ERROR {e:?}: {what}"));
            }
        },
        Err(e) => {
            other_errors += 1;
            lines.push(format!("[m2] hotkey {text:?} ERROR chord rejected: {e}"));
        }
    };
    for hotkey in &HOTKEYS {
        register(hotkey.id, hotkey.chord, hotkey.what);
    }
    if let Some(text) = probe {
        register(PROBE_ID, text, "probe (--probe)");
    }
    println!("[m2] hotkeys registered={registered} in-use={in_use} errors={other_errors}");
    for line in lines {
        println!("{line}");
    }
}

fn use_item(
    tracker: &TargetTracker,
    clipboard: &mut WinClipboard,
    keyboard: &mut WinKeyboard,
    timing: &mut ClobberTiming,
    item: Item,
    mode: UseMode,
) {
    let target = tracker.current();
    let route = route::decide(target.as_ref(), &Policy::default());
    let payload = item.payload();
    let started = Instant::now();
    let result = pipeline::use_item(
        &mut Env {
            clipboard,
            keyboard,
            timing,
        },
        &payload,
        mode,
        &route,
    );
    let ms = started.elapsed().as_millis();
    let outcome = match result {
        Ok(outcome) => format!("{outcome:?}"),
        Err(e) => format!("error({e})"),
    };
    let (app, focus) = target.as_ref().map_or_else(
        || ("none".to_owned(), "n/a".to_owned()),
        |t| (t.app.clone(), format!("{:?}", t.focus)),
    );
    println!(
        "[m2] use={mode:?} target={app} focus={focus} route={} outcome={outcome} ms={ms} item={}",
        route_label(&route),
        item.label()
    );
}

fn route_label(route: &Route) -> String {
    match route {
        Route::Paste { key } => format!("paste({key:?})"),
        Route::Copy(reason) => {
            let why = match reason {
                CopyReason::NoTarget => "NoTarget",
                CopyReason::Elevated { .. } => "Elevated",
                CopyReason::NothingFocused { .. } => "NothingFocused",
                CopyReason::Excluded { .. } => "Excluded",
            };
            format!("copy({why})")
        }
    }
}

/// Target info for Ctrl+Alt+7. The window title is private: only its length is printed.
fn target_info(tracker: &TargetTracker) {
    let own = sys::is_elevated().map_or("unknown".to_owned(), |e| e.to_string());
    match tracker.current() {
        Some(t) => println!(
            "[m2] target app={} title_len={} elevated={} focus={:?} self_elevated={own}",
            t.app,
            t.title.chars().count(),
            t.elevated,
            t.focus
        ),
        None => println!("[m2] target none self_elevated={own}"),
    }
}

fn autostart_test(hold_secs: u64) -> i32 {
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(e) => {
            println!("[m2] autostart=FAIL no exe path: {e}");
            return 1;
        }
    };
    let set = |enabled: bool| -> Result<bool, String> {
        autostart::set_enabled(Path::new(&exe), "", enabled).map_err(|e| e.to_string())?;
        Ok(autostart::is_enabled())
    };
    let on = set(true);
    println!("[m2] autostart set on -> is_enabled={on:?} (expect Ok(true))");
    if hold_secs > 0 {
        println!(
            "[m2] autostart on for {hold_secs} s: check HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run"
        );
        std::thread::sleep(Duration::from_secs(hold_secs));
    }
    let off = set(false);
    println!("[m2] autostart set off -> is_enabled={off:?} (expect Ok(false))");
    let ok = on == Ok(true) && off == Ok(false);
    println!("[m2] autostart={}", if ok { "ok" } else { "FAIL" });
    i32::from(!ok)
}

/// The real clock, plus an optional "another app" that overwrites the clipboard just after the
/// Target has pasted (the pipeline's `SETTLE` sleep), to check that a restore never clobbers
/// newer content.
struct ClobberTiming {
    inner: SystemTiming,
    other_app: Option<WinClipboard>,
}

impl ClobberTiming {
    fn new(enabled: bool) -> Self {
        let other_app = enabled.then(|| WinClipboard::new().ok()).flatten();
        Self {
            inner: SystemTiming::new(),
            other_app,
        }
    }
}

impl Timing for ClobberTiming {
    fn now(&mut self) -> Duration {
        self.inner.now()
    }

    fn sleep(&mut self, duration: Duration) {
        if duration == SETTLE
            && let Some(other) = &mut self.other_app
        {
            use pipeline::Clipboard as _;
            let wrote = other.write_persistent(&items::clobber_payload()).is_ok();
            println!("[m2] clobber: another owner wrote newer content ok={wrote}");
        }
        self.inner.sleep(duration);
    }
}
