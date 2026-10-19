//! The Main window's controller half, without a window: sections and counts, search,
//! selection, the item actions, the Trash with its Undo, list and board editing, the Free
//! 3-board cap and reordering, over the demo library, the platform fakes and a frozen clock.
//! The window itself is covered by `main_window.rs`.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use std::time::Duration;

use snipdeck_app::{
    controller::{
        Controller, Effect, LIST_MENU, MainAction, MainScreen, Services, TOAST_ENTER, TOAST_EXIT,
        TOAST_HOLD, TOAST_UNDO_HOLD, TagRenameError,
    },
    demo,
    host::{FakeTargets, FixedClock},
    main_window::{DeleteHint, EditorKind, NameError, Section, SelectMode},
    settings::Settings,
    toast::ToastMessage,
};
use snipdeck_core::{Entitlements, ListAccent, ListSort, Name, Tag, Tier, Timestamp};
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_store::{BoardRepo, ItemRepo, Store, TagRepo, TrashRepo, Trashable};

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

fn rig(tier: Tier) -> (Ctl, FixedClock) {
    rig_on(demo::store(now()).expect("demo store"), tier)
}

fn rig_on(store: Store, tier: Tier) -> (Ctl, FixedClock) {
    let clock = FixedClock::new(now());
    let ctl = Controller::new(
        store,
        Services {
            clipboard: FakeClipboard {
                asks_after_polls: Some(1),
                ..FakeClipboard::default()
            },
            keyboard: FakeKeyboard::default(),
            timing: FakeTiming::default(),
        },
        Box::new(FakeTargets::new(Some(demo::target()))),
        Box::new(clock.clone()),
        Settings::default(),
        Entitlements::new(tier),
    )
    .expect("controller");
    (ctl, clock)
}

fn pro() -> Ctl {
    rig(Tier::Pro).0
}

fn titles(screen: &MainScreen) -> Vec<String> {
    screen.rows.iter().map(|r| r.title.clone()).collect()
}

fn row_of(screen: &MainScreen, title: &str) -> usize {
    screen
        .rows
        .iter()
        .position(|r| r.title == title)
        .unwrap_or_else(|| panic!("no row {title}"))
}

fn list_index(screen: &MainScreen, name: &str) -> usize {
    screen
        .lists
        .iter()
        .position(|l| l.name == name)
        .unwrap_or_else(|| panic!("no list {name}"))
}

fn open_list(ctl: &mut Ctl, name: &str) -> MainScreen {
    let screen = ctl.main_screen();
    let id = screen.lists[list_index(&screen, name)].id;
    ctl.main_open(Section::List(id));
    ctl.main_screen()
}

fn message(screen: &MainScreen) -> ToastMessage {
    screen.toast.as_ref().expect("a toast").message.clone()
}

// ---- Sections, counts, search ---------------------------------------------------------------

#[test]
fn the_demo_library_shows_its_sections_counts_and_lists() {
    let mut ctl = pro();
    let screen = ctl.main_screen();
    assert_eq!(screen.board_name, "Work");
    assert_eq!(screen.section, Section::All);
    assert_eq!(
        (
            screen.counts.all,
            screen.counts.pinned,
            screen.counts.recent,
            screen.counts.trash
        ),
        (11, 3, 0, 0)
    );
    assert_eq!(
        screen
            .lists
            .iter()
            .map(|l| (l.name.as_str(), l.count))
            .collect::<Vec<_>>(),
        [("Email replies", 6), ("Addresses", 2), ("Code", 2)]
    );
    assert_eq!(screen.rows.len(), 11, "All items spans both boards");
    assert!(screen.rows.iter().all(|r| r.list.is_some()));
    assert!(!screen.dark);
}

#[test]
fn opening_a_list_a_system_view_or_a_board_changes_the_rows() {
    let mut ctl = pro();
    let screen = open_list(&mut ctl, "Addresses");
    assert_eq!(titles(&screen), ["Home address", "Office"]);
    assert_eq!(screen.section_index, 1);

    ctl.main_open(Section::Pinned);
    assert_eq!(
        titles(&ctl.main_screen()),
        ["Thanks, received", "Home address", "Retry helper"]
    );

    ctl.main_open(Section::Recent);
    assert!(
        ctl.main_screen().rows.is_empty(),
        "the inbox is not built yet"
    );

    ctl.main_board_menu_open();
    let menu = ctl.main_screen().board_menu.expect("menu open");
    assert_eq!(menu.len(), 2 + 1 + 5, "two boards, a divider, five actions");
    ctl.main_board_menu_activate(1);
    let screen = ctl.main_screen();
    assert!(screen.board_menu.is_none());
    assert_eq!(screen.board_name, "Personal");
    assert_eq!(
        screen
            .lists
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>(),
        ["Errands"]
    );
}

#[test]
fn search_covers_every_board_and_ends_when_a_section_opens() {
    let mut ctl = pro();
    open_list(&mut ctl, "Code");
    ctl.main_set_query("milk");
    let screen = ctl.main_screen();
    assert_eq!(
        titles(&screen),
        ["Shopping list"],
        "a hit on the other board"
    );
    assert_eq!(screen.rows[0].list.as_ref().expect("label").name, "Errands");
    ctl.main_open(Section::Pinned);
    let screen = ctl.main_screen();
    assert_eq!(screen.query, "");
    assert_eq!(screen.rows.len(), 3);
}

#[test]
fn selection_modes_build_and_a_hostile_query_is_only_a_search() {
    let mut ctl = pro();
    ctl.main_select(1, SelectMode::Replace);
    ctl.main_select(3, SelectMode::Extend);
    let screen = ctl.main_screen();
    assert_eq!(screen.selected_count, 3);
    assert_eq!(screen.cursor, 3);
    assert_eq!(screen.rows.iter().filter(|r| r.selected).count(), 3);
    ctl.main_select(0, SelectMode::Toggle);
    assert_eq!(ctl.main_screen().selected_count, 4);
    ctl.main_clear_selection();
    assert_eq!(ctl.main_screen().selected_count, 0);
    ctl.main_select_all();
    assert_eq!(ctl.main_screen().selected_count, 11);
    ctl.main_set_query("\"(*) AND NOT");
    assert!(ctl.main_screen().toast.is_none(), "no error toast");
}

// ---- Item actions ---------------------------------------------------------------------------

#[test]
fn use_pastes_and_counts_through_the_controller_and_copy_only_copies() {
    let mut ctl = pro();
    let screen = ctl.main_screen();
    let at = row_of(&screen, "Out of office");
    let id = match screen.rows[at].key {
        snipdeck_app::main_window::Key::Item(id) => id,
        other => panic!("{other:?}"),
    };
    ctl.main_action(MainAction::Use, at).expect("a use");
    assert_eq!(ctl.store().item(id).unwrap().use_count, 1);
    ctl.main_action(MainAction::Copy, at).expect("a use");
    assert_eq!(ctl.store().item(id).unwrap().use_count, 2);
}

#[test]
fn pin_toggles_one_row_or_the_whole_selection_and_the_panel_follows() {
    let mut ctl = pro();
    let screen = ctl.main_screen();
    let at = row_of(&screen, "Out of office");
    ctl.main_action(MainAction::Pin, at);
    let screen = ctl.main_screen();
    assert_eq!(screen.counts.pinned, 4);
    assert!(screen.rows[row_of(&screen, "Out of office")].pinned);
    assert_eq!(ctl.view().pinned.len(), 4, "the Panel's strip shows it too");

    // Two selected rows, one pinned and one not: pin both; again: unpin both.
    let a = row_of(&screen, "Out of office");
    let b = row_of(&screen, "Meeting follow-up");
    ctl.main_select(a.min(b), SelectMode::Replace);
    ctl.main_select(a.max(b), SelectMode::Extend);
    ctl.main_action(MainAction::Pin, a);
    assert_eq!(ctl.main_screen().counts.pinned, 5);
    ctl.main_action(MainAction::Pin, a);
    assert_eq!(ctl.main_screen().counts.pinned, 3);
}

#[test]
fn duplicate_copies_each_selected_item_next_to_it_unpinned_and_unused() {
    let mut ctl = pro();
    let screen = ctl.main_screen();
    let at = row_of(&screen, "Out of office");
    let id = match screen.rows[at].key {
        snipdeck_app::main_window::Key::Item(id) => id,
        other => panic!("{other:?}"),
    };
    ctl.main_action(MainAction::Pin, at);
    ctl.main_action(MainAction::Use, at).expect("a use");
    let original = ctl.store().item(id).unwrap();
    let siblings = ctl.store().items(original.list_id).unwrap();
    let place = siblings.iter().position(|i| i.id == id).unwrap();

    assert!(ctl.main_action(MainAction::Duplicate, at).is_none());

    let after = ctl.store().items(original.list_id).unwrap();
    assert_eq!(after.len(), siblings.len() + 1);
    let copy = &after[place + 1];
    assert_ne!(copy.id, id);
    assert_eq!(after[place].id, id, "the original stays put");
    assert_eq!(copy.title, original.title);
    assert_eq!(copy.preview, original.preview);
    assert_eq!(copy.tags, original.tags);
    assert_eq!(
        ctl.store().formats(copy.id).unwrap(),
        ctl.store().formats(id).unwrap()
    );
    assert!(original.pinned && original.use_count == 1);
    assert!(!copy.pinned && copy.use_count == 0 && copy.last_used_at.is_none());
    assert_eq!(copy.created_at, now());
}

#[test]
fn export_list_writes_its_text_items_to_a_dated_file_and_skips_secrets() {
    let mut ctl = pro();
    let screen = ctl.main_screen();
    let code = list_index(&screen, "Code");
    let dir = tempfile::tempdir().expect("dir");

    let file = ctl.main_export_list(code, dir.path()).expect("a file");

    assert_eq!(file.file_name().unwrap(), "Code-20261017.txt");
    let text = std::fs::read_to_string(&file).expect("read");
    assert!(text.starts_with("Code\n\n"), "{text}");
    let list = ctl.main_screen().lists[code].id;
    for item in ctl.store().items(list).unwrap() {
        let body = snipdeck_app::usecase::item_text(ctl.store(), item.id);
        match (item.sensitive, body) {
            (false, Ok(Some(body))) => assert!(text.contains(body.trim_end()), "{body}"),
            (true, Ok(Some(body))) => assert!(!text.contains(body.trim_end()), "secret in file"),
            _ => {}
        }
    }
    assert!(matches!(
        message(&ctl.main_screen()),
        ToastMessage::Exported { .. }
    ));
    // A second export the same day does not overwrite the first.
    let again = ctl.main_export_list(code, dir.path()).expect("a file");
    assert_eq!(again.file_name().unwrap(), "Code-20261017-2.txt");
}

#[test]
fn export_list_that_cannot_be_written_says_so() {
    let mut ctl = pro();
    let code = list_index(&ctl.main_screen(), "Code");
    let dir = tempfile::tempdir().expect("dir");
    let blocker = dir.path().join("file");
    std::fs::write(&blocker, "x").expect("write");
    assert!(
        ctl.main_export_list(code, &blocker.join("inside"))
            .is_none()
    );
    assert_eq!(message(&ctl.main_screen()), ToastMessage::ExportFailed);
}

#[test]
fn move_sends_the_chosen_rows_to_the_end_of_a_list_on_the_open_board() {
    let mut ctl = pro();
    let screen = ctl.main_screen();
    let a = row_of(&screen, "Out of office");
    let b = row_of(&screen, "Meeting follow-up");
    ctl.main_select(a.min(b), SelectMode::Replace);
    ctl.main_select(a.max(b), SelectMode::Extend);
    assert!(ctl.main_action(MainAction::Move, a).is_none());
    let target = list_index(&screen, "Code");
    ctl.main_move_to(target);
    let screen = ctl.main_screen();
    assert_eq!(
        message(&screen),
        ToastMessage::ItemsMoved {
            count: 2,
            list: "Code".into()
        }
    );
    assert!(!screen.toast.as_ref().unwrap().undo, "a move has no undo");
    let screen = open_list(&mut ctl, "Code");
    assert_eq!(
        titles(&screen),
        [
            "Retry helper",
            "Recent customers",
            "Out of office",
            "Meeting follow-up"
        ]
    );
    assert_eq!(screen.lists[list_index(&screen, "Code")].count, 4);
    assert_eq!(screen.lists[list_index(&screen, "Email replies")].count, 4);
}

#[test]
fn tags_are_ticked_on_the_chosen_rows_and_unticked_again() {
    let tag = Tag::new(Name::new("invoices").unwrap());
    let mut store = demo::store(now()).unwrap();
    store.insert_tag(&tag).unwrap();
    let (mut ctl, _) = rig_on(store, Tier::Pro);
    let screen = ctl.main_screen();
    let at = row_of(&screen, "Invoice reminder");
    ctl.main_action(MainAction::Tag, at);
    assert!(!ctl.main_screen().tag_choices[0].checked);
    ctl.main_toggle_tag(0, true);
    let screen = ctl.main_screen();
    assert!(screen.tag_choices[0].checked);
    assert_eq!(screen.tags[0].count, 1);
    ctl.main_open(Section::Tag(screen.tags[0].id));
    assert_eq!(titles(&ctl.main_screen()), ["Invoice reminder"]);
    ctl.main_action(MainAction::Tag, 0);
    ctl.main_toggle_tag(0, false);
    assert_eq!(ctl.main_screen().rows.len(), 0);
}

/// A demo library with the tags `invoices` and `billing`.
fn rig_with_tags() -> Ctl {
    let mut store = demo::store(now()).unwrap();
    store
        .insert_tag(&Tag::new(Name::new("invoices").unwrap()))
        .unwrap();
    store
        .insert_tag(&Tag::new(Name::new("billing").unwrap()))
        .unwrap();
    rig_on(store, Tier::Pro).0
}

fn tag_names(ctl: &mut Ctl) -> Vec<String> {
    ctl.main_screen()
        .tags
        .iter()
        .map(|t| t.name.clone())
        .collect()
}

#[test]
fn renaming_a_tag_changes_it_everywhere() {
    let mut ctl = rig_with_tags();
    let screen = ctl.main_screen();
    let at = row_of(&screen, "Invoice reminder");
    ctl.main_action(MainAction::Tag, at);
    ctl.main_toggle_tag(0, true);
    let before = tag_names(&mut ctl);
    assert_eq!(before.len(), 2);

    assert_eq!(ctl.main_rename_tag(0, "  Invoices 2026 "), Ok(()));
    let after = tag_names(&mut ctl);
    assert!(after.contains(&"Invoices 2026".to_owned()), "{after:?}");
    assert!(
        !after.contains(&before[0]),
        "the old name is gone: {after:?}"
    );
    // The items that carried it still do (the tag keeps its identity).
    let screen = ctl.main_screen();
    let renamed = screen
        .tags
        .iter()
        .find(|t| t.name == "Invoices 2026")
        .unwrap();
    assert_eq!(renamed.count, 1);
    assert!(screen.toast.is_none());
}

#[test]
fn renaming_to_an_existing_name_is_refused_without_a_toast() {
    let mut ctl = rig_with_tags();
    let names = tag_names(&mut ctl);
    // Compared ignoring case, like the store does.
    let other = names[1].to_uppercase();
    assert_eq!(ctl.main_rename_tag(0, &other), Err(TagRenameError::Exists));
    assert_eq!(tag_names(&mut ctl), names, "nothing changed");
    assert!(
        ctl.main_screen().toast.is_none(),
        "the editor says it, not a toast"
    );
    // Keeping its own name (only the case differs) is not a clash.
    assert_eq!(ctl.main_rename_tag(0, &names[0].to_uppercase()), Ok(()));
}

#[test]
fn renaming_to_blank_says_give_it_a_name() {
    let mut ctl = rig_with_tags();
    let names = tag_names(&mut ctl);
    assert_eq!(
        ctl.main_rename_tag(0, "   "),
        Err(TagRenameError::Name(NameError::Empty))
    );
    assert_eq!(tag_names(&mut ctl), names);
}

#[test]
fn renaming_with_a_stale_index_does_nothing() {
    let mut ctl = rig_with_tags();
    let names = tag_names(&mut ctl);
    assert_eq!(ctl.main_rename_tag(9, "ghost"), Ok(()), "nothing to rename");
    assert_eq!(tag_names(&mut ctl), names);
    assert!(ctl.main_screen().toast.is_none());
}

#[test]
fn a_sensitive_row_shows_its_text_only_while_held_revealed() {
    let mut ctl = pro();
    let screen = ctl.main_screen();
    let at = row_of(&screen, "Server password");
    assert!(screen.rows[at].sensitive);
    assert_eq!(screen.rows[at].preview, "");
    assert_eq!(screen.revealed, None);
    ctl.main_reveal(at, true);
    let screen = ctl.main_screen();
    assert_eq!(screen.revealed, Some(at));
    assert_eq!(screen.rows[at].preview, "hunter2-correct-horse");
    ctl.main_reveal(at, false);
    let screen = ctl.main_screen();
    assert_eq!(screen.revealed, None);
    assert_eq!(screen.rows[at].preview, "");
    ctl.main_reveal(row_of(&screen, "Office"), true);
    assert_eq!(
        ctl.main_screen().revealed,
        None,
        "only sensitive rows reveal"
    );
}

// ---- Trash and Undo -------------------------------------------------------------------------

#[test]
fn delete_trashes_at_once_and_the_undo_toast_brings_it_back() {
    let mut ctl = pro();
    let screen = ctl.main_screen();
    let at = row_of(&screen, "Out of office");
    ctl.main_action(MainAction::Delete, at);
    let screen = ctl.main_screen();
    assert!(!titles(&screen).contains(&"Out of office".to_owned()));
    assert_eq!((screen.counts.all, screen.counts.trash), (10, 1));
    assert_eq!(message(&screen), ToastMessage::ItemsTrashed { count: 1 });
    assert!(screen.toast.as_ref().unwrap().undo);
    assert_eq!(ctl.view().rows.len(), 5, "the Panel list follows");

    ctl.main_undo();
    let screen = ctl.main_screen();
    assert!(titles(&screen).contains(&"Out of office".to_owned()));
    assert_eq!((screen.counts.all, screen.counts.trash), (11, 0));
    assert_eq!(message(&screen), ToastMessage::Restored { count: 1 });
    assert!(!screen.toast.unwrap().undo, "the undo is spent");
    let again = ctl.main_screen();
    ctl.main_undo();
    assert_eq!(
        ctl.main_screen().counts,
        again.counts,
        "a second undo does nothing"
    );
}

#[test]
fn a_delete_of_several_rows_is_one_toast_and_one_undo() {
    let mut ctl = pro();
    ctl.main_select(0, SelectMode::Replace);
    ctl.main_select(2, SelectMode::Extend);
    ctl.main_action(MainAction::Delete, 0);
    let screen = ctl.main_screen();
    assert_eq!(message(&screen), ToastMessage::ItemsTrashed { count: 3 });
    assert_eq!(screen.counts.trash, 3);
    assert_eq!(screen.selected_count, 0, "deleted rows leave the selection");
    ctl.main_undo();
    assert_eq!(ctl.main_screen().counts.trash, 0);
    assert_eq!(ctl.main_screen().counts.all, 11);
}

#[test]
fn the_undo_lasts_five_seconds_and_a_newer_toast_takes_it_away() {
    let mut ctl = pro();
    ctl.main_action(MainAction::Delete, 0);
    let seq = ctl.main_screen().toast.unwrap().seq;
    // Fade-in, then the hold that makes the toast 5 s long, then the fade-out.
    assert_eq!(TOAST_ENTER + TOAST_UNDO_HOLD, Duration::from_secs(5));
    assert!(TOAST_UNDO_HOLD > TOAST_HOLD);
    assert_eq!(ctl.toast_tick(seq), Some(TOAST_UNDO_HOLD));
    assert_eq!(ctl.toast_tick(seq), Some(TOAST_EXIT));
    assert_eq!(ctl.toast_tick(seq), None);
    assert!(ctl.main_screen().toast.is_none());
    ctl.main_undo();
    assert_eq!(
        ctl.main_screen().counts.trash,
        1,
        "too late: it stays in the Trash"
    );

    // A second delete, then a copy: the copy's toast replaces the Undo.
    let mut ctl = pro();
    ctl.main_action(MainAction::Delete, 0);
    let at = row_of(&ctl.main_screen(), "Out of office");
    ctl.main_action(MainAction::Copy, at);
    assert!(!ctl.main_screen().toast.unwrap().undo);
    ctl.main_undo();
    assert_eq!(ctl.main_screen().counts.trash, 1);
}

#[test]
fn the_trash_view_restores_deletes_for_good_and_empties() {
    let mut ctl = pro();
    ctl.main_select(0, SelectMode::Replace);
    ctl.main_select(3, SelectMode::Extend);
    ctl.main_action(MainAction::Delete, 0);
    ctl.main_open(Section::Trash);
    let screen = ctl.main_screen();
    assert_eq!(screen.rows.len(), 4);

    // Restore one.
    ctl.main_action(MainAction::Restore, 0);
    let screen = ctl.main_screen();
    assert_eq!((screen.counts.all, screen.counts.trash), (8, 3));
    assert_eq!(message(&screen), ToastMessage::Restored { count: 1 });

    // Delete one for good.
    ctl.main_action(MainAction::DeleteForever, 0);
    let screen = ctl.main_screen();
    assert_eq!(screen.counts.trash, 2);
    assert_eq!(message(&screen), ToastMessage::DeletedForever { count: 1 });
    assert_eq!(ctl.store().trashed().unwrap().len(), 2);

    // Empty.
    ctl.main_empty_trash();
    let screen = ctl.main_screen();
    assert_eq!(screen.counts.trash, 0);
    assert_eq!(screen.rows.len(), 0);
    assert_eq!(message(&screen), ToastMessage::TrashEmptied);
    assert_eq!(ctl.store().trashed().unwrap().len(), 0);
}

#[test]
fn the_trash_keeps_things_thirty_days_by_the_injected_clock() {
    let (mut ctl, clock) = rig(Tier::Pro);
    ctl.main_action(MainAction::Delete, 0);
    clock.set("2026-11-15T10:30:00Z".parse().unwrap()); // 29 days later
    assert_eq!(ctl.purge_expired_trash(), 0);
    assert_eq!(ctl.main_screen().counts.trash, 1);
    clock.set("2026-11-17T10:31:00Z".parse().unwrap()); // 31 days later
    assert_eq!(ctl.purge_expired_trash(), 1);
    assert_eq!(ctl.main_screen().counts.trash, 0);
}

// ---- Lists ----------------------------------------------------------------------------------

#[test]
fn a_new_list_opens_the_editor_and_saving_adds_and_opens_it() {
    let mut ctl = pro();
    ctl.main_new_list();
    let editor = ctl.main_screen().editor.expect("editor");
    assert_eq!(editor.kind, EditorKind::List);
    assert!(editor.is_new && !editor.can_delete);
    assert_eq!(
        editor.name, "New list 1",
        "a sample name the editor selects"
    );

    ctl.main_editor_save("   ", None, ListSort::Manual);
    let editor = ctl.main_screen().editor.expect("still open");
    assert_eq!(editor.name_error, Some(NameError::Empty));
    ctl.main_editor_edited();
    assert_eq!(ctl.main_screen().editor.unwrap().name_error, None);
    ctl.main_editor_save(&"x".repeat(101), None, ListSort::Manual);
    assert_eq!(
        ctl.main_screen().editor.unwrap().name_error,
        Some(NameError::TooLong { max: 100 })
    );

    ctl.main_editor_save("  Snippets 2 ", Some(ListAccent::Rose), ListSort::MostUsed);
    let screen = ctl.main_screen();
    assert!(screen.editor.is_none());
    assert_eq!(screen.lists.len(), 4);
    let new = &screen.lists[3];
    assert_eq!(
        (new.name.as_str(), new.accent),
        ("Snippets 2", Some(ListAccent::Rose))
    );
    assert_eq!(screen.section, Section::List(new.id));
    assert_eq!(ctl.view().tabs.len(), 4, "the Panel's tab bar shows it");
}

#[test]
fn a_cancelled_editor_changes_nothing() {
    let mut ctl = pro();
    ctl.main_new_list();
    ctl.main_editor_close();
    assert!(ctl.main_screen().editor.is_none());
    assert_eq!(ctl.main_screen().lists.len(), 3);
}

#[test]
fn editing_a_list_renames_recolours_and_re_sorts_it() {
    let mut ctl = pro();
    let screen = ctl.main_screen();
    let index = list_index(&screen, "Addresses");
    ctl.main_edit_list(index);
    let editor = ctl.main_screen().editor.expect("editor");
    assert!(!editor.is_new && editor.can_delete);
    assert_eq!(
        (editor.name.as_str(), editor.accent, editor.item_count),
        ("Addresses", Some(ListAccent::Amber), 2)
    );
    ctl.main_editor_save("Places", None, ListSort::RecentlyUsed);
    let screen = ctl.main_screen();
    let list = &screen.lists[index];
    assert_eq!((list.name.as_str(), list.accent), ("Places", None));
    assert_eq!(ctl.view().tabs[index].name, "Places");
}

#[test]
fn deleting_a_list_trashes_it_with_its_items_and_undo_restores_it() {
    let mut ctl = pro();
    let screen = ctl.main_screen();
    let index = list_index(&screen, "Email replies");
    assert_eq!(
        screen.lists[index].count, 6,
        "the confirmation names this count"
    );
    ctl.main_delete_list(index);
    let screen = ctl.main_screen();
    assert_eq!(screen.lists.len(), 2);
    assert_eq!(screen.counts.all, 5);
    assert_eq!(screen.counts.trash, 1, "one entry, whatever is inside it");
    assert_eq!(
        message(&screen),
        ToastMessage::ListTrashed {
            name: "Email replies".into()
        }
    );
    ctl.main_undo();
    let screen = ctl.main_screen();
    assert_eq!(screen.lists.len(), 3);
    assert_eq!(screen.counts.all, 11);
    assert_eq!(screen.lists[0].name, "Email replies", "back in its place");
}

#[test]
fn the_editor_can_delete_a_list_after_its_confirmation() {
    let mut ctl = pro();
    let index = list_index(&ctl.main_screen(), "Code");
    ctl.main_edit_list(index);
    ctl.main_editor_delete();
    let screen = ctl.main_screen();
    assert!(screen.editor.is_none());
    assert_eq!(screen.lists.len(), 2);
    assert!(screen.toast.unwrap().undo);
}

// ---- The Panel's list tab menu --------------------------------------------------------------

fn tab_index(ctl: &Ctl, name: &str) -> usize {
    ctl.view()
        .tabs
        .iter()
        .position(|t| t.name == name)
        .expect("tab")
}

#[test]
fn the_list_menu_opens_on_a_tab_and_closes_without_a_choice() {
    let mut ctl = pro();
    assert_eq!(ctl.view().list_menu, None);
    ctl.open_list_menu(tab_index(&ctl, "Code"));
    assert_eq!(ctl.view().list_menu, Some(LIST_MENU.to_vec()));
    ctl.close_list_menu();
    assert_eq!(ctl.view().list_menu, None);
    ctl.open_list_menu(99);
    assert_eq!(ctl.view().list_menu, None, "no such tab");
    assert!(ctl.main_screen().editor.is_none());
}

#[test]
fn the_list_menu_edits_the_list_it_was_opened_on() {
    let mut ctl = pro();
    ctl.open_list_menu(tab_index(&ctl, "Code"));
    assert_eq!(ctl.choose_list_menu(0), vec![Effect::OpenListEditor]);
    assert_eq!(ctl.view().list_menu, None, "a choice closes the menu");
    let editor = ctl.main_screen().editor.expect("editor");
    assert_eq!(editor.name, "Code");
    assert!(!editor.is_new && editor.can_delete && !editor.open_delete);
    ctl.main_editor_save("Snippets", None, ListSort::Manual);
    assert_eq!(
        ctl.view().tabs[tab_index(&ctl, "Snippets")].name,
        "Snippets"
    );
}

#[test]
fn the_list_menu_delete_confirms_then_trashes_with_undo() {
    let mut ctl = pro();
    let tabs = ctl.view().tabs.len();
    ctl.open_list_menu(tab_index(&ctl, "Code"));
    assert_eq!(ctl.choose_list_menu(1), vec![Effect::OpenListEditor]);
    let editor = ctl.main_screen().editor.expect("editor");
    assert!(editor.open_delete, "the confirmation is already asking");
    assert_eq!(ctl.view().tabs.len(), tabs, "nothing is deleted before it");
    ctl.main_editor_delete();
    assert_eq!(ctl.view().tabs.len(), tabs - 1);
    assert!(ctl.view().toast.expect("toast").undo);
    ctl.main_undo();
    assert_eq!(ctl.view().tabs.len(), tabs);
}

#[test]
fn a_double_click_on_a_tab_opens_the_list_editor() {
    let mut ctl = pro();
    let code = tab_index(&ctl, "Code");
    assert_eq!(ctl.edit_list_tab(code), vec![Effect::OpenListEditor]);
    let editor = ctl.main_screen().editor.expect("editor");
    assert_eq!(editor.name, "Code");
    assert!(!editor.open_delete);
}

// ---- Boards and the Free cap ----------------------------------------------------------------

#[test]
fn a_new_board_is_made_through_the_menu_and_opened() {
    let mut ctl = pro();
    ctl.main_board_menu_open();
    let new = ctl
        .main_screen()
        .board_menu
        .unwrap()
        .iter()
        .position(|e| *e == snipdeck_app::controller::BoardMenuEntry::New)
        .unwrap();
    ctl.main_board_menu_activate(new);
    let editor = ctl.main_screen().editor.expect("board editor");
    assert_eq!(editor.kind, EditorKind::Board);
    assert!(editor.is_new);
    assert_eq!(editor.name, "New board 1");
    ctl.main_editor_save("Side projects", None, ListSort::Manual);
    let screen = ctl.main_screen();
    assert_eq!(screen.board_name, "Side projects");
    assert_eq!(screen.lists.len(), 0);
    assert_eq!(ctl.store().boards().unwrap().len(), 3);
    assert_eq!(
        ctl.view().board_name,
        "Work",
        "the Panel keeps its own board"
    );
}

fn menu_index(ctl: &mut Ctl, entry: &snipdeck_app::controller::BoardMenuEntry) -> usize {
    ctl.main_board_menu_open();
    let menu = ctl.main_screen().board_menu.unwrap();
    ctl.main_board_menu_close();
    menu.iter().position(|e| e == entry).expect("entry")
}

#[test]
fn renaming_the_open_board_uses_the_board_editor() {
    use snipdeck_app::controller::BoardMenuEntry;
    let mut ctl = pro();
    let rename = menu_index(&mut ctl, &BoardMenuEntry::Rename);
    ctl.main_board_menu_activate(rename);
    let editor = ctl.main_screen().editor.unwrap();
    assert_eq!(editor.name, "Work");
    assert_eq!(editor.item_count, 10, "the lists of the open board");
    assert!(editor.can_delete && !editor.open_delete);
    ctl.main_editor_save("Office", None, ListSort::Manual);
    assert_eq!(ctl.main_screen().board_name, "Office");
    assert_eq!(ctl.view().board_name, "Office");
}

#[test]
fn deleting_a_board_asks_with_the_item_count_trashes_it_and_undo_restores_it() {
    use snipdeck_app::controller::BoardMenuEntry;
    let mut ctl = pro();
    let delete = menu_index(&mut ctl, &BoardMenuEntry::Delete);
    ctl.main_board_menu_activate(delete);
    let editor = ctl.main_screen().editor.unwrap();
    assert!(editor.open_delete, "the confirmation opens with the editor");
    assert_eq!(editor.item_count, 10);
    ctl.main_editor_delete();
    let screen = ctl.main_screen();
    assert_eq!(
        screen.board_name, "Personal",
        "falls back to the next board"
    );
    assert_eq!(screen.counts.all, 1);
    assert_eq!(
        message(&screen),
        ToastMessage::BoardTrashed {
            name: "Work".into()
        }
    );
    ctl.main_undo();
    let screen = ctl.main_screen();
    assert_eq!(ctl.store().boards().unwrap().len(), 2);
    assert_eq!(screen.counts.all, 11);
}

#[test]
fn the_last_board_cannot_be_deleted() {
    use snipdeck_app::controller::BoardMenuEntry;
    let mut ctl = pro();
    // Delete Work, leaving Personal.
    let delete = menu_index(&mut ctl, &BoardMenuEntry::Delete);
    ctl.main_board_menu_activate(delete);
    ctl.main_editor_delete();
    let delete = menu_index(&mut ctl, &BoardMenuEntry::Delete);
    ctl.main_board_menu_activate(delete);
    let editor = ctl.main_screen().editor.unwrap();
    assert!(!editor.can_delete);
    assert_eq!(editor.delete_hint, Some(DeleteHint::LastBoard));
    assert!(!editor.open_delete);
    ctl.main_editor_delete();
    assert_eq!(ctl.store().boards().unwrap().len(), 1);
}

#[test]
fn free_allows_three_boards_and_a_fourth_says_so() {
    use snipdeck_app::controller::BoardMenuEntry;
    let (mut ctl, _) = rig(Tier::Free);
    // Two boards in the demo; add the third.
    let new = menu_index(&mut ctl, &BoardMenuEntry::New);
    ctl.main_board_menu_activate(new);
    assert!(ctl.main_screen().editor.is_some());
    ctl.main_editor_save("Third", None, ListSort::Manual);
    assert_eq!(ctl.store().boards().unwrap().len(), 3);

    let new = menu_index(&mut ctl, &BoardMenuEntry::New);
    ctl.main_board_menu_activate(new);
    let screen = ctl.main_screen();
    assert!(screen.editor.is_none(), "no editor for a fourth board");
    assert_eq!(message(&screen), ToastMessage::BoardLimit);
    assert_eq!(ctl.store().boards().unwrap().len(), 3);
}

#[test]
fn undo_cannot_bring_a_board_back_past_the_free_limit() {
    use snipdeck_app::controller::BoardMenuEntry;
    let (mut ctl, _) = rig(Tier::Free);
    let new = menu_index(&mut ctl, &BoardMenuEntry::New);
    ctl.main_board_menu_activate(new);
    ctl.main_editor_save("Third", None, ListSort::Manual);
    // Delete the open board (Third), add another, then try to restore Third from the Trash.
    let delete = menu_index(&mut ctl, &BoardMenuEntry::Delete);
    ctl.main_board_menu_activate(delete);
    ctl.main_editor_delete();
    let third = ctl.store().trashed().unwrap()[0].target;
    let new = menu_index(&mut ctl, &BoardMenuEntry::New);
    ctl.main_board_menu_activate(new);
    ctl.main_editor_save("Replacement", None, ListSort::Manual);
    assert_eq!(ctl.store().boards().unwrap().len(), 3);
    ctl.main_open(Section::Trash);
    let at = ctl
        .main_screen()
        .rows
        .iter()
        .position(|r| Trashable::from(r.key) == third)
        .unwrap();
    ctl.main_action(MainAction::Restore, at);
    let screen = ctl.main_screen();
    assert_eq!(message(&screen), ToastMessage::BoardLimit);
    assert_eq!(ctl.store().boards().unwrap().len(), 3, "still three");
    assert_eq!(screen.counts.trash, 1, "it waits in the Trash");
}

// ---- Reordering -----------------------------------------------------------------------------

#[test]
fn alt_arrows_move_an_item_within_its_list_and_the_cursor_follows() {
    let mut ctl = pro();
    open_list(&mut ctl, "Email replies");
    ctl.main_select(1, SelectMode::Replace);
    ctl.main_reorder_items(1);
    let screen = ctl.main_screen();
    assert_eq!(
        titles(&screen)[..3],
        ["Thanks, received", "Out of office", "Invoice reminder"]
    );
    assert_eq!(screen.cursor, 2);
    ctl.main_reorder_items(-1);
    ctl.main_reorder_items(-1);
    let screen = ctl.main_screen();
    assert_eq!(titles(&screen)[0], "Invoice reminder");
    assert_eq!(screen.cursor, 0);
    ctl.main_reorder_items(-1);
    assert_eq!(ctl.main_screen().cursor, 0, "off the top does nothing");
    assert_eq!(
        ctl.view().rows[0].title,
        "Invoice reminder",
        "the Panel agrees"
    );
}

#[test]
fn items_do_not_move_in_a_sorted_list_a_search_or_a_system_view() {
    let mut ctl = pro();
    let before = titles(&ctl.main_screen());
    ctl.main_reorder_items(1);
    assert_eq!(titles(&ctl.main_screen()), before, "All items");

    open_list(&mut ctl, "Code");
    let index = list_index(&ctl.main_screen(), "Code");
    ctl.main_edit_list(index);
    ctl.main_editor_save("Code", Some(ListAccent::Violet), ListSort::MostUsed);
    let before = titles(&ctl.main_screen());
    ctl.main_reorder_items(1);
    assert_eq!(titles(&ctl.main_screen()), before, "sorted by use");
}

#[test]
fn lists_and_boards_reorder_with_the_same_steps() {
    use snipdeck_app::controller::BoardMenuEntry;
    let mut ctl = pro();
    ctl.main_reorder_list(2, -1);
    let names: Vec<_> = ctl
        .main_screen()
        .lists
        .iter()
        .map(|l| l.name.clone())
        .collect();
    assert_eq!(names, ["Email replies", "Code", "Addresses"]);
    assert_eq!(
        ctl.view()
            .tabs
            .iter()
            .map(|t| t.name.clone())
            .collect::<Vec<_>>(),
        names,
        "the Panel's tabs follow"
    );
    ctl.main_reorder_list(0, -1);
    assert_eq!(
        ctl.main_screen().lists[0].name,
        "Email replies",
        "off the top"
    );

    let down = menu_index(&mut ctl, &BoardMenuEntry::MoveDown { enabled: true });
    ctl.main_board_menu_activate(down);
    let boards: Vec<_> = ctl
        .store()
        .boards()
        .unwrap()
        .iter()
        .map(|b| b.name.as_str().to_owned())
        .collect();
    assert_eq!(boards, ["Personal", "Work"]);
    assert_eq!(ctl.main_screen().board_name, "Work");
    ctl.main_board_menu_open();
    let menu = ctl.main_screen().board_menu.unwrap();
    assert!(menu.contains(&BoardMenuEntry::MoveDown { enabled: false }));
    assert!(menu.contains(&BoardMenuEntry::MoveUp { enabled: true }));
}

#[test]
fn an_empty_view_has_nothing_to_act_on() {
    let mut ctl = pro();
    ctl.main_open(Section::Recent);
    assert!(ctl.main_action(MainAction::Delete, 0).is_none());
    assert_eq!(ctl.main_screen().counts.trash, 0);
}

// ---- Maintenance ----------------------------------------------------------------------------

#[test]
fn maintenance_reports_a_damaged_library_and_a_crash_as_toasts() {
    use snipdeck_app::{maintenance::MaintenanceOutcome, toast::ToastKind};
    let mut ctl = pro();
    ctl.report_maintenance(MaintenanceOutcome::IntegrityOk);
    assert!(ctl.main_screen().toast.is_none(), "a clean check is silent");

    ctl.report_maintenance(MaintenanceOutcome::IntegrityProblems(vec!["x".into()]));
    let screen = ctl.main_screen();
    assert_eq!(
        screen.toast.as_ref().expect("a toast").kind,
        ToastKind::Danger
    );
    assert_eq!(message(&screen), ToastMessage::LibraryProblem);

    ctl.report_maintenance(MaintenanceOutcome::CrashedLastTime);
    assert_eq!(message(&ctl.main_screen()), ToastMessage::CrashedLastTime);
}

#[test]
fn the_main_windows_help_entries_queue_the_same_effect_as_the_panels_kebab() {
    use snipdeck_app::controller::{Effect, HelpTopic};
    let (mut ctl, _) = rig(Tier::Pro);
    for topic in [
        HelpTopic::Troubleshoot,
        HelpTopic::Guide,
        HelpTopic::About,
        HelpTopic::Shortcuts,
    ] {
        ctl.main_open_help(topic);
        assert_eq!(ctl.take_pending(), [Effect::OpenHelp(topic)]);
    }
    assert_eq!(
        ctl.open_help(HelpTopic::Guide),
        [Effect::OpenHelp(HelpTopic::Guide)]
    );
}

#[test]
fn the_real_jobs_back_up_purge_and_check_the_library_on_the_injected_clock() {
    use snipdeck_app::maintenance::{self, Maintenance};
    let dir = tempfile::tempdir().unwrap();
    let (mut ctl, clock) = rig(Tier::Pro);
    ctl.main_action(MainAction::Delete, 0);
    assert_eq!(ctl.main_screen().counts.trash, 1);

    // 31 days later, on the first run after a start: the entry is purged and a backup exists.
    clock.set("2026-11-17T10:31:00Z".parse().unwrap());
    let mut m = Maintenance::new(dir.path(), Box::new(clock.clone()));
    assert_eq!(maintenance::run(&mut m, &mut ctl), 3);
    assert_eq!(ctl.main_screen().counts.trash, 0);
    let backups = snipdeck_portable::backup::list_backups(dir.path()).unwrap();
    assert_eq!(backups.len(), 1);
    assert!(dir.path().join("maintenance.json").exists());

    // An hour later nothing is due; a restart does not change that.
    clock.set("2026-11-17T11:31:00Z".parse().unwrap());
    assert_eq!(maintenance::run(&mut m, &mut ctl), 0);
    let mut restarted = Maintenance::new(dir.path(), Box::new(clock.clone()));
    assert_eq!(maintenance::run(&mut restarted, &mut ctl), 0);
}

#[test]
fn the_main_window_can_show_the_panel_again_and_names_its_hotkey() {
    use snipdeck_app::controller::Effect;
    let (mut ctl, _) = rig(Tier::Pro);
    assert_eq!(ctl.main_screen().panel_hotkey, "Ctrl+Alt+Space");
    // The Panel was closed: "Show Panel" asks the host to show it, as the tray does.
    drop(ctl.hide());
    assert!(!ctl.visible());
    ctl.main_show_panel();
    assert!(ctl.visible());
    assert_eq!(ctl.take_pending(), [Effect::ShowPanel]);
    // Already up: the same effect brings it forward.
    ctl.main_show_panel();
    assert_eq!(ctl.take_pending(), [Effect::ShowPanel]);
}

// ---- The command palette's routes ----------------------------------------------------------------

#[test]
fn palette_commands_that_belong_to_other_windows_queue_the_menus_effects() {
    use snipdeck_app::{
        commands::Command,
        controller::{Effect, HelpTopic},
    };
    let mut ctl = pro();
    for (command, effects) in [
        (Command::Settings, vec![Effect::OpenSettings]),
        (Command::ShowPanel, vec![Effect::ShowPanel]),
        (Command::Tour, vec![]),
        (
            Command::Shortcuts,
            vec![Effect::OpenHelp(HelpTopic::Shortcuts)],
        ),
        (Command::UserGuide, vec![Effect::OpenHelp(HelpTopic::Guide)]),
        (
            Command::Troubleshoot,
            vec![Effect::OpenHelp(HelpTopic::Troubleshoot)],
        ),
        (Command::About, vec![Effect::OpenHelp(HelpTopic::About)]),
    ] {
        ctl.run_command(command, 0);
        assert_eq!(ctl.take_pending(), effects, "{command:?}");
    }
}

#[test]
fn palette_navigation_and_view_commands_change_the_main_screen() {
    use snipdeck_app::{commands::Command, main_window::ViewMode};
    let mut ctl = pro();
    ctl.run_command(Command::GoTrash, 0);
    assert_eq!(ctl.main_screen().section, Section::Trash);
    ctl.run_command(Command::GoPinned, 0);
    assert_eq!(ctl.main_screen().section, Section::Pinned);
    ctl.run_command(Command::GoAll, 0);
    assert_eq!(ctl.main_screen().section, Section::All);
    ctl.run_command(Command::ViewTile, 0);
    assert_eq!(ctl.main_screen().view_mode, ViewMode::Tile);
    ctl.run_command(Command::ViewCompact, 0);
    assert_eq!(ctl.main_screen().view_mode, ViewMode::Compact);
}

#[test]
fn palette_item_commands_act_on_the_row_under_the_cursor() {
    use snipdeck_app::commands::Command;
    let mut ctl = pro();
    let before = ctl.main_screen();
    let target = before.rows[1].title.clone();
    let pinned = before.rows[1].pinned;

    ctl.run_command(Command::PinItem, 1);
    let after = ctl.main_screen();
    assert_eq!(after.rows[row_of(&after, &target)].pinned, !pinned);

    ctl.run_command(Command::TrashItem, row_of(&after, &target));
    let after = ctl.main_screen();
    assert!(!titles(&after).contains(&target), "moved to the Trash");
    assert_eq!(after.counts.trash, before.counts.trash + 1);
}

#[test]
fn the_palette_new_list_command_opens_the_list_editor() {
    use snipdeck_app::{commands::Command, main_window::EditorKind};
    let mut ctl = pro();
    ctl.run_command(Command::NewList, 0);
    let editor = ctl.main_screen().editor.expect("the list editor");
    assert_eq!(editor.kind, EditorKind::List);
    assert!(editor.is_new);
}
