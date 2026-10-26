//! The controller's behaviour tests: shared helpers here, one file per area below.
//!
//! Only `panel_select` is mounted: the other area files (split out by the D5 WIP) do not compile yet
//! (they lack imports) and are left as they were until that work is finished.
#![allow(
    dead_code,
    unused_imports,
    reason = "helpers and imports shared with the area files that are not mounted yet"
)]

use super::*;
use crate::host::{FakeTargets, FixedClock};
use crate::{
    frame::{Edge, Rect},
    panel::{Footer, UseLabel},
    picker::Scope,
};
use snipdeck_core::{
    Board, Content, Format, Formats, Inline, Item, ItemKind, List, Name, Placement, Tier, Timestamp,
};
use snipdeck_core::{Feature, UseMode, UseOutcome};
use snipdeck_platform::system_look::HighContrast;
use snipdeck_platform::{
    keys::{Key, KeyEvent},
    payload::Data,
    route::{Focus, Target},
    testing::{FakeClipboard, FakeKeyboard, FakeTiming},
};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, RecentRepo, Store, TrashRepo};

use look_settings::saving_world;
use menus_frame::display;
use picker::pasted;
use tray::with_tray;
use use_paste::make_sensitive;

mod capture_stack;
mod dock;
mod look_settings;
mod menus_frame;
mod onboarding;
mod panel_select;
mod picker;
mod predictable;
mod recent;
mod screen_clip;
mod symbols;
mod toasts;
mod tray;
mod use_paste;

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

fn at(s: &str) -> Timestamp {
    s.parse().expect("timestamp")
}

fn outlook() -> Target {
    Target {
        app: "OUTLOOK.EXE".into(),
        title: "New message".into(),
        elevated: false,
        focus: Focus::Caret,
    }
}

struct World {
    ctl: Ctl,
    targets: FakeTargets,
    clock: FixedClock,
    ids: Vec<ItemId>,
    links_list: snipdeck_core::ListId,
}

fn add(
    store: &mut Store,
    list: snipdeck_core::ListId,
    kind: ItemKind,
    title: &str,
    text: &str,
) -> ItemId {
    let mut item = Item::new(list, title.into(), kind, at("2026-10-17T09:00:00Z"));
    item.preview = Item::preview_of(text, false);
    let content = Content::Inline(Inline::new(text.as_bytes().to_vec()).expect("inline"));
    let formats = Formats::new(vec![Format::Text(content)]).expect("formats");
    store
        .insert_item(&item, &formats, Placement::Last)
        .expect("insert");
    item.id
}

/// How a test world differs from the default one.
struct Opts {
    target: Option<Target>,
    settings: Settings,
    tier: Tier,
    /// Indexes into `World::ids` of the items that are pinned.
    pinned: Vec<usize>,
    /// What is on the clipboard (as if the user had just copied it).
    clipboard: Option<&'static str>,
    /// Writes to the clipboard fail.
    clipboard_busy: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            target: Some(outlook()),
            settings: Settings {
                panel_pinned: true,
                ..Settings::new_install()
            },
            tier: Tier::Pro,
            pinned: Vec::new(),
            clipboard: None,
            clipboard_busy: false,
        }
    }
}

/// Board "Work" with lists Replies (3 items + a template) and Links (1), pasting into
/// Outlook, on a fixed clock.
fn world_with(target: Option<Target>, settings: Settings) -> World {
    build(Opts {
        target,
        settings,
        ..Opts::default()
    })
}

fn build(opts: Opts) -> World {
    let Opts {
        target,
        settings,
        tier,
        pinned,
        clipboard,
        clipboard_busy,
    } = opts;
    let now = at("2026-10-17T09:00:00Z");
    let mut store = Store::open_in_memory().expect("store");
    let board = Board::new(Name::new("Work").expect("n"), now);
    store.insert_board(&board, Placement::Last).expect("board");
    let replies = List::new(board.id, Name::new("Replies").expect("n"), now);
    let links = List::new(board.id, Name::new("Links").expect("n"), now);
    store.insert_list(&replies, Placement::Last).expect("list");
    store.insert_list(&links, Placement::Last).expect("list");
    let mut ids = Vec::new();
    for (title, text) in [("One", "first"), ("Two", "second"), ("Three", "third")] {
        ids.push(add(&mut store, replies.id, ItemKind::Text, title, text));
    }
    ids.push(add(
        &mut store,
        replies.id,
        ItemKind::Template,
        "Greeting",
        "Hi {field:Name}",
    ));
    ids.push(add(
        &mut store,
        links.id,
        ItemKind::Link,
        "Docs",
        "https://example.com",
    ));
    for n in pinned {
        let mut item = store.item(ids[n]).expect("item");
        item.pinned = true;
        store.update_item(&item, None).expect("pin");
    }
    let targets = FakeTargets::new(target);
    let clock = FixedClock::new(at("2026-10-17T10:30:00Z"));
    let mut clipboard_fake = FakeClipboard {
        asks_after_polls: Some(1),
        fail_write: clipboard_busy,
        ..FakeClipboard::default()
    };
    if let Some(text) = clipboard {
        clipboard_fake.written.push(
            snipdeck_platform::payload::Payload::new(vec![Data::Text(text.into())])
                .expect("payload"),
        );
    }
    let services = Services {
        clipboard: clipboard_fake,
        keyboard: FakeKeyboard::default(),
        timing: FakeTiming::default(),
    };
    let mut ctl = Controller::new(
        store,
        services,
        Box::new(targets.clone()),
        Box::new(clock.clone()),
        settings,
        Entitlements::new(tier),
    )
    .expect("controller");
    // Never the machine's zone: a test's output must not depend on where it runs.
    ctl.tz = TimeZone::UTC;
    World {
        ctl,
        targets,
        clock,
        ids,
        links_list: links.id,
    }
}

fn world() -> World {
    world_with(
        Some(outlook()),
        Settings {
            panel_pinned: true,
            ..Settings::new_install()
        },
    )
}

fn uses(w: &World, id: ItemId) -> u32 {
    w.ctl.store().item(id).expect("item").use_count
}
