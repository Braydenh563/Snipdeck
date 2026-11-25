# Dock redesign review (529fd6b)

Five lenses (state machine, Win32, owner's ask, UX, tests). Each finding was checked by an independent verifier who tried to refute it. 30 of 37 confirmed. Numbered for replies. Owner's complaint: docs/reviews/dock-diagnosis-2026-11-12.md.

Some findings overlap and should be fixed once: 4/8/13 (one docked size shared by every edge), 9/16 (a resize is not counted as held), 20/25 (untranslated strings), 21/27 (strip height docs).

Fix order: blockers, then majors, then minors. Tests first: each fix gets a test that fails before it.

## 1. [major] A deliberate tuck (Close/Esc/hotkey/paste) re-reveals at once when the pointer is still in the tab's reveal zone
`crates/snipdeck-app/src/controller/dock_ctl.rs:174` (state lens)

**Evidence.** dock_tuck (174-186) and DockEvent::Pasted (dock.rs:116-118) only set DockState::Collapsed. The next dock_tick (dock_ctl.rs:155-158) sends HandleHover(near(cursor, tab, zone(REVEAL_ZONE))), and dock.rs:137-144 moves Collapsed + HandleHover(true) to Revealing, which expands after hover_delay_ms (150). Nothing requires the pointer to leave the zone first after a deliberate tuck. Header: close IconButton side-override Space.s8 = 32px with padding-right Space.s1 = 4px (panel.slint:1533-1536, 1672-1675), so it spans x in [W-36, W-4]. The reveal zone of a right tab is x >= W-6-12 = W-18. The test closing_a_docked_panel_tucks_it... (tests/dock.rs:293) never sets a cursor, so it cannot catch this.

**Scenario.** Panel docked right, trigger Both (the new default). The user clicks the right half of the X button, which is within 18 px of the screen edge. close() runs dock_tuck and the window becomes the tab. 50 ms later the tick sees the still pointer inside the reveal zone. 150 ms later the Panel slides out again, so it looks like Close does nothing or bounces. The same happens after pasting a row by clicking within 18 px of the docked side (left-dock row starts, right-dock scrollbar or row ends), and after the hotkey or Esc tuck with the pointer resting at the edge. This undoes the 'a paste tucks at once' and 'Close tucks' promises.

**Fix.** After a deliberate tuck (dock_tuck, Pasted, Hotkey), arm the hover reveal only once the pointer has left the reveal zone. For example, keep a `reveal_needs_leave` flag that dock_tuck and Collapse-by-Pasted set and the first HandleHover(false) clears, and ignore HandleHover(true) while it is set (HandleClick and the hotkey still reveal). Add a controller test with the cursor on the close button of a right dock.

**Verifier.** I could not refute this finding. At 529fd6b, nothing stops the Panel from re-opening while the pointer is still in the reveal zone right after a deliberate tuck.

How Close, Esc and the hotkey tuck the Panel:
- `close()` (controller.rs:907-909) and the Panel hotkey (controller.rs:799) call `dock_tuck` (dock_ctl.rs:174-186).
- `dock_tuck` only sets `DockState::Collapsed` and places the tab. It sets no flag that blocks the next hover reveal.
- A paste goes through `dock_after_use`, which sends `DockEvent::Pasted`. In dock.rs:116-118 that also just collapses.

What happens on the next tick:
- `dock_live` is still true, because the window stays visible and docked. So `dock_tick` (dock_ctl.rs:150-168) reads the real cursor and sends `HandleHover(near(cursor, tab, zone(REVEAL_ZONE)))`.
- dock.rs:137-144 then moves `Collapsed` + `HandleHover(true)` to `Revealing`. 150 ms later, dock.rs:149-153 expands the Panel.
- The defaults are trigger `Both` and `hover_delay_ms` 150 (settings.rs:217-218), so hover reveal is on.

The geometry puts part of the X button inside the zone:
- `handle_rect` puts a right-edge tab 6 px wide, flush with the edge (frame.rs:16, 244-248). It spans the open Panel's own height, so the header row is within its vertical range.
- `near` grows the tab by 12 px, so the zone covers x >= W-18.
- The close button is the last item in the header row (panel.slint:1672-1675). It is 32 px wide (`Space.s8`) with 4 px right padding (`Space.s1`, panel.slint:1534), so it spans [W-36, W-4].
- The button's centre (W-20) is just outside the zone, but its right ~14 px are inside it.

Scenario: a right-docked Panel. The user clicks the right half of the X. The Panel tucks, and the very next 50 ms tick sees the still pointer in the zone. The Panel slides back out 150 ms later, so Close looks like it bounced. The same happens:
- after a paste clicked near the docked edge;
- after the hotkey or Esc tuck with the pointer resting at the edge.

Nothing guards against this:
- I found no "must leave the zone first" flag anywhere in the controller or in `DockState`.
- The design note (dock-redesign.md:61, 64) promises that "a paste tucks at once" and "Close tucks", with no exception for this case.
- The controller tests never place the cursor in the zone after a tuck.

It is not a blocker: it needs the pointer within about 18 px of the docked edge, and moving the pointer away still lets the Panel tuck after the leave delay. But it is visibly wrong behaviour on a default-path control, and it adds to the owner's "unusable" complaint. So major.

## 2. [major] Reveal zone (12 px) is wider than the keep zone (8 px) along the edge: a still pointer makes the Panel open and tuck in a loop
`crates/snipdeck-app/src/dock.rs:189` (state lens)

**Evidence.** REVEAL_ZONE = 12 (dock.rs:189) and KEEP_ZONE = 8 (dock.rs:191). dock_ctl.rs:129 makes the tab span exactly open.along(edge). So along the edge, near(cursor, tab, 12) is true for y in [Y-12, Y+H+12) and near(cursor, open, 8) only for [Y-8, Y+H+8) (frame.rs:500-506). Collapsed: HandleHover(true), then Revealing, then Expand after 150 ms. Expanded: PanelHover(false), then Leaving, then Collapse after 400 ms. Then HandleHover(true) again, and so on. Across the edge the open Panel covers the zone, so only the along ends (and the full-edge tab's ends next to the taskbar) are affected. The band is 4 physical px at 100 % and 6 px at 150 %.

**Scenario.** Right dock, Panel from y=200 to y=700. The pointer rests at (W-3, 710), just below the tab's end, for example on the way to the corner. Tick: near tab, so it reveals after 150 ms. Tick: 10 px below the open Panel is more than 8, so Leaving, and it tucks after 400 ms. Tick: near tab again, so it reveals. The Panel flashes about every 600 ms with the mouse still. This is the 'spasming' the owner reported.

**Fix.** Make the keep test a superset of the reveal test: keep while near(open, KEEP_ZONE) OR near(tab, REVEAL_ZONE), or set KEEP_ZONE >= REVEAL_ZONE. Add a dock_tick test with the cursor 10 px past the tab's end that asserts no Collapse after a long advance.

**Verifier.** I tried to refute this and could not; the code at 529fd6b matches the finding. dock.rs:189 sets REVEAL_ZONE = 12 and dock.rs:191 sets KEEP_ZONE = 8. In dock_ctl.rs:157, a collapsed Panel reveals when frame::near(cursor, tab, scaled 12). In dock_ctl.rs:160, an open Panel stays only while dock_held() || frame::near(cursor, open, scaled 8). frame::handle_rect (frame.rs:224-254) builds the tab from cut(open.along(edge)). So along the edge the tab and the open Panel cover the same span, and the reveal test is wider than the keep test by 4 logical px at each end. frame::near (frame.rs:500-506) is a plain rect inflate. Across the edge the open Panel covers the reveal zone, so only the along ends are affected, as the finding says.

Nothing breaks the loop. DockState::on (dock.rs:107-160) sends Collapsed + HandleHover(true) to Revealing. Revealing + Tick after the hover delay (default 150 ms) expands. Expanded + PanelHover(false) goes to Leaving. Leaving + Tick after the leave delay (default 400 ms) collapses. dock_act / dock_tuck add no re-arm gate, cooldown or "pointer must leave first" condition. A hover reveal does not set panel_focused, and dock_held() is false when no menu, keyboard focus or drag is active.

No test covers the gap. In controller/tests/dock.rs:317-345, the test checks a pointer 9 px across the edge from the tab (reveals) and 5 px beside the open Panel (stays). It never checks a point 8-12 px past the tab's along-edge end. So with a still pointer at, say, (W-3, Y+H+10) on a right dock, the Panel reveals, tucks and reveals again about every 550-600 ms.

This is easy to hit with the full-edge tab: the tab's end sits on the work-area border, and the band lies on the taskbar just below it, where the pointer often rests. I rate it major, not blocker: it is visible, wrong flicker, but only in a 4 logical px band (4-6 physical px). It is not proven to be the owner's drag-to-corner "spasming", which was about drags.

## 3. [major] An open Panel menu keeps a docked Panel revealed forever: clicking another app never closes it
`crates/snipdeck-app/src/controller/dock_ctl.rs:135` (state lens)

**Evidence.** dock_held() = menu != PanelMenu::None || panel_focused || drag (135-137), and dock_event drops PanelHover(false) while a menu is open (89-91). The Panel menus close only on Escape or a click on the in-window scrim (panel.slint:2741, 2754; menu.slint:135-141). on_foreground_changed (controller.rs:966-990) and set_panel_focused never close self.menu. A docked Panel no longer hides on blur (hides_on_blur, controller.rs:996). A hover-revealed Panel never takes the keyboard (binding.rs:1130-1131 comment), so the menu's Escape cannot reach it either.

**Scenario.** The Panel tucks on the right and is revealed by hover. The user opens the kebab (or a row's right-click menu, or the lists menu), changes their mind, moves the mouse to the editor and clicks there. The menu stays open, dock_tick reports PanelHover(true) every 50 ms, and the topmost Panel stays out over the editor until the user goes back and clicks inside it. This is the opposite of the PC Manager behaviour the owner asked for.

**Fix.** Close the Panel's menu when the docked Panel is left. For example, on_foreground_changed sets menu = None when dock_active(). Or count a menu as held only while the pointer is within KEEP_ZONE or the Panel has the keyboard, and dismiss it in dock_act(Collapse) (which already clears it). Add a test: menu open, cursor away, foreground changes, and the Panel tucks after the leave delay.

**Verifier.** I tried to refute this and could not. Nothing at 529fd6b closes a Panel menu when the user leaves the Panel. On a docked Panel an open menu therefore keeps it out indefinitely.

Evidence:
1. `dock_held()` at dock_ctl.rs:135-137 counts any `menu != PanelMenu::None` as held. While held, `dock_tick` sends `PanelHover(true)` (dock_ctl.rs:157-164), whether or not it can read the cursor. `dock_event` also drops `PanelHover(false)` while a menu is open (dock_ctl.rs:89-91).
2. `on_foreground_changed` (controller.rs:966-990) clears the Picker and the fill form but never `self.menu`. `hides_on_blur` (controller.rs:993-1010) returns false when `dock_active()`, so the Panel does not hide either. `set_panel_focused` only stores the flag.
3. The menu is cleared only by the menu handlers in panel.rs, panel_ctl.rs, panel_select.rs and symbol_ctl.rs, by `hide`, by `panel_window_gone`, and by `dock_act(Collapse)`. Collapse can never happen while the menu holds the Panel.
4. The Panel window does not activate on a click. window_style.rs:15 answers `WM_MOUSEACTIVATE` with `MA_NOACTIVATE`, and binding.rs:1128-1129 says the Panel takes the keyboard only for a click into the search box. So opening the kebab, a row menu or the lists menu leaves the keyboard with the other app. Escape never reaches the menu, which only gets Slint-internal focus via `focus-menu()`. The only other way to close it is the in-window scrim TouchArea (panel.slint:2740-2742).
5. Nothing dismisses a menu on an outside click. The "click outside" doc comments refer to the in-window scrim only. Searches found no global hook and no foreground-driven close.
6. The test `leaving_the_panel_while_a_menu_is_open_neither_collapses_nor_closes_it` (tests/dock.rs:504) treats "menu open means stay out" as intended. No test covers a foreground change or an outside click with a menu open.

Failure path: a hover-revealed docked Panel; the user opens the kebab, then clicks into another app. The menu stays open, `dock_held` stays true and the topmost Panel stays expanded over the other app until the user goes back and clicks its scrim. That is visibly wrong and the opposite of the PC Manager behaviour the owner asked for.

Rated major, not blocker: there is a recovery (click back into the Panel) and no data loss. This is inferred from the code; the Windows behaviour has not been run.

## 4. [major] One docked size per monitor is shared by every edge, and the old full-height cut was removed: a side dock's height becomes a top/bottom dock's depth
`crates/snipdeck-app/src/controller/dock_ctl.rs:409` (state lens)

**Evidence.** dock_to uses geometry.docked for any edge (409). sizes_on returns one docked (w,h) per monitor (365-366), which drag Preview uses for every edge (dock.rs:345-355, docked_at). drag_end stores it back (592-600). docked_rect gives a top/bottom dock height = size.1 (frame.rs:308). The diff deletes frame::dock_size / dock_size_for / full_height_cut, which cut a full-height size for horizontal edges. The settings migration keeps an old side dock's full height as `docked` (settings.rs sanitize: docked = Some((g.w, g.h)) from the old docked frame; docs/reviews/dock-redesign.md:97 'stays full height').

**Scenario.** An upgraded user had a left dock stored as 340x1040 (full height), so it migrates to docked = (340,1040). They drag the Panel to the top edge. The Preview, and the dock after release, is 340 wide and min(1040, work_area.h) tall: a top 'toolbar' that covers the whole height of the screen. Even without migration, a 320x900 side dock dragged to the top becomes 900 px deep. This is the 'size reset / I hate how window sizing works' class of complaint.

**Fix.** Keep the docked size per orientation (vertical vs horizontal edges), or store only the across thickness per orientation and take the along length from the floating size. At minimum, restore the cut so a horizontal dock never takes a full-height or near-full-height depth. Add a test: side dock 320x900, drag to the top, and the depth stays reasonable.

**Verifier.** I tried to refute this and could not; the finding holds at 529fd6b. Each monitor keeps one docked size, and every edge uses it:
- `sizes_on` returns `g.docked.unwrap_or((g.w, g.h))` (dock_ctl.rs:365-366), and the drag preview passes that same `ctx.docked` to `docked_rect` (dock.rs:345-355).
- `dock_to` uses `geometry.docked` for any edge (dock_ctl.rs:409).
- `drag_end` writes it back as `docked = (own.w, own.h)` (dock_ctl.rs:600).
- A docked resize also writes it back (panel.rs:777-779).

For a top or bottom edge, `docked_rect` sets the depth to `size.1.max(min_h).min(work_area.h)` (frame.rs ~308), with nothing to stop a full-height depth. The diff deletes `frame::dock_size`, `dock_size_for` and `full_height_cut`. In the old code those replaced a full or near-full height (within 2 px of the work area) with the size before docking, or with the default height, whenever the edge was horizontal.

The full-height case can be reached in two ways:
1. Migration. `settings.rs:1135-1139` sets `docked = (g.w, g.h)` from the old docked frame, and the design note (dock-redesign.md:97) says an old full-height side dock "stays full height". SPEC's first-run default was a right dock at full height, so upgraded users get `docked = (340, 1040)`. Dragging to the top then gives a 340 x 1040 "toolbar" that covers the whole work area, both in the preview and after release.
2. No migration needed. The compact side dock (the default, `full_edge = false`) lets the user resize its ends, so it can be stretched to near full height and stored that way by panel.rs:779. Dragging it to the top does the same thing.

The finding's other case, a 320x900 side dock becoming 900 px deep at the top, is not a regression: the old code did not cut a height that was not full either. That case is weaker but still a sign of the same flaw.

Severity is major, not blocker: it is visible wrong sizing that matches the owner's complaint about window sizing, but the user can recover by resizing, and no data is lost and nothing crashes. No test covers moving from a side dock to a top dock with a tall stored size.

## 5. [major] Monitor removal or a work-area change while the Panel is tucked leaves the controller on a stale monitor; a tab click sends the Panel off-screen
`crates/snipdeck-app/src/controller/panel.rs:726` (state lens)

**Evidence.** remember_frame returns early when dock_collapsed() (726-729), before `self.monitor = Some(monitor.clone())` (730). So Moved/Resized reports while tucked never update self.monitor. dock_rects and dock_place (dock_ctl.rs:117-131, 233-257) place the tab and open frame from self.monitor's stored work_area. restore_frame is the only other path that refreshes it, and it runs only on ShowPanel (main.rs:644-646). No display-change handler exists (grep shows no WM_DISPLAYCHANGE). Tucking is now the default (DockSettings::default trigger Both, plus the migration), so this state is common.

**Scenario.** The Panel is docked and tucked on monitor 2. The user undocks the laptop and monitor 2 disappears. Windows moves the 6 px tab window onto monitor 1. The Moved report is ignored and self.monitor stays monitor 2. Hovering the tab never reveals, because dock_tick tests the cursor against a tab rect on the vanished monitor. Clicking the tab (on_handle_clicked, then HandleClick) expands to docked_rect on monitor 2's work area, which is off-screen. The Panel disappears and comes back only after two hotkey presses (tuck, then show, then restore_frame via monitor_of). A taskbar move or resize on the same monitor (same monitor id, stale work_area) misplaces the tab and its hover zone the same way.

**Fix.** In remember_frame, while collapsed, still compare the reported monitor (id and work_area) with self.monitor. When it differs, update self.monitor and re-place the tab (dock_place(true)) for the new monitor, or fall back to restore_frame(monitor). Add a test: collapsed, frame_moved on another monitor, then a HandleClick that places on the new monitor.

**Verifier.** I confirmed this by reading the code at 529fd6b. In crates/snipdeck-app/src/controller/panel.rs:726-729, remember_frame returns early when dock_collapsed() || dragging(). That happens before `self.monitor = Some(monitor.clone())` at line 730, so a Moved or Resized report that arrives while the Panel is tucked never updates self.monitor. The host forwards Moved and Resized only through main.rs:733-754 (frame_moved/frame_changed). No WM_DISPLAYCHANGE or work-area handler exists: the only WM_DISPLAYCHANGE in the repo is in paint_diag.rs, which is diagnostics. self.monitor is refreshed only by restore_frame (panel.rs:837-842, reached via Effect::ShowPanel at main.rs:644-646), restore_anchor and drag_end.

dock_tick (dock_ctl.rs:149-169) tests the cursor against dock_rects() (dock_ctl.rs:117-131), which uses the stale monitor.work_area. Hover cannot reveal a tab that Windows moved off the removed monitor. dock_hover (dock_ctl.rs:101-106) ignores the window's own enter events whenever the cursor can be read, so it gives no fallback. A tab click (binding.rs:1126, HandleClick, then dock.rs:154 Expand, then dock_place(false) at dock_ctl.rs:233-257) places the open Panel on the vanished monitor's work area, off-screen. The open Panel then has no hover keep-zone the user can reach, so it re-tucks after the leave delay into an off-screen tab.

Recovery exists but is not obvious. While tucked, the Panel hotkey calls show() (controller.rs:798). show() first emits a stale PlaceWindow, then ShowPanel, then restore_frame(monitor_of(window)), which re-reads the real monitor. So one hotkey press recovers from tucked, and two are needed if the click left it expanded (tuck, then show), as the finding says.

The early return is not new: fee4a99d had `if self.dock_collapsed() { return }`. The redesign makes the state common, though: DockSettings::default is trigger Both with tucks_by_default (settings.rs:214-223), and the migration moves Off to Both. Two caveats lower confidence in the severity, not in the defect.
- Windows relocating the tab window on monitor removal is Windows-only behaviour I could not verify here.
- The work-area-change sub-case (taskbar moved) sends no Moved event at all, and it affects an expanded docked Panel too, so it is not tucked-specific.

I rated it major because it is visible wrong behaviour (an off-screen Panel, a dead hover tab) in a now-default state. It is not a blocker: it needs a monitor or work-area change, and the hotkey or tray recovers it.

## 6. [major] Drag-release safety net reads GetAsyncKeyState failure (0) as 'button up': the drag ends 16 ms after the press when the Target is elevated (and likely with pen/touch)
`crates/snipdeck-app/src/controller/dock_ctl.rs:505` (win32 lens)

**Evidence.** dock_ctl.rs:505-507 `self.drag.is_some() && self.target.primary_down() == Some(false)`; platform.rs `fn primary_down -> Some(screen::primary_button_down())`; screen.rs:49-55 returns `GetAsyncKeyState(..) < 0`. The binding polls this on every DRAG_TICK (binding.rs:604-612) and calls drag_finish when it is true. Microsoft Learn (GetAsyncKeyState): 'The return value is zero if the call fails. Reasons for failure include ... UI Privilege Isolation (UIPI) prevents the calling thread from accessing the foreground thread', and also when the current desktop is not the active one. Because the Panel is WS_EX_NOACTIVATE + MA_NOACTIVATE (window_style.rs), pressing its grab strip leaves the Target as the foreground window. Separately, winit 0.30.13 consumes WM_POINTERDOWN/UPDATE/UP for pen and touch and returns 0 after SkipPointerFrameMessages (event_loop.rs:1985-2129), so no mouse messages are promoted. That means VK_LBUTTON probably never reads as down, and GetCursorPos probably does not follow the contact either.

**Scenario.** The Target is an admin terminal, an elevated installer, or any other elevated window, and it stays foreground because the Panel never activates. The user presses the grab strip and starts dragging. On the first 16 ms tick GetAsyncKeyState fails and returns 0, drag_let_go() is true, and drag_finish() runs drag_end. The Panel drops where it is (clamped) while the button is still held, and later moves do nothing because self.drag is None. Every drag ends after a few pixels. This is the owner's 'hard to drag the small panel around' and it shows only on real Windows. Pen or touch drags on a Surface-type device most likely fail the same way (I could not verify this without Windows).

**Fix.** Trust 'up' only after this drag has seen 'down'. Store a `seen_down: bool` in the Drag, or in the controller next to it, and set it on any tick where primary_down() == Some(true). Make drag_let_go `seen_down && primary_down() == Some(false)`. A failed or absent reading then falls back to the window's own pointer-up, which winit's SetCapture delivers anyway. Add a controller test with a fake host whose primary_down() is always Some(false): the drag must survive ticks and end only on drag_end.

**Verifier.** The elevated-window case holds up in the code at 529fd6b. The pen/touch case is speculation I could not check.

Code path:
- dock_ctl.rs:505-506: `drag_let_go()` is `self.drag.is_some() && self.target.primary_down() == Some(false)`. Nothing records that the button was ever seen down, and there is no grace period.
- platform.rs:88-89 always returns `Some(screen::primary_button_down())`.
- screen.rs:49-55 returns `GetAsyncKeyState(key) < 0`, so a failed call (0) reads as "up".
- binding.rs:1108-1109: `on_drag_began` calls `drag_begin` and then `arm_drag()`.
- binding.rs:597-612: the repeating `DRAG_TICK` timer checks `drag_let_go()` first. If it is true it calls `drag_finish()` (binding.rs:637-641), which stops the timer and runs `Controller::drag_end`. After that, later `on_drag_moved` calls do nothing because `self.drag` is `None`.

Microsoft Learn (fetched now) says GetAsyncKeyState returns zero if the call fails. One listed reason is that "UI Privilege Isolation (UIPI) prevents the calling thread from accessing the foreground thread". That covers a non-elevated Snipdeck while an elevated window is in the foreground.

The Panel stays out of the foreground when pressed. It has `WS_EX_NOACTIVATE` (win_logic.rs:265-267) and answers `WM_MOUSEACTIVATE` with `MA_NOACTIVATE` (window_style.rs:249-250). So pressing the grab strip leaves the elevated Target (admin terminal, installer) in the foreground. The app already treats elevated Targets as a normal case (target.rs:410, `route::CopyReason::Elevated`).

Result: the first tick, about 16 ms after the press, reads `Some(false)`, and the drag ends while the button is still held. The Panel drops at its starting position (clamped) and does not follow the pointer. This shows only on real Windows.

The controller tests cannot catch it. The test host's `primary_down` defaults to `None` (host.rs:209-211), and no test drives `drag_let_go` with a host that reports "up" while the button is pressed.

Refutation attempts that failed:
- The Panel is foreground only after an explicit `SetForegroundWindow` (target.rs:346, the search box), not on a grab-strip press.
- The swapped-button mapping in screen.rs is correct, so it is not the cause.

Severity: major, not blocker. The failure needs an elevated foreground window, which is realistic for a developer's admin terminal. It is not shown to be the general cause of the owner's "hard to drag" complaint. The suggested fix is sound: trust "up" only after "down" has been seen during this drag, otherwise fall back to the window's own pointer-up, plus a fake-host test that reports `primary_down() == Some(false)` on every tick.

## 7. [major] A drag across monitors with different DPI uses a grab offset measured at the old scale while Windows/winit resizes the window: the Panel runs off the cursor and lands offset
`crates/snipdeck-app/src/dock.rs:410` (win32 lens)

**Evidence.** In DragPhase::Float, dock.rs:410 places the window at `sub(cursor, self.grab)` with `grab` in the physical pixels of the press window. `grab` is only rescaled in `float()` (dock.rs:319-332), which runs on leaving Preview or Slide, never on a monitor change. With the OS move loop gone, winit's WM_DPICHANGED handler (winit-0.30.13 windows/event_loop.rs:2216ff) still resizes the window to the same logical size at the new DPI (`old_physical_inner_size.to_logical(old).to_physical(new)`). Its cursor-keeping bias only applies when `MARKER_IN_SIZE_MOVE` is set, and that flag is only set inside WM_ENTERSIZEMOVE, which this drag never enters. `released()` (dock.rs:423-433) then combines the stale x/y with the new monitor's floating size. The only cross-monitor test (dock_drag_tests.rs:239) goes from 100% to 150%, the direction that hides the bug.

**Scenario.** A 320-logical-wide Panel sits on a 200% laptop screen (640 px wide). The user presses the pill in the middle of the grab strip, so grab.x is 320 px, and drags onto a 100% external monitor. Windows sends WM_DPICHANGED and winit resizes the window to 320 px. Every following MoveWindow puts its left at cursor-320, so the cursor sits on or past the Panel's right edge and the Panel trails to its left. On release, frame::clamp keeps x = cursor-320, so the Panel lands one window-width left of where it was dropped. From 150% to 100%, any press in the right third of the header leaves the cursor outside the window. If the size stored for that monitor differs, the Panel also jumps size on release.

**Fix.** Rescale the grab offset when the drag's monitor scale changes. Keep the scale of the monitor the grab was measured on in Drag. In moved(), when ctx.monitor.scale differs, scale grab (and grab_w and frame w/h) by new/old, or reuse float()'s proportional rescale against the new floating size. Add a test that drags from a 2.0 monitor to a 1.0 monitor and asserts the cursor stays inside the released frame at the same relative header position.

**Verifier.** I tried to refute this and could not. The defect is real at 529fd6b. Nothing in the dock code handles a change of monitor scale during a drag.

1. `crates/snipdeck-app/src/dock.rs:282`: `Drag::begin` sets `grab = cursor - window.xy` in the physical pixels of the press monitor. Only `float()` (dock.rs:319-342) rescales it, and only on leaving Preview or Slide. In `DragPhase::Float`, `moved()` (dock.rs:410-415) sends `Move(cursor - grab)` and never looks at `ctx.monitor.scale`. `drag_ctx` (`controller/dock_ctl.rs:475-487`) switches to the monitor under the cursor but leaves `grab` alone.

2. The host applies Move by calling `window.set_position(PhysicalPosition)` (`binding.rs:478-484`). That is winit `SetWindowPos`. The process is PerMonitorV2 (`snipdeck-platform/src/windows/dpi.rs:29`), so when the window's monitor changes DPI, Windows sends WM_DPICHANGED.

3. winit 0.30.13 handles WM_DPICHANGED in `windows/event_loop.rs:2216ff`. It resizes the window to `old_physical.to_logical(old).to_physical(new)`, the same logical size. Its cursor-keeping bias runs only when `MARKER_IN_SIZE_MOVE` is set. That flag comes from WM_ENTERSIZEMOVE, and this app-driven `SetWindowPos` drag never enters it. So the window shrinks or grows around a stale `grab`.

4. Concrete case: a 640 px wide Panel on a 200% screen, pressed mid-header (`grab.x` = 320), dragged onto a 100% monitor. The window becomes 320 px wide, and every later Move puts its left edge at cursor-320, so the cursor sits on its right edge. On release, `released()` (dock.rs:423-433) keeps `self.frame.x` and only swaps w/h for the new monitor's floating size. `frame::clamp` leaves x alone inside the work area, so the Panel lands about one window-width left of the drop point, possibly at a different size.

5. The only cross-monitor test (`dock_drag_tests.rs:239-258`, 1.0 to 1.5) works on pure numbers. It never models the OS resize, and it goes in the direction where the cursor stays inside the window.

6. The design note (`docs/reviews/dock-redesign.md:106`) lists "Per-monitor DPI while a drag crosses monitors" as unverified on Windows. It is an open gap, not a handled case.

I rated it major, not blocker. The drag still works, but on a common laptop (150-200%) plus external monitor (100%) setup, a high-to-low DPI crossing visibly separates the Panel from the cursor and drops it in the wrong place. That feeds the owner's "hard to drag / can't have it where I want it" complaint, but only for that multi-DPI setup. The math is confirmed by reading the code; actual Windows behaviour is unverified here.

## 8. [major] One docked size is shared by all four edges: a tall side dock becomes a near-full-screen top dock (old guard deleted)
`crates/snipdeck-app/src/controller/dock_ctl.rs:595` (spec lens)

**Evidence.** PanelGeometry has one `docked: Option<(u32,u32)>` per monitor, used by dock_to (409), drag_ctx (482) and drag_end (592-600) for every edge. frame::docked_rect (frame.rs:308) keeps h up to work_area.h for Top/Bottom. dock_size/full_height_cut and its tests a_top_or_bottom_dock_never_reuses_a_full_height_frame_from_a_side_dock / an_old_full_height_side_dock_is_compact_when_the_full_edge_is_off were removed with no replacement. The migration also turns old side docks into docked = (w, 1040) (settings test an_old_docked_geometry..., D1).

**Scenario.** A Panel docked left and resized to 1000 px tall (or an old full-height side dock carried over by the migration) is dragged to the top edge. The Preview and the dock are a 1000/1040 px top dock covering the whole work area, and its tab reveal covers the screen. Resizing it shorter there also shrinks the left dock next time.

**Fix.** Keep the docked size per orientation: the side (vertical) docks share one, top/bottom share another. Or cap a top/bottom dock's height (e.g. work_area.h/2, or the floating height) when the stored size came from a side dock. Restore a test for side-to-top.

**Verifier.** I could not refute this finding. At 529fd6b, PanelGeometry keeps one `docked` (w,h) per monitor, and every edge reads it.
- `dock_to` uses it at dock_ctl.rs:409.
- The drag preview gets it through `drag_ctx`/`sizes_on` (dock_ctl.rs:366, 482) and `dock.rs` `docked_at` (dock.rs:347-350).
- `drag_end` writes it back for any edge at dock_ctl.rs:592-600.

For Top/Bottom, `frame::docked_rect` only caps the height at the work area (frame.rs:308, `size.1.max(min_h).min(work_area.h)`). The diff deletes `dock_size`, `dock_size_for` and `full_height_cut`, along with their tests (`a_top_or_bottom_dock_never_reuses_a_full_height_frame_from_a_side_dock`, `an_old_full_height_side_dock_is_compact_when_the_full_edge_is_off`). Those functions existed to stop this case; their doc said a top dock "would cover the whole screen with its bottom out of reach". No replacement was added.

The settings test `an_old_docked_geometry_becomes_a_floating_size_and_a_docked_size` (settings.rs:1492) shows the migration turning an old full-height left dock into `docked = (340, 1040)`.

Scenario: an upgraded user, or one who resized a compact side dock tall, drags or menu-docks to the top edge. The Preview and the final dock are about 340x1040, the full height of a 1040 px work area. Resizing it shorter there also changes the left dock's size.

The design note (docs/reviews/dock-redesign.md:84, 97) says there is one docked size and that an old full-height side dock "stays full height until resized". It never considers moving that size to a horizontal edge, so this is a gap in the design, not a deliberate choice for top docks.

One correction: "covering the whole work area" overstates it. The width is still clamped to PANEL_WIDTH, so the result is a narrow strip the full height of the work area, not the whole screen.

Major: the behaviour is visibly wrong and it is the "unwanted size" problem the owner complained about, but nothing crashes and no data is lost.

## 9. [major] A resize in progress is not counted as 'held': a docked Panel can tuck in the middle of the OS resize loop
`crates/snipdeck-app/src/controller/dock_ctl.rs:135` (spec lens)

**Evidence.** dock_held() = `self.menu != PanelMenu::None || self.panel_focused || self.drag.is_some()`. Nothing tracks the resize started by Effect::ResizeWindow (panel.rs:693-701 → main.rs drag_resize_window). Design note line 62 promises: "Revealed stays while held: ... or a drag or resize is under way". No test covers a resize.

**Scenario.** A Panel docked right at 320 px, revealed by hover and not focused. The user drags its inner (left) edge to widen it, past the 480 px maximum. The window stops at 480 and the cursor goes on more than 8 px past it. After 400 ms dock_tick collapses the Panel and places the 6 px tab while Windows' modal resize loop is still running. The Panel vanishes or jumps mid-resize. Windows only, unverified.

**Fix.** Have resize() set a `resizing` flag that dock_held() checks, cleared on the next frame_changed after the button is up (primary_down) or on focus/pointer-up. Add a controller test where the cursor leaves during a resize and nothing tucks.

**Verifier.** The finding holds at 529fd6b. `dock_held()` (dock_ctl.rs:135-137) checks only `menu`, `panel_focused` and `drag`. Nothing records a resize. `resize()` (panel.rs:693-701) just returns `Effect::ResizeWindow`, and the host (main.rs:633-642) calls winit's `drag_resize_window`, which starts Windows' modal size loop. No grep hit for "resiz" in binding.rs or dock_ctl.rs adds a resizing flag or pauses the dock timer, and no test in tests/dock.rs moves the cursor away during a resize.

Neither existing guard covers this case:
- Focus: the Panel is `WS_EX_NOACTIVATE` and answers `WM_MOUSEACTIVATE` with `MA_NOACTIVATE` (window_style.rs:8, 249-250). Pressing a grip never focuses it, so `panel_focused` stays false.
- Keep zone: `dock_rects` does follow the live frame, because `remember_frame` updates the docked size on each resize report. But the window stops at its own min/max (main.rs:636-637: 260-480 wide, height capped at the work area) while the cursor keeps going. Two ways to get the cursor more than 8 px (`KEEP_ZONE`) outside the Panel for the 400 ms leave delay: drag the inner edge past 480, or drag a bottom grip down into the taskbar.

`dock_tick` then sends `PanelHover(false)`, and Leaving moves on to Collapse, which emits `PlaceWindow(tab)` in the middle of the loop. After that, `remember_frame` ignores frame reports because `dock_collapsed()` is true. The design note (dock-redesign.md, Reveal section) says "Revealed stays while held: ... or a drag or resize is under way", so the code does not match the spec.

One condition I could not settle from the code: the tick has to fire during the modal loop. winit on Windows has a wait-thread that posts to a message window so that WaitUntil timers keep firing in modal loops, so this is likely but not verified. Windows-only and unverified on the device. Major: visible wrong behaviour (the Panel jumps or vanishes mid-resize) with no data loss.

## 10. [major] The Panel menu now opens over its own More (kebab) button, so a second click on the button runs Undo or Redo
`crates/snipdeck-ui/ui/panel.slint:2747` (ux lens)

**Evidence.** New placement: `y: max(Space.s2, min(root.header-h - Space.s1, root.height - self.height - Space.s2)); dense-above: ...; menu-max-h: root.height - 2 * Space.s2;`. The 24px grab strip makes header-h 64, which puts the kebab at y 28..60 (the focus-ring test measures the pin at (216,28,32,32); the kebab spans x 254..278). The menu's right edge sits on the kebab's right edge. The kebab menu has 17 actions and 3 separators, about 511px tall with dense 28px rows. Below a window height of about 590 the menu rises over the header. At the default 480px it lands at y=8: row 0 (Undo) covers y 12..40 and row 1 (Redo) covers y 40..68, both directly over the kebab. The regenerated golden panel-app-menu-light.png shows it: the menu top is at about y 40 and covers the '...' button, with 'Nothing to undo' at y≈59. In the old golden the menu hung at y 37, below a kebab at y 4..36, with no overlap. The updated panel_logic test only asserts the menu stays inside the window, so it accepts the overlap.

**Scenario.** Panel at its default 320x480. The user deletes an item, then clicks '...' to open the menu and clicks '...' again to close it, or double-clicks it. The second click lands on the 'Undo delete…' row (y 28..40) or 'Redo' (y 40..60), and the action runs without the user choosing it. Before this change the same click landed on the scrim and only closed the menu.

**Fix.** Keep the menu anchored below the kebab (y >= kebab bottom) and let it scroll there, or, when it has to rise, keep the kebab's rectangle clear: start the menu below the kebab, or put it to the kebab's left. Add a panel_logic assertion that a click at the kebab's centre while the menu is open dismisses the menu and fires no activation.

**Verifier.** I could not refute this one. At 529fd6b, panel.slint:2747 sets the app menu's y to max(Space.s2, min(header-h - Space.s1, height - self.height - Space.s2)). Before this change y was fixed at header-h - Space.s1. The 24px grab strip (panel.slint:622) makes header-h 64, so the kebab sits at about y 28..60.

The menu has 17 actions and 3 separators. Its full height with 32px rows is 579px (menu.slint:193). That is taller than dense-above, so the rows drop to 28px (Space.hit) and the menu is about 511px tall. At the 560px golden size this puts y at 41. At the default 480px height it puts y at 8, and the menu scrolls inside 464px. In both cases the first rows (Undo, then Redo) sit right on top of the kebab. They also line up with it horizontally, because the menu's x aligns its right edge with the kebab's right edge.

The regenerated golden panel-app-menu-light.png (320x560) confirms it. The menu's top edge is at y of about 40, it hides the '...' button completely, and 'Nothing to undo' is drawn where the button was. The old golden had the menu hang below a fully visible kebab.

The Menu is declared after the scrim TouchArea, so it sits on top and gets the click. MenuItem rows fire activated(i) when they are selectable. Undo is disabled only when history.undo is None (binding.rs:1850). So after any edit, clicking '...' a second time to close the menu, or double-clicking it, runs Undo or Redo instead of just dismissing the menu. Nothing in the tests checks for this overlap.

Severity is major rather than blocker. The action runs unasked and the kebab can't close its own menu, which is visible wrong behaviour. But Undo and Redo can be reversed, nothing is lost for good, and it is not the owner's docking complaint.

## 11. [major] A Panel whose top is at the screen top docks to Top after a brief upward wobble; the 'never docks' tests never go near the top edge
`crates/snipdeck-app/src/dock.rs:388` (tests lens)

**Evidence.** In the Float phase the candidate is kept while `frame::edge_gap(cursor, area, edge) <= px(EDGE_EXIT, scale)`, and EDGE_EXIT is 12 (dock.rs:388-390). The grab strip is the window's top 24 px, with the top 6 px kept for resize, so a Panel at y=0 is grabbed at screen y≈6..24, well inside EDGE_EXIT of the top edge. a_drag_through_the_middle_never_docks_whatever_the_panels_size (dock_drag_tests.rs:55-74) keeps the cursor at y=15 the whole time. a_tall_panel_dragged_through_the_middle_of_the_screen_never_docks (controller/tests/dock.rs:574-606) uses y=12..30. Neither one touches y<=4.

**Scenario.** This is the owner's 'it always seems to want to be docked' case for a tall Panel or one placed at the top of the screen. Drag::begin((500,12), window(400,0,FLOATING), None); moved((520,3), t=0) sets candidate Top. moved((600,12), t=50) keeps it (gap 12 <= 12). moved((900,12), t=250) gives Preview(Top) and the window jumps to the docked frame, although the cursor is back at its normal grab height and the user is dragging sideways. Releasing then docks it. The current tests pass while this happens.

**Fix.** Add a test: touch y<=4 for one tick, return to the grab height and hold, then assert Float. Fix the code by keeping the candidate only while the cursor stays within EDGE_ZONE (or restarting the arm timer whenever the cursor leaves EDGE_ZONE), with EDGE_EXIT used only as hysteresis after Preview is shown. Also make the controller 'tall panel' test wobble up to y=2 and back.

**Verifier.** I tried to refute this against the code at 529fd6b and could not. The finding holds.

**What the code does (crates/snipdeck-app/src/dock.rs:388-408).** In the Float phase, a candidate edge is set once the cursor comes within EDGE_ZONE (4 px) of an edge. After that it is kept while `frame::edge_gap(cursor, area, edge) <= px(EDGE_EXIT)`, which is 12 px. The arm timestamp `since` is never reset. So `armed` fires 200 ms after the first touch, as long as the cursor has stayed within 12 px since then. `edge_gap` for Top is just `y - top` (frame.rs:474). With the default test monitor the taskbar is on the Bottom, so Top is an allowed edge.

**The scenario, traced.**
- `begin((500,12), window(400,0))`
- `moved((520,3), t=0)`: the cursor has moved from the press and the gap is 3, so `edge_at` returns Top and the candidate is (Top, 0).
- `moved((600,12), t=50)`: the gap is 12, so the candidate is kept.
- `moved((900,12), t=250)`: 250 ms is at least 200 ms, so the phase becomes `Preview(Top)` and the result is `Place(docked rect)`.
- Releasing now returns `Docked(Top, ..)` (line 425).

**Why it is a defect.** It contradicts the documented intent in three places:
- The state diagram in docs/reviews/dock-redesign.md:26-27 says the cursor must stay "within EDGE_ZONE ... for DOCK_ARM".
- The DOCK_ARM doc in dock.rs:180-181 says "brushing an edge docks nothing".
- The design table entry is "200 ms in the zone".

The note at dock-redesign.md:50 keeps the candidate within EDGE_EXIT, but only to stop corner oscillation. It does not justify arming after a brush. The `brushing_an_edge_docks_nothing` test (dock_drag_tests.rs:97) only passes because it moves the cursor to x=30, beyond EDGE_EXIT.

**Test coverage.** Neither "never docks" test enters the 4 px zone:
- `a_drag_through_the_middle_never_docks_whatever_the_panels_size` keeps y at 15.
- The controller's `a_tall_panel_dragged_through_the_middle_of_the_screen_never_docks` uses y = 12, 20 and 30.

So this path is not tested.

**Severity.** Major rather than blocker. Triggering it needs a sideways drag near the top edge (or another edge) with one momentary dip of 4 px or less, followed by 200 ms within 12 px. That is a realistic hand wobble for a Panel sitting at the top of the screen, and the result is the unwanted docking the owner complains about. It is still a narrower trigger than the old geometry-based bug, and there is no data loss or crash.

## 12. [major] Pushing a side-docked Panel toward its own screen edge undocks it, then re-docks it 200 ms later (size flicker); only the outward pull is tested
`crates/snipdeck-app/src/dock.rs:374` (tests lens)

**Evidence.** Slide phase: `let across = if edge.vertical() { pulled.0 } else { pulled.1 }; if i64::from(across).abs() > px(UNDOCK_PULL, scale) { return self.float(ctx); }`. Because of abs(), a move toward the screen edge undocks just as a pull away from it does. pulling_a_docked_panel_off_its_edge_floats_it_at_its_floating_size (dock_drag_tests.rs:202-221) and a_drag_that_stays_on_the_edge_slides_the_dock_along_it (controller/tests/dock.rs:609-623, '+10 px wobble') only move outward.

**Scenario.** A Panel is docked Left at 280 px wide and pressed at x=200 on the grab strip. The user slides it down-left toward the bottom-left corner and the hand drifts 25 px left, to x=175. It floats at once at the floating size (300x400, x≈-11), which is the 'size reset'. The cursor keeps going to x<=4, so after DOCK_ARM it is previewed docked again at the docked size. The window switches size twice during one slide, which matches the 'spasming' complaint. If released before 200 ms, it is left floating at the left edge, where it no longer tucks.

**Fix.** Count only the pull away from the edge (positive `across` for Left/Top, negative for Right/Bottom) toward UNDOCK_PULL. Add drag tests that push a Left and a Right dock 100 px toward their own edge and assert Slide is kept and the size does not change.

**Verifier.** I tried to refute this and could not. At 529fd6b, crates/snipdeck-app/src/dock.rs:372-375 measures the pull as `across = pulled.0` (vertical edges) or `pulled.1` (horizontal edges) and undocks when `i64::from(across).abs() > px(UNDOCK_PULL, scale)`, with UNDOCK_PULL = 24 (dock.rs:185). Because of `abs()`, a move of 25 px or more toward the docked edge calls `self.float(ctx)`, just as a pull away from it does. The window then switches at once to the floating size (dock.rs:318-342).

The design says otherwise. docs/reviews/dock-redesign.md:47 and :79 call this a pull "off the edge" / "across the edge". Line 51 says a drag never changes size except when "pulling off the edge". A push toward the edge cannot take the Panel off it.

The tests only cover the outward case:
- dock_drag_tests.rs:202-221 moves outward (140 to 165).
- dock_drag_tests.rs:180-198 has an inward wobble of only 20 px (100 to 80), under the threshold.
No test pushes 25 px or more toward the edge.

Worked scenario: the Panel is docked Left at 280 px and pressed at x=200 on the grab strip. Moving to x=175 floats it at 300x400. The grab is rescaled to 214, so the frame lands at x=-39. If the hand keeps going to x<=4, the cursor is in the EDGE_ZONE. After DOCK_ARM (200 ms) the Panel is previewed at the docked size again. That is two size switches in one slide, which matches the owner's "spasming" and "size reset" complaints. If released before 200 ms, it ends Floating, clamped inside the work area, and no longer tucks.

Right docks fail the same way (pulled.0 positive), and so do Top and Bottom on the y axis. I rate it major rather than blocker because it takes 25 px or more of drift toward the edge. Users often shove a Panel toward its edge or corner while sliding it, so this is visible wrong behaviour. The suggested fix is sound: count only the outward sign of `across`, and add inward-push tests.

## 13. [major] Regression tests for the full-height size bloat were deleted, and the bloat is back: an old full-height dock now opens as a full-screen top or bottom dock
`crates/snipdeck-app/src/controller/dock_ctl.rs:409` (tests lens)

**Evidence.** Removed versus fee4a99d: moving_a_side_dock_to_the_bottom_does_not_bring_the_full_height_along (old tests/dock.rs:652), a_stored_bottom_dock_that_was_saved_full_height_reopens_at_the_panel_height (:689) and an_old_full_height_side_dock_opens_compact_when_the_full_edge_is_off (:883), plus frame.rs a_top_or_bottom_dock_never_reuses_a_full_height_frame_from_a_side_dock. Nothing replaces them. Now: settings.rs:1135-1142 migrates an old side dock to `docked = (w, work.h)`; dock_to uses `size = geometry.docked.unwrap_or(...)` (dock_ctl.rs:409); and docked_rect for Top/Bottom uses `h = size.1.max(min_h).min(work_area.h)` (frame.rs:308). The design note only accepts 'a side dock … stays full height', not a top or bottom one.

**Scenario.** An upgraded settings.json has {edge:left, h:1040, pre_dock:[320,480]}. sanitize turns it into docked=(320,1040). The user picks 'Dock to bottom' in the menu, gets a 1040-tall Panel covering the whole work area, and it comes back that way after every restart. Second case: an old {edge:bottom, h:1040} with no pre_dock gives docked=None, so placement() opens a full-screen bottom dock, and undocking gives a full-height floating Panel. That is the owner's 'window sizing … unusable' complaint.

**Fix.** Restore the two controller tests, updated to the docked/floating model, so they assert that a top or bottom dock never takes a height from a side dock and that an old bottom dock stored at full height opens at the floating or default height. Either keep a per-orientation docked size or cut a migrated side-dock height when the edge changes orientation.

**Verifier.** The finding holds at 529fd6b. Its line references are slightly off, but the failure is real. I did not run cargo; this is from reading the code and tests.

1. **The migration keeps the full height.** `crates/snipdeck-app/src/settings.rs:1135-1142` sets `docked = (w, h)` when an old entry has `pre_dock`. The test at settings.rs:1494-1498 confirms the result: an old `{edge:left, w:340, h:1040, pre_dock:[320,480]}` becomes `docked = Some((340, 1040))`.

2. **Docking to top or bottom reuses that height.** `dock_to` at `crates/snipdeck-app/src/controller/dock_ctl.rs:409` uses `size = geometry.docked.unwrap_or(...)` with no check on orientation. For top and bottom, `docked_rect` in `crates/snipdeck-app/src/frame.rs:309` computes `h = size.1.max(min_h).min(work_area.h)`, so "Dock to bottom" gives a Panel as tall as the work area. Line 427 then stores `docked = (own.w, own.h)`, which also saves the full height. So it comes back after every restart, through `placement()` (panel.rs:795-805, which uses `g.docked.unwrap_or((g.w, g.h))`).

3. **The second case also fails.** An old `{edge:bottom, h:1040}` with no `pre_dock` gets `docked = None`. `placement()` then falls back to `(g.w, g.h) = (w, 1040)`, which opens a full-screen bottom dock. Undocking clamps that same full-height frame, so the floating Panel is full height too.

4. **The old protection was dropped with its tests.** At fee4a99d, `tests/dock.rs` had `moving_a_side_dock_to_the_bottom_does_not_bring_the_full_height_along` (it expected the floating height). It also had `a_stored_bottom_dock_that_was_saved_full_height_reopens_at_the_panel_height` (it expected 480, "not the whole screen"). Their setup comments say older versions did write these states. Nothing in the new tests replaces them:
   - The full-height tests that remain cover the full-edge setting on a side dock only.
   - `predictable.rs:119` checks the floating size on a monitor change, not docking to top or bottom.

5. **The design note does not cover this.** `docs/reviews/dock-redesign.md:97` accepts only that "a side dock an old version stored full height stays full height until resized". It says nothing about that height moving to a top or bottom dock, or about an old full-height bottom dock.

New code no longer creates the bloat: it saves its own height even when the full edge is on (dock_ctl.rs:426-427, panel.rs:774-779). So only upgraded settings files are affected. The result, a top or bottom dock covering the screen that persists, matches the owner's sizing complaint. That is visible wrong behaviour, so major rather than blocker: no data is lost, and resizing fixes it.

## 14. [minor] Preview on a top/bottom edge keeps the floating grab offset, so the docked Panel can land away from the cursor and tuck right after the drop
`crates/snipdeck-app/src/dock.rs:345` (state lens)

**Evidence.** docked_at uses `sub(self.cursor, self.grab)` as `along` (346-352). grab was measured on the floating window (begin, 282) and is rescaled only in float() (319-333), never on entering Preview (404-408). For a top/bottom edge, along is x. With a docked width smaller than grab.x, the window ends left of the cursor.

**Scenario.** The floating Panel is 480 wide, grabbed 400 px from its left. The docked size is 300 wide (resized while docked earlier). The user holds the cursor at the top edge and Preview places the window over [cx-400, cx-100], so the 'Release to dock' frame is not under the pointer. After the release, dock_tick finds the cursor 100 px from the open Panel, which is more than KEEP_ZONE. The Panel goes Leaving and tucks 400 ms after docking.

**Fix.** On entering Preview, rescale grab along the edge to the docked length (as float() does for width), or clamp it to the docked length minus 1, so the cursor stays over the previewed frame.

**Verifier.** I could not refute this finding; it holds at 529fd6b. In dock.rs, Drag::begin (282) measures `grab` on the window that was pressed. Only float() (319-333) rescales `grab` to a new width. Entering Preview (404-408) calls docked_at, which passes `cursor - grab` as `along` (345-352). For a top or bottom edge, frame::docked_rect (frame.rs 308-315) uses that value for x and clamps it into the work area with `inside()`. It never clamps x so the cursor stays over the frame. The floating and docked sizes are stored separately (settings.rs:66 `docked: Option<(u32,u32)>`, both widths in PANEL_WIDTH 260..=480), so a 480-wide floating Panel and a 300-wide docked size can both exist. With grab.x = 400, the Preview and the frame after release span [cx-400, cx-100], so the cursor is 100 px to the right of the window. On release, dock_ctl.rs:589 keeps that frame and sets DockState::Expanded. dock_tick (149-168) then sends PanelHover(near(cursor, open, KEEP_ZONE = 8)), which is false. dock_held() is false too: the drag is over, there is no menu, and the Panel never takes focus because the window is WS_EX_NOACTIVATE / MA_NOACTIVATE. The state goes Leaving and collapses after leave_delay_ms (default 400). The existing tests (dock_drag_tests.rs) only cover top/bottom docking for which edge is chosen, not this offset. Left and right edges are rarely affected: along is y there and grab.y is a header offset, smaller than the docked height. I rate it minor because it needs a top or bottom dock, a floating Panel wider than the docked size, and a grab past the docked width. When it happens, though, the Panel docks and then tucks away on its own, which fits the owner's complaint that it "doesn't show when I want".

## 15. [minor] Dragging a floating Panel to another monitor shows a misleading 'Panel floats again' toast
`crates/snipdeck-app/src/controller/dock_ctl.rs:631` (state lens)

**Evidence.** `if now_docked != was_docked || left.is_some() { self.show_toast(neutral(ToastMessage::DockSet(now_docked))) }`. DockSet(None) reads 'Panel floats again' (toast_texts.slint:445). left is Some for any drop on a monitor other than self.monitor, even when the Panel was never docked.

**Scenario.** A floating Panel is dragged from monitor 1 to monitor 2 and dropped. was_docked is None and now_docked is None, but left is Some, so a 'Panel floats again' toast appears on every cross-monitor move of a Panel that was never docked.

**Fix.** Show the toast only when the dock state changed: now_docked != was_docked, or left's entry actually had an edge.

**Verifier.** The finding holds at 529fd6b. In crates/snipdeck-app/src/controller/dock_ctl.rs, `drag_end` sets `left = self.monitor.clone().filter(|m| m.id != monitor.id)` at line 567. That is Some whenever the drop lands on a monitor other than the one the drag started on, whatever the Panel's dock state. `was_docked = self.dock_edge()` at line 568 reads the stored edge for the start monitor, which is None for a floating Panel. A DragEnd::Floating drop sets `geometry.edge = None`, so `now_docked` is None. Line 631 is `if now_docked != was_docked || left.is_some()`, and this diff added the `left.is_some()` part. So a floating-to-floating move across monitors calls `show_toast(DockSet(None))`. Through wording.rs:346, DockSet(None) maps to 0, and crates/snipdeck-ui/ui/toast_texts.slint:445 renders 0 as "Panel floats again". The Panel was never docked, so the toast is wrong. No test guards against this. tests/dock.rs:906 (`crossing_to_another_monitor_docks_nothing_and_the_panel_floats_there`) runs this exact path but checks only placement and dock_edge, not toasts. It is only a wrong informational toast: no placement or data effect, and no part of the owner's docking complaint. So minor. Fix: toast only when the dock state really changed, meaning `now_docked != was_docked`, or the start monitor's entry had an edge that the `update_settings` block cleared.

## 16. [minor] A resize is not counted as 'held': a docked Panel can tuck to its 6 px tab in the middle of the OS resize loop
`crates/snipdeck-app/src/controller/dock_ctl.rs:135` (win32 lens)

**Evidence.** dock_held() is `self.menu != PanelMenu::None || self.panel_focused || self.drag.is_some()`. Nothing records an OS resize in progress: Effect::ResizeWindow (main.rs:633-643) calls winit drag_resize_window, and no controller state is set. The design note (dock-redesign.md line 62) says Revealed stays while '...or a drag or resize is under way'. dock_tick (dock_ctl.rs:149-169) keeps running every 50 ms while dock_live().

**Scenario.** A Panel is docked left. The user drags its right grip outward past the 480 px max-width. The window stops at 480 but the cursor keeps going, more than KEEP_ZONE (8 logical px) outside the Panel. After the 400 ms leave delay, dock_tick collapses it, and dock_place(true) issues PlaceWindow to the 6 px handle rect while Windows' modal sizing loop still owns the window. The Panel vanishes or flickers mid-resize. This assumes Slint timers fire inside the modal loop, which winit's wake messages normally allow; unverified on Windows.

**Fix.** Set a `resizing` flag when Controller::resize returns Effect::ResizeWindow and clear it on the next frame_changed or the button-up (or on WM_EXITSIZEMOVE if the host reports it). Include it in dock_held().

**Verifier.** I could not refute this finding. I read the code at 529fd6b and did not build or run anything.

1. **Nothing holds the Panel during a resize.** `dock_held()` (dock_ctl.rs:134-136) checks only for an open menu, `panel_focused` and an active drag. `Controller::resize` (panel.rs:693-701) returns `Effect::ResizeWindow` and sets no state. The handler in main.rs:633-643 only calls winit's `drag_resize_window`.

2. **Keyboard focus does not save it.** The Panel is created un-activated and does not take the focus on a click (ADR-0003; native_window.rs:4, main.rs:869, binding.rs:1131). Pressing a resize grip therefore leaves `panel_focused` false, unless the user had already given the Panel the keyboard.

3. **The design says it should hold.** dock-redesign.md:62 states that Revealed stays while "a drag or resize is under way". The code does not do this, and no test in controller/tests/dock.rs covers holding during a resize.

4. **The failure path is reachable.** The Panel's window keeps a maximum width of 480 during the OS resize (main.rs:636 comment). Drag the right grip of a left-docked Panel past 480 and the cursor ends up more than `KEEP_ZONE` (8 px) outside the window. `dock_tick` then sends `PanelHover(false)`, and after `leave_delay_ms` (400 ms, dock.rs:121-135) the Panel collapses: `dock_tuck` places the window as the 6 px tab. A second path: drag the bottom grip down over the taskbar. `remember_frame` pulls the window back into the work area (panel.rs:771-797), which leaves the cursor outside the zone.

5. **What follows is wrong.** Once collapsed, `remember_frame` returns early (panel.rs:726). Windows' sizing loop sets the window rect from the cursor on each mouse move, so the window flickers between the tab and full size. When the button is released, the window can stay full size while the controller believes it is collapsed.

6. **Timers probably fire in the sizing loop.** This rests on the Slint dock timer firing inside the modal sizing loop. As far as I recall, winit's Windows backend services WaitUntil there through its thread-target paint handling, but that is from memory and unverified on Windows.

7. **Why minor.** The trigger is an overshoot past a size limit, held for more than 400 ms. The user can recover through the tab. No data is lost.

## 17. [minor] Panel opened at the mouse on a monitor with a stored edge floats there but is treated as docked, then jumps to that edge's tab
`crates/snipdeck-app/src/controller/panel.rs:829` (spec lens)

**Evidence.** open_anchor_now (dock_ctl.rs:265) checks dock_edge() only for the current monitor. restore_anchor (panel.rs:828-830) places the floating frame_at() rect and then sets `self.monitor = Some(on)` for the monitor under the anchor. From then on dock_edge(), dock_live() and dock_rects() read that monitor's stored edge. remember_frame's docked branch (panel.rs ~752-786) also turns a Resized report of the floating frame into PlaceWindow(docked_rect). With the new default trigger Both and no hide-on-blur for docked Panels, this now ends in a tuck.

**Scenario.** Panel opens = At the mouse. The Panel was docked once on monitor B and is floating on monitor A. Showing it with the mouse on B places it floating at the mouse, while the view says docked (dock grips, no hide on blur). On focus loss the leave delay tucks it, and PlaceWindow moves it to B's edge tab. If B has another scale, the Resized report snaps it to B's edge at once. The Panel docks without being asked.

**Fix.** When an anchor places the Panel floating on a monitor, make that monitor's placement floating: skip the anchor when that monitor has an edge, or clear the edge there. Test: anchor on a second monitor with a stored edge, then expect dock_edge() == None or the docked placement.

**Verifier.** The defect is in the code, but the finding's main route does not happen, so I rate it minor rather than major.

What holds (code at 529fd6b):
- open_anchor_now (dock_ctl.rs:265) checks dock_edge() only for the Panel's current monitor (self.monitor).
- restore_anchor (panel.rs:816-831) places the floating frame_at() rect on the monitor under the anchor, then sets self.monitor = Some(on) at line 829. It does not check whether that monitor has a stored edge.
- From then on, dock_edge(), dock_active(), dock_live() and dock_rects() (dock_ctl.rs:46/60/112/117) all read that monitor's stored edge.
- dock_tick (dock_ctl.rs:149-168) compares the cursor with the docked open rect at the edge, not with the real floating frame. Once dock_held() is false (focus lost, no menu), PanelHover(false) starts the leave delay, and Collapse runs dock_place(true). That gives PlaceWindow(the tab at B's edge).
- remember_frame's docked branch (panel.rs:752-786) turns a Resized report into PlaceWindow(docked_rect). hides_on_blur (controller.rs:997) is off while dock_active().
- The redesign changed the default trigger from Off to Both (settings diff), and sanitize moves old files that had Off to Both. Under the old default this state stayed inert. Now it ends in a tuck. The behaviour is new to this diff even though restore_anchor itself is unchanged.
- No test covers an anchor landing on a monitor with a stored edge. access.rs:262 docks only after the anchored open.

What I refuted: the finding's scenario, "docked once on monitor B, then floating on A", built with the app's own gestures. A drag that ends on another monitor clears the edge of the monitor it left (dock_ctl.rs:567 sets `left`; lines 614-626 write edge: None for that monitor). Undocking from the kebab or Settings clears the edge too. So a normal app drag or undock cannot leave B with an edge while the Panel floats on A.

How it still happens: the window reaches A without the app's drag. Examples are unplugging B while docked (Windows moves the window, frame_moved(A) rewrites only A's entry), Win+Shift+Arrow, or a display-settings change. B's entry keeps its edge. Later, with "Panel opens = At the mouse" and the mouse on B, the Panel opens floating at the mouse with docked behaviour, then tucks to B's edge tab on focus loss. B's edge was a real choice the user made earlier, so the Panel ends where they once docked it, not somewhere arbitrary. It is still a visible inconsistency: it opens floating, then jumps without being asked. It needs an OS-driven move first, which is an edge case, so minor.

## 18. [minor] Settings and onboarding text no longer match dock behaviour
`crates/snipdeck-ui/ui/settings.slint:955` (spec lens)

**Evidence.** settings.slint:955 says "Off keeps a docked panel open", but with Off, dock_active() is false, so hides_on_blur (controller.rs ~993) and close() still hide the docked Panel, with no tab (panel_hide_on_blur defaults to true). settings.slint:892-893 says "the panel hides as soon as you switch to another app", but a tucking docked Panel now never hides on blur. settings.slint:879 says a docked Panel "folds back" after "a paste or copy", but dock_after_use (dock_ctl.rs:190-199) folds only on paste. onboarding.slint:386, "Fixed against the right edge of the screen, full height.": dock_to keeps the floating height (full_edge is false by default) and the Panel tucks by default. The design note says "'Stay open' remains selectable" but the option is labelled "Off". The grab-strip tooltip (panel.slint:1511) says "Hold at a screen edge to dock" on a docked Panel too, where a drag slides it.

**Scenario.** A new user picks 'Docked to the right' in onboarding, expecting a full-height panel that stays. They get a 480 px Panel that disappears into a 6 px strip 400 ms later. A user who picks Off to keep a docked Panel open sees it vanish completely when they click another app.

**Fix.** Rewrite the four descriptions and the onboarding hint to match the code (or change the code: an Off dock should not hide on blur if the text stays). Rename Off to "Stay open" as the design note says. Give the docked strip its own tooltip ("Drag along the edge, pull away to undock").

**Verifier.** I checked each claim against the code at 529fd6b and could not refute any of them. The UI text is wrong in several places, but behaviour is correct, so this stays minor.

(1) settings.slint:955 says "Off keeps a docked panel open". With the trigger Off, dock_config() sets the trigger to Off, so dock_active() is false (dock_ctl.rs:51-63). hides_on_blur() (controller.rs:990-1006) therefore still applies, and panel_hide_on_blur defaults to true (settings.rs:828). The result: a docked Panel set to Off hides completely when another app takes the foreground. There is no tab, and close() (controller.rs:906) also hides it to the tray because dock_live() is false.

(2) settings.slint:892 says "the panel hides as soon as you switch to another app". This diff added `&& !self.dock_active()` to hides_on_blur (compared with fee4a99d), so a docked Panel that tucks no longer hides on blur. The text was not updated.

(3) settings.slint:879 says a docked Panel "folds back" after "a paste or copy". dock_after_use (dock_ctl.rs:190-199) acts only on Pasted or PastedNotRestored. Also, after_use returns early when dock_active(), so a copy neither folds nor hides it.

(4) onboarding.slint:386 says "Fixed against the right edge of the screen, full height." full_edge defaults to false (settings.rs:220). This diff changed the default trigger from Off to Both (fee4a99d settings.rs:206 compared with settings.rs:213), so the Panel now tucks away by default. Both parts of the hint are wrong.

(5) panel.slint:1511: the diff removed the docked-only tooltip "Slide the panel along the edge" and now shows "Drag to move. Hold at a screen edge to dock." in every state. On a docked Panel a drag slides it along the edge (design note, line 79). This is a regression introduced by the diff.

(6) The design note says "'Stay open' remains selectable", but the option is still labelled "Off".

Some of the text was already wrong before this diff, notably (1) and the full-height claim in (4). The diff made it worse by turning tucking on by default, adding the blur exemption and dropping the docked tooltip. None of this causes data loss or wrong placement, only misleading text, so I rate it minor. It does feed the owner's complaint that the dock is "hard to find where to drag" and confusing.

## 19. [minor] Docking from the kebab or Settings tucks the Panel within 400 ms, before the user can see it or the toast
`crates/snipdeck-app/src/controller/dock_ctl.rs:437` (spec lens)

**Evidence.** dock_to sets DockState::Expanded and shows the DockSet toast, but the pointer is on the kebab menu or the Settings window, not on the moved Panel. The next dock_tick sends PanelHover(false), and after leave_delay the Panel collapses. tests/dock.rs:320-326 codifies this ("the leave delay runs from the first look"). The toast is drawn inside the Panel window, which is then a 6 px tab.

**Scenario.** The user clicks 'Dock to the right edge' in the kebab. The Panel jumps right and folds to a sliver almost at once, and 'Panel docked to the right edge' is never readable. To the user it looks like the Panel disappeared.

**Fix.** After a menu or Settings dock, hold the Panel open for one leave cycle that starts when the pointer first enters it (or for a few seconds), so the user sees where it went.

**Verifier.** Confirmed at 529fd6b. `dock_to` (dock_ctl.rs:395-442) sets `self.dock = DockState::Expanded` and shows the DockSet toast, but it gives the Panel no grace period. The kebab path (panel.rs:240 and 263-265) closes the menu (`self.menu = PanelMenu::None`) before it calls `dock_toggle`, so `dock_held()` (dock_ctl.rs:134-136: menu, panel_focused or drag) is false. `panel_focused` stays false because the Panel window is WS_EX_NOACTIVATE (win_logic.rs:252-267). Settings (settings_binding.rs:1900) and onboarding (onboarding_binding.rs:391) call `dock_to` while the pointer is on another window. The next `dock_tick` (dock_ctl.rs:149-171) checks the cursor against the new docked rect and sends PanelHover(false). In dock.rs:120-135 that moves Expanded to Leaving{since: now}, and after `leave_delay_ms` (default 400, settings.rs:219) a Tick collapses the Panel to the handle. The default trigger is Both (settings.rs:217), and old files are migrated to Both, so this happens on default settings. The test fixture `docked()` calls `dock_to` (tests/dock.rs:35), and tests/dock.rs:320-326 shows the same sequence: with the cursor away, it collapses at 400 ms. There is one limit. When the kebab docks a Panel to a nearby edge and the cursor still falls inside the moved Panel (or within KEEP_ZONE), it stays open. It always folds when docking from Settings or onboarding, and when the Panel jumps far. The DockSet toast and the Panel's new position are then gone within about 0.4 s, leaving only a 6 px tab. That fits the owner's "it doesn't show when I want it". I rate it minor, not major: the tab stays visible and hovering it brings the Panel back, so nothing is lost. It is a discoverability problem.

## 20. [minor] New panel strings are untranslated in all six locales
`crates/snipdeck-ui/lang/de/LC_MESSAGES/snipdeck-ui.po:861` (spec lens)

**Evidence.** "Drag to move. Hold at a screen edge to dock.", "Move the panel" and "Release to dock" have `msgstr ""` in de, es, fr, ja, pt-BR and zh-Hans. The removed "Slide the panel along the edge" had translations.

**Scenario.** A German or Japanese user sees the new tooltip and the 'Release to dock' preview label in English, inside an otherwise translated Panel.

**Fix.** Add translations for the three strings in every .po (scripts/i18n.py).

**Verifier.** I confirmed this at 529fd6b. Three new strings have an empty `msgstr ""` in all six locales: "Drag to move. Hold at a screen edge to dock.", "Move the panel" and "Release to dock". In de, es, fr and pt-BR they sit at lines 861, 1536 and 2161; in ja and zh-Hans at 860, 1532 and 2155. The string they replace, "Slide the panel along the edge", was translated at fee4a99d (de: "Panel am Rand entlang verschieben"). In the de .po these three are the only empty msgstr apart from the header (4 in total), so the German translation was complete before this diff and is not now. scripts/i18n.py:17 says untranslated strings fall back to English at run time, so nothing crashes and no data is lost. But docs/HANDOVER.md:413 says `i18n.py check` must report 0, and this diff breaks that. The result is English text inside an otherwise translated Panel. This is a minor regression: it does not touch the owner's docking complaint.

## 21. [minor] Dock docs disagree on the grab-strip and header sizes
`docs/SPEC.md:54` (spec lens)

**Evidence.** SPEC 5.1: "The top 20 px of the Panel is a full-width grab strip". DESIGN.md and panel.slint (`grab-h: 24px`): 24 px. The design note: "The top 6 px stay the resize edge; 14 px ... are drag" (24 - 6 = 18). DESIGN.md still says "Header: 40px" although header-h is now grab-h plus the old header.

**Scenario.** The ux-auditor or the owner measures the Panel against SPEC or the design note and finds a mismatch. The next session 'fixes' the code to the wrong number.

**Fix.** Use one set of numbers everywhere: a 24 px strip, of which 18 px drag below the 6 px resize grip, and a 64 px header including the strip.

**Verifier.** This is a real but doc-only mismatch, and this diff introduced it. The code has `grab-h: 24px` (crates/snipdeck-ui/ui/panel.slint:622) and `grip: 6px` (panel.slint:683). So the top 6 px resize and the 18 px below them drag. DESIGN.md:288 (added in this diff) says 24px, which is correct. Three places disagree with the code. docs/SPEC.md:54 (also added in this diff) says "The top 20 px of the Panel is a full-width grab strip". docs/reviews/dock-redesign.md:77 says the strip is 24 px but also "6 px stay the resize edge; 14 px ... are drag", which adds up to 20 px, not 24. panel.slint:1503 has the stale comment "Grab strip (20)". The "Header: 40px" point (DESIGN.md:290, comment at panel.slint:1514) is weak. DESIGN.md lists the grab strip on its own line, so 40px can fairly be read as the controls row under the strip, sized by `header-h - grab-h` (panel.slint:1516). It is not clearly wrong. The suggested "64 px header including the strip" is not needed. Runtime behaviour does not change. The risk is that a later session or the ux-auditor trusts SPEC's 20 px or the design note's 14 px and "fixes" the code to match. Minor.

## 22. [minor] The grab strip's tooltip still opens, and stays open, while the Panel is being dragged
`crates/snipdeck-ui/ui/panel.slint:1511` (ux lens)

**Evidence.** `tip(on) => { root.tip-set(..., on && !root.dragging); }` runs only when the strip's has-hover changes. A press after the hover started leaves tip-timer running, and an already open tip stays open. `tooltip-shown` (line 553) does not include `dragging`. The window follows the cursor while dragging, so has-hover never turns false.

**Scenario.** The user points at the pill and starts dragging within the 600ms show delay, or after the tooltip has appeared. 'Drag to move. Hold at a screen edge to dock.' then pops up, or stays, under the strip, centred over the header's search and New buttons, and travels with the window for the whole drag, including under the 'Release to dock' preview. The code's `!root.dragging` guard shows the tooltip was meant to be off during a drag.

**Fix.** Add `!root.dragging` to `tooltip-shown`, or call `tip-set(..., false)` from `changed dragging` and in `began`.

**Verifier.** I tried to refute this from the code at 529fd6b and could not. Only the delay is misstated.

1. The strip calls `tip(on)` only from `changed has-hover` (crates/snipdeck-ui/ui/panel.slint:183). The `!root.dragging` guard at panel.slint:1511 is checked only when hover changes.
2. In `tip-set` (panel.slint:694-713), `on=true` starts `tip-timer`. Only a later call with `on=false` and the same text stops the timer or closes the tip. Nothing in panel.slint has a `changed dragging` handler. Neither `drag-began` nor the strip's `began` handler at panel.slint:1508 calls `tip-set(..., false)`.
3. `tooltip-shown` (panel.slint:553) does not check `dragging`.
4. The drag is moved by the app, not by the OS. `on_drag_began` (crates/snipdeck-app/src/binding.rs:1097) calls `ctl.drag_begin` and `arm_drag`, and the window then follows the pointer. The TouchArea keeps the pointer captured at the same local offset, so `has-hover` stays true and the guarded `false` call never comes.

Result: if the user hovers the pill and presses within the delay, the tooltip opens partway through the drag. If it is already open when they press, it stays open. Because `tip-bottom` is in window coordinates, it travels with the window. No test in crates/snipdeck-ui/tests covers the grab tooltip.

Correction: the delay is 500ms, not 600ms (Tooltip `show-delay` at crates/snipdeck-ui/ui/components/tooltip.slint:11).

Severity stays minor. It is a visible glitch, a tooltip over the header during a drag, but it does not block dragging or docking and is not what the owner complained about.

## 23. [minor] A docked Panel's grab strip tooltip gives floating-only advice and nothing about undocking
`crates/snipdeck-ui/ui/panel.slint:1511` (ux lens)

**Evidence.** The tooltip text is fixed: @tr("Drag to move. Hold at a screen edge to dock."), with no branch on `root.dock-edge`. Docked, the strip's actual behaviour is a slide along the edge plus an undock after a 24px pull across it (dock-redesign.md lines 36-39). The removed pill used to say 'Slide the panel along the edge'.

**Scenario.** The owner says the Panel 'always seems to want to be docked'. Hovering the strip on a docked Panel tells them to hold it at a screen edge to dock, which it already is. Nothing explains that dragging slides it along the edge, or that pulling it away from the edge undocks it.

**Fix.** Pick the text from dock-edge: when docked, something like 'Drag along the edge to move. Pull away from the edge to float.'

**Verifier.** Confirmed at 529fd6b. In crates/snipdeck-ui/ui/panel.slint:1511 the grab strip's tooltip is a fixed string, @tr("Drag to move. Hold at a screen edge to dock."). Nothing in it depends on root.dock-edge (declared at line 339). The GrabStrip sits unconditionally in the VerticalLayout (lines 1504-1512), so a docked Panel shows the same advice as a floating one. On a docked Panel the strip actually slides the Panel along the edge, and a pull of UNDOCK_PULL (24 px) across the edge undocks it (docs/reviews/dock-redesign.md, the state diagram and constants table). The panel.slint header comment at line 148 says the same. The diff removes the old docked-only pill and its "Slide the panel along the edge" text from the code, the .po files and the tests, and nothing replaces it for the docked case. I found no undock or "Float" control on the Panel itself. The only other way to float is the "Dock to: Floating" option in Settings (settings.slint:928-933). So the tooltip is the one in-place hint, and on a docked Panel it gives docking advice while saying nothing about pulling away to float. That bears on the owner's "always seems to want to be docked". It is only a hint text problem, though: the gesture itself works and there is a way out through Settings. So this is minor, not major.

## 24. [minor] The grab strip is exposed as a button with no action and cannot be focused
`crates/snipdeck-ui/ui/panel.slint:160` (ux lens)

**Evidence.** GrabStrip sets `accessible-role: button; accessible-label: "Move the panel"` but has no `accessible-action-default` and no FocusScope. The keyboard and assistive-technology way to dock or undock is the kebab row ('Dock to the nearest edge' / 'Undock the panel').

**Scenario.** A screen-reader user finds a 'Move the panel' button. Invoking it does nothing, and Tab never reaches it.

**Fix.** Use a non-interactive role (or none) with a description that points to the menu's Dock/Undock row. Alternatively, give it an accessible-action-default that opens the Panel menu or toggles the dock.

**Verifier.** I tried to refute this and could not. At 529fd6b, `GrabStrip` (crates/snipdeck-ui/ui/panel.slint:150-185) sets `accessible-role: button` and `accessible-label: root.a11y-label`. The label is "Move the panel", set at panel.slint:1506. The component has no `accessible-action-default` and no FocusScope. Its only interaction is a TouchArea `pointer-event` handler for left-button press and release. Every other accessible button in the UI defines `accessible-action-default`: panel.slint:1578, 1775 and 2619; editors.slint:578; main_window.slint:126; symbols.slint:226. This one is the exception. So a screen-reader user finds a "Move the panel" button, invoking it does nothing, and Tab cannot reach it because a Rectangle with a TouchArea takes no keyboard focus. The redesign added this component; the old strip was also a button, labelled "Slide the panel along the edge". The test at crates/snipdeck-ui/tests/panel_logic.rs:2422 only checks that the role is Button and never invokes the action, so the tests do not cover the gap. Severity stays minor: the kebab menu still offers "Dock to the nearest edge" / "Undock the panel" (menu_texts.slint:30/45), so assistive-technology and keyboard users can still dock and undock. Free positioning has no keyboard equivalent, but that is not what the owner complained about. This does not affect the owner's mouse-drag complaint.

## 25. [minor] New dock strings are untranslated in all six locales, while the string they replace was translated
`crates/snipdeck-ui/lang/de/LC_MESSAGES/snipdeck-ui.po:861` (ux lens)

**Evidence.** 'Drag to move. Hold at a screen edge to dock.' (861), 'Move the panel' (1536) and 'Release to dock' (2161) all have `msgstr ""` in de, es, fr, ja, pt-BR and zh-Hans; empty msgstr entries in de went from 1 to 4. The removed 'Slide the panel along the edge' had a German translation ('Panel am Rand entlang verschieben'). `scripts/i18n.py check` only warns about untranslated entries.

**Scenario.** A German or Japanese user sees the dock preview label 'Release to dock', the strip tooltip and the accessible name in English.

**Fix.** Add translations for the three msgids in each locale (`python3 scripts/i18n.py todo <locale>`).

**Verifier.** I checked this at 529fd6b and the finding holds. In all six locales (de, es, fr, ja, pt-BR, zh-Hans), these three msgids have `msgstr ""`:
- "Drag to move. Hold at a screen edge to dock." (de .po line 861)
- "Move the panel" (line 1536)
- "Release to dock" (line 2161)

In ja and zh-Hans the line numbers are a few lower.

The UI uses all three strings:
- panel.slint:1506 uses "Move the panel" as the strip's a11y-label.
- panel.slint:1511 uses the drag hint as the strip's tooltip.
- panel.slint:3035 uses "Release to dock" as the dock preview label.

At fee4a99d the de file had one empty msgstr, which is the header. Now it has four, so every real string was translated before this change. The removed "Slide the panel along the edge" was translated in de as "Panel am Rand entlang verschieben".

scripts/i18n.py:17 and :295-296 only warn about untranslated entries and fall back to English at run time. So nothing crashes and the check does not fail. But German, Japanese, Spanish, French, Portuguese and Chinese users will see English text on the main new dock affordances, which they did not before.

This is a localisation gap. It does not cause wrong dock behaviour, so it stays minor.

## 26. [minor] No golden or pixel test covers the new dock preview overlay or the strip's hover and dragging states
`crates/snipdeck-ui/tests/panel.rs:596` (ux lens)

**Evidence.** The only new pixel check is the resting pill contrast (the_grab_pill_reads_at_3_to_1...). None of the tests under crates/snipdeck-ui/tests sets dock_preview or dragging. The accent frame plus 'Release to dock' badge (panel.slint:3018-3041) and the 48px accent pill while dragging are never rendered in a test.

**Scenario.** The 'Release to dock' badge could clip at 260px wide, at 225% scale or in a longer locale, or lose contrast in high-contrast mode, and no test would catch it. This preview is the main cue that answers the owner's 'it always wants to be docked' complaint.

**Fix.** Add goldens for dock_preview=1 and 3 in light and dark at 260px and at scale, and one with dragging=true, plus a contrast check of the label on accent.

**Verifier.** The coverage gap is real. At 529fd6b nothing under crates/snipdeck-ui/tests calls set_dock_preview or set_dragging, and no golden in the diff covers either state. The setters are called only from crates/snipdeck-app/src/binding.rs:1574-1577. The app-side tests (controller/tests/dock.rs:594-853) check only the view-model value (dock_preview == Some(Right) or None), never the rendered pixels. So three things the builder added are never drawn in any test: the overlay at panel.slint:3018-3041 (2px accent frame plus the centred 'Release to dock' badge), the GrabStrip `active: root.dragging` state at panel.slint:1507, and the move cursor at panel.slint:1520. The only new pixel test, panel.rs:596, checks the resting pill (dragging=false). That breaks the project rule that a UI claim needs a measurement, and this overlay is the cue meant to answer the owner's complaint that the panel always wants to dock.

Severity stays minor because the failure scenario is speculative, not a demonstrated defect. The badge is sized from the label's preferred width plus Space.s6 and centred in the window. At sm font, 'Release to dock' is far narrower than the 260px minimum, and scale factors apply to logical pixels, so clipping at 225% is unlikely. text-on-accent on accent is the same token pair the primary buttons use. What is missing is a regression guard and visual proof for an owner-facing cue, not a visible wrong behaviour.

## 27. [minor] Grab strip height is stated three different ways
`docs/SPEC.md:54` (ux lens)

**Evidence.** SPEC.md:54 says 'The top 20 px of the Panel is a full-width grab strip'. panel.slint:1503 comments 'Grab strip (20)'. dock-redesign.md:77 says '14 px by the full width are drag'. The code has `grab-h: 24px` with a 6px top grip and 8px corner grips, so 18px drag across the middle. DESIGN.md:288 says 24px, which matches the code.

**Scenario.** The next visual pass or a reviewer works from the SPEC's 20px or the note's 14px and changes the strip or the tests to match a wrong number.

**Fix.** Use 24px (18px drag below the 6px resize edge) in SPEC.md, the slint comment and the design note.

**Verifier.** This holds up at 529fd6b, but it is a documentation problem only. The code has `grab-h: 24px` (crates/snipdeck-ui/ui/panel.slint:622) and a top ResizeGrip of height `grip: 6px` (panel.slint:683, 2959-2965), which leaves 18 px of drag across the middle; the 8 px corners are resize. The tests agree with 24: tests/panel_logic.rs:2040 asserts "grab strip 24 and header 40" and tests/panel.rs:602 says "the top 24 px", and DESIGN.md:288 also says 24px. Three places disagree. docs/SPEC.md:54 says "The top 20 px of the Panel is a full-width grab strip", and the comment at panel.slint:1503 says "Grab strip (20)". docs/reviews/dock-redesign.md:77 is wrong against itself: it says "the top 24 logical px" and then "the top 6 px stay the resize edge; 14 px by the full width are drag", but 24 - 6 is 18, not 14. The finding slightly misstates the note: the note does say 24, and only its drag-height figure is wrong. Nothing behaves wrongly at runtime, and it has no bearing on the owner's docking complaint. The only risk is that a later pass trusts the SPEC's 20 or the note's 14. So it stays minor.

## 28. [minor] The release safety net (polled primary button) has no test, and the test double cannot report the button state
`crates/snipdeck-app/src/controller/dock_ctl.rs:505` (tests lens)

**Evidence.** drag_let_go() is called only from binding.rs:608. The test Targets double (host.rs:425-445) implements cursor and monitor_at but not primary_down, so the default None applies and drag_let_go is always false in tests. drag_cancel is not tested either (grep finds no test callers). The design note relies on this safety net for 'Release is the real button release … plus a polled primary button is up'.

**Scenario.** If drag_let_go regressed (for example `== Some(true)`), or if the Slint pointer-up were lost after the window was resized under the pointer by a Preview, the window would keep following the cursor after the button is released, the old 'spasming' symptom. Every test would still pass.

**Fix.** Add a settable primary_down to the test double. Add controller tests: press, pull, set the button up, assert drag_let_go() is true and that drag_end floats or docks correctly. Also test drag_cancel returning the remembered placement.

**Verifier.** I tried to refute this and could not. At 529fd6b, dock_ctl.rs:505-506 defines `drag_let_go()` as `self.drag.is_some() && self.target.primary_down() == Some(false)`. A grep of every .rs file finds one caller, the drag timer in binding.rs:605-611, and no test calls it. The `Targets` trait default `primary_down()` returns None (host.rs:209-211). The test double's impl (host.rs:~433-445) overrides `cursor` and `monitor_at` but not `primary_down`, so in every controller test `drag_let_go()` is constantly false. A regression in it would go unnoticed: for example `== Some(true)`, a dropped `drag.is_some()` check, or a swapped branch in binding.rs. If release then failed, the window would keep following the cursor (the old 'spasming' symptom) and every test would still pass. The only real `primary_down()` impl is in platform.rs:88 (calls the Windows `primary_button_down()`, screen.rs:49). It is Windows-only and also untested here. The finding also says `drag_cancel` is untested. That is true, and in fact worse: grep finds no caller of `drag_cancel` anywhere in the workspace, test or production. It is dead code (dock_ctl.rs:639), so the 'window went away' cancel path the doc comment describes is not wired up. That is worth a separate look. The finding is still minor. It is a test-coverage gap, not a behaviour defect I can show today: the production logic reads correctly, and binding.rs calls `drag_finish` when `let_go` is true.

## 29. [minor] The 'a press that does not move never docks' tests take a single step, so they pass with the guard removed
`crates/snipdeck-app/src/dock_drag_tests.rs:115` (tests lens)

**Evidence.** a_press_that_does_not_move_never_docks makes one call, moved((500,2), t=5000). a_press_on_a_floating_header_changes_nothing_about_docking (controller/tests/dock.rs:626-636) also makes one pull at 5000. Without the `pushed` guard (dock.rs:392-396) that first call would only set the candidate with since=now, and arming needs at least 200 ms more, so neither test can fail. The real host calls drag_move every 16 ms with the same cursor (binding.rs:598-613), and that is exactly the case the guard is for.

**Scenario.** Delete `if !pushed { return None; }` and both tests still pass. In the app, a press held still within 4 px of an edge would then preview a dock after 200 ms.

**Fix.** In both tests, call moved/pull with the same cursor at t and t+200 (and t+5000), then assert Float and no preview.

**Verifier.** I tried to refute this and could not. At 529fd6b, `Drag::begin` (crates/snipdeck-app/src/dock.rs:279-289) starts with `candidate: None`. In the Float branch of `moved` (dock.rs:387-404), removing the `if !pushed { return None; }` guard only means the first call sets `candidate = (edge, ctx.now)`. That is the Top edge for (500,2) and the Left edge for (2,110). `armed` then needs `elapsed_ms(now, since) >= DOCK_ARM` (200 ms), and on that first call the elapsed time is 0.

- **dock_drag_tests.rs:115-121:** the test calls `moved((500,2), t=5000)` once. Without the guard nothing arms. grab is (100,2), so (x,y) = (400,0), which equals the frame, and the call returns `DragStep::None` with the phase still Float. Both assertions still pass.
- **controller/tests/dock.rs:626-640:** `press` calls only `drag_begin`, which does not move the drag (dock_ctl.rs:494-500). `advance(5000)` then sets the clock, and the single `pull` calls `drag_move` once (dock_ctl.rs:512+). That call sets the candidate with since = 5000 and elapsed 0, so the result is no effects, no preview, and `drag_end` places `at_edge` with no dock edge. Every assertion still passes.

I also checked the other tests that begin near an edge: dock_drag_tests.rs:62, and controller/tests/dock.rs lines 585, 865 and 1196. In each one the cursor moves away from the press point, so `pushed` is true, and none of them holds still at the press point for two or more ticks. No other test catches the guard's removal either.

The host does call `drag_move` repeatedly, on a `DRAG_TICK` timer (binding.rs:600, 624). So a press held still at an edge is the real case the guard exists for, and it is untested. This is a gap in the tests, not a defect in the code: behaviour is correct today. Minor.

## 30. [minor] The corner wobble assertions run only after Preview has started, so they do not test corner stickiness before arming
`crates/snipdeck-app/src/dock_drag_tests.rs:155` (tests lens)

**Evidence.** a_corner_offers_one_edge_and_keeps_it arms Left at t=200 and only then wobbles (lines 156-159). In the Preview phase moved() only checks edge_gap to the previewed edge (dock.rs:380-386), and no code there can change the edge, so `preview()==Some(Left)` cannot fail. The pre-arm keep (dock.rs:388-399), which stops a corner from flipping between Left and Top and restarting DOCK_ARM, is never tested with a wobble before arming. The boundary at exactly EDGE_ZONE (4 px) is also untested: the zone test checks 5 and 3 only (dock_drag_tests.rs:82-87).

**Scenario.** Change the candidate logic so it recomputes the nearest edge each tick and keeps `since` only when the edge is unchanged. A corner wobble like (3,0),(0,2) then flips Left/Top and restarts the 200 ms, which is the 'can't drag to a corner, spasming' report, and this test still passes. A `<` vs `<=` mistake at gap 4 also survives.

**Fix.** Wobble in the corner between t=0 and t=199 and assert that Preview(Left) appears at t=200 exactly. Add a gap==4 case.

**Verifier.** The main claim holds at 529fd6b. In `a_corner_offers_one_edge_and_keeps_it` (dock_drag_tests.rs:149-164), the drag arms Left at t=200, and the wobble loop runs at t=300-450, after that. Once the phase is `DragPhase::Preview(edge)`, dock.rs:380-386 only compares `edge_gap` to `EDGE_EXIT` for the edge already chosen, so the edge cannot change and `preview()==Some(Left)` cannot fail there. The code that does stop a corner flipping is the `candidate` keep in the Float phase (dock.rs:388-399: `kept = candidate.filter(gap <= EDGE_EXIT)`, which runs before `edge_at` is asked again), and no test exercises it with a corner wobble before arming. I checked the other tests for that coverage. `brushing_an_edge_docks_nothing` moves along one edge only. `the_dock_is_offered_only_after...` holds the cursor still. The second half of the corner test, `top`, also holds still. Under the suggested mutation (recompute the nearest edge every tick and keep `since` only when the edge is unchanged), every test in dock_drag_tests.rs still passes. So the regression behind the owner's "corner, spasming" report could come back without a test failing. The code itself is correct today, so this is a test-coverage gap, not a behaviour defect: minor. The secondary claim, that the gap==EDGE_ZONE boundary is untested, is wrong. frame.rs:1248 asserts `edge_at((4,500), work, None, 4) == Some(Left)`, which catches a `<` vs `<=` mistake in `edge_at`. `the_edge_zone_scales_with_the_monitor` (dock_drag_tests.rs:260-266) arms a Drag at exactly 6 physical px, which is `px(4, 1.5)`, the zone boundary at 150% scale.

# Completeness critic (not separately verified: check each before fixing; say which you reject and why)

## 31. [blocker] Upgrade keeps every edge the old buggy auto-dock saved, and now also turns on tucking, so the owner's Panel opens docked and hides to a 6 px tab on first launch
`crates/snipdeck-app/src/settings.rs:1136`

**Evidence.** Settings::sanitize (settings.rs:1136-1143) moves pre_dock into w/h but leaves g.edge alone for every old entry. DockSettings::sanitize (settings.rs:230-235) turns an old trigger of `off` into `Both` for any file without `tucks_by_default`. docs/reviews/dock-diagnosis-2026-11-12.md:15 and :27 show the old code wrote an `edge` on almost every drop (a tall frame always previewed Top; frame_settled docked mid-drag) and never cleared it. dock-redesign.md:97-99 says these stay. The test an_old_docked_geometry_becomes_a_floating_size_and_a_docked_size checks that `edge: left` survives.

**Scenario.** The owner's settings.json holds `edge: "top"` (or right) from one of the unwanted docks they complained about, and `dock.trigger: "off"` (the old default). After the upgrade the Panel opens docked to that edge. dock_live() is now true, so the 50 ms tick tucks it to a 6 px tab 400 ms after the pointer moves away. A Panel they never meant to dock is now docked and also hides itself. That is their complaint, "it always seems to want to be docked", on the very first run. Getting it to float takes a hover-reveal and then a 24 px pull or a menu row.

**Fix.** In the one-time migration (the same `!tucks_by_default` branch), treat edges written by old versions as unreliable: clear `edge`/`along` (keep `docked` as a size hint) or at least do not switch Off to Both for an entry docked by the old code. Add a migration test: an old file with edge+pre_dock and trigger off loads as floating, or as a docked Panel that stays open.

## 32. [major] After a hidden start (the default at Windows login) or a tray hide, a docked Panel has no tab, so pointing at the edge shows nothing
`crates/snipdeck-app/src/main.rs:464`

**Evidence.** main.rs:464-466: `if start_hidden { controller.borrow_mut().hide(); ... }`, and no window is ever shown. start_hidden is true at login because panel_start_visible defaults to false (settings.rs:860; starts_visible() at settings.rs:892-894). dock_ctl.rs:112-114: `dock_live() = self.visible && self.dock_active()`, so the hover poll (binding.rs arm_dock) never runs and no 6 px tab exists. controller.rs:925: the tray toggle calls hide() on a docked Panel in the same way. DESIGN.md:292 and dock-redesign.md:60 promise a tab that is always there.

**Scenario.** The owner docks the Panel to the right edge as they asked ("like the toolbar with microsoft pc manager") and restarts or logs in. Snipdeck starts hidden from the startup task. Moving the mouse to the right edge does nothing until they find the hotkey or the tray icon. The same happens after any tray-click hide. Their exact words: "it doesnt show when I want it or hover my mouse near it".

**Fix.** When the stored geometry for the Panel's monitor has an edge and the trigger is not Off, start (and hide from the tray) to the tucked state instead of hidden: show the window as the handle (dock = Collapsed, PlaceWindow(handle_rect)), so dock_live() is true. Add a controller test: a hidden start with a docked geometry gives a live dock and a handle placement.

## 33. [major] A docked Panel is now always topmost and reveals on hover over full-screen apps (games, videos, slideshows)
`crates/snipdeck-app/src/controller.rs:566`

**Evidence.** controller.rs:566: `topmost: (self.on_top || self.dock_active()) && !self.own_window_active`. dock_ctl.rs:149-169: dock_tick polls the cursor every 50 ms and reveals when it is within REVEAL_ZONE of the tab. Nothing checks for a full-screen app: snipdeck-platform/src/windows/target.rs:830 fullscreen_in_front() exists but has no caller (grep finds only its definition). SPEC.md:73 requires that "the panel auto-hides while a fullscreen app is in front". Before this change an unpinned docked Panel was not topmost and hid on focus loss, so it never appeared over full-screen apps.

**Scenario.** The Panel is docked Left or Bottom. The owner plays a full-screen game (RTS edge-scrolling moves the cursor to the screen edges), or moves the mouse to the seek bar of a full-screen video, or runs a PowerPoint slideshow. The accent tab sits on top of the full-screen content, and after 150 ms near the edge the whole Panel slides out over it and stays while the pointer is within 8 px.

**Fix.** In dock_live() (or in dock_tick and in the topmost computation), stop when a full-screen app is in front: expose fullscreen_in_front through TargetSource, and while it is true drop topmost and skip reveals. Restore both when it clears. Add a controller test with the test double reporting a full-screen app.

## 34. [major] The Panel hotkey tucks a docked Panel that the pointer has revealed instead of giving it the keyboard
`crates/snipdeck-app/src/controller.rs:799`

**Evidence.** controller.rs:798-799: `TogglePanel if dock_live() && dock_collapsed() => show()`, and `TogglePanel if dock_live() => dock_tuck()`. The second arm runs for any open docked Panel, including one revealed by hover. A hover reveal never takes the keyboard (binding.rs:451-454: "A dock-hover reveal never comes here"), so panel_focused is false. dock_ctl.rs:174-186 then collapses it. The only test, the_panel_hotkey_folds_and_unfolds_a_docked_panel_instead_of_hiding_it (controller/tests/dock.rs:264-290), first calls set_panel_focused(true), so the unfocused case is not tested.

**Scenario.** The pointer passes near the tab and the Panel slides out without the keyboard. The owner presses the Panel hotkey to type a search. The Panel tucks away and the keystrokes go to the Target. He has to press the hotkey again to get it back. To them the Panel vanishes when they ask for it ("it doesnt show when I want it").

**Fix.** When the docked Panel is open but does not have the keyboard (`!self.panel_focused`), make the hotkey call show() (which activates it and focuses search). Tuck only when it already has the keyboard. Add a test: reveal by dock_tick, panel_focused false, the hotkey gives ShowPanel and no handle placement.

## 35. [major] The reveal zone reaches 18 px into the screen, over the scrollbars and browser tab strips of maximized windows, so the Panel pops out when the user means to use them
`crates/snipdeck-app/src/controller/dock_ctl.rs:157`

**Evidence.** dock_ctl.rs:157: `HandleHover(frame::near(cursor, tab, zone(dock::REVEAL_ZONE, scale)))`. REVEAL_ZONE is 12 (dock.rs:189) and the tab is 6 px (frame.rs:16), so the zone is 18 logical px from the edge. The test the_pointer_near_the_tab_reveals_the_panel_and_leaving_tucks_it (controller/tests/dock.rs:333-338) reveals at x=1905 on a 1920 work area, 15 px from the edge. A Windows vertical scrollbar is 17 px wide at the right edge of a maximized window, and Chrome/Edge tabs start at y=0 when maximized. The tab window itself is topmost (controller.rs:566) and covers the outer 6 px of those controls.

**Scenario.** Docked Right: the owner reaches for the scrollbar of a maximized browser or editor beside the Panel's span. After 150 ms the Panel slides out over the scrollbar and stays while the pointer is within 8 px, so they cannot scroll. Docked Top: hovering browser tabs reveals the Panel over them. The Panel shows when they do not want it and covers what they wanted, which adds to "sooooo unusable".

**Fix.** Reveal only when the cursor is at the edge itself (within the tab, or 1-2 px of the work-area edge, like an auto-hide taskbar), not 12 px into the screen. Keep the larger zone only for keeping an open Panel. Add a test that a cursor at the scrollbar position (for example x=1905 on Right) does not reveal and one at x=1919 does.

# Refuted (no action)

- A resize under way does not hold the docked Panel open, although the design says it does: The code does not match the design note. dock_held() (dock_ctl.rs:134-137) checks only the menu, panel_focused and drag, and Controller::resize (panel.rs:693-701) records nothing, so the note's line "a drag or resize is under way" (dock-redesign.md:62) is not implemented, and no test claims it is. The failure scenario does not hold up, though.

(1) The resize is the OS modal loop (main.rs:633-642,
- The dock now polls at 20 Hz for as long as a Panel is docked, including when it is idle and tucked: The facts are accurate, but this is a deliberate design trade-off, not a defect.

What checks out at 529fd6b:
- `arm_dock` (binding.rs:559-583) now keys on `dock_live()` instead of `dock_needs_tick()`. It starts a repeated timer at `dock::TICK`, which is 50 ms (dock.rs:23).
- `dock_live` is `visible && dock_active()` (dock_ctl.rs:112-114). A floating Panel gets trigger Off from `dock_config` (dock
- Migration turns tucking on but keeps the docks the old bug created, so the owner's Panel stays docked and now hides: The mechanism is real, but it is not a defect. At 529fd6b, DockSettings::sanitize (settings.rs:229-235) changes an old `off` trigger to `both` once. sanitize_panel_geometry (settings.rs:1127-1155) keeps `edge` and `along`. placement() (controller/panel.rs:800-809) puts a stored edge flush with docked_rect, and dock_config/dock_tick (dock_ctl.rs:51-57, 149-170) tuck it after the leave delay. So an 
- Re-docking ignores the size the user gave the floating Panel (owner asked to dock 'at the same height and width I had it at'): The code works as specified, so this is not a defect. dock_to (crates/snipdeck-app/src/controller/dock_ctl.rs:409) docks at `geometry.docked`, which is the docked size the user last had. It falls back to the floating size only on the first dock. The specs ask for exactly this:
- docs/reviews/dock-redesign.md:84 says "Docking for the first time keeps the floating size. A resize while docked changes
- Sliding a docked Panel undocks on a pull toward the edge too, so wobble into a neighbouring monitor undocks it: This is not a real defect. The code is as quoted: dock.rs:372-375 takes the cursor movement perpendicular to the edge (`across`) and floats the Panel when `abs(across) > px(UNDOCK_PULL)`. That matches what the design asks for. The note (dock-redesign.md:38 and the table at :47) and the constant's doc comment (dock.rs:183-184) say "24 logical px across the edge". There, "across" means perpendicular
- An edge candidate keeps arming after the cursor leaves the 4 px zone (up to 12 px): I don't think this is a defect. The code does what the design note says. In docs/reviews/dock-redesign.md:50 the note states: "The candidate is kept while the cursor stays within `EDGE_EXIT` of it, so a corner cannot oscillate." So the wider EDGE_EXIT check before the preview appears was chosen on purpose, not only as hysteresis for leaving Preview, as the finding claims.

dock.rs:388-390 does exa
- Kebab menu test relaxed to accept the menu covering the header at the default rig size: The facts in the finding are correct, but the change was intended and documented, not a test weakened to hide a bug. At fee4a99d the test asserted `mp.y >= kebab.y + kebab.h - 8` at 560 px. At 529fd6b, crates/snipdeck-ui/tests/panel_logic.rs:2160-2172 asserts only that the menu stays inside the window at 560 px, then resizes to 640 px and asserts "below the button". This follows a deliberate place
