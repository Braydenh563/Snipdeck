# Spike S4: Mica chrome, frameless DWM window, theme / DPI / text scale in Slint

> Code: [`spikes/s4-mica/`](../../spikes/s4-mica/) · Slint 1.18.1 (winit 0.30.13 backend) · `windows` 0.62.2 · Rust 1.97
> Inputs: DESIGN.md §1 (signature detail 5 "System material"), §2 `bg.chrome`, §5 Panel, §6 Depth; SPEC.md §5.1, §13.

## Status

**Built and compiled. Not run on Windows.** Every Windows behaviour below comes from reading the
Slint/winit source, Microsoft Learn and the Slint issue tracker. None of it has been seen on screen. Treat it as
UNVERIFIED until the [manual test script](#manual-test-script-real-windows) passes. The only thing that ran was
the non-Windows stub (solid fallback chrome) under Xvfb on Linux.

## Recommendation: GO WITH CHANGES

Slint can do what DESIGN.md asks without patching Slint or winit. On Windows, Slint already creates every winit
window as transparent, so per-pixel alpha reaches DWM. winit 0.30 exposes the system backdrop, corner preference
and an "undecorated shadow" frameless mode as creation-time attributes. The rest (dark mode, border colour, frame
extension) is `DwmSetWindowAttribute` on the HWND, which we get through `raw-window-handle`.

The changes needed:

1. **Renderer choice matters more than the DWM calls.** Use the **software renderer** (`renderer-software`) or
   FemtoVG/OpenGL for any window with a transparent chrome. **Do not use Skia on Windows**: its D3D and wgpu-DX12
   surfaces render transparent pixels black ([#4120], [#13572], both open). FemtoVG/OpenGL is reported to turn
   black on NVIDIA unless "OpenGL GDI compatibility" is set ([mica template]). Test steps 1–2 decide between
   software and FemtoVG. If only the software renderer is clean, use it for the panel and picker. They are small
   windows, and the software renderer is also the lightest on RAM (SPEC §9).
2. **The panel probably can't show "live" Mica.** Mica falls back to a flat neutral colour while its window is
   inactive ([Mica Win32]). The panel is `WS_EX_NOACTIVATE` (ADR-0003), so it is never active. Unless the
   experimental "keep active look" trick works (test step 5), the panel's chrome will be flat system grey and the
   wallpaper tint will never show. That still meets "system material" in a technical sense, but the signature
   effect is lost on the surface users see most. Decide after step 5: either accept flat Mica on the panel and keep
   live Mica on the main window and sidebar (they activate normally), or give the panel the solid fallback chrome.
3. **Mica needs Windows 11 22H2 (build 22621)**, not just "Windows 11". `DWMWA_SYSTEMBACKDROP_TYPE` is documented
   from 22621 ([DWM_SYSTEMBACKDROP_TYPE]). Build 22000 gets rounded corners and the border colour, but solid chrome.
   (Only the undocumented attribute 1029 would give it Mica, and we don't use it.)
4. **Use the fallback logic as written.** It's already implemented and unit-tested: solid `bg.app` chrome on
   Windows 10, Windows 11 before 22H2, Transparency effects off, high contrast, or a failed backdrop call. It costs
   nothing extra because those users need it anyway.

**NO-GO trigger:** if both the software renderer and FemtoVG show black or opaque chrome on stock drivers (steps 1–2),
drop Mica and ship the [fallback](#fallback-if-mica--slint-transparency-is-unreliable) everywhere.

## What was built

| Piece | Where | What it does |
|---|---|---|
| Token generation | `build.rs` | Reads `design/tokens/palette.json` and writes `tokens.slint` into `OUT_DIR`, with `Primitives` (all primitives) and `Palette` (semantic tokens resolved through `semantic_refs`, switched by `Palette.dark`). It fails the build on a missing dark mapping or unknown primitive. The `.slint` and `.rs` sources contain no colour hex literals. |
| Panel layout | `ui/panel.slint` | DESIGN.md §5 Panel: 40px header, 32px tab bar, 32px target footer (all **transparent** while Mica is active, `bg.app` otherwise), hairline `border.subtle` dividers, an opaque `bg.app` card zone with ItemCards (number gutter, 8px radius, `bg.surface`, `border.subtle`, depth-1 shadow in light and none in dark), and list-colour tab underlines (600 light / 400 dark). The type scale is multiplied by `SystemLook.text-scale`, and the bars use `max(spec height, text height + padding)` so they grow with text size. High contrast swaps every colour for system colours. A Slint-drawn 1px `border.default` appears when DWM can't draw one. Edge `TouchArea`s call winit `drag_resize_window`, and the header calls `drag_window`. A diagnostics card shows what the platform layer decided. |
| Policy (OS-free) | `src/appearance.rs` | Holds `SystemAppearance` and `choose_material()` (the fallback rules), the `S4_*` env-var config, the text-scale clamp (1.0–2.25) and the COLORREF conversion. **8 unit tests.** |
| Windows layer | `src/platform/windows.rs` | Creation attributes via `BackendSelector::with_winit_window_attributes_hook`: `with_system_backdrop`, `with_corner_preference(Round)`, `with_undecorated_shadow`. Gets the HWND via `slint::Window::window_handle()` → `RawWindowHandle::Win32`. `DwmSetWindowAttribute` sets `USE_IMMERSIVE_DARK_MODE`, `WINDOW_CORNER_PREFERENCE`, `BORDER_COLOR` (from the `border.default` token) and `SYSTEMBACKDROP_TYPE`, plus `DwmExtendFrameIntoClientArea`. Appearance is read from the registry (`AppsUseLightTheme`, `EnableTransparency`, `TextScaleFactor`), WinRT `UISettings` (`AdvancedEffectsEnabled`, `TextScaleFactor`), `SPI_GETHIGHCONTRAST`, `GetSysColor` and `RtlGetVersion`. Change detection uses a `SetWindowSubclass` that watches `WM_SETTINGCHANGE` / `WM_THEMECHANGED` / `WM_SYSCOLORCHANGE` / `WM_DWMCOMPOSITIONCHANGED` / `WM_DWMCOLORIZATIONCOLORCHANGED` / `WM_DPICHANGED`, plus the `UISettings` change events and winit `ThemeChanged` / `ScaleFactorChanged`. All of them only queue a coalesced refresh with `slint::invoke_from_event_loop`, so nothing re-enters Slint inside a window procedure or on a WinRT thread-pool thread. |
| Linux stub | `src/platform/stub.rs` | No DWM. Runs with the solid fallback. `S4_PREVIEW_DARK=1` and `S4_PREVIEW_TEXT_SCALE=2.25` preview the tokens and layout. |

**Test knobs** (environment variables, no rebuild): `SLINT_BACKEND=winit-femtovg|winit-software`,
`S4_BACKDROP=mica|mica-alt|acrylic|none`, `S4_EXTEND=full|1px|none`, `S4_SHADOW=1|0`, `S4_NOACTIVATE=1`,
`S4_KEEP_ACTIVE_LOOK=1`, `S4_FORCE_SOLID=1`. The defaults are Mica, `full`, shadow on, femtovg.

## Verification actually performed (Linux host)

| Command | Result |
|---|---|
| `rustup target add x86_64-pc-windows-gnu` | ✅ installed |
| `cargo check` (Linux) | ✅ |
| `cargo check --target x86_64-pc-windows-gnu` | ✅ (compiles `platform/windows.rs`) |
| `cargo fmt --check` | ✅ (checked that it covers the `cfg_attr(path)` Windows module) |
| `cargo clippy --all-targets -- -D warnings` (Linux) | ✅ |
| `cargo clippy --target x86_64-pc-windows-gnu -- -D warnings` | ✅ |
| `cargo test` | ✅ 8 passed (fallback policy, config parsing, text-scale clamp, COLORREF) |
| `cargo build --release --target x86_64-pc-windows-gnu` (mingw-w64) | ✅ links. Imports include `dwmapi!DwmSetWindowAttribute / DwmExtendFrameIntoClientArea / DwmEnableBlurBehindWindow`, `comctl32!SetWindowSubclass`, `ntdll!RtlGetVersion`, `advapi32!RegGetValueW`, `RoGetActivationFactory`, `opengl32` |
| Linux stub under Xvfb, `winit-software` | ✅ renders the panel. Light at 100% looks as designed. Dark at 175% text: the tokens switch and the bars grow, but **the tab bar overflows**, so ListTabBar needs the scrolling DESIGN.md §4 already calls for. |

**Not verified:** anything involving DWM, Mica, corners, shadow, DPI changes, text-size change notifications or
high contrast. That needs the Windows test script.

## Findings

### Slint: what the transparent-window path actually is (from the 1.18.1 source)

- **Slint makes every winit window transparent on Windows.** `WinitWindowAdapter::window_attributes()` starts from
  `WindowAttributes::default().with_transparent(true)` (`i-slint-backend-winit/winitwindowadapter.rs:811`). The
  only exception is when a muda menubar is present (#8793 workaround, line ~612). `wants_transparent()` (background
  not opaque, or `no-frame`) is used **only on macOS**. So on Windows, `background: transparent` plus painting is
  all we need. No extra flag is required.
- **winit turns `transparent` into `DwmEnableBlurBehindWindow` with an empty region**
  (`winit/platform_impl/windows/window.rs:1232`). This makes DWM honour the renderer's alpha channel. It's the
  legacy (Vista-era) path, but it is what lets OpenGL and GDI contents carry alpha.
- **FemtoVG/OpenGL** asks glutin for a config with transparency and falls back to an opaque config if none exists
  (`renderer/femtovg/glcontext.rs:67-84`). A driver without an alpha-capable WGL pixel format therefore gives
  **opaque black** chrome silently. That's the NVIDIA case ([mica template]).
- **The software renderer** writes premultiplied ARGB into softbuffer (`renderer/sw.rs:32-50`), and softbuffer
  presents it with GDI `BitBlt` (`softbuffer/backends/win32.rs:189`). With blur-behind enabled, DWM should use
  that alpha. The reporter of [#8496] confirmed that "transparency works with every backend except D3D".
- **Skia on Windows:** transparent renders black with Direct3D ([#4120], open since 2023, workaround
  `skia-opengl`) and with wgpu/DX12 ([#13572], open, filed against 1.18.1; the root cause is
  `DXGI_ALPHA_MODE_IGNORE`). **Avoid Skia for these windows.**
- **There is no Slint API for backdrop, corners or extended styles.** Window effects are an open feature request
  ([#4121]). A maintainer's answer for rounded corners on a frameless window is "no feature like that in Slint
  itself; use winit or the native handle" ([discussion #9856]). The supported routes are
  `BackendSelector::with_winit_window_attributes_hook` (`unstable-winit-030`, applied in `create_window_adapter`
  before creation, `lib.rs:897`) and `slint::Window::window_handle()` (`raw-window-handle-06`).
- **The HWND only exists once the event loop runs.** Wait for `WinitWindowAccessor::winit_window().await` inside
  `slint::spawn_local`, the same as S1.
- **`no-frame: true` maps to winit `set_decorations(false)`** on every property update (`winitwindowadapter.rs:1882`).
  winit then keeps `WS_CAPTION | WS_SIZEBOX` and removes the non-client area in `WM_NCCALCSIZE`.
  `with_undecorated_shadow(true)` leaves a 1px strip so DWM keeps the shadow. winit's own comment admits this
  "leads to a small black 1px border on the top" (`event_loop.rs:1182`). Test step 11 compares it with
  `S4_SHADOW=0` + frame extension.
- **Frameless windows have no resize border** (the client area covers everything), so resizing needs
  `drag_resize_window` from edge hit areas, as implemented.

### DWM attributes and minimum builds (Microsoft Learn)

| Attribute / API | Min build | Use here |
|---|---|---|
| `DWMWA_USE_IMMERSIVE_DARK_MODE` (20) | 22000 (documented) | **Mica's tint follows this flag**, not the system theme. Without it a dark-mode user gets light Mica under dark cards. Set it on every theme change. |
| `DWMWA_WINDOW_CORNER_PREFERENCE` = `DWMWCP_ROUND` | 22000 | Opt-in rounding. Per MS, frameless windows with an "empty non-client area" aren't rounded by policy but *can* be with this hint. Windows using per-pixel-alpha **layering** (`WS_EX_LAYERED`) or window regions **can never be rounded** ([rounded corners]). That is why we must not use the `WS_EX_LAYERED` route some users tried ([#7710]). Corners are never rounded when maximized or snapped, in VMs or AVD. |
| `DWMWA_BORDER_COLOR` | 22000 | The depth-3 "1px `border.default`" drawn by DWM so it follows the rounded corner. In high contrast it resets to `DWMWA_COLOR_DEFAULT`. |
| `DWMWA_SYSTEMBACKDROP_TYPE` = `DWMSBT_MAINWINDOW` (2) | **22621** | Mica "behind the entire window bounds". `TABBEDWINDOW` (Mica Alt) and `TRANSIENTWINDOW` (Desktop Acrylic) are available for comparison ([DWM_SYSTEMBACKDROP_TYPE]). |
| `DwmExtendFrameIntoClientArea` (margins -1) | Vista | Lets the system-drawn material show where the client paints alpha 0 ([custom frame]). Also brings the DWM shadow back for a frameless window. |
| `DWMWA_REDIRECTIONBITMAP_ALPHA` | 26100 | A newer way to give a GDI redirection bitmap alpha. Not used, because the GL and blur-behind path already carries alpha. Note it for later. |

Mica guidance: set every layer above the backdrop to transparent where Mica should show, apply the backdrop
once, and expect a fallback to a neutral colour when the window is inactive ([Mica design], [Mica Win32]).

### Theme, high contrast, transparency, text size, DPI

- **winit already follows the system theme on Windows.** On `WM_SETTINGCHANGE` it re-queries and emits
  `ThemeChanged` (`event_loop.rs:2423`), and Slint maps that to `Palette.color-scheme`
  (`winitwindowadapter.rs:1556`). Two caveats. First, winit uses the *undocumented*
  `ShouldAppsUseDarkMode` / `SetWindowCompositionAttribute(WCA_USEDARKMODECOLORS)` instead of
  `DWMWA_USE_IMMERSIVE_DARK_MODE` (`dark_mode.rs`), so we still set the DWM flag ourselves. Second, winit reports
  Light whenever high contrast is on. **Don't pin a theme with `with_theme`**: that disables `ThemeChanged`. The
  spike treats the registry value `AppsUseLightTheme` as the source of truth, as SPEC asks. The documented
  alternative is `UISettings.GetColorValue(Foreground)` + `ColorValuesChanged` ([Win32 themes]), which the spike
  also listens to.
- **Transparency effects:** use `UISettings.AdvancedEffectsEnabled` + `AdvancedEffectsEnabledChanged`
  (documented, 15063+) ([tailoring effects]), with `EnableTransparency` from the registry as a fallback. With it off,
  we switch to `DWMSBT_NONE` and paint `bg.app`. Otherwise DWM would draw its own grey fallback, which isn't our
  token.
- **High contrast:** `SPI_GETHIGHCONTRAST`. Colours come from `GetSysColor` (`COLOR_WINDOW`, `COLOR_WINDOWTEXT`,
  `COLOR_HIGHLIGHT`, `COLOR_HIGHLIGHTTEXT`, `COLOR_GRAYTEXT`). Every element keeps a 1px border (DESIGN §8).
- **Text size** (Settings › Accessibility › Text size, 100–225%): Microsoft states that "DirectWrite, GDI, and XAML
  SwapChainPanels do not natively support text scaling" ([text scaling]). The app has to read
  `UISettings.TextScaleFactor` (range 1–2.25) and listen to `TextScaleFactorChanged`. The spike does that, with
  `HKCU\Software\Microsoft\Accessibility\TextScaleFactor` as a fallback, and multiplies the DESIGN type scale.
  Slint 1.18 separately reads `NONCLIENTMETRICS.lfMessageFont` **once** at start-up as the default font size
  (`lib.rs:863`). Text without an explicit size would therefore *not* follow later changes, and could be scaled
  twice if the message font already includes text scale. **Rule for the real UI: every `Text` uses a type token.**
  Whether `WM_SETTINGCHANGE` also fires for text size is unknown; the diagnostics card shows which signal arrived
  (step 10).
- **DPI:** winit sets Per-Monitor v2 at runtime (`dpi.rs`), and Slint handles `ScaleFactorChanged`. Microsoft
  recommends declaring `<dpiAwareness>PerMonitorV2</dpiAwareness>` in the **application manifest** rather than
  calling the API ([DPI awareness]). The real app should embed a manifest. It also helps `SetWindowSubclass`
  (comctl32 v6), which S1 flagged.

## Risks

1. **Black or opaque chrome from the GL driver** (NVIDIA + OpenGL). Mitigation: the software renderer. Detected by
   steps 1, 2 and 16.
2. **Skia regressions.** If a later milestone switches to Skia for text quality, transparency breaks on Windows
   ([#4120], [#13572]). Keep a check in CI screenshots, or pin the renderer per window.
3. **Inactive Mica on the non-activating panel** (Recommendation 2). `S4_KEEP_ACTIVE_LOOK` calls
   `DefWindowProcW(WM_NCACTIVATE, TRUE)` behind winit's back. It's an experiment: DWM may ignore it, and it may
   confuse accessibility tools about which window is active.
4. **winit rewrites `GWL_EXSTYLE`** on Slint property updates (found in S1). `S4_NOACTIVATE` here sets the styles
   once without S1's `WM_STYLECHANGING` guard, which is good enough to observe Mica but not for the product. Reuse
   S1's guard.
5. **First-frame flash.** Backdrop, corners and shadow are set before creation. Dark mode, border colour and frame
   extension can only be set once the HWND exists, possibly after the first present. A dark-mode user may see one
   frame of light Mica or a light border (step 12). Mitigation if seen: show the window from the same
   `spawn_local` after applying, or keep it cloaked (`DWMWA_CLOAK`) until the first refresh.
6. **Live-resize artefacts.** With GL and blur-behind, DWM can compose a resized frame before the renderer catches
   up, so a transparent or black strip may appear. winit's comment calls this "janky resize behavior" (step 13).
7. **Text contrast on Mica isn't guaranteed by `palette.py`.** Mica's tint depends on the wallpaper. The chrome
   uses only `text.primary` and `text.secondary`, which should hold on Mica's light and dark base tones, but it
   needs an eyeball check with a saturated wallpaper (step 14).
8. **The `unstable-winit-030` API** may change in any Slint minor release. Pin Slint exactly, and keep all of this
   inside `snipdeck-platform` / `snipdeck-ui`.
9. **Rounding of `WS_EX_TOOLWINDOW` windows** under the corner-preference hint is undocumented (step 5).
10. **The 1px top line** from winit's undecorated-shadow hack (step 11).
11. **Software renderer cost on big windows.** That's fine for the panel. The main window at 4K is unmeasured and
    belongs in the perf smoke (SPEC §12).

## Fallback if Mica + Slint transparency is unreliable

Use **solid chrome**, which is already implemented: `S4_FORCE_SOLID=1`, or automatic via `choose_material`.

- The header, tab bar and footer are painted `bg.app` (light `neutral-25`, dark `neutral-900`), exactly DESIGN.md's
  documented `bg.chrome` fallback. The `border.subtle` hairlines above and below the card zone keep the chrome and
  content reading as separate layers. DWM backdrop is `DWMSBT_NONE`. Corners, shadow and the DWM border colour stay
  on Windows 11.
- Everything is opaque, so the result doesn't depend on the renderer or GPU driver. Text contrast is the one
  `palette.py` already verifies.
- **Optional refinement (not implemented; needs a `palette.py` change and its contrast check):** a dedicated
  `bg.chrome` token one step away from `bg.app` (light `neutral-50`, dark `neutral-850`). It would approximate Mica's
  "base layer under a content layer" look from the Mica layering guidance. It's worth trying only if the hairline
  separation reads too flat in review.

## Manual test script (real Windows)

**Setup:** an MSVC Rust toolchain. Run `cd spikes\s4-mica` and `cargo run --release`. In PowerShell, set knobs
first, e.g. `$env:SLINT_BACKEND="winit-software"`, and clear them with `Remove-Item Env:S4_*`. For each step, record
the diagnostics card (bottom of the card list; the same line is printed to the terminal as `[s4] …`), take a
screenshot, and note anything unexpected. Note the GPU vendor.

### Windows 11 22H2 or later (build ≥ 22621)

| # | Steps | Expected |
|---|---|---|
| 1 | Default launch (`winit-femtovg`, light mode, Transparency effects on), over a colourful wallpaper. Keep the window active. | Header, tab bar and footer show **Mica** (subtle wallpaper tint), and the card zone is opaque warm off-white. **8px rounded corners**, the DWM shadow, and a 1px border in `border.default` grey. Diagnostics show `chrome: Backdrop(Mica)` and `DWM: dark=ok corners=ok border=ok extend=Full:ok backdrop=2:ok`. **Failure modes to record:** black chrome (GL alpha lost), opaque grey or white chrome (backdrop not visible through client), a 1px dark line along the top edge. |
| 2 | Repeat 1 with `SLINT_BACKEND=winit-software`. | Same as 1. This decides the renderer (Recommendation 1). |
| 3 | Drag the window by the header across the wallpaper. Resize from the left edge, right edge, bottom edge and bottom-right corner. | The tint updates with position, and dragging and resizing work. Width stays within 260–480. |
| 4 | Click another app so the panel becomes inactive, then click back. | Inactive: the chrome goes **flat neutral** (expected Mica behaviour). Active again: the tint returns. |
| 5 | `S4_NOACTIVATE=1`, launch, click inside the panel. Then add `S4_KEEP_ACTIVE_LOOK=1`. | Without the hack: record whether the chrome is ever tinted (expected: flat, since the window never activates). With the hack: record whether the tint appears, and whether corners and shadow survive with `WS_EX_TOOLWINDOW`. This decides Recommendation 2. |
| 6 | While running: Settings › Personalization › Colors › Choose your mode → **Dark**, then back to Light. | Within about 1 s, the card zone turns near-black, cards turn dark, text turns light and **Mica turns dark** (`dark=ok`). The diagnostics name the trigger (`WM_SETTINGCHANGE`, `winit ThemeChanged` or `UISettings.ColorValuesChanged`), and `theme dark`. Reverses cleanly. |
| 7 | Settings › Personalization › Colors › **Transparency effects** off, then on. | Off: the chrome becomes solid and matches the card zone, `chrome: Solid(TransparencyOff)`, `backdrop=1:ok`, hairlines still separate the zones. On: Mica returns. |
| 8 | Turn on a contrast theme (Settings › Accessibility › Contrast themes › e.g. Night sky), then turn it off. | On: every surface uses the theme's window colour, text uses its text colour, 1px borders in the text colour on cards and around the window, solid chrome, `chrome: Solid(HighContrast)`. Off: the normal theme returns. |
| 9 | With two monitors at different scales (e.g. 100% and 150%), drag the window between them. Also change Settings › Display › Scale while it's running. | The window keeps its logical size (320 logical px wide by default), text stays sharp, `DPI scale` updates in the diagnostics, and corners and shadow persist. |
| 10 | Settings › Accessibility › **Text size** → 150% → Apply. Then 225%, then back to 100%. | Text grows and the header, tabs and footer grow with it. `text scale 1.50` / `2.25`. **Record which signal triggered the refresh.** Known: at ≥ 175% the tab bar overflows (the real ListTabBar must scroll). Titles may elide; DESIGN asks that titles never truncate below 12 characters. |
| 11 | Matrix, one launch each: `S4_EXTEND=1px`; `S4_EXTEND=none`; `S4_SHADOW=0`; `S4_SHADOW=0` with `S4_EXTEND=full`; `S4_BACKDROP=mica-alt`; `S4_BACKDROP=acrylic`. | Record for each: is Mica visible in the chrome? Is there a shadow? Rounded corners? A top 1px line? Expected: `full` is needed for Mica in the client area. `1px` or `none` give opaque chrome. Shadow comes from either `S4_SHADOW=1` or frame extension. |
| 12 | Dark mode on. Launch five times, watching the first frame. | No white or light flash before the dark UI appears. Record any flash (Risk 5). |
| 13 | Resize quickly from the right edge back and forth. | Record any black or transparent strips during resize (Risk 6). Compare the two renderers. |
| 14 | Use a saturated dark wallpaper in light mode, then a saturated light wallpaper in dark mode. Zoom the footer with Magnifier (Win+Plus). | The footer text is readable, with no coloured fringes or halos around glyphs on the transparent chrome (Risk 7). |
| 15 | (Laptop) Turn on Energy saver / Battery saver. | Record whether Mica goes flat and whether `transparency` in the diagnostics changes. |
| 16 | (NVIDIA only) If step 1 showed black chrome: NVIDIA Control Panel › Manage 3D settings › OpenGL GDI compatibility → *Prefer compatibility*, then repeat 1. | Record whether that fixes FemtoVG. That's informational only: we can't ask users to change it. |
| 17 | `S4_FORCE_SOLID=1`. | This is the fallback look: `Solid(ForcedByConfig)`, `backdrop=1:ok`. Corners, shadow and border stay. Compare it with DESIGN.md. |

### Windows 10 (22H2 recommended; also 1903+ if available)

| # | Steps | Expected |
|---|---|---|
| W1 | Default launch, then `winit-software`. | Solid `bg.app` chrome, `chrome: Solid(UnsupportedOs)`, **square corners**, a DWM shadow, and a **Slint-drawn** 1px `border.default`. No black areas: everything is opaque in this mode. The `dark=` entry may show an error code on older builds, which is expected and harmless here. |
| W2 | Drag and resize as in step 3. | Works. |
| W3 | Settings › Personalization › Colors › default app mode Dark/Light while running. | Tokens switch within about 1 s and the diagnostics show the trigger. |
| W4 | Settings › Ease of Access › Display › Make text bigger → 150%. | Same as step 10. Record which signal arrived. |
| W5 | High contrast on/off (Left Alt+Left Shift+Print Screen). | Same as step 8. |
| W6 | Two monitors at different scales, as in step 9. | Same as step 9. |
| W7 | Check for an "entry point not found" dialog at start-up (comctl32 `SetWindowSubclass` without a v6 manifest; see S1). | None. |

**GO** if steps 1 **or** 2 show Mica with an opaque card zone on the owner's machine (and on an NVIDIA machine, if
one is available, with the chosen renderer), and steps 6–10 and W1–W6 pass. Steps 5 and 11–15 shape the details
(panel material, flags), not the go/no-go. **NO-GO for Mica** (ship the fallback) if steps 1 and 2 both fail.

## Sources

- Slint issues: [#4121] window effects (open) · [#4120] Skia D3D transparent → black (open) · [#13572] Skia
  wgpu/DX12 transparent → black (open, 1.18.1) · [#8496] (duplicate of #13572; "works with every backend except
  D3D") · [#7710] user Mica via `WS_EX_LAYERED` + attribute 38 · [discussion #9856] rounded corners on frameless.
- Community: [mica template] (Slint + Mica; NVIDIA OpenGL black-transparency note) · `window-vibrancy` 0.8.1
  `apply_mica` (sets `DWMWA_USE_IMMERSIVE_DARK_MODE`, then `DWMSBT_MAINWINDOW` on ≥ 22523, else undocumented 1029
  on ≥ 22000).
- Microsoft Learn: [DWMWINDOWATTRIBUTE] · [DWM_SYSTEMBACKDROP_TYPE] · [rounded corners] · [Mica Win32] ·
  [Mica design] · [custom frame] · [Win32 themes] · [tailoring effects] · [text scaling] · [UISettings.TextScaleFactor] ·
  [DPI awareness].
- Source read (crates.io registry): `i-slint-backend-winit` 1.18.1 (`winitwindowadapter.rs`, `lib.rs`,
  `renderer/femtovg/glcontext.rs`, `renderer/sw.rs`), `winit` 0.30.13 (`platform/windows.rs`,
  `platform_impl/windows/{window.rs,event_loop.rs,window_state.rs,dark_mode.rs,dpi.rs}`), `softbuffer` 0.4.8.

[#4121]: https://github.com/slint-ui/slint/issues/4121
[#4120]: https://github.com/slint-ui/slint/issues/4120
[#13572]: https://github.com/slint-ui/slint/issues/13572
[#8496]: https://github.com/slint-ui/slint/issues/8496
[#7710]: https://github.com/slint-ui/slint/issues/7710
[discussion #9856]: https://github.com/slint-ui/slint/discussions/9856
[mica template]: https://github.com/Secrtect/slint-windows-mica-template
[DWMWINDOWATTRIBUTE]: https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute
[DWM_SYSTEMBACKDROP_TYPE]: https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwm_systembackdrop_type
[rounded corners]: https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-rounded-corners
[Mica Win32]: https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-mica-win32
[Mica design]: https://learn.microsoft.com/en-us/windows/apps/design/style/mica
[custom frame]: https://learn.microsoft.com/en-us/windows/win32/dwm/customframe
[Win32 themes]: https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-windows-themes
[tailoring effects]: https://learn.microsoft.com/en-us/windows/apps/develop/composition/composition-tailoring
[text scaling]: https://learn.microsoft.com/en-us/windows/apps/develop/input/text-scaling
[UISettings.TextScaleFactor]: https://learn.microsoft.com/en-us/uwp/api/windows.ui.viewmanagement.uisettings.textscalefactor
[DPI awareness]: https://learn.microsoft.com/en-us/windows/win32/hidpi/setting-the-default-dpi-awareness-for-a-process
