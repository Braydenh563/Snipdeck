//! The real `HelpWindow` bound to a `Controller` over the demo library and the platform fakes:
//! the Shortcuts tab (grouped lines, the search box, the configured global hotkeys). Time is
//! mocked, never the real clock. One process for this file (the testing platform is process-wide).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};

use slint::{ComponentHandle, Global, Model};
use snipdeck_app::{
    controller::{Controller, Effect, HelpTopic, Services},
    demo,
    help_binding::{HelpBinding, HelpHost, tab_of},
    host::{FakeTargets, FixedClock},
    settings::{Hotkeys, Settings},
    shortcuts::CATALOGUE,
    tips::TIPS,
};
use snipdeck_core::Timestamp;
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_store::Store;
use snipdeck_ui::{HelpWindow, ShortcutTexts, TipTexts};

#[path = "support/mod.rs"]
mod support;
use support::{ArchiveSteps as _, HeldArchive, on_ui_thread};

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

struct NoHost;

impl HelpHost for NoHost {
    fn open_folder(&self, _: &Path) -> Result<(), String> {
        Ok(())
    }
    fn open_url(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn open_file(&self, _: &Path) -> Result<(), String> {
        Ok(())
    }
    fn os_summary(&self) -> String {
        "test".into()
    }
    fn write_notices(&self) -> Result<PathBuf, String> {
        Ok(PathBuf::from("notices.txt"))
    }
}

/// A host that remembers every address it was asked to open.
#[derive(Default)]
struct RecordingHost {
    urls: RefCell<Vec<String>>,
}

impl HelpHost for RecordingHost {
    fn open_folder(&self, folder: &Path) -> Result<(), String> {
        NoHost.open_folder(folder)
    }
    fn open_url(&self, url: &str) -> Result<(), String> {
        self.urls.borrow_mut().push(url.to_owned());
        Ok(())
    }
    fn open_file(&self, file: &Path) -> Result<(), String> {
        NoHost.open_file(file)
    }
    fn os_summary(&self) -> String {
        NoHost.os_summary()
    }
    fn write_notices(&self) -> Result<PathBuf, String> {
        Ok(PathBuf::from("notices.txt"))
    }
}

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

struct Rig {
    ctl: Rc<RefCell<Ctl>>,
    ui: HelpWindow,
    binding: HelpBinding<FakeClipboard, FakeKeyboard, FakeTiming>,
}

fn rig(settings: Settings) -> Rig {
    rig_on(settings)
}

fn rig_on(settings: Settings) -> Rig {
    let ctl: Rc<RefCell<Ctl>> = Rc::new(RefCell::new(
        Controller::new(
            demo::store(now()).expect("demo store"),
            Services {
                clipboard: FakeClipboard::default(),
                keyboard: FakeKeyboard::default(),
                timing: FakeTiming::default(),
            },
            Box::new(FakeTargets::new(Some(demo::target()))),
            Box::new(FixedClock::new(now())),
            settings,
        )
        .expect("controller"),
    ));
    let ui = HelpWindow::new().expect("help window");
    ui.window().set_size(slint::LogicalSize::new(600.0, 900.0));
    let binding = HelpBinding::new(&ctl, &ui, None, Rc::new(NoHost));
    ui.set_tab(2);
    ui.show().expect("show");
    binding.refresh();
    Rig { ctl, ui, binding }
}

impl Rig {
    /// `(group title, [line title, keys joined with +])` as the window shows them now.
    fn shown(&self) -> Vec<(String, Vec<(String, String)>)> {
        self.ui
            .get_shortcut_groups()
            .iter()
            .map(|g| {
                (
                    g.title.to_string(),
                    g.lines
                        .iter()
                        .map(|l| {
                            let keys: Vec<String> = l.keys.iter().map(|k| k.to_string()).collect();
                            (l.title.to_string(), keys.join("+"))
                        })
                        .collect(),
                )
            })
            .collect()
    }

    fn titles(&self) -> Vec<String> {
        self.shown()
            .into_iter()
            .flat_map(|(_, lines)| lines.into_iter().map(|(title, _)| title))
            .collect()
    }
}

#[test]
fn the_shortcut_texts_have_one_title_per_shortcut_and_one_name_per_scope() {
    on_ui_thread(|| {
        let r = rig(Settings::default());
        let texts = ShortcutTexts::get(&r.ui);
        assert_eq!(
            texts.get_titles().row_count(),
            CATALOGUE.len(),
            "`ShortcutTexts.titles` (shortcut_texts.slint) is in the order of `shortcuts::CATALOGUE`"
        );
        assert_eq!(
            texts.get_scopes().row_count(),
            snipdeck_app::shortcuts::Scope::ALL.len()
        );
        let _ = &r.binding;
    });
}

/// One chord means one thing in every window (DESIGN.md "Shortcut conventions"): a chord with `Ctrl` or
/// `Alt` that two scopes list carries the same words in both. `Esc` and `Enter` are context keys and are not
/// checked. The pairs below are the same act worded for the window (the Panel opens the palette in Main).
#[test]
fn a_ctrl_or_alt_chord_has_the_same_title_in_every_scope_that_lists_it() {
    use snipdeck_app::shortcuts::Keys;
    const SAME_ACT: &[(&str, &str)] = &[
        ("Commands (every action)", "Commands in the Main window"),
        ("New item (quick add in an open list)", "New item"),
        ("Close the window", "Close"),
        ("Close the window", "Hide the Panel"),
        ("Close the window", "Clear the search, then close"),
        (
            "Move the item up or down its list",
            "Move the group up or down",
        ),
        ("Move the item up or down its list", "Move the group up"),
        ("Move the item up or down its list", "Move the group down"),
        ("Edit the item", "Edit the result"),
        ("Select every item", "Select several items"),
    ];
    on_ui_thread(|| {
        let r = rig(Settings::default());
        let titles: Vec<String> = ShortcutTexts::get(&r.ui)
            .get_titles()
            .iter()
            .map(|t| t.to_string())
            .collect();
        let mut seen: Vec<(&str, String)> = Vec::new();
        for (s, title) in CATALOGUE.iter().zip(&titles) {
            let Keys::Chord(chord) = s.keys else { continue };
            if !(chord.contains("Ctrl") || chord.contains("Alt")) {
                continue;
            }
            for (c, other_title) in &seen {
                if *c != chord || other_title == title {
                    continue;
                }
                let same = SAME_ACT.iter().any(|(a, b)| {
                    (a == title && b == other_title) || (b == title && a == other_title)
                });
                assert!(
                    same,
                    "{chord} is `{other_title}` in one scope and `{title}` in another"
                );
            }
            seen.push((chord, title.clone()));
        }
        let _ = &r.binding;
    });
}

#[test]
fn the_tab_lists_every_shortcut_in_six_groups_with_the_note_under_the_global_ones() {
    on_ui_thread(|| {
        let r = rig(Settings::default());
        let shown = r.shown();
        let names: Vec<&str> = shown.iter().map(|(g, _)| g.as_str()).collect();
        assert_eq!(
            names,
            [
                "Anywhere in Windows",
                "Main window",
                "Panel",
                "Picker",
                "Editors",
                "Unlock window"
            ]
        );
        let total: usize = shown.iter().map(|(_, lines)| lines.len()).sum();
        assert_eq!(total, CATALOGUE.len());
        let first = r.ui.get_shortcut_groups().row_data(0).expect("group");
        assert_eq!(first.note, "Change these in Settings.");
        let second = r.ui.get_shortcut_groups().row_data(1).expect("group");
        assert_eq!(second.note, "");
    });
}

#[test]
fn the_global_hotkeys_show_the_chords_in_the_settings() {
    on_ui_thread(|| {
        let r = rig(Settings {
            hotkeys: Hotkeys {
                toggle_panel: "Ctrl+Alt+K".into(),
                capture: String::new(),
                ..Hotkeys::new_install()
            },
            ..Settings::default()
        });
        let anywhere = &r.shown()[0].1;
        assert_eq!(
            anywhere[0],
            ("Show or hide the Panel".to_owned(), "Ctrl+Alt+K".to_owned())
        );
        assert_eq!(
            anywhere[2],
            ("Quick capture".to_owned(), String::new()),
            "an off hotkey has no keys"
        );
    });
}

#[test]
fn typing_in_the_search_box_filters_by_action_name_or_chord() {
    on_ui_thread(|| {
        let r = rig(Settings::default());
        r.ui.invoke_shortcut_query_changed("trash".into());
        // The Main window and, since the Panel's Delete key is listed, the Panel.
        assert_eq!(
            r.titles(),
            ["Move the item to the Trash", "Move the item to the Trash"]
        );
        assert_eq!(r.shown().len(), 2, "groups without a match are gone");

        r.ui.invoke_shortcut_query_changed("ctrl+shift+p".into());
        let found = r.titles();
        assert!(
            found.contains(&"Commands in the Main window".to_owned()),
            "{found:?}"
        );
        assert!(!found.contains(&"New list".to_owned()), "{found:?}");

        r.ui.invoke_shortcut_query_changed("nothing like this".into());
        assert_eq!(r.titles(), Vec::<String>::new());

        r.ui.invoke_shortcut_query_changed(String::new().into());
        assert_eq!(
            r.titles().len(),
            CATALOGUE.len(),
            "clearing brings every line back"
        );
    });
}

#[test]
fn tips_topic_opens_the_fourth_tab() {
    on_ui_thread(|| {
        assert_eq!(tab_of(HelpTopic::Tips), 3, "Tips is the fourth tab");
        assert_ne!(tab_of(HelpTopic::Shortcuts), tab_of(HelpTopic::Tips));
        let r = rig(Settings::default());
        r.ui.set_tab(tab_of(HelpTopic::Tips));
        r.binding.refresh();
        let words = TipTexts::get(&r.ui);
        for list in [
            words.get_titles().row_count(),
            words.get_first_lines().row_count(),
            words.get_second_lines().row_count(),
        ] {
            assert_eq!(list, TIPS.len(), "one entry per tip in every TipTexts list");
        }
        let cards: Vec<_> = r.ui.get_tips().iter().collect();
        assert_eq!(cards.len(), TIPS.len());
        for card in &cards {
            assert!(!card.title.is_empty() && !card.first_line.is_empty());
            assert!(!card.second_line.is_empty());
            assert!((0..11).contains(&card.art), "{}: no picture", card.title);
        }
        assert_eq!(cards[0].title.as_str(), "Open the Panel from anywhere");
        // The template card shows the two placeholder examples, not their format holes.
        assert!(cards[4].second_line.contains("{field:Name}"));
    });
}

#[test]
fn try_it_runs_the_tip_action() {
    on_ui_thread(|| {
        let r = rig(Settings::default());
        r.ui.set_tab(3);
        r.binding.refresh();
        // The Pinned card shows its result in the Main window: it must come forward.
        let pinned = TIPS
            .iter()
            .position(|t| t.action.command() == snipdeck_app::commands::Command::GoPinned)
            .expect("a Pinned card");
        let _ = r.ctl.borrow_mut().take_pending();
        r.ui.invoke_try_tip_requested(i32::try_from(pinned).expect("index"));
        let effects = r.ctl.borrow_mut().take_pending();
        assert!(effects.contains(&Effect::OpenMainWindow), "{effects:?}");

        // The Shortcuts card asks for Help on the Shortcuts tab.
        let shortcuts = TIPS
            .iter()
            .position(|t| t.action.command() == snipdeck_app::commands::Command::Shortcuts)
            .expect("a Shortcuts card");
        r.ui.invoke_try_tip_requested(i32::try_from(shortcuts).expect("index"));
        let effects = r.ctl.borrow_mut().take_pending();
        assert!(
            effects.contains(&Effect::OpenHelp(HelpTopic::Shortcuts)),
            "{effects:?}"
        );

        // A stale index does nothing.
        r.ui.invoke_try_tip_requested(99);
        assert_eq!(r.ctl.borrow_mut().take_pending(), Vec::new());
    });
}

#[test]
fn the_recent_card_says_what_recent_holds() {
    on_ui_thread(|| {
        let r = rig(Settings::default());
        r.binding.refresh();
        let cards: Vec<_> = r.ui.get_tips().iter().collect();
        let capture = cards
            .iter()
            .find(|c| c.title.as_str() == "Grab text as you go")
            .expect("the capture card");
        assert!(
            capture.second_line.contains("copy in other programs"),
            "{}",
            capture.second_line
        );
    });
}

#[test]
fn the_about_page_support_row_opens_exactly_the_coffee_page_and_nothing_else() {
    on_ui_thread(|| {
        let r = rig(Settings::default());
        let ctl = Rc::clone(&r.ctl);
        let ui = HelpWindow::new().expect("help window");
        let host = Rc::new(RecordingHost::default());
        let binding = HelpBinding::new(&ctl, &ui, None, Rc::clone(&host) as Rc<dyn HelpHost>);
        ui.show().expect("show");
        binding.refresh();
        assert!(host.urls.borrow().is_empty(), "nothing opens by itself");
        ui.invoke_support_requested();
        assert_eq!(
            *host.urls.borrow(),
            ["https://buymeacoffee.com/braydenh563".to_owned()]
        );
        assert_eq!(
            snipdeck_app::help_binding::SUPPORT_URL,
            host.urls.borrow()[0]
        );
        assert_eq!(
            r.ctl.borrow_mut().take_pending(),
            Vec::new(),
            "nothing unlocked"
        );
    });
}

// ---- The checks run off the UI thread (HELP-1) ---------------------------------------------------

type HeldRun = (
    snipdeck_app::help_binding::CheckWork,
    snipdeck_app::help_binding::CheckFinish,
);

/// Holds the checks instead of running them, so a test can look at the window while they wait.
#[derive(Clone, Default)]
struct HeldChecks {
    held: Rc<RefCell<Vec<HeldRun>>>,
}

impl snipdeck_app::help_binding::CheckOffload for HeldChecks {
    fn run(
        &self,
        work: snipdeck_app::help_binding::CheckWork,
        finish: snipdeck_app::help_binding::CheckFinish,
    ) {
        self.held.borrow_mut().push((work, finish));
    }
}

impl HeldChecks {
    /// Runs the oldest waiting checks and hands the findings to the window, as the worker would.
    fn release(&self) {
        let (work, finish) = self.held.borrow_mut().remove(0);
        finish(work());
    }
}

/// Puts the inline checks back when dropped.
struct InlineChecks;

impl Drop for InlineChecks {
    fn drop(&mut self) {
        snipdeck_app::help_binding::set_check_offload(Rc::new(
            snipdeck_app::help_binding::CheckInline,
        ));
    }
}

fn rig_with_folder(dir: &Path) -> Rig {
    let ctl: Rc<RefCell<Ctl>> = Rc::new(RefCell::new(
        Controller::new(
            demo::store(now()).expect("demo store"),
            Services {
                clipboard: FakeClipboard::default(),
                keyboard: FakeKeyboard::default(),
                timing: FakeTiming::default(),
            },
            Box::new(FakeTargets::new(Some(demo::target()))),
            Box::new(FixedClock::new(now())),
            Settings::default(),
        )
        .expect("controller"),
    ));
    let ui = HelpWindow::new().expect("help window");
    let binding = HelpBinding::new(&ctl, &ui, Some(dir.to_path_buf()), Rc::new(NoHost));
    ui.show().expect("show");
    Rig { ctl, ui, binding }
}

#[test]
fn the_checks_wait_on_a_worker_and_the_window_says_so() {
    on_ui_thread(|| {
        let dir = tempfile::tempdir().expect("dir");
        // The library file the worker's read-only handle opens.
        drop(Store::open(dir.path()).expect("store"));
        let held = HeldChecks::default();
        snipdeck_app::help_binding::set_check_offload(Rc::new(held.clone()));
        let _back = InlineChecks;
        let r = rig_with_folder(dir.path());
        r.binding.refresh();
        assert!(r.ui.get_checking(), "the window says it is checking");
        assert_eq!(r.ui.get_checks().row_count(), 0);
        assert_eq!(held.held.borrow().len(), 1);
        // A second run (Check again) supersedes the first: its late findings are dropped.
        r.ui.invoke_check_again_requested();
        assert_eq!(held.held.borrow().len(), 2);
        held.release();
        assert!(r.ui.get_checking(), "the superseded run changes nothing");
        assert_eq!(r.ui.get_checks().row_count(), 0);
        held.release();
        assert!(!r.ui.get_checking());
        assert_eq!(r.ui.get_checks().row_count(), 7);
        let first = r.ui.get_checks().row_data(0).expect("row");
        assert_eq!(first.label, "Data folder");
        assert_eq!(first.detail, "Writable, with room to spare.");
        let database = r.ui.get_checks().row_data(1).expect("row");
        assert_eq!(database.detail, "The integrity check found no problems.");
    });
}

#[test]
fn the_check_rows_follow_the_selected_translation() {
    on_ui_thread(|| {
        let dir = tempfile::tempdir().expect("dir");
        drop(Store::open(dir.path()).expect("store"));
        let r = rig_with_folder(dir.path());
        r.binding.refresh();
        assert_eq!(r.ui.get_checks().row_count(), 7);
        slint::select_bundled_translation("de").expect("de is bundled");
        r.binding.refresh();
        let tray = r.ui.get_checks().row_data(5).expect("row");
        let data = r.ui.get_checks().row_data(0).expect("row");
        slint::select_bundled_translation("").expect("back to the source language");
        assert_ne!(data.label, "Data folder", "the label is German");
        assert_ne!(data.detail, "Writable, with room to spare.");
        assert_ne!(tray.label, "Tray icon");
    });
}

#[test]
fn a_user_guide_that_will_not_open_says_so() {
    struct NoBrowser;
    impl HelpHost for NoBrowser {
        fn open_folder(&self, _: &Path) -> Result<(), String> {
            Ok(())
        }
        fn open_url(&self, _: &str) -> Result<(), String> {
            Err("no browser".into())
        }
        fn open_file(&self, _: &Path) -> Result<(), String> {
            Ok(())
        }
        fn os_summary(&self) -> String {
            "test".into()
        }
        fn write_notices(&self) -> Result<PathBuf, String> {
            Ok(PathBuf::from("notices.txt"))
        }
    }
    on_ui_thread(|| {
        let r = rig(Settings::default());
        let help = snipdeck_app::help_binding::HelpUi::new(&r.ctl, None, Rc::new(NoBrowser));
        help.open(HelpTopic::Guide).expect("no window needed");
        let toast = r.ctl.borrow().view().toast.expect("a toast");
        assert_eq!(
            toast.message,
            snipdeck_app::toast::ToastMessage::LinkNotOpened
        );
    });
}

#[test]
fn rebuilding_restores_the_backup_the_question_named_or_says_it_is_gone() {
    on_ui_thread(|| {
        let dir = tempfile::tempdir().expect("dir");
        drop(Store::open(dir.path()).expect("store"));
        let r = rig_with_folder(dir.path());
        assert!(r.ctl.borrow_mut().backup_now(dir.path()));
        r.ctl.borrow_mut().toast_dismiss();
        r.ui.invoke_fix_requested(4);
        assert!(
            r.ui.get_rebuild_open(),
            "the question names the newest backup"
        );
        // The backup goes away before the answer: nothing is restored, and the user is told.
        for file in snipdeck_portable::backup::list_backups(dir.path()).expect("list") {
            std::fs::remove_file(file.path).expect("remove");
        }
        r.ui.invoke_rebuild_confirmed();
        assert!(!r.ui.get_rebuild_open());
        let toast = r.ctl.borrow().view().toast.expect("a toast");
        assert!(matches!(
            toast.message,
            snipdeck_app::toast::ToastMessage::RestoreFailed(_)
        ));
    });
}

#[test]
fn rebuild_shows_the_busy_state_while_the_backup_is_read_and_then_the_confirmation() {
    on_ui_thread(|| {
        let dir = tempfile::tempdir().expect("dir");
        let mut store = Store::open(dir.path()).expect("store");
        snipdeck_app::demo::seed(&mut store, now()).expect("seed");
        snipdeck_portable::backup::run_if_due(
            &store,
            &snipdeck_portable::backup::backups_dir(dir.path()),
            &now().to_zoned(jiff::tz::TimeZone::UTC),
            7,
            None,
        )
        .expect("backup");
        let ctl: Rc<RefCell<Ctl>> = Rc::new(RefCell::new(
            Controller::new(
                store,
                Services {
                    clipboard: FakeClipboard::default(),
                    keyboard: FakeKeyboard::default(),
                    timing: FakeTiming::default(),
                },
                Box::new(FakeTargets::new(Some(demo::target()))),
                Box::new(FixedClock::new(now())),
                Settings::default(),
            )
            .expect("controller"),
        ));
        let ui = HelpWindow::new().expect("help window");
        let binding = HelpBinding::new(&ctl, &ui, Some(dir.path().to_owned()), Rc::new(NoHost));
        ui.show().expect("show");
        binding.refresh();
        assert!(!ui.get_archive_busy());

        let held = HeldArchive::default();
        let _inline_again = held.install();
        ui.invoke_fix_requested(4);
        assert_eq!(held.held(), 1, "the backup is read off this thread");
        assert!(ui.get_archive_busy(), "the window shows the busy state");
        assert!(
            !ui.get_rebuild_open(),
            "no confirmation until the read is done"
        );

        held.release();
        assert!(!ui.get_archive_busy());
        assert!(ui.get_rebuild_open());
        assert!(
            !ui.get_rebuild_date().is_empty(),
            "the confirmation names the backup"
        );
    });
}
