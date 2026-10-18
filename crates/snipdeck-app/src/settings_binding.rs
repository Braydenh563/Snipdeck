//! Binds the Settings page to the [`Controller`]: copies the settings, the licence tier and the
//! backups into the window, and turns every callback into a change of the settings (stored by the
//! controller's quiet-time saver, see [`Controller::update_settings`]) or a call of one of the
//! controller's Settings methods (licence, backup, restore, export, erase).
//!
//! Everything the page does to the machine goes through [`SettingsHost`], so tests run it against
//! fakes: registering the global shortcuts, the Windows start-up entry, opening a folder.
//!
//! The first half of this file is plain logic (what a shortcut may be, which program names are
//! accepted, what "DELETE" must look like, how a backup is described) and is tested without a
//! window; the second half is the Slint glue.

use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::{Rc, Weak},
};

use slint::{ComponentHandle, SharedString};
use snipdeck_platform::{
    chord::Chord,
    pipeline::{Clipboard, Keyboard, Timing},
};
use snipdeck_portable::backup::{self, BackupFile};
use snipdeck_store::Store;
use snipdeck_ui as ui;

use crate::{
    binding::{Loggable, Shown, arm_toast, model, redraw_all, register_redraw, to_i32},
    controller::{Controller, ToastPhase, ToastView},
    host::{self, ClipboardText, HotkeyBackend, HotkeyFailure, HotkeyProblem},
    main_binding::{RegularWindow, raise, set_look},
    settings::{HotkeyAction, Hotkeys, Settings, Theme},
    toast::{Toast, ToastKind, ToastMessage},
    wording::{self, LANGUAGE_NAMES, LANGUAGE_TAGS},
};

// ---- What the machine is asked to do -----------------------------------------------------------

/// What Settings asks of the machine. [`SystemHost`] is the real one; tests bring a fake.
pub trait SettingsHost {
    /// Makes `hotkeys` the registered global shortcuts: the empty ones are removed, the others
    /// registered. Returns the ones that could not be.
    fn apply_hotkeys(&self, hotkeys: &Hotkeys) -> Vec<HotkeyProblem>;

    /// Adds or removes the "Start with Windows" entry.
    ///
    /// # Errors
    ///
    /// Why it could not be changed.
    fn set_autostart(&self, on: bool) -> Result<(), String>;

    /// Shows `folder` in the file manager.
    ///
    /// # Errors
    ///
    /// The file manager could not be started.
    fn open_folder(&self, folder: &Path) -> Result<(), String>;

    /// Where an export is saved, if the machine has a Documents folder.
    fn documents_folder(&self) -> Option<PathBuf>;
}

/// Registers `hotkeys` with `backend`: a shortcut that is off is removed (which
/// [`host::register_all`] alone would leave registered), the others are registered. Returns the
/// ones that could not be.
pub fn apply_hotkeys(
    backend: &mut (impl HotkeyBackend + ?Sized),
    hotkeys: &Hotkeys,
) -> Vec<HotkeyProblem> {
    for action in HotkeyAction::ALL {
        if hotkeys.chord(action).is_none() {
            // Removing one that is not there succeeds; a service that is down is reported by
            // the registrations that follow.
            let _ = backend.unregister(host::id_of(action));
        }
    }
    host::register_all(backend, hotkeys)
}

/// The real machine: the hotkey service the host started, the registry, the file manager.
pub struct SystemHost {
    hotkeys: Rc<RefCell<Box<dyn HotkeyBackend>>>,
}

impl SystemHost {
    /// A host that registers shortcuts with `hotkeys`.
    #[must_use]
    pub fn new(hotkeys: Rc<RefCell<Box<dyn HotkeyBackend>>>) -> Self {
        Self { hotkeys }
    }
}

impl SettingsHost for SystemHost {
    fn apply_hotkeys(&self, hotkeys: &Hotkeys) -> Vec<HotkeyProblem> {
        apply_hotkeys(&mut **self.hotkeys.borrow_mut(), hotkeys)
    }

    fn set_autostart(&self, on: bool) -> Result<(), String> {
        #[cfg(windows)]
        {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            snipdeck_platform::windows::autostart::set_enabled(&exe, "--hidden", on)
                .map_err(|e| e.to_string())
        }
        #[cfg(not(windows))]
        {
            // Only Windows has the entry; elsewhere the setting is just remembered.
            let _ = on;
            Ok(())
        }
    }

    fn open_folder(&self, folder: &Path) -> Result<(), String> {
        let opener = if cfg!(windows) {
            "explorer"
        } else if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        std::process::Command::new(opener)
            .arg(folder)
            .spawn()
            .map(drop)
            .map_err(|e| e.to_string())
    }

    fn documents_folder(&self) -> Option<PathBuf> {
        let home = if cfg!(windows) {
            std::env::var_os("USERPROFILE")
        } else {
            std::env::var_os("HOME")
        }?;
        Some(PathBuf::from(home).join("Documents"))
    }
}

// ---- Logic -------------------------------------------------------------------------------------

/// What the user must type to erase the library, exactly (the page shows the same word).
pub const ERASE_WORD: &str = "DELETE";

/// Whether `typed` is the confirmation for "Erase library": exactly [`ERASE_WORD`], nothing else
/// (no other case, no spaces around it).
#[must_use]
pub fn erase_confirmed(typed: &str) -> bool {
    typed == ERASE_WORD
}

/// Why a recorded shortcut was not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyRefusal {
    /// The text is not a shortcut Windows can register.
    NotAShortcut,
    /// Another Snipdeck action has it already.
    UsedBy(HotkeyAction),
    /// Windows or another program refused it.
    Refused(HotkeyFailure),
}

/// `current` with `action` set to `combo` ("Ctrl+Alt+V").
///
/// # Errors
///
/// [`HotkeyRefusal::NotAShortcut`] for text that is not a valid shortcut, and
/// [`HotkeyRefusal::UsedBy`] when another action has the same one.
pub fn with_hotkey(
    current: &Hotkeys,
    action: HotkeyAction,
    combo: &str,
) -> Result<Hotkeys, HotkeyRefusal> {
    let chord: Chord = combo.parse().map_err(|_| HotkeyRefusal::NotAShortcut)?;
    if let Some(other) = HotkeyAction::ALL
        .into_iter()
        .find(|a| *a != action && current.chord(*a) == Some(chord))
    {
        return Err(HotkeyRefusal::UsedBy(other));
    }
    let mut next = current.clone();
    *next.slot(action) = chord.to_string();
    Ok(next)
}

/// `current` with `action` switched off.
#[must_use]
pub fn without_hotkey(current: &Hotkeys, action: HotkeyAction) -> Hotkeys {
    let mut next = current.clone();
    next.slot(action).clear();
    next
}

/// Why a program name was not added to "Never paste into".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoPasteRefusal {
    /// Nothing was typed.
    Empty,
    /// A path, not a file name.
    HasPath,
    /// Already in the list (the list ignores case).
    Listed(String),
}

/// `apps` with `typed` added, in the form the list keeps (trimmed, lower case).
///
/// # Errors
///
/// See [`NoPasteRefusal`].
pub fn with_no_paste_app(apps: &[String], typed: &str) -> Result<Vec<String>, NoPasteRefusal> {
    let name = typed.trim().to_ascii_lowercase();
    if name.is_empty() {
        return Err(NoPasteRefusal::Empty);
    }
    if name.contains(['/', '\\', ':']) {
        return Err(NoPasteRefusal::HasPath);
    }
    if apps.contains(&name) {
        return Err(NoPasteRefusal::Listed(name));
    }
    let mut next = apps.to_vec();
    next.push(name);
    Ok(next)
}

/// A size for a list: "812 B", "34 KB", "1.4 MB".
#[must_use]
pub fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{} KB", bytes.div_ceil(KB))
    } else {
        let tenths = bytes.saturating_mul(10) / MB;
        format!("{}.{} MB", tenths / 10, tenths % 10)
    }
}

/// The chips of a shortcut's text: `Ctrl+Alt+V` is `Ctrl`, `Alt`, `V` (and `Ctrl++` is `Ctrl`, `+`).
#[must_use]
pub fn key_chips(value: &str) -> Vec<String> {
    if value.is_empty() {
        return Vec::new();
    }
    let (rest, plus) = value
        .strip_suffix("++")
        .map_or((value, false), |rest| (rest, true));
    let mut chips: Vec<String> = rest.split('+').map(str::to_owned).collect();
    if plus {
        chips.push("+".to_owned());
    }
    chips
}

/// Names of the actions, for the message when two share a shortcut.
const fn action_name(action: HotkeyAction) -> &'static str {
    match action {
        HotkeyAction::TogglePanel => "Show or hide the panel",
        HotkeyAction::Picker => "Open the picker",
        HotkeyAction::Capture => "Capture a region",
        HotkeyAction::PasteNext => "Paste the next item",
    }
}

fn hotkey_refusal_text(refusal: HotkeyRefusal, combo: &str) -> String {
    match refusal {
        HotkeyRefusal::NotAShortcut => "That combination can't be used as a shortcut.".into(),
        HotkeyRefusal::UsedBy(other) => {
            format!(
                "{combo} is already used for \u{201c}{}\u{201d}.",
                action_name(other)
            )
        }
        HotkeyRefusal::Refused(HotkeyFailure::InUse) => {
            format!("Another program already uses {combo}.")
        }
        HotkeyRefusal::Refused(HotkeyFailure::Invalid) => {
            format!("Windows doesn't allow {combo} as a shortcut.")
        }
        HotkeyRefusal::Refused(HotkeyFailure::Unavailable) => {
            "Shortcuts are not available right now. Try again in a moment.".into()
        }
    }
}

fn no_paste_refusal_text(refusal: &NoPasteRefusal) -> String {
    match refusal {
        NoPasteRefusal::Empty => String::new(),
        NoPasteRefusal::HasPath => "Use just the program's file name, like notepad.exe.".into(),
        NoPasteRefusal::Listed(name) => format!("{name} is already in the list."),
    }
}

// ---- What the page shows -----------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct HotkeyRow {
    keys: Vec<String>,
    value: String,
    conflict: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BackupLine {
    date: String,
    size: String,
}

/// Everything the page shows, built from the controller and the page's own state. Compared with
/// the last one drawn so a redraw sets only what changed (re-setting a model would reset the
/// page's scroll and a recorder that is waiting for keys).
#[derive(Debug, Clone, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one field per switch the page shows; they are independent settings, not a state"
)]
struct PageView {
    look: snipdeck_platform::system_look::SystemLook,
    hotkeys: Vec<HotkeyRow>,
    theme_index: i32,
    language_index: i32,
    autostart: bool,
    close_to_tray: bool,
    site_icons: bool,
    panel_width: i32,
    no_paste_apps: Vec<String>,
    no_paste_error: String,
    pro: bool,
    licence_error: String,
    has_data_folder: bool,
    backups: Vec<BackupLine>,
    toast: Option<ToastView>,
}

/// What the page remembers between redraws: the reasons things were refused, and the backups on disk.
#[derive(Default)]
struct PageState {
    hotkey_conflicts: [String; 4],
    no_paste_error: String,
    licence_error: String,
    backups: Vec<BackupFile>,
}

fn theme_index(theme: Theme) -> i32 {
    match theme {
        Theme::System => 0,
        Theme::Light => 1,
        Theme::Dark => 2,
    }
}

fn language_index(tag: &str) -> i32 {
    LANGUAGE_TAGS
        .iter()
        .position(|t| t.eq_ignore_ascii_case(tag))
        .map_or(0, |i| to_i32(i + 1))
}

fn page_view<C, K, T>(
    controller: &Controller<Store, C, K, T>,
    state: &PageState,
    has_data_folder: bool,
) -> PageView
where
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    let settings: &Settings = controller.settings();
    let hotkeys = HotkeyAction::ALL
        .into_iter()
        .enumerate()
        .map(|(i, action)| {
            let value = match action {
                HotkeyAction::TogglePanel => &settings.hotkeys.toggle_panel,
                HotkeyAction::Picker => &settings.hotkeys.picker,
                HotkeyAction::Capture => &settings.hotkeys.capture,
                HotkeyAction::PasteNext => &settings.hotkeys.paste_next,
            };
            HotkeyRow {
                keys: key_chips(value),
                value: value.clone(),
                conflict: state.hotkey_conflicts[i].clone(),
            }
        })
        .collect();
    PageView {
        look: controller.settings_look(),
        hotkeys,
        theme_index: theme_index(settings.theme),
        language_index: language_index(&settings.language),
        autostart: settings.autostart,
        close_to_tray: settings.close_to_tray,
        site_icons: settings.site_icons,
        panel_width: i32::try_from(settings.panel_width).unwrap_or(320),
        no_paste_apps: settings.no_paste_apps.clone(),
        no_paste_error: state.no_paste_error.clone(),
        pro: controller.tier() == snipdeck_core::Tier::Pro,
        licence_error: state.licence_error.clone(),
        has_data_folder,
        backups: state
            .backups
            .iter()
            .map(|b| BackupLine {
                date: b.date.to_string(),
                size: human_size(b.bytes),
            })
            .collect(),
        toast: controller.settings_toast(),
    }
}

// ---- The binding -------------------------------------------------------------------------------

struct SettingsCore<C, K, T> {
    controller: Rc<RefCell<Controller<Store, C, K, T>>>,
    window: slint::Weak<ui::SettingsPage>,
    /// The data folder; `None` in the demo, which keeps nothing on disk.
    data_dir: Option<PathBuf>,
    host: Rc<dyn SettingsHost>,
    state: RefCell<PageState>,
    /// What the window shows now.
    shown: RefCell<Option<PageView>>,
    /// Redraws this window (registered with `binding::register_redraw`).
    redraw: Rc<dyn Fn()>,
    /// A timer is waiting to write changed settings.
    save_timer: Cell<bool>,
}

/// A bound Settings page. Dropping it does not unbind the window; the window's callbacks keep the
/// binding alive.
pub struct SettingsBinding<C, K, T> {
    core: Rc<SettingsCore<C, K, T>>,
}

impl<C, K, T> std::fmt::Debug for SettingsBinding<C, K, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsBinding").finish_non_exhaustive()
    }
}

impl<C, K, T> SettingsBinding<C, K, T>
where
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// Reads the backups folder and redraws the window from the controller.
    pub fn redraw(&self) {
        self.core.refresh_backups();
        self.core.sync();
    }
}

impl<C, K, T> SettingsCore<C, K, T>
where
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// Runs `f` on the controller, then redraws every window.
    fn act<R>(self: &Rc<Self>, f: impl FnOnce(&mut Controller<Store, C, K, T>) -> R) -> R {
        let result = f(&mut self.controller.borrow_mut());
        redraw_all();
        result
    }

    fn refresh_backups(&self) {
        let found = self
            .data_dir
            .as_deref()
            .and_then(|dir| backup::list_backups(dir).ok())
            .unwrap_or_default();
        self.state.borrow_mut().backups = found;
    }

    /// Copies the controller's state into the window and keeps the toast and save timers running.
    fn sync(self: &Rc<Self>) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        // A closed page is drawn again when it opens.
        if self.shown.borrow().is_some() && !window.window().is_visible() {
            self.arm_save();
            return;
        }
        let view = page_view(
            &self.controller.borrow(),
            &self.state.borrow(),
            self.data_dir.is_some(),
        );
        render(&window, self.shown.borrow().as_ref(), &view);
        if let Some(toast) = &view.toast
            && self.controller.borrow_mut().claim_toast_timer(toast.seq)
        {
            arm_toast(
                Rc::downgrade(&self.controller),
                toast.seq,
                crate::controller::TOAST_ENTER,
            );
        }
        *self.shown.borrow_mut() = Some(view);
        self.arm_save();
    }

    /// Keeps one timer running while changed settings wait for their quiet time: when it fires
    /// the controller writes them if they are due (its clock decides) and the timer is armed
    /// again for whatever is still waiting.
    fn arm_save(self: &Rc<Self>) {
        if self.save_timer.get() {
            return;
        }
        let Some(wait) = self.controller.borrow().settings_wait() else {
            return;
        };
        self.save_timer.set(true);
        let core = Rc::downgrade(self);
        slint::Timer::single_shot(wait, move || {
            let Some(core) = core.upgrade() else {
                return;
            };
            core.save_timer.set(false);
            core.controller.borrow_mut().save_settings_if_due();
            redraw_all();
        });
    }

    /// Draws everything again, also what the page would otherwise believe it already shows (a
    /// switch the user flipped that the host then refused).
    fn force_redraw(self: &Rc<Self>) {
        *self.shown.borrow_mut() = None;
        redraw_all();
    }

    fn toast(self: &Rc<Self>, kind: ToastKind, message: ToastMessage) {
        self.controller
            .borrow_mut()
            .show_toast(Toast { kind, message });
    }

    // ---- Shortcuts -------------------------------------------------------------------------

    fn set_conflict(&self, index: usize, text: String) {
        if let Some(slot) = self.state.borrow_mut().hotkey_conflicts.get_mut(index) {
            *slot = text;
        }
    }

    fn hotkey_recorded(self: &Rc<Self>, index: usize, combo: &str) {
        let Some(action) = HotkeyAction::ALL.get(index).copied() else {
            return;
        };
        let current = self.controller.borrow().settings().hotkeys.clone();
        let next = match with_hotkey(&current, action, combo) {
            Ok(next) => next,
            Err(refusal) => {
                self.set_conflict(index, hotkey_refusal_text(refusal, combo));
                redraw_all();
                return;
            }
        };
        if let Some(failure) = self.try_register(action, &current, &next) {
            self.set_conflict(
                index,
                hotkey_refusal_text(HotkeyRefusal::Refused(failure), combo),
            );
            redraw_all();
            return;
        }
        self.set_conflict(index, String::new());
        self.act(|ctl| ctl.update_settings(|s| s.hotkeys = next));
    }

    fn hotkey_cleared(self: &Rc<Self>, index: usize) {
        let Some(action) = HotkeyAction::ALL.get(index).copied() else {
            return;
        };
        let current = self.controller.borrow().settings().hotkeys.clone();
        let next = without_hotkey(&current, action);
        // Taking a shortcut away cannot clash; a service that is down is not the user's problem here.
        let _ = self.try_register(action, &current, &next);
        self.set_conflict(index, String::new());
        self.act(|ctl| ctl.update_settings(|s| s.hotkeys = next));
    }

    /// Registers `next` with the system (unless "Pause hotkeys" is on, when the chords in force
    /// are registered at resume). If `action`'s shortcut is refused the old ones are put back and
    /// the reason is returned.
    fn try_register(
        &self,
        action: HotkeyAction,
        current: &Hotkeys,
        next: &Hotkeys,
    ) -> Option<HotkeyFailure> {
        if self.controller.borrow().hotkeys_paused() {
            return None;
        }
        let refused = self
            .host
            .apply_hotkeys(next)
            .into_iter()
            .find(|p| p.action == action);
        if let Some(problem) = refused {
            let _ = self.host.apply_hotkeys(current);
            return Some(problem.failure);
        }
        None
    }

    // ---- Programs that never get a paste ---------------------------------------------------

    fn no_paste_added(self: &Rc<Self>, typed: &str) {
        let apps = self.controller.borrow().settings().no_paste_apps.clone();
        match with_no_paste_app(&apps, typed) {
            Ok(next) => {
                self.state.borrow_mut().no_paste_error.clear();
                self.act(|ctl| ctl.update_settings(|s| s.no_paste_apps = next));
            }
            Err(refusal) => {
                self.state.borrow_mut().no_paste_error = no_paste_refusal_text(&refusal);
                redraw_all();
            }
        }
    }

    fn no_paste_removed(self: &Rc<Self>, index: usize) {
        self.state.borrow_mut().no_paste_error.clear();
        self.act(|ctl| {
            ctl.update_settings(|s| {
                if index < s.no_paste_apps.len() {
                    s.no_paste_apps.remove(index);
                }
            });
        });
    }

    // ---- Start with Windows ----------------------------------------------------------------

    fn autostart_toggled(self: &Rc<Self>, on: bool) {
        match self.host.set_autostart(on) {
            Ok(()) => {
                self.act(|ctl| ctl.update_settings(|s| s.autostart = on));
            }
            Err(error) => {
                crate::trace!("autostart {on}: {error}");
                self.toast(ToastKind::Danger, ToastMessage::AutostartFailed);
                self.force_redraw();
            }
        }
    }

    // ---- Licence, backups, data ------------------------------------------------------------

    fn licence_submitted(self: &Rc<Self>, key: &str) {
        let data_dir = self.data_dir.clone();
        let result = self.act(|ctl| ctl.apply_licence(key, data_dir.as_deref()));
        self.state.borrow_mut().licence_error = result
            .err()
            .map(|p| wording::licence_problem_text(p).to_owned())
            .unwrap_or_default();
        redraw_all();
    }

    fn backup_now(self: &Rc<Self>) {
        let Some(dir) = self.data_dir.clone() else {
            return;
        };
        self.act(|ctl| ctl.backup_now(&dir));
        self.refresh_backups();
        redraw_all();
    }

    fn restore(self: &Rc<Self>, index: usize) {
        let file = self
            .state
            .borrow()
            .backups
            .get(index)
            .map(|b| b.path.clone());
        if let Some(file) = file {
            self.act(|ctl| ctl.restore_backup(&file));
        }
    }

    fn open_data_folder(self: &Rc<Self>) {
        let Some(dir) = self.data_dir.clone() else {
            return;
        };
        self.open_folder(&dir);
    }

    fn open_folder(self: &Rc<Self>, folder: &Path) {
        if let Err(error) = self.host.open_folder(folder) {
            crate::trace!("open folder: {error}");
            self.toast(ToastKind::Danger, ToastMessage::FolderNotOpened);
            redraw_all();
        }
    }

    fn export(self: &Rc<Self>) {
        let Some(dir) = self.data_dir.clone() else {
            return;
        };
        let folder = self
            .host
            .documents_folder()
            .unwrap_or_else(|| dir.join("exports"));
        let saved = self.act(|ctl| ctl.export_library(&folder));
        if let Some(file) = saved
            && let Some(parent) = file.parent()
        {
            self.open_folder(parent);
        }
    }

    fn erase(self: &Rc<Self>, typed: &str, forget_key: bool) {
        let Some(dir) = self.data_dir.clone() else {
            return;
        };
        // The page only enables the button for the exact word; this is the check that counts.
        if !erase_confirmed(typed) {
            return;
        }
        self.act(|ctl| ctl.erase_library(&dir, forget_key));
        self.refresh_backups();
        redraw_all();
    }
}

impl<C, K, T> Controller<Store, C, K, T>
where
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// Draws the settings in `window` and routes the page's gestures to the controller. A
    /// `data_dir` of `None` (the demo) switches the backups and the data tools off.
    pub fn bind_settings(
        this: &Rc<RefCell<Self>>,
        window: &ui::SettingsPage,
        data_dir: Option<PathBuf>,
        host: Rc<dyn SettingsHost>,
    ) -> SettingsBinding<C, K, T> {
        let core = Rc::new_cyclic(|weak: &Weak<SettingsCore<C, K, T>>| {
            let weak = weak.clone();
            SettingsCore {
                controller: Rc::clone(this),
                window: window.as_weak(),
                data_dir,
                host,
                state: RefCell::new(PageState::default()),
                shown: RefCell::new(None),
                redraw: Rc::new(move || {
                    if let Some(core) = weak.upgrade() {
                        core.sync();
                    }
                }),
                save_timer: Cell::new(false),
            }
        });
        register_redraw(&core.redraw);
        let follow = window.invoke_follow_windows_text();
        window.set_languages(model(
            std::iter::once(follow).chain(LANGUAGE_NAMES.iter().map(|n| SharedString::from(*n))),
        ));
        core.refresh_backups();
        core.sync();

        macro_rules! wire {
            ($on:ident, |$core:ident $(, $arg:ident)*| $body:expr) => {{
                let $core = Rc::clone(&core);
                window.$on(move |$($arg),*| {
                    crate::trace!("callback settings {}{}", stringify!($on), Shown(&[$(&$arg as &dyn Loggable),*]));
                    $body;
                });
            }};
        }
        wire!(on_hotkey_recorded, |core, i, combo| core
            .hotkey_recorded(crate::binding::index(i), &combo));
        wire!(on_hotkey_cleared, |core, i| core
            .hotkey_cleared(crate::binding::index(i)));
        wire!(on_theme_chosen, |core, i| core.act(|ctl| ctl
            .update_settings(|s| {
                s.theme = match i {
                    1 => Theme::Light,
                    2 => Theme::Dark,
                    _ => Theme::System,
                };
            })));
        wire!(on_language_chosen, |core, i| core.act(|ctl| ctl
            .update_settings(|s| {
                s.language = usize::try_from(i - 1)
                    .ok()
                    .and_then(|at| LANGUAGE_TAGS.get(at))
                    .map(|tag| (*tag).to_owned())
                    .unwrap_or_default();
            })));
        wire!(on_autostart_toggled, |core, on| core.autostart_toggled(on));
        wire!(on_close_to_tray_toggled, |core, on| core
            .act(|ctl| ctl.update_settings(|s| s.close_to_tray = on)));
        wire!(on_site_icons_toggled, |core, on| core
            .act(|ctl| ctl.update_settings(|s| s.site_icons = on)));
        wire!(on_panel_width_chosen, |core, width| core
            .act(|ctl| ctl.update_settings(
                |s| s.panel_width = u32::try_from(width).unwrap_or(0)
            )));
        wire!(on_no_paste_app_added, |core, name| core
            .no_paste_added(&name));
        wire!(on_no_paste_app_removed, |core, i| core
            .no_paste_removed(crate::binding::index(i)));
        wire!(on_licence_submitted, |core, key| core
            .licence_submitted(&key));
        wire!(on_backup_now_requested, |core| core.backup_now());
        wire!(on_backup_restore_requested, |core, i| core
            .restore(crate::binding::index(i)));
        wire!(on_open_data_folder_requested, |core| core
            .open_data_folder());
        wire!(on_export_requested, |core| core.export());
        wire!(on_erase_confirmed, |core, typed, forget| core
            .erase(&typed, forget));
        {
            let weak = window.as_weak();
            window.on_close_requested(move || {
                if let Some(window) = weak.upgrade() {
                    let _ = window.hide();
                }
            });
        }
        SettingsBinding { core }
    }
}

// ---- Page state -> window ----------------------------------------------------------------------

fn render(window: &ui::SettingsPage, previous: Option<&PageView>, view: &PageView) {
    let differs = |same: &dyn Fn(&PageView) -> bool| previous.is_none_or(|p| !same(p));

    if differs(&|p| p.look == view.look) {
        window.set_dark(view.look.dark);
        set_look(window, view.look.high_contrast.as_ref());
    }
    if differs(&|p| p.hotkeys == view.hotkeys) {
        window.set_hotkeys(model(view.hotkeys.iter().map(|h| ui::HotkeySetting {
            keys: model(h.keys.iter().map(|k| SharedString::from(k.as_str()))),
            value: h.value.as_str().into(),
            conflict: h.conflict.as_str().into(),
        })));
    }
    if differs(&|p| p.theme_index == view.theme_index) {
        window.set_theme_index(view.theme_index);
    }
    if differs(&|p| p.language_index == view.language_index) {
        window.set_language_index(view.language_index);
    }
    if differs(&|p| p.autostart == view.autostart) {
        window.set_autostart(view.autostart);
    }
    if differs(&|p| p.close_to_tray == view.close_to_tray) {
        window.set_close_to_tray(view.close_to_tray);
    }
    if differs(&|p| p.site_icons == view.site_icons) {
        window.set_site_icons(view.site_icons);
    }
    if differs(&|p| p.panel_width == view.panel_width) {
        window.set_panel_width(view.panel_width);
    }
    if differs(&|p| p.no_paste_apps == view.no_paste_apps) {
        window.set_no_paste_apps(model(
            view.no_paste_apps
                .iter()
                .map(|a| SharedString::from(a.as_str())),
        ));
    }
    if differs(&|p| p.no_paste_error == view.no_paste_error) {
        window.set_no_paste_error(view.no_paste_error.as_str().into());
    }
    if differs(&|p| p.pro == view.pro) {
        window.set_pro(view.pro);
    }
    if differs(&|p| p.licence_error == view.licence_error) {
        window.set_licence_error(view.licence_error.as_str().into());
    }
    if differs(&|p| p.has_data_folder == view.has_data_folder) {
        window.set_has_data_folder(view.has_data_folder);
    }
    if differs(&|p| p.backups == view.backups) {
        window.set_backups(model(view.backups.iter().map(|b| ui::BackupRow {
            date: b.date.as_str().into(),
            size: b.size.as_str().into(),
        })));
    }
    if differs(&|p| p.toast == view.toast) {
        if let Some(toast) = &view.toast {
            window.set_toast_kind(match toast.kind {
                ToastKind::Neutral => ui::ToastKind::Neutral,
                ToastKind::Success => ui::ToastKind::Success,
                ToastKind::Danger => ui::ToastKind::Danger,
            });
            window.set_toast_phase(match toast.phase {
                ToastPhase::Enter => ui::ToastPhase::Enter,
                ToastPhase::Hold => ui::ToastPhase::Hold,
                ToastPhase::Exit => ui::ToastPhase::Exit,
            });
            window.set_toast_text(wording::toast_text(&toast.message).into());
        } else {
            window.set_toast_text(SharedString::new());
            window.set_toast_phase(ui::ToastPhase::Exit);
        }
    }
}

// ---- Opening the window on demand --------------------------------------------------------------

thread_local! {
    /// Opens the Settings page; set by [`SettingsUi::new`]. The Main window's Settings button
    /// reaches the page through [`request_open`] because it is created long before the host
    /// could hand it anything.
    static OPENER: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Opens the Settings page (creating it the first time), if the host made one with
/// [`SettingsUi::new`]. UI thread only.
pub fn request_open() {
    let opener = OPENER.with(|o| o.borrow().clone());
    if let Some(opener) = opener {
        opener();
    }
}

/// Creates the Settings page the first time it is asked for, binds it, and shows and raises it
/// every time. Not generic: the host keeps one of these.
pub struct SettingsUi {
    open: Box<dyn Fn() -> Result<(), slint::PlatformError>>,
    window: Box<dyn Fn() -> Option<ui::SettingsPage>>,
}

impl std::fmt::Debug for SettingsUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsUi").finish_non_exhaustive()
    }
}

impl SettingsUi {
    /// A Settings page for `controller`, not created yet. `data_dir` is the library's folder
    /// (`None` in the demo) and `host` does what the page asks of the machine.
    pub fn new<C, K, T>(
        controller: &Rc<RefCell<Controller<Store, C, K, T>>>,
        data_dir: Option<PathBuf>,
        host: Rc<dyn SettingsHost>,
    ) -> Self
    where
        C: Clipboard + ClipboardText + 'static,
        K: Keyboard + 'static,
        T: Timing + 'static,
    {
        type Slot<C, K, T> = Rc<RefCell<Option<(ui::SettingsPage, SettingsBinding<C, K, T>)>>>;
        let slot: Slot<C, K, T> = Rc::default();
        let open = {
            let slot = Rc::clone(&slot);
            let controller = Rc::clone(controller);
            Box::new(move || {
                let mut slot = slot.borrow_mut();
                if slot.is_none() {
                    let window = ui::SettingsPage::new()?;
                    let binding = Controller::bind_settings(
                        &controller,
                        &window,
                        data_dir.clone(),
                        Rc::clone(&host),
                    );
                    *slot = Some((window, binding));
                }
                let Some((window, binding)) = slot.as_ref() else {
                    return Ok(());
                };
                let _regular = RegularWindow::begin();
                window.show()?;
                // The page was hidden, so it skipped the redraws; draw it now that it is up.
                binding.redraw();
                raise(window.window());
                Ok(())
            })
        };
        let window = {
            let slot = Rc::clone(&slot);
            Box::new(move || {
                slot.borrow()
                    .as_ref()
                    .map(|(window, _)| window.clone_strong())
            })
        };
        let open: Rc<dyn Fn() -> Result<(), slint::PlatformError>> = Rc::new(open);
        OPENER.with(|o| {
            let open = Rc::clone(&open);
            *o.borrow_mut() = Some(Rc::new(move || {
                if let Err(e) = open() {
                    crate::trace!("cannot open the settings: {e}");
                }
            }));
        });
        Self {
            open: Box::new(move || open()),
            window,
        }
    }

    /// Creates the page if this is the first time, then shows it and brings it to the front.
    ///
    /// # Errors
    ///
    /// The windowing system failed to create or show it.
    pub fn open(&self) -> Result<(), slint::PlatformError> {
        (self.open)()
    }

    /// The page, once it has been created.
    #[must_use]
    pub fn window(&self) -> Option<ui::SettingsPage> {
        (self.window)()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::FakeHotkeys;

    #[test]
    fn only_the_exact_word_confirms_the_erase() {
        assert!(erase_confirmed("DELETE"));
        for typed in [
            "",
            "delete",
            "Delete",
            "DELETE ",
            " DELETE",
            "DELET",
            "DELETEE",
            "DELETE\n",
            "\u{ff24}ELETE",
        ] {
            assert!(!erase_confirmed(typed), "{typed:?}");
        }
    }

    #[test]
    fn a_new_shortcut_replaces_only_its_own_action() {
        let current = Hotkeys::default();
        let next = with_hotkey(&current, HotkeyAction::Capture, "ctrl+alt+k").expect("free");
        assert_eq!(next.capture, "Ctrl+Alt+K");
        assert_eq!(next.picker, current.picker);
        assert_eq!(next.toggle_panel, current.toggle_panel);
    }

    #[test]
    fn a_shortcut_another_action_has_is_refused_and_names_that_action() {
        let current = Hotkeys::default();
        let taken = current.picker.clone();
        assert_eq!(
            with_hotkey(&current, HotkeyAction::Capture, &taken),
            Err(HotkeyRefusal::UsedBy(HotkeyAction::Picker))
        );
        assert!(
            hotkey_refusal_text(HotkeyRefusal::UsedBy(HotkeyAction::Picker), &taken)
                .contains("Open the picker")
        );
        // Re-recording an action's own shortcut is not a clash.
        assert!(with_hotkey(&current, HotkeyAction::Picker, &taken).is_ok());
    }

    #[test]
    fn text_that_is_not_a_shortcut_is_refused() {
        for combo in ["", "Ctrl+", "Hyper+Q", "K"] {
            assert_eq!(
                with_hotkey(&Hotkeys::default(), HotkeyAction::Capture, combo),
                Err(HotkeyRefusal::NotAShortcut),
                "{combo:?}"
            );
        }
    }

    #[test]
    fn applying_the_hotkeys_removes_the_ones_that_are_off() {
        let mut backend = FakeHotkeys::default();
        let all = Hotkeys::default();
        assert_eq!(apply_hotkeys(&mut backend, &all), Vec::new());
        assert_eq!(backend.registered.len(), 4);

        let off = without_hotkey(&all, HotkeyAction::Capture);
        assert_eq!(apply_hotkeys(&mut backend, &off), Vec::new());
        let ids: Vec<u32> = backend.registered.iter().map(|(id, _)| *id).collect();
        assert!(!ids.contains(&host::id_of(HotkeyAction::Capture)));
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn a_shortcut_another_program_owns_is_reported_for_its_action() {
        let mut backend = FakeHotkeys {
            taken: vec!["Ctrl+Alt+K".into()],
            ..FakeHotkeys::default()
        };
        let next = with_hotkey(&Hotkeys::default(), HotkeyAction::Capture, "Ctrl+Alt+K").unwrap();
        let problems = apply_hotkeys(&mut backend, &next);
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].action, HotkeyAction::Capture);
        assert_eq!(problems[0].failure, HotkeyFailure::InUse);
    }

    #[test]
    fn program_names_are_trimmed_lower_cased_and_not_repeated() {
        let none: Vec<String> = Vec::new();
        let one = with_no_paste_app(&none, "  KeePass.EXE ").unwrap();
        assert_eq!(one, ["keepass.exe"]);
        assert_eq!(
            with_no_paste_app(&one, "keepass.exe"),
            Err(NoPasteRefusal::Listed("keepass.exe".into()))
        );
        assert_eq!(with_no_paste_app(&one, "  "), Err(NoPasteRefusal::Empty));
        assert_eq!(
            with_no_paste_app(&one, r"C:\Program Files\KeePass\keepass.exe"),
            Err(NoPasteRefusal::HasPath)
        );
        assert_eq!(
            with_no_paste_app(&one, "putty.exe").unwrap(),
            ["keepass.exe", "putty.exe"]
        );
    }

    #[test]
    fn sizes_read_in_the_unit_a_person_would_use() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(812), "812 B");
        assert_eq!(human_size(1024), "1 KB");
        assert_eq!(human_size(1025), "2 KB");
        assert_eq!(human_size(34 * 1024), "34 KB");
        assert_eq!(human_size(1024 * 1024), "1.0 MB");
        assert_eq!(human_size(1024 * 1024 * 3 / 2), "1.5 MB");
    }

    #[test]
    fn a_shortcut_splits_into_one_chip_per_key() {
        assert_eq!(key_chips("Ctrl+Alt+V"), ["Ctrl", "Alt", "V"]);
        assert_eq!(key_chips("Ctrl++"), ["Ctrl", "+"]);
        assert_eq!(key_chips(""), [] as [String; 0]);
    }

    #[test]
    fn every_language_has_a_name_and_a_tag() {
        assert_eq!(LANGUAGE_NAMES.len(), LANGUAGE_TAGS.len());
        assert_eq!(language_index(""), 0);
        assert_eq!(language_index("en"), 1);
        assert_eq!(language_index("PT-br"), 5);
        assert_eq!(language_index("tlh"), 0, "an unknown tag follows Windows");
    }
}
