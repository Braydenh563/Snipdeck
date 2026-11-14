# Render glitches and shortcuts on Windows: analysis (2026-10-24)

Owner reports (`docs/reviews/owner-test-2026-10-21.md`): (a) About or Edit opened while the Panel is up shows
"mostly transparent except a grey transparent box", then a few header elements, and normal after a click;
(b) dragging the Panel makes "a small column on the right" go transparent for a while; (c) a Tags window
dragged back from off screen "didn't exist, it was just transparent" where it had been off screen; also
"when I opened the panel the small panel disappeared and glitched visually again".
(d) Shortcuts: only Ctrl+Shift+Space worked; Ctrl+Alt+Space stopped showing the Panel mid-session;
Ctrl+Shift+2 and Ctrl+Alt+V did nothing.

Nothing here has run on Windows. The code paths below come from this repo and the pinned sources
(Slint 1.18.1, winit 0.30.13, softbuffer 0.4.8, in `~/.cargo/registry`). The diagnostic
(`tools/windows-test/diag-render.ps1`) decides between the hypotheses.

## The pipeline that draws a Snipdeck window

1. Every window is a winit window drawn by Slint's **software renderer** (`main.rs:select_backend`,
   `renderer_name("software")`). No Mica or other DWM backdrop is set anywhere in the app (no
   `DwmSetWindowAttribute` in the tree; DESIGN.md §5 Mica is still an unbuilt enhancement).
2. Slint creates **every winit window with `with_transparent(true)`** on Windows
   (`i-slint-backend-winit/winitwindowadapter.rs:811`, see `docs/spikes/S4-mica.md`). winit turns that
   into `DwmEnableBlurBehindWindow` with an empty region (`winit/platform_impl/windows/window.rs:1232`):
   DWM then uses the **alpha channel** of the window's pixels. A pixel nobody wrote, or one written by GDI
   (which leaves alpha 0), shows the desktop through it: "transparent".
3. Each frame Slint renders only the **dirty region** into a kept buffer: `renderer/sw.rs:111-116` maps
   softbuffer's buffer age 1 to `RepaintBufferType::ReusedBuffer`, and softbuffer's Win32 backend reports
   age 1 for every buffer that was presented once (`softbuffer/backends/win32.rs:308-313`).
4. `renderer/sw.rs:152-169`: the damage is `BitBlt`ed to the window and then `ValidateRect` is called
   (`win32.rs:176-204`) **only if the damage is not empty**. With nothing dirty, nothing is presented and the
   window is not validated.
5. winit handles `WM_PAINT` by sending `RedrawRequested` and then calling `DefWindowProcW`
   (`winit/platform_impl/windows/event_loop.rs:1276-1297`). For a window that is still invalid,
   `DefWindowProcW` runs `BeginPaint`, which sends `WM_ERASEBKGND`; winit leaves that to `DefWindowProcW`,
   which fills the update region with the class brush. Since O52 that brush is the theme colour
   (`snipdeck-platform/src/windows/erase.rs`), drawn by GDI with **alpha 0**.
6. Slint's own `draw()` (`winitwindowadapter.rs:841-866`) does not widen the damage when the redraw came
   from Windows. The renderer forgets its kept frame only on `occluded()` (`sw.rs:176-180`).
7. The app's existing remedy, `RepaintNudge` (`snipdeck-ui/ui/components/repaint.slint`, `repaint.rs`),
   damages the whole window on focus gained, un-occluded, restore from minimise, DPI and theme change. It is
   wired **only into the Panel and the Main window** (`main.rs:662-668`, `main_binding.rs:801`). The Panel is
   never activated (ADR-0003), so its `Focused(true)` trigger never fires. About (HelpWindow), Edit
   (ItemEditor), Tags (TagManager), Settings and the other child windows have no nudge at all.

## Hypotheses, ranked

### H1 (most likely): an area Windows invalidates gets no frame, and the erase makes it transparent

When Windows itself invalidates part of a window (the part was off screen and is dragged back; the part was
never visible because the window opened partly off screen; DWM dropped the surface), it sends `WM_PAINT` with
an update region. Nothing in Slint's scene changed, so the partial renderer has an empty dirty region, step 4
presents nothing and does not validate, and step 5 erases the region with the brush at alpha 0, which DWM
shows as see-through. The patch stays until something inside it changes (hover, click), which matches all
three reports:

- (c) Tags dragged back: the part that was off screen was never `BitBlt`ed (GDI clips to the visible region)
  and is invalidated on the way back. Exactly the "stays transparent where it was off screen" report.
- (b) Panel drag "column on the right goes transparent for a while": a column along one edge is the shape of
  an area uncovered by a move (off the screen edge, or under a window that was on top), and "then back to
  normal" is the next real change (the Panel's hover/list redraw, or the `frame_moved` clamp resizing it).
- (a) About/Edit: these are created, shown and then re-fitted by `fit_when_settled` (120 ms later,
  `native_window.rs:261-271`). If the first frame was drawn before the window was mapped (erase.rs notes that
  frame is thrown away) or into a size that is then changed by the fit while age stays 1, the visible window
  has invalid regions with nothing dirty: brush-erased, see-through, "a grey transparent box" where the brush
  or a blurred backdrop shows, and "a couple header elements showed" as hover redraws them one by one. A click
  redraws enough to look normal. The Panel next to it can be uncovered the same way when the child window
  moves or closes.

Evidence in code: steps 3-7 above. The repo already met the full-window form of this bug (O39) and fixed it
only for named events on two windows.

### H2: the class brush erase itself (O52) is what shows, opaque but wrong

If DWM honours GDI's alpha 0 as opaque on this PC (blur-behind ignored), the erased area shows the theme
colour instead of the content: a flat "grey box". Same root cause as H1 (no frame for an OS-invalidated area),
different look. The diagnostic tells it apart only by the owner's description; the fix is the same.

### H3: the softbuffer surface is resized without a full redraw

`fit_when_settled` and the Panel's `frame_changed` clamp change the size. softbuffer recreates its DIB on
resize, the new buffer has `presented = false`, age 0, so Slint should draw it all (`NewBuffer`). If a resize
lands between `buffer_mut` and present, or winit coalesces `WM_SIZE` with a paint, a partly drawn buffer could
be presented. Less likely: the code paths are consistent. The log shows `resized` followed by a `WM_PAINT`
with an update region and a `WM_ERASEBKGND`, which would point here.

### H4: DWM blur-behind / backdrop changes when the window is activated or deactivated

The legacy blur-behind path (step 2) can repaint differently on activation, and a GPU driver can show
blur-behind regions grey. That would show as glitches on `WM_ACTIVATE` with no `WM_PAINT` update region at
all. Lowest: there is no backdrop attribute set, and the reports follow moves and newly shown windows, not
activation.

### Ruled out by the code

- **Layered windows**: no `WS_EX_LAYERED` anywhere (`window_style.rs` sets only NOACTIVATE / TOOLWINDOW /
  APPWINDOW / TOPMOST). The log prints `layered=` per window to confirm.
- **Mica**: not set (no `DWMWA_SYSTEMBACKDROP_TYPE` call). The log prints `backdrop_type=` to confirm.

## The diagnostic

`tools/windows-test/diag-render.ps1` (one run, two passes, one file on the Desktop). The app side is
`SNIPDECK_DIAG` (`crates/snipdeck-app/src/diag.rs`, `crates/snipdeck-platform/src/windows/paint_diag.rs`):

- `render`: thread hooks on the UI thread log, for every Snipdeck window, `WM_PAINT` **with an update region
  made by Windows** (`GetUpdateRect` before winit sees it), `WM_ERASEBKGND`, `WM_WINDOWPOSCHANGED`, `WM_SIZE`,
  the modal move loop, DPI/display/composition changes, show and activate; each window's styles, `layered`,
  DPI, rectangles, DWM composition, backdrop type, dark mode, corners and cloaking on first sight; each
  move/resize with "partly OFF SCREEN" against the virtual screen; and which redraws follow an
  OS-invalidated area.
- `fix`: the same, plus such a redraw damages the whole window first (`snipdeck_ui::damage::whole_window`,
  Slint's own `mark_dirty_region`, the hook it uses when a popup closes; test
  `whole_window_damage_repaints_all_of_a_child_window_that_has_no_flip`).
- Pass A runs `render`, pass B `render,fix`, with the same steps; the owner answers y/n per step and the
  answers are merged into the log by time (approximate: the script's clock starts at launch, the app's at its
  first log line).
- Every hotkey register/unregister is logged with its result (`InUse` is `ERROR_HOTKEY_ALREADY_REGISTERED`,
  `Os(code)` carries `GetLastError`), plus the configured chords and the plan at start.

### Reading the result

| Log shows | Owner says | Meaning | Fix |
|---|---|---|---|
| `WM_PAINT ... os_update=` then `WM_ERASEBKGND` for the same hwnd, then `fix off` line | A: glitch y, B: glitch n | **H1 confirmed**, and the candidate fix works | Make the fix permanent: on a `RedrawRequested` that follows an OS update region, call `damage::whole_window` for every window (move the hook out of `diag` into `repaint.rs`; keep the `paint_diag` `WM_PAINT` probe as the trigger, without logging). Also answer `WM_ERASEBKGND` with 1 for our windows once the full frame is guaranteed, so nothing is ever brush-filled at alpha 0. |
| as above | A: y, B: still y | H1 is real but whole-window damage is not enough: the frame is drawn but not where Windows wants it | Present the full buffer on such a paint (skip the partial path): force `RepaintBufferType::NewBuffer` via a resize-to-same trick or upstream a Slint change so `WM_PAINT` with an update region always presents the kept buffer's rectangles; report upstream (Slint winit sw renderer) |
| `os_update` paints with **no** `WM_ERASEBKGND` after them, glitch y | | The frame was presented; the transparency is in the pixels (alpha) | H4/H2: set the window opaque (patch Slint's `with_transparent(true)` via the attributes hook: `attributes.with_transparent(false)` in `select_backend`) and retest |
| `resized` lines interleaved with `os_update` + erase right after `fit_when_settled` (about 120 ms after show) | glitch on About/Edit only | H3 | Size child windows before `show()` (`size_before_show`) instead of the settle re-fit, or damage the whole window after the fit |
| No `os_update` lines at all during the glitch, but `WM_ACTIVATE` | glitch y | H4 | Opaque windows as above; test with transparency effects off (the header records `EnableTransparency`) |
| `layered=true` or `backdrop_type` not 0/1 | | Something else set a style | Find who sets it (log shows when it first appears) |

## Shortcuts (problem d)

Facts from the code:

- **Ctrl+Shift+2 (Capture) is off by default** (`settings.rs:205`, empty `default_chord`; U19 in
  `docs/reviews/fable-analysis.md`). It does nothing unless the owner set it in Settings. Not a bug, but the
  Settings row for an empty shortcut gives no hint; the diag logs the chord in force at start.
- **Ctrl+Alt+V (Paste next) is never registered on the Free plan** (`settings.rs:400-410`, a recorded
  decision: Ctrl+Alt+V is Paste Special in Word and Excel, so a Free user would lose it for an upsell). The
  wizard's library (`spikes-results\m3-data`) is Free unless a licence was entered. So the key press never
  reaches Snipdeck and nothing happens, by design, with no word in the UI. **Fixed here:** the Settings row
  for Paste next on Free now says "Paste next is a Supporter feature, so this shortcut does nothing on the Free
  plan." A toast on the key press itself is not possible without registering the chord, which the decision
  forbids; the Panel's Start stack and the Paste next action now toast "Paste next and the paste stack are
  Supporter features." instead of the generic "This item uses a Supporter feature."
- **Ctrl+Alt+Space stopped mid-session**: the stale "Panel visible" state (merged fix from
  `claude/owner-test-ctl`) made the press hide an already hidden Panel. Other ways it can stop mid-session:
  a re-registration that failed (pause/resume, a plan change after a licence key: that one was **swallowed**,
  `settings_binding.rs` `apply_hotkeys_for_tier` ignored the result; **fixed here**, it is reported now), or
  another program taking the chord while Snipdeck had it released. The log shows every register/unregister
  with its result and every `hotkey <action>` press, so a press that arrives but does nothing is told apart
  from one that never arrives.
- **Only the first failure was shown** (`toast_ctl.rs` `report_hotkey_problems` took `problems.first()`), and
  the Settings rows only showed errors from rebinding in Settings, never the start-up ones. **Fixed here:**
  one failure keeps its specific toast; several are named together ("These shortcuts are not working:
  Ctrl+Alt+Space, Ctrl+Alt+V. Settings > Shortcuts says why."); the controller keeps every problem and each
  Settings row shows why its chord failed until a working chord is recorded or the next registration
  succeeds.

Unverified: whether another program on the owner's PC owns Ctrl+Alt+Space (the script lists well-known
shortcut-using programs that are running, by name only from a fixed list, and the log shows `InUse` if so).
