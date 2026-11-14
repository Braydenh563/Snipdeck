# Render flicker and see-through Panel on Windows: root cause and fix (2026-11-14)

Branch `claude/render-flicker`. Nothing here has run on Windows. The chain below is read from this repo and
the pinned sources (`Cargo.lock`: Slint 1.18.1, winit 0.30.13, softbuffer 0.4.8). The Windows checks
that decide it are steps 20-24 of the Quick check (`tools/windows-test/tests/quick-check.json`).

## Owner, verbatim

- 2026-11-14: "when I drag the window if flickers between being solif and transparent really fast and one time
  when I docked it in the top right corner or near it I think, it went that wierd mostly transparent with some
  elements showing and I needed to like spam click on it or unfocus the window or smth to make it fully show."
- "after saving changes, the small panel doesnt instantly properly show back but instead parts show but not all
  and the rest stays transparent"
- "I tried using the slide along the edge thingo and it went transparent for a little bit"
- v1 T1: render glitches on Windows (`docs/reviews/render-glitch-analysis.md`).

## Root cause 1: every window is composed with per-pixel alpha, so anything Windows paints is see-through

1. Slint creates every winit window with `with_transparent(true)`
   (`i-slint-backend-winit-1.18.1/winitwindowadapter.rs:811`). The app's attributes hook
   (`crates/snipdeck-app/src/main.rs` `select_backend`) did not change it. Slint's own code turns it off on
   Windows only for windows with a menu bar, for the same reason (`winitwindowadapter.rs:612-615`, Slint #8793).
2. winit turns that into `DwmEnableBlurBehindWindow` with an empty blur region
   (`winit-0.30.13/src/platform_impl/windows/window.rs:1231-1247`): DWM then composes the window by the **alpha
   channel** of its pixels.
3. winit's window class has `CS_HREDRAW | CS_VREDRAW` (`window.rs:1412`): every size change invalidates the
   whole client area **with erase**.
4. The erase is `DefWindowProcW`'s `WM_ERASEBKGND`, which fills with the class brush. Since O52 that brush is the
   theme colour (`crates/snipdeck-platform/src/windows/erase.rs`). GDI writes **alpha 0**, so on a blur-behind
   window the "theme colour" is composed as fully transparent: the desktop shows through.
5. The software renderer then presents only its damage rectangles and validates the window
   (`softbuffer-0.4.8/src/backends/win32.rs:175-204`, `i-slint-backend-winit-1.18.1/renderer/sw.rs:111-169`).
   Its frames are throttled to the display rate by a timer (`frame_throttle.rs:53-84`), so the erased state can
   be composed by DWM for a frame or more before Slint's frame replaces it.

Slint itself writes every pixel it draws with alpha 255 (`renderer/sw.rs:65,142`), so a frame Slint drew is
solid. What is see-through is any pixel Windows painted (the erase) or that no Slint frame has covered yet. So:

- Every resize of the Panel is "see-through, then solid": a dock, an undock, a tuck, a reveal
  (`binding.rs` `place`), and a preview of a full-height floating Panel (`dock.rs` `Drag::moved`; the
  usual preview keeps the floating size, pinned by
  `a_drag_only_moves_the_window_and_never_resizes_it`). This is "the slide along the edge went transparent
  for a little bit" (the slide ends in a re-place) and the dock in the corner.
- **A plain drag does not resize**, so the "flickers between solid and transparent really fast" while dragging
  is not explained by the erase above. With per-pixel alpha on, DWM composes from the window's redirection
  surface by alpha on every move (one per 16 ms tick, `binding.rs` `DRAG_TICK`); any part of it that holds
  alpha-0 pixels at that moment (an erase from an earlier resize not yet drawn over, a `WM_PAINT` that found
  nothing dirty, see root cause 2) shows the desktop for that composition. This link is **inferred, not
  measured**. With an opaque window DWM ignores alpha entirely, so whatever the exact source, no pixel can be
  see-through; the worst left is a frame of the theme colour.

The same alpha-0 composition is what makes every other render gap below **see-through** rather than merely
blank.

## Root cause 2: a frame that follows an area Windows invalidated covers only what Slint thinks changed

1. Slint's partial renderer keeps its last frame and redraws only items whose properties changed
   (`RepaintBufferType::ReusedBuffer`, buffer age 1 for every buffer presented once).
2. winit answers `WM_PAINT` by sending `RedrawRequested` and only then calling `DefWindowProcW`
   (`event_loop.rs:1276-1297`). When Windows invalidated part of the window itself (the window shown again, an
   area uncovered or dragged back on screen, a DPI change), nothing is dirty in Slint, so the frame presents
   nothing there. `DefWindowProcW` then erases that area with the brush (alpha 0) and validates it.
3. The area stays like that until something inside it changes: hovering redraws a control at a time ("mostly
   transparent with some elements showing"), a click or focus change redraws more ("spam click on it or unfocus
   the window").
4. The existing remedy (`RepaintNudge`, `repaint.rs` `Watch`) covered only the Panel and the Main window, only
   for focus gained, un-occluded, restore, DPI and theme, and laid the damage **at the event**. On `Resized` that
   is before Slint knows the new size (the hook runs first: `winitwindowadapter.rs:1258-1265`), so the damage
   covered the old size. About, Edit, Tags, Settings, the Picker, toasts and the hint had no remedy at all. The
   candidate fix for this (`SNIPDECK_DIAG=fix`, `diag.rs`) was never turned on outside the diagnostic.

Cases that match the reports:

- **Show after save**: the Panel is hidden while the editor is open and shown again when it closes
  (`binding.rs` `Effect::ShowPanel`). `repaint_whole` damages the whole window then, but that frame is drawn
  before Windows maps the window and is thrown away (`erase.rs` notes this); the `WM_PAINT` Windows sends once
  it is mapped finds nothing dirty. "Parts show but not all and the rest stays transparent."
- **Dock near the top-right corner**: the drop places the Panel at the docked size (`drag_end` ->
  `PlaceWindow`), which erases all of it (root cause 1), and a corner on a multi-monitor desktop can also bring a
  DPI change and a second resize from Windows. Any `WM_PAINT` that lands with nothing dirty leaves the erased
  area see-through until a click.

## Ruled out or not the cause

- **Opacity or visibility toggled per drag step**: no. `drag_move` returns only `MoveWindow` or `PlaceWindow`
  (`controller/dock_ctl.rs:777-805`); no layered style or `SetLayeredWindowAttributes` exists in the tree
  (`window_style.rs`). Pinned by the new test
  `a_drag_only_moves_the_window_and_never_resizes_it`.
- **`SWP_NOREDRAW` / `SWP_NOCOPYBITS` in the move loop**: not used. winit's `set_outer_position` uses
  `SWP_ASYNCWINDOWPOS | SWP_NOZORDER | SWP_NOSIZE | SWP_NOACTIVATE` and then invalidates the whole window without
  erase (`window.rs:199-209`); the frame that follows is drawn whole (`binding.rs` `repaint_whole` on every
  `MoveWindow`). A plain move erases nothing.
- **A missing redraw request after a geometry change in the app's own code**: `place`, `MoveWindow` and
  `ShowPanel` all call `repaint_whole` (`binding.rs:98-117, 445-460, 486-495`). The gap is the OS-made paint and
  the per-pixel alpha, above. Pinned by `every_placement_dock_commit_and_show_draws_the_panel_whole`.
- **Back buffer uninitialised after a resize**: softbuffer makes a new DIB on a size change and reports age 0,
  so Slint draws it whole (`NewBuffer`). Not the cause on its own; root cause 1 is what makes the gap before
  that frame see-through.

## The fix

1. **Opaque windows** (`window_kind::Plan::transparent`, always `false`, applied with `with_transparent` in the
   attributes hook). DWM then ignores the alpha channel: an erase shows the theme colour (what O52 meant), never
   the desktop. Every window background is an opaque palette colour (`bg.app`, `bg.overlay`,
   `design/tokens/palette.json`), so nothing needs per-pixel alpha. Mica (DESIGN.md §5, unbuilt) would need its
   own decision then.
2. **A whole frame after anything Windows may have drawn over, for every window** (`repaint.rs`). `Watch` now
   owes a whole frame after focus gained, visible again, restore, **every real resize**, DPI and theme, and lays
   the damage **at the frame** (`RedrawRequested`, when Slint knows the size). A frame is also drawn whole when
   Windows holds an update region of its own at that moment (`GetUpdateRect` in
   `snipdeck-platform/src/windows/update_region.rs`, read before `DefWindowProcW` empties it). The Panel, the Main
   window, every regular window (`track_focus`) and every popup window (`popup_window::Shell::ensure`) follow
   it. Ordinary frames (hover, typing) stay partial.
3. `SNIPDECK_DIAG=fix` is gone (the fix is permanent); the word still switches the render log on.

Tests (Linux, fakes and a fixed clock): `repaint::tests` (10), `window_kind::tests::no_window_is_composed_see_through`,
the drag test above, the binding test above, `diag::tests`.

## Unverified (Windows only)

- That DWM composes a non-blur-behind window opaquely with this renderer on the owner's PC (expected; Slint does
  the same for menu-bar windows).
- That `GetUpdateRect` during `RedrawRequested` sees the update region in every case. A `WM_PAINT` handled while
  winit is busy (`should_buffer`) is erased and validated before the app hears of it; a resize or a show that
  precedes it still owes a whole frame, but a bare uncover during a busy callback would not be caught.
- The exact source of the alpha-0 pixels during a plain drag (no resize happens; see root cause 1).
- Whether the resize erase is now visible as a theme-coloured blink during the preview. If so, the next step is
  to answer `WM_ERASEBKGND` with 1 once a window has presented a frame.
- Multi-monitor corners with two scales (a DPI change during the drop).

Quick check steps 20-24 cover each of these.
