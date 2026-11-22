//! Using an item (SPEC §5.1a): resolve its content, route it, run the platform pipeline and count
//! the use.

use std::collections::BTreeMap;

use snipdeck_core::{
    CaseShape, Format, Formats, Item, ItemId, ItemKind, PasteFailure, Timestamp, UseMode,
    UseOutcome,
    template::{
        KeyStop, Locale, Prompt, RenderContext, RenderError, SnippetLookup, SnippetSource,
        Template, TemplateError, TemplateKey, TimeZone,
        transform::{self, Transform},
    },
};
use snipdeck_platform::{
    keys::{Key, KeyEvent},
    payload::{Data, FilePayload, Payload},
    pipeline::{self, Clipboard, Env, Keyboard, Timing},
    route::Route,
};
use snipdeck_store::{BlobStore, ItemRepo, TriggerRepo};

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
    /// The user's answers to a template's prompts, by name.
    pub answers: BTreeMap<String, String>,
    /// The clipboard text for `{clipboard}`, read by the caller before the pipeline touches it.
    pub clipboard_text: Option<String>,
    /// The earlier entries of the clipboard history, newest first, for `{clipboard:2}` and on
    /// (empty unless the template names a position; see `Template::max_clipboard_position`).
    pub clipboard_history: Vec<String>,
    /// The text selected in the Target, for `{selection}`, read before the form opened.
    pub selection: Option<String>,
    /// The time zone for `{date}` and `{time}`.
    pub tz: TimeZone,
    /// The date and clock style for `{date}` and `{time}`.
    pub locale: Locale,
    /// A "Paste as..." transform: applied to the text (a template is rendered first) and the
    /// item goes out as plain text only, with the caret left at the end.
    pub transform: Option<Transform>,
    /// Random bytes for each `{uuid}`; `None` draws them from the system.
    pub uuids: Option<Vec<[u8; 16]>>,
    /// Backspaces to send with the paste, to erase a typed trigger (0 for every other use).
    pub erase: usize,
    /// What a typed trigger asks of the text that replaces it.
    pub retype: Retype,
}

/// What a typed trigger asks of the text that replaces it: the case of the code typed
/// ([`CaseMode::Adapt`](snipdeck_core::CaseMode)) and the delimiter that ended it, which went
/// with the Backspaces and is typed again after the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Retype {
    /// How the text follows the case typed.
    pub shape: CaseShape,
    /// The delimiter to put back after the text.
    pub delimiter: Option<char>,
}

impl Retype {
    /// The HTML for the delimiter.
    fn html(c: char) -> String {
        match c {
            '\n' | '\r' => "<br>".to_owned(),
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            c => c.to_string(),
        }
    }
}

/// The most Left presses sent to put the caret at `{cursor}`.
pub const MAX_CURSOR_PRESSES: usize = 512;

/// What a use did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Used {
    /// What happened, for the toast and the card pull.
    pub outcome: UseOutcome,
    /// For a template with `{cursor}`: how many Left-arrow presses move the caret to it. Sent
    /// after a paste (at most [`MAX_CURSOR_PRESSES`]); a copy has no caret to move.
    pub cursor_from_end: Option<usize>,
    /// How many characters a paste puts into the Target (a line break counts once), when that is
    /// known: text only, and no `{cursor}` that moves the caret off the end. The undo of a typed
    /// expansion erases this many (PANEL-16).
    pub inserted_chars: Option<usize>,
    /// Why the use could not be counted, when the paste or copy worked but the store refused the
    /// usage update. The use is best-effort bookkeeping: failing it must not turn a paste that
    /// happened into an error (the user would retry and paste twice). The host may log it.
    pub count_failed: Option<String>,
}

impl Used {
    /// The diagnostic line for a use that worked but could not be counted (`SNIPDECK_LOG=1`).
    #[must_use]
    pub fn count_failed_line(&self) -> Option<String> {
        self.count_failed
            .as_ref()
            .map(|why| format!("use not counted: {why}"))
    }
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
    /// The template cannot be rendered: too large.
    #[error(transparent)]
    Render(#[from] RenderError),
    /// The template needs answers the request did not carry: show the fill form.
    #[error("the template needs answers")]
    NeedsAnswers(Vec<Prompt>),
    /// The clipboard or key injection failed.
    #[error(transparent)]
    Platform(#[from] pipeline::Error),
    /// The system gave no random numbers for `{uuid}`.
    #[error("the system gave no random numbers")]
    NoRandomness,
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
    S: ItemRepo + TriggerRepo + BlobStore,
    C: Clipboard,
    K: Keyboard,
    T: Timing,
{
    use_item_guarded(store, env, request, &|| true)
}

/// [`use_item`] that asks `proceed` right before the keys go out and calls the use off when it
/// says no ([`pipeline::Error::Superseded`], as [`UseError::Platform`]).
///
/// # Errors
///
/// See [`UseError`].
pub fn use_item_guarded<S, C, K, T>(
    store: &mut S,
    env: &mut Env<'_, C, K, T>,
    request: &UseRequest,
    proceed: &dyn Fn() -> bool,
) -> Result<Used, UseError>
where
    S: ItemRepo + TriggerRepo + BlobStore,
    C: Clipboard,
    K: Keyboard,
    T: Timing,
{
    match begin_use(store, env, request, proceed)? {
        Started::Done(used) => Ok(used),
        Started::Waiting(mut pending) => loop {
            if let Some(used) = pending.poll(store, env.clipboard, env.keyboard, env.timing) {
                return Ok(used);
            }
            env.timing.sleep(pipeline::POLL_INTERVAL);
        },
    }
}

/// A use begun by [`begin_use`] that has not finished: the paste key is out and the clipboard
/// still holds the item until [`Pending::poll`] says the paste is over.
pub struct Pending<C: Clipboard> {
    waiting: pipeline::Waiting<C>,
    item: ItemId,
    now: Timestamp,
    cursor_from_end: Option<usize>,
    inserted_chars: Option<usize>,
}

impl<C: Clipboard> Pending<C> {
    /// The item being used.
    #[must_use]
    pub fn item(&self) -> ItemId {
        self.item
    }

    /// The characters this paste puts into the Target, when known (see [`Used::inserted_chars`]).
    #[must_use]
    pub fn inserted_chars(&self) -> Option<usize> {
        self.inserted_chars
    }

    /// One look, without waiting (see [`pipeline::Waiting::poll`]): `Some` when the paste is
    /// over, with the caret moved and the use counted.
    pub fn poll<S, K, T>(
        &mut self,
        store: &mut S,
        clipboard: &mut C,
        keyboard: &mut K,
        timing: &mut T,
    ) -> Option<Used>
    where
        S: ItemRepo,
        K: Keyboard,
        T: Timing,
    {
        let outcome = self.waiting.poll(clipboard, timing)?;
        Some(finish(
            store,
            keyboard,
            self.item,
            self.now,
            outcome,
            (self.cursor_from_end, self.inserted_chars),
        ))
    }
}

/// How [`begin_use`] stands.
pub enum Started<C: Clipboard> {
    /// Over already: a copy, or a paste that could not send its key.
    Done(Used),
    /// The paste key is out; poll the rest.
    Waiting(Pending<C>),
}

/// [`use_item_guarded`] up to the paste key: the paste then finishes by polling
/// ([`Pending::poll`]) instead of blocking the caller's thread.
///
/// # Errors
///
/// See [`UseError`].
pub fn begin_use<S, C, K, T>(
    store: &mut S,
    env: &mut Env<'_, C, K, T>,
    request: &UseRequest,
    proceed: &dyn Fn() -> bool,
) -> Result<Started<C>, UseError>
where
    S: ItemRepo + TriggerRepo + BlobStore,
    C: Clipboard,
    K: Keyboard,
    T: Timing,
{
    let item = store.item(request.item)?;
    let formats = store.formats(request.item)?;
    let (payload, cursor_from_end, keys) = build_payload(store, &item, &formats, request)?;
    // An item set to "always paste as plain text" pastes plain whatever the caller asked.
    let mode = item.effective_mode(request.mode);
    if !keys.is_empty() && mode != UseMode::Copy {
        // A template with key presses goes out piece by piece, waiting for each piece (the
        // typed-characters count of a trigger expansion does not cover the keys).
        let outcome = paste_in_pieces(env, &payload, &keys, mode, request, proceed)?;
        return Ok(Started::Done(finish(
            store,
            env.keyboard,
            item.id,
            request.now,
            outcome,
            (cursor_from_end, None),
        )));
    }
    let inserted_chars = inserted_chars(&payload, cursor_from_end);
    let began =
        pipeline::begin_item_erasing(env, &payload, mode, &request.route, request.erase, proceed)?;
    Ok(match began {
        pipeline::Began::Done(outcome) => Started::Done(finish(
            store,
            env.keyboard,
            item.id,
            request.now,
            outcome,
            (cursor_from_end, inserted_chars),
        )),
        pipeline::Began::Waiting(waiting) => Started::Waiting(Pending {
            waiting,
            item: item.id,
            now: request.now,
            cursor_from_end,
            inserted_chars,
        }),
    })
}

/// The characters `payload` puts into a Target: its text, a line break counted once; `None` when
/// it holds a picture or files, has no text, or `{cursor}` leaves the caret before the end.
fn inserted_chars(payload: &Payload, cursor_from_end: Option<usize>) -> Option<usize> {
    if cursor_from_end.is_some_and(|n| n > 0) {
        return None;
    }
    let mut text = None;
    for data in payload.data() {
        match data {
            Data::Text(t) => text = Some(t),
            Data::Html(_) | Data::Rtf(_) => {}
            Data::Png(_) | Data::Files(_) => return None,
        }
    }
    text.map(|t| t.replace("\r\n", "\n").chars().count())
}

/// The paste or copy is over: moves the caret back for a template's `{cursor}` and counts the
/// use (best-effort, never an error).
fn finish<S: ItemRepo, K: Keyboard>(
    store: &mut S,
    keyboard: &mut K,
    item: ItemId,
    now: Timestamp,
    outcome: UseOutcome,
    (cursor_from_end, inserted_chars): (Option<usize>, Option<usize>),
) -> Used {
    if matches!(outcome, UseOutcome::Pasted | UseOutcome::PastedNotRestored)
        && let Some(n) = cursor_from_end.filter(|n| *n > 0)
    {
        // The paste happened: a failed caret move is not an error. Unverified on Windows
        // (Fable v1 sweep A5).
        let left = |down| KeyEvent {
            key: Key::Left,
            down,
        };
        let presses: Vec<KeyEvent> = (0..n.min(MAX_CURSOR_PRESSES))
            .flat_map(|_| [left(true), left(false)])
            .collect();
        let _ = keyboard.send(&presses);
    }
    // The paste or copy has happened: counting it is best-effort and never an error.
    let count_failed = store.record_use(item, now).err().map(|e| e.to_string());
    Used {
        outcome,
        cursor_from_end,
        inserted_chars,
        count_failed,
    }
}

/// A template with key presses (`{key:tab}`) goes out piece by piece: each piece is pasted as
/// an ordinary use, and the key goes out between two pieces. A route that only copies has no
/// key to press ([`UseMode::Copy`] and `Route::Copy` keep the text whole, without the keys).
fn paste_in_pieces<C: Clipboard, K: Keyboard, T: Timing>(
    env: &mut Env<'_, C, K, T>,
    payload: &Payload,
    keys: &[KeyStop],
    mode: UseMode,
    request: &UseRequest,
    proceed: &dyn Fn() -> bool,
) -> Result<UseOutcome, pipeline::Error> {
    let Some(Data::Text(text)) = payload.data().first() else {
        return pipeline::use_item_erasing(
            env,
            payload,
            mode,
            &request.route,
            request.erase,
            proceed,
        );
    };
    if !matches!(request.route, Route::Paste { .. }) {
        return pipeline::use_item_erasing(
            env,
            payload,
            mode,
            &request.route,
            request.erase,
            proceed,
        );
    }
    let private = payload.is_private();
    let mut start = 0;
    let mut erase = request.erase;
    let mut worst = UseOutcome::Pasted;
    let always: &dyn Fn() -> bool = &|| true;
    let mut asked = proceed;
    for (i, end) in keys.iter().map(|k| k.at).chain([text.len()]).enumerate() {
        let piece = &text[start..end];
        start = end;
        if !piece.is_empty() {
            let one = Payload::new(vec![Data::Text(piece.to_owned())]);
            let one = one.map(|p| if private { p.private() } else { p });
            let Some(one) = one else { continue };
            let outcome =
                pipeline::use_item_erasing(env, &one, mode, &request.route, erase, asked)?;
            erase = 0;
            asked = always;
            match outcome {
                UseOutcome::Pasted => {}
                UseOutcome::PastedNotRestored => worst = outcome,
                // The pipeline fell back to a copy: the rest cannot be typed either.
                other => return Ok(other),
            }
        }
        if let Some(stop) = keys.get(i) {
            let mut events = snipdeck_platform::keys::backspaces(erase);
            erase = 0;
            events.extend(key_events(stop.key));
            if env.keyboard.send(&events).is_err() {
                return Ok(UseOutcome::CopiedInstead(PasteFailure::TargetUnavailable));
            }
        }
    }
    Ok(worst)
}

/// The press and release of a template's key.
fn key_events(key: TemplateKey) -> [KeyEvent; 2] {
    let key = match key {
        TemplateKey::Tab => Key::Tab,
        TemplateKey::Enter => Key::Enter,
    };
    [true, false].map(|down| KeyEvent { key, down })
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
    let title = if !item.title.trim().is_empty() {
        item.title
    } else if item.sensitive {
        // The first line of a sensitive body is never put on the screen.
        String::new()
    } else {
        text.lines().next().unwrap_or_default().to_owned()
    };
    Ok(Some((title, text)))
}

/// The text of `item`, or `None` when it holds none (an image, files). For a sensitive item
/// this reads the real text, so the caller shows it only while the user holds it revealed.
///
/// # Errors
///
/// A store error (a locked key ring for a sensitive item), or [`UseError::NotText`].
pub fn item_text<S: ItemRepo + BlobStore>(
    store: &S,
    item: ItemId,
) -> Result<Option<String>, UseError> {
    let formats = store.formats(item)?;
    text_of(store, &formats)
}

/// The PNG bytes of image item `item`, or `None` when it keeps no picture.
///
/// # Errors
///
/// A store error.
pub fn item_png<S: ItemRepo + BlobStore>(
    store: &S,
    item: ItemId,
) -> Result<Option<Vec<u8>>, UseError> {
    let formats = store.formats(item)?;
    match formats.get(snipdeck_core::FormatKind::Png) {
        Some(Format::Png(content)) => Ok(Some(store.read(content)?.into_owned())),
        _ => Ok(None),
    }
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

/// What a use puts on the clipboard: the payload, where the caret ends up, and the key presses
/// that go between pieces of its text.
type Built = (Payload, Option<usize>, Vec<KeyStop>);

fn build_payload<S: ItemRepo + TriggerRepo + BlobStore>(
    store: &S,
    item: &Item,
    formats: &Formats,
    request: &UseRequest,
) -> Result<Built, UseError> {
    let mut cursor = None;
    let mut keys = Vec::new();
    let mut data = Vec::new();
    for format in formats {
        match format {
            // A transformed paste is plain text: no HTML, RTF, picture or files go with it.
            Format::Html(_) | Format::Rtf(_) | Format::Png(_) | Format::Files(_)
                if request.transform.is_some() => {}
            // A text in another case is plain; an RTF twin cannot take a delimiter.
            Format::Html(_) if request.retype.shape != CaseShape::AsIs => {}
            Format::Rtf(_)
                if request.retype.shape != CaseShape::AsIs
                    || request.retype.delimiter.is_some() => {}
            Format::Text(content) => {
                let bytes = store.read(content)?;
                let text = String::from_utf8(bytes.into_owned()).map_err(|_| UseError::NotText)?;
                let mut text = if item.kind == ItemKind::Template {
                    let rendered = render_template(store, &text, request)?;
                    cursor = rendered.cursor_from_end;
                    keys = rendered.keys;
                    rendered.text
                } else {
                    text
                };
                if let Some(transform) = request.transform {
                    text = transform::apply(transform, &text);
                    // The transform can change lengths (`ß` is `SS`): no caret to restore, and
                    // no place for a key press.
                    cursor = None;
                    keys.clear();
                }
                if request.retype.shape != CaseShape::AsIs {
                    text = request.retype.shape.apply(&text);
                    cursor = None;
                }
                if let Some(d) = request.retype.delimiter {
                    text.push(d);
                    cursor = cursor.map(|n| n + 1);
                }
                data.push(Data::Text(text));
            }
            // A template's rich formats cannot be filled in: its text is what gets pasted.
            Format::Html(_) | Format::Rtf(_) if item.kind == ItemKind::Template => {}
            Format::Html(content) => {
                let bytes = store.read(content)?;
                let mut html =
                    String::from_utf8(bytes.into_owned()).map_err(|_| UseError::NotText)?;
                if let Some(d) = request.retype.delimiter {
                    html.push_str(&Retype::html(d));
                }
                data.push(Data::Html(html));
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
        keys,
    ))
}

fn render_template<S: ItemRepo + TriggerRepo + BlobStore>(
    store: &S,
    text: &str,
    request: &UseRequest,
) -> Result<snipdeck_core::template::Rendered, UseError> {
    let template = parse_template(store, text)?;
    // Any prompt without an answer opens the form, whatever its kind: a default only prefills
    // it, and a choice, date or tick box alone must still be asked (empty date = today). A
    // prompt inside an `{optional}` section the answers leave unticked is not asked.
    let prompts = template.prompts();
    let active = template.active_prompts(&request.answers);
    if prompts
        .iter()
        .zip(&active)
        .any(|(p, on)| *on && !request.answers.contains_key(&p.name))
    {
        return Err(UseError::NeedsAnswers(prompts));
    }
    let drawn;
    let uuids = if let Some(given) = &request.uuids {
        given.as_slice()
    } else {
        drawn = (0..template.uuid_count())
            .map(|_| {
                let mut bytes = [0u8; 16];
                getrandom::fill(&mut bytes).map_err(|_| UseError::NoRandomness)?;
                Ok(bytes)
            })
            .collect::<Result<Vec<_>, UseError>>()?;
        drawn.as_slice()
    };
    let mut ctx = RenderContext::new(request.now, &request.answers)
        .with_tz(request.tz.clone())
        .with_locale(request.locale)
        .with_uuids(uuids)
        .with_clipboard_history(&request.clipboard_history);
    if let Some(clipboard) = &request.clipboard_text {
        ctx = ctx.with_clipboard(clipboard);
    }
    if let Some(selection) = &request.selection {
        ctx = ctx.with_selection(selection);
    }
    Ok(template.try_render(&ctx)?)
}

/// How this user writes dates and times in a template: the language's names, with the numeric
/// order and the clock taken from the PC's regional format when Windows tells it (SPEC §5.4).
#[must_use]
pub fn template_locale(language: &str) -> Locale {
    let regional = snipdeck_platform::regional::read();
    Locale::for_tag(language).with_regional(
        regional.short_date.as_deref(),
        regional.short_time.as_deref(),
    )
}

/// `src` parsed as a template, with `{snippet:Title}` resolved against the library.
///
/// # Errors
///
/// The template's own error: a bad placeholder, or a snippet that is missing, ambiguous,
/// sensitive, too deep or inside itself.
pub fn parse_template<S: ItemRepo + TriggerRepo + BlobStore>(
    store: &S,
    src: &str,
) -> Result<Template, TemplateError> {
    Template::parse_with(src, &StoreSnippets::new(store))
}

/// The library as the place `{snippet:Title}` looks in: the item with that title (ignoring case
/// and surrounding spaces), else the one with that trigger. A text snippet is inserted as it
/// is, braces and all; a template snippet is inserted as a template. A Sensitive item, or one
/// without text, is refused.
pub struct StoreSnippets<'a, S> {
    store: &'a S,
    items: std::cell::OnceCell<Vec<Item>>,
}

impl<'a, S: ItemRepo + TriggerRepo + BlobStore> StoreSnippets<'a, S> {
    /// The library of `store`; nothing is read until a snippet is asked for.
    #[must_use]
    pub fn new(store: &'a S) -> Self {
        Self {
            store,
            items: std::cell::OnceCell::new(),
        }
    }

    fn text_of_item(&self, item: &Item) -> SnippetLookup {
        if item.sensitive || !matches!(item.kind, ItemKind::Text | ItemKind::Template) {
            return SnippetLookup::Unusable;
        }
        let Ok(formats) = self.store.formats(item.id) else {
            return SnippetLookup::Unusable;
        };
        match text_of(self.store, &formats) {
            Ok(Some(text)) if item.kind == ItemKind::Template => SnippetLookup::Found(text),
            Ok(Some(text)) => SnippetLookup::Found(snipdeck_core::template::escape(&text)),
            _ => SnippetLookup::Unusable,
        }
    }
}

impl<S: ItemRepo + TriggerRepo + BlobStore> SnippetSource for StoreSnippets<'_, S> {
    fn snippet(&self, name: &str) -> SnippetLookup {
        let wanted = name.trim().to_lowercase();
        let items = self
            .items
            .get_or_init(|| self.store.all_items().unwrap_or_default());
        let mut titled = items
            .iter()
            .filter(|i| !i.title.trim().is_empty() && i.title.trim().to_lowercase() == wanted);
        if let Some(first) = titled.next() {
            return if titled.next().is_some() {
                SnippetLookup::Ambiguous
            } else {
                self.text_of_item(first)
            };
        }
        let by_trigger = snipdeck_core::Trigger::new(name.trim())
            .ok()
            .and_then(|t| self.store.trigger_holder(&t).ok().flatten());
        match by_trigger.and_then(|id| items.iter().find(|i| i.id == id)) {
            Some(item) => self.text_of_item(item),
            None => SnippetLookup::NotFound,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snipdeck_core::{Board, Content, Inline, List, Name, Placement};
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
            self.add_titled("t", kind, sensitive, text)
        }

        fn add_titled(
            &mut self,
            title: &str,
            kind: ItemKind,
            sensitive: bool,
            text: &str,
        ) -> ItemId {
            let now = at("2026-10-17T09:00:00Z");
            let mut item = Item::new(self.list, title.into(), kind, now);
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
                answers: BTreeMap::new(),
                clipboard_text: None,
                clipboard_history: Vec::new(),
                selection: None,
                tz: TimeZone::UTC,
                locale: Locale::for_tag("en"),
                transform: None,
                uuids: None,
                erase: 0,
                retype: Retype::default(),
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

    /// Gives item `id` an HTML flavour next to its text; optionally marks it always-plain.
    fn add_html(w: &mut World, id: ItemId, always_plain: bool) {
        let existing = w.store.formats(id).expect("formats");
        let mut formats: Vec<Format> = existing.into_iter().cloned().collect();
        formats.push(Format::Html(snipdeck_core::Content::Inline(
            snipdeck_core::Inline::new(b"<b>hello</b>".to_vec()).expect("inline"),
        )));
        let mut item = w.store.item(id).expect("item");
        item.always_plain = always_plain;
        w.store
            .update_item(&item, Some(&Formats::new(formats).expect("formats")))
            .expect("set");
    }

    #[test]
    fn an_always_plain_item_pastes_text_only_but_a_copy_keeps_its_formats() {
        let mut w = World::new();
        let id = w.add(ItemKind::Text, false, "hello");
        add_html(&mut w, id, true);
        w.run(&World::request(id, UseMode::Paste, PASTE))
            .expect("used");
        assert_eq!(texts_written(&w), [[Data::Text("hello".into())]]);
        let copy = World::request(id, UseMode::Copy, Route::Copy(CopyReason::NoTarget));
        w.run(&copy).expect("used");
        assert_eq!(w.clipboard.written[1].data().len(), 2, "a copy stays rich");
    }

    #[test]
    fn an_explicit_plain_paste_drops_the_formats_of_an_ordinary_item() {
        let mut w = World::new();
        let id = w.add(ItemKind::Text, false, "hello");
        add_html(&mut w, id, false);
        w.run(&World::request(id, UseMode::Paste, PASTE))
            .expect("used");
        assert_eq!(w.clipboard.written[0].data().len(), 2);
        w.run(&World::request(id, UseMode::PastePlain, PASTE))
            .expect("used");
        assert_eq!(w.clipboard.written[1].data(), [Data::Text("hello".into())]);
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
        fn change_counter(&self) -> u64 {
            self.0.change_counter()
        }
        fn usage_counter(&self) -> u64 {
            self.0.usage_counter()
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
        fn search(
            &self,
            query: &snipdeck_core::search::Query,
        ) -> snipdeck_store::Result<snipdeck_core::search::BodyMatches> {
            self.0.search(query)
        }
    }

    impl TriggerRepo for NoCount<'_> {
        fn triggers(&self) -> snipdeck_store::Result<Vec<(snipdeck_core::Trigger, ItemId)>> {
            self.0.triggers()
        }
        fn trigger_of(
            &self,
            item: ItemId,
        ) -> snipdeck_store::Result<Option<snipdeck_core::Trigger>> {
            self.0.trigger_of(item)
        }
        fn trigger_holder(
            &self,
            trigger: &snipdeck_core::Trigger,
        ) -> snipdeck_store::Result<Option<ItemId>> {
            self.0.trigger_holder(trigger)
        }
        fn set_trigger(
            &mut self,
            item: ItemId,
            trigger: Option<&snipdeck_core::Trigger>,
        ) -> snipdeck_store::Result<Vec<snipdeck_core::Trigger>> {
            self.0.set_trigger(item, trigger)
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
        assert!(
            used.count_failed_line()
                .is_some_and(|l| l.starts_with("use not counted:") && l.contains("disk full"))
        );
        // A normal use reports nothing.
        let ok = w.run(&request).expect("used");
        assert_eq!(ok.count_failed, None);
        assert_eq!(ok.count_failed_line(), None);
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

    fn texts_written(w: &World) -> Vec<Vec<Data>> {
        w.clipboard
            .written
            .iter()
            .map(|p| p.data().to_vec())
            .collect()
    }

    #[test]
    fn paste_as_upper_is_plain_with_caret_at_end() {
        // A template with a caret and an HTML twin: the transform keeps the text only and
        // leaves the caret where the paste ends.
        let mut w = World::new();
        let id = w.add(ItemKind::Template, false, "Hi {cursor} straße");
        let mut request = World::request(id, UseMode::Paste, PASTE);
        request.transform = Some(Transform::Upper);
        let used = w.run(&request).expect("used");
        assert_eq!(texts_written(&w), [[Data::Text("HI  STRASSE".into())]]);
        assert_eq!(used.cursor_from_end, None);
        assert!(
            w.keyboard.sent.iter().flatten().all(|e| e.key != Key::Left),
            "no caret moves"
        );

        // Rich formats are left behind.
        let mut w = World::new();
        let id = w.add(ItemKind::Text, false, "plain words");
        let existing = w.store.formats(id).expect("formats");
        let mut formats: Vec<Format> = existing.into_iter().cloned().collect();
        let html = snipdeck_core::Content::Inline(
            snipdeck_core::Inline::new(b"<b>plain words</b>".to_vec()).expect("inline"),
        );
        formats.push(Format::Html(html));
        let item = w.store.item(id).expect("item");
        w.store
            .update_item(&item, Some(&Formats::new(formats).expect("formats")))
            .expect("set");
        let mut request = World::request(id, UseMode::Paste, PASTE);
        w.run(&request).expect("used");
        assert_eq!(
            w.clipboard.written[0].data().len(),
            2,
            "html goes along untransformed"
        );
        request.transform = Some(Transform::Title);
        w.run(&request).expect("used");
        assert_eq!(
            w.clipboard.written[1].data(),
            [Data::Text("Plain Words".into())]
        );
    }

    #[test]
    fn paste_as_on_a_sensitive_item_still_goes_out_private() {
        let mut w = World::new();
        let id = w.add(ItemKind::Text, true, "hunter2");
        let mut request = World::request(id, UseMode::Copy, Route::Copy(CopyReason::NoTarget));
        request.transform = Some(Transform::Upper);
        w.run(&request).expect("used");
        assert!(w.clipboard.written[0].is_private());
        assert_eq!(
            w.clipboard.written[0].data(),
            [Data::Text("HUNTER2".into())]
        );
    }

    #[test]
    fn a_paste_as_on_a_picture_is_refused_not_pasted() {
        let mut w = World::new();
        let id = w.add(ItemKind::Text, false, "x");
        let formats = vec![Format::Png(snipdeck_core::Content::Inline(
            snipdeck_core::Inline::new(vec![1, 2, 3]).expect("inline"),
        ))];
        let item = w.store.item(id).expect("item");
        w.store
            .update_item(&item, Some(&Formats::new(formats).expect("formats")))
            .expect("set");
        let mut request = World::request(id, UseMode::Paste, PASTE);
        request.transform = Some(Transform::Upper);
        assert!(matches!(w.run(&request), Err(UseError::NotText)));
        assert_eq!(w.clipboard.written.len(), 0);
    }

    #[test]
    fn uuids_in_a_template_come_from_the_request_and_differ_per_occurrence() {
        let mut w = World::new();
        let id = w.add(ItemKind::Template, false, "{uuid}|{uuid}");
        let mut request = World::request(id, UseMode::Copy, Route::Copy(CopyReason::NoTarget));
        request.uuids = Some(vec![[0x11; 16], [0x22; 16]]);
        w.run(&request).expect("used");
        assert_eq!(
            w.clipboard.written[0].data(),
            [Data::Text(
                "11111111-1111-4111-9111-111111111111|22222222-2222-4222-a222-222222222222".into()
            )]
        );
        // Without bytes from the caller the system draws them: two distinct v4 ids.
        let mut w = World::new();
        let id = w.add(ItemKind::Template, false, "{uuid}|{uuid}");
        w.run(&World::request(
            id,
            UseMode::Copy,
            Route::Copy(CopyReason::NoTarget),
        ))
        .expect("used");
        let Data::Text(text) = &w.clipboard.written[0].data()[0] else {
            panic!("text")
        };
        let (a, b) = text.split_once('|').expect("two");
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(a.as_bytes()[14], b'4');
    }

    #[test]
    fn a_template_with_a_cursor_pastes_then_moves_the_caret_back() {
        let mut w = World::new();
        let id = w.add(ItemKind::Template, false, "Hi {cursor} there");
        w.run(&World::request(id, UseMode::Paste, PASTE))
            .expect("used");
        let lefts: Vec<_> = w
            .keyboard
            .sent
            .last()
            .expect("sent")
            .iter()
            .map(|e| (e.key, e.down))
            .collect();
        let expected: Vec<_> = (0..6)
            .flat_map(|_| [(Key::Left, true), (Key::Left, false)])
            .collect();
        assert_eq!(lefts, expected, "six Left pairs for ' there'");

        let mut w = World::new();
        let id = w.add(ItemKind::Template, false, "Hi {cursor} there");
        let copy = World::request(id, UseMode::Copy, Route::Copy(CopyReason::NoTarget));
        w.run(&copy).expect("used");
        assert!(w.keyboard.sent.is_empty(), "a copy sends no keys");
    }

    /// The Backspace, Tab and Enter presses of every batch the keyboard was sent, in order.
    fn edit_keys(w: &World) -> Vec<Key> {
        w.keyboard
            .sent
            .iter()
            .flatten()
            .filter(|e| e.down && matches!(e.key, Key::Backspace | Key::Tab | Key::Enter))
            .map(|e| e.key)
            .collect()
    }

    fn written_text(w: &World) -> Vec<String> {
        w.clipboard
            .written
            .iter()
            .filter_map(|p| match p.data().first() {
                Some(Data::Text(t)) => Some(t.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn key_stops_paste_the_text_in_pieces_with_the_keys_between() {
        let mut w = World::new();
        let id = w.add(ItemKind::Template, false, "Ann{key:tab}Lee{key:enter}Done");
        let mut request = World::request(id, UseMode::Paste, PASTE);
        request.erase = 2;
        let used = w.run(&request).expect("used");
        assert_eq!(used.outcome, UseOutcome::Pasted);
        assert_eq!(written_text(&w), ["Ann", "Lee", "Done"]);
        // The typed trigger is erased once, before the first piece.
        assert_eq!(
            edit_keys(&w),
            [Key::Backspace, Key::Backspace, Key::Tab, Key::Enter]
        );
    }

    #[test]
    fn a_template_that_starts_with_a_key_erases_the_trigger_before_it() {
        let mut w = World::new();
        let id = w.add(ItemKind::Template, false, "{key:tab}Lee");
        let mut request = World::request(id, UseMode::Paste, PASTE);
        request.erase = 1;
        w.run(&request).expect("used");
        assert_eq!(written_text(&w), ["Lee"]);
        assert_eq!(edit_keys(&w), [Key::Backspace, Key::Tab]);
    }

    #[test]
    fn a_copy_keeps_the_text_whole_and_presses_no_key() {
        let mut w = World::new();
        let id = w.add(ItemKind::Template, false, "Ann{key:tab}Lee");
        let copy = World::request(id, UseMode::Copy, Route::Copy(CopyReason::NoTarget));
        w.run(&copy).expect("used");
        assert_eq!(written_text(&w), ["AnnLee"]);
        assert_eq!(edit_keys(&w).len(), 0);
        // A copy route with a paste mode is the same: there is nowhere to press a key.
        let mut w = World::new();
        let id = w.add(ItemKind::Template, false, "Ann{key:tab}Lee");
        let routed = World::request(id, UseMode::Paste, Route::Copy(CopyReason::NoTarget));
        w.run(&routed).expect("used");
        assert_eq!(written_text(&w), ["AnnLee"]);
        assert_eq!(edit_keys(&w).len(), 0);
    }

    #[test]
    fn a_failed_key_press_leaves_the_text_copied_for_the_user() {
        let mut w = World::new();
        let id = w.add(ItemKind::Template, false, "Ann{key:tab}Lee");
        w.keyboard.fail = true;
        let used = w
            .run(&World::request(id, UseMode::Paste, PASTE))
            .expect("used");
        assert!(
            matches!(used.outcome, UseOutcome::CopiedInstead(_)),
            "{:?}",
            used.outcome
        );
        assert_eq!(edit_keys(&w).len(), 0);
    }

    #[test]
    fn nested_snippets_resolve_by_title_or_trigger_and_refuse_what_is_unsafe() {
        let mut w = World::new();
        w.add_titled("Sig", ItemKind::Text, false, "Best {regards}");
        w.add_titled("Wrapper", ItemKind::Template, false, "Hi,\n{snippet:sig}");
        let wrapper = w.store.all_items().expect("items")[1].id;
        let used = w
            .run(&World::request(wrapper, UseMode::Paste, PASTE))
            .expect("used");
        assert_eq!(used.outcome, UseOutcome::Pasted);
        // The plain text is taken literally: its braces are not placeholders.
        assert_eq!(written_text(&w), ["Hi,\nBest {regards}"]);

        // A Sensitive item is not a snippet.
        let mut w = World::new();
        w.add_titled("Secret", ItemKind::Text, true, "hunter2");
        let id = w.add_titled("Uses", ItemKind::Template, false, "{snippet:secret}");
        assert!(matches!(
            w.run(&World::request(id, UseMode::Paste, PASTE)),
            Err(UseError::Template(_))
        ));
        assert_eq!(w.clipboard.written.len(), 0);

        // Two items with one title are ambiguous, not silently the first.
        let mut w = World::new();
        w.add_titled("Dup", ItemKind::Text, false, "one");
        w.add_titled("dup ", ItemKind::Text, false, "two");
        let id = w.add_titled("Uses", ItemKind::Template, false, "{snippet:DUP}");
        assert!(matches!(
            w.run(&World::request(id, UseMode::Paste, PASTE)),
            Err(UseError::Template(_))
        ));

        // A snippet that contains itself is a cycle, not a hang.
        let mut w = World::new();
        let id = w.add_titled("Loop", ItemKind::Template, false, "a{snippet:loop}");
        assert!(matches!(
            w.run(&World::request(id, UseMode::Paste, PASTE)),
            Err(UseError::Template(_))
        ));
    }

    #[test]
    fn clipboard_history_positions_and_the_selection_reach_the_render() {
        let mut w = World::new();
        let id = w.add(
            ItemKind::Template,
            false,
            "{clipboard}|{clipboard:2}|{clipboard:3}|{selection}",
        );
        let mut request = World::request(id, UseMode::Paste, PASTE);
        request.clipboard_text = Some("now".into());
        request.clipboard_history = vec!["before".into(), "older".into()];
        request.selection = Some("picked".into());
        w.run(&request).expect("used");
        assert_eq!(written_text(&w), ["now|before|older|picked"]);
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
    fn every_prompt_kind_opens_the_form_until_it_is_answered() {
        for text in [
            "Hi{optional:PS}, P.S.{/optional}",
            "Due {pickdate:Due}",
            "Size {choice:Size|S|M}",
            "Hi {field:Name|there}",
            "{multiline:Body|Dear all}",
        ] {
            let mut w = World::new();
            let id = w.add(ItemKind::Template, false, text);
            let mut request = World::request(id, UseMode::Copy, Route::Copy(CopyReason::NoTarget));
            assert!(
                matches!(w.run(&request), Err(UseError::NeedsAnswers(_))),
                "{text} asks first"
            );
            let prompts = Template::parse(text).expect("parses").prompts();
            request.answers = prompts
                .iter()
                .map(|p| (p.name.clone(), String::new()))
                .collect();
            w.run(&request).expect("answered, it renders");
        }
    }

    #[test]
    fn plain_items_have_no_template_source() {
        let mut w = World::new();
        let id = w.add(ItemKind::Text, false, "{not a placeholder}");
        assert_eq!(template_source(&w.store, id).expect("source"), None);
    }
}
