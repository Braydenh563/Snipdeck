//! The Symbols grid's arrow-key table (symbols spec §4, Keyboard): a flat selection index over rows
//! of cells, Up/Down keep the column, Left/Right walk the flat order, Up from the first row leaves
//! the grid (`None`).

use snipdeck_ui::symbol_nav::{Move, NavRow, navigate};

/// Section header, 7 + 7 + 3 cells, header, 2 + 1 kaomoji chips.
fn rows() -> Vec<NavRow> {
    vec![
        NavRow::header(),
        NavRow::cells(0, 7),
        NavRow::cells(7, 7),
        NavRow::cells(14, 3),
        NavRow::header(),
        NavRow::cells(17, 2),
        NavRow::cells(19, 1),
    ]
}

const TOTAL: usize = 20;

#[test]
fn right_and_left_walk_the_flat_order_and_stop_at_the_ends() {
    let r = rows();
    assert_eq!(
        navigate(&r, TOTAL, 6, Move::Right, 3),
        Some(7),
        "wraps to the next row"
    );
    assert_eq!(navigate(&r, TOTAL, 7, Move::Left, 3), Some(6));
    assert_eq!(
        navigate(&r, TOTAL, 19, Move::Right, 3),
        Some(19),
        "the last cell stays"
    );
    assert_eq!(navigate(&r, TOTAL, 0, Move::Left, 3), Some(0));
}

#[test]
fn down_keeps_the_column_and_clamps_to_a_short_row() {
    let r = rows();
    assert_eq!(navigate(&r, TOTAL, 3, Move::Down, 3), Some(10));
    assert_eq!(
        navigate(&r, TOTAL, 13, Move::Down, 3),
        Some(16),
        "column 6 of 3 cells: the last"
    );
    assert_eq!(
        navigate(&r, TOTAL, 16, Move::Down, 3),
        Some(18),
        "crosses the header, column clamped to 2 chips"
    );
    assert_eq!(
        navigate(&r, TOTAL, 19, Move::Down, 3),
        Some(19),
        "the last row stays"
    );
}

#[test]
fn up_keeps_the_column_and_leaves_the_grid_from_the_first_row() {
    let r = rows();
    assert_eq!(navigate(&r, TOTAL, 10, Move::Up, 3), Some(3));
    assert_eq!(
        navigate(&r, TOTAL, 19, Move::Up, 3),
        Some(17),
        "column 0 of the chips above"
    );
    assert_eq!(
        navigate(&r, TOTAL, 4, Move::Up, 3),
        None,
        "the first row hands over to the tabs"
    );
}

#[test]
fn home_end_and_pages() {
    let r = rows();
    assert_eq!(navigate(&r, TOTAL, 12, Move::Home, 3), Some(0));
    assert_eq!(navigate(&r, TOTAL, 2, Move::End, 3), Some(19));
    assert_eq!(
        navigate(&r, TOTAL, 2, Move::PageDown, 2),
        Some(16),
        "two rows down, column 2"
    );
    assert_eq!(navigate(&r, TOTAL, 16, Move::PageUp, 2), Some(2));
    assert_eq!(
        navigate(&r, TOTAL, 2, Move::PageUp, 2),
        Some(2),
        "a page up stays in the grid"
    );
}

#[test]
fn an_empty_grid_has_nowhere_to_go() {
    assert_eq!(navigate(&[], 0, 0, Move::Down, 3), None);
}
