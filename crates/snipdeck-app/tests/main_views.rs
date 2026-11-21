//! The Main window's views: the filter (tags any or all, untagged, kind, trigger, sensitive), one
//! sort for every section, groups under a sort, tag colours and merging, board looks, and the
//! Trash's search. Over the demo library, the platform fakes and a frozen clock.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use snipdeck_app::{
    controller::{BoardMenuEntry, Controller, MainAction, MainScreen, Services, TagRenameError},
    demo,
    host::{FakeTargets, FixedClock},
    main_window::{FilterTerm, Section, SelectMode},
    settings::Settings,
};
use snipdeck_core::{
    ItemKind, ItemLabel, ListAccent, ListIcon, Name, Sort, SortKey, Tag, Timestamp,
};
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_store::{BoardRepo, Store, TagRepo, TrashRepo};

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

fn rig_on(store: Store) -> Ctl {
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
    .expect("controller")
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

/// The demo library with tags `a` and `b`: "Thanks, received" has both, "Out of office" has `a`,
/// "Home address" has `b`.
fn tagged() -> Ctl {
    let mut store = demo::store(now()).unwrap();
    let (a, b) = (
        Tag::new(Name::new("a").unwrap()),
        Tag::new(Name::new("b").unwrap()),
    );
    store.insert_tag(&a).unwrap();
    store.insert_tag(&b).unwrap();
    let mut ctl = rig_on(store);
    ctl.main_open(Section::All);
    for (title, tags) in [
        ("Thanks, received", vec![0, 1]),
        ("Out of office", vec![0]),
        ("Home address", vec![1]),
    ] {
        let at = row_of(&ctl.main_screen(), title);
        for tag in tags {
            ctl.main_action(MainAction::Tag, at);
            ctl.main_toggle_tag(tag, true);
        }
    }
    ctl
}

#[test]
fn two_tags_in_the_filter_match_any_by_default_and_all_on_request() {
    let mut ctl = tagged();
    let all = ctl.main_screen().rows.len();
    ctl.main_toggle_filter_tag(0);
    ctl.main_toggle_filter_tag(1);
    let any = titles(&ctl.main_screen());
    assert_eq!(any.len(), 3, "{any:?}");
    ctl.main_set_filter_match_all(true);
    assert_eq!(titles(&ctl.main_screen()), ["Thanks, received"]);
    assert_eq!(ctl.main_screen().filter_chips.len(), 2);
    ctl.main_clear_filter();
    assert_eq!(ctl.main_screen().filter_chips.len(), 0);
    assert_eq!(ctl.main_screen().rows.len(), all);
}

#[test]
fn untagged_kind_trigger_and_sensitive_each_narrow_the_view() {
    let mut ctl = tagged();
    let all = ctl.main_screen().rows.len();
    ctl.main_toggle_filter(FilterTerm::Untagged);
    assert_eq!(ctl.main_screen().rows.len(), all - 3);
    ctl.main_clear_filter();
    ctl.main_toggle_filter(FilterTerm::Kind(ItemKind::Link));
    assert_eq!(titles(&ctl.main_screen()), ["Design spec"]);
    ctl.main_clear_filter();
    ctl.main_toggle_filter(FilterTerm::Sensitive);
    assert_eq!(titles(&ctl.main_screen()), ["Server password"]);
    ctl.main_clear_filter();
    ctl.main_toggle_filter(FilterTerm::HasTrigger);
    assert_eq!(ctl.main_screen().rows.len(), 0, "the demo has no triggers");
}

#[test]
fn every_section_sorts_by_the_one_sort_and_keeps_it_per_section() {
    let mut ctl = tagged();
    for section in [Section::All, Section::Pinned] {
        ctl.main_open(section);
        assert!(ctl.main_screen().sortable, "{section:?}");
    }
    ctl.main_open(Section::All);
    ctl.main_set_sort(Sort {
        key: SortKey::Name,
        reversed: false,
    });
    let a_to_z = titles(&ctl.main_screen());
    let mut sorted = a_to_z.clone();
    sorted.sort_by_key(|t| t.to_lowercase());
    assert_eq!(a_to_z, sorted);
    ctl.main_set_sort(Sort {
        key: SortKey::Name,
        reversed: true,
    });
    let z_to_a = titles(&ctl.main_screen());
    sorted.reverse();
    assert_eq!(z_to_a, sorted);
    // A tag's view keeps its own sort.
    let tag = ctl.main_screen().tags[0].id;
    ctl.main_open(Section::Tag(tag));
    assert_eq!(ctl.main_screen().sort, Sort::MANUAL);
    ctl.main_open(Section::All);
    assert!(ctl.main_screen().sort.reversed);
}

#[test]
fn a_group_keeps_its_header_under_a_sort_and_sorts_inside() {
    let mut ctl = tagged();
    let board = ctl.main_screen().board_name;
    assert_eq!(board, "Work");
    let list = ctl.main_screen().lists[0].id;
    ctl.main_open(Section::List(list));
    ctl.main_select(1, SelectMode::Replace);
    ctl.main_select(2, SelectMode::Toggle);
    ctl.main_group_marked(1);
    let before = ctl
        .main_screen()
        .rows
        .iter()
        .filter(|r| r.header.is_some())
        .count();
    assert_eq!(before, 1);
    ctl.main_set_sort(Sort {
        key: SortKey::Name,
        reversed: true,
    });
    let screen = ctl.main_screen();
    assert_eq!(
        screen.rows.iter().filter(|r| r.header.is_some()).count(),
        1,
        "the header stays under a sort"
    );
    let at = screen.rows.iter().position(|r| r.header.is_some()).unwrap();
    let members: Vec<&str> = screen.rows[at + 1..at + 3]
        .iter()
        .map(|r| r.title.as_str())
        .collect();
    let mut expect = members.clone();
    expect.sort_by_key(|t| std::cmp::Reverse(t.to_lowercase()));
    assert_eq!(members, expect, "sorted inside the group");
    assert!(screen.rows[at + 1].depth == 1 && screen.rows[at + 2].depth == 1);
}

#[test]
fn deleting_a_folded_group_asks_with_the_count_and_cancel_keeps_it() {
    let mut ctl = tagged();
    let list = ctl.main_screen().lists[0].id;
    ctl.main_open(Section::List(list));
    ctl.main_select(0, SelectMode::Replace);
    ctl.main_select(1, SelectMode::Toggle);
    ctl.main_group_marked(0);
    ctl.main_group_collapse(0, Some(true));
    let rows = ctl.main_screen().rows.len();
    ctl.main_group_delete(0);
    let ask = ctl.main_screen().group_delete.expect("asks first");
    assert_eq!(ask.1, 2);
    assert_eq!(ctl.main_screen().rows.len(), rows, "nothing is deleted yet");
    ctl.main_group_delete_cancel();
    assert_eq!(ctl.main_screen().group_delete, None);
    assert_eq!(ctl.main_screen().rows.len(), rows);
    ctl.main_group_delete(0);
    ctl.main_group_delete_confirm();
    assert_eq!(ctl.main_screen().rows.len(), rows - 1);
    ctl.undo();
    assert_eq!(ctl.main_screen().rows.len(), rows);
}

#[test]
fn an_unfolded_group_is_deleted_without_asking() {
    let mut ctl = tagged();
    let list = ctl.main_screen().lists[0].id;
    ctl.main_open(Section::List(list));
    ctl.main_select(0, SelectMode::Replace);
    ctl.main_group_marked(0);
    ctl.main_group_delete(0);
    assert_eq!(ctl.main_screen().group_delete, None);
}

#[test]
fn a_group_moves_to_another_list_whole_and_one_undo_brings_it_back() {
    let mut ctl = tagged();
    let screen = ctl.main_screen();
    let (from, to) = (screen.lists[0].id, 1);
    ctl.main_open(Section::List(from));
    ctl.main_select(0, SelectMode::Replace);
    ctl.main_select(1, SelectMode::Toggle);
    ctl.main_group_marked(0);
    ctl.main_group_rename(0, "Pair");
    let moved = ctl.main_screen().rows[0].header.as_ref().map(|h| h.count);
    assert_eq!(moved, Some(2));
    ctl.main_group_move_start(0);
    ctl.main_move_to(to);
    let target = ctl.main_screen().lists[to].id;
    ctl.main_open(Section::List(target));
    let screen = ctl.main_screen();
    let header = screen
        .rows
        .iter()
        .find(|r| r.header.is_some())
        .expect("the group came along");
    assert_eq!(header.title, "Pair");
    assert_eq!(header.header.as_ref().map(|h| h.count), Some(2));
    ctl.undo();
    ctl.main_open(Section::List(from));
    assert_eq!(ctl.main_screen().rows[0].title, "Pair");
}

fn header_colour(ctl: &mut Ctl) -> Option<snipdeck_core::ListAccent> {
    ctl.main_screen()
        .rows
        .iter()
        .find_map(|r| r.header.as_ref())
        .and_then(|h| h.colour)
}

#[test]
fn a_group_colour_is_one_undo_step_and_goes_with_the_group_to_another_list() {
    use snipdeck_core::ListAccent;
    let mut ctl = tagged();
    let (from, to) = (ctl.main_screen().lists[0].id, 1);
    ctl.main_open(Section::List(from));
    ctl.main_select(0, SelectMode::Replace);
    ctl.main_select(1, SelectMode::Toggle);
    ctl.main_group_marked(0);
    ctl.main_group_rename(0, "Pair");
    assert_eq!(header_colour(&mut ctl), None, "a new group has no colour");

    ctl.main_group_colour(0, Some(ListAccent::Teal));
    assert_eq!(header_colour(&mut ctl), Some(ListAccent::Teal));
    // The same colour again changes nothing and adds no step.
    ctl.main_group_colour(0, Some(ListAccent::Teal));
    ctl.main_group_colour(0, Some(ListAccent::Rose));
    ctl.main_group_colour(0, None);
    assert_eq!(header_colour(&mut ctl), None);
    ctl.undo();
    assert_eq!(header_colour(&mut ctl), Some(ListAccent::Rose));
    ctl.undo();
    assert_eq!(header_colour(&mut ctl), Some(ListAccent::Teal));
    ctl.redo();
    assert_eq!(header_colour(&mut ctl), Some(ListAccent::Rose));

    // The colour survives an ungroup and its undo, and the move to another list.
    ctl.main_group_ungroup(0);
    ctl.undo();
    assert_eq!(header_colour(&mut ctl), Some(ListAccent::Rose));
    ctl.main_group_move_start(0);
    ctl.main_move_to(to);
    let target = ctl.main_screen().lists[to].id;
    ctl.main_open(Section::List(target));
    assert_eq!(header_colour(&mut ctl), Some(ListAccent::Rose));
    ctl.undo();
    ctl.main_open(Section::List(from));
    assert_eq!(header_colour(&mut ctl), Some(ListAccent::Rose));
}

#[test]
fn a_tag_takes_one_of_the_item_colours_and_undo_takes_it_off() {
    let mut ctl = tagged();
    ctl.main_set_tag_colour(0, Some(ItemLabel::Red));
    assert_eq!(ctl.main_screen().tags[0].colour, Some(ItemLabel::Red));
    ctl.undo();
    assert_eq!(ctl.main_screen().tags[0].colour, None);
}

#[test]
fn a_rename_that_clashes_can_merge_the_tag_into_the_other_and_undo_splits_them() {
    let mut ctl = tagged();
    assert_eq!(ctl.main_rename_tag(0, "b"), Err(TagRenameError::Exists));
    ctl.main_merge_tag(0, "b").expect("merged");
    let screen = ctl.main_screen();
    assert_eq!(screen.tags.len(), 1);
    assert_eq!(screen.tags[0].name, "b");
    assert_eq!(screen.tags[0].count, 3, "every item of a now carries b");
    ctl.undo();
    let screen = ctl.main_screen();
    assert_eq!(screen.tags.len(), 2);
    assert_eq!(screen.tags[0].count, 2);
    assert_eq!(screen.tags[1].count, 2);
}

#[test]
fn a_tag_stands_for_the_tags_nested_under_it_in_its_view_its_count_the_filter_and_search() {
    let mut ctl = tagged();
    // a carries Thanks and Out of office; b carries Thanks and Home address.
    assert_eq!(ctl.main_screen().tags[0].count, 2);
    ctl.main_nest_tag(1, Some(0));
    let screen = ctl.main_screen();
    assert_eq!(
        screen.tags[0].count, 3,
        "a counts what is under it, each item once"
    );
    assert_eq!(screen.tags[1].count, 2);
    ctl.main_open(Section::Tag(screen.tags[0].id));
    assert_eq!(titles(&ctl.main_screen()).len(), 3);
    ctl.main_open(Section::Tag(screen.tags[1].id));
    assert_eq!(titles(&ctl.main_screen()).len(), 2);
    ctl.main_open(Section::All);
    ctl.main_toggle_filter_tag(0);
    let filtered = titles(&ctl.main_screen());
    assert_eq!(filtered.len(), 3, "{filtered:?}");
    ctl.main_clear_filter();
    ctl.main_set_query("tag:a");
    assert_eq!(titles(&ctl.main_screen()).len(), 3);
    ctl.main_set_query("tag:a/b");
    let found = titles(&ctl.main_screen());
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.contains(&"Home address".to_owned()), "{found:?}");
    ctl.main_set_query("tag:b");
    assert_eq!(titles(&ctl.main_screen()).len(), 2);
}

#[test]
fn a_board_keeps_the_colour_and_icon_its_editor_gave_it() {
    let mut ctl = rig_on(demo::store(now()).unwrap());
    ctl.main_board_menu_open();
    let new = ctl
        .main_screen()
        .board_menu
        .unwrap()
        .iter()
        .position(|e| *e == BoardMenuEntry::New)
        .unwrap();
    ctl.main_board_menu_activate(new);
    ctl.main_editor_save(
        "Side",
        Some(ListAccent::Teal),
        Some(ListIcon::Star),
        Sort::MANUAL,
    );
    let id = ctl
        .store()
        .boards()
        .unwrap()
        .into_iter()
        .find(|b| b.name.as_str() == "Side")
        .unwrap();
    assert_eq!(
        (id.accent, id.icon),
        (Some(ListAccent::Teal), Some(ListIcon::Star))
    );
    ctl.main_board_menu_open();
    let edit = ctl
        .main_screen()
        .board_menu
        .unwrap()
        .iter()
        .position(|e| matches!(e, BoardMenuEntry::Rename))
        .unwrap();
    ctl.main_board_menu_activate(edit);
    let editor = ctl.main_screen().editor.expect("editor");
    assert_eq!(
        (editor.accent, editor.icon),
        (Some(ListAccent::Teal), Some(ListIcon::Star))
    );
}

#[test]
fn the_trash_is_searched_by_title_and_preview() {
    let mut ctl = rig_on(demo::store(now()).unwrap());
    ctl.main_open(Section::All);
    let at = row_of(&ctl.main_screen(), "Out of office");
    ctl.main_action(MainAction::Delete, at);
    let at = row_of(&ctl.main_screen(), "Home address");
    ctl.main_action(MainAction::Delete, at);
    ctl.main_open(Section::Trash);
    assert_eq!(ctl.main_screen().rows.len(), 2);
    ctl.main_set_query("office");
    assert_eq!(titles(&ctl.main_screen()), ["Out of office"]);
    ctl.main_set_query("fremantle");
    assert_eq!(
        titles(&ctl.main_screen()),
        ["Home address"],
        "found by its preview"
    );
    ctl.main_set_query("zzzz");
    assert_eq!(ctl.main_screen().rows.len(), 0);
    assert_eq!(ctl.store().trashed().unwrap().len(), 2);
}
