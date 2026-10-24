//! Opening the item editor on a new or an existing item.

use std::collections::BTreeSet;

use snipdeck_core::{Format, ItemId, ItemKind, ListId, Name, TagId};
use snipdeck_store::name_key;

use super::super::{Controller, Library, Toast, ToastKind, ToastMessage};
use super::{ItemEditorSlot, ItemTarget, Problems, TagEntry, open_failed};
use crate::{
    capture,
    drafts::Draft,
    host::ClipboardText,
    item_editor::{self, AttachmentView, EDIT_LIMIT_BYTES, EditorInput, EditorProblem, Origin},
    usecase,
};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
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

    /// Ctrl+Shift+N in the quick-add field: the full editor on a new item whose content is `text`
    /// (a lone web address makes it a Link). The title stays empty: it comes from the first line.
    pub fn open_new_item_with(&mut self, origin: Origin, text: &str) {
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
            Ok(mut slot) => {
                if !text.trim().is_empty() {
                    slot.fields.kind = capture::kind_of(text);
                    text.trim_end().clone_into(&mut slot.fields.content);
                }
                self.install_item_editor(slot);
            }
            Err(e) => self.show_toast(open_failed(&e)),
        }
    }

    /// "Sensitive" in an item's menu: the item with the flag flipped, saved through the editor's own
    /// path (the content is encrypted or decrypted there). A refusal leaves the editor open with its
    /// reason.
    pub fn toggle_item_sensitive(&mut self, id: ItemId) {
        if self.raise_item_editor() {
            return;
        }
        match self.existing_slot(id) {
            Ok(slot) => {
                let mut input = slot.fields.clone();
                input.sensitive = !input.sensitive;
                self.install_item_editor(slot);
                self.item_editor_save(&input);
            }
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
    pub(super) fn raise_item_editor(&mut self) -> bool {
        match self.item_editor.as_mut() {
            Some(slot) => {
                slot.raise += 1;
                true
            }
            None => false,
        }
    }

    pub(super) fn new_item_list(&self, origin: Origin) -> Option<ListId> {
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
    pub(super) fn list_choices(
        &self,
        list: ListId,
    ) -> snipdeck_store::Result<(Vec<(ListId, String)>, usize)> {
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

    pub(super) fn tag_entries(
        &self,
        ticked: &BTreeSet<TagId>,
    ) -> snipdeck_store::Result<Vec<TagEntry>> {
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

    pub(super) fn new_slot(&self, list: ListId) -> snipdeck_store::Result<ItemEditorSlot> {
        let (lists, list_index) = self.list_choices(list)?;
        let fields = EditorInput {
            kind: ItemKind::Text,
            title: String::new(),
            content: String::new(),
            sensitive: false,
            list_index,
            source_path: String::new(),
            label: None,
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
            live_sensitive: false,
        };
        slot.baseline = slot.draft_of(&fields);
        Ok(slot)
    }

    pub(super) fn existing_slot(&mut self, id: ItemId) -> snipdeck_store::Result<ItemEditorSlot> {
        let item = self.store.item(id)?;
        if item.sensitive {
            // Opening decrypts the text: the key first.
            self.ensure_key(false)?;
            self.note_activity();
        }
        let (lists, list_index) = self.list_choices(item.list_id)?;
        let mut fields = EditorInput {
            kind: item.kind,
            title: item.title.clone(),
            content: String::new(),
            sensitive: item.sensitive,
            list_index,
            source_path: String::new(),
            label: item.label,
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
                item_editor::prefix_of(&text, EDIT_LIMIT_BYTES).clone_into(&mut fields.content);
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
            live_sensitive: false,
        };
        slot.baseline = slot.draft_of(&fields);
        Ok(slot)
    }

    /// What an existing Image or File item holds.
    pub(super) fn attachment_of(
        &self,
        kind: ItemKind,
        id: ItemId,
    ) -> snipdeck_store::Result<AttachmentView> {
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
    pub(super) fn install_item_editor(&mut self, mut slot: ItemEditorSlot) {
        self.item_editor_seq += 1;
        slot.seq = self.item_editor_seq;
        self.restore_draft(&mut slot);
        self.item_editor = Some(slot);
    }

    /// Puts the draft of this editor, if there is a usable one, into the fields. A draft of an
    /// item that changed since is stale and is removed. A sensitive item never has a draft
    /// (see [`crate::drafts`]).
    pub(super) fn restore_draft(&self, slot: &mut ItemEditorSlot) {
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
        slot.fields.label = draft.item_label();
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
    pub(super) fn add_pending_tag(
        tags: &mut Vec<TagEntry>,
        name: &str,
    ) -> Result<(), EditorProblem> {
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
}
