//! The Colours controller: current colour, use (paste or copy), recents, saving to the home
//! list or any list, the CSS variables export, and a colour picked from the screen.

use snipdeck_colour::{Format as Fmt, Rgba8};

use super::*;
use crate::controller::colour_ctl::SaveTarget;

const BLUE: Rgba8 = Rgba8::rgb(58, 123, 213);

fn written(w: &World, n: usize) -> String {
    match &w.ctl.services().clipboard.written[n].data()[0] {
        Data::Text(t) => t.clone(),
        other => panic!("not text: {other:?}"),
    }
}

fn items_of(w: &World, list: snipdeck_core::ListId) -> Vec<(String, String)> {
    w.ctl
        .store()
        .items(list)
        .expect("items")
        .into_iter()
        .map(|i| {
            let text = crate::usecase::item_text(w.ctl.store(), i.id)
                .expect("text")
                .unwrap_or_default();
            (i.title, text)
        })
        .collect()
}

fn colours_list(w: &World) -> Option<snipdeck_core::ListId> {
    let id = w.ctl.settings().colour_save_list.as_deref()?;
    id.parse().ok()
}

/// Adds a snippet through the quick-add box and returns its id.
fn quick(w: &mut World, text: &str) -> ItemId {
    assert!(w.ctl.quick_add(
        text,
        Some(ItemKind::Text),
        false,
        crate::item_editor::Origin::Panel
    ));
    w.ctl
        .store()
        .all_items()
        .expect("items")
        .into_iter()
        .find(|i| {
            crate::usecase::item_text(w.ctl.store(), i.id)
                .ok()
                .flatten()
                .as_deref()
                == Some(text)
        })
        .expect("the new item")
        .id
}

#[test]
fn the_tab_starts_on_the_saved_colour_and_setting_it_pushes_no_recent() {
    let mut w = world();
    assert_eq!(w.ctl.colour_view().current, Rgba8::rgb(0x28, 0x70, 0xd4));
    w.ctl.set_colour(BLUE);
    assert_eq!(w.ctl.colour_view().current, BLUE);
    assert_eq!(w.ctl.settings().colour_current, "#3a7bd5");
    assert!(w.ctl.colour_view().recents.is_empty());
}

#[test]
fn a_format_row_pastes_with_a_target_and_remembers_the_colour_at_the_fixed_time() {
    let mut w = world();
    let result = w.ctl.use_colour(BLUE, Fmt::Rgb, UseMode::Paste);
    assert_eq!(result, UseResult::Done(UseOutcome::Pasted));
    assert_eq!(written(&w, 0), "rgb(58, 123, 213)");
    assert_eq!(w.ctl.services().keyboard.sent.len(), 1, "Ctrl+V went out");
    assert_eq!(w.ctl.colour_view().recents, [BLUE]);
    let saved = &w.ctl.settings().colours_recent;
    assert_eq!(saved[0].hex, "#3a7bd5");
    assert_eq!(
        saved[0].used_at,
        at("2026-10-17T10:30:00Z").as_millisecond(),
        "stamped by the injected clock"
    );
}

#[test]
fn shift_or_no_target_copies_without_pressing_keys() {
    let mut w = world();
    assert_eq!(
        w.ctl.use_colour(BLUE, Fmt::Hex, UseMode::Copy),
        UseResult::Done(UseOutcome::Copied)
    );
    assert_eq!(written(&w, 0), "#3a7bd5");
    assert_eq!(w.ctl.services().keyboard.sent.len(), 0);

    let mut w = world_with(None, Settings::default());
    assert_eq!(
        w.ctl.use_colour(BLUE, Fmt::Oklch, UseMode::Paste),
        UseResult::Done(UseOutcome::Copied)
    );
    assert_eq!(written(&w, 0), "oklch(58.62% 0.1533 257.23)");
    assert_eq!(w.ctl.services().keyboard.sent.len(), 0);
}

#[test]
fn the_name_row_copies_the_bare_keyword() {
    let mut w = world();
    w.ctl
        .use_colour(Rgba8::rgb(0x41, 0x69, 0xe1), Fmt::Name, UseMode::Copy);
    assert_eq!(written(&w, 0), "royalblue");
}

#[test]
fn recents_dedupe_newest_first_and_can_be_forgotten() {
    let mut w = world();
    let red = Rgba8::rgb(255, 0, 0);
    w.ctl.use_colour(BLUE, Fmt::Hex, UseMode::Copy);
    w.ctl.use_colour(red, Fmt::Hex, UseMode::Copy);
    w.ctl.use_colour(BLUE, Fmt::Hex, UseMode::Copy);
    assert_eq!(w.ctl.colour_view().recents, [BLUE, red]);
    w.ctl.forget_recent_colour(BLUE);
    assert_eq!(w.ctl.colour_view().recents, [red]);
}

#[test]
fn a_busy_clipboard_toasts_and_remembers_nothing() {
    let mut w = build(Opts {
        clipboard_busy: true,
        ..Opts::default()
    });
    assert_eq!(
        w.ctl.use_colour(BLUE, Fmt::Hex, UseMode::Copy),
        UseResult::Failed
    );
    assert!(w.ctl.colour_view().recents.is_empty());
}

#[test]
fn the_other_colour_swaps_and_the_choices_are_remembered() {
    let mut w = world();
    w.ctl.set_colour(BLUE);
    w.ctl.set_contrast_with(Rgba8::rgb(0, 0, 0));
    w.ctl.swap_contrast();
    let v = w.ctl.colour_view();
    assert_eq!((v.current, v.other), (Rgba8::rgb(0, 0, 0), BLUE));
    w.ctl.set_colour_format(Fmt::Hsl);
    w.ctl.set_colour_format(Fmt::Name);
    assert_eq!(
        w.ctl.colour_view().format,
        Fmt::Hsl,
        "Name cannot be the default"
    );
    w.ctl.set_colour_scheme(snipdeck_colour::Scheme::Triadic);
    assert_eq!(
        w.ctl.settings().colour_scheme,
        snipdeck_colour::Scheme::Triadic
    );
}

#[test]
fn ctrl_s_makes_the_colours_list_once_at_the_end_and_titles_an_exact_name() {
    let mut w = world();
    let before = w
        .ctl
        .store()
        .lists(w.ctl.store().boards().unwrap()[0].id)
        .unwrap()
        .len();
    assert!(w.ctl.save_colour(BLUE, SaveTarget::Home));
    let list = colours_list(&w).expect("remembered by id");
    let lists = w
        .ctl
        .store()
        .lists(w.ctl.store().boards().unwrap()[0].id)
        .unwrap();
    assert_eq!(lists.len(), before + 1);
    assert_eq!(lists.last().unwrap().id, list, "at the end of the board");
    assert_eq!(lists.last().unwrap().name.as_str(), "Colours");
    assert_eq!(items_of(&w, list), [(String::new(), "#3a7bd5".to_owned())]);

    assert!(
        w.ctl
            .save_colour(Rgba8::rgb(0x41, 0x69, 0xe1), SaveTarget::Home)
    );
    assert_eq!(colours_list(&w), Some(list), "the same list again");
    assert_eq!(
        items_of(&w, list)[1],
        ("royalblue".to_owned(), "#4169e1".to_owned())
    );
    assert_eq!(w.ctl.colour_view().recents[0], Rgba8::rgb(0x41, 0x69, 0xe1));
}

#[test]
fn a_default_format_that_would_round_the_colour_saves_the_hex_instead() {
    let mut w = world();
    w.ctl.set_colour_format(Fmt::Hsl);
    assert!(w.ctl.save_colour(BLUE, SaveTarget::Home));
    let list = colours_list(&w).unwrap();
    assert_eq!(
        items_of(&w, list)[0].1,
        "#3a7bd5",
        "hsl(215, 65%, 53%) is not #3a7bd5"
    );
    // An exact format is kept as chosen.
    w.ctl.set_colour_format(Fmt::Rgb);
    assert!(w.ctl.save_colour(BLUE, SaveTarget::Home));
    assert_eq!(items_of(&w, list)[1].1, "rgb(58, 123, 213)");
}

#[test]
fn a_deleted_home_list_is_made_again_on_the_next_save() {
    let mut w = world();
    w.ctl.save_colour(BLUE, SaveTarget::Home);
    let first = colours_list(&w).unwrap();
    w.ctl.trash_list(first, "Colours".to_owned());
    assert!(w.ctl.save_colour(BLUE, SaveTarget::Home));
    let second = colours_list(&w).unwrap();
    assert_ne!(first, second);
    assert_eq!(items_of(&w, second).len(), 1);
}

#[test]
fn save_to_any_list_puts_the_colour_at_the_end_of_that_list() {
    let mut w = world();
    let links = w.links_list;
    assert!(w.ctl.save_colour(BLUE, SaveTarget::List(links)));
    let items = items_of(&w, links);
    assert_eq!(items.last().unwrap().1, "#3a7bd5");
    assert_eq!(colours_list(&w), None, "the home list was not made");
}

#[test]
fn saved_colours_are_the_colour_items_of_the_home_list() {
    let mut w = world();
    let red = Rgba8::rgb(255, 0, 0);
    w.ctl.save_colour(BLUE, SaveTarget::Home);
    w.ctl.save_colour(red, SaveTarget::Home);
    let saved = w.ctl.saved_colours();
    assert_eq!(
        saved.iter().map(|(_, c)| *c).collect::<Vec<_>>(),
        [BLUE, red]
    );
    assert_eq!(w.ctl.colour_of_item(saved[0].0), Some(BLUE));
    // A plain snippet is not a colour.
    assert_eq!(w.ctl.colour_of_item(w.ids[0]), None);
    assert_eq!(w.ctl.colour_view().saved, [BLUE, red]);
}

#[test]
fn a_snippet_that_is_only_a_colour_literal_is_a_colour_anywhere() {
    let mut w = world();
    let id = quick(&mut w, "  rgb(10, 20, 30) ");
    assert_eq!(w.ctl.colour_of_item(id), Some(Rgba8::rgb(10, 20, 30)));
    let bare = quick(&mut w, "red");
    assert_eq!(
        w.ctl.colour_of_item(bare),
        None,
        "a bare keyword is a word, not a swatch"
    );
}

#[test]
fn open_in_colours_makes_it_current_and_shows_the_tab() {
    let mut w = world();
    w.ctl.save_colour(BLUE, SaveTarget::Home);
    w.ctl.set_colour(Rgba8::rgb(1, 2, 3));
    let id = w.ctl.saved_colours()[0].0;
    let effects = w.ctl.open_item_in_colours(id);
    assert_eq!(effects, [Effect::ShowPanel, Effect::ShowColours]);
    assert_eq!(w.ctl.colour_view().current, BLUE);
    assert!(w.ctl.open_item_in_colours(w.ids[0]).is_empty());
}

#[test]
fn copy_list_as_css_variables_names_each_colour_and_dedupes() {
    let mut w = world();
    w.ctl
        .save_colour(Rgba8::rgb(0x41, 0x69, 0xe1), SaveTarget::Home);
    w.ctl
        .save_colour(Rgba8::rgb(0x41, 0x69, 0xe1), SaveTarget::Home);
    w.ctl.save_colour(BLUE, SaveTarget::Home);
    let list = colours_list(&w).unwrap();
    assert!(w.ctl.copy_list_as_css(list));
    assert_eq!(
        written(&w, 0),
        ":root {\n  --royalblue: #4169e1;\n  --royalblue-2: #4169e1;\n  --3a7bd5: #3a7bd5;\n}"
    );
    // A list with no colour in it copies nothing.
    let n = w.ctl.services().clipboard.written.len();
    assert!(!w.ctl.copy_list_as_css(w.links_list));
    assert_eq!(w.ctl.services().clipboard.written.len(), n);
}

#[test]
fn a_colour_picked_from_the_screen_is_current_remembered_and_copied_when_asked() {
    let mut w = world();
    let effects = w.ctl.colour_picked(BLUE, false);
    assert_eq!(effects, [Effect::ShowPanel, Effect::ShowColours]);
    assert_eq!(w.ctl.colour_view().current, BLUE);
    assert_eq!(w.ctl.colour_view().recents, [BLUE]);
    assert_eq!(written(&w, 0), "#3a7bd5", "copied in the default format");
    assert_eq!(w.ctl.services().keyboard.sent.len(), 0, "never pasted");

    let mut w = world_with(
        outlook().into(),
        Settings {
            colour_copy_on_pick: false,
            ..Settings::default()
        },
    );
    w.ctl.colour_picked(BLUE, false);
    assert!(w.ctl.services().clipboard.written.is_empty());
    assert_eq!(w.ctl.colour_view().recents, [BLUE], "still remembered");
}

// ---- The screen eyedropper ---------------------------------------------------------------

mod eyedropper {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use snipdeck_platform::eyedrop::{EyedropOutcome, FakeEyedropper};

    use super::*;

    /// A world whose eyedropper is `fake`; the second value counts the wake-ups.
    fn with(fake: &FakeEyedropper) -> (World, Arc<AtomicUsize>) {
        let mut w = world();
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&wakes);
        w.ctl.set_eyedropper(
            Some(Box::new(fake.clone())),
            Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
        );
        (w, wakes)
    }

    #[test]
    fn start_hides_the_panel_and_a_pick_comes_back_as_the_current_colour() {
        let fake = FakeEyedropper::answering(EyedropOutcome::Picked {
            rgb: [58, 123, 213],
            hdr: false,
        });
        let (mut w, wakes) = with(&fake);
        assert_eq!(w.ctl.start_eyedropper(), vec![Effect::HidePanel]);
        assert_eq!(fake.starts().len(), 1);
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        let effects = w.ctl.poll_eyedropper();
        assert_eq!(effects, vec![Effect::ShowPanel, Effect::ShowColours]);
        assert_eq!(w.ctl.colour_view().current, BLUE);
        assert_eq!(w.ctl.colour_view().recents.first(), Some(&BLUE));
        assert!(w.ctl.poll_eyedropper().is_empty(), "taken once");
    }

    #[test]
    fn a_pick_is_copied_in_the_default_format_when_the_setting_says_so() {
        let fake = FakeEyedropper::answering(EyedropOutcome::Picked {
            rgb: [58, 123, 213],
            hdr: false,
        });
        let (mut w, _) = with(&fake);
        w.ctl.start_eyedropper();
        w.ctl.poll_eyedropper();
        assert_eq!(written(&w, 0), "#3a7bd5");
    }

    #[test]
    fn a_cancel_brings_the_panel_back_and_changes_nothing() {
        let fake = FakeEyedropper::answering(EyedropOutcome::Cancelled);
        let (mut w, _) = with(&fake);
        let before = w.ctl.colour_view().current;
        w.ctl.start_eyedropper();
        assert_eq!(
            w.ctl.poll_eyedropper(),
            vec![Effect::ShowPanel, Effect::ShowColours]
        );
        assert_eq!(w.ctl.colour_view().current, before);
    }

    #[test]
    fn a_failure_says_so_and_brings_the_panel_back() {
        let fake = FakeEyedropper::answering(EyedropOutcome::Failed("BitBlt 5".into()));
        let (mut w, _) = with(&fake);
        w.ctl.start_eyedropper();
        assert_eq!(
            w.ctl.poll_eyedropper(),
            vec![Effect::ShowPanel, Effect::ShowColours]
        );
        assert!(matches!(
            w.ctl.view().toast.map(|t| t.message),
            Some(ToastMessage::EyedropFailed)
        ));
    }

    #[test]
    fn a_second_start_while_one_runs_does_nothing() {
        let fake = FakeEyedropper::default();
        let (mut w, _) = with(&fake);
        assert_eq!(w.ctl.start_eyedropper(), vec![Effect::HidePanel]);
        assert_eq!(w.ctl.eyedropper_state(), (true, true));
        assert!(w.ctl.start_eyedropper().is_empty());
        assert_eq!(fake.starts().len(), 1);
        assert!(fake.complete(EyedropOutcome::Cancelled));
        assert_eq!(w.ctl.eyedropper_state(), (true, false));
    }

    #[test]
    fn without_an_eyedropper_the_button_is_absent_and_start_does_nothing() {
        let mut w = world();
        assert_eq!(w.ctl.eyedropper_state(), (false, false));
        assert!(w.ctl.start_eyedropper().is_empty());
    }
}

mod commands {
    use snipdeck_platform::eyedrop::FakeEyedropper;

    use super::*;
    use crate::commands::Command;

    #[test]
    fn the_colours_command_shows_the_panel_on_the_tab() {
        let mut w = world();
        w.ctl.run_command(Command::Colours, 0);
        let effects = w.ctl.take_pending();
        assert!(effects.contains(&Effect::ShowPanel));
        assert!(effects.contains(&Effect::ShowColours));
        assert!(!effects.contains(&Effect::HidePanel));
    }

    #[test]
    fn the_pick_command_starts_the_eyedropper_after_showing_the_tab() {
        let mut w = world();
        let fake = FakeEyedropper::default();
        w.ctl
            .set_eyedropper(Some(Box::new(fake.clone())), std::sync::Arc::new(|| {}));
        w.ctl.run_command(Command::PickColour, 0);
        let effects = w.ctl.take_pending();
        assert_eq!(effects.last(), Some(&Effect::HidePanel));
        assert_eq!(fake.starts().len(), 1);
    }

    #[test]
    fn both_do_nothing_while_the_tab_is_off() {
        let mut w = world();
        w.ctl.update_settings(|s| s.show_colours_tab = false);
        w.ctl.run_command(Command::Colours, 0);
        w.ctl.run_command(Command::PickColour, 0);
        assert!(w.ctl.take_pending().is_empty());
    }
}

mod item_menu {
    use super::*;
    use crate::controller::{ChooseLabel, ItemMenuEntry};

    fn row_of(w: &World, id: ItemId) -> usize {
        w.ctl
            .model()
            .rows()
            .iter()
            .position(|r| r.id == id)
            .expect("the new item is in the Panel's list")
    }

    #[test]
    fn a_colour_item_offers_open_in_colours_and_formats_in_paste_as() {
        let mut w = world();
        let id = quick(&mut w, "#3a7bd5");
        let at = row_of(&w, id);
        w.ctl.open_item_menu(at, false);
        let entries = w.ctl.view().item_menu.expect("menu");
        assert!(entries.contains(&ItemMenuEntry::OpenInColours));
        let paste_as = entries
            .iter()
            .position(|e| *e == ItemMenuEntry::PasteAs)
            .expect("Paste as");
        assert!(w.ctl.choose_item_menu(paste_as).is_empty());
        let choose = w.ctl.view().choose_menu.expect("the second level");
        assert_eq!(choose.entries.len(), Fmt::ALL.len());
        assert_eq!(
            choose.entries[1].label,
            ChooseLabel::Verbatim("RGB  rgb(58, 123, 213)".into())
        );
        // Choosing RGB pastes it that way.
        w.ctl.choose_item_menu(1);
        assert!(
            w.ctl.services().keyboard.sent.len() + w.ctl.services().clipboard.written.len() > 0,
            "the use went to the keyboard or the clipboard"
        );
    }

    #[test]
    fn open_in_colours_makes_it_current_and_shows_the_tab() {
        let mut w = world();
        let id = quick(&mut w, "#3a7bd5");
        let at = row_of(&w, id);
        w.ctl.open_item_menu(at, false);
        let entries = w.ctl.view().item_menu.expect("menu");
        let open = entries
            .iter()
            .position(|e| *e == ItemMenuEntry::OpenInColours)
            .expect("entry");
        let effects = w.ctl.choose_item_menu(open);
        assert_eq!(effects, vec![Effect::ShowPanel, Effect::ShowColours]);
        assert_eq!(w.ctl.colour_view().current, BLUE);
    }

    #[test]
    fn plain_text_has_no_open_in_colours_and_keeps_its_transforms() {
        let mut w = world();
        let id = quick(&mut w, "hello there");
        let at = row_of(&w, id);
        w.ctl.open_item_menu(at, false);
        let entries = w.ctl.view().item_menu.expect("menu");
        assert!(!entries.contains(&ItemMenuEntry::OpenInColours));
    }
}

mod list_menu {
    use super::*;
    use crate::controller::{LIST_MENU, ListMenuEntry};

    #[test]
    fn copy_as_css_is_on_only_for_a_list_holding_a_colour_and_copies_the_block() {
        let mut w = world();
        let css = LIST_MENU
            .iter()
            .position(|e| *e == ListMenuEntry::CopyCss)
            .expect("a CSS row");
        // The demo's first list holds no colour.
        w.ctl.open_list_menu(0);
        assert!(!w.ctl.view().list_menu_css);
        w.ctl.close_list_menu();
        // Save a colour into the current list, then ask again.
        let list = w.ctl.model().tabs()[0].id;
        assert!(w.ctl.save_colour(BLUE, SaveTarget::List(list)));
        w.ctl.open_list_menu(0);
        assert!(w.ctl.view().list_menu_css);
        let before = w.ctl.services().clipboard.written.len();
        assert!(w.ctl.choose_list_menu(css).is_empty());
        assert_eq!(w.ctl.services().clipboard.written.len(), before + 1);
        let text = written(&w, before);
        assert!(text.starts_with(":root {"), "{text}");
        assert!(text.contains("#3a7bd5"), "{text}");
    }
}
