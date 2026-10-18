//! The notification-area icon (`Shell_NotifyIconW`, `NOTIFYICON_VERSION_4`).
//!
//! [`WinTray`] owns a hidden window (`hidden_window.rs`) on its own thread. The
//! shell sends the icon's mouse and keyboard notifications to that window; the decisions about
//! them (left click toggles the Panel, right click opens the menu, which entry means what) are
//! in the portable [`TrayModel`], so this file only talks to Windows:
//!
//! - **Add / remove / retitle** the icon, from commands posted by [`Tray`] calls (a channel plus a
//!   window message to wake the thread; the caller waits for the answer, like the hotkey service).
//! - **`TaskbarCreated`**: Explorer restarted and dropped every icon; the icon is added again if
//!   it is wanted. A failed [`Tray::show`] (no taskbar yet, at logon) keeps the icon wanted, so
//!   this also covers a start-up before Explorer. Only one attempt is made per announcement.
//! - **Left click** (`NIN_SELECT` / `NIN_KEYSELECT`) gives [`TrayEvent::TogglePanel`].
//! - **Right click** (`WM_CONTEXTMENU`) shows a popup menu with `TrackPopupMenu` at the click
//!   position; the chosen entry becomes a [`TrayEvent`]. The window is made foreground first and
//!   `WM_NULL` is posted afterwards, as the menu documentation requires, otherwise the menu does
//!   not close when the user clicks elsewhere.
//!
//! The app also hears whether the icon is in the notification area ([`TrayEvent::IconPresent`]):
//! after a successful add, after Explorer dropped it, and after it was added again. Closing the
//! Panel hides it only while that is `true`.
//!
//! Events are delivered by calling `on_event` **on the tray thread**; it must return quickly
//! (forward to the UI thread with `slint::invoke_from_event_loop`), and a panic in it is caught.
//!
//! Known limits: the popup menu is a classic Win32 menu, so it stays light when Windows is in
//! dark mode; the icon is the executable's resource 1 if it has one, otherwise the generic
//! application icon. **Unverified on Windows** (`tools/windows-test/tests/m3-app.json`).

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use windows::Win32::Foundation::{GetLastError, HANDLE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
    NOTIFY_ICON_DATA_FLAGS, NOTIFY_ICON_MESSAGE, NOTIFYICON_VERSION_4, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, ChangeWindowMessageFilterEx, CreatePopupMenu, DestroyMenu, GetCursorPos, HICON,
    HMENU, IDI_APPLICATION, IMAGE_ICON, LR_SHARED, LoadImageW, MF_CHECKED, MF_SEPARATOR, MF_STRING,
    MSGFLT_ALLOW, PostMessageW, RegisterWindowMessageW, SetForegroundWindow, TPM_BOTTOMALIGN,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP, WM_CLOSE, WM_CREATE, WM_DESTROY,
    WM_NULL,
};
use windows::core::{HSTRING, PCWSTR, w};

use super::dpi;
use super::hidden_window::{HiddenWindow, WindowHandler};
use crate::error::Error;
use crate::tray::{
    MenuLine, Reaction, Tray, TrayEntry, TrayEvent, TrayModel, classify, decode_anchor,
    decode_callback,
};

/// Wakes the window to drain the command channel.
const WM_COMMANDS: u32 = WM_APP + 1;
/// The icon's callback message (`uCallbackMessage`).
const WM_TRAY: u32 = WM_APP + 2;
/// The icon's id within this window.
const ICON_ID: u32 = 1;
/// How long a caller waits for the tray thread before reporting it unresponsive.
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

type Reply = Sender<Result<(), Error>>;

enum Command {
    Show(Reply),
    Hide(Reply),
    Tooltip(String, Reply),
    Paused(bool, Reply),
}

/// The Windows tray icon. Dropping it removes the icon and stops its thread.
pub struct WinTray {
    commands: Sender<Command>,
    // Drops after `commands`, closing the window and joining the thread.
    window: HiddenWindow,
}

impl std::fmt::Debug for WinTray {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WinTray").finish_non_exhaustive()
    }
}

impl WinTray {
    /// Starts the tray thread. The icon is not shown until [`Tray::show`].
    ///
    /// `on_event` is called on the tray thread with each [`TrayEvent`].
    ///
    /// # Errors
    ///
    /// [`Error::Unavailable`] if the thread cannot start; [`Error::Os`] if the window cannot be
    /// created.
    pub fn start(
        tooltip: &str,
        on_event: impl Fn(TrayEvent) + Send + 'static,
    ) -> Result<Self, Error> {
        let (commands, command_rx) = mpsc::channel();
        let tooltip = tooltip.to_owned();
        let window = HiddenWindow::spawn("snipdeck-tray", "SnipdeckTrayWindow", move || {
            TrayHandler::new(&tooltip, command_rx, Box::new(on_event))
        })?;
        Ok(Self { commands, window })
    }

    fn call(&self, make: impl FnOnce(Reply) -> Command) -> Result<(), Error> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.commands
            .send(make(reply_tx))
            .map_err(|_| Error::Unavailable("the tray service is not running"))?;
        if !self.window.post(WM_COMMANDS) {
            return Err(Error::Unavailable("the tray service is not running"));
        }
        reply_rx
            .recv_timeout(REPLY_TIMEOUT)
            .map_err(|_| Error::Unavailable("the tray service did not answer"))?
    }
}

impl Tray for WinTray {
    fn show(&mut self) -> Result<(), Error> {
        self.call(Command::Show)
    }

    fn hide(&mut self) -> Result<(), Error> {
        self.call(Command::Hide)
    }

    fn set_tooltip(&mut self, text: &str) -> Result<(), Error> {
        self.call(|reply| Command::Tooltip(text.to_owned(), reply))
    }

    fn set_hotkeys_paused(&mut self, paused: bool) -> Result<(), Error> {
        self.call(|reply| Command::Paused(paused, reply))
    }
}

/// Lives on the tray thread; see the module documentation.
struct TrayHandler {
    model: RefCell<TrayModel>,
    commands: Receiver<Command>,
    on_event: Box<dyn Fn(TrayEvent) + Send>,
    taskbar_created: u32,
    icon: HICON,
    /// Whether the shell currently has our icon.
    added: Cell<bool>,
}

impl TrayHandler {
    fn new(
        tooltip: &str,
        commands: Receiver<Command>,
        on_event: Box<dyn Fn(TrayEvent) + Send>,
    ) -> Self {
        // SAFETY: registers (or looks up) a window message by name; the string is a static,
        // NUL-terminated wide string. 0 on failure, which then never matches a message.
        let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
        Self {
            model: RefCell::new(TrayModel::new(tooltip)),
            commands,
            on_event,
            taskbar_created,
            icon: load_icon(),
            added: Cell::new(false),
        }
    }

    /// An elevated app only hears `TaskbarCreated` (sent by the non-elevated Explorer) if it lets
    /// the message through the integrity filter (UIPI). A failure leaves the filter as it was:
    /// the icon is then not restored after an Explorer restart, which is the situation without
    /// this call, so the result is ignored.
    fn allow_taskbar_created(&self, hwnd: HWND) {
        if self.taskbar_created == 0 {
            return;
        }
        // SAFETY: `hwnd` is our own window, on this thread; the message id was registered in
        // `new`; the optional out-struct is not asked for.
        let _ =
            unsafe { ChangeWindowMessageFilterEx(hwnd, self.taskbar_created, MSGFLT_ALLOW, None) };
    }

    fn emit(&self, event: TrayEvent) {
        let _ = catch_unwind(AssertUnwindSafe(|| (self.on_event)(event)));
    }

    /// Records whether the shell has our icon and tells the app, once per change (the app closes
    /// the Panel to the tray only while there is an icon to bring it back).
    fn set_added(&self, added: bool) {
        if self.added.replace(added) != added {
            self.emit(TrayEvent::IconPresent(added));
        }
    }

    fn data(&self, hwnd: HWND, flags: NOTIFY_ICON_DATA_FLAGS) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: u32::try_from(std::mem::size_of::<NOTIFYICONDATAW>()).unwrap_or(0),
            hWnd: hwnd,
            uID: ICON_ID,
            uFlags: flags,
            uCallbackMessage: WM_TRAY,
            hIcon: self.icon,
            ..Default::default()
        };
        // `tooltip_utf16` leaves room for the terminating NUL the zeroed array already has.
        for (slot, unit) in data
            .szTip
            .iter_mut()
            .zip(self.model.borrow().tooltip_utf16())
        {
            *slot = unit;
        }
        data
    }

    fn notify(
        op: &'static str,
        message: NOTIFY_ICON_MESSAGE,
        data: &NOTIFYICONDATAW,
    ) -> Result<(), Error> {
        // SAFETY: `data` is a fully initialised `NOTIFYICONDATAW` with the right `cbSize`, valid
        // for the call.
        if unsafe { Shell_NotifyIconW(message, data) }.as_bool() {
            Ok(())
        } else {
            // SAFETY: reads this thread's last error, set (if at all) by the call just above.
            let code = unsafe { GetLastError() };
            Err(Error::Os { op, code: code.0 })
        }
    }

    fn add_icon(&self, hwnd: HWND) -> Result<(), Error> {
        let flags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        let mut data = self.data(hwnd, flags);
        // An earlier icon of ours may be left over (Explorer crashed without announcing): the
        // add then fails, so remove first and ignore the result.
        let _ = Self::notify("Shell_NotifyIconW(NIM_DELETE)", NIM_DELETE, &data);
        Self::notify("Shell_NotifyIconW(NIM_ADD)", NIM_ADD, &data)?;
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        if let Err(error) = Self::notify("Shell_NotifyIconW(NIM_SETVERSION)", NIM_SETVERSION, &data)
        {
            // Without version 4 the callbacks mean something else: do not leave the icon half set up.
            let _ = Self::notify("Shell_NotifyIconW(NIM_DELETE)", NIM_DELETE, &data);
            return Err(error);
        }
        self.set_added(true);
        Ok(())
    }

    fn remove_icon(&self, hwnd: HWND) -> Result<(), Error> {
        if !self.added.get() {
            return Ok(());
        }
        self.set_added(false);
        Self::notify(
            "Shell_NotifyIconW(NIM_DELETE)",
            NIM_DELETE,
            &self.data(hwnd, NOTIFY_ICON_DATA_FLAGS(0)),
        )
    }

    fn serve(&self, hwnd: HWND, command: Command) {
        // A dropped reply receiver (the caller timed out) is not an error here.
        match command {
            Command::Show(reply) => {
                self.model.borrow_mut().show();
                let result = if self.added.get() {
                    Ok(())
                } else {
                    self.add_icon(hwnd)
                };
                let _ = reply.send(result);
            }
            Command::Hide(reply) => {
                self.model.borrow_mut().hide();
                let _ = reply.send(self.remove_icon(hwnd));
            }
            Command::Tooltip(text, reply) => {
                self.model.borrow_mut().set_tooltip(&text);
                let result = if self.added.get() {
                    Self::notify(
                        "Shell_NotifyIconW(NIM_MODIFY)",
                        NIM_MODIFY,
                        &self.data(hwnd, NIF_TIP | NIF_SHOWTIP),
                    )
                } else {
                    Ok(())
                };
                let _ = reply.send(result);
            }
            Command::Paused(paused, reply) => {
                self.model.borrow_mut().set_hotkeys_paused(paused);
                let _ = reply.send(Ok(()));
            }
        }
    }

    /// A callback from the shell for our icon.
    fn on_callback(&self, hwnd: HWND, wparam: WPARAM, lparam: LPARAM) {
        let (code, icon_id) = decode_callback(lparam.0);
        if icon_id != ICON_ID {
            return;
        }
        let Some(gesture) = classify(code) else {
            return;
        };
        // Decide first and drop the borrow: the menu below runs a modal message loop.
        let reaction = self.model.borrow_mut().gesture(gesture);
        match reaction {
            Some(Reaction::Event(event)) => self.emit(event),
            Some(Reaction::PopupMenu(lines)) => {
                if let Some(entry) = popup_menu(hwnd, &lines, wparam.0) {
                    let event = self.model.borrow_mut().pick(entry);
                    self.emit(event);
                }
            }
            None => {}
        }
    }
}

impl WindowHandler for TrayHandler {
    fn message(&self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        if msg == WM_COMMANDS {
            while let Ok(command) = self.commands.try_recv() {
                self.serve(hwnd, command);
            }
            return Some(LRESULT(0));
        }
        if msg == WM_TRAY {
            self.on_callback(hwnd, wparam, lparam);
            return Some(LRESULT(0));
        }
        if msg == self.taskbar_created && msg != 0 {
            // Every icon was dropped with the old taskbar.
            self.set_added(false);
            if self.model.borrow().on_taskbar_created() {
                // One attempt; the shell may not accept icons for a moment yet, and the next
                // `TaskbarCreated` or `show` tries again.
                let _ = self.add_icon(hwnd);
            }
            return Some(LRESULT(0));
        }
        if msg == WM_CREATE {
            self.allow_taskbar_created(hwnd);
        }
        if msg == WM_CLOSE {
            // Something outside asked the window to close (`taskkill` without `/f`, the Restart
            // Manager). The window itself stays (see `hidden_window.rs`); the app is asked to quit.
            self.emit(TrayEvent::Quit);
            return Some(LRESULT(0));
        }
        if msg == WM_DESTROY {
            let _ = self.remove_icon(hwnd);
        }
        None
    }
}

/// The small icon: the executable's resource 1, else the generic application icon.
fn load_icon() -> HICON {
    // The size at the system DPI, whatever the thread's awareness is (a stretched 16-px icon is
    // blurry at 150 % / 200 %).
    let (cx, cy) = dpi::small_icon_size();
    // SAFETY: a null module name asks for this process's own module handle.
    let own = unsafe { GetModuleHandleW(None) }.ok().map(Into::into);
    let load = |module, name| -> Option<HICON> {
        // SAFETY: `name` is a `MAKEINTRESOURCE` id (a small integer in a pointer), which
        // `LoadImageW` accepts. `LR_SHARED` makes the system own and cache the icon, so it is
        // never destroyed here and stays valid for the life of the process.
        let handle: HANDLE =
            unsafe { LoadImageW(module, name, IMAGE_ICON, cx, cy, LR_SHARED) }.ok()?;
        Some(HICON(handle.0))
    };
    own.and_then(|module| load(Some(module), PCWSTR(std::ptr::without_provenance::<u16>(1))))
        .or_else(|| load(None, IDI_APPLICATION))
        .unwrap_or_default()
}

/// Shows `lines` at the click position (`anchor`, the callback's `wParam`) and returns the entry
/// chosen, if any.
fn popup_menu(hwnd: HWND, lines: &[MenuLine], anchor: usize) -> Option<TrayEntry> {
    let menu = build_menu(lines)?;
    let (mut x, mut y) = decode_anchor(anchor);
    if (x, y) == (0, 0) {
        let mut at = POINT::default();
        // SAFETY: `at` is a valid out-pointer. On failure the menu opens at the corner.
        if unsafe { GetCursorPos(&raw mut at) }.is_ok() {
            (x, y) = (at.x, at.y);
        }
    }
    // SAFETY: `hwnd` is our own window, on this thread. The menu only closes on an outside click
    // if the owner is the foreground window (documented requirement of `TrackPopupMenu`).
    let _ = unsafe { SetForegroundWindow(hwnd) };
    // SAFETY: `menu` is the live popup menu built above and `hwnd` its owner on this thread.
    // `TPM_RETURNCMD` makes the return value the chosen command id (0 if none), not a BOOL.
    let chosen = unsafe {
        TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
            x,
            y,
            None,
            hwnd,
            None,
        )
    };
    // SAFETY: documented companion of `TrackPopupMenu` for taskbar icons; no pointers.
    let _ = unsafe { PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0)) };
    // SAFETY: `menu` is ours and no longer in use.
    let _ = unsafe { DestroyMenu(menu) };
    u32::try_from(chosen.0)
        .ok()
        .and_then(TrayEntry::from_command_id)
}

fn build_menu(lines: &[MenuLine]) -> Option<HMENU> {
    // SAFETY: no preconditions; the menu is destroyed by the caller (or below on failure).
    let menu = unsafe { CreatePopupMenu() }.ok()?;
    for line in lines {
        let added = match line {
            MenuLine::Separator => {
                // SAFETY: `menu` is a live menu; a separator has no id or text.
                unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()) }
            }
            MenuLine::Item {
                entry,
                label,
                checked,
            } => {
                let text = HSTRING::from(*label);
                let flags = if *checked {
                    MF_STRING | MF_CHECKED
                } else {
                    MF_STRING
                };
                // SAFETY: `menu` is a live menu; `text` is a NUL-terminated wide string that
                // outlives the call (the menu copies it).
                unsafe {
                    AppendMenuW(
                        menu,
                        flags,
                        entry.command_id() as usize,
                        PCWSTR(text.as_ptr()),
                    )
                }
            }
        };
        if added.is_err() {
            // SAFETY: `menu` is ours and not yet shown.
            let _ = unsafe { DestroyMenu(menu) };
            return None;
        }
    }
    Some(menu)
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::Shell::NIN_SELECT;
    use windows::Win32::UI::WindowsAndMessaging::WM_CONTEXTMENU;

    use super::*;

    #[test]
    fn the_portable_callback_codes_match_the_windows_constants() {
        assert_eq!(crate::tray::NIN_SELECT, NIN_SELECT);
        // NIN_KEYSELECT = NIN_SELECT | NINF_KEY (1).
        assert_eq!(crate::tray::NIN_KEYSELECT, NIN_SELECT | 1);
        assert_eq!(crate::tray::WM_CONTEXTMENU, WM_CONTEXTMENU);
    }

    #[test]
    fn the_tooltip_buffer_holds_what_the_model_allows() {
        assert_eq!(
            crate::tray::TOOLTIP_CAPACITY,
            NOTIFYICONDATAW::default().szTip.len()
        );
    }

    #[test]
    fn the_icon_starts_and_stops_without_being_shown() {
        let tray = WinTray::start("Snipdeck test", |_| {}).unwrap();
        drop(tray);
    }
}
