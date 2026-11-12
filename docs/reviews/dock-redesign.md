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

- Tucked: a 6 px tab on the edge (the handle), topmost, pinned or not. While a full-screen app covers the Panel's own monitor (`TargetSource::fullscreen_on`: a window with no frame that covers the monitor, in front or at the monitor's middle) and the Panel has not the keyboard, it is not topmost and the pointer reveals nothing. One on another monitor changes nothing.
- The tab is there whenever the dock is: a hidden start (`--hidden`, "When Windows starts") shows the window as the tab when the Panel was last left docked, on a monitor that is still connected, and the trigger is not Off (`Controller::start_hidden`). The window does not exist before its first show, so the controller finds that monitor itself (`find_monitor`: the newest stored place whose monitor the host still finds there) and places the window before it is made; the first show of an open Panel is placed the same way (`restore_anchor`).
- Pointer on the tab, or at the edge within `REVEAL_ZONE` = 12 logical px past either end of it, or up to one tab's thickness past the edge (where another monitor begins there and nothing stops the pointer), for the hover delay (150 ms) reveals it (`frame::on_tab`). Not in front of the tab: those pixels are the window's underneath. a click on the tab reveals at once; the Panel hotkey reveals it and takes the keyboard.
- Revealed stays while "held": the pointer is within `KEEP_ZONE` (16 logical px) of the Panel (a menu is drawn inside the Panel, so the pointer on it counts; an open menu alone does not hold), or the Panel has the keyboard, or a drag or resize is under way. A resize lasts while its button is down; where the button never reads as down (under a window of higher privilege every button reads as up) it lasts until 1 s after the window last reported a new size. Not held for the leave delay (400 ms): tucks.
- The pointer is level-triggered: the binding polls the cursor every 50 ms while the dock is live (Windows), and falls back to Slint's hover events where the cursor cannot be read.
- A paste tucks at once. Close (X, Esc) and the hotkey tuck a docked Panel and give the keyboard back to the Target; they do not hide the window. The tray's toggle tucks an open dock and opens a tucked one.
- A docked Panel no longer hides on focus loss: the tab is how it comes back.
- No slide animation: the frame is placed in one step and repainted whole (`repaint_whole`).

## The window (a note you can park)

The owner asked for "a mix between microsoft sticky notes, and the toolbar/main window for microsoft PC manager". What that means here:

| From | Taken |
|---|---|
| Sticky Notes | A title strip across the whole top that is nothing but a grab area; the window is exactly where and as big as it was left; resize from every side and corner |
| PC Manager toolbar | Drag it anywhere; pushed to a screen edge it attaches, tucks to a sliver and comes out when the pointer arrives |

- **Grab strip:** the top 24 logical px, full width, with a pill in the middle (36 x 4, wider and brighter on hover, accent while dragged), the move cursor and a tooltip. It replaces the six dots that sat between the header buttons, so the header has that room back. The top 6 px stay the resize edge; 18 px by the full width are drag.
- The header's empty space under the strip still drags, as before.
- The strip is there floating and docked: docked it slides the Panel along its edge, and a pull across the edge takes it off.
- Not changed here (left to the visual pass, to avoid two sessions restyling the same rows): the header's controls, the tabs, the rows and the footer.

## Sizes

Per monitor: the floating frame (`x y w h`) and the docked size (`docked`), plus where the Panel sits along each edge (`along`, `other_along`). Docking for the first time keeps the floating size. A resize while docked changes only the docked size; a resize while floating only the floating one. Nothing else writes a size. "Docked panel uses the full edge" still stretches a side dock to the work area's height without touching the stored height.

## Persisted settings and migration

`panel_geometry[]`:

| field | before | now |
|---|---|---|
| `x y w h` | the open frame, docked or floating | the floating frame, always |
| `docked` | - | `[w, h]` of a left or right dock, while docked or after; absent: use the floating size |
| `docked_flat` | - | the same for a top or bottom dock, kept apart from the side dock's |
| `pre_dock` | floating size while docked | read only, never written |
| `edge`, `along`, `other_along` | unchanged | unchanged |

On load (`Settings::sanitize`), once per entry: a docked entry without a docked size for its kind of edge gets `[w, h]` as that size; if it has `pre_dock`, `w h = pre_dock`. `pre_dock` is then dropped. A stored height as tall as the work area is not believed where it cannot have been chosen (`frame::dock_size`): a top or bottom dock, and a side dock stored without a place along its edge while the full edge is off, open at the floating height (or the default).

`dock`: new `tucks_by_default` marker (absent in old files = false). On load, when false and no monitor has the Panel docked: a trigger of `off` becomes `both`, and the marker is set. Reason: Off was the old default, not a choice, and it is why a docked Panel never tucked. When false and a Panel is docked, nothing changes yet: that Panel was docked under the old rules (the old first-run default, or the dock that took a Panel coming near an edge) and stays open as it was, so it does not turn into a tab on the first launch. The turn to `both` happens the next time the user docks the Panel to an edge (`DockSettings::docked_by_choice`), undocks it, or picks a trigger in Settings (`choose`). "Off" remains selectable afterwards.

## Windows-only, unverified here

- Moving the window from a polled cursor (`SetWindowPos` through winit) at 16 ms: smoothness, and no see-through gaps.
- `GetAsyncKeyState` for the release safety net.
- The 6 px topmost no-activate tab receiving hover and click, and the 50 ms cursor poll reaching it under other topmost windows.
- Per-monitor DPI while a drag crosses monitors.

Checklist: `tools/windows-test/README.md`, "Dock and drag".
