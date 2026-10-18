//! The item editor's controller half, without a window: opening it, the checks on Save, the
//! write for every kind, the library key made on the first sensitive item, drafts, the read-only
//! viewer for large text and the Free-tier gates. Over the demo library, the platform fakes, a
//! frozen clock and a temporary data folder. The window itself is covered by `editor_window.rs`.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use std::{cell::RefCell, fs, path::Path, rc::Rc};

use snipdeck_app::{
    controller::{Controller, MainAction, Services},
    demo,
    drafts::{Draft, DraftKey, Drafts},
    host::{FakeTargets, FixedClock},
    item_editor::{
        EDIT_LIMIT_BYTES, EditorInput, EditorProblem, ItemEditorView, Origin, SaveFailure,
    },
    settings::Settings,
    toast::{ToastKind, ToastMessage},
};
use snipdeck_core::{
    Content, Entitlements, Feature, Format, FormatKind, Formats, Inline, Item, ItemId, ItemKind,
    Placement, Tier, Timestamp,
};
use snipdeck_platform::{
    file_dialog::{FakeFilePicker, PickKind},
    testing::{FakeClipboard, FakeKeyboard, FakeTiming},
};
use snipdeck_store::{BlobStore, ItemRepo, ListRepo, Store, TagRepo};

#[path = "support/mod.rs"]
mod support;
use support::{SharedProvider, assert_no_plaintext};

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

struct Rig {
    ctl: Ctl,
    clock: FixedClock,
    dir: tempfile::TempDir,
    provider: SharedProvider,
}

fn controller(
    store: Store,
    tier: Tier,
    dir: &Path,
    provider: &SharedProvider,
    with_keys: bool,
) -> (Ctl, FixedClock) {
    let clock = FixedClock::new(now());
    let mut ctl = Controller::new(
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
    ctl.set_drafts(Drafts::in_data_dir(dir));
    if with_keys {
        ctl.set_key_provider(Box::new(provider.clone()));
    }
    (ctl, clock)
}

fn rig(tier: Tier) -> Rig {
    let dir = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    let (ctl, clock) = controller(
        demo::store(now()).expect("demo"),
        tier,
        dir.path(),
        &provider,
        true,
    );
    Rig {
        ctl,
        clock,
        dir,
        provider,
    }
}

fn pro() -> Rig {
    rig(Tier::Pro)
}

fn id_of(ctl: &Ctl, title: &str) -> ItemId {
    ctl.store()
        .all_items()
        .expect("items")
        .into_iter()
        .find(|i| i.title == title)
        .unwrap_or_else(|| panic!("no item {title}"))
        .id
}

fn view(ctl: &Ctl) -> ItemEditorView {
    ctl.item_editor_view().expect("an editor is open")
}

fn toast(ctl: &Ctl) -> ToastMessage {
    ctl.view().toast.expect("a toast").message
}

fn text_of(ctl: &Ctl, id: ItemId) -> String {
    snipdeck_app::usecase::item_text(ctl.store(), id)
        .expect("text")
        .expect("some text")
}

/// The input a window would send after the user typed `title` and `content`.
fn typed(view: &ItemEditorView, title: &str, content: &str) -> EditorInput {
    EditorInput {
        title: title.to_owned(),
        content: content.to_owned(),
        ..view.fields.clone()
    }
}

fn save_new(ctl: &mut Ctl, kind: ItemKind, title: &str, content: &str) -> ItemId {
    ctl.open_new_item(Origin::Main);
    let v = view(ctl);
    let input = EditorInput {
        kind,
        ..typed(&v, title, content)
    };
    ctl.item_editor_save(&input);
    assert!(
        !ctl.item_editor_open(),
        "saved: {:?}",
        ctl.item_editor_view()
            .map(|v| (v.title_problem, v.content_problem, v.banner))
    );
    id_of(ctl, title)
}

// ---- Opening -----------------------------------------------------------------------------------

#[test]
fn a_new_item_starts_in_the_list_the_origin_shows_with_that_boards_lists_to_choose_from() {
    let mut r = pro();
    r.ctl.open_new_item(Origin::Panel);
    let v = view(&r.ctl);
    assert!(v.is_new);
    assert_eq!(v.fields.kind, ItemKind::Text);
    assert_eq!(v.list_names, ["Email replies", "Addresses", "Code"]);
    assert_eq!(v.fields.list_index, 0, "the Panel's first tab");
    assert_eq!(
        (v.fields.title.as_str(), v.fields.content.as_str()),
        ("", "")
    );
    assert!(v.prompted_available && v.encryption_available);
    assert!(!v.can_browse, "no file dialog was given");
    assert!(!v.restored);

    r.ctl.item_editor_cancel();
    let screen = r.ctl.main_screen();
    let code = screen
        .lists
        .iter()
        .find(|l| l.name == "Code")
        .expect("Code")
        .id;
    r.ctl
        .main_open(snipdeck_app::main_window::Section::List(code));
    r.ctl.open_new_item(Origin::Main);
    assert_eq!(
        view(&r.ctl).fields.list_index,
        2,
        "the Main window's open list"
    );
}

#[test]
fn a_second_request_brings_the_open_editor_forward_and_keeps_it() {
    let mut r = pro();
    r.ctl.open_new_item(Origin::Main);
    let first = view(&r.ctl);
    r.ctl.open_item_editor(id_of(&r.ctl, "Office"));
    let second = view(&r.ctl);
    assert!(second.is_new, "still the new-item editor");
    assert_eq!(second.seq, first.seq);
    assert_eq!(second.raise, first.raise + 1, "asked to come to the front");
    r.ctl.open_new_item(Origin::Panel);
    assert_eq!(view(&r.ctl).raise, first.raise + 2);
}

#[test]
fn a_library_with_no_list_says_so_instead_of_opening() {
    let dir = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    let (mut ctl, _) = controller(
        Store::open_in_memory().expect("store"),
        Tier::Pro,
        dir.path(),
        &provider,
        true,
    );
    ctl.open_new_item(Origin::Panel);
    assert!(!ctl.item_editor_open());
    assert_eq!(toast(&ctl), ToastMessage::NoListToSave);
}

#[test]
fn edit_opens_the_item_with_its_fields_and_the_main_window_and_picker_ask_the_same_way() {
    let mut r = pro();
    let office = id_of(&r.ctl, "Office");
    r.ctl.open_item_editor(office);
    let v = view(&r.ctl);
    assert!(!v.is_new);
    assert_eq!(v.fields.title, "Office");
    assert_eq!(
        v.fields.content,
        "Level 4, 100 St Georges Terrace, Perth WA 6000"
    );
    assert_eq!(v.list_names[v.fields.list_index], "Addresses");
    r.ctl.item_editor_cancel();
    assert!(!r.ctl.item_editor_open());

    // The Main window's Edit.
    let screen = r.ctl.main_screen();
    let row = screen
        .rows
        .iter()
        .position(|r| r.title == "Out of office")
        .expect("row");
    r.ctl.main_action(MainAction::Edit, row);
    assert_eq!(view(&r.ctl).fields.title, "Out of office");
    r.ctl.item_editor_cancel();

    // Ctrl+E in the Picker closes it and edits that result.
    r.ctl.open_picker();
    r.ctl.picker_query("Shopping");
    r.ctl.picker_edit(0);
    assert!(r.ctl.picker().is_none(), "the Picker closed");
    assert_eq!(view(&r.ctl).fields.title, "Shopping list");
    // Nothing happens for a row that is not there.
    r.ctl.item_editor_cancel();
    r.ctl.open_picker();
    r.ctl.picker_edit(99);
    assert!(!r.ctl.item_editor_open());
    assert!(r.ctl.picker().is_some());
}

#[test]
fn an_item_that_is_gone_is_a_toast_not_an_editor() {
    let mut r = pro();
    r.ctl.open_item_editor(ItemId::generate());
    assert!(!r.ctl.item_editor_open());
    assert_eq!(toast(&r.ctl), ToastMessage::ItemGone);
}

// ---- Saving each kind --------------------------------------------------------------------------

#[test]
fn a_new_text_item_is_saved_at_the_top_with_its_preview_and_a_toast() {
    let mut r = pro();
    let id = save_new(
        &mut r.ctl,
        ItemKind::Text,
        "Greeting",
        "  Hello there\nsecond line",
    );
    let item = r.ctl.store().item(id).expect("item");
    assert_eq!(item.kind, ItemKind::Text);
    assert_eq!(item.preview, "Hello there\nsecond line");
    assert!(!item.sensitive);
    assert_eq!(item.created_at, now());
    assert_eq!(text_of(&r.ctl, id), "  Hello there\nsecond line");
    let first = r.ctl.store().items(item.list_id).expect("items")[0].id;
    assert_eq!(first, id, "new items go to the top");
    let t = r.ctl.view().toast.expect("toast");
    assert_eq!(t.kind, ToastKind::Success);
    assert_eq!(
        t.message,
        ToastMessage::Saved {
            list: "Email replies".into()
        }
    );
    // The Panel shows it at once.
    assert_eq!(r.ctl.view().rows[0].title, "Greeting");
}

#[test]
fn code_link_and_template_items_round_trip() {
    let mut r = pro();
    let code = save_new(&mut r.ctl, ItemKind::Code, "Snippet", "fn main() {}\n");
    assert_eq!(r.ctl.store().item(code).expect("item").kind, ItemKind::Code);
    assert_eq!(text_of(&r.ctl, code), "fn main() {}\n");

    let link = save_new(
        &mut r.ctl,
        ItemKind::Link,
        "Site",
        "  https://example.com/a  ",
    );
    assert_eq!(r.ctl.store().item(link).expect("item").kind, ItemKind::Link);
    assert_eq!(text_of(&r.ctl, link), "https://example.com/a", "trimmed");

    let template = save_new(
        &mut r.ctl,
        ItemKind::Template,
        "Reminder",
        "Dear {field:Name}, today is {date}.",
    );
    assert_eq!(
        r.ctl.store().item(template).expect("item").kind,
        ItemKind::Template
    );
    assert_eq!(
        text_of(&r.ctl, template),
        "Dear {field:Name}, today is {date}."
    );
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> String {
    let path = dir.join(name);
    fs::write(&path, bytes).expect("write");
    path.to_str().expect("utf8").to_owned()
}

fn tiny_png() -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, 2, 1);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("header");
    writer.write_image_data(&[9, 8, 7, 6, 5, 4]).expect("data");
    writer.finish().expect("finish");
    out
}

#[test]
fn a_new_image_item_keeps_its_own_copy_of_the_picture() {
    let mut r = pro();
    let files = tempfile::tempdir().expect("dir");
    let png = tiny_png();
    let path = write(files.path(), "logo.png", &png);
    r.ctl.open_new_item(Origin::Main);
    let v = view(&r.ctl);
    r.ctl.item_editor_save(&EditorInput {
        kind: ItemKind::Image,
        title: "Logo".into(),
        source_path: path,
        ..v.fields
    });
    assert!(!r.ctl.item_editor_open());
    let id = id_of(&r.ctl, "Logo");
    let item = r.ctl.store().item(id).expect("item");
    assert_eq!((item.kind, item.preview.as_str()), (ItemKind::Image, ""));
    let formats = r.ctl.store().formats(id).expect("formats");
    let Some(Format::Png(content)) = formats.get(FormatKind::Png) else {
        panic!("a png format")
    };
    assert_eq!(
        r.ctl.store().read(content).expect("read").as_ref(),
        png.as_slice()
    );
    // Deleting the original does not matter: Snipdeck holds its own copy.
    fs::remove_file(files.path().join("logo.png")).expect("remove");
    assert_eq!(
        r.ctl.store().read(content).expect("read").as_ref(),
        png.as_slice()
    );

    // Opening it again shows what it holds, and the kind cannot change.
    r.ctl.open_item_editor(id);
    let v = view(&r.ctl);
    let a = v.attachment.expect("attachment");
    assert_eq!(
        (a.kind, a.dimensions, a.bytes),
        (ItemKind::Image, Some((2, 1)), png.len() as u64)
    );
    assert!(a.preview.is_some());
}

#[test]
fn a_new_file_item_keeps_the_file_under_its_name() {
    let mut r = pro();
    let files = tempfile::tempdir().expect("dir");
    let path = write(files.path(), "invoice.pdf", b"%PDF-1.7 data");
    r.ctl.open_new_item(Origin::Main);
    let v = view(&r.ctl);
    r.ctl.item_editor_save(&EditorInput {
        kind: ItemKind::File,
        title: "Invoice".into(),
        source_path: format!("\"{path}\""),
        ..v.fields
    });
    assert!(!r.ctl.item_editor_open());
    let id = id_of(&r.ctl, "Invoice");
    let formats = r.ctl.store().formats(id).expect("formats");
    let Some(Format::Files(files)) = formats.get(FormatKind::Files) else {
        panic!("a files format")
    };
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name(), "invoice.pdf");
    assert_eq!(
        r.ctl
            .store()
            .read(files[0].content())
            .expect("read")
            .as_ref(),
        b"%PDF-1.7 data"
    );
    r.ctl.open_item_editor(id);
    let a = view(&r.ctl).attachment.expect("attachment");
    assert_eq!((a.name.as_str(), a.files, a.bytes), ("invoice.pdf", 1, 13));
}

#[test]
fn a_bad_file_is_refused_with_the_reason_and_nothing_is_saved() {
    let mut r = pro();
    let files = tempfile::tempdir().expect("dir");
    let before = r.ctl.store().all_items().expect("items").len();
    let sparse = files.path().join("huge.bin");
    let f = fs::File::create(&sparse).expect("file");
    f.set_len(snipdeck_portable::Limits::default().max_entry_bytes + 1)
        .expect("len");
    let not_png = write(files.path(), "pic.png", b"definitely not a png");
    let cases = [
        (ItemKind::File, String::new(), EditorProblem::SourceEmpty),
        (
            ItemKind::File,
            files
                .path()
                .join("nope.txt")
                .to_str()
                .expect("utf8")
                .to_owned(),
            EditorProblem::SourceMissing,
        ),
        (
            ItemKind::File,
            files.path().to_str().expect("utf8").to_owned(),
            EditorProblem::SourceNotAFile,
        ),
        (
            ItemKind::File,
            sparse.to_str().expect("utf8").to_owned(),
            EditorProblem::SourceTooLarge { max_mb: 64 },
        ),
        (ItemKind::Image, not_png, EditorProblem::SourceNotPng),
    ];
    for (kind, path, problem) in cases {
        r.ctl.open_new_item(Origin::Main);
        let v = view(&r.ctl);
        r.ctl.item_editor_save(&EditorInput {
            kind,
            source_path: path,
            ..v.fields
        });
        assert_eq!(
            view(&r.ctl).source_problem,
            Some(problem.clone()),
            "{problem:?}"
        );
        assert!(r.ctl.item_editor_open(), "the editor stays open");
        r.ctl.item_editor_edited();
        assert_eq!(view(&r.ctl).source_problem, None, "typing clears it");
        r.ctl.item_editor_cancel();
    }
    assert_eq!(r.ctl.store().all_items().expect("items").len(), before);
}

// ---- Checks ------------------------------------------------------------------------------------

#[test]
fn fields_that_fail_the_checks_stay_open_with_the_reason() {
    let mut r = pro();
    r.ctl.open_new_item(Origin::Main);
    let v = view(&r.ctl);
    let save = |r: &mut Rig, kind, title: &str, content: &str| {
        let v = view(&r.ctl);
        r.ctl.item_editor_save(&EditorInput {
            kind,
            ..typed(&v, title, content)
        });
        view(&r.ctl)
    };
    assert_eq!(
        save(&mut r, ItemKind::Text, "", "  \n").content_problem,
        Some(EditorProblem::ContentEmpty)
    );
    assert_eq!(
        save(&mut r, ItemKind::Link, "", "example.com").content_problem,
        Some(EditorProblem::LinkNeedsScheme)
    );
    assert_eq!(
        save(&mut r, ItemKind::Link, "", "https://a b").content_problem,
        Some(EditorProblem::LinkHasSpace)
    );
    assert!(matches!(
        save(&mut r, ItemKind::Template, "", "Hello {nope}").content_problem,
        Some(EditorProblem::Template(_))
    ));
    assert_eq!(
        save(
            &mut r,
            ItemKind::Text,
            "",
            &"a".repeat(EDIT_LIMIT_BYTES + 1)
        )
        .content_problem,
        Some(EditorProblem::ContentTooLong { max_kb: 16 })
    );
    let both = save(&mut r, ItemKind::Text, &"t".repeat(201), "");
    assert_eq!(
        both.title_problem,
        Some(EditorProblem::TitleTooLong { max: 200 })
    );
    assert_eq!(
        both.content_problem,
        Some(EditorProblem::ContentEmpty),
        "every problem is reported at once"
    );
    assert_eq!(v.seq, both.seq, "still the same editor");
    assert_eq!(r.ctl.store().all_items().expect("items").len(), 11);
}

// ---- Free tier ---------------------------------------------------------------------------------

#[test]
fn a_free_user_cannot_add_prompted_fields_and_is_told_why() {
    let mut r = rig(Tier::Free);
    r.ctl.open_new_item(Origin::Main);
    let v = view(&r.ctl);
    assert!(!v.prompted_available);
    r.ctl.item_editor_save(&EditorInput {
        kind: ItemKind::Template,
        ..typed(&v, "Ask", "Dear {field:Name}")
    });
    assert!(r.ctl.item_editor_open());
    assert_eq!(
        view(&r.ctl).content_problem,
        Some(EditorProblem::NeedsPro(Feature::PromptedFields))
    );
    let t = r.ctl.view().toast.expect("toast");
    assert_eq!(
        (t.kind, t.message),
        (
            ToastKind::Danger,
            ToastMessage::NeedsPro(Feature::PromptedFields)
        )
    );
    // A choice is prompted too; the free placeholders are fine.
    r.ctl.item_editor_save(&EditorInput {
        kind: ItemKind::Template,
        ..typed(&view(&r.ctl), "Ask", "Size {choice:Size|S|M}")
    });
    assert!(r.ctl.item_editor_open());
    r.ctl.item_editor_save(&EditorInput {
        kind: ItemKind::Template,
        ..typed(&view(&r.ctl), "Dated", "Today is {date} at {time}")
    });
    assert!(!r.ctl.item_editor_open());
    // Switching an existing text item to a template that prompts is the same gate.
    let office = id_of(&r.ctl, "Office");
    r.ctl.open_item_editor(office);
    let v = view(&r.ctl);
    r.ctl.item_editor_save(&EditorInput {
        kind: ItemKind::Template,
        content: "Hi {field:Who}".into(),
        ..v.fields
    });
    assert!(r.ctl.item_editor_open());
    assert_eq!(
        r.ctl.store().item(office).expect("item").kind,
        ItemKind::Text,
        "nothing changed"
    );
}

#[test]
fn a_free_user_can_still_retitle_a_template_that_already_prompts() {
    let mut r = rig(Tier::Free);
    let id = id_of(&r.ctl, "Invoice reminder");
    r.ctl.open_item_editor(id);
    let v = view(&r.ctl);
    r.ctl.item_editor_save(&EditorInput {
        title: "Invoice nudge".into(),
        ..v.fields
    });
    assert!(!r.ctl.item_editor_open(), "saved");
    assert_eq!(r.ctl.store().item(id).expect("item").title, "Invoice nudge");
}

#[test]
fn a_free_user_may_mark_an_item_sensitive_but_nothing_is_encrypted() {
    let mut r = rig(Tier::Free);
    r.ctl.open_new_item(Origin::Main);
    let v = view(&r.ctl);
    assert!(!v.encryption_available);
    r.ctl.item_editor_save(&EditorInput {
        sensitive: true,
        ..typed(&v, "Pin", "1234")
    });
    assert!(!r.ctl.item_editor_open());
    let id = id_of(&r.ctl, "Pin");
    let item = r.ctl.store().item(id).expect("item");
    assert!(item.sensitive);
    assert_eq!(item.preview, "", "a sensitive item has no preview");
    assert!(
        !r.ctl.store().key_ring_exists().expect("ring"),
        "no key was made"
    );
}

// ---- Editing -----------------------------------------------------------------------------------

#[test]
fn editing_changes_the_text_title_list_and_tags_and_leaves_the_use_count() {
    let mut r = pro();
    let id = id_of(&r.ctl, "Office");
    r.ctl.use_item(id, snipdeck_core::UseMode::Copy);
    r.clock.set("2026-10-17T11:00:00Z".parse().expect("time"));
    r.ctl.open_item_editor(id);
    let v = view(&r.ctl);
    assert_eq!(
        v.tags.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        Vec::<&str>::new()
    );
    r.ctl.item_editor_tag_added("Work");
    r.ctl.item_editor_tag_added("work");
    assert_eq!(
        view(&r.ctl).tags,
        [("Work".to_owned(), true)],
        "one tag, any letter case"
    );
    r.ctl.item_editor_tag_added("  ");
    assert!(matches!(view(&r.ctl).banner, Some(EditorProblem::Tag(_))));
    r.ctl.item_editor_save(&EditorInput {
        title: "Perth office".into(),
        content: "Level 5".into(),
        list_index: 0,
        ..v.fields
    });
    assert!(!r.ctl.item_editor_open());
    let item = r.ctl.store().item(id).expect("item");
    assert_eq!(item.title, "Perth office");
    assert_eq!(item.preview, "Level 5");
    assert_eq!(item.use_count, 1, "editing does not touch the count");
    assert_eq!(
        item.updated_at,
        "2026-10-17T11:00:00Z".parse().expect("time")
    );
    assert_eq!(text_of(&r.ctl, id), "Level 5");
    let tag = r.ctl.store().tags().expect("tags");
    assert_eq!(tag.len(), 1);
    assert!(item.tags.contains(&tag[0].id), "the new tag is on the item");
    let replies = r.ctl.store().list(item.list_id).expect("list");
    assert_eq!(
        replies.name.as_str(),
        "Email replies",
        "moved to the chosen list"
    );
}

#[test]
fn a_new_tag_that_is_unticked_before_saving_is_never_made() {
    let mut r = pro();
    r.ctl.open_new_item(Origin::Main);
    r.ctl.item_editor_tag_added("temp");
    r.ctl.item_editor_tag_toggled(0);
    assert!(view(&r.ctl).tags.is_empty(), "forgotten");
    r.ctl.item_editor_tag_added("keep");
    let v = view(&r.ctl);
    r.ctl.item_editor_save(&typed(&v, "T", "x"));
    let tags: Vec<String> = r
        .ctl
        .store()
        .tags()
        .expect("tags")
        .iter()
        .map(|t| t.name.as_str().to_owned())
        .collect();
    assert_eq!(tags, ["keep"]);
}

#[test]
fn changing_only_the_title_keeps_the_other_formats_of_a_rich_item() {
    let mut r = pro();
    let list = r.ctl.store().all_items().expect("items")[0].list_id;
    let mut item = Item::new(list, "Rich".into(), ItemKind::Text, now());
    item.preview = "bold".into();
    let text = Content::Inline(Inline::new(b"bold".to_vec()).expect("inline"));
    let html = Content::Inline(Inline::new(b"<b>bold</b>".to_vec()).expect("inline"));
    let formats = Formats::new(vec![Format::Html(html), Format::Text(text)]).expect("formats");
    let mut store = Store::open_in_memory().expect("store");
    drop(r);
    // A fresh library with just that item.
    let board = snipdeck_core::Board::new(snipdeck_core::Name::new("B").expect("name"), now());
    snipdeck_store::BoardRepo::insert_board(&mut store, &board, Placement::Last).expect("board");
    let l = snipdeck_core::List::new(
        board.id,
        snipdeck_core::Name::new("L").expect("name"),
        now(),
    );
    store.insert_list(&l, Placement::Last).expect("list");
    item.list_id = l.id;
    store
        .insert_item(&item, &formats, Placement::Last)
        .expect("item");
    let dir = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    let (mut ctl, _) = controller(store, Tier::Pro, dir.path(), &provider, true);
    ctl.open_item_editor(item.id);
    let v = view(&ctl);
    ctl.item_editor_save(&EditorInput {
        title: "Rich renamed".into(),
        ..v.fields
    });
    assert!(!ctl.item_editor_open());
    let kinds: Vec<FormatKind> = ctl
        .store()
        .formats(item.id)
        .expect("formats")
        .iter()
        .map(Format::kind)
        .collect();
    assert_eq!(kinds, [FormatKind::Html, FormatKind::Text], "untouched");
}

#[test]
fn deleting_from_the_editor_moves_the_item_to_the_trash_with_undo() {
    let mut r = pro();
    let id = id_of(&r.ctl, "Office");
    r.ctl.open_item_editor(id);
    r.ctl.item_editor_delete();
    assert!(!r.ctl.item_editor_open());
    assert!(r.ctl.store().item(id).is_err(), "no longer live");
    let t = r.ctl.view().toast.expect("toast");
    assert!(t.undo);
    assert_eq!(t.message, ToastMessage::ItemsTrashed { count: 1 });
    r.ctl.main_undo();
    assert!(r.ctl.store().item(id).is_ok(), "Undo brought it back");
    // A new item has no Delete.
    r.ctl.open_new_item(Origin::Main);
    r.ctl.item_editor_delete();
    assert!(r.ctl.item_editor_open());
}

#[test]
fn a_write_that_fails_says_what_failed_and_changes_nothing() {
    let mut r = pro();
    let id = id_of(&r.ctl, "Office");
    r.ctl.open_item_editor(id);
    let v = view(&r.ctl);
    // The item is deleted from under the editor (here by the Main window's Delete).
    let screen = r.ctl.main_screen();
    let row = screen
        .rows
        .iter()
        .position(|row| row.title == "Office")
        .expect("row");
    r.ctl.main_action(MainAction::Delete, row);
    assert!(r.ctl.store().item(id).is_err());
    r.ctl.item_editor_save(&EditorInput {
        title: "Late".into(),
        ..v.fields
    });
    assert!(
        r.ctl.item_editor_open(),
        "the editor stays so nothing typed is lost"
    );
    assert_eq!(
        view(&r.ctl).banner,
        Some(EditorProblem::Save(SaveFailure::ItemGone))
    );
    assert_eq!(toast(&r.ctl), ToastMessage::ItemGone);
}

// ---- Sensitive items and the library key -------------------------------------------------------

/// A controller over the demo library kept on disk in `data` (seeded the first time), so a second
/// controller on the same folder is a restart.
fn on_disk(tier: Tier, provider: &SharedProvider, with_keys: bool, data: &Path) -> Ctl {
    let mut store = Store::open(data).expect("store");
    if snipdeck_store::BoardRepo::boards(&store)
        .expect("boards")
        .is_empty()
    {
        demo::seed(&mut store, now()).expect("seed");
    }
    controller(store, tier, data, provider, with_keys).0
}

const SECRET: &str = "ZQ9-correct-horse-battery";

fn save_sensitive(ctl: &mut Ctl, title: &str, secret: &str) -> ItemId {
    ctl.open_new_item(Origin::Main);
    let v = view(ctl);
    ctl.item_editor_save(&EditorInput {
        sensitive: true,
        ..typed(&v, title, secret)
    });
    assert!(
        !ctl.item_editor_open(),
        "saved: {:?}",
        ctl.item_editor_view().map(|v| v.banner)
    );
    id_of(ctl, title)
}

#[test]
fn the_first_sensitive_item_makes_the_key_silently_and_is_stored_encrypted() {
    let data = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    let mut ctl = on_disk(Tier::Pro, &provider, true, data.path());
    assert!(!ctl.store().key_ring_exists().expect("ring"));
    let id = save_sensitive(&mut ctl, "Vault code", SECRET);
    assert!(
        ctl.store().key_ring_exists().expect("ring"),
        "made on first use"
    );
    assert!(ctl.store().is_unlocked());
    let item = ctl.store().item(id).expect("item");
    assert!(item.sensitive);
    assert_eq!(item.preview, "");
    assert_eq!(text_of(&ctl, id), SECRET, "reads back while unlocked");
    // The database, its log and the drafts folder never saw the text.
    assert_no_plaintext(data.path(), &[SECRET.as_bytes()]);
    // The toast is the ordinary one: no prompt, no mention of keys.
    assert_eq!(
        toast(&ctl),
        ToastMessage::Saved {
            list: "Email replies".into()
        }
    );
}

#[test]
fn a_sensitive_item_survives_a_restart_and_edits_back_to_the_same_text() {
    let data = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    let id = {
        let mut ctl = on_disk(Tier::Pro, &provider, true, data.path());
        save_sensitive(&mut ctl, "Vault code", SECRET)
    };
    // A new run: the key is opened at start-up, then the item opens decrypted in the editor.
    let mut ctl = on_disk(Tier::Pro, &provider, true, data.path());
    assert!(!ctl.store().is_unlocked());
    ctl.unlock_library();
    assert!(ctl.store().is_unlocked());
    ctl.open_item_editor(id);
    let v = view(&ctl);
    assert!(v.fields.sensitive);
    assert_eq!(v.fields.content, SECRET);
    // Editing only the title keeps the content encrypted; a new text is encrypted too.
    ctl.item_editor_save(&EditorInput {
        title: "Vault".into(),
        ..v.fields.clone()
    });
    ctl.open_item_editor(id);
    let v = view(&ctl);
    ctl.item_editor_save(&EditorInput {
        content: "second-SECRET-9X".into(),
        ..v.fields
    });
    assert_eq!(text_of(&ctl, id), "second-SECRET-9X");
    assert_no_plaintext(data.path(), &[SECRET.as_bytes(), b"second-SECRET-9X"]);
}

#[test]
fn opening_a_sensitive_item_opens_the_key_even_without_the_start_up_unlock() {
    let data = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    let id = {
        let mut ctl = on_disk(Tier::Pro, &provider, true, data.path());
        save_sensitive(&mut ctl, "Vault code", SECRET)
    };
    let mut ctl = on_disk(Tier::Pro, &provider, true, data.path());
    ctl.open_item_editor(id);
    assert_eq!(view(&ctl).fields.content, SECRET);
}

#[test]
fn marking_an_existing_item_sensitive_encrypts_it_and_clearing_the_mark_decrypts_it() {
    let data = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    let mut ctl = on_disk(Tier::Pro, &provider, true, data.path());
    let id = save_new(&mut ctl, ItemKind::Text, "Later secret", "plain-NEEDLE-77");
    assert_eq!(
        ctl.store().item(id).expect("item").preview,
        "plain-NEEDLE-77"
    );
    ctl.open_item_editor(id);
    let v = view(&ctl);
    ctl.item_editor_save(&EditorInput {
        sensitive: true,
        ..v.fields
    });
    let item = ctl.store().item(id).expect("item");
    assert!(item.sensitive);
    assert_eq!(item.preview, "", "the preview goes with the mark");
    assert_eq!(text_of(&ctl, id), "plain-NEEDLE-77");
    assert_no_plaintext(data.path(), &[b"plain-NEEDLE-77"]);

    ctl.open_item_editor(id);
    let v = view(&ctl);
    ctl.item_editor_save(&EditorInput {
        sensitive: false,
        ..v.fields
    });
    let item = ctl.store().item(id).expect("item");
    assert!(!item.sensitive);
    assert_eq!(item.preview, "plain-NEEDLE-77", "the preview comes back");
    assert_eq!(text_of(&ctl, id), "plain-NEEDLE-77");
}

#[test]
fn without_key_protection_a_sensitive_item_is_refused_not_stored_in_the_clear() {
    let data = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    let mut ctl = on_disk(Tier::Pro, &provider, false, data.path());
    let before = ctl.store().all_items().expect("items").len();
    ctl.open_new_item(Origin::Main);
    let v = view(&ctl);
    ctl.item_editor_save(&EditorInput {
        sensitive: true,
        ..typed(&v, "Nope", SECRET)
    });
    assert!(ctl.item_editor_open());
    assert_eq!(
        view(&ctl).banner,
        Some(EditorProblem::Save(SaveFailure::KeyUnavailable))
    );
    assert_eq!(toast(&ctl), ToastMessage::KeyUnavailable);
    assert_eq!(ctl.store().all_items().expect("items").len(), before);
    assert!(!ctl.store().key_ring_exists().expect("ring"));
    assert_no_plaintext(data.path(), &[SECRET.as_bytes()]);
}

#[test]
fn a_key_that_will_not_open_is_a_toast_at_start_and_when_opening() {
    let data = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    let id = {
        let mut ctl = on_disk(Tier::Pro, &provider, true, data.path());
        save_sensitive(&mut ctl, "Vault code", SECRET)
    };
    // Another Windows user's protection cannot open the key.
    let stranger = SharedProvider::new();
    let mut ctl = on_disk(Tier::Pro, &stranger, true, data.path());
    ctl.unlock_library();
    assert_eq!(toast(&ctl), ToastMessage::KeyUnavailable);
    ctl.open_item_editor(id);
    assert!(
        !ctl.item_editor_open(),
        "no editor over text that cannot be read"
    );
    assert_eq!(toast(&ctl), ToastMessage::KeyUnavailable);
}

// ---- Large text --------------------------------------------------------------------------------

#[test]
fn text_over_16_kb_opens_read_only_and_saving_never_rewrites_it() {
    let mut r = pro();
    let big = format!("{}{}", "line of text\n".repeat(2000), "THE-END");
    assert!(big.len() > EDIT_LIMIT_BYTES);
    let list = r.ctl.store().all_items().expect("items")[0].list_id;
    let mut item = Item::new(list, "Big".into(), ItemKind::Text, now());
    item.preview = Item::preview_of(&big, false);
    // Written through the store, as capture does: the editor cannot make this itself.
    let (mut store, _) = {
        let mut s = Store::open_in_memory().expect("store");
        let board = snipdeck_core::Board::new(snipdeck_core::Name::new("B").expect("name"), now());
        snipdeck_store::BoardRepo::insert_board(&mut s, &board, Placement::Last).expect("board");
        let l = snipdeck_core::List::new(
            board.id,
            snipdeck_core::Name::new("L").expect("name"),
            now(),
        );
        s.insert_list(&l, Placement::Last).expect("list");
        item.list_id = l.id;
        (s, l)
    };
    let content = store.put(big.clone().into_bytes()).expect("put");
    store
        .insert_item(
            &item,
            &Formats::new(vec![Format::Text(content)]).expect("formats"),
            Placement::Last,
        )
        .expect("insert");
    let dir = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    let (mut ctl, _) = controller(store, Tier::Pro, dir.path(), &provider, true);
    ctl.open_item_editor(item.id);
    let v = view(&ctl);
    assert_eq!(v.large, Some(big.len() as u64), "read-only, with its size");
    assert_eq!(
        v.fields.content.len(),
        EDIT_LIMIT_BYTES,
        "the start is shown"
    );
    assert!(big.starts_with(&v.fields.content));
    // Only the title changes; the text on disk stays whole.
    ctl.item_editor_save(&EditorInput {
        title: "Big, renamed".into(),
        ..v.fields
    });
    assert!(!ctl.item_editor_open());
    assert_eq!(
        ctl.store().item(item.id).expect("item").title,
        "Big, renamed"
    );
    assert_eq!(text_of(&ctl, item.id), big);
    drop(r);
}

#[test]
fn a_large_text_gets_no_draft() {
    let mut store = Store::open_in_memory().expect("store");
    let board = snipdeck_core::Board::new(snipdeck_core::Name::new("B").expect("name"), now());
    snipdeck_store::BoardRepo::insert_board(&mut store, &board, Placement::Last).expect("board");
    let l = snipdeck_core::List::new(
        board.id,
        snipdeck_core::Name::new("L").expect("name"),
        now(),
    );
    store.insert_list(&l, Placement::Last).expect("list");
    let big = "x".repeat(EDIT_LIMIT_BYTES + 5);
    let item = Item::new(l.id, "Big".into(), ItemKind::Text, now());
    let content = store.put(big.into_bytes()).expect("put");
    store
        .insert_item(
            &item,
            &Formats::new(vec![Format::Text(content)]).expect("formats"),
            Placement::Last,
        )
        .expect("insert");
    let dir = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    let (mut ctl, _) = controller(store, Tier::Pro, dir.path(), &provider, true);
    ctl.open_item_editor(item.id);
    let v = view(&ctl);
    assert!(!ctl.item_editor_autosave(&EditorInput {
        title: "typed".into(),
        ..v.fields
    }));
    assert!(!dir.path().join("drafts").exists());
}

// ---- Drafts ------------------------------------------------------------------------------------

fn draft_files(dir: &Path) -> Vec<String> {
    let folder = dir.join("drafts");
    let Ok(read) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut names: Vec<String> = read
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_draft_is_written_when_something_changed_and_not_before() {
    let mut r = pro();
    r.ctl.open_new_item(Origin::Main);
    let v = view(&r.ctl);
    r.ctl.item_editor_autosave(&v.fields);
    assert_eq!(
        draft_files(r.dir.path()),
        Vec::<String>::new(),
        "nothing typed, no draft"
    );
    let input = typed(&v, "Half", "half a thought");
    assert!(!r.ctl.item_editor_autosave(&input));
    assert_eq!(draft_files(r.dir.path()), ["new.json"]);
    // Typing it back to the start removes the draft again.
    r.ctl.item_editor_autosave(&v.fields);
    assert_eq!(draft_files(r.dir.path()), Vec::<String>::new());
}

#[test]
fn a_draft_comes_back_when_the_editor_is_opened_again() {
    let data = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    {
        // The app is killed with the editor open: nothing but the draft survives.
        let mut ctl = on_disk(Tier::Pro, &provider, true, data.path());
        ctl.open_new_item(Origin::Main);
        ctl.item_editor_tag_added("later");
        let v = view(&ctl);
        ctl.item_editor_autosave(&EditorInput {
            kind: ItemKind::Code,
            list_index: 2,
            ..typed(&v, "Unsaved", "let x = 1;")
        });
    }
    let mut ctl = on_disk(Tier::Pro, &provider, true, data.path());
    ctl.open_new_item(Origin::Main);
    let v = view(&ctl);
    assert!(v.restored);
    assert_eq!(v.fields.kind, ItemKind::Code);
    assert_eq!(
        (v.fields.title.as_str(), v.fields.content.as_str()),
        ("Unsaved", "let x = 1;")
    );
    assert_eq!(v.list_names[v.fields.list_index], "Code");
    assert_eq!(v.tags, [("later".to_owned(), true)]);
    // Saving uses it and removes the draft; a later editor starts clean.
    ctl.item_editor_save(&v.fields);
    assert!(!ctl.item_editor_open());
    assert_eq!(draft_files(data.path()), Vec::<String>::new());
    ctl.open_new_item(Origin::Main);
    assert!(!view(&ctl).restored);
}

#[test]
fn discarding_removes_the_draft_and_an_edit_draft_belongs_to_its_item() {
    let mut r = pro();
    let id = id_of(&r.ctl, "Office");
    r.ctl.open_item_editor(id);
    let v = view(&r.ctl);
    r.ctl.item_editor_autosave(&EditorInput {
        content: "Level 9 (unsaved)".into(),
        ..v.fields.clone()
    });
    assert_eq!(draft_files(r.dir.path()), [format!("{id}.json")]);
    r.ctl.item_editor_cancel();
    assert_eq!(
        draft_files(r.dir.path()),
        Vec::<String>::new(),
        "Discard drops it"
    );
    assert_eq!(
        text_of(&r.ctl, id),
        "Level 4, 100 St Georges Terrace, Perth WA 6000"
    );

    // Autosaved, then the app dies: reopening that item restores it.
    r.ctl.open_item_editor(id);
    r.ctl.item_editor_autosave(&EditorInput {
        content: "Level 9 (unsaved)".into(),
        ..v.fields
    });
    let data = r.dir.path().to_owned();
    r.ctl.item_editor_cancel();
    // (Cancel removed it; write it back the way a crash would have left it.)
    let drafts = Drafts::in_data_dir(&data);
    let stamp = r.ctl.store().item(id).expect("item").updated_at.to_string();
    let draft = Draft::new(
        Some(stamp),
        ItemKind::Text,
        "Office".into(),
        "Level 9 (unsaved)".into(),
        r.ctl.store().item(id).expect("item").list_id.to_string(),
        Vec::new(),
        Vec::new(),
        String::new(),
    );
    drafts.save(DraftKey::Item(id), &draft).expect("save");
    r.ctl.open_item_editor(id);
    let v = view(&r.ctl);
    assert!(v.restored);
    assert_eq!(v.fields.content, "Level 9 (unsaved)");
}

#[test]
fn a_draft_of_an_item_that_changed_since_is_stale_and_dropped() {
    let mut r = pro();
    let id = id_of(&r.ctl, "Office");
    let drafts = Drafts::in_data_dir(r.dir.path());
    let list = r.ctl.store().item(id).expect("item").list_id;
    let old = Draft::new(
        Some("2020-01-01T00:00:00Z".into()),
        ItemKind::Text,
        "Old".into(),
        "old".into(),
        list.to_string(),
        Vec::new(),
        Vec::new(),
        String::new(),
    );
    drafts.save(DraftKey::Item(id), &old).expect("save");
    r.ctl.open_item_editor(id);
    let v = view(&r.ctl);
    assert!(!v.restored);
    assert_eq!(v.fields.title, "Office");
    assert_eq!(draft_files(r.dir.path()), Vec::<String>::new());
}

#[test]
fn a_sensitive_item_never_has_a_draft_and_the_mark_removes_one_already_written() {
    let mut r = pro();
    r.ctl.open_new_item(Origin::Main);
    let v = view(&r.ctl);
    r.ctl
        .item_editor_autosave(&typed(&v, "Maybe", "ordinary text"));
    assert_eq!(draft_files(r.dir.path()), ["new.json"]);
    // The user turns "Sensitive item" on: the draft goes, and none is written while it is on.
    let secret = EditorInput {
        sensitive: true,
        ..typed(&v, "Maybe", "NEEDLE-in-a-draft")
    };
    r.ctl.item_editor_autosave(&secret);
    assert_eq!(draft_files(r.dir.path()), Vec::<String>::new());
    r.ctl.item_editor_autosave(&secret);
    assert_no_plaintext(r.dir.path(), &[b"NEEDLE-in-a-draft"]);
    // An existing sensitive item is never drafted, even with the mark turned off.
    r.ctl.item_editor_cancel();
    let server = id_of(&r.ctl, "Server password");
    r.ctl.open_item_editor(server);
    let v = view(&r.ctl);
    r.ctl.item_editor_autosave(&EditorInput {
        content: "changed-secret".into(),
        sensitive: false,
        ..v.fields
    });
    assert_eq!(draft_files(r.dir.path()), Vec::<String>::new());
}

#[test]
fn a_draft_that_cannot_be_written_is_reported_once() {
    let mut r = pro();
    // `drafts` is a file, so the folder cannot be made.
    fs::write(r.dir.path().join("drafts"), b"in the way").expect("write");
    r.ctl.open_new_item(Origin::Main);
    let v = view(&r.ctl);
    assert!(r.ctl.item_editor_autosave(&typed(&v, "A", "a")), "reported");
    assert_eq!(toast(&r.ctl), ToastMessage::DraftNotSaved);
    assert_eq!(view(&r.ctl).banner, Some(EditorProblem::DraftNotSaved));
    assert!(
        !r.ctl.item_editor_autosave(&typed(&v, "AB", "ab")),
        "not again"
    );
}

// ---- The file dialog and the crash toast -------------------------------------------------------

#[test]
fn browse_hands_the_chosen_path_to_the_field_and_reports_a_dialog_that_failed() {
    let mut r = pro();
    assert!(r.ctl.file_picker().is_none());
    let picker = FakeFilePicker::answering(Ok(Some(r"C:\files\a.png".into())));
    picker.push(Ok(None));
    picker.push(Err(snipdeck_platform::error::Error::Os {
        op: "GetOpenFileNameW",
        code: 7,
    }));
    r.ctl.set_file_picker(Box::new(picker.clone()));
    r.ctl.open_new_item(Origin::Main);
    assert!(view(&r.ctl).can_browse);
    let ask = |r: &mut Rig| {
        let p = r.ctl.file_picker().expect("picker");
        let answer = p.pick(PickKind::Image, Some(42));
        r.ctl.item_editor_picked(answer)
    };
    assert_eq!(ask(&mut r).as_deref(), Some(r"C:\files\a.png"));
    assert_eq!(ask(&mut r), None, "cancelled");
    assert_eq!(view(&r.ctl).banner, None);
    assert_eq!(ask(&mut r), None);
    assert_eq!(view(&r.ctl).banner, Some(EditorProblem::BrowseFailed));
    assert_eq!(picker.asked().len(), 3);
}

#[test]
fn the_crash_toast_opens_the_log_folder_and_says_when_it_cannot() {
    let mut r = pro();
    let opened = Rc::new(RefCell::new(0));
    let count = Rc::clone(&opened);
    r.ctl.set_log_opener(Box::new(move || {
        *count.borrow_mut() += 1;
        Ok(())
    }));
    r.ctl.show_toast(snipdeck_app::toast::Toast {
        kind: ToastKind::Neutral,
        message: ToastMessage::CrashedLastTime,
    });
    let t = r.ctl.view().toast.expect("toast");
    assert!(t.open_log && !t.undo);
    r.ctl.toast_action();
    assert_eq!(*opened.borrow(), 1);
    assert!(r.ctl.view().toast.is_none(), "the toast closes");

    // A folder that cannot be opened is said so.
    r.ctl
        .set_log_opener(Box::new(|| Err(std::io::Error::other("no explorer"))));
    r.ctl.show_toast(snipdeck_app::toast::Toast {
        kind: ToastKind::Neutral,
        message: ToastMessage::CrashedLastTime,
    });
    r.ctl.toast_action();
    assert_eq!(toast(&r.ctl), ToastMessage::CannotOpenLog);

    // Any other toast's button is still Undo.
    let id = id_of(&r.ctl, "Office");
    r.ctl.open_item_editor(id);
    r.ctl.item_editor_delete();
    r.ctl.toast_action();
    assert!(
        r.ctl.store().item(id).is_ok(),
        "restored by the same button"
    );
}
