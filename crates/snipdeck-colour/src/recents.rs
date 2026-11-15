//! Recently used colours: newest first, at most 24, persisted in the settings file.

use crate::model::Rgba8;
use serde::{Deserialize, Serialize};

/// One persisted recent: `{ "hex": "#3a7bd5", "used_at": 1789000000000 }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentColour {
    /// `#rrggbb` or `#rrggbbaa`.
    pub hex: String,
    /// Milliseconds since the Unix epoch, from the app's injected clock.
    pub used_at: i64,
}

/// Most-recently-used colours. Pure: the time is passed in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColourRecents {
    items: Vec<(Rgba8, i64)>,
}

impl ColourRecents {
    /// The most colours remembered.
    pub const MAX: usize = 24;

    /// Restores a saved list (newest first), dropping bad hex and duplicates and capping at [`Self::MAX`].
    pub fn from_saved(saved: &[RecentColour]) -> Self {
        let mut items: Vec<(Rgba8, i64)> = Vec::new();
        for r in saved {
            if let Some(c) = Rgba8::from_hex(&r.hex)
                && !items.iter().any(|(k, _)| *k == c)
                && items.len() < Self::MAX
            {
                items.push((c, r.used_at));
            }
        }
        Self { items }
    }

    /// Records a use at `now_ms`: moves the colour to the front, drops the oldest past [`Self::MAX`].
    pub fn push(&mut self, c: Rgba8, now_ms: i64) {
        self.items.retain(|(k, _)| *k != c);
        self.items.insert(0, (c, now_ms));
        self.items.truncate(Self::MAX);
    }

    /// Forgets a colour.
    pub fn remove(&mut self, c: Rgba8) {
        self.items.retain(|(k, _)| *k != c);
    }

    /// The colours, newest first.
    pub fn list(&self) -> Vec<Rgba8> {
        self.items.iter().map(|(c, _)| *c).collect()
    }

    /// The persisted form, newest first.
    pub fn to_saved(&self) -> Vec<RecentColour> {
        self.items
            .iter()
            .map(|(c, t)| RecentColour {
                hex: c.to_hex(),
                used_at: *t,
            })
            .collect()
    }
}
