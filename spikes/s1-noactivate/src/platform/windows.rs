//! Windows implementation: non-activating panel, Target tracking, clipboard + `SendInput` paste.

use std::cell::{Cell, OnceCell, RefCell};
use std::ffi::c_void;
use std::time::Duration;

use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::winit_030::winit::window::WindowAttributes;
use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, Timer};
use windows::core::{w, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TokenIntegrityLevel,
    TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL, VK_LMENU,
    VK_LSHIFT, VK_LWIN, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_V,
};
use windows::Win32::UI::Shell::{
    DefSubclassProc, SHQueryUserNotificationState, SetWindowSubclass, QUNS_BUSY,
    QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, GetWindowLongPtrW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindow, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos,
    EVENT_SYSTEM_FOREGROUND, GUITHREADINFO, GWL_EXSTYLE, MA_NOACTIVATE, STYLESTRUCT,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS, WM_MOUSEACTIVATE, WM_STYLECHANGING, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW,
};

use super::Route;

/// Forces the panel's extended style: never activated by clicks, no taskbar button, no Alt+Tab
/// entry. winit always adds WS_EX_APPWINDOW to top-level windows, which would override
/// WS_EX_TOOLWINDOW, so it is stripped.
fn panel_ex_style(style: u32) -> u32 {
    (style | WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) & !WS_EX_APPWINDOW.0
}

/// How long the target gets to read the clipboard before we restore the previous contents.
const RESTORE_DELAY: Duration = Duration::from_millis(300);
/// Time for a re-activated target to restore its focused control before keys arrive.
const REFOCUS_DELAY: Duration = Duration::from_millis(50);

thread_local! {
    // Everything below lives on the UI thread, which owns the panel window and the WinEvent hook.
    static PANEL: Cell<HWND> = Cell::new(HWND::default());
    static TARGET: Cell<HWND> = Cell::new(HWND::default());
    /// Route captured when the panel activated itself for text entry: while the panel is
    /// foreground the target has lost focus, so its caret can no longer be observed.
    static ENTRY_ROUTE: RefCell<Option<Route>> = const { RefCell::new(None) };
    static ON_FOREGROUND_CHANGE: OnceCell<Box<dyn Fn()>> = const { OnceCell::new() };
}

/// Applied by winit when it creates the window. `with_active(false)` makes winit show the
/// window with `SW_SHOWNOACTIVATE` (now and on every later re-show).
pub fn window_attributes(attributes: WindowAttributes) -> WindowAttributes {
    attributes.with_active(false).with_skip_taskbar(true)
}

/// Makes the panel non-activating and starts Target tracking. Slint creates the winit window
/// lazily once the event loop runs, so the HWND is only available asynchronously.
pub fn install<C: ComponentHandle + 'static>(ui: &C, on_foreground_change: impl Fn() + 'static) {
    ON_FOREGROUND_CHANGE.with(|cell| cell.set(Box::new(on_foreground_change)).ok());
    let foreground = unsafe { GetForegroundWindow() };
    if !is_own_window(foreground) {
        TARGET.set(foreground);
    }

    let ui = ui.as_weak();
    slint::spawn_local(async move {
        let Some(ui) = ui.upgrade() else {
            return;
        };
        let Ok(winit_window) = ui.window().winit_window().await else {
            return;
        };
        let Ok(handle) = winit_window.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return;
        };
        let hwnd = HWND(handle.hwnd.get() as *mut c_void);
        PANEL.set(hwnd);
        unsafe {
            // Subclass first, so the style guard is in place before we set the style.
            let _ = SetWindowSubclass(hwnd, Some(panel_subclass_proc), 1, 0);
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, panel_ex_style(ex_style) as isize);
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
            // Out-of-context: the callback runs on this thread from its message loop.
            // Skipping our own process means activating the panel never becomes the "target".
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(foreground_hook),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );
        }
    })
    .expect("Slint event loop is available");
}

/// Subclass proc on the panel HWND, called before winit's own window proc.
unsafe extern "system" fn panel_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        // Belt and braces with WS_EX_NOACTIVATE: a click must not activate the panel.
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        // winit rewrites GWL_EXSTYLE from its own flags whenever Slint changes a window
        // property (title, decorations, level, visibility...), dropping our bits. Re-add them.
        WM_STYLECHANGING if wparam.0 as i32 == GWL_EXSTYLE.0 => {
            let style = unsafe { &mut *(lparam.0 as *mut STYLESTRUCT) };
            style.styleNew = panel_ex_style(style.styleNew);
        }
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

unsafe extern "system" fn foreground_hook(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if hwnd.is_invalid() {
        return;
    }
    TARGET.set(hwnd);
    ON_FOREGROUND_CHANGE.with(|cell| cell.get().map(|callback| callback()));
}

fn is_own_window(hwnd: HWND) -> bool {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid == std::process::id()
}

fn panel_is_foreground() -> bool {
    let panel = PANEL.get();
    !panel.is_invalid() && unsafe { GetForegroundWindow() } == panel
}

pub fn route() -> Route {
    if panel_is_foreground() {
        if let Some(route) = ENTRY_ROUTE.with_borrow(Clone::clone) {
            return route;
        }
    }
    live_route()
}

fn live_route() -> Route {
    let target = TARGET.get();
    if target.is_invalid() || !unsafe { IsWindow(Some(target)) }.as_bool() {
        return Route::Copy {
            reason: "no target window".into(),
        };
    }
    let mut pid = 0;
    let thread = unsafe { GetWindowThreadProcessId(target, Some(&mut pid)) };
    let app = process_name(pid).unwrap_or_else(|| "unknown app".into());
    if !can_send_input_to(pid) {
        return Route::Copy {
            reason: format!("{app} runs as admin; Snipdeck does not"),
        };
    }
    let mut info = GUITHREADINFO {
        cbSize: size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetGUIThreadInfo(thread, &mut info) }.is_err() || info.hwndFocus.is_invalid() {
        return Route::Copy {
            reason: format!("nothing focused in {app}"),
        };
    }
    let title = window_title(target);
    // Many apps (Win32 edit, Word, ...) own a system caret; custom-drawn ones may not. The spike
    // still pastes when only a focus window exists, but labels it so testing can tell them apart.
    let caret_note = if info.hwndCaret.is_invalid() {
        " (no caret seen)"
    } else {
        ""
    };
    Route::Paste {
        label: format!("{app} · {title}{caret_note}"),
    }
}

fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

fn process_name(pid: u32) -> Option<String> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    };
    let _ = unsafe { CloseHandle(process) };
    result.ok()?;
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    path.rsplit('\\').next().map(str::to_owned)
}

/// UIPI drops injected input aimed at a process with a higher integrity level than ours
/// (typically an elevated app), and `SendInput` does not report it. So check up front.
fn can_send_input_to(pid: u32) -> bool {
    let own = integrity_level(unsafe { GetCurrentProcess() });
    let target = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
        .ok()
        .and_then(|process| {
            let level = integrity_level(process);
            let _ = unsafe { CloseHandle(process) };
            level
        });
    // An unreadable token is what an elevated process looks like from a non-elevated one.
    matches!((own, target), (Some(own), Some(target)) if target <= own)
}

/// The mandatory integrity RID of a process token (e.g. 0x2000 medium, 0x3000 high).
fn integrity_level(process: HANDLE) -> Option<u32> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }.ok()?;
    // u64 storage keeps the TOKEN_MANDATORY_LABEL pointer field aligned.
    let mut buf = [0u64; 16];
    let mut len = 0;
    let result = unsafe {
        GetTokenInformation(
            token,
            TokenIntegrityLevel,
            Some(buf.as_mut_ptr().cast()),
            size_of_val(&buf) as u32,
            &mut len,
        )
    };
    let _ = unsafe { CloseHandle(token) };
    result.ok()?;
    let label = unsafe { &*buf.as_ptr().cast::<TOKEN_MANDATORY_LABEL>() };
    unsafe {
        let count = *GetSidSubAuthorityCount(label.Label.Sid);
        Some(*GetSidSubAuthority(label.Label.Sid, u32::from(count) - 1))
    }
}

pub fn fullscreen_in_front() -> bool {
    matches!(
        unsafe { SHQueryUserNotificationState() },
        Ok(QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE)
    )
}

pub fn shift_held() -> bool {
    key_down(VK_SHIFT)
}

fn key_down(vk: VIRTUAL_KEY) -> bool {
    (unsafe { GetAsyncKeyState(i32::from(vk.0)) }) < 0
}

/// The search field got focus: activate the panel so keystrokes reach it. Allowed by the
/// foreground lock rules because our process received the last input event (the click).
pub fn begin_text_entry() {
    ENTRY_ROUTE.set(Some(live_route()));
    let _ = unsafe { SetForegroundWindow(PANEL.get()) };
}

/// The search field lost focus: hand the foreground back to the target if we still hold it.
/// If the user clicked into another app meanwhile, that app stays in front.
pub fn end_text_entry() {
    if panel_is_foreground() {
        let _ = unsafe { SetForegroundWindow(TARGET.get()) };
    }
}

pub fn use_text(text: &str, copy_only: bool, done: impl FnOnce(String) + 'static) {
    let label = match (copy_only, route()) {
        (false, Route::Paste { label }) => label,
        (true, _) => return done(copy(text, "Copied (Shift+click)".into())),
        (false, Route::Copy { reason }) => return done(copy(text, format!("Copied: {reason}"))),
    };
    // A failed snapshot only means we cannot restore afterwards; pasting still works.
    let previous = clipboard_snapshot().unwrap_or_default();
    let mut formats = vec![(CF_UNICODETEXT.0.into(), utf16_bytes(text))];
    formats.extend(private_markers());
    let Some(sequence) = clipboard_write(&formats) else {
        return done("Couldn't open the clipboard".into());
    };

    // Text entry case: the panel is foreground, so give it back to the target first.
    let delay = if panel_is_foreground() {
        let _ = unsafe { SetForegroundWindow(TARGET.get()) };
        REFOCUS_DELAY
    } else {
        Duration::ZERO
    };
    Timer::single_shot(delay, move || {
        if !send_ctrl_v() {
            return done(format!("Couldn't paste into {label}. Copied instead."));
        }
        Timer::single_shot(RESTORE_DELAY, move || {
            // Only restore if nobody (e.g. the user) has put something newer on the clipboard.
            if unsafe { GetClipboardSequenceNumber() } == sequence {
                let mut formats = previous;
                formats.extend(private_markers());
                clipboard_write(&formats);
            }
            done(format!("Pasted into {label}"));
        });
    });
}

/// Plain copy: the item stays on the clipboard and may appear in clipboard history.
fn copy(text: &str, message: String) -> String {
    match clipboard_write(&[(CF_UNICODETEXT.0.into(), utf16_bytes(text))]) {
        Some(_) => message,
        None => "Couldn't open the clipboard".into(),
    }
}

fn utf16_bytes(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// Formats that keep a transient paste out of clipboard managers, Win+V history and cloud sync.
fn private_markers() -> [(u32, Vec<u8>); 3] {
    let zero = 0u32.to_le_bytes().to_vec();
    unsafe {
        [
            (
                RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing")),
                zero.clone(),
            ),
            (
                RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory")),
                zero.clone(),
            ),
            (
                RegisterClipboardFormatW(w!("CanUploadToCloudClipboard")),
                zero,
            ),
        ]
    }
}

/// Ctrl+V via `SendInput`, coping with modifiers the user is physically holding.
fn send_ctrl_v() -> bool {
    let ctrl_held = key_down(VK_LCONTROL) || key_down(VK_RCONTROL);
    let mut keys = Vec::new();
    if !ctrl_held {
        keys.push(key_input(VK_CONTROL, false));
    }
    // Release held Shift/Alt/Win so the target sees exactly Ctrl+V. Ctrl is already down at this
    // point, so a lone Alt-up cannot open the menu bar and a lone Win-up cannot open Start.
    // We do not re-press them afterwards; the user's eventual physical key-up is harmless.
    for vk in [VK_LSHIFT, VK_RSHIFT, VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN] {
        if key_down(vk) {
            keys.push(key_input(vk, true));
        }
    }
    keys.push(key_input(VK_V, false));
    keys.push(key_input(VK_V, true));
    if !ctrl_held {
        keys.push(key_input(VK_CONTROL, true));
    }
    // Note: a UIPI block is NOT reported here (it still returns the full count).
    unsafe { SendInput(&keys, size_of::<INPUT>() as i32) as usize == keys.len() }
}

fn key_input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let mut flags = KEYBD_EVENT_FLAGS::default();
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    if matches!(vk, VK_RMENU | VK_LWIN | VK_RWIN) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                dwFlags: flags,
                ..Default::default()
            },
        },
    }
}

/// Holds the clipboard open; closes it on drop.
struct ClipboardGuard;

impl ClipboardGuard {
    fn open() -> Option<Self> {
        // Another process may hold the clipboard briefly; retry for up to ~50 ms.
        // The owner must be a real window: with NULL, EmptyClipboard makes SetClipboardData fail.
        for _ in 0..10 {
            if unsafe { OpenClipboard(Some(PANEL.get())) }.is_ok() {
                return Some(ClipboardGuard);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        None
    }
}

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

/// Copies every HGLOBAL-backed format currently on the clipboard. GDI-handle formats are
/// skipped; Windows re-synthesises CF_BITMAP/CF_ENHMETAFILE from CF_DIB/CF_METAFILEPICT
/// data where possible. Note: this forces any delay-rendered formats to render.
fn clipboard_snapshot() -> Option<Vec<(u32, Vec<u8>)>> {
    let _guard = ClipboardGuard::open()?;
    let mut formats = Vec::new();
    let mut format = 0;
    loop {
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            break;
        }
        // CF_BITMAP, CF_METAFILEPICT, CF_PALETTE, CF_ENHMETAFILE, CF_OWNERDISPLAY, CF_DSP* and
        // the private/GDI-object ranges are not plain HGLOBAL memory.
        if matches!(
            format,
            2 | 3 | 9 | 14 | 0x80 | 0x82 | 0x83 | 0x8E | 0x200..=0x3FF
        ) {
            continue;
        }
        let Ok(handle) = (unsafe { GetClipboardData(format) }) else {
            continue;
        };
        let memory = HGLOBAL(handle.0);
        let data = unsafe { GlobalLock(memory) };
        if data.is_null() {
            continue;
        }
        let bytes = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), GlobalSize(memory)) };
        formats.push((format, bytes.to_vec()));
        let _ = unsafe { GlobalUnlock(memory) };
    }
    Some(formats)
}

/// Replaces the clipboard with `formats`; returns the resulting clipboard sequence number.
fn clipboard_write(formats: &[(u32, Vec<u8>)]) -> Option<u32> {
    {
        let _guard = ClipboardGuard::open()?;
        unsafe { EmptyClipboard() }.ok()?;
        for (format, bytes) in formats {
            set_clipboard_bytes(*format, bytes);
        }
    }
    Some(unsafe { GetClipboardSequenceNumber() })
}

fn set_clipboard_bytes(format: u32, bytes: &[u8]) {
    unsafe {
        let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) else {
            return;
        };
        let data = GlobalLock(memory);
        if !data.is_null() {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), data.cast::<u8>(), bytes.len());
            let _ = GlobalUnlock(memory);
            // On success the system owns the memory; otherwise we must free it.
            if SetClipboardData(format, Some(HANDLE(memory.0))).is_ok() {
                return;
            }
        }
        let _ = GlobalFree(Some(memory));
    }
}
