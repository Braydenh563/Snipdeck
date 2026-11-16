//! Saving the item, and the library key made on the first sensitive item.

use std::collections::BTreeSet;

use snipdeck_core::{
    Format, Formats, Item, ItemKind, ItemLabel, ListId, Name, Placement, Tag, TagId, Trigger,
};
use snipdeck_store::{
    GroupRepo, ItemRepo, TagRepo, Trashable, TriggerRepo,
    crypto::{CryptoError, KeyProviderError},
    name_key,
};

use super::super::{Controller, Library, Toast, ToastKind, ToastMessage};
use super::{
    ItemEditorSlot, ItemTarget, Problems, ResolvedTags, build_formats, preview_of,
    save_failed_message,
};
use crate::{
    history::{Action, Change, Snap},
    host::ClipboardText,
    item_editor::{self, EditorInput, EditorProblem, SaveFailure, Source},
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
            Ok((list, overlaps)) => {
                self.clear_draft(&slot);
                self.show_toast(if let Some(other) = overlaps {
                    Toast {
                        kind: ToastKind::Danger,
                        message: ToastMessage::TriggerOverlaps { other },
                    }
                } else {
                    Toast {
                        kind: ToastKind::Success,
                        message: ToastMessage::Saved { list },
                    }
                });
                self.library_changed();
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
    ) -> Result<(String, Option<String>), ()> {
        let now = self.clock.now();
        let mut overlaps: Option<String> = None;
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
                ItemKind::Template => item_editor::check_template(
                    &input.content,
                    &usecase::StoreSnippets::new(&self.store),
                )
                .map(|()| input.content.clone()),
                _ => Ok(input.content.clone()),
            });
            match checked {
                Ok(t) => {
                    if new || slot.original_text.as_deref() != Some(t.as_str()) {
                        text = Some(t);
                    }
                }
                Err(p) => {
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
        // A Sensitive item has no trigger (a short code must not release a secret).
        let mut trigger: Option<Trigger> = None;
        if !sensitive && !slot.trigger.is_empty() {
            if let Some(t) = slot.trigger_code() {
                let taken =
                    self.store
                        .triggers()
                        .unwrap_or_default()
                        .into_iter()
                        .any(|(other, holder)| {
                            other.same_code(&t) && slot.target != ItemTarget::Item(holder)
                        });
                if t.fires_in_words() && slot.original_code().as_ref() != Some(&t) {
                    slot.problems.trigger = Some(EditorProblem::TriggerTooShort);
                    refused = true;
                } else if taken {
                    slot.problems.trigger = Some(EditorProblem::TriggerTaken);
                    refused = true;
                } else {
                    trigger = Some(t);
                }
            } else {
                slot.problems.trigger = Some(EditorProblem::TriggerInvalid);
                refused = true;
            }
        }
        let list = slot.lists.get(input.list_index).cloned();
        if refused {
            return Err(());
        }
        let Some((list_id, list_name)) = list else {
            return self.save_failed(slot, SaveFailure::ListGone);
        };
        // The same text is already saved: ask before making a second copy.
        if new
            && !slot.allow_duplicate
            && !sensitive
            && let Some(found) = text
                .as_deref()
                .and_then(|t| crate::duplicate::find_text(&self.store, t))
        {
            slot.problems.duplicate = Some(found);
            return Err(());
        }

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
        let mut recorded: Option<(Action, Vec<Change>)> = None;
        let wrote = match slot.target {
            ItemTarget::New => {
                let mut item = Item::new(list_id, title, kind, now);
                item.sensitive = sensitive;
                item.always_plain = input.always_plain;
                item.label = input.label;
                item.tags = tags.ticked;
                item.preview = preview_of(is_text, text.as_deref(), sensitive);
                let done = self.store.atomically(|b| {
                    for tag in &tags.created {
                        b.insert_tag(tag)?;
                    }
                    let formats = build_formats(b, sensitive, text.as_deref(), source.as_ref())?;
                    // After the row the cursor was on (inside its group); else the top.
                    let anchor = slot
                        .insert_after
                        .and_then(|a| b.item(a).ok())
                        .filter(|a| a.list_id == list_id);
                    let at = anchor
                        .as_ref()
                        .map_or(Placement::First, |a| Placement::After(a.id));
                    b.insert_item(&item, &formats, at)?;
                    if let Some(anchor) = &anchor
                        && let Some(run) = b
                            .groups_of(list_id)?
                            .into_iter()
                            .find(|r| r.items.contains(&anchor.id))
                    {
                        b.set_item_group(item.id, Some(run.group.id))?;
                    }
                    if trigger.is_some() {
                        let codes = b.set_trigger(item.id, trigger.as_ref())?;
                        overlaps = overlap_title(b, &codes)?;
                    }
                    Ok(())
                });
                if done.is_ok() {
                    recorded = Some((
                        Action::Add,
                        vec![Change::Restore(vec![Trashable::Item(item.id)])],
                    ));
                }
                done
            }
            ItemTarget::Item(id) => match self.store.item(id) {
                Ok(mut item) => {
                    let before = item.clone();
                    // Only what the person changed since the editor opened goes onto the item as
                    // it is now: a colour, tag or move done elsewhere meanwhile stays.
                    let edit = Fields {
                        title,
                        kind,
                        list_id,
                        tags: tags.ticked,
                        sensitive,
                        always_plain: input.always_plain,
                        label: input.label,
                    };
                    let opened = Fields::of(slot.opened.as_ref().unwrap_or(&item));
                    edit.merge_onto(&opened, &mut item);
                    // What is stored decides how the text is written.
                    let sensitive = item.sensitive;
                    item.updated_at = now;
                    // The preview follows the text on screen when the text itself is unchanged.
                    let shown = text.as_deref().unwrap_or(&input.content);
                    item.preview = if is_text {
                        preview_of(true, Some(shown), sensitive)
                    } else {
                        String::new()
                    };
                    // "Remove formatting" was pressed (and the text is not being rewritten, which
                    // drops them anyway).
                    let drop_rich = slot.drop_formats && slot.formatted && !before.sensitive;
                    // What the edit is about to drop (HTML, RTF), kept for Undo.
                    let old_formats = ((text.is_some() || drop_rich) && !before.sensitive)
                        .then(|| self.store.formats(id).ok())
                        .flatten();
                    let kept = match old_formats.as_ref().filter(|_| drop_rich && text.is_none()) {
                        Some(f) => match without_rich(f) {
                            Ok(k) => Some(k),
                            Err(e) => return self.save_failed(slot, SaveFailure::of(&e)),
                        },
                        None => None,
                    };
                    let done = self.index_cache.edit_item(&mut self.store, id, |store| {
                        store.atomically(|b| {
                            for tag in &tags.created {
                                b.insert_tag(tag)?;
                            }
                            let formats = match text.as_deref() {
                                Some(t) => Some(build_formats(b, sensitive, Some(t), None)?),
                                None => kept.clone(),
                            };
                            b.update_item(&item, formats.as_ref())?;
                            if trigger.is_some() || !slot.original_trigger.is_empty() {
                                let codes = b.set_trigger(id, trigger.as_ref())?;
                                overlaps = overlap_title(b, &codes)?;
                            }
                            Ok(())
                        })
                    });
                    if done.is_ok() {
                        recorded = Some(edit_step(
                            &before,
                            &item,
                            (slot.original_text.as_deref(), old_formats),
                            text.as_deref(),
                            kept.as_ref(),
                            (slot.original_code() != trigger)
                                .then(|| (slot.original_code(), trigger.clone())),
                        ));
                    }
                    done
                }
                Err(e) => Err(e),
            },
        };
        match wrote {
            Ok(()) => {
                if let Some((action, changes)) = recorded {
                    self.record_step(action, changes);
                }
                Ok((list_name, overlaps))
            }
            Err(e) => self.save_failed(slot, SaveFailure::of(&e)),
        }
    }

    /// Records a failed save in the editor and as a toast, and ends the save.
    pub(super) fn save_failed(
        &mut self,
        slot: &mut ItemEditorSlot,
        failure: SaveFailure,
    ) -> Result<(String, Option<String>), ()> {
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
        // unlock window opens it (`begin_unlock_with_password`).
        if exists {
            self.store.unlock_key_ring(&ring, None)
        } else {
            self.store.create_key_ring(&ring, None)
        }
    }
}

/// The history step for an edit that changed `before` into `after`, with the text it replaced
/// (`old_text`) and the text it wrote (`new_text`, `None` when the content stayed). `kept` is the
/// formats left when only "Remove formatting" was pressed. A text edit that undoing could not
/// reverse (a sensitive item's text is not kept in memory, or the editor never read the old
/// text) is a [`Change::Barrier`] step, so Undo says why it stops.
fn edit_step(
    before: &Item,
    after: &Item,
    (old_text, old_formats): (Option<&str>, Option<Formats>),
    new_text: Option<&str>,
    kept: Option<&Formats>,
    triggers: Option<(Option<Trigger>, Option<Trigger>)>,
) -> (Action, Vec<Change>) {
    let (old_trigger, new_trigger) = match triggers {
        Some((old, new)) => (Some(old), Some(new)),
        None => (None, None),
    };
    let (old, new) = match new_text {
        None => (None, None),
        Some(new) => {
            if before.sensitive || after.sensitive {
                // The text is not kept (it would be plaintext in memory): the step only marks
                // the edit, so Undo says why it stops instead of undoing something older.
                return (Action::Edit, vec![Change::Barrier(after.id)]);
            }
            let Some(old) = old_text else {
                return (Action::Edit, vec![Change::Barrier(after.id)]);
            };
            (Some(old.to_owned()), Some(new.to_owned()))
        }
    };
    // The same fields apart from the colour label and the edit time: a colour change.
    let mut same = after.clone();
    same.label = before.label;
    same.updated_at = before.updated_at;
    let colour_only =
        new.is_none() && old_trigger.is_none() && same == *before && after.label != before.label;
    let action = if colour_only {
        Action::Colour
    } else {
        Action::Edit
    };
    (
        action,
        vec![Change::Update {
            before: Box::new(Snap {
                item: before.clone(),
                // The formats the edit drops come back with it.
                formats: (old.is_some() || kept.is_some())
                    .then_some(old_formats)
                    .flatten(),
                text: old,
                trigger: old_trigger,
            }),
            after: Box::new(Snap {
                item: after.clone(),
                formats: kept.cloned(),
                text: new,
                trigger: new_trigger,
            }),
        }],
    )
}

/// `formats` without the HTML and RTF.
fn without_rich(formats: &Formats) -> snipdeck_store::Result<Formats> {
    let kept: Vec<Format> = formats
        .iter()
        .filter(|f| !matches!(f, Format::Html(_) | Format::Rtf(_)))
        .cloned()
        .collect();
    Ok(Formats::new(kept)?)
}

/// The fields of an item the editor owns.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Fields {
    title: String,
    kind: ItemKind,
    list_id: ListId,
    tags: BTreeSet<TagId>,
    sensitive: bool,
    always_plain: bool,
    label: Option<ItemLabel>,
}

impl Fields {
    fn of(item: &Item) -> Self {
        Self {
            title: item.title.clone(),
            kind: item.kind,
            list_id: item.list_id,
            tags: item.tags.clone(),
            sensitive: item.sensitive,
            always_plain: item.always_plain,
            label: item.label,
        }
    }

    /// Writes onto `item` each field of `self` that differs from `opened` (the fields when the
    /// editor opened); tags by what was ticked or unticked, so tags added elsewhere stay.
    fn merge_onto(&self, opened: &Self, item: &mut Item) {
        if self.title != opened.title {
            item.title.clone_from(&self.title);
        }
        if self.kind != opened.kind {
            item.kind = self.kind;
        }
        if self.list_id != opened.list_id {
            item.list_id = self.list_id;
        }
        if self.sensitive != opened.sensitive {
            item.sensitive = self.sensitive;
        }
        if self.always_plain != opened.always_plain {
            item.always_plain = self.always_plain;
        }
        if self.label != opened.label {
            item.label = self.label;
        }
        for tag in self.tags.difference(&opened.tags) {
            item.tags.insert(*tag);
        }
        for tag in opened.tags.difference(&self.tags) {
            item.tags.remove(tag);
        }
    }
}

/// The title of the item that owns the first of these overlapping codes, for the "Saved, but" toast.
fn overlap_title<B: TriggerRepo + ItemRepo>(
    b: &B,
    codes: &[Trigger],
) -> snipdeck_store::Result<Option<String>> {
    let Some(code) = codes.first() else {
        return Ok(None);
    };
    Ok(match b.trigger_holder(code)? {
        Some(id) => Some(b.item(id)?.title),
        None => Some(String::new()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use snipdeck_core::{ItemLabel, ListId, Timestamp};

    fn item() -> Item {
        let at: Timestamp = "2026-10-17T10:30:00Z".parse().unwrap();
        Item::new(
            ListId::generate(),
            "Greeting".to_owned(),
            ItemKind::Text,
            at,
        )
    }

    #[test]
    fn a_label_only_change_is_a_colour_step() {
        let before = item();
        let mut after = before.clone();
        after.label = Some(ItemLabel::Blue);
        let (action, changes) = edit_step(&before, &after, (Some("hi"), None), None, None, None);
        assert_eq!(action, Action::Colour);
        assert_eq!(changes.len(), 1);
    }

    #[test]
    fn a_text_change_keeps_both_texts() {
        let before = item();
        let after = before.clone();
        let (action, changes) = edit_step(
            &before,
            &after,
            (Some("old"), None),
            Some("new"),
            None,
            None,
        );
        assert_eq!(action, Action::Edit);
        let Change::Update {
            before: b,
            after: a,
        } = &changes[0]
        else {
            panic!("an update");
        };
        assert_eq!(
            (b.text.as_deref(), a.text.as_deref()),
            (Some("old"), Some("new"))
        );
    }

    #[test]
    fn a_sensitive_or_unknown_text_edit_is_a_barrier_not_nothing() {
        let before = item();
        let mut secret = before.clone();
        secret.sensitive = true;
        for (b, a, old) in [
            (&before, &secret, Some("old")),
            (&secret, &before, Some("old")),
            (&before, &before, None),
        ] {
            let (action, changes) = edit_step(b, a, (old, None), Some("new"), None, None);
            assert_eq!(action, Action::Edit);
            assert_eq!(changes, vec![Change::Barrier(a.id)]);
        }
    }

    #[test]
    fn only_the_fields_changed_since_opening_are_applied() {
        let opened = Fields::of(&item());
        let mut now = item();
        now.label = Some(ItemLabel::Red);
        now.tags.insert(TagId::generate());
        let elsewhere_tag = *now.tags.iter().next().unwrap();
        let mut edit = opened.clone();
        edit.title = "Renamed".into();
        let added = TagId::generate();
        edit.tags.insert(added);
        edit.merge_onto(&opened, &mut now);
        assert_eq!(now.title, "Renamed");
        assert_eq!(
            now.label,
            Some(ItemLabel::Red),
            "colour set meanwhile stays"
        );
        assert!(
            now.tags.contains(&elsewhere_tag),
            "tag added meanwhile stays"
        );
        assert!(now.tags.contains(&added));
    }

    #[test]
    fn an_unticked_tag_is_removed_and_nothing_else_is_touched() {
        let mut base = item();
        let t = TagId::generate();
        base.tags.insert(t);
        let opened = Fields::of(&base);
        let mut edit = opened.clone();
        edit.tags.remove(&t);
        let mut now = base.clone();
        now.sensitive = true;
        edit.merge_onto(&opened, &mut now);
        assert!(now.tags.is_empty());
        assert!(now.sensitive, "an untouched field keeps its current value");
    }

    #[test]
    fn formats_without_rich_drop_html_and_rtf() {
        let inline = |b: &[u8]| {
            snipdeck_core::Content::Inline(snipdeck_core::Inline::new(b.to_vec()).unwrap())
        };
        let f = Formats::new(vec![
            Format::Text(inline(b"a")),
            Format::Html(inline(b"<b>a</b>")),
        ])
        .unwrap();
        let k = without_rich(&f).unwrap();
        assert!(k.iter().all(|x| matches!(x, Format::Text(_))));
    }
}
