// The drag machine's tests; part of `dock::tests` (included there).

fn screen() -> Monitor {
    Monitor {
        id: "one".to_owned(),
        work_area: Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        },
        taskbar: Some(DockEdge::Bottom),
        scale: 1.0,
    }
}

/// A second monitor to the right of [`screen`], at 150 %.
fn second() -> Monitor {
    Monitor {
        id: "two".to_owned(),
        work_area: Rect {
            x: 1920,
            y: 0,
            w: 2560,
            h: 1400,
        },
        taskbar: None,
        scale: 1.5,
    }
}

const FLOATING: (u32, u32) = (300, 400);
const DOCKED: (u32, u32) = (280, 600);

fn ctx(monitor: &Monitor, ms: i64) -> DragCtx<'_> {
    DragCtx {
        monitor,
        floating: FLOATING,
        docked: DOCKED,
        full_edge: false,
        now: at(ms),
    }
}

fn window(x: i32, y: i32, size: (u32, u32)) -> Rect {
    Rect {
        x,
        y,
        w: size.0,
        h: size.1,
    }
}

#[test]
fn a_drag_through_the_middle_never_docks_whatever_the_panels_size() {
    let screen = screen();
    // As tall as the work area: its sides touch the top and the bottom all the way.
    let tall = DragCtx {
        floating: (320, 1040),
        ..ctx(&screen, 0)
    };
    let mut drag = Drag::begin((100, 10), window(60, 0, (320, 1040)), None);
    for (step, x) in (120..1800).step_by(40).enumerate() {
        let now = i64::try_from(step).unwrap_or(0) * 300;
        let moved = drag.moved((x, 10 + 5), &DragCtx { now: at(now), ..tall });
        assert_eq!(moved, DragStep::Move(x - 40, 5), "at {x}");
        assert_eq!(drag.phase(), DragPhase::Float);
    }
    assert_eq!(
        drag.released(&tall),
        DragEnd::Floating(window(1600, 0, (320, 1040))),
        "floats where it was dropped, at its own size"
    );
}

#[test]
fn the_dock_is_offered_only_after_the_cursor_stays_at_the_edge() {
    let screen = screen();
    let mut drag = Drag::begin((500, 300), window(400, 290, FLOATING), None);
    // The window's own left side is far off screen: that docks nothing.
    assert_eq!(drag.moved((40, 300), &ctx(&screen, 0)), DragStep::Move(-60, 290));
    assert_eq!(drag.moved((5, 300), &ctx(&screen, 500)), DragStep::Move(-95, 290));
    assert_eq!(drag.phase(), DragPhase::Float, "5 px from the edge is not at it");
    assert_eq!(drag.moved((3, 300), &ctx(&screen, 1000)), DragStep::Move(-97, 290));
    assert_eq!(drag.moved((3, 300), &ctx(&screen, 1199)), DragStep::None);
    assert_eq!(drag.preview(), None, "not yet: 199 ms");
    let shown = drag.moved((3, 300), &ctx(&screen, 1200));
    assert_eq!(shown, DragStep::Place(window(0, 290, DOCKED)));
    assert_eq!(drag.preview(), Some(DockEdge::Left));
    assert_eq!(
        drag.released(&ctx(&screen, 1300)),
        DragEnd::Docked(DockEdge::Left, window(0, 290, DOCKED))
    );
}

#[test]
fn brushing_an_edge_docks_nothing() {
    let screen = screen();
    let mut drag = Drag::begin((500, 300), window(400, 290, FLOATING), None);
    let _ = drag.moved((0, 300), &ctx(&screen, 0));
    let _ = drag.moved((0, 310), &ctx(&screen, 150));
    let _ = drag.moved((30, 310), &ctx(&screen, 180));
    // Back at the edge: the wait starts again.
    let _ = drag.moved((0, 310), &ctx(&screen, 250));
    let _ = drag.moved((0, 310), &ctx(&screen, 400));
    assert_eq!(drag.phase(), DragPhase::Float);
    assert_eq!(
        drag.released(&ctx(&screen, 420)),
        DragEnd::Floating(window(0, 300, FLOATING)),
        "dropped at the edge without the offer: it floats there, inside the work area"
    );
}

#[test]
fn a_press_that_does_not_move_never_docks() {
    let screen = screen();
    // A floating Panel at the top of the screen, pressed on the top pixels of its header.
    let mut drag = Drag::begin((500, 2), window(400, 0, FLOATING), None);
    assert_eq!(drag.moved((500, 2), &ctx(&screen, 5000)), DragStep::None);
    assert_eq!(drag.phase(), DragPhase::Float);
}

#[test]
fn pulling_back_from_the_edge_takes_the_offer_away() {
    let screen = screen();
    let mut drag = Drag::begin((500, 300), window(400, 290, FLOATING), None);
    let _ = drag.moved((0, 300), &ctx(&screen, 0));
    let _ = drag.moved((0, 300), &ctx(&screen, 200));
    assert_eq!(drag.preview(), Some(DockEdge::Left));
    // Inside the exit distance the offer holds, and the docked Panel follows along the edge.
    assert_eq!(
        drag.moved((12, 340), &ctx(&screen, 300)),
        DragStep::Place(window(0, 330, DOCKED))
    );
    assert_eq!(drag.preview(), Some(DockEdge::Left));
    // One pixel further: floating again, at the floating size, under the cursor.
    assert_eq!(
        drag.moved((13, 340), &ctx(&screen, 320)),
        DragStep::Place(window(-87, 330, FLOATING))
    );
    assert_eq!(drag.preview(), None);
    assert_eq!(
        drag.released(&ctx(&screen, 400)),
        DragEnd::Floating(window(0, 330, FLOATING))
    );
}

#[test]
fn a_corner_offers_one_edge_and_keeps_it() {
    let screen = screen();
    let mut drag = Drag::begin((500, 300), window(400, 290, FLOATING), None);
    let _ = drag.moved((0, 0), &ctx(&screen, 0));
    let _ = drag.moved((0, 0), &ctx(&screen, 200));
    assert_eq!(drag.preview(), Some(DockEdge::Left), "a tie goes to the side");
    // Wobbling in the corner, nearer the top now: the offer does not flip.
    for (step, cursor) in [(3, 0), (0, 2), (4, 1), (1, 0)].into_iter().enumerate() {
        let _ = drag.moved(cursor, &ctx(&screen, 300 + i64::try_from(step).unwrap_or(0) * 50));
        assert_eq!(drag.preview(), Some(DockEdge::Left), "at {cursor:?}");
    }

    let mut top = Drag::begin((500, 300), window(400, 290, FLOATING), None);
    let _ = top.moved((3, 0), &ctx(&screen, 0));
    let _ = top.moved((3, 0), &ctx(&screen, 200));
    assert_eq!(top.preview(), Some(DockEdge::Top), "the nearer edge");
}

#[test]
fn the_taskbar_edge_is_never_offered() {
    let screen = screen();
    let mut drag = Drag::begin((500, 300), window(400, 290, FLOATING), None);
    let _ = drag.moved((800, 1039), &ctx(&screen, 0));
    let _ = drag.moved((800, 1039), &ctx(&screen, 1000));
    assert_eq!(drag.phase(), DragPhase::Float);
    // In the corner by the taskbar the side is offered.
    let _ = drag.moved((1919, 1039), &ctx(&screen, 2000));
    let _ = drag.moved((1919, 1039), &ctx(&screen, 2200));
    assert_eq!(drag.preview(), Some(DockEdge::Right));
}

#[test]
fn a_docked_panel_slides_along_its_edge_and_stays_docked() {
    let screen = screen();
    let mut drag = Drag::begin((100, 210), window(0, 200, DOCKED), Some(DockEdge::Left));
    assert_eq!(drag.phase(), DragPhase::Slide(DockEdge::Left));
    // The hand wobbles across the edge while it slides: still docked.
    assert_eq!(
        drag.moved((120, 400), &ctx(&screen, 100)),
        DragStep::Place(window(0, 390, DOCKED))
    );
    assert_eq!(
        drag.moved((80, 2000), &ctx(&screen, 200)),
        DragStep::Place(window(0, 440, DOCKED)),
        "it stops at the end of the edge"
    );
    assert_eq!(
        drag.released(&ctx(&screen, 300)),
        DragEnd::Docked(DockEdge::Left, window(0, 440, DOCKED))
    );
}

#[test]
fn pulling_a_docked_panel_off_its_edge_floats_it_at_its_floating_size() {
    let screen = screen();
    let mut drag = Drag::begin((140, 210), window(0, 200, DOCKED), Some(DockEdge::Left));
    let _ = drag.moved((164, 210), &ctx(&screen, 100));
    assert_eq!(drag.phase(), DragPhase::Slide(DockEdge::Left), "24 px is not a pull");
    // 140 of 280 across the docked header is 150 of 300 across the floating one.
    assert_eq!(
        drag.moved((165, 212), &ctx(&screen, 120)),
        DragStep::Place(window(15, 202, FLOATING))
    );
    assert_eq!(drag.phase(), DragPhase::Float);
    assert_eq!(
        drag.moved((700, 500), &ctx(&screen, 400)),
        DragStep::Move(550, 490)
    );
    assert_eq!(
        drag.released(&ctx(&screen, 500)),
        DragEnd::Floating(window(550, 490, FLOATING))
    );
}

#[test]
fn a_full_edge_dock_is_previewed_at_the_full_height() {
    let screen = screen();
    let full = |ms| DragCtx {
        full_edge: true,
        ..ctx(&screen, ms)
    };
    let mut drag = Drag::begin((1500, 300), window(1400, 290, FLOATING), None);
    let _ = drag.moved((1919, 300), &full(0));
    assert_eq!(
        drag.moved((1919, 300), &full(200)),
        DragStep::Place(window(1640, 0, (280, 1040)))
    );
}

#[test]
fn crossing_to_another_monitor_docks_nothing_and_lands_at_that_monitors_size() {
    let (one, two) = (screen(), second());
    let mut drag = Drag::begin((1500, 300), window(1400, 290, FLOATING), None);
    // Over the seam in a few ticks: the right edge of one, then the left edge of two.
    let _ = drag.moved((1917, 300), &ctx(&one, 0));
    let _ = drag.moved((1919, 300), &ctx(&one, 16));
    let _ = drag.moved((1921, 300), &ctx(&two, 32));
    let _ = drag.moved((1960, 300), &ctx(&two, 48));
    let _ = drag.moved((2400, 300), &ctx(&two, 400));
    assert_eq!(drag.phase(), DragPhase::Float);
    let there = DragCtx {
        floating: (450, 600),
        ..ctx(&two, 500)
    };
    assert_eq!(
        drag.released(&there),
        DragEnd::Floating(window(2300, 290, (450, 600)))
    );
}

#[test]
fn the_edge_zone_scales_with_the_monitor() {
    let two = second();
    let mut drag = Drag::begin((2500, 300), window(2400, 290, FLOATING), None);
    // 6 physical pixels are 4 logical ones at 150 %.
    let _ = drag.moved((1926, 300), &ctx(&two, 0));
    let _ = drag.moved((1926, 300), &ctx(&two, 200));
    assert_eq!(drag.preview(), Some(DockEdge::Left));
    // 18 physical pixels out is still inside the exit distance; 19 is not.
    let _ = drag.moved((1938, 300), &ctx(&two, 300));
    assert_eq!(drag.preview(), Some(DockEdge::Left));
    let _ = drag.moved((1939, 300), &ctx(&two, 400));
    assert_eq!(drag.preview(), None);
}

#[test]
fn a_panel_dropped_half_off_screen_is_pulled_inside() {
    let screen = screen();
    let mut drag = Drag::begin((500, 300), window(400, 290, FLOATING), None);
    let _ = drag.moved((1900, 1000), &ctx(&screen, 0));
    assert_eq!(
        drag.released(&ctx(&screen, 50)),
        DragEnd::Floating(window(1620, 640, FLOATING))
    );
}
