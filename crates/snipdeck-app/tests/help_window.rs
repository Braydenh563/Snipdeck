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
    controller::{Controller, Services},
    demo,
    help_binding::{HelpBinding, HelpHost},
    host::{FakeTargets, FixedClock},
    settings::{Hotkeys, Settings},
    shortcuts::CATALOGUE,
};
use snipdeck_core::{Entitlements, Tier, Timestamp};
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_store::Store;
use snipdeck_ui::{HelpWindow, ShortcutTexts};

#[path = "support/mod.rs"]
mod support;
use support::on_ui_thread;

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
    fn notices_file(&self) -> Option<PathBuf> {
        None
    }
}

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

struct Rig {
    ui: HelpWindow,
    binding: HelpBinding<FakeClipboard, FakeKeyboard, FakeTiming>,
}

fn rig(settings: Settings) -> Rig {
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
            Entitlements::new(Tier::Pro),
        )
        .expect("controller"),
    ));
    let ui = HelpWindow::new().expect("help window");
    ui.window().set_size(slint::LogicalSize::new(600.0, 900.0));
    let binding = HelpBinding::new(&ctl, &ui, None, Rc::new(NoHost));
    ui.set_tab(2);
    ui.show().expect("show");
    binding.refresh();
    Rig { ui, binding }
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

#[test]
fn the_tab_lists_every_shortcut_in_five_groups_with_the_note_under_the_global_ones() {
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
                "Editors"
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
            ("Quick capture the clipboard".to_owned(), String::new()),
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
            found.contains(&"Keep the Panel on top (pin)".to_owned()),
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
