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
    assert!(w.ctl.quick_add(text, Some(ItemKind::Text), false, crate::item_editor::Origin::Panel));
    w.ctl
        .store()
        .all_items()
        .expect("items")
        .into_iter()
        .find(|i| crate::usecase::item_text(w.ctl.store(), i.id).ok().flatten().as_deref() == Some(text))
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
    assert_eq!(w.ctl.colour_view().format, Fmt::Hsl, "Name cannot be the default");
    w.ctl.set_colour_scheme(snipdeck_colour::Scheme::Triadic);
    assert_eq!(w.ctl.settings().colour_scheme, snipdeck_colour::Scheme::Triadic);
}

#[test]
fn ctrl_s_makes_the_colours_list_once_at_the_end_and_titles_an_exact_name() {
    let mut w = world();
    let before = w.ctl.store().lists(w.ctl.store().boards().unwrap()[0].id).unwrap().len();
    assert!(w.ctl.save_colour(BLUE, SaveTarget::Home));
    let list = colours_list(&w).expect("remembered by id");
    let lists = w.ctl.store().lists(w.ctl.store().boards().unwrap()[0].id).unwrap();
    assert_eq!(lists.len(), before + 1);
    assert_eq!(lists.last().unwrap().id, list, "at the end of the board");
    assert_eq!(lists.last().unwrap().name.as_str(), "Colours");
    assert_eq!(items_of(&w, list), [(String::new(), "#3a7bd5".to_owned())]);

    assert!(w.ctl.save_colour(Rgba8::rgb(0x41, 0x69, 0xe1), SaveTarget::Home));
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
    assert_eq!(items_of(&w, list)[0].1, "#3a7bd5", "hsl(215, 65%, 53%) is not #3a7bd5");
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
    assert_eq!(saved.iter().map(|(_, c)| *c).collect::<Vec<_>>(), [BLUE, red]);
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
    assert_eq!(w.ctl.colour_of_item(bare), None, "a bare keyword is a word, not a swatch");
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
    w.ctl.save_colour(Rgba8::rgb(0x41, 0x69, 0xe1), SaveTarget::Home);
    w.ctl.save_colour(Rgba8::rgb(0x41, 0x69, 0xe1), SaveTarget::Home);
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
