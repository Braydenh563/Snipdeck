//! What the Main window's Detail pane shows (SPEC 5.3), and the picture thumbnails of its image
//! rows (MAIN-21).

use jiff::{Timestamp, tz::TimeZone};
use snipdeck_core::{Format, FormatKind, ItemId, ItemKind};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};

use super::super::{Controller, Library};
use crate::main_window::{DetailView, Key};
use crate::thumb_cache::{DETAIL_SIDE, Thumb};
use crate::{
    binding::MASKED_PREVIEW,
    host::ClipboardText,
    recent::{thumbnail, thumbnail_within},
    usecase,
};

/// The most characters of an item's text the pane shows.
pub(crate) const DETAIL_TEXT_CHARS: usize = 4000;

/// The slow part of a [`DetailView`] (the store reads and the picture decode), kept for the
/// item the pane last showed.
#[derive(Debug)]
pub(crate) struct DetailBody {
    /// What it was read for: the item, when it last changed, whether it was shown revealed.
    key: (ItemId, Timestamp, bool),
    text: String,
    text_cut: bool,
    thumb: Option<Thumb>,
    formats: Vec<FormatKind>,
}

/// `at` as local `YYYY-MM-DD HH:MM`.
fn stamp(at: Timestamp, tz: &TimeZone) -> String {
    at.to_zoned(tz.clone())
        .strftime("%Y-%m-%d %H:%M")
        .to_string()
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// Decodes the picture of the item at row `index` for its list row, when it is an image item
    /// that has none yet. Returns whether a picture was added (the window then draws again).
    pub fn main_thumbnail(&mut self, index: usize) -> bool {
        self.main_fresh();
        let Some(Key::Item(id)) = self.main.keys().get(index).copied() else {
            return false;
        };
        let Some(item) = self.main.item(id) else {
            return false;
        };
        if item.kind != ItemKind::Image || item.sensitive {
            return false;
        }
        let stamp = item.updated_at;
        if self.thumbs.get(id, stamp).is_some() {
            return false;
        }
        let thumb = usecase::item_png(&self.store, id)
            .ok()
            .flatten()
            .and_then(|png| crate::item_editor::decode_preview(&png))
            .and_then(|p| thumbnail(p.width, p.height, &p.rgba))
            .map(Thumb::new);
        let found = thumb.is_some();
        self.thumbs.put(id, stamp, thumb);
        found
    }

    /// How many row thumbnails are kept.
    #[must_use]
    pub fn main_thumbnails_kept(&self) -> usize {
        self.thumbs.len()
    }

    /// The Detail pane's view of the item under the cursor, `None` when the cursor is on no item.
    pub(super) fn detail_view(&mut self) -> Option<DetailView> {
        let Key::Item(id) = self.main.keys().get(self.main.cursor()).copied()? else {
            return None;
        };
        let item = self.main.item(id)?.clone();
        let revealed = item.sensitive && self.revealed == Some(id);
        let key = (id, item.updated_at, revealed);
        if self.detail_body.as_ref().is_none_or(|b| b.key != key) {
            self.detail_body = Some(self.read_detail_body(&item, key, revealed));
        }
        let body = self.detail_body.as_ref()?;
        let list_name = self
            .main
            .move_targets()
            .into_iter()
            .find(|t| t.list == item.list_id)
            .map(|t| t.label)
            .unwrap_or_default();
        let tags = self
            .main
            .tags()
            .iter()
            .filter(|t| item.tags.contains(&t.id))
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let title = if item.title.trim().is_empty() {
            if item.sensitive && !revealed {
                String::new()
            } else {
                item.preview.lines().next().unwrap_or_default().to_owned()
            }
        } else {
            item.title.clone()
        };
        Some(DetailView {
            item: id,
            kind: item.kind,
            sensitive: item.sensitive,
            title,
            text: body.text.clone(),
            text_cut: body.text_cut,
            thumb: body.thumb.clone(),
            formats: body.formats.clone(),
            use_count: item.use_count,
            last_used: item
                .last_used_at
                .map(|at| stamp(at, &self.tz))
                .unwrap_or_default(),
            created: stamp(item.created_at, &self.tz),
            updated: stamp(item.updated_at, &self.tz),
            source_app: item.source_app.clone().unwrap_or_default(),
            list_name,
            tags,
        })
    }

    fn read_detail_body(
        &self,
        item: &snipdeck_core::Item,
        key: (ItemId, Timestamp, bool),
        revealed: bool,
    ) -> DetailBody {
        let formats = self.store.formats(item.id).ok();
        let kinds = formats
            .as_ref()
            .map(|f| f.iter().map(Format::kind).collect())
            .unwrap_or_default();
        let mut text = String::new();
        let mut text_cut = false;
        let mut thumb = None;
        if item.sensitive && !revealed {
            text = MASKED_PREVIEW.to_owned();
        } else if let Some(formats) = &formats {
            if item.kind == ItemKind::Image {
                thumb = usecase::item_png(&self.store, item.id)
                    .ok()
                    .flatten()
                    .and_then(|png| crate::item_editor::decode_preview(&png))
                    .and_then(|p| thumbnail_within(p.width, p.height, &p.rgba, DETAIL_SIDE))
                    .map(Thumb::new);
            } else if let Some(Format::Files(files)) = formats.get(FormatKind::Files) {
                text = files
                    .iter()
                    .map(|f| f.name())
                    .collect::<Vec<_>>()
                    .join("\n");
            } else if let Ok(Some(full)) = usecase::item_text(&self.store, item.id) {
                let mut chars = full.chars();
                text = chars.by_ref().take(DETAIL_TEXT_CHARS).collect();
                text_cut = chars.next().is_some();
            }
        }
        DetailBody {
            key,
            text,
            text_cut,
            thumb,
            formats: kinds,
        }
    }
}
