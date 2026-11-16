//! The Panel's row model: each `ui::PanelRow` is built when the list asks for it (a `ListView`
//! asks only for the rows on screen), not for every row of a long list on every change.
//!
//! The model keeps the controller's rows (shared, not copied) and the few integers that depend
//! on the rows around one (ghosts above, position in a group, stack number). An update compares
//! controller rows, not built ui rows, and tells the view which rows changed, so the list is the
//! same model across changes and its scroll position and focus survive.

use std::{any::Any, cell::Cell, cell::RefCell, collections::HashMap, rc::Rc};

use slint::{Model, ModelNotify, ModelTracker};
use snipdeck_core::ItemId;
use snipdeck_ui as ui;

use crate::panel::{GroupHeader, Row};

/// More rows added or removed than this reset the list at once.
const BULK_ROWS: usize = 16;

/// What a row shows that depends on the rows around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Extra {
    pub ghosts_before: i32,
    pub set_position: i32,
    pub set_size: i32,
    pub stack_position: i32,
}

/// One pass over `rows` with running counters (never a scan per row).
pub fn extras(rows: &[Row], marked: &[ItemId]) -> Vec<Extra> {
    let mut stack_at: HashMap<ItemId, i32> = HashMap::with_capacity(marked.len());
    for (at, id) in marked.iter().enumerate() {
        stack_at
            .entry(*id)
            .or_insert_with(|| i32::try_from(at + 1).unwrap_or(i32::MAX));
    }
    let to_i32 = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
    // Collapsed "Deleted · Undo" rows and group headers do not take a number.
    let mut ghosts_before = 0_usize;
    // The grouped rows just above, and the header of the row above that run.
    let mut run = 0_usize;
    let mut header_above: Option<&GroupHeader> = None;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let mut extra = Extra {
            ghosts_before: to_i32(ghosts_before),
            stack_position: stack_at.get(&r.id).copied().unwrap_or(0),
            ..Extra::default()
        };
        // A group's items count off under their header: "2 of 5".
        if r.grouped {
            extra.set_position = to_i32(run + 1);
            extra.set_size = header_above.map_or(0, |h| to_i32(h.count));
        }
        out.push(extra);
        if r.deleted_pending || r.header.is_some() {
            ghosts_before += 1;
        }
        if r.grouped {
            run += 1;
        } else {
            run = 0;
            header_above = r.header.as_ref();
        }
    }
    out
}

struct State {
    rows: Rc<Vec<Row>>,
    extra: Vec<Extra>,
    dark: bool,
    /// The one sensitive row held revealed: the controller put its text in the preview.
    revealed: Option<usize>,
}

impl State {
    fn same_row(&self, i: usize, new: &State, j: usize) -> bool {
        (Rc::ptr_eq(&self.rows, &new.rows) && i == j || self.rows[i] == new.rows[j])
            && self.extra[i] == new.extra[j]
            && (self.revealed == Some(i)) == (new.revealed == Some(j))
    }
}

/// The Panel's rows, built on demand.
pub struct LazyRows {
    state: RefCell<State>,
    notify: ModelNotify,
    built: Cell<usize>,
}

impl LazyRows {
    pub fn new(rows: Rc<Vec<Row>>, extra: Vec<Extra>, dark: bool, revealed: Option<usize>) -> Self {
        Self {
            state: RefCell::new(State {
                rows,
                extra,
                dark,
                revealed,
            }),
            notify: ModelNotify::default(),
            built: Cell::new(0),
        }
    }

    /// How many ui rows have been built (for tests).
    #[cfg(test)]
    pub fn built(&self) -> usize {
        self.built.get()
    }

    /// Takes the new rows and tells the view which differ from the old, comparing controller
    /// rows (no ui row is built). Rows kept at the head and the tail are left alone.
    pub fn update(
        &self,
        rows: Rc<Vec<Row>>,
        extra: Vec<Extra>,
        dark: bool,
        revealed: Option<usize>,
    ) {
        let new = State {
            rows,
            extra,
            dark,
            revealed,
        };
        let (head, tail, old_len, reset) = {
            let old = self.state.borrow();
            let (o, n) = (old.rows.len(), new.rows.len());
            if old.dark == new.dark {
                let mut head = 0;
                while head < o.min(n) && old.same_row(head, &new, head) {
                    head += 1;
                }
                let mut tail = 0;
                while tail < o.min(n) - head && old.same_row(o - 1 - tail, &new, n - 1 - tail) {
                    tail += 1;
                }
                let (om, nm) = (o - head - tail, n - head - tail);
                (head, tail, o, om.abs_diff(nm) > BULK_ROWS)
            } else {
                (0, 0, o, true)
            }
        };
        let new_len = new.rows.len();
        let old_state = self.state.replace(new);
        if reset {
            self.notify.reset();
            return;
        }
        let (old_mid, new_mid) = (old_len - head - tail, new_len - head - tail);
        let shared = old_mid.min(new_mid);
        let new_state = self.state.borrow();
        for i in 0..shared {
            if !old_state.same_row(head + i, &new_state, head + i) {
                self.notify.row_changed(head + i);
            }
        }
        drop(new_state);
        if old_mid > shared {
            self.notify.row_removed(head + shared, old_mid - shared);
        } else if new_mid > shared {
            self.notify.row_added(head + shared, new_mid - shared);
        }
    }
}

impl Model for LazyRows {
    type Data = ui::PanelRow;

    fn row_count(&self) -> usize {
        self.state.borrow().rows.len()
    }

    fn row_data(&self, row: usize) -> Option<ui::PanelRow> {
        let state = self.state.borrow();
        let r = state.rows.get(row)?;
        let extra = state.extra.get(row)?;
        self.built.set(self.built.get() + 1);
        let mut out = super::panel_row(r, state.dark);
        out.ghosts_before = extra.ghosts_before;
        out.set_position = extra.set_position;
        out.set_size = extra.set_size;
        out.stack_position = extra.stack_position;
        if state.revealed == Some(row) {
            out.preview = r.preview.as_str().into();
        }
        Some(out)
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snipdeck_core::ItemKind;

    fn row(n: usize) -> Row {
        Row {
            id: ItemId::generate(),
            number: 1,
            kind: ItemKind::Text,
            sensitive: false,
            title: format!("Title {n}"),
            preview: "p".into(),
            list: None,
            title_mark: None,
            preview_mark: None,
            tag_hit: None,
            pinned: false,
            label: None,
            marked: false,
            deleted_pending: false,
            formatted: false,
            header: None,
            grouped: false,
        }
    }

    fn model(rows: &[Row]) -> LazyRows {
        LazyRows::new(Rc::new(rows.to_vec()), extras(rows, &[]), false, None)
    }

    #[test]
    fn a_long_list_builds_only_the_rows_asked_for() {
        let rows: Vec<Row> = (0..10_000).map(row).collect();
        let lazy = model(&rows);
        assert_eq!(lazy.row_count(), 10_000);
        assert_eq!(lazy.built(), 0);
        for i in 0..20 {
            assert!(lazy.row_data(i).is_some());
        }
        assert_eq!(lazy.built(), 20);
    }

    #[test]
    fn an_update_builds_nothing_and_the_model_stays_the_same() {
        let mut rows: Vec<Row> = (0..10_000).map(row).collect();
        let lazy = model(&rows);
        rows.insert(5_000, row(99_999));
        lazy.update(Rc::new(rows.clone()), extras(&rows, &[]), false, None);
        assert_eq!(lazy.row_count(), 10_001);
        assert_eq!(lazy.built(), 0);
        assert_eq!(
            lazy.row_data(5_000).map(|r| r.title.to_string()),
            Some("Title 99999".into())
        );
        rows.remove(5_000);
        lazy.update(Rc::new(rows.clone()), extras(&rows, &[]), false, None);
        assert_eq!(lazy.row_count(), 10_000);
        assert_eq!(
            lazy.row_data(5_000).map(|r| r.title.to_string()),
            Some("Title 5000".into())
        );
    }
}
