# Panel dock redesign (2026-11-12)

Replaces the dock and drag path diagnosed in `dock-diagnosis-2026-11-12.md`. Reference: the Microsoft PC Manager toolbar (drag it anywhere; dragged to a screen edge it attaches there and auto-hides; it appears again when the pointer reaches its place. Sources: makeuseof.com "How to use the toolbar in Microsoft PC Manager", techcortex.blog; no official Microsoft page describes it).

## What was wrong, in one line each

- The drag was the OS move loop: no release event (a 250 ms quiet gap stood in for it), no cursor, and Windows Snap could act on the window.
- The dock was decided by the window's sides being near an edge, so a tall Panel always docked.
- One stored frame served both the floating and the docked Panel; `pre_dock` gave sizes back at the wrong time.
- A docked, unpinned Panel was not topmost and hid on focus loss: no tab left to hover.
- The dock trigger defaulted to Off, so a docked Panel never tucked.

## The model

Three small machines, all pure (`dock.rs`, `frame.rs`), driven by the controller with the injected clock.

### 1. Placement (persisted, per monitor)

`Floating` or `Docked(edge)`. Only three things change it: a drag released on a preview, a drag pulled off the edge, and the menu / Settings "Dock to" rows.

### 2. Drag (`dock::Drag`)

The app moves the window itself from the cursor on screen; the OS move loop is gone.

```
            press on header / grip
 Idle ───────────────────────────────► Float (window follows cursor, never docks)
  ▲                                      │ cursor within EDGE_ZONE of an allowed
  │                                      │ work-area edge for DOCK_ARM
  │ release: float where dropped         ▼
  ├───────────────────────────────── Preview(edge) (window shown docked, accent frame,
  │ release: dock to edge                │           "Release to dock")
  │                                      │ cursor leaves EDGE_EXIT of that edge
  │                                      └──────────► Float
  │
  │          press on a docked Panel
  └──────── Slide(edge) (follows the cursor along the edge, stays docked)
                 │ cursor pulled UNDOCK_PULL away from the press, across the edge
                 └──────────► Float (floating size back, under the cursor)
```

| Name | Value | Why |
|---|---|---|
| `EDGE_ZONE` | 4 logical px from the work-area edge, cursor only | "the cursor is at the edge"; the window's own sides never count |
| `EDGE_EXIT` | 12 logical px | hysteresis: no flicker at the zone's border; the easy escape is to pull back 12 px |
| `DOCK_ARM` | 200 ms in the zone | crossing the seam between two monitors, or brushing an edge, docks nothing |
| `UNDOCK_PULL` | 24 logical px across the edge | a slide along the edge wobbles a few pixels; an undock is a clear pull |
| drag tick | 16 ms while dragging | the window follows the polled cursor even when pointer events stall |

- Corner: the nearer edge; on a tie left/right wins. The taskbar's edge is never offered. The candidate is kept while the cursor stays within `EDGE_EXIT` of it, so a corner cannot oscillate.
- A drag never changes a size, except the two deliberate switches: Preview shows the docked size, pulling off the edge gives the floating size back.
- Release is the real button release (the Slint pointer-up, plus a polled "primary button is up" as a safety net on Windows).
- While floating, the frame is not clamped during the drag (it must cross monitors); on release it is pulled inside the work area of the monitor under the cursor.
- On another monitor the Panel takes the floating size remembered there, else its logical size at that monitor's scale.

### 3. Reveal (`dock::DockState`, kept; only for a docked Panel whose trigger is not Off)

`Revealed → Leaving(since) → Tucked → Revealing(since) → Revealed`.

- Tucked: a 6 px tab on the edge (the handle), always topmost, pinned or not.
- Pointer in the tab's reveal zone (the tab grown by `REVEAL_ZONE` = 12 logical px) for the hover delay (150 ms) reveals it; a click on the tab reveals at once; the Panel hotkey reveals it and takes the keyboard.
- Revealed stays while "held": the pointer is within `KEEP_ZONE` (8 logical px) of the Panel, or a menu is open, or the Panel has the keyboard, or a drag or resize is under way. Not held for the leave delay (400 ms): tucks.
- The pointer is level-triggered: the binding polls the cursor every 50 ms while the dock is live (Windows), and falls back to Slint's hover events where the cursor cannot be read.
- A paste tucks at once. Close (X, Esc) and the hotkey tuck a docked Panel and give the keyboard back to the Target; they do not hide the window. The tray's toggle still hides it.
- A docked Panel no longer hides on focus loss: the tab is how it comes back.
- No slide animation: the frame is placed in one step and repainted whole (`repaint_whole`).

## Sizes

Per monitor: the floating frame (`x y w h`) and the docked size (`docked`), plus where the Panel sits along each edge (`along`, `other_along`). Docking for the first time keeps the floating size. A resize while docked changes only the docked size; a resize while floating only the floating one. Nothing else writes a size. "Docked panel uses the full edge" still stretches a side dock to the work area's height without touching the stored height.

## Persisted settings and migration

`panel_geometry[]`:

| field | before | now |
|---|---|---|
| `x y w h` | the open frame, docked or floating | the floating frame, always |
| `docked` | - | `[w, h]` while docked or after; absent: use the floating size |
| `pre_dock` | floating size while docked | read only, never written |
| `edge`, `along`, `other_along` | unchanged | unchanged |

On load (`Settings::sanitize`), once per entry: a docked entry without `docked` gets `docked = [w, h]`; if it has `pre_dock`, `w h = pre_dock`. `pre_dock` is then dropped. A side dock an old version stored full height stays full height until resized (sizes are not guessed any more).

`dock`: new `tucks_by_default` marker (absent in old files = false). On load, when false: a trigger of `off` becomes `both`, and the marker is set. Reason: Off was the old default, not a choice, and it is why a docked Panel never tucked. "Stay open" remains selectable afterwards.

## Windows-only, unverified here

- Moving the window from a polled cursor (`SetWindowPos` through winit) at 16 ms: smoothness, and no see-through gaps.
- `GetAsyncKeyState` for the release safety net.
- The 6 px topmost no-activate tab receiving hover and click, and the 50 ms cursor poll reaching it under other topmost windows.
- Per-monitor DPI while a drag crosses monitors.

Checklist: `tools/windows-test/README.md`, "Dock and drag".
