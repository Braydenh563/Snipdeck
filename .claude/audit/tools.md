# Deep audit: tools (colour tool, eyedropper, symbols and emoji picker)

Head: `0a3eacac` (tip of `origin/claude/compassionate-pascal-pod8ux`). Agent `tools`, model Fable.
Two probe tests were run one after the other (`crates/snipdeck-colour/tests/probe_tools.rs`, then
`crates/snipdeck-symbols/tests/probe_tools.rs`), both deleted; `git status` is clean. Static data checks
used python (fontTools 4.68 for the font coverage). The Slint UI was not driven (no harness run): every UI
claim below is from the `.slint` and binding code and is marked `code`.

## 1. Coverage

| Row | State | How |
|---|---|---|
| Colour model and conversions (`snipdeck-colour/src/model.rs`) | checked, driven | run (round trip over 172,961 colours per format) |
| Colour formats written (`format.rs`: hex, rgb, hsl, hsb, cmyk, oklch, name) | checked, driven | run |
| Colour parsing (`parse.rs`: lenient `parse`, strict `literal`) | checked, driven | run (57 inputs) |
| CSS names and nearest name (`names.rs`) | checked, driven | run (3 colours) + code |
| Contrast checker (`contrast.rs`, UI block in `colours.slint`) | checked, driven (maths); UI code only | run + code |
| Schemes and palettes (`scheme.rs`) | checked, driven | run (5 base colours x 6 schemes) |
| Picker square, wheel, sliders (`picker.rs`, `colours.slint`) | checked | code |
| Recent colours (`recents.rs`) | checked | code |
| Saved colours (`saved.rs`, `colour_ctl.rs`, `colours_panel.rs`) | checked | code |
| Colour controller and binding (`colour_ctl.rs`, `colour_main.rs`, `binding/colours.rs`, `colour_words.rs`) | checked | code |
| Colours tab UI (`colours.slint`, `components/swatch.slint`) | checked | code |
| `components/colour_block.slint` | not reached (time; only size checked) | - |
| Colours keyboard entry (`panel.slint` key handler, channel fields) | checked | code |
| Accent colours (`accent.rs`) | checked (contrast rules only) | code |
| Eyedropper portable session (`snipdeck-platform/src/eyedrop.rs`) | checked | code |
| Eyedropper Win32 (`windows/eyedrop.rs`, `windows/screen.rs`) | checked; not runnable here | code |
| Eyedropper controller (`eyedrop_ctl.rs`, `screen_clip_ctl.rs`) | checked | code |
| `screen_clip.rs` | not reached (time) | - |
| Symbols catalogue and data (`catalog.rs`, `data.rs`, `data/*.z`, `manifest.json`) | checked, driven | run (python counts + probe) |
| Symbols search and folding (`search.rs`, `fold.rs`) | checked, driven | run (100 queries, 7 languages) |
| Symbols recents (`recents.rs`) | checked | code |
| Emoji art (`art.rs`) | skimmed (layout, cache size) | code |
| Icons (`icon.rs`, export) | checked | code |
| Symbols controller, grid, navigation (`symbol_ctl.rs`, `symbols_panel.rs`, `symbol_nav.rs`, `symbols_binding.rs`) | checked (`symbols_binding.rs` by signature only) | code |
| Symbols tab UI (`symbols.slint`, `panel.slint` key handler) | checked | code |
| Kaomoji source (`data/symbols/kaomoji.tsv`) and bundled fonts | checked, driven | run (fontTools cmap) |
| Translations of tool strings (six `.po` files) | checked (no empty `msgstr` in 1311 entries per language) | run (python) |
| Generator scripts (`scripts/symbols.py`, `scripts/ui_icons.py`) | not reached (grepped only) | - |
| Test files of the three areas, `tests/colours_window.rs` | not reached (header only) | - |
| Standalone Colours or Symbols window | not reached: none exists (see TOOLS-58) | code (grep) |
| Rendering in the running app (glyphs, sizes, text scale, light and dark) | not reached (no harness run in the time box) | - |
| Windows behaviour (eyedropper, paste into target, font fallback) | not reached: needs Windows | - |

Counts: 33 rows; 26 checked (10 of them driven by a run), 7 not reached.

## 2. Findings

Severity: critical, high, medium, low. "run" = measured by a probe or a data script; "code" = read only.

### Colour: formats, parsing, maths

| Id | Sev | Lens | Evidence | Verified | Fix (whole class) | Effort |
|---|---|---|---|---|---|---|
| TOOLS-1 | medium | Broken | HSB, CMYK and Name text drop alpha with no warning (`format.rs`: no alpha branch for the three). Measured: alpha survives for 1 of 256 values; `#33669980` copies as `hsb(210, 67%, 60%)`. | run | One rule for every format: either write alpha (`hsba(...)`, `cmyk(... / a)`) or disable the row and say "no transparency in this format" when alpha < 100%. | S |
| TOOLS-2 | medium | Broken | Alpha is printed with two decimals in rgba, hsla and oklch. Measured: 101 of 256 alpha values round-trip; 155 change when pasted back. | run | Print the shortest decimal that parses back to the same byte (CSS serialisation rule: 2 decimals, else 3); test all 256. | S |
| TOOLS-3 | medium | Broken | HSL, HSB and CMYK text use whole degrees and percent. Measured exact round trip: HSL 18,352 of 172,961 (10.6%), HSB 21,138 (12.2%), CMYK 22,034 (12.7%). A colour copied as HSL and pasted back into the tool is a different colour 9 times in 10. `saved_text` falls back to hex for this reason but copy and paste do not. | run | Add one decimal where needed until the text parses back to the same RGB (as oklch already does), or show a "rounded" mark on rows that do not round-trip. | M |
| TOOLS-4 | low | Broken | OKLCH text does not round-trip for 155 of 172,961 sampled colours (0.09%). | run | Raise the printed precision for the failing cases (shortest text that round-trips), test over the full cube in CI as an ignored test. | S |
| TOOLS-5 | medium | Broken | Hue schemes rotate hue in OKLCH at fixed lightness and chroma, then clip. For light or dark saturated colours the result is nearly white or nearly black. Measured: yellow `#ffff00` complementary = `#f4f3ff`, triadic = `#e8f7ff`, `#fff0f6`; navy `#000028` complementary = `#0b0600`. | run | Gamut-map by reducing chroma only as far as needed and allow lightness to move toward the target hue's cusp, or rotate in HSL like PowerToys and Adobe do; test that every scheme swatch differs from white, black and the base by a minimum distance. | M |
| TOOLS-6 | medium | Broken | Tints and shades invert at the ends. Measured: white gives "tints" `#fcfcfc, #fafafa, #f7f7f7, #f5f5f5` (each darker); black gives "shades" `#000000, #030303, #0b0b0b, #161616` (each lighter, and the first equals the base). | run | Build the ramp between the base and fixed end points on the correct side only; when no room is left on one side, return fewer swatches instead of wrong ones. | S |
| TOOLS-7 | low | UX | Every hue scheme of a grey, white or black returns identical swatches (measured: grey `#808080` tetradic = four times `#808080`) with no note. | run | For chroma below a threshold show "No hue to rotate" in place of the chips, or fall back to tints and shades. | S |
| TOOLS-8 | medium | Broken | Lenient parsing turns ordinary words and numbers into colours in the live input. Measured: `bad` = `#bbaadd`, `ace`, `fed`, `100` = `#110000`, `255` = `#225555`, `16711680` = `#16711680` (a colour with alpha 0x80). Each successful parse also replaces the current colour and writes settings (TOOLS-20). | run | In the live field require `#` for 3 and 4 digit hex, treat a bare integer as nothing (or as decimal RGB with a visible hint), and apply only on Enter or after a pause. | S |
| TOOLS-9 | low | Missing | Formats not parsed (measured, all `BadFunction` or `BadHex`): `hwb()`, `lab()`, `lch()`, `oklab()`, `color(srgb ...)`, `color(display-p3 ...)`, `0xAARRGGBB` (8 digits after `0x`), `255,255,255,0.5` (reported as "unknown name"), decimal integer, `Color.FromArgb(...)`, `UIColor(...)`. PowerToys Color Picker offers HEX, RGB, HSL, HSV, CMYK, HSB, HSI, HWB, NCol, CIELAB, CIEXYZ, VEC4, Decimal, HEX Int and user-defined format strings. | run | Add the CSS Color 4 functions and `0xAARRGGBB`; add user-defined copy formats (a template string with %Re, %Gr... like PowerToys) so code formats do not each need a parser and a row. | M |
| TOOLS-10 | low | UX | Text pasted from code is rejected for a trailing or surrounding character. Measured: `#FF0000;`, `"#ff0000"`, `background: #ff0000`, `rgb(255,0,0);`, `#ff0000 #00ff00` all fail. | run | On paste, extract the first colour literal from the text (the strict `literal` scanner already exists) and show which part was used. | S |
| TOOLS-11 | low | i18n | Full-width input is not folded: `＃ｆｆ００００` fails as "unknown name" (ja and zh-Hans are shipped languages and their IMEs produce full-width digits and `＃`). Decimal comma in the alpha field (`50,5`) is rejected in de, fr, es, pt-BR. | run (first), code (second) | NFKC-fold colour input before parsing; accept the locale decimal separator in number fields. | S |
| TOOLS-12 | low | UX | The Name row shows "near royalblue" for `#4169e0` and pastes `royalblue`, which is `#4169e1`; for `#783cc8` it pastes `blueviolet` = `#8a2be2`, a visibly different colour. Measured: Name round-trips for 1 of 172,961 sampled colours. | run | Paste the name only on an exact match; otherwise the row copies nothing and offers "Use royalblue (#4169e1)" as an explicit change of the current colour. | S |
| TOOLS-13 | medium | Broken | Contrast ignores the alpha of the background and flattens the foreground on white only. Measured: white text on fully transparent black reports 21.00 (black treated as opaque); black at 50% on white reports 4.00 whatever the real backdrop is. | run | Composite foreground over background and background over a stated backdrop (white, with a toggle to black), and say "over white" next to the ratio whenever either alpha is below 100%. | S |
| TOOLS-14 | low | Missing | Contrast checker has no "use as the other colour" on a swatch, recent or eyedropper pick, no swap button found in the block, no suggestion of the nearest passing colour, no APCA value. WebAIM, Polypane and Raycast's Color Picker extension offer at least swap and nearest pass. | code | Add swap, "pick other with eyedropper" and "nearest colour that passes AA" (walk OKLCH lightness). | M |
| TOOLS-15 | low | UX | `ratio_text` truncates: 4.4781 shows `4.47` and fails AA; 4.5422 shows `4.54`. Correct (never rounds up over a threshold), but the pass marks have no text for the threshold, so a user sees "4.47" and a fail with no "needs 4.5". | run + code | Show the threshold next to each mark ("AA 4.5"). | S |

### Colour: controller and UI

| Id | Sev | Lens | Evidence | Verified | Fix (whole class) | Effort |
|---|---|---|---|---|---|---|
| TOOLS-16 | high | Broken | The Saved strip shows the first 24 colour items (`SAVED_STRIP`, `read_saved_colours` `take(24)`) while a save appends with `Placement::Last`. The 25th saved colour is stored but never appears in the strip; the user sees "saved" and no swatch. | code | Show newest first (or scroll or "Show all N" link to the Colours list); test with 25 saves that the last one is visible. | S |
| TOOLS-17 | medium | UX | Saving the same colour twice creates two items (`save_colour` has no lookup), and the menu of an already saved swatch offers "Save to Colours" again. | code | Dedupe on RGBA at save (select the existing item and say "Already saved"); hide the entry on saved swatches. | S |
| TOOLS-18 | medium | Missing | A saved swatch has no delete, rename or edit in the Colours tab; "Remove from recents" exists only for recents. The user must leave for the Library to remove a saved colour. | code | One swatch menu for both strips: Use, Copy, Rename, Delete (with undo). | S |
| TOOLS-19 | medium | UX | Click means different things in one view: a chip (recent, saved, scheme) makes the colour current (`on_swatch_used` -> `set_colour`), a format row pastes. The footer hint "Click pastes into {} · Shift+click copies" is not scoped to the rows. ChipGrid accepts Space, FormatList only Return. | code | One rule per element type, stated once near the rows; accept both Enter and Space on every activatable element. | S |
| TOOLS-20 | medium | Performance | `input-edited` calls `set_colour`, which calls `update_settings`, on every keystroke that parses: typing `#3a7bd5` writes settings and pushes recents state several times (`#3a7` already parses). | code | Debounce persistence (write on blur, Enter or 500 ms idle); keep the live preview in memory only. | S |
| TOOLS-21 | low | UX | The format menu's first entry reads "Paste or copy" whenever the use-hint is non-empty, which is always; next to "Copy only" both do the same when there is no target. | code | Label from the real state: "Paste into {target}" or only "Copy". | S |
| TOOLS-22 | low | Broken | The "A" channel field exists only while alpha != 255: stepping alpha back to 100 removes the field that has focus. Alpha has 101 slider steps, so 155 of 256 alpha bytes cannot be set from the UI. | code | Keep the field while it has focus (or always show it); accept 0..255 or one decimal. | S |
| TOOLS-23 | low | UX | Scheme chip captions use the default format text in a 60 px cell: with OKLCH as default (`oklch(49.93% 0.0987 250.43)`, 27 characters) every caption is elided to the same prefix. | code (length measured in probe) | Caption always in hex (short), full default format in the tooltip. | S |
| TOOLS-24 | low | Missing | Ctrl+V in the Colours tab without field focus does nothing; there is no "previous colour" or undo for colour edits (Ctrl+Z runs the app undo). PowerToys keeps a 20-entry history editor. | code | Ctrl+V anywhere in the tab parses the clipboard; keep the previous colour as a split swatch that restores on click. | S |
| TOOLS-25 | low | Accessibility | TrackSlider and the picker Surface expose `accessible-value` as formatted text, not a number with min, max and step; no PageUp or PageDown (large step) on the sliders. `SectionTitle.hint` is declared and never set. | code | Give sliders numeric value, minimum, maximum, step and increment/decrement actions; add Page keys (10 steps); delete the unused property. | S |
| TOOLS-26 | low | UX | After an HDR pick the HDR toast replaces the "copied" toast, so the user is not told the value was copied (`colour_picked`). | code | One toast: "Copied #... · HDR screen: approximate". | S |
| TOOLS-27 | low | Broken | `EyedropOutcome::Failed(reason)` drops the reason: not traced, not shown; the user gets a generic toast and a bug report has nothing to quote. | code | Trace the reason; put a short cause in the toast ("screen capture was refused"). | S |
| TOOLS-28 | medium | Broken | The eyedropper hides only the Panel before the shot. Main, Settings or a pinned note of Snipdeck stay in the frozen frame and cover the pixels behind them. On return `back_from_eyedropper` always shows the Panel on the Colours tab, also after Esc and when the run started elsewhere (hotkey or Main). | code | Hide every Snipdeck top-level window for the shot (or exclude them from capture) and restore exactly what was visible before; on cancel restore without changing tab. | M |
| TOOLS-29 | low | Missing | A saved colour gets a title only when it is an exact CSS keyword, so most saved colours are untitled and cannot be found by name in search; no way to name one at save time. | code | Title = nearest name with the hex ("Royal blue-ish #4169e0") or ask inline; allow rename (TOOLS-18). | S |
| TOOLS-30 | low | Accessibility | `accent.rs`: for a Windows accent the dark fill is nudged only to 3:1 against the surface, and the test accepts `text.on-accent` on the fill at 3:1 (line 217). Body-size button labels need 4.5:1 (WCAG 1.4.3). | code | Nudge until on-accent text reaches 4.5:1 in both themes, or switch the on-accent ink (black or white) per fill; assert 4.5 in the test. | S |
| TOOLS-31 | low | Missing | Copy formats miss common code forms that PowerToys and Raycast offer: hex without `#`, upper-case hex, `0xRRGGBB`, RGB 0..1 floats, Swift, Android ARGB, Tailwind nearest. No option for upper-case hex was found. | code | Solved by the user-defined formats of TOOLS-9; add a "hex case" setting. | M |

### Eyedropper (Windows; code read only)

| Id | Sev | Lens | Evidence | Verified | Fix (whole class) | Effort |
|---|---|---|---|---|---|---|
| TOOLS-32 | medium | UX | The loupe readout is always lower-case `#rrggbb` (`hex_text`), whatever the default copy format is; what is shown is not what is copied. | code | Format the readout with the default format (and a second line in hex). | S |
| TOOLS-33 | medium | i18n, Accessibility | The window title "Eyedropper, #hex" (`window_title`) is built in the platform crate in English; it is the only thing a screen reader hears. `EyedropTexts` carries only `hint` and `hdr_note`. | code | Add the title pattern to `EyedropTexts` so the host passes a translated one; a test that no user-visible literal lives in `snipdeck-platform`. | S |
| TOOLS-34 | medium | Broken | At zoom 4 on a 100% screen the centre cell is 4 px and the frame is 1 px black plus 2 px white, so the frame covers the whole picked cell: the user cannot see the pixel being picked. | code (arithmetic from the constants; not rendered) | Draw the frame outside the cell (around it), never over it; minimum zoom such that cell >= 8 px. | S |
| TOOLS-35 | low | Missing | Zoom is on the mouse wheel only: no key (`+`, `-`), so a keyboard user (arrows and Enter are supported) cannot zoom. | code | `+`/`-` and Ctrl+arrows (10 px step). | S |
| TOOLS-36 | low | Missing | One pixel only: no averaged sample (3x3, 5x5), which is what is needed on photos, gradients and anti-aliased text. One pick ends the run: no Shift+click to collect several colours. Frozen frame: hover states, tooltips and video cannot be picked "live". | code | Aperture setting cycled with a key and shown in the loupe; Shift+click adds to recents and continues. | M |
| TOOLS-37 | low | UX | The hint bar is painted top-centre on every monitor for the whole run and covers pixels there; it cannot be dismissed. | code | Fade it after 2 s or move it away when the cursor comes within 80 px. | S |
| TOOLS-38 | low | Performance | Every monitor's pixels are held twice (the DIB and a `Vec` copy in `Shot`): about 66 MB per 4K monitor, about 200 MB for three, for the length of the run. | code (4 x 3840 x 2160 x 2) | Read from the DIB section memory directly. | S |
| TOOLS-39 | low | Accessibility | Loupe text is a fixed 12 pt Segoe UI: it follows DPI but not the Windows text-size setting (Accessibility > Text size). | code | Multiply by the text scale factor the UI already reads (`SystemLook.text-scale`). | S |
| TOOLS-40 | low | UX | The run ends silently on a display change and after the 120 s timeout: no toast says why the eyedropper went away. A touch tap picks at once, with no chance to place the loupe. | code | Toast on every non-user end; on touch, first tap places, second tap or a "Pick" button confirms. | S |
| TOOLS-41 | low | Broken | `capture_all` fails the whole run when one monitor's BitBlt fails (a protected or disconnected-in-flight display). The `collect_monitor` `extern "system"` callback has no `catch_unwind`. | code | Skip the failed monitor (paint it grey with a note) and continue; wrap the callback body. | S |
| TOOLS-42 | medium | Broken (unverified) | HDR detection reads the old `DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO` bits. On Windows 11 24H2 with automatic colour management (ACM) on an SDR panel, GDI BitBlt values are also converted, and the old struct does not report that state. Values may be off without the "approximate" note. | code | Query `DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO_2` when available and treat `activeColorMode` other than SDR as approximate; long term capture with DXGI duplication in scRGB and convert. | M |
| TOOLS-43 | low | Broken (unverified) | 150 ms settle between hiding the Panel and the capture; if the hide animation or DWM is slower, the Panel is in the shot. The hotkey fallback registers Esc, Enter, Space and the arrows as global hotkeys, taking them from every other app for the run and failing if another app owns one. | code | Wait for the window to be cloaked (`DWMWA_CLOAKED`) instead of a fixed time; say in a toast when a key could not be registered. | S |

### Symbols and emoji

| Id | Sev | Lens | Evidence | Verified | Fix (whole class) | Effort |
|---|---|---|---|---|---|---|
| TOOLS-44 | high | Missing | The tab is named Symbols and has no symbol characters: no arrows, maths, currency, punctuation, Greek, superscripts, box drawing. Measured: `degree`, `nbsp`, `euro` (banknote emoji only), `bullet`, `dash` return no typographic character; `pi` returns a Phosphor icon that pastes a PNG. Win+. has a Symbols page with these. | run | Add a fourth kind "Characters" from Unicode data (name, block, a few hundred curated first), pasted as text. | M |
| TOOLS-45 | high | Missing | 328 of 1,923 emoji are dropped because Fluent Emoji Flat has no art (`manifest.json`: 1,595 kept). All 258 country flags are gone (Flags group has 8 entries), plus family, couple with heart, kiss (people), people holding hands, and the Emoji 16 and 17 additions. Measured: `germany` 0 hits, `family` returns a crown and a tree icon. Recorded as a decision in `docs/reviews/symbols-spec.md` lines 41-42, but the user is not told. | run | Keep every RGI emoji and draw the missing ones as text with the system emoji font (the fallback branch already exists in `symbols.slint`), marked as "system style"; a test that the catalogue equals emoji-test.txt. | M |
| TOOLS-46 | medium | Missing | No skin tones: 2,040 variants skipped (`manifest.json`), `skin` 0 hits, no long-press or menu. Deferred in `symbols-spec.md` D7; not in the SPEC post-1.0 list. Win+. and Raycast both have a default tone setting. | run | A default-tone setting plus a per-emoji tone row in the cell menu; art exists in Fluent for the five tones. | M |
| TOOLS-47 | medium | i18n | Kaomoji and icons have English names and keywords in every language. Measured: 341 of 341 kaomoji names and 1,512 of 1,512 icon names are identical to English in de, es, fr, ja, pt, zh; they are also the screen-reader labels and tooltips. A German user typing `zahnrad` or a Japanese user typing `設定` finds no icon. | run | Translate the 10 kaomoji group tags and the about 260 distinct kaomoji names; for icons translate the category names and the top keywords, at least through the existing `.po` flow. | L |
| TOOLS-48 | medium | i18n | Folding misses the common ASCII spellings. Measured (de): `straße` 7 hits, `strasse` 1 unrelated hit; `lächeln` 18, `laecheln` 0. (fr): `cœur` 37 hits, `coeur` 5. | run | Extend `fold` with ß to ss, œ to oe, æ to ae, and index the German ae/oe/ue forms as alternates; table-driven test per language. | S |
| TOOLS-49 | medium | i18n | Japanese and Chinese readings are not searchable. Measured (ja): `顔` 186 hits, `かお` 0; `猫` 13 hits, `ねこ` 3 (only the katakana-named ones); (zh): `xiao` 0. A Japanese user types kana before IME conversion, and the field reports `composing`, so the uncommitted text finds nothing. | run | Ship CLDR's kana annotations (the `ja` annotation set includes hiragana keywords for most emoji) and pinyin for zh names generated at build time. | M |
| TOOLS-50 | low | Broken | Substring matching needs three characters, and a CJK name is one "word", so a one or two character query matches only at the start of a name or as a keyword. Measured (zh): `车` returns 6 hits and not 🚆 `火车`, which `火` finds. | run | For CJK scripts match any substring from one character; rank it below prefix. | S |
| TOOLS-51 | low | Broken | `fold` strips combining marks after NFKD, which removes dakuten: `か` and `が`, `は` `ば` `ぱ` become the same letter. Measured (ja): `ひ` returns 113 hits including `びっくり` and `ピエロ`. | run | Skip mark stripping for kana (U+3099, U+309A), keep it for Latin. | S |
| TOOLS-52 | low | Missing | No tolerance and no aliases. Measured: `smlie` 0, `thumbsup` 0 (`thumbs up` 5), `tableflip` 0 (`table flip` 4), `:smile:` 0, `:)` 0, `lenny` 0, `U+1F600` and `1f600` 0. Shortcodes are declined in `symbols-spec.md` line 225; the others are not. | run | Match with spaces removed, one-edit tolerance for queries of 5+ letters, code point lookup, and ASCII emoticon aliases (`:)`, `<3`). | M |
| TOOLS-53 | low | Broken | Odd keyword hits from the data. Measured: `pizza` returns 🍄‍🟫 brown mushroom as `KeywordExact`; `family` returns 👑 crown; `100` returns 💶 euro banknote. | run | Review the CLDR keyword import for the newest emoji (likely a column shift for ZWJ sequences added in 15.1) and add a test of 20 fixed query to emoji pairs. | S |
| TOOLS-54 | medium | Accessibility | With text in the search field, Left, Right, Home and End go to the caret (`panel.slint:1442`), so the grid selection moves only by Up, Down and Page keys: in a result grid of N columns only one column is reachable by keyboard without clearing the query. | code | Give the grid its own focus stop (Tab or Down from the field enters it, then all arrows move in it, typing returns to the field), as Win+. does. | M |
| TOOLS-55 | medium | Accessibility | The emoji group strip (9 buttons) and the "Show all" header links are mouse-only: `accessible-role: button` with no focus handling in `symbols.slint`; cells are not focusable either and show the ring only while the search field has focus. The strip exists only for Emoji: kaomoji (10 groups) and icons (categories) have no jump strip. | code | Make the strip a focusable tab-list with arrow keys; build it from the kind's own groups so all three kinds have it. | M |
| TOOLS-56 | low | Accessibility | 45 kaomoji names are shared by 2 to 10 entries (measured: `cat` x10, `bunny` x4, `bear` x3, `dog` x3; 264 distinct names for 341 kaomoji). Name is the accessible label and the tooltip, so a screen reader says "cat, Kaomoji" ten times. | run | Label = name plus index within the name ("cat 3 of 10") or a short description of the face. | S |
| TOOLS-57 | medium | Broken (unverified) | 284 of 341 kaomoji contain a character that the bundled IBM Plex Sans lacks (177 distinct: box drawing, half and full width forms, kana, Kannada `ಠ`, Canadian syllabics `ᕗ ᗒ`, Thai `ง ฅ`, Arabic `ﻭ ٩ ۶`, Hangul `눈`, combining marks). Rendering depends on the renderer's system fallback; a missing fallback font shows tofu, and mixed fonts change the face's shape and width. Cells elide (`overflow: elide`) at half width; the longest kaomoji is 24 characters. | run (cmap) for coverage; rendering not checked | Render each kaomoji once in the harness on Windows and compare to a stored width; drop or replace entries that need a font not present on a clean Windows 10; name a fallback list for kaomoji text. | M |
| TOOLS-58 | low | Missing | There is no standalone Colours or Symbols window: `ColoursView` and `SymbolsView` are instantiated only in `panel.slint`. The tools cannot stay open beside a design or a document while the Panel closes on paste. PowerToys' editor and Raycast's pinned picker stay open. | code (grep) | A "keep open" pin for the Panel while a tool tab is shown, or pop the tab out as a small always-on-top window. | M |
| TOOLS-59 | low | UX | For an icon the "Paste" entry puts a PNG on the clipboard: fixed 64 px, in the theme's text colour (near white in the dark theme, invisible on a white document). `IconExport`'s doc mentions 24 to 256 px; no size or colour choice exists in the menu. | code | Ask size and colour once (remember them); default to the document-safe dark ink regardless of theme. | S |
| TOOLS-60 | low | UX | The All view shows two rows per kind (`ALL_ROWS = 2`), so with recents present the first screen has about 2 rows of emoji before kaomoji and icons; no frequency ordering beyond the 24 recents, and no "frequently used". | code | Order the first rows by use count; let All show one kind's rows fully when the others are empty for the query. | S |
| TOOLS-61 | low | Performance | The first search in a language builds the folded views lazily. Measured (debug build): first query 20 to 37 ms, later ones 1.3 to 5 ms. Fine in release, but it runs on the UI thread on the first keystroke. | run | Build the view for the UI language when the tab opens (idle), not on the first key. | S |
| TOOLS-62 | low | Docs | `docs/SPEC.md` line 69 says the icon menu (copy SVG, save as) is "planned (slice 6)"; `symbol_ctl.rs` implements CopySvg, CopyPng, SaveAsSvg, SaveAsPng. Skin tones (D7) and the flags drop are decisions in `docs/reviews/symbols-spec.md` only and are missing from the SPEC post-1.0 list. | code | Update SPEC from the code; move deferred items into the one post-1.0 list. | S |
| TOOLS-63 | low | UX | Emoji art is stored at 48 px and drawn at 24 px logical: sharp up to 200% scale, upscaled (soft) at 250% and 300%, and there is no larger preview of the selected emoji (Win+. and Raycast show one). | code | Draw from the SVG source at 300%+, or store 72 px; show name and a large preview in the footer for the selection. | M |

Total: 63 findings (0 critical, 3 high, 23 medium, 37 low).

## 3. Not verified; what a Windows run must check

Not verified here:

- Nothing in the running UI was measured (no harness run): sizes of chips and cells, the 60 px caption
  elision (TOOLS-23), the loupe frame over the centre cell (TOOLS-34), focus rings, light and dark
  rendering, text scale 200%.
- Paste into the target window from a format row, an emoji cell, a kaomoji and an icon PNG.
- Font fallback for kaomoji and for emoji without art (TOOLS-57).
- Owner bugs noted in `.claude/now.md` lines 182-183 (Panel aqua or collapsed after a pick; lines while
  scrolling emoji) were not re-tested.
- `components/colour_block.slint`, `screen_clip.rs`, the generator scripts and the test files were not read.

A Windows run must check:

1. Eyedropper on two monitors with different scale (100% and 150%) and a monitor left of the primary
   (negative coordinates): the loupe stays on the cursor's monitor, the picked value equals a known
   test pattern pixel on each.
2. HDR on, and Windows 11 24H2 "automatically manage colour for apps" on an SDR screen: pick a known
   sRGB patch (`#ff0000`, `#808080`); record the value and whether the "approximate" note shows (TOOLS-42).
3. Esc, right click, the 120 s timeout, and unplugging a monitor during a run: the Panel and every other
   Snipdeck window return as before; what the user is told (TOOLS-28, TOOLS-40).
4. Start the eyedropper with Main open over the target: is Main in the frozen frame (TOOLS-28); is the
   Panel ever in the frame at the 150 ms settle with animations on (TOOLS-43).
5. Zoom 4 at 100%: is the picked pixel visible under the frame (TOOLS-34). Text size 200% in Windows
   Accessibility: loupe text size (TOOLS-39).
6. The hotkey fallback path: while the eyedropper runs, Esc and arrows in another app; and with another
   app owning a global Esc hotkey.
7. Narrator on the eyedropper (title changes per pixel) in German or Japanese UI (TOOLS-33), and on the
   symbol grid and the group strip (TOOLS-54, 55, 56).
8. All 341 kaomoji in the Kaomoji tab on a clean Windows 10 and Windows 11: count tofu boxes and
   elided cells (TOOLS-57); paste five of them into Notepad and Word.
9. Emoji pasted into Notepad, Word, a browser field and a terminal: ZWJ sequences and keycaps arrive
   whole; icon PNG paste into Word and Paint on a white page from the dark theme (TOOLS-59).
10. Search with a Japanese and a Chinese IME: results while composing and after commit (TOOLS-49).
11. Save 25 colours and look for the 25th in the Saved strip (TOOLS-16).
