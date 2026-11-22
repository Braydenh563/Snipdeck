//! The item editor's half of the [`Controller`]: opening it on a new or an existing item, the
//! checks on Save, the write (with the encryption key made silently on the first sensitive
//! item), the draft kept every two seconds, and the files chosen for Image and File items.
//!
//! Slint-free like the rest of the controller. [`editor_binding`](crate::editor_binding) copies
//! an [`ItemEditorView`] into the window and calls these methods from its callbacks; the window
//! keeps the typed text and hands it over as an [`EditorInput`] on Save and for the draft.
//!
//! Only one item editor is open at a time. Asking for another while one is up brings the open
//! one to the front: what is typed in it is not thrown away to make room (and it is in the
//! draft folder within two seconds anyway).
//!
//! Nothing here logs or traces item text, a path or a title.

use std::{collections::BTreeSet, path::PathBuf, rc::Rc, sync::Arc};

use crate::item_editor;

use snipdeck_core::{
    Format, Formats, Item, ItemId, ItemKind, ListId, Tag, TagId, template::Locale,
};
use snipdeck_platform::file_dialog::FilePicker;
use snipdeck_store::{
    BlobStore, ItemRepo,
    crypto::{CryptoError, KeyProvider},
};

use super::{Controller, Library, Toast, ToastKind, ToastMessage, failed};
use crate::{
    drafts::{Draft, DraftKey, Drafts},
    host::ClipboardText,
    item_editor::{
        AttachmentView, EditorInput, EditorProblem, ItemEditorView, SaveFailure, Source,
    },
    item_preview::{self, CardPreview, PreviewLook, PreviewWords},
};

use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};

mod open;
mod save;

/// What the item editor needs from the machine, set by the host.
#[derive(Default)]
pub(super) struct EditorEnv {
    /// Where drafts go (none in the demo, which has no data folder).
    drafts: Option<Drafts>,
    /// Wraps the library key (DPAPI on Windows).
    pub(super) keys: Option<Arc<dyn KeyProvider>>,
    /// The file dialog. Shared so the window can ask it without holding the controller: the
    /// dialog runs its own message loop, and the controller must be free meanwhile.
    picker: Option<Rc<dyn FilePicker>>,
    /// Reads what the editor window shows right now, so the lock does not depend on the last
    /// two-second tick. Must not touch the controller.
    text_source: Option<Rc<dyn Fn() -> Option<EditorInput>>>,
}

/// What the editor edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemTarget {
    New,
    Item(ItemId),
}

/// A tag the editor can tick.
#[derive(Debug, Clone)]
struct TagEntry {
    id: TagId,
    name: String,
    checked: bool,
    /// Typed in the editor: made when the item is saved.
    pending: bool,
}

#[derive(Debug, Clone, Default)]
struct Problems {
    title: Option<EditorProblem>,
    content: Option<EditorProblem>,
    source: Option<EditorProblem>,
    privacy: Option<EditorProblem>,
    trigger: Option<EditorProblem>,
    banner: Option<EditorProblem>,
}

/// An open item editor.
#[derive(Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent flags of one open editor, not a state"
)]
pub(super) struct ItemEditorSlot {
    seq: u64,
    raise: u64,
    target: ItemTarget,
    /// The values to start the window from (the item's, or the draft's).
    fields: EditorInput,
    lists: Vec<(ListId, String)>,
    tags: Vec<TagEntry>,
    problems: Problems,
    original_kind: ItemKind,
    original_sensitive: bool,
    /// The Trigger field as typed (empty: none); the window sends it with
    /// [`item_editor_set_trigger`](Controller::item_editor_set_trigger).
    trigger: String,
    /// The item's trigger when the editor opened (empty: none).
    original_trigger: String,
    /// The saved text of a text item up to the editing cap: what "changed" is measured against.
    /// A sensitive item's text is held here while its editor is open and no longer.
    original_text: Option<String>,
    /// The item keeps HTML or RTF next to its text: saving changed text drops them, and
    /// "Remove formatting" drops them alone.
    formatted: bool,
    /// `updated_at` of the item when it was opened, as text: a draft of an older state is stale.
    stamp: Option<String>,
    attachment: Option<AttachmentView>,
    restored: bool,
    /// What the editor starts as: a draft equal to it is no draft.
    baseline: Draft,
    last_draft: Option<Draft>,
    draft_cleared: bool,
    /// This editor neither reads nor writes a draft, and leaves the draft of the same key alone
    /// (a pasted secret opens beside a half-typed new item's draft).
    no_draft: bool,
    draft_failed_told: bool,
    /// The Sensitive switch as the window last reported it (the two-second tick): an editor of a
    /// plain item becomes sensitive when the user turns it on.
    live_sensitive: bool,
    /// The fields of a sensitive editor as the last tick saw them, so a lock can keep them.
    /// Always `None` for an editor with nothing sensitive in it.
    latest: Option<EditorInput>,
}

/// What the lock did to a sensitive editor that was open.
pub(super) enum Parked {
    /// Kept sealed in memory; it comes back after the unlock.
    Kept,
    /// Closed, its text dropped (no key to seal it with).
    Dropped,
}

/// A sensitive editor the lock took off the screen: its window fields and the title and text
/// sealed under the data key (the key itself is gone while locked). Given back after the unlock.
pub(super) struct ParkedEditor {
    /// The editor with its title and text blanked and the saved text forgotten.
    slot: ItemEditorSlot,
    /// The id the envelope is bound to.
    seal_id: ItemId,
    /// Title and text, sealed.
    sealed: Vec<u8>,
}

/// Title and text as one sealed message: the title's length, the title, the text.
fn pack_texts(title: &str, content: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + title.len() + content.len());
    out.extend_from_slice(&u32::try_from(title.len()).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(title.as_bytes());
    out.extend_from_slice(content.as_bytes());
    out
}

fn unpack_texts(bytes: &[u8]) -> Option<(String, String)> {
    let (len, rest) = bytes.split_at_checked(4)?;
    let len = usize::try_from(u32::from_le_bytes(len.try_into().ok()?)).ok()?;
    let (title, content) = rest.split_at_checked(len)?;
    Some((
        String::from_utf8(title.to_vec()).ok()?,
        String::from_utf8(content.to_vec()).ok()?,
    ))
}

impl ItemEditorSlot {
    /// Whether this editor holds, or is about to hold, the text of a sensitive item.
    fn sensitive(&self) -> bool {
        self.original_sensitive || self.fields.sensitive || self.live_sensitive
    }

    fn draft_key(&self) -> DraftKey {
        match self.target {
            ItemTarget::New => DraftKey::New,
            ItemTarget::Item(id) => DraftKey::Item(id),
        }
    }

    fn is_new(&self) -> bool {
        self.target == ItemTarget::New
    }

    /// The draft of `input` in this editor.
    fn draft_of(&self, input: &EditorInput) -> Draft {
        let list = self
            .lists
            .get(input.list_index)
            .map(|(id, _)| id.to_string())
            .unwrap_or_default();
        let ticked = |pending: bool| {
            self.tags
                .iter()
                .filter(move |t| t.checked && t.pending == pending)
        };
        Draft::new(
            self.stamp.clone(),
            input.kind,
            input.title.clone(),
            input.content.clone(),
            list,
            ticked(false).map(|t| t.id.to_string()).collect(),
            ticked(true).map(|t| t.name.clone()).collect(),
            input.source_path.clone(),
        )
        .with_label(input.label)
    }
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    // ---- Setting up ----------------------------------------------------------------------------

    /// Keeps the item editor's drafts in `drafts` from now on.
    pub fn set_drafts(&mut self, drafts: Drafts) {
        self.editor_env.drafts = Some(drafts);
    }

    /// Wraps the library's encryption key with `provider` (DPAPI on Windows).
    pub fn set_key_provider(&mut self, provider: Box<dyn KeyProvider>) {
        self.editor_env.keys = Some(Arc::from(provider));
    }

    /// Lets the lock read the editor window's text at that moment (it must not borrow the
    /// controller). Without it the last autosave tick supplies the text.
    pub fn set_editor_text_source(&mut self, source: Box<dyn Fn() -> Option<EditorInput>>) {
        self.editor_env.text_source = Some(Rc::from(source));
    }

    /// Lets the item editor open a file dialog with `picker`.
    pub fn set_file_picker(&mut self, picker: Box<dyn FilePicker>) {
        self.editor_env.picker = Some(Rc::from(picker));
    }

    /// The file dialog, if the machine has one. Ask it with the controller released, then hand
    /// the answer to [`item_editor_picked`](Self::item_editor_picked).
    #[must_use]
    pub fn file_picker(&self) -> Option<Rc<dyn FilePicker>> {
        self.editor_env.picker.clone()
    }

    /// Opens the library key at start-up when one was made on an earlier run, so sensitive items
    /// can be used and edited. A key that does not open is a toast (the items stay as they are).
    ///
    /// A library from before encryption was free for every plan can hold sensitive items (or
    /// Recent entries, before version 5) as plain text and no key: when the machine can protect
    /// a key, it is made now, which encrypts them. Without key protection they stay as they are, and no toast is shown.
    pub fn unlock_library(&mut self) {
        let migrate = self.editor_env.keys.is_some()
            && !self.store.is_unlocked()
            && !matches!(self.store.key_ring_exists(), Ok(true))
            && (matches!(self.store.has_plain_sensitive(), Ok(true))
                || matches!(self.store.has_plain_recent(), Ok(true)));
        self.refresh_master();
        match self.ensure_key(migrate) {
            // A master password is set: the library waits for it, silently. The unlock window
            // opens when a sensitive item is first needed.
            Ok(()) | Err(snipdeck_store::Error::Crypto(CryptoError::PasswordRequired)) => {}
            Err(e) => self.show_toast(open_failed(&e)),
        }
        // Whether the library is locked has just been settled: the clipboard listener's gate
        // follows it.
        self.sync_recent_gate();
    }

    // ---- Reading -------------------------------------------------------------------------------

    /// What the editor window shows now, if one is open.
    #[must_use]
    pub fn item_editor_view(&self) -> Option<ItemEditorView> {
        let slot = self.item_editor.as_ref()?;
        let look = self.look();
        Some(ItemEditorView {
            seq: slot.seq,
            raise: slot.raise,
            is_new: slot.is_new(),
            dark: look.dark,
            high_contrast: look.high_contrast,
            accent: self.accent(),
            reduced_motion: look.reduce_motion,
            fields: slot.fields.clone(),
            list_names: slot.lists.iter().map(|(_, n)| n.clone()).collect(),
            tags: slot
                .tags
                .iter()
                .map(|t| (t.name.clone(), t.checked))
                .collect(),
            title_problem: slot.problems.title.clone(),
            content_problem: slot.problems.content.clone(),
            source_problem: slot.problems.source.clone(),
            privacy_problem: slot.problems.privacy.clone(),
            trigger: slot.trigger.clone(),
            trigger_problem: slot.problems.trigger.clone(),
            banner: slot.problems.banner.clone(),
            formatted: slot.formatted,
            saved_text: slot
                .original_text
                .as_ref()
                .filter(|t| t.len() <= item_editor::PART_BYTES)
                .cloned()
                .unwrap_or_default(),
            attachment: slot.attachment.clone(),
            can_browse: self.editor_env.picker.is_some(),
            restored: slot.restored,
            more_open: self.settings.editor_more_open,
        })
    }

    /// The card the open editor's `input` would make, built from the unsaved fields with the
    /// controller's clock, time zone and language. `None` when no editor is open. Reads nothing
    /// from the store and writes nothing.
    #[must_use]
    pub fn item_editor_preview(
        &self,
        input: &EditorInput,
        words: PreviewWords<'_>,
    ) -> Option<CardPreview> {
        self.item_editor.as_ref()?;
        Some(item_preview::card_of(
            input,
            self.clock.now(),
            &self.tz,
            Locale::for_tag(&self.settings.language),
            words,
        ))
    }

    /// How the user's own Panel and Main list draw a card, for the editor's Preview.
    #[must_use]
    pub fn item_editor_preview_look(&self) -> PreviewLook {
        PreviewLook {
            panel_layout: self.settings.panel_layout,
            main_view: self.main.view_mode(),
            show_numbers: self.settings.show_numbers,
        }
    }

    // ---- Editing -------------------------------------------------------------------------------

    /// The Trigger field now holds `text` (blank: the item has no trigger). Checked on Save.
    pub fn item_editor_set_trigger(&mut self, text: &str) {
        if let Some(slot) = self.item_editor.as_mut() {
            text.trim().clone_into(&mut slot.trigger);
            slot.problems.trigger = None;
        }
    }

    /// A field was edited: refusals no longer apply.
    pub fn item_editor_edited(&mut self) {
        let Some(slot) = self.item_editor.as_mut() else {
            return;
        };
        slot.problems = Problems::default();
        // Typing in a sensitive item is use: it must not idle the library out under the
        // user's hands (the lock closes this editor).
        if slot.sensitive() {
            self.note_activity();
        }
    }

    /// The lock takes a sensitive item's editor off the screen: its title and text are sealed under
    /// the data key (so call this before the key is dropped) and kept in memory, and the editor
    /// comes back with them after the unlock. Nothing is written to disk and no draft is kept.
    /// With no key to seal with the editor is closed and its text dropped. Returns what happened
    /// to an editor that was open: `None` when none held anything sensitive.
    pub(super) fn park_sensitive_item_editor(&mut self) -> Option<Parked> {
        let mut slot = self.item_editor.take_if(|slot| slot.sensitive())?;
        // Ask the window for its text now: the tick may be up to two seconds old.
        if let Some(live) = self.editor_env.text_source.as_ref().and_then(|read| read()) {
            slot.latest = Some(live);
        }
        let input = slot.latest.take().unwrap_or_else(|| slot.fields.clone());
        let seal_id = ItemId::generate();
        let sealed = self.store.duplicate_data_key().and_then(|key| {
            snipdeck_store::crypto::encrypt_item(
                &key,
                &seal_id,
                &pack_texts(&input.title, &input.content),
            )
            .ok()
        });
        let Some(sealed) = sealed else {
            self.clear_draft(&slot);
            return Some(Parked::Dropped);
        };
        slot.fields = EditorInput {
            title: String::new(),
            content: String::new(),
            ..input
        };
        slot.original_text = None;
        self.clear_draft(&slot);
        self.parked_editor = Some(ParkedEditor {
            slot,
            seal_id,
            sealed,
        });
        Some(Parked::Kept)
    }

    /// After an unlock: opens the parked editor again with what was typed, marked as unsaved.
    /// Returns `false` when something was parked but could not come back (the item is gone, the
    /// text does not open, or another editor is open now).
    pub(super) fn restore_parked_editor(&mut self) -> bool {
        let Some(parked) = self.parked_editor.take() else {
            return true;
        };
        if self.item_editor.is_some() {
            return false;
        }
        let ParkedEditor {
            mut slot,
            seal_id,
            sealed,
        } = parked;
        let texts = self
            .store
            .duplicate_data_key()
            .and_then(|key| snipdeck_store::crypto::decrypt_item(&key, &seal_id, &sealed).ok())
            .and_then(|bytes| unpack_texts(bytes.as_slice()));
        let Some((title, content)) = texts else {
            return false;
        };
        if let ItemTarget::Item(id) = slot.target {
            // What "changed" is measured against comes from the stored item again.
            let Ok(fresh) = self.existing_slot(id) else {
                return false;
            };
            slot.original_text = fresh.original_text;
            slot.attachment = fresh.attachment;
            slot.formatted = fresh.formatted;
            slot.stamp = fresh.stamp;
        }
        slot.fields.title = title;
        slot.fields.content = content;
        slot.restored = true;
        self.item_editor_seq += 1;
        slot.seq = self.item_editor_seq;
        self.item_editor = Some(slot);
        true
    }

    /// Tag `index` of the view was ticked or unticked.
    pub fn item_editor_tag_toggled(&mut self, index: usize) {
        let Some(slot) = self.item_editor.as_mut() else {
            return;
        };
        let Some(tag) = slot.tags.get_mut(index) else {
            return;
        };
        tag.checked = !tag.checked;
        if tag.pending && !tag.checked {
            // A tag that was only typed in and is now unticked is forgotten.
            slot.tags.remove(index);
        }
    }

    /// "Remove formatting": the open item keeps its text and loses its HTML and RTF at once, with
    /// the editor left open and the fields as they are. Does nothing for an item without them.
    pub fn item_editor_remove_formatting(&mut self) {
        let Some(slot) = self.item_editor.as_ref() else {
            return;
        };
        let ItemTarget::Item(id) = slot.target else {
            return;
        };
        if !slot.formatted {
            return;
        }
        let done = self.store.formats(id).and_then(|formats| {
            let kept: Vec<Format> = formats
                .iter()
                .filter(|f| !matches!(f, Format::Html(_) | Format::Rtf(_)))
                .cloned()
                .collect();
            let item = self.store.item(id)?;
            self.store
                .atomically(|b| b.update_item(&item, Some(&Formats::new(kept.clone())?)))
        });
        match done {
            Ok(()) => {
                if let Some(slot) = self.item_editor.as_mut() {
                    slot.formatted = false;
                }
                self.library_changed();
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// A new tag name was entered: the tag with that name (any letter case) is ticked, or a new
    /// one is made when the item is saved.
    pub fn item_editor_tag_added(&mut self, name: &str) {
        let Some(slot) = self.item_editor.as_mut() else {
            return;
        };
        slot.problems.banner = Self::add_pending_tag(&mut slot.tags, name).err();
    }

    /// The answer of the file dialog opened by Browse: returns the path to put in the field. A
    /// dialog that could not be shown, or a path that is not text, says so in the editor.
    pub fn item_editor_picked(
        &mut self,
        answer: Result<Option<PathBuf>, snipdeck_platform::error::Error>,
    ) -> Option<String> {
        let slot = self.item_editor.as_mut()?;
        match answer {
            Ok(Some(path)) => {
                if let Some(text) = path.to_str() {
                    Some(text.to_owned())
                } else {
                    slot.problems.source = Some(EditorProblem::SourceBadName);
                    None
                }
            }
            Ok(None) => None,
            Err(_) => {
                slot.problems.banner = Some(EditorProblem::BrowseFailed);
                None
            }
        }
    }

    // ---- Drafts --------------------------------------------------------------------------------

    /// The two-second tick: writes the draft if the fields differ from what the editor started
    /// with and from the last draft, removes it when they are back to the start. Nothing is
    /// written for a sensitive item (see [`crate::drafts`]), a text over a draft's size, or without a
    /// drafts folder. A failed write is reported once per editor. Returns whether something the
    /// windows show changed (that report).
    pub fn item_editor_autosave(&mut self, input: &EditorInput) -> bool {
        let Some(slot) = self.item_editor.as_mut() else {
            return false;
        };
        slot.live_sensitive = input.sensitive;
        slot.latest = (input.sensitive || slot.original_sensitive).then(|| input.clone());
        if slot.no_draft {
            return false;
        }
        let Some(drafts) = self.editor_env.drafts.as_ref() else {
            return false;
        };
        if input.content.len() > item_editor::DRAFT_TEXT_MAX_BYTES {
            return false;
        }
        let key = slot.draft_key();
        if input.sensitive || slot.original_sensitive {
            if !slot.draft_cleared {
                slot.draft_cleared = true;
                slot.last_draft = None;
                if let Err(e) = drafts.clear(key) {
                    crate::trace!("draft not removed: {e}");
                }
            }
            return false;
        }
        slot.draft_cleared = false;
        let draft = slot.draft_of(input);
        if draft == slot.baseline {
            if slot.last_draft.take().is_some()
                && let Err(e) = drafts.clear(key)
            {
                crate::trace!("draft not removed: {e}");
            }
            return false;
        }
        if slot.last_draft.as_ref() == Some(&draft) {
            return false;
        }
        match drafts.save(key, &draft) {
            Ok(()) => {
                slot.last_draft = Some(draft);
                false
            }
            // Encoded past the drafts file cap (control characters escape to six bytes): no
            // draft, as for a text over DRAFT_TEXT_MAX_BYTES, and nothing to warn about.
            Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => false,
            Err(e) => {
                crate::trace!("draft not written: {e}");
                if slot.draft_failed_told {
                    return false;
                }
                slot.draft_failed_told = true;
                slot.problems.banner = Some(EditorProblem::DraftNotSaved);
                self.show_toast(Toast {
                    kind: ToastKind::Neutral,
                    message: ToastMessage::DraftNotSaved,
                });
                true
            }
        }
    }

    fn clear_draft(&self, slot: &ItemEditorSlot) {
        if slot.no_draft {
            return;
        }
        if let Some(drafts) = self.editor_env.drafts.as_ref()
            && let Err(e) = drafts.clear(slot.draft_key())
        {
            crate::trace!("draft not removed: {e}");
        }
    }

    // ---- Closing -------------------------------------------------------------------------------

    /// Cancel, after the window asked about unsaved changes: closes the editor and drops its
    /// draft.
    pub fn item_editor_cancel(&mut self) {
        if let Some(slot) = self.item_editor.take() {
            self.clear_draft(&slot);
        }
    }

    /// The window that opened the editor closed: the editor closes without asking and keeps what
    /// was typed as its draft (the same draft the two-second tick writes), so opening the item
    /// again restores it. A sensitive item's text is never kept.
    pub fn item_editor_park(&mut self, input: &EditorInput) {
        self.item_editor_autosave(input);
        if let Some(slot) = self.item_editor.take()
            && slot.sensitive()
        {
            self.clear_draft(&slot);
        }
    }

    /// Delete on an existing item: it moves to the Trash with an Undo toast, and the editor
    /// closes.
    pub fn item_editor_delete(&mut self) {
        let Some(ItemTarget::Item(id)) = self.item_editor.as_ref().map(|s| s.target) else {
            return;
        };
        self.item_editor_cancel();
        self.trash_items(&[id]);
    }
}

/// The tags to write for an item.
#[derive(Default)]
struct ResolvedTags {
    ticked: BTreeSet<TagId>,
    created: Vec<Tag>,
}

fn preview_of(is_text: bool, text: Option<&str>, sensitive: bool) -> String {
    match (is_text, text) {
        (true, Some(t)) => Item::preview_of(t, sensitive),
        _ => String::new(),
    }
}

/// The formats of a new text, Image or File item, with their bytes stored through `b`. A
/// sensitive text goes through `put_sensitive`, which keeps a large one off the disk until it
/// is written encrypted.
fn build_formats(
    b: &mut snipdeck_store::Batch<'_>,
    sensitive: bool,
    text: Option<&str>,
    source: Option<&Source>,
) -> snipdeck_store::Result<Formats> {
    let format = match (text, source) {
        (Some(t), _) => {
            let bytes = t.as_bytes().to_vec();
            let content = if sensitive {
                b.put_sensitive(bytes)?
            } else {
                b.put(bytes)?
            };
            Format::Text(content)
        }
        (None, Some(s)) if s.kind == ItemKind::Image => Format::Png(b.put(s.bytes.clone())?),
        (None, Some(s)) => {
            let content = b.put(s.bytes.clone())?;
            Format::Files(vec![snipdeck_core::FileEntry::new(&s.name, content)?])
        }
        (None, None) => {
            return Err(snipdeck_store::Error::Invalid(
                "an item needs content".to_owned(),
            ));
        }
    };
    Ok(Formats::new(vec![format])?)
}

/// The toast for an item that could not be written.
fn save_failed_message(failure: SaveFailure) -> ToastMessage {
    match failure {
        SaveFailure::Locked => ToastMessage::Locked,
        SaveFailure::KeyUnavailable => ToastMessage::KeyUnavailable,
        SaveFailure::ItemGone => ToastMessage::ItemGone,
        SaveFailure::ListGone => ToastMessage::ListGone,
        SaveFailure::Storage => ToastMessage::StorageFailed,
        SaveFailure::Other => ToastMessage::NotSaved,
    }
}

/// The toast for an item that could not be opened.
fn open_failed(error: &snipdeck_store::Error) -> Toast {
    let message = match SaveFailure::of(error) {
        SaveFailure::Locked => ToastMessage::Locked,
        SaveFailure::KeyUnavailable => ToastMessage::KeyUnavailable,
        SaveFailure::ItemGone | SaveFailure::ListGone => ToastMessage::ItemGone,
        SaveFailure::Storage | SaveFailure::Other => ToastMessage::NotOpened,
    };
    Toast {
        kind: ToastKind::Danger,
        message,
    }
}
