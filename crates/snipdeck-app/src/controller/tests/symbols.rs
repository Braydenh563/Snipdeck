//! The Symbols controller: use, add to a list, copy an icon, save it, and the recents.

use std::path::PathBuf;

use snipdeck_core::{ItemKind, UseMode, UseOutcome};
use snipdeck_platform::file_dialog::{FakeFilePicker, SaveKind};
use snipdeck_symbols::{Catalog, Kind, SymbolId};

use super::*;
use crate::controller::symbol_ctl::{IconExport, SaveOutcome};

fn emoji() -> SymbolId {
    SymbolId::Emoji("1f600".into())
}

fn first_of(kind: Kind) -> SymbolId {
    let c = Catalog::get();
    c.symbols()[c.range(kind).start].id.clone()
}

fn written_text(w: &World, n: usize) -> String {
    match &w.ctl.services().clipboard.written[n].data()[0] {
        Data::Text(t) => t.clone(),
        other => panic!("not text: {other:?}"),
    }
}

fn with_picker(w: &mut World, answer: Option<PathBuf>) -> FakeFilePicker {
    let picker = FakeFilePicker::default();
    picker.push_save(Ok(answer));
    w.ctl.set_file_picker(Box::new(picker.clone()));
    picker
}

#[test]
fn a_click_with_a_target_pastes_the_emoji_text_and_remembers_it() {
    let mut w = world();
    let result = w.ctl.use_symbol(&emoji(), UseMode::Paste);
    assert_eq!(result, UseResult::Done(UseOutcome::Pasted));
    let services = w.ctl.services();
    assert_eq!(
        services.clipboard.written[0].data(),
        [Data::Text("\u{1F600}".into())]
    );
    assert_eq!(services.keyboard.sent.len(), 1, "Ctrl+V went out");
    assert!(services.timing.slept.contains(&REFOCUS_WAIT));
    assert_eq!(w.targets.refocus_count(), 1);
    assert_eq!(
        w.ctl.symbol_recents(None),
        [emoji()],
        "the use is remembered"
    );
}

#[test]
fn shift_copies_and_no_target_copies_without_pressing_keys() {
    let mut w = world();
    assert_eq!(
        w.ctl.use_symbol(&emoji(), UseMode::Copy),
        UseResult::Done(UseOutcome::Copied)
    );
    assert_eq!(w.ctl.services().keyboard.sent.len(), 0);
    assert_eq!(w.targets.refocus_count(), 0);

    let mut w = world_with(None, Settings::default());
    assert_eq!(
        w.ctl.use_symbol(&emoji(), UseMode::Paste),
        UseResult::Done(UseOutcome::Copied)
    );
    assert_eq!(w.ctl.services().keyboard.sent.len(), 0);
    assert_eq!(written_text(&w, 0), "\u{1F600}");
}

#[test]
fn using_a_symbol_does_not_touch_any_item_or_its_use_count() {
    let mut w = world();
    w.ctl.use_symbol(&emoji(), UseMode::Paste);
    for id in w.ids.clone() {
        assert_eq!(uses(&w, id), 0);
    }
}

#[test]
fn an_icon_is_not_text_so_using_it_fails_and_remembers_nothing() {
    let mut w = world();
    let icon = first_of(Kind::Icon);
    assert_eq!(w.ctl.use_symbol(&icon, UseMode::Paste), UseResult::Failed);
    assert_eq!(w.ctl.services().clipboard.written.len(), 0);
    assert_eq!(w.ctl.symbol_recents(None), Vec::<SymbolId>::new());
}

#[test]
fn a_busy_clipboard_toasts_and_remembers_nothing() {
    let mut w = build(Opts {
        clipboard_busy: true,
        ..Opts::default()
    });
    assert_eq!(w.ctl.use_symbol(&emoji(), UseMode::Copy), UseResult::Failed);
    assert_eq!(w.ctl.symbol_recents(None), Vec::<SymbolId>::new());
}

#[test]
fn add_to_list_creates_a_text_item_at_the_end_of_the_shown_list() {
    let mut w = world();
    assert!(
        w.ctl
            .add_symbol_to_list(&emoji(), crate::item_editor::Origin::Panel)
    );
    let view = w.ctl.view();
    assert_eq!(view.rows.len(), 5);
    assert_eq!(view.rows[4].title, "\u{1F600}");
    assert_eq!(view.rows[4].kind, ItemKind::Text);
    assert_eq!(
        view.toast.expect("toast").message,
        ToastMessage::Saved {
            list: "Replies".into()
        }
    );
    assert_eq!(w.ctl.symbol_recents(None), [emoji()]);
}

#[test]
fn add_to_list_with_no_list_says_so_and_adds_nothing() {
    let now = at("2026-10-17T09:00:00Z");
    let mut store = Store::open_in_memory().expect("store");
    let board = snipdeck_core::Board::new(snipdeck_core::Name::new("Empty").expect("n"), now);
    store
        .insert_board(&board, snipdeck_core::Placement::Last)
        .expect("board");
    let mut ctl = Controller::new(
        store,
        Services {
            clipboard: FakeClipboard::default(),
            keyboard: FakeKeyboard::default(),
            timing: FakeTiming::default(),
        },
        Box::new(FakeTargets::new(None)),
        Box::new(FixedClock::new(now)),
        Settings::default(),
        snipdeck_core::Entitlements::new(snipdeck_core::Tier::Pro),
    )
    .expect("controller");
    assert!(!ctl.add_symbol_to_list(&emoji(), crate::item_editor::Origin::Panel));
    assert_eq!(
        ctl.view().toast.expect("toast").message,
        ToastMessage::NoListToSave
    );
    assert_eq!(ctl.symbol_recents(None), Vec::<SymbolId>::new());
}

#[test]
fn copy_svg_writes_the_icon_markup_with_current_colour() {
    let mut w = world();
    let icon = first_of(Kind::Icon);
    assert!(w.ctl.copy_icon_svg(&icon));
    let text = written_text(&w, 0);
    assert!(text.starts_with("<svg"), "{text}");
    assert!(text.contains(r#"fill="currentColor""#));
    assert_eq!(w.ctl.symbol_recents(Some(Kind::Icon)), [icon]);
}

#[test]
fn copy_png_writes_a_decodable_64_pixel_png_in_the_given_colour() {
    let mut w = world();
    let icon = first_of(Kind::Icon);
    assert!(w.ctl.copy_icon_png(&icon, [200, 10, 10, 255]));
    let Data::Png(bytes) = &w.ctl.services().clipboard.written[0].data()[0] else {
        panic!("not a PNG")
    };
    let (info, pixels) = decode(bytes);
    assert_eq!((info.0, info.1), (64, 64));
    assert!(
        pixels
            .chunks(4)
            .any(|p| p[3] > 0 && p[0] == 200 && p[1] == 10),
        "some opaque pixel has the colour"
    );
}

fn decode(bytes: &[u8]) -> ((u32, u32), Vec<u8>) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .expect("png");
    let mut buf = vec![0; reader.output_buffer_size().expect("size")];
    let info = reader.next_frame(&mut buf).expect("frame");
    buf.truncate(info.buffer_size());
    ((info.width, info.height), buf)
}

#[test]
fn copying_an_emoji_as_an_icon_does_nothing() {
    let mut w = world();
    assert!(!w.ctl.copy_icon_svg(&emoji()));
    assert!(!w.ctl.copy_icon_png(&emoji(), [0, 0, 0, 255]));
    assert_eq!(w.ctl.services().clipboard.written.len(), 0);
}

#[test]
fn save_as_writes_the_file_the_picker_returns_and_suggests_the_icon_name() {
    let dir = tempfile::tempdir().expect("dir");
    let target = dir.path().join("out.svg");
    let mut w = world();
    let picker = with_picker(&mut w, Some(target.clone()));
    let icon = first_of(Kind::Icon);
    let SymbolId::Icon(name) = icon.clone() else {
        panic!("not an icon")
    };
    let result = w.ctl.save_icon(&icon, IconExport::Svg, Some(9));
    assert_eq!(result, SaveOutcome::Saved(target.clone()));
    let text = std::fs::read_to_string(&target).expect("file");
    assert!(text.starts_with("<svg"));
    assert_eq!(picker.asked_save(), [(SaveKind::Svg, name, Some(9))]);
    assert_eq!(w.ctl.symbol_recents(None), [icon]);
}

#[test]
fn save_as_png_writes_a_png_of_the_chosen_size() {
    let dir = tempfile::tempdir().expect("dir");
    let target = dir.path().join("out.png");
    let mut w = world();
    let picker = with_picker(&mut w, Some(target.clone()));
    let icon = first_of(Kind::Icon);
    let result = w.ctl.save_icon(
        &icon,
        IconExport::Png {
            px: 128,
            rgba: [200, 10, 10, 255],
        },
        None,
    );
    assert_eq!(result, SaveOutcome::Saved(target.clone()));
    let (size, pixels) = decode(&std::fs::read(&target).expect("file"));
    assert_eq!(size, (128, 128));
    assert!(
        pixels
            .chunks(4)
            .any(|p| p[3] > 0 && p[0] == 200 && p[1] == 10),
        "some opaque pixel has the given text colour, not a fixed one"
    );
    assert_eq!(picker.asked_save()[0].0, SaveKind::Png);
}

#[test]
fn cancelling_the_dialog_writes_nothing_and_remembers_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut w = world();
    with_picker(&mut w, None);
    let icon = first_of(Kind::Icon);
    assert_eq!(
        w.ctl.save_icon(&icon, IconExport::Svg, None),
        SaveOutcome::Cancelled
    );
    assert_eq!(std::fs::read_dir(dir.path()).expect("dir").count(), 0);
    assert_eq!(w.ctl.symbol_recents(None), Vec::<SymbolId>::new());
}

#[test]
fn a_path_that_cannot_be_written_is_a_failure_with_a_toast() {
    let dir = tempfile::tempdir().expect("dir");
    let mut w = world();
    with_picker(
        &mut w,
        Some(dir.path().join("no-such-folder").join("x.svg")),
    );
    let icon = first_of(Kind::Icon);
    assert_eq!(
        w.ctl.save_icon(&icon, IconExport::Svg, None),
        SaveOutcome::Failed
    );
    assert!(w.ctl.view().toast.is_some());
}

#[test]
fn without_a_file_dialog_save_as_is_unavailable() {
    let mut w = world();
    let icon = first_of(Kind::Icon);
    assert_eq!(
        w.ctl.save_icon(&icon, IconExport::Svg, None),
        SaveOutcome::Unavailable
    );
}

#[test]
fn recents_are_newest_first_stamped_by_the_injected_clock_and_kept_in_settings() {
    let mut w = world();
    let a = emoji();
    let b = SymbolId::Emoji("1f601".into());
    w.ctl.use_symbol(&a, UseMode::Copy);
    w.clock.set(at("2026-10-17T10:31:00Z"));
    w.ctl.use_symbol(&b, UseMode::Copy);
    w.clock.set(at("2026-10-17T10:32:00Z"));
    w.ctl.use_symbol(&a, UseMode::Copy);
    assert_eq!(w.ctl.symbol_recents(None), [a.clone(), b.clone()]);
    let saved = &w.ctl.settings().symbols_recent;
    assert_eq!(saved[0].id, a.key());
    assert_eq!(
        saved[0].used_at,
        at("2026-10-17T10:32:00Z").as_millisecond()
    );
    assert_eq!(
        saved[1].used_at,
        at("2026-10-17T10:31:00Z").as_millisecond()
    );
}

#[test]
fn recents_round_trip_through_the_settings_file_and_drop_unknown_ids() {
    let mut w = world();
    w.ctl.use_symbol(&emoji(), UseMode::Copy);
    let json = serde_json::to_string(w.ctl.settings()).expect("json");
    let back = Settings::from_json(&json);
    assert_eq!(back.symbols_recent, w.ctl.settings().symbols_recent);

    let w2 = world_with(
        Some(outlook()),
        Settings {
            symbols_recent: vec![
                snipdeck_symbols::RecentEntry {
                    id: "emoji:ffffff-not-real".into(),
                    used_at: 5,
                },
                snipdeck_symbols::RecentEntry {
                    id: emoji().key(),
                    used_at: 4,
                },
            ],
            ..Settings::default()
        },
    );
    assert_eq!(w2.ctl.symbol_recents(None), [emoji()]);
}

/// Snipdeck's own clipboard writes are marked as own by the clipboard owner window (every write
/// through the `Clipboard` trait, `clipboard_watch::ClipboardChange::own`), so the Recent inbox
/// skips them. Symbols must use the same write calls as a snippet, not a path of their own.
#[test]
fn symbol_pastes_and_icon_copies_write_the_clipboard_like_a_snippet_does() {
    let mut w = world();
    let item = w.ids[0];
    w.ctl.use_item(item, UseMode::Paste);
    let snippet_log = w.ctl.services().clipboard.log.clone();
    let mut w = world();
    w.ctl.use_symbol(&emoji(), UseMode::Paste);
    let strip = |log: &[String]| -> Vec<String> {
        log.iter()
            .map(|l| l.split(':').next().unwrap_or("").to_owned())
            .collect()
    };
    assert_eq!(strip(&w.ctl.services().clipboard.log), strip(&snippet_log));

    let mut w = world();
    w.ctl.use_symbol(&emoji(), UseMode::Copy);
    assert_eq!(strip(&w.ctl.services().clipboard.log), ["persistent"]);
    let mut w = world();
    assert!(w.ctl.copy_icon_svg(&first_of(Kind::Icon)));
    assert_eq!(strip(&w.ctl.services().clipboard.log), ["persistent"]);
}
