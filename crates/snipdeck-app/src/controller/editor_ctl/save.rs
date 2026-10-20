//! Saving the item, and the library key made on the first sensitive item.

use snipdeck_core::{Feature, Item, ItemKind, Name, Placement, Tag};
use snipdeck_store::{
    ItemRepo, TagRepo,
    crypto::{CryptoError, KeyProviderError},
    name_key,
};

use super::super::{Controller, Library, Toast, ToastKind, ToastMessage};
use super::{
    ItemEditorSlot, ItemTarget, Problems, ResolvedTags, build_formats, preview_of,
    save_failed_message,
};
use crate::{
    host::ClipboardText,
    item_editor::{self, EditorInput, EditorProblem, SaveFailure, Source},
};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
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
    pub(super) fn save_item(
        &mut self,
        slot: &mut ItemEditorSlot,
        input: &EditorInput,
    ) -> Result<String, ()> {
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

        // The key: made on the first sensitive item (on every plan: encryption is free), opened
        // whenever one exists.
        if (sensitive || slot.original_sensitive)
            && let Err(e) = self.ensure_key(sensitive)
        {
            return self.save_failed(slot, SaveFailure::of(&e));
        }

        let tags = match self.resolve_tags(slot) {
            Ok(t) => t,
            Err(e) => return self.save_failed(slot, SaveFailure::of(&e)),
        };
        let wrote = match slot.target {
            ItemTarget::New => {
                let mut item = Item::new(list_id, title, kind, now);
                item.sensitive = sensitive;
                item.label = input.label;
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
                    item.label = input.label;
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
    pub(super) fn save_failed(
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
    pub(super) fn resolve_tags(
        &self,
        slot: &ItemEditorSlot,
    ) -> snipdeck_store::Result<ResolvedTags> {
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
    /// sensitive item; the user is not asked).
    pub(in super::super) fn ensure_key(&mut self, create: bool) -> snipdeck_store::Result<()> {
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
        let ring = super::super::lock_ctl::ring_on(provider, self.lock.kdf);
        // A master password is never typed here: a library that has one stays locked until the
        // unlock window opens it ([`Controller::unlock_with_password`]).
        if exists {
            self.store.unlock_key_ring(&ring, None)
        } else {
            self.store.create_key_ring(&ring, None)
        }
    }
}
