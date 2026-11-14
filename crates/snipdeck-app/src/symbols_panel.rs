//! The Symbols tab's grid, as data: which symbols show for a kind and a query, laid out in rows for
//! a number of columns, with the keyboard table and the group jump points. Slint-free (the
//! binding turns [`Grid`] into the window's rows), so it is tested without a window.

use snipdeck_symbols::{Catalog, EMOJI_GROUPS, Kind, Lang, SymbolId, search};
use snipdeck_ui::symbol_nav::NavRow;

/// Height of a section header row.
pub const HEADER_H: f32 = 24.0;
/// Height of a row of emoji or icon cells.
pub const CELL_H: f32 = 40.0;
/// Height of a row of kaomoji chips (a 32 px chip and a 4 px gap).
pub const CHIP_H: f32 = 36.0;
/// Rows of each kind the "All" view shows before its "Show all" link.
const ALL_ROWS: usize = 2;
/// Kaomoji chips per row.
const CHIP_COLUMNS: usize = 2;

/// The tab a kind number names: 0 All, 1 Emoji, 2 Kaomoji, 3 Icons.
#[must_use]
pub const fn kind_of(tab: i32) -> Option<Kind> {
    match tab {
        1 => Some(Kind::Emoji),
        2 => Some(Kind::Kaomoji),
        3 => Some(Kind::Icon),
        _ => None,
    }
}

/// A section's title, before translation (the binding words it through the window's texts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Title {
    /// No header: a flat result list of one kind.
    None,
    /// "Recently used".
    Recent,
    /// A kind's own section ("Emoji", "Kaomoji", "Icons").
    Kind(Kind),
    /// An emoji group, by its place in [`EMOJI_GROUPS`].
    EmojiGroup(usize),
    /// A kaomoji group, by its place in `KAOMOJI_GROUPS`.
    KaomojiGroup(usize),
    /// An icon category (English, from the catalog).
    IconCategory(String),
}

/// One cell: a catalog index and where it sits in the reading order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// Index into [`Catalog::symbols`].
    pub index: usize,
    /// Position in the reading order: the selection index.
    pub flat: usize,
    /// A kaomoji over ten characters, which spans both columns.
    pub wide: bool,
}

/// One row of the grid.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// A section header when `Some`; the row then has no cells.
    pub title: Option<Title>,
    /// For a header: the kind tab its "Show all" link switches to.
    pub link: Option<Kind>,
    /// The cells of a cell row.
    pub cells: Vec<Cell>,
    /// A row of kaomoji chips.
    pub chips: bool,
    /// Top of the row in the scrolled content.
    pub y: f32,
    /// Height of the row.
    pub h: f32,
}

/// What the grid shows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Grid {
    /// The rows, top to bottom.
    pub rows: Vec<Row>,
    /// The catalog index of every selectable cell, in reading order.
    pub items: Vec<usize>,
    /// The keyboard table for [`snipdeck_ui::symbol_nav::navigate`].
    pub nav: Vec<NavRow>,
    /// For each emoji group: the header row and the first cell, when the group is on show.
    pub groups: Vec<Option<(usize, usize)>>,
}

impl Grid {
    /// The row that holds cell `flat`.
    #[must_use]
    pub fn row_of(&self, flat: usize) -> Option<usize> {
        self.rows
            .iter()
            .position(|r| r.cells.iter().any(|c| c.flat == flat))
    }
}

struct Builder {
    grid: Grid,
    y: f32,
}

impl Builder {
    fn header(&mut self, title: Title, link: Option<Kind>) -> usize {
        let at = self.grid.rows.len();
        self.push(Row {
            title: Some(title),
            link,
            cells: Vec::new(),
            chips: false,
            y: 0.0,
            h: HEADER_H,
        });
        self.grid.nav.push(NavRow::header());
        at
    }

    fn push(&mut self, mut row: Row) {
        row.y = self.y;
        self.y += row.h;
        self.grid.rows.push(row);
    }

    /// Adds `indices` as rows of `columns` cells (emoji, icons), at most `limit` rows.
    fn cells(&mut self, catalog: &Catalog, indices: &[usize], columns: usize, limit: usize) {
        for chunk in indices.chunks(columns.max(1)).take(limit) {
            let first = self.grid.items.len();
            let cells = chunk
                .iter()
                .enumerate()
                .map(|(n, &index)| Cell {
                    index,
                    flat: first + n,
                    wide: false,
                })
                .collect();
            self.grid.items.extend_from_slice(chunk);
            self.grid.nav.push(NavRow::cells(first, chunk.len()));
            self.push(Row {
                title: None,
                link: None,
                cells,
                chips: false,
                y: 0.0,
                h: CELL_H,
            });
        }
        let _ = catalog;
    }

    /// Adds kaomoji chips, two to a row; a wide one has a row to itself. At most `limit` rows.
    fn chips(&mut self, catalog: &Catalog, indices: &[usize], limit: usize) {
        let mut rows: Vec<Vec<usize>> = Vec::new();
        let mut open: Vec<usize> = Vec::new();
        for &index in indices {
            let wide = catalog.symbol(index).is_some_and(|s| s.wide);
            if wide {
                if !open.is_empty() {
                    rows.push(std::mem::take(&mut open));
                }
                rows.push(vec![index]);
            } else {
                open.push(index);
                if open.len() == CHIP_COLUMNS {
                    rows.push(std::mem::take(&mut open));
                }
            }
        }
        if !open.is_empty() {
            rows.push(open);
        }
        for chunk in rows.into_iter().take(limit) {
            let first = self.grid.items.len();
            let cells = chunk
                .iter()
                .enumerate()
                .map(|(n, &index)| Cell {
                    index,
                    flat: first + n,
                    wide: catalog.symbol(index).is_some_and(|s| s.wide),
                })
                .collect();
            self.grid.items.extend_from_slice(&chunk);
            self.grid.nav.push(NavRow::cells(first, chunk.len()));
            self.push(Row {
                title: None,
                link: None,
                cells,
                chips: true,
                y: 0.0,
                h: CHIP_H,
            });
        }
    }

    /// Adds `indices` the way their kind lays out: chips for kaomoji, cells otherwise.
    fn section(
        &mut self,
        catalog: &Catalog,
        kind: Kind,
        indices: &[usize],
        columns: usize,
        limit: usize,
    ) {
        if kind == Kind::Kaomoji {
            self.chips(catalog, indices, limit);
        } else {
            self.cells(catalog, indices, columns, limit);
        }
    }
}

/// The grid for `tab` (0 All, 1 Emoji, 2 Kaomoji, 3 Icons) and `query`, in `columns` columns.
/// `recents` are the symbols used last, newest first. Without a query the grid opens with the
/// recents of the kind; with one it is the ranked results.
#[must_use]
pub fn build(
    catalog: &Catalog,
    tab: i32,
    query: &str,
    lang: Lang,
    recents: &[SymbolId],
    columns: usize,
) -> Grid {
    let kind = kind_of(tab);
    let mut b = Builder {
        grid: Grid {
            groups: vec![None; EMOJI_GROUPS.len()],
            ..Grid::default()
        },
        y: 0.0,
    };
    if query.trim().is_empty() {
        browse(&mut b, catalog, kind, recents, columns);
    } else {
        let hits = search(catalog, query, lang, kind);
        let of = |k: Kind| -> Vec<usize> {
            hits.iter()
                .map(|h| h.index)
                .filter(|&i| catalog.range(k).contains(&i))
                .collect()
        };
        for k in [Kind::Emoji, Kind::Kaomoji, Kind::Icon] {
            let found = of(k);
            if found.is_empty() {
                continue;
            }
            if kind.is_none() {
                b.header(Title::Kind(k), None);
            }
            b.section(catalog, k, &found, columns, usize::MAX);
        }
    }
    b.grid
}

fn browse(
    b: &mut Builder,
    catalog: &Catalog,
    kind: Option<Kind>,
    recents: &[SymbolId],
    columns: usize,
) {
    let recent: Vec<usize> = recents
        .iter()
        .filter(|id| kind.is_none_or(|k| id.kind() == k))
        .filter_map(|id| catalog.index_of(id))
        .collect();
    if !recent.is_empty() {
        b.header(Title::Recent, None);
        // Recents are a mix of kinds in "All": cells for text and icons, chips for kaomoji.
        let (kao, rest): (Vec<usize>, Vec<usize>) = recent
            .iter()
            .partition(|&&i| catalog.range(Kind::Kaomoji).contains(&i));
        b.cells(catalog, &rest, columns, usize::MAX);
        b.chips(catalog, &kao, usize::MAX);
    }
    match kind {
        None => {
            for k in [Kind::Emoji, Kind::Kaomoji, Kind::Icon] {
                b.header(Title::Kind(k), Some(k));
                let all: Vec<usize> = catalog.range(k).collect();
                b.section(catalog, k, &all, columns, ALL_ROWS);
            }
        }
        Some(k) => {
            // One section per group, in catalog order (the catalog is sorted by group).
            let mut current: Option<u8> = None;
            let mut run: Vec<usize> = Vec::new();
            let flush = |b: &mut Builder, group: Option<u8>, run: &mut Vec<usize>| {
                let Some(g) = group else { return };
                if run.is_empty() {
                    return;
                }
                let title = match k {
                    Kind::Emoji => Title::EmojiGroup(usize::from(g)),
                    Kind::Kaomoji => Title::KaomojiGroup(usize::from(g)),
                    Kind::Icon => Title::IconCategory(
                        catalog
                            .icon_categories()
                            .get(usize::from(g))
                            .cloned()
                            .unwrap_or_default(),
                    ),
                };
                let row = b.header(title, None);
                if k == Kind::Emoji
                    && let Some(slot) = b.grid.groups.get_mut(usize::from(g))
                {
                    *slot = Some((row, b.grid.items.len()));
                }
                b.section(catalog, k, run, columns, usize::MAX);
                run.clear();
            };
            for i in catalog.range(k) {
                let group = catalog.symbol(i).map(|s| s.group);
                if group != current {
                    flush(b, current, &mut run);
                    current = group;
                }
                run.push(i);
            }
            flush(b, current, &mut run);
        }
    }
}

#[cfg(test)]
mod tests {
    use snipdeck_ui::symbol_nav::{Move, navigate};

    use super::*;

    fn grid(tab: i32, query: &str, recents: &[SymbolId], columns: usize) -> Grid {
        build(Catalog::get(), tab, query, Lang::En, recents, columns)
    }

    fn cell_rows(g: &Grid) -> Vec<usize> {
        g.rows
            .iter()
            .filter(|r| r.title.is_none())
            .map(|r| r.cells.len())
            .collect()
    }

    #[test]
    fn the_emoji_kind_is_one_section_per_group_with_a_row_of_the_column_count() {
        let g = grid(1, "", &[], 7);
        let c = Catalog::get();
        assert_eq!(g.items.len(), c.range(Kind::Emoji).len());
        assert!(g.rows.iter().all(|r| r.cells.len() <= 7));
        assert_eq!(g.rows[0].title, Some(Title::EmojiGroup(0)));
        let found = g.groups.iter().flatten().count();
        assert_eq!(found, EMOJI_GROUPS.len(), "every group has a jump point");
        let (row, first) = g.groups[1].expect("People & Body");
        assert_eq!(g.rows[row].title, Some(Title::EmojiGroup(1)));
        assert_eq!(g.rows[row + 1].cells[0].flat, first);
    }

    #[test]
    fn rows_are_stacked_with_their_heights() {
        let g = grid(1, "", &[], 7);
        let mut y = 0.0;
        for r in &g.rows {
            assert!((r.y - y).abs() < f32::EPSILON);
            y += r.h;
        }
        assert!((g.rows[0].h - HEADER_H).abs() < f32::EPSILON);
        assert!((g.rows[1].h - CELL_H).abs() < f32::EPSILON);
    }

    #[test]
    fn recents_open_the_grid_and_only_the_kinds_own() {
        let c = Catalog::get();
        let emoji = c.symbols()[c.range(Kind::Emoji).start].id.clone();
        let kao = c.symbols()[c.range(Kind::Kaomoji).start].id.clone();
        let recents = [kao.clone(), emoji.clone()];
        let g = grid(1, "", &recents, 7);
        assert_eq!(g.rows[0].title, Some(Title::Recent));
        assert_eq!(g.items[0], c.index_of(&emoji).expect("index"));
        assert_eq!(
            cell_rows(&g)[0],
            1,
            "the kaomoji is not among the emoji recents"
        );
        let all = grid(0, "", &recents, 7);
        assert_eq!(all.rows[0].title, Some(Title::Recent));
        assert_eq!(all.items.len() - all.items.iter().skip(2).count(), 2);
    }

    #[test]
    fn all_shows_two_rows_of_each_kind_and_a_show_all_link() {
        let g = grid(0, "", &[], 7);
        let headers: Vec<_> = g
            .rows
            .iter()
            .filter(|r| r.title.is_some())
            .map(|r| (r.title.clone(), r.link))
            .collect();
        assert_eq!(
            headers,
            [
                (Some(Title::Kind(Kind::Emoji)), Some(Kind::Emoji)),
                (Some(Title::Kind(Kind::Kaomoji)), Some(Kind::Kaomoji)),
                (Some(Title::Kind(Kind::Icon)), Some(Kind::Icon)),
            ]
        );
        let emoji_rows = g.rows[1..].iter().take_while(|r| r.title.is_none()).count();
        assert_eq!(emoji_rows, 2);
    }

    #[test]
    fn kaomoji_are_two_chips_a_row_and_a_wide_one_is_alone() {
        let g = grid(2, "", &[], 7);
        let c = Catalog::get();
        for r in g.rows.iter().filter(|r| r.title.is_none()) {
            assert!(r.chips && r.cells.len() <= 2);
            if r.cells.iter().any(|cell| cell.wide) {
                assert_eq!(r.cells.len(), 1, "a wide chip spans both columns");
            }
        }
        assert_eq!(g.items.len(), c.range(Kind::Kaomoji).len());
    }

    #[test]
    fn a_query_is_a_ranked_flat_grid_and_all_names_the_kinds() {
        let c = Catalog::get();
        let g = grid(1, "cat", &[], 7);
        assert!(g.rows[0].title.is_none(), "one kind: no header");
        let first = &c.symbols()[g.items[0]];
        assert!(first.name.contains("cat"), "{}", first.name);
        let all = grid(0, "cat", &[], 7);
        assert!(matches!(all.rows[0].title, Some(Title::Kind(_))));
    }

    #[test]
    fn a_query_with_no_match_is_an_empty_grid() {
        let g = grid(0, "zzzzqqqq", &[], 7);
        assert!(g.rows.is_empty() && g.items.is_empty());
    }

    #[test]
    fn the_keyboard_table_follows_the_rows() {
        let g = grid(1, "", &[], 7);
        assert_eq!(navigate(&g.nav, g.items.len(), 0, Move::Down, 3), Some(7));
        assert_eq!(navigate(&g.nav, g.items.len(), 0, Move::Up, 3), None);
        assert_eq!(g.row_of(0), Some(1));
    }

    #[test]
    fn icons_are_grouped_by_category() {
        let g = grid(3, "", &[], 7);
        assert!(matches!(g.rows[0].title, Some(Title::IconCategory(_))));
        assert_eq!(g.items.len(), Catalog::get().range(Kind::Icon).len());
    }
}
