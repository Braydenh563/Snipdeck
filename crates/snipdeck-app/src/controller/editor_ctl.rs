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

use snipdeck_core::{Feature, Format, Formats, Item, ItemId, ItemKind, ListId, Tag, TagId};
use snipdeck_platform::file_dialog::FilePicker;
use snipdeck_store::{
    BlobStore,
    crypto::{CryptoError, KeyProvider},
};

use super::{Controller, Library, Toast, ToastKind, ToastMessage};
use crate::{
    drafts::{Draft, DraftKey, Drafts},
    host::ClipboardText,
    item_editor::{
        AttachmentView, EditorInput, EditorProblem, ItemEditorView, SaveFailure, Source,
    },
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
    /// The saved text of a text item up to the editing cap: what "changed" is measured against.
    /// A sensitive item's text is held here while its editor is open and no longer.
    original_text: Option<String>,
    /// `updated_at` of the item when it was opened, as text: a draft of an older state is stale.
    stamp: Option<String>,
    /// The size of a text over the editing cap, which is shown read-only.
    large: Option<u64>,
    attachment: Option<AttachmentView>,
    restored: bool,
    /// What the editor starts as: a draft equal to it is no draft.
    baseline: Draft,
    last_draft: Option<Draft>,
    draft_cleared: bool,
    draft_failed_told: bool,
    /// The Sensitive switch as the window last reported it (the two-second tick): an editor of a
    /// plain item becomes sensitive when the user turns it on.
    live_sensitive: bool,
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
            banner: slot.problems.banner.clone(),
            large: slot.large,
            attachment: slot.attachment.clone(),
            prompted_available: self.entitlements.allows(Feature::PromptedFields),
            can_browse: self.editor_env.picker.is_some(),
            restored: slot.restored,
            more_open: self.settings.editor_more_open,
        })
    }

    // ---- Editing -------------------------------------------------------------------------------

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

    /// Closes the item editor if it holds a sensitive item's text, as the lock does: the text is
    /// dropped, not saved, and no draft is kept. Returns whether one was closed.
    pub(super) fn close_sensitive_item_editor(&mut self) -> bool {
        if !self
            .item_editor
            .as_ref()
            .is_some_and(ItemEditorSlot::sensitive)
        {
            return false;
        }
        if let Some(slot) = self.item_editor.take() {
            self.clear_draft(slot.draft_key());
        }
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
    /// written for a sensitive item (see [`crate::drafts`]), a read-only text, or without a
    /// drafts folder. A failed write is reported once per editor. Returns whether something the
    /// windows show changed (that report).
    pub fn item_editor_autosave(&mut self, input: &EditorInput) -> bool {
        let Some(slot) = self.item_editor.as_mut() else {
            return false;
        };
        slot.live_sensitive = input.sensitive;
        let Some(drafts) = self.editor_env.drafts.as_ref() else {
            return false;
        };
        if slot.large.is_some() {
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

    fn clear_draft(&self, key: DraftKey) {
        if let Some(drafts) = self.editor_env.drafts.as_ref()
            && let Err(e) = drafts.clear(key)
        {
            crate::trace!("draft not removed: {e}");
        }
    }

    // ---- Closing -------------------------------------------------------------------------------

    /// Cancel, after the window asked about unsaved changes: closes the editor and drops its
    /// draft.
    pub fn item_editor_cancel(&mut self) {
        if let Some(slot) = self.item_editor.take() {
            self.clear_draft(slot.draft_key());
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
            self.clear_draft(slot.draft_key());
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
