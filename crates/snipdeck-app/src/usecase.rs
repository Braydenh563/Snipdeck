//! Using an item (SPEC §5.1a): resolve its content, route it, run the platform pipeline and count
//! the use.

use std::collections::BTreeMap;

use snipdeck_core::{
    Entitlements, Feature, Format, Formats, Item, ItemId, ItemKind, Timestamp, UseMode, UseOutcome,
    template::{
        Locale, Prompt, PromptKind, RenderContext, RenderError, Template, TemplateError, TimeZone,
    },
};
use snipdeck_platform::{
    payload::{Data, FilePayload, Payload},
    pipeline::{self, Clipboard, Env, Keyboard, Timing},
    route::Route,
};
use snipdeck_store::{BlobStore, ItemRepo};

/// Everything a use needs from the caller, resolved at the moment of the click.
pub struct UseRequest {
    /// The item to use.
    pub item: ItemId,
    /// Paste, paste as plain text, or copy.
    pub mode: UseMode,
    /// Where the platform layer decided the item can go ([`snipdeck_platform::route::decide`]).
    pub route: Route,
    /// The moment of the use, for `{date}`, `{time}` and the usage count.
    pub now: Timestamp,
    /// What the user may do.
    pub entitlements: Entitlements,
    /// The user's answers to a template's prompts, by name.
    pub answers: BTreeMap<String, String>,
    /// The clipboard text for `{clipboard}`, read by the caller before the pipeline touches it.
    pub clipboard_text: Option<String>,
    /// The time zone for `{date}` and `{time}`.
    pub tz: TimeZone,
    /// The date and clock style for `{date}` and `{time}`.
    pub locale: Locale,
}

/// What a use did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Used {
    /// What happened, for the toast and the card pull.
    pub outcome: UseOutcome,
    /// For a template with `{cursor}`: how many Left-arrow presses move the caret to it.
    /// Not yet applied by the pipeline.
    pub cursor_from_end: Option<usize>,
    /// Why the use could not be counted, when the paste or copy worked but the store refused the
    /// usage update. The use is best-effort bookkeeping: failing it must not turn a paste that
    /// happened into an error (the user would retry and paste twice). The host may log it.
    pub count_failed: Option<String>,
}

/// Why a use did not happen.
#[derive(Debug, thiserror::Error)]
pub enum UseError {
    /// The store failed (missing item, locked key ring, unreadable blob).
    #[error(transparent)]
    Store(#[from] snipdeck_store::Error),
    /// A text format is not valid UTF-8.
    #[error("the item's text is not valid UTF-8")]
    NotText,
    /// The item's template no longer parses.
    #[error(transparent)]
    Template(#[from] TemplateError),
    /// The template cannot be rendered: a Pro placeholder on Free, or too large.
    #[error(transparent)]
    Render(#[from] RenderError),
    /// The template needs answers the request did not carry: show the fill form.
    #[error("the template needs answers")]
    NeedsAnswers(Vec<Prompt>),
    /// The clipboard or key injection failed.
    #[error(transparent)]
    Platform(#[from] pipeline::Error),
}

/// Uses an item. The use is counted only when it happened (pasted or copied); if counting it
/// fails the use still succeeds and [`Used::count_failed`] says why.
///
/// # Errors
///
/// See [`UseError`]. Never for the usage count.
pub fn use_item<S, C, K, T>(
    store: &mut S,
    env: &mut Env<'_, C, K, T>,
    request: &UseRequest,
) -> Result<Used, UseError>
where
    S: ItemRepo + BlobStore,
    C: Clipboard,
    K: Keyboard,
    T: Timing,
{
    let item = store.item(request.item)?;
    let formats = store.formats(request.item)?;
    let (payload, cursor_from_end) = build_payload(store, &item, &formats, request)?;
    let outcome = pipeline::use_item(env, &payload, request.mode, &request.route)?;
    // The paste or copy has happened: counting it is best-effort and never an error.
    let count_failed = store
        .record_use(item.id, request.now)
        .err()
        .map(|e| e.to_string());
    Ok(Used {
        outcome,
        cursor_from_end,
        count_failed,
    })
}

/// The source text of template `item`, with the item's title for the form's header (the first
/// line of the text when it has none), or `None` for an item that is not a template.
///
/// # Errors
///
/// A store error, or [`UseError::NotText`] if the template has no text.
pub fn template_source<S: ItemRepo + BlobStore>(
    store: &S,
    item: ItemId,
) -> Result<Option<(String, String)>, UseError> {
    let item = store.item(item)?;
    if item.kind != ItemKind::Template {
        return Ok(None);
    }
    let formats = store.formats(item.id)?;
    let text = text_of(store, &formats)?.ok_or(UseError::NotText)?;
    let title = if item.title.trim().is_empty() {
        text.lines().next().unwrap_or_default().to_owned()
    } else {
        item.title
    };
    Ok(Some((title, text)))
}

fn text_of<S: BlobStore>(store: &S, formats: &Formats) -> Result<Option<String>, UseError> {
    match formats.get(snipdeck_core::FormatKind::Text) {
        Some(Format::Text(content)) => {
            let bytes = store.read(content)?;
            String::from_utf8(bytes.into_owned())
                .map(Some)
                .map_err(|_| UseError::NotText)
        }
        _ => Ok(None),
    }
}

fn build_payload<S: BlobStore>(
    store: &S,
    item: &Item,
    formats: &Formats,
    request: &UseRequest,
) -> Result<(Payload, Option<usize>), UseError> {
    let mut cursor = None;
    let mut data = Vec::new();
    for format in formats {
        match format {
            Format::Text(content) => {
                let bytes = store.read(content)?;
                let text = String::from_utf8(bytes.into_owned()).map_err(|_| UseError::NotText)?;
                if item.kind == ItemKind::Template {
                    let rendered = render_template(&text, request)?;
                    cursor = rendered.1;
                    data.push(Data::Text(rendered.0));
                } else {
                    data.push(Data::Text(text));
                }
            }
            // A template's rich formats cannot be filled in: its text is what gets pasted.
            Format::Html(_) | Format::Rtf(_) if item.kind == ItemKind::Template => {}
            Format::Html(content) => {
                let bytes = store.read(content)?;
                data.push(Data::Html(
                    String::from_utf8(bytes.into_owned()).map_err(|_| UseError::NotText)?,
                ));
            }
            Format::Rtf(content) => data.push(Data::Rtf(store.read(content)?.into_owned())),
            Format::Png(content) => data.push(Data::Png(store.read(content)?.into_owned())),
            Format::Files(entries) => {
                let mut files = Vec::with_capacity(entries.len());
                for entry in entries {
                    files.push(FilePayload {
                        name: entry.name().to_owned(),
                        bytes: store.read(entry.content())?.into_owned(),
                    });
                }
                data.push(Data::Files(files));
            }
        }
    }
    let payload = Payload::new(data).ok_or(UseError::NotText)?;
    Ok((
        if item.sensitive {
            payload.private()
        } else {
            payload
        },
        cursor,
    ))
}

fn render_template(text: &str, request: &UseRequest) -> Result<(String, Option<usize>), UseError> {
    let template = Template::parse(text)?;
    // Before asking anything: a Free user must not fill in a form they cannot use.
    for &feature in Feature::ALL {
        if template.requires(feature) && !request.entitlements.allows(feature) {
            return Err(RenderError::ProRequired(feature).into());
        }
    }
    let missing: Vec<Prompt> = template
        .prompts()
        .into_iter()
        .filter(|p| {
            !request.answers.contains_key(&p.name)
                && !matches!(
                    p.kind,
                    PromptKind::Field { default: Some(_) } | PromptKind::Choice { .. }
                )
        })
        .collect();
    if !missing.is_empty() {
        return Err(UseError::NeedsAnswers(template.prompts()));
    }
    let mut ctx = RenderContext::new(request.now, &request.answers)
        .with_tz(request.tz.clone())
        .with_locale(request.locale);
    if let Some(clipboard) = &request.clipboard_text {
        ctx = ctx.with_clipboard(clipboard);
    }
    let rendered = template.render_checked(&request.entitlements, &ctx)?;
    Ok((rendered.text, rendered.cursor_from_end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use snipdeck_core::{Board, Content, Inline, List, Name, Placement, Tier};
    use snipdeck_platform::{
        keys::PasteKey,
        route::{CopyReason, Route},
        testing::{FakeClipboard, FakeKeyboard, FakeTiming},
    };
    use snipdeck_store::{BoardRepo, ListRepo, Store};

    fn at(s: &str) -> Timestamp {
        s.parse().expect("timestamp")
    }

    struct World {
        store: Store,
        clipboard: FakeClipboard,
        keyboard: FakeKeyboard,
        timing: FakeTiming,
        list: snipdeck_core::ListId,
    }

    impl World {
        fn new() -> Self {
            let mut store = Store::open_in_memory().expect("store");
            let now = at("2026-10-17T09:00:00Z");
            let board = Board::new(Name::new("Work").expect("name"), now);
            store.insert_board(&board, Placement::Last).expect("board");
            let list = List::new(board.id, Name::new("Replies").expect("name"), now);
            store.insert_list(&list, Placement::Last).expect("list");
            Self {
                store,
                clipboard: FakeClipboard {
                    asks_after_polls: Some(1),
                    ..FakeClipboard::default()
                },
                keyboard: FakeKeyboard::default(),
                timing: FakeTiming::default(),
                list: list.id,
            }
        }

        fn add(&mut self, kind: ItemKind, sensitive: bool, text: &str) -> ItemId {
            let now = at("2026-10-17T09:00:00Z");
            let mut item = Item::new(self.list, "t".into(), kind, now);
            item.sensitive = sensitive;
            item.preview = Item::preview_of(text, sensitive);
            let content = Content::Inline(Inline::new(text.as_bytes().to_vec()).expect("inline"));
            let formats = Formats::new(vec![Format::Text(content)]).expect("formats");
            self.store
                .insert_item(&item, &formats, Placement::Last)
                .expect("item");
            item.id
        }

        fn request(item: ItemId, mode: UseMode, route: Route) -> UseRequest {
            UseRequest {
                item,
                mode,
                route,
                now: at("2026-10-17T10:30:00Z"),
                entitlements: Entitlements::new(Tier::Pro),
                answers: BTreeMap::new(),
                clipboard_text: None,
                tz: TimeZone::UTC,
                locale: Locale::for_tag("en"),
            }
        }

        fn run(&mut self, request: &UseRequest) -> Result<Used, UseError> {
            let mut env = Env {
                clipboard: &mut self.clipboard,
                keyboard: &mut self.keyboard,
                timing: &mut self.timing,
            };
            use_item(&mut self.store, &mut env, request)
        }
    }

    const PASTE: Route = Route::Paste {
        key: PasteKey::CtrlV,
    };

    #[test]
    fn a_paste_goes_through_the_pipeline_and_counts_one_use() {
        let mut w = World::new();
        let id = w.add(ItemKind::Text, false, "hello");
        let request = World::request(id, UseMode::Paste, PASTE);
        let used = w.run(&request).expect("used");
        assert_eq!(used.outcome, UseOutcome::Pasted);
        assert_eq!(w.clipboard.written[0].data(), [Data::Text("hello".into())]);
        let item = w.store.item(id).expect("item");
        assert_eq!(item.use_count, 1);
        assert_eq!(item.last_used_at, Some(request.now));
    }

    #[test]
    fn a_copy_leaves_the_text_on_the_clipboard() {
        let mut w = World::new();
        let id = w.add(ItemKind::Text, false, "hello");
        let request = World::request(id, UseMode::Copy, Route::Copy(CopyReason::NoTarget));
        assert_eq!(w.run(&request).expect("used").outcome, UseOutcome::Copied);
        assert_eq!(w.clipboard.log, ["persistent:1"]);
        assert_eq!(
            w.keyboard.sent,
            Vec::<Vec<snipdeck_platform::keys::KeyEvent>>::new()
        );
    }

    /// A store whose usage update fails, to see that a use which happened is not reported as
    /// failed (PR 4 review 4).
    struct NoCount<'a>(&'a mut Store);

    impl ItemRepo for NoCount<'_> {
        fn items(&self, list: snipdeck_core::ListId) -> snipdeck_store::Result<Vec<Item>> {
            self.0.items(list)
        }
        fn all_items(&self) -> snipdeck_store::Result<Vec<Item>> {
            self.0.all_items()
        }
        fn item(&self, id: ItemId) -> snipdeck_store::Result<Item> {
            self.0.item(id)
        }
        fn formats(&self, id: ItemId) -> snipdeck_store::Result<Formats> {
            self.0.formats(id)
        }
        fn insert_item(
            &mut self,
            item: &Item,
            formats: &Formats,
            at: Placement<ItemId>,
        ) -> snipdeck_store::Result<()> {
            self.0.insert_item(item, formats, at)
        }
        fn update_item(
            &mut self,
            item: &Item,
            formats: Option<&Formats>,
        ) -> snipdeck_store::Result<()> {
            self.0.update_item(item, formats)
        }
        fn move_item(
            &mut self,
            id: ItemId,
            list: snipdeck_core::ListId,
            at: Placement<ItemId>,
        ) -> snipdeck_store::Result<()> {
            self.0.move_item(id, list, at)
        }
        fn record_use(&mut self, _id: ItemId, _at: Timestamp) -> snipdeck_store::Result<()> {
            Err(snipdeck_store::Error::Corrupt("disk full".to_owned()))
        }
        fn search(&self, query: &str, limit: usize) -> snipdeck_store::Result<Vec<ItemId>> {
            self.0.search(query, limit)
        }
    }

    impl BlobStore for NoCount<'_> {
        fn put(&mut self, bytes: Vec<u8>) -> snipdeck_store::Result<Content> {
            self.0.put(bytes)
        }
        fn read<'a>(
            &self,
            content: &'a Content,
        ) -> snipdeck_store::Result<std::borrow::Cow<'a, [u8]>> {
            self.0.read(content)
        }
    }

    #[test]
    fn a_paste_that_happened_is_a_success_even_when_the_count_cannot_be_saved() {
        let mut w = World::new();
        let id = w.add(ItemKind::Text, false, "hello");
        let request = World::request(id, UseMode::Paste, PASTE);
        let mut env = Env {
            clipboard: &mut w.clipboard,
            keyboard: &mut w.keyboard,
            timing: &mut w.timing,
        };
        let used = use_item(&mut NoCount(&mut w.store), &mut env, &request)
            .expect("the paste happened: not an error");
        assert_eq!(used.outcome, UseOutcome::Pasted);
        assert!(
            used.count_failed
                .as_deref()
                .is_some_and(|m| m.contains("disk full")),
            "{:?}",
            used.count_failed
        );
        assert_eq!(w.store.item(id).expect("item").use_count, 0, "not counted");
        // A normal use reports nothing.
        let ok = w.run(&request).expect("used");
        assert_eq!(ok.count_failed, None);
    }

    #[test]
    fn a_failed_clipboard_is_an_error_and_does_not_count_a_use() {
        let mut w = World::new();
        let id = w.add(ItemKind::Text, false, "hello");
        w.clipboard.fail_write = true;
        let request = World::request(id, UseMode::Paste, PASTE);
        assert!(matches!(w.run(&request), Err(UseError::Platform(_))));
        assert_eq!(w.store.item(id).expect("item").use_count, 0);
    }

    #[test]
    fn sensitive_items_are_written_private() {
        let mut w = World::new();
        let id = w.add(ItemKind::Text, true, "hunter2");
        let request = World::request(id, UseMode::Copy, Route::Copy(CopyReason::NoTarget));
        w.run(&request).expect("used");
        assert!(w.clipboard.written[0].is_private());
    }

    #[test]
    fn a_template_is_rendered_with_the_answers() {
        let mut w = World::new();
        let id = w.add(ItemKind::Template, false, "Hi {field:Name}, on {date}.");
        let mut request = World::request(id, UseMode::Paste, PASTE);
        request.answers.insert("Name".into(), "Sam".into());
        w.run(&request).expect("used");
        let Data::Text(text) = &w.clipboard.written[0].data()[0] else {
            panic!("text expected")
        };
        assert!(text.starts_with("Hi Sam, on "), "{text}");
        assert!(!text.contains('{'), "{text}");
    }

    #[test]
    fn a_template_without_its_answers_asks_for_them_and_changes_nothing() {
        let mut w = World::new();
        let id = w.add(ItemKind::Template, false, "Hi {field:Name}");
        let request = World::request(id, UseMode::Paste, PASTE);
        let Err(UseError::NeedsAnswers(prompts)) = w.run(&request) else {
            panic!("expected NeedsAnswers")
        };
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].name, "Name");
        assert_eq!(w.clipboard.log, Vec::<String>::new());
        assert_eq!(w.store.item(id).expect("item").use_count, 0);
        // The form gets the template's text and a name for its header.
        let (title, text) = template_source(&w.store, id)
            .expect("source")
            .expect("a template");
        assert_eq!(title, "t");
        assert_eq!(text, "Hi {field:Name}");
    }

    #[test]
    fn a_pro_placeholder_on_free_is_refused() {
        let mut w = World::new();
        let id = w.add(ItemKind::Template, false, "Hi {field:Name|there}");
        let mut request = World::request(id, UseMode::Copy, Route::Copy(CopyReason::NoTarget));
        request.entitlements = Entitlements::new(Tier::Free);
        assert!(matches!(
            w.run(&request),
            Err(UseError::Render(RenderError::ProRequired(_)))
        ));
        assert_eq!(w.store.item(id).expect("item").use_count, 0);
    }

    #[test]
    fn plain_items_have_no_template_source() {
        let mut w = World::new();
        let id = w.add(ItemKind::Text, false, "{not a placeholder}");
        assert_eq!(template_source(&w.store, id).expect("source"), None);
    }
}
