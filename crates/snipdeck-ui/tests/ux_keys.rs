//! UX audit UX-1..4, UX-21: one chord means one thing in every window. Ctrl+W closes a window, Ctrl+S saves
//! in an editor, F1 opens the guide, Alt+P (not Ctrl+P, which pins) toggles the editor's Preview, and the
//! Help window keeps its keyboard focus after its Rebuild dialog is cancelled.

#![allow(clippy::expect_used, clippy::unwrap_used, reason = "test code")]

use std::{cell::Cell, rc::Rc};

use slint::{ComponentHandle, platform::Key};
use snipdeck_ui::{HelpWindow, ItemEditor, ListEditor, SettingsPage, TagManager};

struct H<T> {
    component: T,
}
impl<T> H<T> {
    fn of(component: T) -> Self {
        Self { component }
    }
}

#[path = "support/editor_fixture.rs"]
mod editor;
#[path = "support/mod.rs"]
mod support;

use support::{chord, on_ui_thread, press};

fn flag() -> (Rc<Cell<u32>>, Rc<Cell<u32>>) {
    let c = Rc::new(Cell::new(0));
    (c.clone(), c)
}

#[test]
fn ctrl_w_closes_help_settings_and_the_tag_manager() {
    on_ui_thread(|| {
        let h = H::of(HelpWindow::new().unwrap());
        editor::help(&h.component);
        let (n, m) = flag();
        h.component.on_close_requested(move || m.set(m.get() + 1));
        chord(h.component.window(), &[Key::Control], "w");
        assert_eq!(n.get(), 1, "Help closes on Ctrl+W");

        let h = H::of(SettingsPage::new().unwrap());
        editor::settings(&h.component);
        let (n, m) = flag();
        h.component.on_close_requested(move || m.set(m.get() + 1));
        press(h.component.window(), &support::key(Key::Tab));
        chord(h.component.window(), &[Key::Control], "w");
        assert_eq!(n.get(), 1, "Settings closes on Ctrl+W");

        let h = H::of(TagManager::new().unwrap());
        editor::tag_manager(&h.component);
        let (n, m) = flag();
        h.component.on_close_requested(move || m.set(m.get() + 1));
        press(h.component.window(), &support::key(Key::Tab));
        chord(h.component.window(), &[Key::Control], "w");
        assert_eq!(n.get(), 1, "the Tag manager closes on Ctrl+W");
        let (f, g) = flag();
        h.component.on_help_requested(move || g.set(g.get() + 1));
        press(h.component.window(), &support::key(Key::F1));
        assert_eq!(f.get(), 1, "the Tag manager opens the guide on F1");
    });
}

#[test]
fn editors_save_on_ctrl_s_and_close_on_ctrl_w() {
    on_ui_thread(|| {
        let h = H::of(ItemEditor::new().unwrap());
        editor::text_item(&h.component);
        let (saved, s) = flag();
        h.component.on_save_requested(move || s.set(s.get() + 1));
        let (closed, c) = flag();
        h.component.on_cancel_requested(move || c.set(c.get() + 1));
        chord(h.component.window(), &[Key::Control], "s");
        assert_eq!(saved.get(), 1, "the item editor saves on Ctrl+S");
        chord(h.component.window(), &[Key::Control], "w");
        assert_eq!(closed.get(), 1, "the item editor closes on Ctrl+W");

        let h = H::of(ListEditor::new().unwrap());
        editor::list_editor_existing(&h.component);
        let (saved, s) = flag();
        h.component.on_save_requested(move || s.set(s.get() + 1));
        chord(h.component.window(), &[Key::Control], "s");
        assert_eq!(saved.get(), 1, "the list editor saves on Ctrl+S");
    });
}

#[test]
fn preview_is_alt_p_and_ctrl_p_is_left_for_pin() {
    on_ui_thread(|| {
        let h = H::of(ItemEditor::new().unwrap());
        editor::text_item(&h.component);
        let win = h.component.window();
        chord(win, &[Key::Control], "p");
        assert!(
            !h.component.get_preview_on(),
            "Ctrl+P no longer toggles Preview"
        );
        chord(win, &[Key::Alt], "p");
        assert!(h.component.get_preview_on(), "Alt+P turns Preview on");
        chord(win, &[Key::Alt], "p");
        assert!(!h.component.get_preview_on(), "Alt+P turns Preview off");
    });
}

#[test]
fn help_keeps_the_keyboard_after_the_rebuild_dialog_is_cancelled() {
    on_ui_thread(|| {
        let h = H::of(HelpWindow::new().unwrap());
        editor::help(&h.component);
        let (n, m) = flag();
        h.component.on_close_requested(move || m.set(m.get() + 1));
        let win = h.component.window();
        h.component.on_rebuild_dismissed({
            let weak = h.component.as_weak();
            move || weak.upgrade().unwrap().set_rebuild_open(false)
        });
        h.component.set_rebuild_open(true);
        press(win, &support::key(Key::Escape)); // the dialog's own Esc: dismissed, not close
        assert_eq!(n.get(), 0, "Esc inside the dialog does not close Help");
        press(win, &support::key(Key::Escape));
        assert_eq!(
            n.get(),
            1,
            "after the dialog closes, Esc reaches the window again"
        );
    });
}
