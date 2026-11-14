# Step 13 plan: owner findings + UX (diagnosis by Sonnet, 2026-10-19, read-only, no cargo)

Order: implement AFTER the agent-dock, agent-picker and agent-editor merges (they edit `settings.rs`, `settings.slint`,
`editors.slint`, `main_window.slint`, `main_binding.rs`; item 7 needs agent-picker's toast window). Line numbers are
from `claude/compassionate-pascal-pod8ux` at 0759a2c; re-grep after the merges. Build order of tasks: T1 (items 1, 2?, 4),
T2 (3), T3 (5), T4 (6, 7), T5 (8), T6 (9). Tests first for each. All Windows behaviour below is UNVERIFIED until the owner runs it.

## Findings in one paragraph

There is no hide-on-focus-loss anywhere in the code (grep of `crates/` for Focused / blur / deactivate finds only a
trace line, `main.rs:445`). What the owner sees is (a) the unpinned Panel buried behind other windows with no taskbar
button (`controller.rs:655` `set_on_top` only toggles always-on-top; `main.rs:540` gives every window but the guarded
ones `skip_taskbar(true)`), and (b) the Main window ALSO getting `skip_taskbar(true)` + `active(false)` because the
`RegularWindow` guard is taken too late (item 1). Same bug hits the editors and Settings.

## 1. Main window in the taskbar, never auto-hides

Root cause (certain from reading Slint 1.18.1 source, Windows effect unverified):
- `select_backend` hook (`main.rs:529-546`) returns early only while `regular_window_being_shown()` is true.
- Slint calls the hook in `create_window_adapter` (`i-slint-backend-winit-1.18.1/lib.rs:894-899`), i.e. inside
  `ui::X::new()`, and bakes the attributes into the adapter (`winitwindowadapter.rs:489`, used by `ensure_window`
  at show time). The guard is taken AFTER `new()`:
  - Main: `MainWindow::new()` `main_binding.rs:470`, guard `main_binding.rs:479`.
  - Editors: `ListEditor::new()` `main_binding.rs:233`, `BoardEditor::new()` `:265`, guards `:194`, `:215`.
  - Settings: `SettingsPage::new()` `settings_binding.rs:959`, guard `:971`.
  - agent-editor: `editor_binding.rs:171` has the same shape; fix it too after the merge.
  So all of them are created `skip_taskbar(true)` + `active(false)`: no taskbar button, shown un-activated, and once
  another app is clicked they sink behind it with no way back except the tray. That matches "hides when I switch".
- No other code hides the Main window: it has no `on_close_requested` (default = hide on the X), no focus handler.

Change:
1. `main_binding.rs`: add `impl RegularWindow { pub fn create<T>(make: impl FnOnce() -> T) -> T }` that holds a guard
   around `make()`. Replace `ui::MainWindow::new()?` (:470), `ui::ListEditor::new().ok()?` (:233), `BoardEditor::new` (:265),
   `settings_binding.rs:959` and later `ItemEditor::new` / `TagManager::new` by `RegularWindow::create(ui::X::new)`.
   Keep the guard around `show()` as well (harmless; remove the now useless `let _regular` at :194/:215/:479/:971 =
   "replace, do not layer").
2. Extract the hook decision into a pure fn so it is testable off Windows (new file `crates/snipdeck-app/src/window_kind.rs`):
   `pub struct Plan { pub skip_taskbar: bool, pub active: bool, pub window_icon: bool }`
   `pub const fn plan(regular: bool) -> Plan` -> regular: `{false, true, true}`, floating: `{true, false, false}`.
   `select_backend` applies it (Windows part only under `cfg(windows)`; add `trace!("window attributes: regular={regular}")`
   so a `SNIPDECK_LOG=1` run proves each window got the right plan).
3. Close behaviour (polish A2, the setting label already promises it): `bind_main` registers
   `window.window().on_close_requested(..)`: ask `core.controller.borrow_mut().main_window_closed()`; if it returns
   true call `slint::quit_event_loop()`; return `HideWindow`. New `Controller::main_window_closed(&mut self) -> bool`
   in `main_ctl.rs`: `!settings.close_to_tray || (!session.tray_available && !self.visible)` (quit when the setting is off,
   or when nothing could bring a window back). Minimise needs nothing (OS caption).
4. Never auto-hide: nothing to remove. Add a regression test (below) so item 6's blur-hide cannot reach the Main window.

Tests first:
- `crates/snipdeck-app/src/main_binding.rs` (unit): `a_window_created_under_create_sees_the_regular_flag` (closure sees
  `regular_window_being_shown()`, false after) and `create_nests_with_begin`.
- `crates/snipdeck-app/src/window_kind.rs` (unit): `a_regular_window_is_in_the_taskbar_and_active`,
  `a_floating_window_is_skipped_and_not_activated`, `only_regular_windows_get_the_icon`.
- `crates/snipdeck-app/src/controller/main_ctl.rs` or `controller.rs` tests (use `world_with(.., Settings{close_to_tray:false,..})`,
  `FixedClock`): `closing_the_main_window_quits_when_keep_running_is_off`, `..._does_not_quit_with_a_tray_icon`,
  `..._quits_without_tray_when_the_panel_is_hidden`.
- `crates/snipdeck-app/tests/main_window.rs`: `the_main_window_stays_open_when_the_foreground_app_changes` (after item 6: open
  Main, unpinned panel + blur-hide on, `ctl.on_foreground_changed()`; assert `ui.window().is_visible()`).
Goldens: none. Windows-unverified: taskbar button, Alt+Tab entry, activation on open (log `regular=true` for Main/editors/Settings).

## 2. "+" dead after closing New list and the Main window

Traced end to end, no stuck state found in the code:
`main_window.slint:729` SidebarHeading `action` -> `add-list-requested` -> `main_binding.rs:373` `wire!(on_add_list_requested)`
-> `core.act(|ctl| ctl.main_new_list())` -> `main_ctl.rs:737` `main_fresh()`, `open_editor` (`:836`, `editor_seq += 1`, so each open is "fresh")
-> `redraw_all` -> `MainCore::sync` -> `sync_editors` (`main_binding.rs:169-226`) shows the editor when `editors.seq != view.seq`.
Editor X: `on_close_requested` (:252) -> `main_editor_close` (`main_ctl.rs:954`, `editor = None`) -> hides. Main X: default hide (no handler).
Reopen: `MainUi::open` (:470-485) reuses window and binding, `redraw`, `show`, `fit_to_screen`, `raise`. The existing test
`open_snipdeck_from_the_tray_creates_the_window_once...` (tests/main_window.rs:165) covers reopen but not close-editor-then-reopen.
Also note: the toolbar "+ New item" button (`main_window.slint:905`) is a NO-OP by design today (`main_binding.rs:392` logs "no item editor yet";
agent-editor wires it at its :398). If the owner meant that "+", nothing is broken beyond the missing feature; confirm after the merge.

Verdict: not reproducible from code, needs a Windows run. Most likely cause is windowing, not state: the Main window and the editor were created
un-activated and out of the taskbar (item 1), so after the editor closed and Main was hidden and re-shown Windows may leave a window that never
becomes the foreground one. Item 1's fix is the first thing to try. Do this regardless:
- Test first (`tests/main_window.rs`): `add_list_works_again_after_the_editor_and_the_main_window_were_closed`:
  rig; `invoke_add_list_requested`; `editor.window().dispatch_event(WindowEvent::CloseRequested)` (headless close path, not `invoke_cancel`);
  `ui.window().hide()`; `main_ui.open()`; `invoke_add_list_requested`; assert `list_editor().window().is_visible()` and name empty.
  Expected to pass; it pins the state machine so the next report points at windowing.
- Add the diagnostics the Panel has to the Main window and editors: generalise `watch_window_events` (`main.rs:407-471`) so its tracing half
  (pointer, Focused, Resized, CloseRequested) can be attached to any window; call it from `MainUi::open` on creation and from the editor
  constructors (only when `trace::enabled()`). Log to ask the owner for: `SNIPDECK_LOG=1`; after the failure click "+":
  line `callback main add_list_requested` present -> controller/show path (then look at `show list editor` lines);
  absent but `pointer Pressed` present -> Slint did not hit the button; absent and no pointer line -> the window is not receiving input (OS level).
- Windows-unverified: everything above.

## 3. Tags gear (and "Manage tags…" menu entry) do nothing

Root cause: `main_binding.rs:393-395` binds `on_manage_tags_requested` to a trace line ("no tag manager yet"). The Slint side is complete:
`main_window.slint:758` (gear in the Tags heading) and `:340` (nav menu "Manage tags…") both emit `manage-tags-requested`. The `TagManager` window
exists and is tested (`editors.slint:996`, goldens `tag-manager-*.png`, `editors_logic.rs:575-660`) but nothing in `snipdeck-app` constructs it.
The store can rename (`TagRepo::update_tag`, `repo.rs:114`, clash -> `Error::TagExists`) and delete (`main_delete_tag`, `main_ctl.rs:515`).

Change (bind it; do not hide the gear):
- `main_ctl.rs`: `pub enum TagRenameError { Name(NameError), Exists, Failed }` and
  `pub fn main_rename_tag(&mut self, index: usize, name: &str) -> Result<(), TagRenameError>`: `main_fresh()`, `Name::new(name)` (map with `NameError::of`),
  `self.main.tags().get(index)` -> `Tag { id, name }`, `store.update_tag`; `Err(Error::TagExists)` -> `Exists` (no toast); other `Err` -> `finish` toast + `Failed`;
  `Ok` -> `library_changed()`. Same index space as `screen.tags`.
- `wording.rs`: `pub fn tag_rename_error_text(&TagRenameError) -> String`: Exists "A tag with that name already exists." (Name(..) reuses `name_error_text`). English literal like the rest of wording.rs.
- `main_binding.rs`: `MainCore` gets `tag_manager: RefCell<Option<ui::TagManager>>`; `fn tag_manager_window(self: &Rc<Self>)` creates it once with
  `RegularWindow::create(ui::TagManager::new)` and wires: `on_rename_requested(i, name)` -> `core.act(..)` result -> `Err` => `set_rename_error(text)`; `Ok` => `set_rename_error("")` + `finish_rename()`;
  `on_delete_confirmed(i)` -> `core.act(|c| c.main_delete_tag(i))`; `on_close_requested` -> `hide()`; `window().on_close_requested` default hide.
  `fn open_tag_manager(self)`: create, `sync_tag_manager(&screen)`, `show()`, `fit_to_screen`, `raise`. `on_manage_tags_requested` calls it (via `wire!`-style closure, not `act`).
  In `sync()` after `render`: if the window exists, `set_tags(model of ChoiceEntry{label: name, count, color: default, checked: false})`, `set_dark`, `apply_look(&tm, &screen)`,
  so a delete or an Undo refreshes it. Add `pub fn tag_manager(&self) -> Option<ui::TagManager>` on `MainBinding` for tests.
- Update the doc comment of `bind_main` (:322-324) that lists "Manage tags…" as not wired.
Tests first:
- `controller/main_ctl.rs` tests (or `tests/main_controller.rs`): `renaming_a_tag_changes_it_everywhere`, `renaming_to_an_existing_name_is_refused_without_a_toast`
  (case-insensitive clash), `renaming_to_blank_says_give_it_a_name`, `renaming_with_a_stale_index_does_nothing`.
- `tests/main_window.rs`: `the_tags_gear_opens_the_tag_manager_with_the_tags_and_counts` (`invoke_manage_tags_requested`; rows == `ui.get_tags()` len; window visible),
  `renaming_in_the_tag_manager_updates_the_sidebar`, `a_clashing_rename_shows_the_error_and_keeps_the_editor`, `deleting_in_the_tag_manager_asks_then_removes_the_tag`.
Goldens: none (TagManager goldens exist and are untouched). Windows-unverified: window placement only.

## 4. Window icon on every window

How the icon exists today: `assets/logo/snipdeck.ico` (8 PNG entries 16..256, made by `scripts/make_logo.py`) is embedded as Win32 resource id 1 by
`crates/snipdeck-app/build.rs` (winresource); the tray loads that resource (`tray.rs:375-391`, `LoadImageW`, `LR_SHARED`, size from `dpi::small_icon_size()`).
No window sets an icon; Slint's `Window.icon` is unset everywhere and Slint never overwrites a winit icon when it is empty
(`winitwindowadapter.rs:1870-1876`: `ImageCacheKey::new(empty)` is `None`, equal to the initial cache key), so an icon set by the hook survives.

Change (one place, crisp per-size marks, no new asset): in `select_backend`'s hook, for the regular branch on Windows:
```rust
use slint::winit_030::winit::{dpi::PhysicalSize, platform::windows::{IconExtWindows, WindowAttributesExtWindows}, window::Icon};
let (s, b) = (snipdeck_platform::windows::dpi::small_icon_size(), /* GetSystemMetrics SM_CXICON, 32 */);
attributes.with_window_icon(Icon::from_resource(1, Some(PhysicalSize::new(s.0 as u32, s.1 as u32))).ok())
          .with_taskbar_icon(Icon::from_resource(1, Some(PhysicalSize::new(b, b))).ok())
```
Expose a small `pub fn icon_sizes() -> ((u32,u32),(u32,u32))` in `snipdeck-platform/src/windows/dpi.rs` (check what `small_icon_size` already returns; add `large_icon_size` next to it with `GetSystemMetricsForDpi(SM_CXICON)`).
Guarded by `window_kind::plan(regular).window_icon`, so the Panel and the other floating windows keep none (backlog A3). Failure (`from_resource` Err, e.g. in tests/dev builds without the resource) = no icon, never an error.
`ICON_ID` stays the single source: export the constant `1` from one place (`snipdeck-platform` `windows::dpi` or a `const APP_ICON_ID: u16 = 1` next to `load_icon` in `tray.rs`) and use it in both.
Tests first: `window_kind` unit test above (`only_regular_windows_get_the_icon`); `crates/snipdeck-app/tests/app_icon.rs`: `the_ico_has_the_16_and_32_pixel_marks_the_window_icon_asks_for` (parse, assert sizes 16/20/24/32 present).
Goldens: none. Windows-unverified: title-bar and taskbar icon for Main, Settings, editors, TagManager (check light/dark taskbar, 100%/150%/200% scale).
Fallback if `from_resource` misbehaves: add `window-64.png` to `make_logo.py` output and put `icon: @image-url("../assets/logo/window-64.png")` on each Window root (MainWindow, SettingsPage, ListEditor, BoardEditor, TagManager, ItemEditor); Slint renders it itself.

## 5. New list dialog has a big empty bottom

Root cause: fixed `preferred-height: 440px` on `ListEditor` (`editors.slint:901`) and `BoardEditor` (`:951`); `EntityEditorBody`'s `FormScroll`
(`:779`, `vertical-stretch: 1`) swallows the unused height. A new list has name + colour + sort only (no Delete row, no Notice). Also note Slint applies
`preferred-size` only on the FIRST show (`winitwindowadapter.rs:1659-1669`, `!recreating_window`), and the editor windows are reused, so the host must size on each open.
Change:
- `EntityEditorBody`: give the footer an id (`footer := EditorFooter`) and add
  `out property <length> wanted-height: form.preferred-height + footer.preferred-height;` (check `EditorFooter` exposes it: its `height` is `row.preferred-height + 1px`, so use `footer.height`).
- `ListEditor` / `BoardEditor`: `preferred-height: body.wanted-height;`, `min-height: min(300px, body.wanted-height)`; expose `out property <length> wanted-height <=> body.wanted-height;`. Remove the literal 440px (audit 13a item 3).
- Host (`main_binding.rs` `sync_editors`, both arms, in the `fresh` block after `show()` and before `fit_to_screen`): `window.window().set_size(slint::LogicalSize::new(460.0, window.get_wanted_height()))`
  (get it after `fill_*` so is-new / hint are applied). Keep `fit_to_screen` and `raise`. Do the same in the agent-editor's item editor only if it has the same dead space (not in scope here).
Tests first: `crates/snipdeck-ui/tests/editors_logic.rs`: `a_new_list_asks_for_less_height_than_an_existing_one` (is-new true < existing with delete row; both < 440 and >= footer+form),
`the_wanted_height_grows_with_the_last_board_notice`. `crates/snipdeck-app/tests/main_window.rs`: `add_list_sizes_the_editor_to_its_content` (`editor.window().size()` height == wanted, logical).
Goldens: the editor goldens use a fixed `SMALL` harness size (`editors.rs:185-217`), so `list-editor-*` / `board-editor-*` should NOT change; if the implementer changes the harness size they will.
Windows-unverified: real window height with caption + DPI scaling, and fit_to_screen after set_size.

## 6. Settings "Window behaviour" group

State of the code: `Settings.autostart` + a working registry writer exist (`settings.rs:177`, `SettingsHost::set_autostart` `settings_binding.rs:52,100`, HKCU Run with `--hidden`, `windows/autostart.rs:78`),
toggle in the "General" section (`settings.slint:249-259`) with toast on failure (`settings_binding.rs:639-647`, test `settings_window.rs:394`). SPEC 5.x: opt-in at onboarding (7a), default off. So "Start with Windows" is a MOVE, not new.
`close_to_tray` (`settings.rs:179`, `settings.slint:262`) means "close button hides to tray instead of quitting" (`controller.rs:548-557`, Panel only today; Main after item 1). It is NOT hide-on-blur.
There is no hide-on-blur setting or code: new.
New fields (`settings.rs`, struct + `Default` + `from_fields` `field!` lines after `field!(close_to_tray)` at :247):
- `pub panel_in_taskbar: bool` default `false` ("Show Snipdeck in the taskbar", Panel button).
- `pub panel_hide_on_blur: bool` default `true` ("Hide the panel when it loses focus"; acts only while the Panel pin is off: `on_top == false`, which is session state, starts true at `controller.rs:355`, so nothing changes for a user who never unpins).
Tests first: `settings.rs` tests `old_files_without_the_window_fields_get_the_defaults` (`from_json("{}")`: false/true), `one_bad_window_field_keeps_the_rest` (`{"panel_in_taskbar":"x","theme":"dark"}`), round trip.
Settings UI: `settings.slint` new `FormSection { title: @tr("Window behaviour"); ... }` placed before "General" (after dock's "Panel dock" section when merged) with four `SettingRow`s in this order:
1. "Show Snipdeck in the taskbar" / "Gives the panel a taskbar button. Off keeps it out of the taskbar and Alt+Tab; the tray icon and the panel shortcut still bring it back." (`in-out property <bool> panel-in-taskbar`, `callback panel-in-taskbar-toggled(on: bool)`)
2. "Hide the panel when it loses focus" / "When the pin is off, the panel hides as soon as you switch to another app. A pinned panel stays." (`panel-hide-on-blur`, `panel-hide-on-blur-toggled`)
3. "Start with Windows" (MOVED from General, same @tr msgids so no new translation)
4. "Keep running in the tray when the window closes" (MOVED; change the description to "Turn this off and closing the Panel or the Main window quits Snipdeck.": a new msgid)
Copy the Toggle block with `a11y-label` and the `reveal-field` focus handler exactly as at `settings.slint:251-259`. Wire like `close_to_tray`: `settings_binding.rs:325` (PageView field), `:394` (view builder), `:799` (`wire!`), `:853-857` (`render` diff). Strings: `python3 scripts/i18n.py extract` then Haiku pass for the new msgids in es/de/fr/pt-BR/ja/zh-Hans (`check` must report 0 per HANDOVER).
Goldens that change: every full-page settings golden: `settings-light`, `settings-dark`, `settings-full-light`, `settings-full-pro-erase-dark`, `settings-demo-light`, `settings-scale-light`, `settings-recording-light`, `settings-conflict-narrow-light` (regenerate with `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui`, review each diff by eye; fixture `editor_fixture.rs:91 settings()` gets the new properties).
Behaviour, Panel taskbar button (Windows):
- Pure part first: `win_logic.rs` `panel_ex_style(style, in_taskbar)`: true -> `(style | WS_EX_NOACTIVATE) & !WS_EX_TOOLWINDOW | WS_EX_APPWINDOW`; false -> today's. Test `panel_style_with_taskbar_has_appwindow_and_no_toolwindow` next to `win_logic.rs:518`; update the existing test to the 2-arg form.
- `window_style.rs`: keep the flag in the subclass `dwRefData` (`SetWindowSubclass(.., SUBCLASS_ID, in_taskbar as usize)`; the proc already receives `_data`, :143) and use it in the `WM_STYLECHANGING` branch (:157). Add `pub fn set_panel_taskbar(hwnd, on) -> bool`: re-install subclass data (`SetWindowSubclass` again updates refdata), if the window is visible `ShowWindow(SW_HIDE)`, `SetWindowLongPtrW(GWL_EXSTYLE, panel_ex_style(cur, on))`, `ShowWindow(SW_SHOWNOACTIVATE)` (Windows only refreshes the taskbar button on a hide/show). `install_panel_style(hwnd, in_taskbar)` gains the argument.
- App: `native_window.rs` `pub fn set_panel_taskbar(window: &PanelWindow, on: bool)` (also calls winit `WindowExtWindows::set_skip_taskbar(!on)` via `with_winit_window`). `SettingsHost` gets `fn apply_window_behaviour(&self, s: &Settings)`; `SystemHost` holds an `Rc<dyn Fn(bool)>` built in `main.rs` from `window.as_weak()`; `window_behaviour_changed` calls it after `update_settings`. Startup: `attach_panel` closure in `platform.rs:144` passes `settings.panel_in_taskbar` (capture it) so `install_panel_style` starts correct; known gap `--hidden` never attaches (tray-icons.md): attach on first `ShowPanel` as that note says.
- Blur hide (pure controller, testable): `controller.rs` new `pub fn on_foreground_changed(&mut self) -> Vec<Effect>`: call `on_target_changed()`; then if `self.visible && !self.on_top && self.settings.panel_hide_on_blur && self.picker.is_none() && self.fill.is_none()`
  AND the new foreground is a real app (`self.target.current().is_some()`, so clicking the tray or desktop, which the tracker reports as target `None`, does not hide the Panel the tray just opened)
  AND `clock.now() - self.shown_at > 400 ms` (new field `shown_at: Timestamp`, set in `show()`; frozen-clock tests) -> `self.hide_with_hint()`.
  `binding.rs:387` calls `core.act_effects(Controller::on_foreground_changed)` instead of `act(on_target_changed)` (keep the `try_borrow_mut` guard). Own windows (Main, Settings) never trigger it: `target.rs:119` ignores own-process foreground.
  Tests first (`controller.rs`, `world_with`, `FixedClock.set`): `an_unpinned_panel_hides_when_another_app_takes_the_foreground`, `a_pinned_panel_stays`, `with_the_setting_off_the_panel_stays`,
  `the_tray_or_desktop_taking_the_foreground_does_not_hide_the_panel`, `a_foreground_change_right_after_show_is_ignored`, `the_picker_open_blocks_the_hide`, `the_main_window_is_not_affected` (tests/main_window.rs, item 1).
Windows-unverified: taskbar button appearing/vanishing live, click on that button with a WS_EX_NOACTIVATE window, no flicker on toggle, blur-hide vs. the tray left click race.

## 7. First-hide toast

Dependency: agent-picker's `toast_binding.rs` + `toast_window.slint` (shows the controller's toast in a small window while the Panel is hidden and no other Snipdeck window shows toasts). Without that branch merged, a toast raised at hide time is only drawn when the Panel reopens. Do this task after that merge.
Change:
- `toast.rs`: `ToastMessage::PanelHidden { chord: Option<String> }`. `wording.rs`: with chord "Snipdeck is still running. Press {chord} or click the tray icon to bring the panel back."; without "Snipdeck is still running. Click the tray icon to bring the panel back." (English literal, like all toasts).
- `controller.rs`: `Session.hide_hint_shown: bool`; `fn hide_with_hint(&mut self) -> Vec<Effect>`: `hide()`, and if `!hide_hint_shown` set it and `show_toast(Neutral, PanelHidden{chord: non-empty settings.hotkeys.toggle_panel})`. Called by `close()` (:551, when it hides) and by the blur hide (item 6). NOT by the toggle hotkey / tray toggle (the user just used the way back). Per session, not persisted.
- `on_top`-independent: Close button hides too (that is V1-PLAN U "hide-to-tray one-time toast").
Tests first (`controller.rs`): `the_first_close_or_blur_hide_of_a_session_tells_how_to_get_the_panel_back`, `a_second_hide_says_nothing`, `the_toggle_hotkey_and_tray_hide_say_nothing`, `without_a_toggle_chord_the_toast_names_only_the_tray`, `closing_without_a_tray_quits_and_shows_no_toast`; `wording.rs`: `panel_hidden_text_names_the_chord_when_there_is_one`.
`crates/snipdeck-app/tests/` (after merge): toast window test `the_hide_toast_is_drawn_by_the_toast_window_while_the_panel_is_hidden`.
Goldens: none unless a toast golden is added (`panel-*-toast`). Windows-unverified: toast window shows without stealing focus; timing 400 ms grace.

## 8. Wrong strings

- "Capture a region": `settings.slint:106` (`hotkey-names[2]`), help at `:112` "Selects part of the screen and saves it as an item.", and `settings_binding.rs:262` (`action_name`). Truth: the hotkey is quick capture, `capture.rs:1`: saves the TEXT on the clipboard as a new item in the default list (toasts Saved / NothingToSave / TooLargeToSave). There is no region overlay or OCR (HANDOVER M5 lists it as unbuilt; SPEC line 102 describes the future one).
  New text: name "Save the clipboard as an item" (the `action_name` string must equal the Slint one; `settings_window.rs:318` conflict test checks the words, update it), help "Saves the text on the clipboard as a new item in your first list." Note the default chord is empty (capture is off until set).
  Tests: update `settings_window.rs` expectation of the conflict message; `settings_binding.rs` tests that print `action_name`. `i18n.py extract` drops the old msgids and adds two new; translate them.
- "cleared from the clipboard 30 seconds after you use it": `editors.slint:176` (`sensitive-help`; in agent-editor `:193` `sensitive-base`) with `clear-seconds: 30` at `:144`, claimed by SPEC 5.7 (`SPEC.md:41,116`). The code does NOT clear on a timer: grep for any timer/clear after use finds nothing (polish A11 says the same). What actually happens: a sensitive item's payload is written `private()` (`usecase.rs:206`, kept out of clipboard history); an auto-paste snapshots and restores the user's previous clipboard (`pipeline.rs:171-199`) so the secret does not stay; a copy-only use (Shift+Enter, elevated target) leaves the text on the clipboard indefinitely.
  Step 13 change (make the UI true): drop the clause: "Hidden in lists until you hold Space and kept out of clipboard history." (keep the Pro encryption sentence from agent-editor); delete `clear-seconds` and its test (`editors_logic.rs:381-389`); fix SPEC 5.7 and HANDOVER to say "auto-clear is v1.1".
  Real feature (if time, own task, A11): after a copy-only use of a sensitive item, `Timing`-driven timer (injected, test with the fake `Timing`/`FixedClock`) clears the clipboard only if it still holds what Snipdeck wrote (`Clipboard::restore`'s `written` token identifies it), toast "Clipboard cleared"; setting `sensitive_clear_seconds` (default 30, 0 = off) in Settings.
Goldens: `item-editor-*` goldens that show the sensitive help text (check `item-editor-text-*`, `-long-light`) change if the sentence is shown in them.

## 9. Tier A of polish-backlog.md and audit-13a.md, grouped by file (S = this pass, M = next, L = not v1/own step)

### Rust, `crates/snipdeck-app/src`
- `main_binding.rs`: A2 close handler + guard fix (items 1, 3); A1 (M) remember Main size/position/maximised: reuse `frame.rs` + a `main_geometry` settings field, restore in `MainUi::open` before `show`, watch Resized/Moved like `main.rs:407`; A4 dialogs centred over Main and kept above (M): thread-local `PARENT` read by the hook for `with_owner_window(hwnd)`, pure `frame::centre_in(parent, size)` + test, `native_window::centre_over`.
- `main.rs`: A8 (S) second launch: `on_second` (:100) shows the Panel; if the Panel is already visible open the Main window instead (`Effect::OpenMainWindow`); test in `controller.rs`: `a_second_launch_opens_the_main_window_when_the_panel_is_already_up`. A3 icon (item 4).
- `controller/main_ctl.rs`: A13 (S/M) Duplicate item (`MainAction::Duplicate` + menu entry; "Move to list…" exists at `main_window.slint:445`); audit finding 5 labels need no Rust.
- `usecase.rs` / `controller.rs`: A12 (M) `{clipboard}` from `ClipboardText::read_text` (controller passes `None` today), `{date}` local zone (jiff `tz-system`, run `cargo deny`); tests with the frozen clock and a fake clipboard. A10 (S, doc only): `pipeline.rs:171-199` already restores the previous clipboard; confirm the controller's paste goes through it, then delete A10 from the backlog.
- `settings_binding.rs`: A23 (S) "Open backups folder" button next to "Open data folder" (`open_data_folder` pattern, `wire!`, test in `settings_window.rs`); restore preview "what it will add" (M).
- `wording.rs`: A22 already partly done; no change.
- A5 (S, verify only): Esc cancels editors/Settings and Enter saves are in `editors.slint` `EntityEditorBody` (`accepted => try-save`) and `settings.slint` key handler (:124); add one app-level test per window only if a missing case shows up.
- A19/A20: toasts do not take focus (toast window un-activated, after agent-picker); "No results" offers "Create item named …" (S after agent-editor: `Effect::NewItem` with prefilled title).
### Rust, `crates/snipdeck-platform/src/windows`
- `tray.rs`: A7 tooltip names the app and "hotkeys paused" (S: `set_tooltip` exists at :135, call it from `set_hotkeys_paused`); double-click opens Main (M, Windows-unverified: NIN_SELECT also fires for each click of the double-click, so the Panel would toggle twice: skip for v1 unless the owner insists).
- `window_style.rs`, `win_logic.rs`, `dpi.rs`: items 4, 6.
### Slint, `crates/snipdeck-ui/ui`
- `main_window.slint`: audit 2 (S): `⋯` `IconButton` (icon `dots-three`, tooltip "More") on the row on hover/focus that emits the same `item-action` menu request as right-click (`:1039,1106,1176`); audit 5 (S): "Delete" -> "Move to Trash" at `:448` (+ `:1319`), "Empty Trash…" at `:934` -> `ButtonVariant.danger`; audit 3/4 (S): literal `220px/100px` `:312-313` -> tokens, `list-bullets` `:877` -> table icon (rows/squares-four/stack per DESIGN §4). Goldens: `main-*`, `main-compact-light`, `main-empty-*`.
- `components/empty_state.slint:25-26`: audit 1 (S) swap the glyphs (list = `rows`, board = `stack`). Goldens: `empty-light/dark`, `main-empty-*`, `panel-empty-light`, `picker-empty-light`.
- `editors.slint`: items 5; `NoColour` (:559, 22/20px cell :582) -> reuse `Swatch`; literal 440px (`:901,951,1001`) via item 5; `x` removes an app at `settings.slint:331` -> `trash` icon with tooltip (audit 4). `Delete` label `:524` -> "Move to Trash" (it moves items; for list/board delete keep "Delete list").
- `components/hotkey_recorder.slint:9` `HotkeyChip` -> `Keycap`; `:174` `keyboard` icon not in table; `pinned-strip.slint:11` `PinnedChip` -> `Tag`; `form.slint:433` `StepButton` -> ghost `IconButton`; `template_field.slint:137` `calendar-blank` (audit 3/4: M, mechanical, goldens `panel-pinned-overflow`, `settings-recording`, template-fill ones).
- `components/menu.slint:39` (audit 9, S): highlight gets the 2px focus ring; `button.slint:33`, `state-color.slint:8/13`, `tag.slint:27`, `feedback.slint:69`, `dialog.slint:30` derived colours -> new tokens through `scripts/palette.py` (audit 7, M); disabled opacity 0.45 x18 -> `Theme.disabled-opacity` token (M).
- Audit 6 untranslated English (key names `hotkey_recorder.slint:39-52,279-280`, keycaps `picker.slint:476-486`, `panel.slint:766`, `item-card.slint:278`, `template_field.slint:47`): wrap in `@tr` (S), then `i18n.py extract` + translation pass. Audit 8 off-scale glyph sizes (`choice.slint:71/175`, `tag.slint:52/80`, `swatch.slint:56`, `item-card.slint:181`): check each; small controls may be intentional (M, decide per item). Audit 9 13 unused icons: regenerate with `scripts/ui_icons.py` only at the end (L).
- Goldens to regenerate after audit 1-3 (audit item 10): `empty`, `main-compact`, `menu`, `list-editor`, `settings-recording`, `panel-pinned-overflow`.
### Deferred (say so in the HANDOVER, not in this pass)
A9 right-click menu on Panel rows/pinned chips (L, host side unbuilt), A11 timed clipboard clear (M, item 8), A14 drag-and-drop reorder (L), A15 drop to create (L), A18 onboarding/Help (steps 7a/7b), A21 drafts (done by step 8), A24 launcher "already running" (outside the Rust app: `scripts/`), A16 wording is item 8.

## Checks for the implementer
- One build at a time (machine is saturated): `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test -p snipdeck-app -p snipdeck-ui --locked`, then `scripts/gate.sh --changed`; `--full` once at the end. i18n: `python3 scripts/i18n.py extract` then `check`.
- Windows-only code (`window_style.rs`, `tray.rs`, hook icon) cannot be compiled on Linux here; keep it small, `cfg(windows)`, and put the logic in pure fns (`window_kind::plan`, `win_logic::panel_ex_style`).
- Owner run list (Windows): taskbar button for Main/editors/Settings; Alt+Tab; Main stays when clicking other apps; "+" after close/reopen with `SNIPDECK_LOG=1`; icons in title bars; New list height; Settings taskbar toggle live; blur-hide with pin off; first-hide toast; tray left click right after blur-hide.

## 1b. Settings opens half off-screen, white band after dragging (owner 2026-10-19 00:45)
Seen on Windows with a 1478 px work area: the window is created at preferred-height 760 logical (scaled 1.5+ = taller than the work area), `fit_to_screen` runs after `show()` and the window is still off-screen; after a drag the bottom ~200 px are white (never painted). Hypotheses: (a) `request_inner_size` is refused on Windows after show when the Slint min-size constraint holds, (b) the resize is applied asynchronously and Slint renders the old size, (c) the owner's build predates fe2172d. Fix: set the inner size in the winit attributes hook (before the window exists) from the nearest monitor's work area via `work_area_of`, keep `fit_to_screen` as a fallback, make Settings `preferred-height` = min(760px, work area / scale), and log the before/after sizes under SNIPDECK_LOG=1. Verify on Windows: open Settings on a 1080p screen at 150 %.

## 10. Backups to keep (owner 2026-10-19 00:55)
`snipdeck_portable::backup::DEFAULT_KEEP = 7` is a constant; the prune takes a keep count. Add `Settings.backups_keep: u32` (default 7, clamp 1..=60 on load, `field!` wiring), a stepper row "Backups to keep" under the Backups group in settings.slint (same control as the panel width stepper, step 1, min 1, max 60) bound in settings_binding.rs, and pass it to the maintenance backup call. Tests: settings round-trip and clamp; maintenance test that a keep of 2 leaves two files on the injected clock. Goldens: settings-full-* change. Strings: one @tr label plus six .po entries.

## WCAG 2.2 bar for every window
ux-auditor checks each window golden against WCAG 2.2 AA: 2.4.11/2.4.13 focus visible and not obscured, 2.5.8 target size at least 24x24 px, 1.4.3 contrast, 2.1.1 keyboard reach for every control, 1.3.1 labels on every input, 3.3.7 no redundant entry (editors keep drafts). Findings become ranked fixes with goldens.
