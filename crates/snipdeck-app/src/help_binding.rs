//! Binds the Help window (Troubleshoot and About) to the [`Controller`]: runs the doctor's
//! checks ([`crate::doctor`]) on the real library, offers their fixes, opens folders and links,
//! and copies the diagnostic report.
//!
//! Everything the window does to the machine goes through [`HelpHost`], so tests can bring a
//! fake. The window is created the first time it is asked for and is single: asking again
//! raises it. Nothing is fetched from the network; "Check for updates" opens the releases page
//! in the browser.

use std::{
    cell::RefCell,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    rc::{Rc, Weak},
};

use jiff::civil::Date;
use slint::{ComponentHandle, SharedString};
use snipdeck_core::Tier;
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_portable::backup;
use snipdeck_store::{ItemRepo, Store};
use snipdeck_ui as ui;

use crate::{
    binding::{arm_toast, model, redraw_all, register_redraw},
    controller::{Controller, HelpTopic, ToastPhase, ToastView},
    crash,
    doctor::{self, Check, DoctorHost, Fix, ReportInfo, Status},
    host::ClipboardText,
    main_binding::{RegularWindow, raise, set_look},
    native_window::fit_to_screen,
    settings::HotkeyAction,
    settings_binding::action_name,
    toast::{ToastKind, ToastMessage},
    wording,
};

/// The project's home, shown under "Source code".
pub const REPO_URL: &str = "https://github.com/Braydenh563/Snipdeck";
/// "Check for updates" opens this page; the app fetches nothing itself.
pub const RELEASES_URL: &str = "https://github.com/Braydenh563/Snipdeck/releases";
/// The user guide, opened in the browser by [`HelpTopic::Guide`].
pub const GUIDE_URL: &str = "https://github.com/Braydenh563/Snipdeck/tree/main/docs/user-guide";
/// The Microsoft Store page; the link stays hidden while this is `None`.
pub const STORE_URL: Option<&str> = None;
/// The third-party notices file shipped next to the program, if the package has one.
pub const NOTICES_FILE: &str = "THIRD-PARTY-NOTICES.txt";

// ---- What the machine is asked to do -----------------------------------------------------------

/// What the Help window asks of the machine. [`SystemHelpHost`] is the real one.
pub trait HelpHost {
    /// Shows `folder` in the file manager.
    ///
    /// # Errors
    ///
    /// The file manager could not be started.
    fn open_folder(&self, folder: &Path) -> Result<(), String>;

    /// Opens `url` in the default browser.
    ///
    /// # Errors
    ///
    /// The browser could not be started.
    fn open_url(&self, url: &str) -> Result<(), String>;

    /// Opens `file` with the program the system associates with it.
    ///
    /// # Errors
    ///
    /// The program could not be started.
    fn open_file(&self, file: &Path) -> Result<(), String>;

    /// The operating system and its build, for the report.
    fn os_summary(&self) -> String;

    /// The third-party notices file, if the package has one.
    fn notices_file(&self) -> Option<PathBuf>;
}

/// [`HelpHost`] on the real system.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemHelpHost;

fn launch(target: &OsStr) -> Result<(), String> {
    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(target)
        .spawn()
        .map(drop)
        .map_err(|e| e.to_string())
}

impl HelpHost for SystemHelpHost {
    fn open_folder(&self, folder: &Path) -> Result<(), String> {
        launch(folder.as_os_str())
    }

    fn open_url(&self, url: &str) -> Result<(), String> {
        launch(OsStr::new(url))
    }

    fn open_file(&self, file: &Path) -> Result<(), String> {
        launch(file.as_os_str())
    }

    fn os_summary(&self) -> String {
        if cfg!(windows)
            && let Ok(out) = std::process::Command::new("cmd")
                .args(["/c", "ver"])
                .output()
        {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            if !text.is_empty() {
                return text;
            }
        }
        std::env::consts::OS.to_owned()
    }

    fn notices_file(&self) -> Option<PathBuf> {
        let file = std::env::current_exe().ok()?.parent()?.join(NOTICES_FILE);
        file.is_file().then_some(file)
    }
}

// ---- The doctor on the real library ------------------------------------------------------------

/// The facts of [`DoctorHost`], read from the controller's library and the data folder.
pub struct ControllerDoctor<'a, C, K, T> {
    controller: &'a Controller<Store, C, K, T>,
    data_dir: &'a Path,
}

impl<'a, C, K, T> ControllerDoctor<'a, C, K, T> {
    /// The facts of `controller`, whose data folder is `data_dir`.
    pub fn new(controller: &'a Controller<Store, C, K, T>, data_dir: &'a Path) -> Self {
        Self {
            controller,
            data_dir,
        }
    }
}

impl<C, K, T> DoctorHost for ControllerDoctor<'_, C, K, T>
where
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    fn today(&self) -> Date {
        self.controller.today()
    }

    fn data_dir_writable(&self) -> Result<(), String> {
        let probe = self.data_dir.join(".snipdeck-write-test");
        fs::write(&probe, b"").map_err(|e| e.to_string())?;
        fs::remove_file(&probe).map_err(|e| e.to_string())
    }

    /// Not known: the standard library cannot ask a drive for its free space.
    fn free_bytes(&self) -> Option<u64> {
        None
    }

    fn integrity(&self) -> Result<Vec<String>, String> {
        self.controller
            .store()
            .check_integrity()
            .map_err(|e| e.to_string())
    }

    fn backup_dates(&self) -> Result<Vec<Date>, String> {
        backup::list_backups(self.data_dir)
            .map(|files| files.into_iter().map(|f| f.date).collect())
            .map_err(|e| e.to_string())
    }

    fn sensitive_items(&self) -> Result<usize, String> {
        self.controller
            .store()
            .all_items()
            .map(|items| items.iter().filter(|i| i.sensitive).count())
            .map_err(|e| e.to_string())
    }

    fn key_ring_exists(&self) -> Result<bool, String> {
        self.controller
            .store()
            .key_ring_exists()
            .map_err(|e| e.to_string())
    }

    /// A shortcut the user left empty. Whether the others registered is told by a toast when
    /// the app starts; the Help window cannot ask the keyboard service again.
    fn shortcuts_unset(&self) -> Vec<String> {
        let hotkeys = &self.controller.settings().hotkeys;
        HotkeyAction::ALL
            .into_iter()
            .filter(|action| {
                let chord = match action {
                    HotkeyAction::TogglePanel => &hotkeys.toggle_panel,
                    HotkeyAction::Picker => &hotkeys.picker,
                    HotkeyAction::Capture => &hotkeys.capture,
                    HotkeyAction::PasteNext => &hotkeys.paste_next,
                };
                chord.is_empty()
            })
            .map(|action| action_name(action).to_owned())
            .collect()
    }

    fn shortcuts_total(&self) -> usize {
        HotkeyAction::ALL.len()
    }

    fn tray_present(&self) -> bool {
        self.controller.tray_icon_present()
    }

    fn newest_crash(&self) -> Option<String> {
        crash::newest(self.data_dir)
    }
}

// ---- Logic -------------------------------------------------------------------------------------

/// The setting switches the report lists: booleans and numbers only, never text the user typed.
fn report_flags(settings: &crate::settings::Settings) -> Vec<(String, String)> {
    let on = |b: bool| if b { "on" } else { "off" }.to_owned();
    vec![
        ("Start with Windows".into(), on(settings.autostart)),
        (
            "Keep running in the tray".into(),
            on(settings.close_to_tray),
        ),
        ("Panel in the taskbar".into(), on(settings.panel_in_taskbar)),
        (
            "Hide the panel on blur".into(),
            on(settings.panel_hide_on_blur),
        ),
        ("Panel width".into(), settings.panel_width.to_string()),
        ("Backups kept".into(), settings.backups_keep.to_string()),
        (
            "Programs that never get a paste".into(),
            settings.no_paste_apps.len().to_string(),
        ),
    ]
}

/// The date this build was made, when the build recorded it (`SNIPDECK_BUILD_DATE`).
fn build_date() -> &'static str {
    option_env!("SNIPDECK_BUILD_DATE").unwrap_or("")
}

// ---- The binding -------------------------------------------------------------------------------

struct HelpCore<C, K, T> {
    controller: Rc<RefCell<Controller<Store, C, K, T>>>,
    window: slint::Weak<ui::HelpWindow>,
    /// The data folder; `None` in the demo, which has nothing to check.
    data_dir: Option<PathBuf>,
    host: Rc<dyn HelpHost>,
    checks: RefCell<Vec<Check>>,
    /// Redraws this window (registered with `binding::register_redraw`).
    redraw: Rc<dyn Fn()>,
}

impl<C, K, T> HelpCore<C, K, T>
where
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// Runs every check again.
    fn run_checks(&self) {
        let Some(dir) = self.data_dir.as_deref() else {
            self.checks.borrow_mut().clear();
            return;
        };
        let found = {
            let controller = self.controller.borrow();
            doctor::run(&ControllerDoctor::new(&controller, dir))
        };
        *self.checks.borrow_mut() = found;
    }

    fn sync(self: &Rc<Self>) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        if !window.window().is_visible() {
            return;
        }
        let (look, tier, toast): (_, _, Option<ToastView>) = {
            let controller = self.controller.borrow();
            (
                controller.settings_look(),
                controller.tier(),
                controller.settings_toast(),
            )
        };
        window.set_dark(look.dark);
        set_look(&window, look.high_contrast.as_ref());
        window.set_has_data_folder(self.data_dir.is_some());
        window.set_version(env!("CARGO_PKG_VERSION").into());
        window.set_build_date(build_date().into());
        window.set_tier(
            if matches!(tier, Tier::Pro) {
                "Pro"
            } else {
                "Free"
            }
            .into(),
        );
        window.set_source_url(REPO_URL.into());
        window.set_show_store_link(STORE_URL.is_some());
        window.set_has_notices(self.host.notices_file().is_some());
        {
            let checks = self.checks.borrow();
            window.set_problems(
                i32::try_from(checks.iter().filter(|c| c.status != Status::Ok).count())
                    .unwrap_or(i32::MAX),
            );
            window.set_checks(model(checks.iter().map(|c| ui::HelpCheck {
                label: c.label.into(),
                detail: c.detail.as_str().into(),
                status: match c.status {
                    Status::Ok => 0,
                    Status::Warn => 1,
                    Status::Fail => 2,
                },
                fix: c.fix.map_or(0, Fix::code),
            })));
        }
        if let Some(toast) = &toast {
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
            if self.controller.borrow_mut().claim_toast_timer(toast.seq) {
                arm_toast(
                    Rc::downgrade(&self.controller),
                    toast.seq,
                    crate::controller::TOAST_ENTER,
                );
            }
        } else {
            window.set_toast_text(SharedString::new());
            window.set_toast_phase(ui::ToastPhase::Exit);
        }
    }

    fn danger_toast(&self, message: ToastMessage) {
        self.controller
            .borrow_mut()
            .show_toast(crate::toast::Toast {
                kind: ToastKind::Danger,
                message,
            });
        redraw_all();
    }

    fn opened(&self, result: Result<(), String>, what: &str) {
        if let Err(error) = result {
            crate::trace!("help: cannot open {what}: {error}");
            self.danger_toast(ToastMessage::FolderNotOpened);
        }
    }

    fn open_folder(&self, folder: &Path) {
        self.opened(self.host.open_folder(folder), "a folder");
    }

    fn open_logs_folder(&self) {
        let Some(dir) = &self.data_dir else {
            return;
        };
        let logs = crash::logs_dir(dir);
        let _ = fs::create_dir_all(&logs);
        self.open_folder(&logs);
    }

    fn fix(self: &Rc<Self>, code: i32) {
        match Fix::from_code(code) {
            Some(Fix::BackUpNow) => {
                if let Some(dir) = self.data_dir.clone() {
                    self.controller.borrow_mut().backup_now(&dir);
                    self.run_checks();
                }
            }
            Some(Fix::OpenSettings) => crate::settings_binding::request_open(),
            Some(Fix::OpenLogsFolder) => self.open_logs_folder(),
            None => {}
        }
        redraw_all();
    }

    fn copy_report(self: &Rc<Self>) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let info = {
            let controller = self.controller.borrow();
            ReportInfo {
                version: env!("CARGO_PKG_VERSION").to_owned(),
                tier: if matches!(controller.tier(), Tier::Pro) {
                    "Pro"
                } else {
                    "Free"
                }
                .to_owned(),
                os: self.host.os_summary(),
                dpi: format!("{:.0}%", window.window().scale_factor() * 100.0),
                flags: report_flags(controller.settings()),
                data_dir: self
                    .data_dir
                    .as_deref()
                    .map(|d| d.display().to_string())
                    .unwrap_or_default(),
            }
        };
        let text = doctor::report(&info, &self.checks.borrow());
        self.controller.borrow_mut().copy_text(&text);
        redraw_all();
    }

    fn open_url(&self, url: &str) {
        self.opened(self.host.open_url(url), "a link");
    }
}

/// A bound Help window. Dropping it does not unbind the window.
pub struct HelpBinding<C, K, T> {
    core: Rc<HelpCore<C, K, T>>,
}

impl<C, K, T> std::fmt::Debug for HelpBinding<C, K, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HelpBinding").finish_non_exhaustive()
    }
}

impl<C, K, T> HelpBinding<C, K, T>
where
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// Binds `window`: the checks, the tools and the links. `data_dir` of `None` (the demo)
    /// switches the checks and the folder buttons off.
    pub fn new(
        controller: &Rc<RefCell<Controller<Store, C, K, T>>>,
        window: &ui::HelpWindow,
        data_dir: Option<PathBuf>,
        host: Rc<dyn HelpHost>,
    ) -> Self {
        let core = Rc::new_cyclic(|weak: &Weak<HelpCore<C, K, T>>| {
            let weak = weak.clone();
            HelpCore {
                controller: Rc::clone(controller),
                window: window.as_weak(),
                data_dir,
                host,
                checks: RefCell::new(Vec::new()),
                redraw: Rc::new(move || {
                    if let Some(core) = weak.upgrade() {
                        core.sync();
                    }
                }),
            }
        });
        register_redraw(&core.redraw);

        macro_rules! wire {
            ($on:ident, |$core:ident $(, $arg:ident)*| $body:expr) => {{
                let $core = Rc::clone(&core);
                window.$on(move |$($arg),*| {
                    crate::trace!("callback help {}", stringify!($on));
                    $body;
                });
            }};
        }
        wire!(on_check_again_requested, |core| {
            core.run_checks();
            redraw_all();
        });
        wire!(on_fix_requested, |core, fix| core.fix(fix));
        wire!(on_open_data_folder_requested, |core| {
            if let Some(dir) = core.data_dir.clone() {
                core.open_folder(&dir);
            }
        });
        wire!(on_open_logs_folder_requested, |core| core
            .open_logs_folder());
        wire!(on_copy_report_requested, |core| core.copy_report());
        wire!(on_guide_requested, |core| core.open_url(GUIDE_URL));
        wire!(on_tour_requested, |_core| {
            crate::onboarding_binding::request_open();
        });
        wire!(on_source_requested, |core| core.open_url(REPO_URL));
        wire!(on_releases_requested, |core| core.open_url(RELEASES_URL));
        wire!(on_store_requested, |core| {
            if let Some(url) = STORE_URL {
                core.open_url(url);
            }
        });
        wire!(on_notices_requested, |core| {
            if let Some(file) = core.host.notices_file() {
                core.opened(core.host.open_file(&file), "the notices");
            }
        });
        {
            let weak = window.as_weak();
            window.on_close_requested(move || {
                if let Some(window) = weak.upgrade() {
                    let _ = window.hide();
                }
            });
        }
        Self { core }
    }

    /// Runs the checks again and draws the window.
    pub fn refresh(&self) {
        self.core.run_checks();
        self.core.sync();
    }
}

// ---- Opening the window on demand --------------------------------------------------------------

/// Creates the Help window the first time it is asked for, binds it, and shows and raises it
/// every time (one window: asking again raises it). Not generic: the host keeps one of these.
pub struct HelpUi {
    open: Box<dyn Fn(HelpTopic) -> Result<(), slint::PlatformError>>,
    host: Rc<dyn HelpHost>,
}

impl std::fmt::Debug for HelpUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HelpUi").finish_non_exhaustive()
    }
}

impl HelpUi {
    /// A Help window for `controller`, not created yet. `data_dir` is the library's folder
    /// (`None` in the demo) and `host` does what the window asks of the machine.
    pub fn new<C, K, T>(
        controller: &Rc<RefCell<Controller<Store, C, K, T>>>,
        data_dir: Option<PathBuf>,
        host: Rc<dyn HelpHost>,
    ) -> Self
    where
        C: Clipboard + ClipboardText + 'static,
        K: Keyboard + 'static,
        T: Timing + 'static,
    {
        type Slot<C, K, T> = Rc<RefCell<Option<(ui::HelpWindow, HelpBinding<C, K, T>)>>>;
        let slot: Slot<C, K, T> = Rc::default();
        let open = {
            let controller = Rc::clone(controller);
            let host = Rc::clone(&host);
            move |topic: HelpTopic| -> Result<(), slint::PlatformError> {
                let mut slot = slot.borrow_mut();
                if slot.is_none() {
                    let window = RegularWindow::create(ui::HelpWindow::new)?;
                    crate::native_window::size_before_show(
                        window.window(),
                        (window.get_wanted_width(), window.get_wanted_height()),
                    );
                    let binding =
                        HelpBinding::new(&controller, &window, data_dir.clone(), Rc::clone(&host));
                    *slot = Some((window, binding));
                }
                let Some((window, binding)) = slot.as_ref() else {
                    return Ok(());
                };
                window.set_tab(i32::from(topic == HelpTopic::About));
                window.show()?;
                fit_to_screen(window.window());
                // The checks are fresh every time the window comes up.
                binding.refresh();
                raise(window.window());
                Ok(())
            }
        };
        Self {
            open: Box::new(open),
            host,
        }
    }

    /// Shows the page of `topic`: the Help window on Troubleshoot or About (created if this is
    /// the first time, raised if it is open), or the user guide in the browser.
    ///
    /// # Errors
    ///
    /// The windowing system failed to create or show the window.
    pub fn open(&self, topic: HelpTopic) -> Result<(), slint::PlatformError> {
        if topic == HelpTopic::Guide {
            if let Err(error) = self.host.open_url(GUIDE_URL) {
                crate::trace!("help: cannot open the user guide: {error}");
            }
            return Ok(());
        }
        (self.open)(topic)
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    #[test]
    fn the_links_point_at_the_repository() {
        assert!(RELEASES_URL.starts_with(REPO_URL));
        assert!(GUIDE_URL.starts_with(REPO_URL));
        assert!(GUIDE_URL.ends_with("docs/user-guide"));
    }

    #[test]
    fn the_report_flags_are_switches_and_numbers_never_typed_text() {
        let settings = crate::settings::Settings {
            no_paste_apps: vec!["keepass.exe".into()],
            language: "de".into(),
            ..crate::settings::Settings::default()
        };
        let flags = report_flags(&settings);
        let text = flags.iter().fold(String::new(), |mut s, (n, v)| {
            let _ = write!(s, "{n}={v};");
            s
        });
        assert!(!text.contains("keepass"));
        assert!(!text.contains("de;"));
        assert!(flags.contains(&("Programs that never get a paste".to_owned(), "1".to_owned())));
    }
}
