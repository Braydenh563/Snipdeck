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

use std::{collections::BTreeSet, path::PathBuf, rc::Rc};

use snipdeck_core::{
    Feature, Format, Formats, Item, ItemId, ItemKind, ListId, Name, Placement, Tag, TagId,
};
use snipdeck_platform::file_dialog::FilePicker;
use snipdeck_store::{
    BlobStore, ItemRepo, TagRepo,
    crypto::{CryptoError, KeyProvider, KeyProviderError, KeyRing},
    name_key,
};

use super::{Controller, Library, Toast, ToastKind, ToastMessage};
use crate::{
    capture,
    drafts::{Draft, DraftKey, Drafts},
    host::ClipboardText,
    item_editor::{
        self, AttachmentView, EDIT_LIMIT_BYTES, EditorInput, EditorProblem, ItemEditorView, Origin,
        SaveFailure, Source,
    },
    usecase,
};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};

/// What the item editor needs from the machine, set by the host.
#[derive(Default)]
pub(super) struct EditorEnv {
    /// Where drafts go (none in the demo, which has no data folder).
    drafts: Option<Drafts>,
    /// Wraps the library key (DPAPI on Windows).
    keys: Option<Box<dyn KeyProvider>>,
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
}

impl ItemEditorSlot {
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
        self.editor_env.keys = Some(provider);
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
    /// can be used and edited. Without a key nothing happens; a key that does not open is a toast
    /// (the items stay as they are).
    pub fn unlock_library(&mut self) {
        if let Err(e) = self.ensure_key(false) {
            self.show_toast(open_failed(&e));
        }
    }

    // ---- Opening -------------------------------------------------------------------------------

    /// Whether an item editor is open.
    #[must_use]
    pub fn item_editor_open(&self) -> bool {
        self.item_editor.is_some()
    }

    /// Opens the editor on a new item in the list `origin` is showing. With no list at all a
    /// toast says to add one.
    pub fn open_new_item(&mut self, origin: Origin) {
        if self.raise_item_editor() {
            return;
        }
        self.main_fresh();
        let Some(list) = self.new_item_list(origin) else {
            self.show_toast(Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::NoListToSave,
            });
            return;
        };
        match self.new_slot(list) {
            Ok(slot) => self.install_item_editor(slot),
            Err(e) => self.show_toast(open_failed(&e)),
        }
    }

    /// Opens the editor on item `id` (Ctrl+E, a double-click, Edit in a menu). A sensitive item
    /// needs the library key: it is opened first, and a key that cannot be opened is a toast.
    pub fn open_item_editor(&mut self, id: ItemId) {
        if self.raise_item_editor() {
            return;
        }
        match self.existing_slot(id) {
            Ok(slot) => self.install_item_editor(slot),
            Err(e) => self.show_toast(open_failed(&e)),
        }
    }

    /// Ctrl+E in the Picker on result row `index`: closes the Picker and edits that item.
    pub fn picker_edit(&mut self, index: usize) {
        let Some(id) = self.picker.as_ref().and_then(|p| p.item_at(index)) else {
            return;
        };
        self.picker = None;
        self.open_item_editor(id);
    }

    /// Brings an open editor to the front. True when there was one.
    fn raise_item_editor(&mut self) -> bool {
        match self.item_editor.as_mut() {
            Some(slot) => {
                slot.raise += 1;
                true
            }
            None => false,
        }
    }

    fn new_item_list(&self, origin: Origin) -> Option<ListId> {
        let shown = match origin {
            Origin::Main => self
                .main
                .open_list()
                .or_else(|| self.main.lists().first())
                .map(|l| l.id),
            Origin::Panel => self.model.list().map(|l| l.id),
        };
        shown.or_else(|| {
            capture::default_list(&self.store, None)
                .ok()
                .flatten()
                .map(|d| d.id)
        })
    }

    /// The lists of `list`'s board, and the position of `list` among them.
    fn list_choices(&self, list: ListId) -> snipdeck_store::Result<(Vec<(ListId, String)>, usize)> {
        let board = self.store.list(list)?.board_id;
        let lists: Vec<(ListId, String)> = self
            .store
            .lists(board)?
            .into_iter()
            .map(|l| (l.id, l.name.as_str().to_owned()))
            .collect();
        let at = lists.iter().position(|(id, _)| *id == list).unwrap_or(0);
        Ok((lists, at))
    }

    fn tag_entries(&self, ticked: &BTreeSet<TagId>) -> snipdeck_store::Result<Vec<TagEntry>> {
        Ok(self
            .store
            .tags()?
            .into_iter()
            .map(|t| TagEntry {
                checked: ticked.contains(&t.id),
                id: t.id,
                name: t.name.as_str().to_owned(),
                pending: false,
            })
            .collect())
    }

    fn new_slot(&self, list: ListId) -> snipdeck_store::Result<ItemEditorSlot> {
        let (lists, list_index) = self.list_choices(list)?;
        let fields = EditorInput {
            kind: ItemKind::Text,
            title: String::new(),
            content: String::new(),
            sensitive: false,
            list_index,
            source_path: String::new(),
        };
        let mut slot = ItemEditorSlot {
            seq: 0,
            raise: 0,
            target: ItemTarget::New,
            fields: fields.clone(),
            lists,
            tags: self.tag_entries(&BTreeSet::new())?,
            problems: Problems::default(),
            original_kind: ItemKind::Text,
            original_sensitive: false,
            original_text: None,
            stamp: None,
            large: None,
            attachment: None,
            restored: false,
            baseline: Draft::new(
                None,
                ItemKind::Text,
                String::new(),
                String::new(),
                String::new(),
                Vec::new(),
                Vec::new(),
                String::new(),
            ),
            last_draft: None,
            draft_cleared: false,
            draft_failed_told: false,
        };
        slot.baseline = slot.draft_of(&fields);
        Ok(slot)
    }

    fn existing_slot(&mut self, id: ItemId) -> snipdeck_store::Result<ItemEditorSlot> {
        let item = self.store.item(id)?;
        if item.sensitive {
            // Opening decrypts the text: the key first.
            self.ensure_key(false)?;
        }
        let (lists, list_index) = self.list_choices(item.list_id)?;
        let mut fields = EditorInput {
            kind: item.kind,
            title: item.title.clone(),
            content: String::new(),
            sensitive: item.sensitive,
            list_index,
            source_path: String::new(),
        };
        let (mut original_text, mut large, mut attachment) = (None, None, None);
        if item_editor::is_text_kind(item.kind) {
            let text = usecase::item_text(&self.store, id)
                .map_err(|e| match e {
                    usecase::UseError::Store(e) => e,
                    _ => snipdeck_store::Error::Invalid("the item's text is not text".to_owned()),
                })?
                .unwrap_or_default();
            if text.len() > EDIT_LIMIT_BYTES {
                large = u64::try_from(text.len()).ok();
                fields.content = item_editor::prefix_of(&text, EDIT_LIMIT_BYTES).to_owned();
            } else {
                fields.content.clone_from(&text);
                original_text = Some(text);
            }
        } else {
            attachment = Some(self.attachment_of(item.kind, id)?);
        }
        let tags = self.tag_entries(&item.tags)?;
        let mut slot = ItemEditorSlot {
            seq: 0,
            raise: 0,
            target: ItemTarget::Item(id),
            fields: fields.clone(),
            lists,
            tags,
            problems: Problems::default(),
            original_kind: item.kind,
            original_sensitive: item.sensitive,
            original_text,
            stamp: Some(item.updated_at.to_string()),
            large,
            attachment,
            restored: false,
            baseline: Draft::new(
                None,
                item.kind,
                String::new(),
                String::new(),
                String::new(),
                Vec::new(),
                Vec::new(),
                String::new(),
            ),
            last_draft: None,
            draft_cleared: false,
            draft_failed_told: false,
        };
        slot.baseline = slot.draft_of(&fields);
        Ok(slot)
    }

    /// What an existing Image or File item holds.
    fn attachment_of(&self, kind: ItemKind, id: ItemId) -> snipdeck_store::Result<AttachmentView> {
        let formats = self.store.formats(id)?;
        let mut view = AttachmentView {
            kind,
            name: String::new(),
            files: 0,
            bytes: 0,
            dimensions: None,
            preview: None,
        };
        for format in &formats {
            match format {
                Format::Png(content) if kind == ItemKind::Image => {
                    view.files = 1;
                    view.bytes = content.len();
                    let bytes = self.store.read(content)?;
                    view.dimensions = item_editor::png_size(&bytes);
                    view.preview = item_editor::decode_preview(&bytes);
                }
                Format::Files(entries) if kind == ItemKind::File => {
                    view.files = entries.len();
                    view.name = entries
                        .first()
                        .map(|e| e.name().to_owned())
                        .unwrap_or_default();
                    view.bytes = entries.iter().map(|e| e.content().len()).sum();
                }
                _ => {}
            }
        }
        Ok(view)
    }

    /// Gives the slot its number, restores a draft into it, and makes it the open editor.
    fn install_item_editor(&mut self, mut slot: ItemEditorSlot) {
        self.item_editor_seq += 1;
        slot.seq = self.item_editor_seq;
        self.restore_draft(&mut slot);
        self.item_editor = Some(slot);
    }

    /// Puts the draft of this editor, if there is a usable one, into the fields. A draft of an
    /// item that changed since is stale and is removed. A sensitive item never has a draft
    /// (see [`crate::drafts`]).
    fn restore_draft(&self, slot: &mut ItemEditorSlot) {
        let Some(drafts) = self.editor_env.drafts.as_ref() else {
            return;
        };
        let key = slot.draft_key();
        let Some(draft) = drafts.load(key) else {
            return;
        };
        let fresh = match slot.target {
            ItemTarget::New => true,
            ItemTarget::Item(_) => draft.base == slot.stamp,
        };
        if !fresh || slot.original_sensitive || slot.large.is_some() {
            if let Err(e) = drafts.clear(key) {
                crate::trace!("stale draft not removed: {e}");
            }
            return;
        }
        let existing_text = !slot.is_new() && item_editor::is_text_kind(slot.original_kind);
        if let Some(kind) = draft.item_kind() {
            let allowed = match slot.target {
                ItemTarget::New => true,
                ItemTarget::Item(_) => existing_text && item_editor::is_text_kind(kind),
            };
            if allowed {
                slot.fields.kind = kind;
            }
        }
        slot.fields.title.clone_from(&draft.title);
        if item_editor::is_text_kind(slot.fields.kind) {
            slot.fields.content.clone_from(&draft.content);
        }
        if let Some(at) = slot
            .lists
            .iter()
            .position(|(id, _)| id.to_string() == draft.list)
        {
            slot.fields.list_index = at;
        }
        if slot.is_new() {
            slot.fields.source_path.clone_from(&draft.source_path);
        }
        for tag in &mut slot.tags {
            tag.checked = draft.tags.contains(&tag.id.to_string());
        }
        for name in &draft.new_tags {
            // A name that is refused now (it was fine when typed) is simply not restored.
            let _ = Self::add_pending_tag(&mut slot.tags, name);
        }
        slot.restored = true;
    }

    /// Ticks the tag called `name`, making a pending one if none has that name. Returns the
    /// refusal of the name, if any.
    fn add_pending_tag(tags: &mut Vec<TagEntry>, name: &str) -> Result<(), EditorProblem> {
        let name = Name::new(name).map_err(|e| {
            EditorProblem::Tag(
                crate::main_window::NameError::of(&e)
                    .unwrap_or(crate::main_window::NameError::Empty),
            )
        })?;
        let key = name_key(name.as_str());
        if let Some(tag) = tags.iter_mut().find(|t| name_key(&t.name) == key) {
            tag.checked = true;
        } else {
            tags.push(TagEntry {
                id: TagId::generate(),
                name: name.as_str().to_owned(),
                checked: true,
                pending: true,
            });
        }
        Ok(())
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
            encryption_available: self.entitlements.allows(Feature::SensitiveEncryption),
            can_browse: self.editor_env.picker.is_some(),
            restored: slot.restored,
        })
    }

    // ---- Editing -------------------------------------------------------------------------------

    /// A field was edited: refusals no longer apply.
    pub fn item_editor_edited(&mut self) {
        if let Some(slot) = self.item_editor.as_mut() {
            slot.problems = Problems::default();
        }
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

    /// Delete on an existing item: it moves to the Trash with an Undo toast, and the editor
    /// closes.
    pub fn item_editor_delete(&mut self) {
        let Some(ItemTarget::Item(id)) = self.item_editor.as_ref().map(|s| s.target) else {
            return;
        };
        self.item_editor_cancel();
        self.trash_items(&[id]);
    }

    // ---- Saving --------------------------------------------------------------------------------

    /// Save: checks the fields, makes the library key if a sensitive item needs one, writes the
    /// item and closes the editor. A refused field stays open with the reason; a failed write
    /// stays open with a banner and a toast that say what failed, and nothing is written.
    pub fn item_editor_save(&mut self, input: &EditorInput) {
        let Some(mut slot) = self.item_editor.take() else {
            return;
        };
        slot.problems = Problems::default();
        match self.save_item(&mut slot, input) {
            Ok(list) => {
                self.clear_draft(slot.draft_key());
                self.library_changed();
                self.show_toast(Toast {
                    kind: ToastKind::Success,
                    message: ToastMessage::Saved { list },
                });
            }
            Err(()) => self.item_editor = Some(slot),
        }
    }

    /// The checks and the write of [`item_editor_save`](Self::item_editor_save). On `Err` the
    /// reasons are in `slot.problems` and a toast is up where the user needs one. `Ok` carries
    /// the name of the list the item is in.
    #[allow(
        clippy::too_many_lines,
        reason = "one straight path: check each field, provision the key, write"
    )]
    fn save_item(&mut self, slot: &mut ItemEditorSlot, input: &EditorInput) -> Result<String, ()> {
        let now = self.clock.now();
        let new = slot.is_new();
        // A new item takes any kind; an existing text item any text kind; the rest keep theirs.
        let kind = if new
            || (item_editor::is_text_kind(slot.original_kind)
                && item_editor::is_text_kind(input.kind))
        {
            input.kind
        } else {
            slot.original_kind
        };
        let is_text = item_editor::is_text_kind(kind);
        let sensitive = is_text && input.sensitive;
        let prompted = self.entitlements.allows(Feature::PromptedFields);
        let mut refused = false;

        let title = match item_editor::check_title(&input.title) {
            Ok(t) => t,
            Err(p) => {
                slot.problems.title = Some(p);
                refused = true;
                String::new()
            }
        };

        // The text to write, when it is new or changed; the file to keep, for a new Image or File.
        let mut text: Option<String> = None;
        let mut source: Option<Source> = None;
        if is_text && slot.large.is_none() {
            let checked = item_editor::check_text(&input.content).and_then(|()| match kind {
                ItemKind::Link => item_editor::check_link(&input.content),
                ItemKind::Template => {
                    let introduced = new
                        || slot.original_kind != ItemKind::Template
                        || slot.original_text.as_deref() != Some(input.content.as_str());
                    item_editor::check_template(&input.content, prompted, introduced)
                        .map(|()| input.content.clone())
                }
                _ => Ok(input.content.clone()),
            });
            match checked {
                Ok(t) => {
                    if new || slot.original_text.as_deref() != Some(t.as_str()) {
                        text = Some(t);
                    }
                }
                Err(p) => {
                    if let EditorProblem::NeedsPro(feature) = p {
                        self.show_toast(Toast {
                            kind: ToastKind::Danger,
                            message: ToastMessage::NeedsPro(feature),
                        });
                    }
                    slot.problems.content = Some(p);
                    refused = true;
                }
            }
        } else if !is_text && new {
            match item_editor::read_source(kind, &input.source_path) {
                Ok(s) => source = Some(s),
                Err(p) => {
                    slot.problems.source = Some(p);
                    refused = true;
                }
            }
        }
        let list = slot.lists.get(input.list_index).cloned();
        if refused {
            return Err(());
        }
        let Some((list_id, list_name)) = list else {
            return self.save_failed(slot, SaveFailure::ListGone);
        };

        // The key: made on the first sensitive item on Pro, opened whenever one exists.
        if sensitive || slot.original_sensitive {
            let create = sensitive && self.entitlements.allows(Feature::SensitiveEncryption);
            if let Err(e) = self.ensure_key(create) {
                return self.save_failed(slot, SaveFailure::of(&e));
            }
        }

        let tags = match self.resolve_tags(slot) {
            Ok(t) => t,
            Err(e) => return self.save_failed(slot, SaveFailure::of(&e)),
        };
        let wrote = match slot.target {
            ItemTarget::New => {
                let mut item = Item::new(list_id, title, kind, now);
                item.sensitive = sensitive;
                item.tags = tags.ticked;
                item.preview = preview_of(is_text, text.as_deref(), sensitive);
                self.store.atomically(|b| {
                    for tag in &tags.created {
                        b.insert_tag(tag)?;
                    }
                    let formats = build_formats(b, sensitive, text.as_deref(), source.as_ref())?;
                    b.insert_item(&item, &formats, Placement::First)
                })
            }
            ItemTarget::Item(id) => match self.store.item(id) {
                Ok(mut item) => {
                    item.title = title;
                    item.kind = kind;
                    item.list_id = list_id;
                    item.tags = tags.ticked;
                    item.sensitive = sensitive;
                    item.updated_at = now;
                    // The preview follows the text on screen when the text itself is unchanged.
                    let shown = text.as_deref().unwrap_or(&input.content);
                    item.preview = if is_text {
                        preview_of(true, Some(shown), sensitive)
                    } else {
                        String::new()
                    };
                    self.store.atomically(|b| {
                        for tag in &tags.created {
                            b.insert_tag(tag)?;
                        }
                        let formats = match text.as_deref() {
                            Some(t) => Some(build_formats(b, sensitive, Some(t), None)?),
                            None => None,
                        };
                        b.update_item(&item, formats.as_ref())
                    })
                }
                Err(e) => Err(e),
            },
        };
        match wrote {
            Ok(()) => Ok(list_name),
            Err(e) => self.save_failed(slot, SaveFailure::of(&e)),
        }
    }

    /// Records a failed save in the editor and as a toast, and ends the save.
    fn save_failed(
        &mut self,
        slot: &mut ItemEditorSlot,
        failure: SaveFailure,
    ) -> Result<String, ()> {
        slot.problems.banner = Some(EditorProblem::Save(failure));
        self.show_toast(Toast {
            kind: ToastKind::Danger,
            message: save_failed_message(failure),
        });
        Err(())
    }

    /// The ids of the ticked tags, and the pending ones that must be made. A pending tag whose
    /// name exists by now (made elsewhere meanwhile) uses the existing tag.
    fn resolve_tags(&self, slot: &ItemEditorSlot) -> snipdeck_store::Result<ResolvedTags> {
        let existing = self.store.tags()?;
        let mut out = ResolvedTags::default();
        for entry in slot.tags.iter().filter(|t| t.checked) {
            if !entry.pending {
                out.ticked.insert(entry.id);
                continue;
            }
            let key = name_key(&entry.name);
            if let Some(found) = existing.iter().find(|t| name_key(t.name.as_str()) == key) {
                out.ticked.insert(found.id);
            } else if let Ok(name) = Name::new(&entry.name) {
                out.ticked.insert(entry.id);
                out.created.push(Tag { id: entry.id, name });
            }
        }
        Ok(out)
    }

    // ---- The library key -----------------------------------------------------------------------

    /// Makes the library key usable: nothing to do when it is open; opens it when it exists;
    /// makes it, with no master password, when `create` and it does not exist (the first
    /// sensitive item on Pro; the user is not asked).
    fn ensure_key(&mut self, create: bool) -> snipdeck_store::Result<()> {
        if self.store.is_unlocked() {
            return Ok(());
        }
        let exists = self.store.key_ring_exists()?;
        if !exists && !create {
            return Ok(());
        }
        let Some(provider) = self.editor_env.keys.as_deref() else {
            return Err(snipdeck_store::Error::Crypto(CryptoError::Provider(
                KeyProviderError::Unavailable("no key protection on this machine".to_owned()),
            )));
        };
        let ring = KeyRing::new(provider);
        if exists {
            self.store.unlock_key_ring(&ring)
        } else {
            self.store.create_key_ring(&ring)
        }
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
