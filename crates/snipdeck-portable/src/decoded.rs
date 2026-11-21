//! The two halves of an import. A source (archive, folder, Ditto, ...) is first *decoded* into a
//! [`Decoded`]: a validated, owned plan that needs no library, so it can be built on a worker
//! thread. It is then either [previewed](Decoded::preview) (read-only: nothing is written) or
//! [applied](Decoded::apply) in one short transaction on the thread that owns the library.
//!
//! Duplicates are decided against a set of content hashes, [`Known`]: the caller passes one
//! read from a snapshot (so the slow pass over the library happens off the UI thread) or `None`
//! to read it from the library at hand.

use std::collections::HashSet;

use snipdeck_store::name_key;

use crate::{
    error::{Error, Result},
    import::{ImportOptions, MergeStrategy},
    plan::{AssetRef, Library, Plan, apply, existing_hashes},
    report::{ImportReport, Note, Warning},
};

/// The content hashes of the items a library holds, for skipping duplicates.
#[derive(Debug, Clone, Default)]
pub struct Known(HashSet<[u8; 32]>);

impl Known {
    #[cfg(test)]
    pub(crate) fn from_hashes(hashes: HashSet<[u8; 32]>) -> Self {
        Self(hashes)
    }

    /// Reads the hashes from `lib` (a pass over every item, so not for the UI thread of a big
    /// library: read them from a snapshot on a worker).
    pub fn of<L: Library>(lib: &L) -> Result<Self> {
        existing_hashes(lib).map(Self)
    }
}

type Fetch<'a> = Box<dyn FnMut(&AssetRef) -> Result<Vec<u8>> + Send + 'a>;

/// A source decoded and validated, waiting to be previewed or applied. `Send`; `Decoded<'static>`
/// (from an owned reader or path) can move to another thread.
pub struct Decoded<'a> {
    plan: Plan,
    report: ImportReport,
    merge: MergeStrategy,
    fetch: Fetch<'a>,
}

impl std::fmt::Debug for Decoded<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decoded")
            .field("plan", &self.plan)
            .field("report", &self.report)
            .finish_non_exhaustive()
    }
}

impl<'a> Decoded<'a> {
    /// A plan whose payloads are all in memory.
    pub(crate) fn new(plan: Plan, report: ImportReport, options: &ImportOptions<'_>) -> Self {
        Self::with_assets(
            plan,
            report,
            options,
            Box::new(|_| Err(Error::malformed("import", "internal: no assets expected"))),
        )
    }

    /// A plan whose plain assets are read from their archive when written.
    pub(crate) fn with_assets(
        plan: Plan,
        report: ImportReport,
        options: &ImportOptions<'_>,
        fetch: Fetch<'a>,
    ) -> Self {
        Self {
            plan,
            report,
            merge: options.merge,
            fetch,
        }
    }

    /// For each item in write order, whether it is kept (not a duplicate of `known` or of an
    /// earlier item of this import).
    fn keep_flags(&self, known: &Known) -> Vec<bool> {
        let mut seen = known.0.clone();
        let skip = self.merge == MergeStrategy::SkipDuplicates;
        self.plan
            .boards
            .iter()
            .flat_map(|b| &b.lists)
            .flat_map(|l| &l.items)
            .map(|i| !skip || seen.insert(i.content_hash))
            .collect()
    }

    fn known_for<L: Library>(&self, lib: &L, known: Option<&Known>) -> Result<Known> {
        match known {
            Some(k) => Ok(k.clone()),
            None if self.merge == MergeStrategy::SkipDuplicates => Known::of(lib),
            None => Ok(Known::default()),
        }
    }

    /// What [`apply`](Self::apply) would report on `lib` as it is now, without writing anything.
    /// `known` is the duplicate set to use (`None`: read it from `lib`).
    ///
    /// # Errors
    ///
    /// A storage error from reading `lib`.
    pub fn preview<L: Library>(&self, lib: &L, known: Option<&Known>) -> Result<ImportReport> {
        let flags = self.keep_flags(&self.known_for(lib, known)?);
        let mut report = self.report.clone();
        let mut kept = flags.iter().copied();
        let mut dropped = 0;

        let mut used: HashSet<usize> = HashSet::new();
        let mut triggers: Vec<String> = lib
            .triggers()?
            .into_iter()
            .map(|(t, _)| t.as_str().to_owned())
            .collect();
        let mut wanted = Vec::new();
        for board in &self.plan.boards {
            let mut board_items = 0;
            for list in &board.lists {
                for item in &list.items {
                    if kept.next() == Some(true) {
                        board_items += 1;
                        used.extend(item.tags.iter().copied());
                        wanted.push(item);
                    } else {
                        dropped += 1;
                    }
                }
            }
            if board_items > 0 || self.merge != MergeStrategy::SkipDuplicates {
                report.boards_added += 1;
                report.lists_added += board.lists.len();
            }
        }
        report.items_added += wanted.len();
        report.items_skipped += dropped;
        report.duplicates_skipped += dropped;

        let mut have: HashSet<String> = lib
            .tags()?
            .into_iter()
            .map(|t| name_key(t.name.as_str()))
            .collect();
        for (index, name) in self.plan.tags.iter().enumerate() {
            if used.contains(&index) && have.insert(name_key(name.as_str())) {
                report.tags_added += 1;
            }
        }

        for item in wanted {
            let Some(trigger) = item.trigger.as_ref().filter(|_| !item.sensitive) else {
                continue;
            };
            let code = trigger.as_str();
            if triggers.iter().any(|t| t == code) {
                report.warn(Warning::Note(Note::TriggerTaken {
                    title: item.title.clone(),
                    trigger: trigger.to_string(),
                }));
                continue;
            }
            let others: Vec<&str> = triggers
                .iter()
                .map(String::as_str)
                .filter(|o| o.starts_with(code) || code.starts_with(o))
                .collect();
            if !others.is_empty() {
                report.warn(Warning::Note(Note::TriggerOverlap {
                    title: item.title.clone(),
                    trigger: trigger.to_string(),
                    others: others.join(", "),
                }));
            }
            triggers.push(code.to_owned());
            triggers.sort();
        }
        Ok(report)
    }

    /// Writes what is left of the plan into `lib` in one transaction; on an error nothing is
    /// added. `known` is the duplicate set to use (`None`: read it from `lib`).
    ///
    /// # Errors
    ///
    /// A storage error, or a payload that changed since it was checked; `lib` is unchanged.
    pub fn apply<L: Library>(mut self, lib: &mut L, known: Option<&Known>) -> Result<ImportReport> {
        let mut flags = self.keep_flags(&self.known_for(lib, known)?).into_iter();
        let mut dropped = 0;
        for list in self.plan.boards.iter_mut().flat_map(|b| &mut b.lists) {
            list.items.retain(|_| {
                let keep = flags.next() == Some(true);
                dropped += usize::from(!keep);
                keep
            });
        }
        if self.merge == MergeStrategy::SkipDuplicates {
            self.plan
                .boards
                .retain(|b| b.lists.iter().any(|l| !l.items.is_empty()));
        }
        let mut report = self.report;
        report.items_skipped += dropped;
        report.duplicates_skipped += dropped;
        apply(lib, &mut self.plan, &mut self.fetch, report)
    }
}
