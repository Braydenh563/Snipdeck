//! Recently used symbols: most recent first, at most 24, persisted by the app in its settings.

use serde::{Deserialize, Serialize};

use crate::catalog::{Catalog, Kind, SymbolId};

/// The most symbols remembered.
pub const RECENTS_MAX: usize = 24;

/// One persisted recent: `symbols.recent = [{id, used_at}]` in the settings file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentEntry {
    /// [`SymbolId::key`].
    pub id: String,
    /// When it was used, in milliseconds since the Unix epoch (the app's injected clock).
    pub used_at: i64,
}

/// Most-recently-used list. Pure: the time is passed in, nothing reads a clock.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recents {
    items: Vec<(SymbolId, i64)>,
}

impl Recents {
    /// An empty list.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a use at `now_ms`: moves the symbol to the front, drops the oldest past 24.
    pub fn push(&mut self, id: SymbolId, now_ms: i64) {
        self.items.retain(|(i, _)| *i != id);
        self.items.insert(0, (id, now_ms));
        self.items.truncate(RECENTS_MAX);
    }

    /// The ids, most recent first; only one kind when `kind` is given.
    #[must_use]
    pub fn list(&self, kind: Option<Kind>) -> Vec<SymbolId> {
        self.items
            .iter()
            .filter(|(id, _)| kind.is_none_or(|k| id.kind() == k))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// The use time of an id.
    #[must_use]
    pub fn used_at(&self, id: &SymbolId) -> Option<i64> {
        self.items.iter().find(|(i, _)| i == id).map(|(_, t)| *t)
    }

    /// Loads saved entries: unknown or malformed ids (a symbol that vanished in a data update) and
    /// duplicates are dropped, newest first by `used_at`, capped at 24.
    #[must_use]
    pub fn from_saved(saved: &[RecentEntry], catalog: &Catalog) -> Self {
        let mut items: Vec<(SymbolId, i64)> = Vec::new();
        for e in saved {
            let Some(id) = SymbolId::from_key(&e.id) else {
                continue;
            };
            if catalog.index_of(&id).is_none() || items.iter().any(|(i, _)| *i == id) {
                continue;
            }
            items.push((id, e.used_at));
        }
        items.sort_by_key(|(_, t)| std::cmp::Reverse(*t));
        items.truncate(RECENTS_MAX);
        Self { items }
    }

    /// The entries to save, most recent first.
    #[must_use]
    pub fn to_saved(&self) -> Vec<RecentEntry> {
        self.items
            .iter()
            .map(|(id, t)| RecentEntry {
                id: id.key(),
                used_at: *t,
            })
            .collect()
    }
}
