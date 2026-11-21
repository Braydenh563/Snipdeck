//! Arrow-key movement in the Symbols grid.
//!
//! Slint has no loops, so the grid cannot walk its own rows. The host keeps one [`NavRow`] per
//! grid row (a section header is a row with no cells) and the Panel reports each key as a
//! [`Move`]; [`navigate`] answers with the new flat selection index. Pure, Slint-free.

/// One row of the grid: a section header (no cells) or `count` cells from flat index `first`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavRow {
    first: usize,
    count: usize,
}

impl NavRow {
    /// A section header: selection skips it.
    #[must_use]
    pub const fn header() -> Self {
        Self { first: 0, count: 0 }
    }

    /// A row of `count` cells, the first one numbered `first` in the flat order.
    #[must_use]
    pub const fn cells(first: usize, count: usize) -> Self {
        Self { first, count }
    }
}

/// A key that moves the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// Left arrow: the previous cell.
    Left,
    /// Right arrow: the next cell.
    Right,
    /// Up arrow: the same column one row up.
    Up,
    /// Down arrow: the same column one row down.
    Down,
    /// Home: the first cell.
    Home,
    /// End: the last cell.
    End,
    /// Page Up: a page of rows up.
    PageUp,
    /// Page Down: a page of rows down.
    PageDown,
}

impl Move {
    /// The move for the integer the Panel reports (0 left, 1 right, 2 up, 3 down, 4 home, 5 end,
    /// 6 page up, 7 page down).
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            0 => Self::Left,
            1 => Self::Right,
            2 => Self::Up,
            3 => Self::Down,
            4 => Self::Home,
            5 => Self::End,
            6 => Self::PageUp,
            7 => Self::PageDown,
            _ => return None,
        })
    }
}

/// Where `mv` takes the selection `current` in a grid of `total` cells. `page` is how many rows a
/// page key moves. `None`: Up from the first row of cells, which leaves the grid for the tabs
/// above it (and an empty grid).
#[must_use]
pub fn navigate(
    rows: &[NavRow],
    total: usize,
    current: usize,
    mv: Move,
    page: usize,
) -> Option<usize> {
    if total == 0 {
        return None;
    }
    let current = current.min(total - 1);
    let at = rows
        .iter()
        .position(|r| r.count > 0 && (r.first..r.first + r.count).contains(&current))?;
    let column = current - rows[at].first;
    let land = |row: usize| rows[row].first + column.min(rows[row].count - 1);
    let below = |from: usize, steps: usize| {
        (from + 1..rows.len())
            .filter(|&i| rows[i].count > 0)
            .nth(steps - 1)
    };
    let above = |from: usize, steps: usize| {
        (0..from)
            .rev()
            .filter(|&i| rows[i].count > 0)
            .nth(steps - 1)
    };
    Some(match mv {
        Move::Left => current.saturating_sub(1),
        Move::Right => (current + 1).min(total - 1),
        Move::Home => 0,
        Move::End => total - 1,
        Move::Down => below(at, 1).map_or(current, land),
        Move::Up => return above(at, 1).map(land),
        Move::PageDown => {
            let page = page.max(1);
            (1..=page)
                .rev()
                .find_map(|n| below(at, n))
                .map_or(current, land)
        }
        Move::PageUp => {
            let page = page.max(1);
            (1..=page)
                .rev()
                .find_map(|n| above(at, n))
                .map_or(current, land)
        }
    })
}
