//! Copy a saved colour in any format from where it is stored: the Main window's Colour block,
//! its "Paste as…" and "Open in Colours", and the Picker's "Paste as" and "Open in Colours"
//! (colour-spec D6a). A fixed clock throughout (`world()`).

use snipdeck_colour::{Format as Fmt, Rgba8};

use super::*;
use crate::main_window::{Section, SelectMode};

const BLUE: Rgba8 = Rgba8::rgb(58, 123, 213);

/// Adds a snippet through the quick-add box (the Main window's open list is not needed: the
/// Panel's list takes it) and returns its id.
fn quick(w: &mut World, text: &str, sensitive: bool) -> ItemId {
    assert!(w.ctl.quick_add(
        text,
        Some(ItemKind::Text),
        sensitive,
        crate::item_editor::Origin::Panel
    ));
    w.ctl
        .store()
        .all_items()
        .expect("items")
        .into_iter()
        .max_by_key(|i| i.created_at)
        .map(|i| i.id)
        .expect("the new item")
}

/// A world with the colour `#3a7bd5` saved as text, the Main window on "All items" and the
/// cursor on that row. Returns the row too.
fn on_colour_row() -> (World, ItemId, usize) {
    let mut w = world();
    let id = quick(&mut w, "#3a7bd5", false);
    let row = open_row(&mut w, id);
    (w, id, row)
}

fn open_row(w: &mut World, id: ItemId) -> usize {
    w.ctl.main_open(Section::All);
    let row = w
        .ctl
        .main_screen()
        .rows
        .iter()
        .position(|r| r.key == crate::main_window::Key::Item(id))
        .expect("the item has a row");
    w.ctl.main_select(row, SelectMode::Replace);
    row
}

fn written(w: &World, n: usize) -> String {
    match &w.ctl.services().clipboard.written[n].data()[0] {
        Data::Text(t) => t.clone(),
        other => panic!("not text: {other:?}"),
    }
}

mod main_window {
    use super::*;

    #[test]
    fn the_block_lists_the_colour_in_all_seven_formats() {
        let (mut w, _, _) = on_colour_row();
        let block = w.ctl.main_screen().colour_block.expect("a Colour block");
        assert_eq!(block.colour, BLUE);
        assert!(w.ctl.main_screen().colours_tab);
        let formats: Vec<Fmt> = block.formats.iter().map(|(f, _)| *f).collect();
        assert_eq!(formats, Fmt::ALL);
        assert_eq!(formats.len(), 7);
        assert_eq!(block.formats[0].1, "#3a7bd5");
        assert_eq!(block.formats[1].1, "rgb(58, 123, 213)");
        assert_eq!(block.formats[6], (Fmt::Name, "steelblue".to_owned()));
    }

    #[test]
    fn plain_text_a_secret_a_second_row_and_the_trash_show_no_block() {
        let (mut w, id, row) = on_colour_row();
        // Plain text.
        let plain = w.ids[0];
        let plain_row = open_row(&mut w, plain);
        assert_ne!(plain_row, row);
        assert_eq!(w.ctl.main_screen().colour_block, None);
        // A Sensitive item that holds a colour literal.
        let secret = quick(&mut w, "#ff0000", true);
        open_row(&mut w, secret);
        assert_eq!(w.ctl.main_screen().colour_block, None);
        // Two rows selected.
        let row = open_row(&mut w, id);
        w.ctl.main_select(plain_row, SelectMode::Toggle);
        assert!(w.ctl.main_screen().selected_count > 1);
        assert_eq!(w.ctl.main_screen().colour_block, None);
        w.ctl.main_select(row, SelectMode::Replace);
        assert!(w.ctl.main_screen().colour_block.is_some());
        // Trashed: the Trash lists it, the block does not.
        w.ctl.main_action(crate::controller::MainAction::Delete, row);
        w.ctl.main_open(Section::Trash);
        w.ctl.main_select(0, SelectMode::Replace);
        assert_eq!(w.ctl.main_screen().colour_block, None);
    }

    #[test]
    fn a_block_button_copies_that_format_with_the_copied_toast_and_no_keys() {
        let (mut w, id, row) = on_colour_row();
        let keys = w.ctl.services().keyboard.sent.len();
        let result = w.ctl.main_colour_copy(row, 1);
        assert_eq!(result, Some(UseResult::Done(UseOutcome::Copied)));
        assert_eq!(written(&w, 0), "rgb(58, 123, 213)");
        assert_eq!(
            w.ctl.services().keyboard.sent.len(),
            keys,
            "a copy sends no paste keys, even with a Target"
        );
        let toast = w.ctl.main_screen().toast.expect("a toast");
        assert_eq!(toast.message, ToastMessage::Copied);
        assert_eq!(uses(&w, id), 1, "it counts as a use of the item");
        assert_eq!(w.ctl.colour_view().recents, [BLUE]);
        // The last format is the keyword.
        w.ctl.main_colour_copy(row, 6);
        assert_eq!(written(&w, 1), "steelblue");
    }

    #[test]
    fn a_format_past_the_end_or_a_row_that_is_no_colour_copies_nothing() {
        let (mut w, _, row) = on_colour_row();
        assert_eq!(w.ctl.main_colour_copy(row, 7), None);
        let plain = w.ids[0];
        let plain_row = open_row(&mut w, plain);
        assert_eq!(w.ctl.main_colour_copy(plain_row, 0), None);
        assert!(w.ctl.services().clipboard.written.is_empty());
    }

    #[test]
    fn paste_as_on_a_colour_row_offers_the_formats_and_pastes_or_copies_the_choice() {
        let (mut w, id, row) = on_colour_row();
        let choices = w.ctl.main_colour_choices(row);
        assert_eq!(choices.len(), 7);
        assert_eq!(choices[2], (Fmt::Hsl, "hsl(215, 64%, 53%)".to_owned()));
        assert!(
            w.ctl
                .main_colour_choices(open_row(&mut w, w.ids[0]))
                .is_empty()
        );
        let row = open_row(&mut w, id);
        // Enter pastes (Ctrl+V goes out).
        w.ctl.main_action(crate::controller::MainAction::PasteAs, row);
        let result = w.ctl.main_colour_paste_as(2, false);
        assert_eq!(result, Some(UseResult::Done(UseOutcome::Pasted)));
        assert_eq!(written(&w, 0), "hsl(215, 64%, 53%)");
        assert_eq!(w.ctl.services().keyboard.sent.len(), 1);
        // Shift+Enter copies: no new keys.
        w.ctl.main_action(crate::controller::MainAction::PasteAs, row);
        let result = w.ctl.main_colour_paste_as(0, true);
        assert_eq!(result, Some(UseResult::Done(UseOutcome::Copied)));
        assert_eq!(written(&w, 1), "#3a7bd5");
        assert_eq!(w.ctl.services().keyboard.sent.len(), 1);
        assert_eq!(uses(&w, id), 2);
        // The chooser is spent.
        assert_eq!(w.ctl.main_colour_paste_as(0, false), None);
    }

    #[test]
    fn open_in_colours_makes_it_current_and_asks_the_panel_to_show_the_tab() {
        let (mut w, _, row) = on_colour_row();
        w.ctl.main_open_in_colours(row);
        assert_eq!(w.ctl.colour_view().current, BLUE);
        assert_eq!(
            w.ctl.take_pending(),
            vec![Effect::ShowPanel, Effect::ShowColours]
        );
        assert!(
            w.ctl.colour_view().recents.is_empty(),
            "opening a colour is not a use"
        );
    }

    #[test]
    fn with_the_tab_off_there_is_nothing_to_open() {
        let mut w = world_with(
            Some(outlook()),
            Settings {
                show_colours_tab: false,
                ..Settings::new_install()
            },
        );
        let id = quick(&mut w, "#3a7bd5", false);
        let row = open_row(&mut w, id);
        let screen = w.ctl.main_screen();
        assert!(screen.colour_block.is_some(), "still copyable");
        assert!(!screen.colours_tab);
        w.ctl.main_open_in_colours(row);
        assert!(w.ctl.take_pending().is_empty());
        assert_eq!(
            w.ctl.main_colour_copy(row, 0),
            Some(UseResult::Done(UseOutcome::Copied))
        );
    }
}

mod picker {
    use super::*;

    fn open_on_colour() -> (World, ItemId) {
        let mut w = world();
        let id = quick(&mut w, "#3a7bd5", false);
        w.ctl.open_picker();
        w.ctl.picker_query("3a7bd5");
        (w, id)
    }

    #[test]
    fn a_colour_result_offers_the_seven_formats_and_a_plain_one_none() {
        let (mut w, _) = open_on_colour();
        let choices = w.ctl.picker_colour_choices(0);
        assert_eq!(choices.len(), 7);
        assert_eq!(choices[1], (Fmt::Rgb, "rgb(58, 123, 213)".to_owned()));
        w.ctl.picker_query("second");
        assert!(w.ctl.picker_colour_choices(0).is_empty());
        assert!(w.ctl.picker_colour_choices(9).is_empty());
    }

    #[test]
    fn enter_on_a_format_pastes_it_and_shift_enter_copies_it() {
        let (mut w, id) = open_on_colour();
        let item = w.ctl.picker_take(0).expect("the row");
        assert_eq!(item, id);
        assert!(w.ctl.picker().is_none(), "the Picker closed");
        let result = w.ctl.use_colour_item_as(item, 3, UseMode::Paste);
        assert_eq!(result, UseResult::Done(UseOutcome::Pasted));
        assert_eq!(written(&w, 0), "hsb(215, 73%, 84%)");
        assert_eq!(w.ctl.services().keyboard.sent.len(), 1);
        assert_eq!(uses(&w, id), 1);
        let result = w.ctl.use_colour_item_as(item, 0, UseMode::Copy);
        assert_eq!(result, UseResult::Done(UseOutcome::Copied));
        assert_eq!(w.ctl.services().keyboard.sent.len(), 1);
        assert_eq!(written(&w, 1), "#3a7bd5");
    }

    #[test]
    fn a_format_that_is_not_there_or_an_item_that_is_no_colour_fails() {
        let (mut w, id) = open_on_colour();
        assert_eq!(
            w.ctl.use_colour_item_as(id, 7, UseMode::Copy),
            UseResult::Failed
        );
        let plain = w.ids[0];
        assert_eq!(
            w.ctl.use_colour_item_as(plain, 0, UseMode::Copy),
            UseResult::Failed
        );
        assert!(w.ctl.services().clipboard.written.is_empty());
    }

    #[test]
    fn open_in_colours_closes_the_picker_and_shows_the_tab() {
        let (mut w, _) = open_on_colour();
        assert!(w.ctl.picker_open_in_colours(0));
        assert!(w.ctl.picker().is_none());
        assert_eq!(w.ctl.colour_view().current, BLUE);
        assert_eq!(
            w.ctl.take_pending(),
            vec![Effect::ShowPanel, Effect::ShowColours]
        );
        assert!(
            w.ctl.services().clipboard.written.is_empty(),
            "nothing was used"
        );
    }

    #[test]
    fn open_in_colours_on_plain_text_or_with_the_tab_off_keeps_the_picker() {
        let (mut w, _) = open_on_colour();
        w.ctl.picker_query("second");
        assert!(!w.ctl.picker_open_in_colours(0));
        assert!(w.ctl.picker().is_some());
        assert!(w.ctl.take_pending().is_empty());

        let mut w = world_with(
            Some(outlook()),
            Settings {
                show_colours_tab: false,
                ..Settings::new_install()
            },
        );
        quick(&mut w, "#3a7bd5", false);
        w.ctl.open_picker();
        w.ctl.picker_query("3a7bd5");
        assert!(!w.ctl.picker_open_in_colours(0));
        assert!(w.ctl.picker().is_some());
    }
}
