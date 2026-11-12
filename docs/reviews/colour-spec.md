# Colours tab and screen eyedropper: build spec (2026-11-12)

Design only; nothing here is built. Owner, 2026-10-20: "a colour picker and eyedrop tool that i can use
to copy colours from anywhere on my computer and then I can get the details like the rgb cmyk, hex etc
for that colour as well as see accompanying colour schemes and contrast". 2026-11-12: "I like to save
and copy colours I see for when I do things like web design as I am an interaction design student".

Pattern to copy: the Symbols tab (`docs/reviews/symbols-spec.md`). Builder reference files:
| New file | Copy the shape of |
|---|---|
| `crates/snipdeck-colour/` (new crate) | `crates/snipdeck-symbols/` (pure crate, `recents.rs`, `tests/`) |
| `crates/snipdeck-ui/ui/colours.slint` | `crates/snipdeck-ui/ui/symbols.slint` (rows from the host, `SymbolTexts` global) |
| `panel.slint` tab and view | `panel.slint:224-252` (props), `:1852-1863` (smiley `ClockTab`), `:2268` (`SymbolsView`) |
| `crates/snipdeck-app/src/colours_binding.rs` | `crates/snipdeck-app/src/symbols_binding.rs` (`Glue`) |
| `crates/snipdeck-app/src/controller/colour_ctl.rs` | `controller/symbol_ctl.rs` (`use_symbol` :111, `add_symbol_to_list` :146, `remember_symbol` :395) |
| `controller/eyedrop_ctl.rs` | `controller/screen_clip_ctl.rs` + `screen_clip.rs` (host trait in an `Option<Box<dyn _>>`, Panel steps aside) |
| `snipdeck-platform/src/eyedrop.rs` + `windows/eyedrop.rs` | `instance_names.rs` (portable logic, tested on Linux) + `windows/single_instance.rs`; trait + fake like `file_dialog.rs` (`FilePicker`, `FakeFilePicker`) |
| settings fields | `settings.rs:768` `show_symbols_tab`, `:790` `symbols_recent`, `field!` at `:968/:976`, test `:2129` |
| `crates/snipdeck-ui/tests/colours.rs` | `crates/snipdeck-ui/tests/symbols.rs` (`Harness`, `assert_golden`, `UPDATE_GOLDEN=1`) |
| `tools/windows-test/tests/colour.json` | `tools/windows-test/tests/quick-check.json` |

## 1. User stories

1. I see a colour in Figma, a website or a photo. I press my eyedropper chord (or the tab's eyedropper
   button), point, click: the colour is on the clipboard in my default format and the Colours tab shows
   every format, its contrast and its schemes.
2. I paste or type `#3A7BD5`, `rgb(58 123 213)`, `hsl(215 65% 53%)`, `oklch(...)` or `royalblue` and see
   the same details.
3. I click `HEX` to paste `#3a7bd5` into the field I was typing in; Shift+click copies only.
4. I check whether white text on my button colour passes WCAG AA, and against my page background.
5. I get complementary, analogous, triadic, split-complementary, tetradic and tint/shade colours, and
   take any of them as the current colour or copy it.
6. I save a colour to the list I am looking at (Ctrl+S) and build a project palette as a list; its cards
   show a swatch.
7. I find colours I used recently without saving them.
8. I can do all of it with the keyboard and Narrator, at 175 % text size, in my language.

## 2. Decisions (written once, with reasons)

**D1. One entry point: a "Colours" tab in the Panel tab row, right of the Symbols tab.** `ClockTab` with
Phosphor `palette`, label `@tr("Colours")`, same states as the smiley tab; reachable by Ctrl+Tab cycling
and by the palette (Ctrl+K) commands "Colours" and "Pick a colour from the screen". Setting "Show Colours
tab" (Settings > Panel, on by default, beside "Show Symbols tab"). Reason: same as Symbols D6, the Panel
is the window with a paste Target. Not in Main or the Picker.

**D2. Clicks: format rows follow Symbols, swatches do not (changed from the starting decision).** A
format row (HEX, RGB, ...) is the thing pasted: click pastes into the Target (copies when there is
none), Shift+click copies only. A swatch (recent, scheme, tint) is a colour to look at first: click makes
it the current colour; Shift+click copies it in the default format. Reason: a swatch click that pasted
would make "show me this scheme colour's details" impossible without a menu; designers inspect, then
copy a specific format.

**D3. Formats and their copy text (canonical value is 8-bit sRGB RGBA).** Lower-case hex (`#rrggbb`,
`#rrggbbaa` when alpha < 255). `rgb()`/`hsl()` use the comma syntax (`rgb(58, 123, 213)`,
`rgba(58, 123, 213, 0.5)`, `hsl(215, 65%, 53%)`, `hsla(...)`): it pastes into every CSS version, email
HTML, Figma, Android XML tooling and older libraries; the parser takes both syntaxes. `hsb(215, 73%,
84%)`: labelled **HSB** with Narrator description "HSB, also called HSV", because Figma, Photoshop and
Illustrator call it HSB. `cmyk(73%, 42%, 0%, 16%)`: naive device-independent conversion, the row's
tooltip says "Approximate: print needs your printer's profile". `oklch(58.62% 0.1533 257.23)` (CSS Color
4, `/ a` when alpha < 255). **Name**: the nearest CSS Color 4 keyword (148, English, never translated),
shown "royalblue" when exact, else "near royalblue"; the copy text is the bare keyword. Default copy
format: one of Hex, Rgb, Hsl, Hsb, Cmyk, Oklch (default Hex). **Name cannot be the default** (lossy).

**D4. Schemes rotate hue in OKLCH, keeping L and C; out-of-gamut results reduce chroma.** Reason: OKLCH
is perceptually uniform, so a rotated colour keeps the base's perceived lightness (HSL rotation of a
blue to yellow jumps lightness and breaks a designed contrast); it is what CSS relative colour
(`oklch(from var(--c) l c calc(h + 180))`) gives, which a web student will meet. Consequence, said in
the UI hint: a complementary colour is a hue partner, not a contrast partner (for #3a7bd5 the ratio is
1.01:1); contrast is its own section.

**D5. Contrast: WCAG 2.x, compared unrounded, shown truncated.** Pass marks for AA normal (4.5), AA large
(3), AAA normal (7), AAA large (4.5) and non-text UI 1.4.11 (3) against white, black and a chosen
"other" colour (default white; persisted; a Swap button exchanges current and other). Each mark is a
glyph that differs in shape (check / x) plus the word in its accessible name: never colour alone. A
ratio is never rounded up into a pass: #777777 on white is 4.478 and shows "4.47:1", AA fail.

**D6. Saved colour = a Text item; a palette = a list. No schema change.** Ctrl+S calls `quick_add` with
the colour in the default format (same path, toast, Undo as Symbols' "Add to list"). The card's type tile
becomes a swatch when the item is not sensitive, its kind is Text or Code, and its whole trimmed text is
one strict CSS colour literal (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb[a]()`, `hsl[a]()`,
`oklch()`; never a bare keyword such as "red", never hex without `#`). Derived from content at view time,
so existing snippets like `#ffcc00` gain swatches too and nothing is stored. Rejected: a new ItemKind
(schema V-bump for no data the text does not already hold); the 8 `ItemLabel` colours (not arbitrary).

**D7. Recents: most recently used, 24 max, deduped by RGBA, in settings.** Pushed on paste, copy, save
and eyedropper pick (not on typing or slider drags). Stamped by the injected `Clock`.

**D8. Eyedropper: freeze-frame per monitor, own Win32 overlay, Panel steps aside.** Details §6. Reason
for freezing over a live sampler: the overlay shows the exact pixels it will return (protected content
that captures black also *shows* black, hover effects do not flicker away under the loupe), and the
loupe never samples itself. Reason for raw Win32/GDI over a Slint window: a full-screen software-rendered
Slint frame per mouse move at 4K is a measured-nowhere risk; GDI repaints only the two loupe rectangles.

**D9. After a pick: copy in the default format and show the Colours tab (setting, on).** "Copy the
picked colour" (`colour_copy_on_pick`, default on): the owner's words are "copy colours from anywhere".
No paste: after an overlay the Target is not trustworthy. Toast "Copied #3a7bd5".

**D10. Global hotkey "Pick a colour from the screen": off until the user sets a chord.** Same rule as
Capture (`docs/reviews/default-chords.md`): Win+ chords are reserved, Ctrl+Alt is AltGr, and the
obvious Ctrl+Shift+C is Inspect in Chrome and Edge. Inside the Colours tab, Ctrl+I (Photoshop/Figma use
I for the eyedropper; Ctrl+I is unused in `shortcuts.rs`).

**D11. Free for everyone.** `docs/licensing.md` makes Supporter about convenience across the library
(boards, prompted fields, paste stack); Symbols is free; a colour picker is an everyday utility that
Windows' own PowerToys gives free, and gating it would be the first paywalled Panel tab. No `Feature` key.

**D12. Colour maths in a new pure crate `crates/snipdeck-colour`** (deps: `serde` only). Not in
`snipdeck-core` (about the user's library) nor `snipdeck-symbols` (catalog data). The workspace's
`members = ["crates/*"]` picks it up.

## 3. UI: the Colours tab (Panel 260-480, default 320; P2 tokens)

```
┌───────────────────────────────────────────────┐
│ header (40, unchanged)                        │
│ [🕘][☺][▣ Colours]│ Snippets  Email  +        │ tab row 32, Colours selected (2px accent.text underline)
├───────────────────────────────────────────────┤
│ ┌──────┐ ┌────────────────────────────┐ ┌──┐  │ current band 64
│ │swatch│ │ #3a7bd5                    │ │⌖ │  │ 48×48 swatch · TextField h32 · eyedropper 32×32
│ │48×48 │ └────────────────────────────┘ └──┘  │
│ └──────┘  near royalblue                      │ type.sm text.secondary
│ ⚠ Not a colour. Try #3a7bd5 or rgb(58, 123, 213)   (only while invalid, h20, role alert)
│ RECENTLY USED                                 │ header h24
│ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■                           │ 24×24 swatches in 28×28 hits, wraps, ≤ 3 rows
│ FORMATS                                       │
│ HEX    #3a7bd5                    default [⧉] │ rows h28; label col 56 type.sm; value mono, elided
│ RGB    rgb(58, 123, 213)                  [⧉] │ [⧉] = copy button on hover/selection/focus
│ HSL    hsl(215, 65%, 53%)                     │
│ HSB    hsb(215, 73%, 84%)                     │
│ CMYK   cmyk(73%, 42%, 0%, 16%)                │
│ OKLCH  oklch(58.62% 0.1533 257.23)            │
│ Name   near royalblue                         │
│ CONTRAST                                      │
│ [Aa]  on white  4.22:1   AA ✗  Large ✓        │ chip 40×24: "Aa" in current on the bg
│                          AAA ✗ Large ✗  UI ✓  │ marks wrap under 360 px
│ [Aa]  on black  4.97:1   AA ✓ Large ✓ ...     │
│ [Aa]  on [#ffffff      ] [⇄]  4.22:1 ...      │ "other" TextField h28 + Swap button
│ SCHEMES  [Complementary        ▾]             │ SelectField (form.slint:359)
│ ■ base   ■                                    │ 40×40 swatches, hex under each (type.xs mono)
│ #3a7bd5  #a47100                              │
│ ▸ Adjust                                      │ disclosure, remembered
│   ┌ saturation × brightness area, h120 ─────┐ │
│   │                         ○               │ │
│   └─────────────────────────────────────────┘ │
│   hue    ═════════════●═══════════            │ h16
│   alpha  ▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒●             │ h16 on a checkerboard
├───────────────────────────────────────────────┤
│ near royalblue          Click pastes into Figma│ footer 32 (P2 target line)
└───────────────────────────────────────────────┘
```

The band below the tab row is one vertical `ScrollView` with `DeckEdge` overflow; the current band stays
pinned at the top. Swatches: radius 4 (current 6), 1 px `border.subtle` outline so white shows on light
and black on dark, checkerboard under alpha; rest / hover (`bg.hover` ring 2 px) / selected (`bg.selected`
ring) / focus (2 px `focus.ring`, P2 §2.5). In high-contrast mode swatches keep their real colour (content,
like images) with a 1 px system-text outline; everything else maps to `SystemColors`. Tints and shades
show 9 swatches (4 tints, base, 4 shades) in rows of floor((W-16)/44). Hints: the schemes header tooltip
"Hue partners keep the lightness: check contrast below". The 48×48 swatch is not a button (no action).

### 3.1 Keys and clicks (one tab stop per group, roving selection inside, as the Symbols grid)

Focus order: input field, eyedropper button, recents, format rows, other-colour field, Swap, scheme
select, scheme swatches, Adjust disclosure, S/B area, hue, alpha. `Tab`/`Shift+Tab` move between them.

| Where | Input | Action |
|---|---|---|
| anywhere in the tab | Ctrl+I | start the eyedropper |
| anywhere in the tab | Ctrl+S | save the current colour (default format) to the shown list |
| not in a text field | Ctrl+C | copy the current colour in the default format |
| not in a text field | a printable character | goes to the input field (replaces its text), as Symbols' search |
| anywhere | Esc | in a field with unapplied text: revert it; else P2 §2.10 (hide the Panel) |
| input / other field | typing | applies live whenever the text parses; the error shows only after Enter or leaving the field |
| input / other field | Enter | apply; invalid: error line, text kept, focus stays |
| input / other field | Ctrl+V of a valid colour | applies at once |
| format rows | Up/Down, Home/End | move |
| format rows | click, Enter | paste into the Target; copy when there is none |
| format rows | Shift+click, Shift+Enter, copy button | copy only |
| format rows | right-click, Menu, Shift+F10 | Paste into "X" · Copy · Make default format (not Name) · Save to "List" |
| recents / scheme swatches | arrows, Home/End | move (2-D for wrapped rows) |
| swatches | click, Enter, Space | make current (does not push recents) |
| swatches | Shift+click, Shift+Enter | copy in the default format |
| swatches | Ctrl+S | save that swatch to the shown list |
| recents | Delete | remove from recents |
| swatches | right-click, Menu | Make current · Copy as ▸ (6 formats + Name) · Save to "List" · Remove (recents) |
| S/B area | Left/Right, Up/Down | saturation, brightness ±1 %; Shift ±10 %; drag/click sets both |
| hue / alpha | Left/Right | ±1° / ±1 %; Shift ±10; Home/End to the ends; drag |
| Swap | click, Enter, Space | exchange current and other |

The footer left shows the text of the hovered or selected row/swatch ("#a47100 · near darkgoldenrod"); the
right shows P2's target line. The default format row carries the word "default" (not colour alone).

### 3.2 Accessibility (WCAG 2.2 AA) and text scale

- Swatch: `accessible-role: list-item`, `accessible-label` "#3a7bd5, near royalblue" ("#4169e1,
  royalblue" when exact; with alpha "#3a7bd580, 50 % opaque, near royalblue"), `accessible-description`
  "Recently used, 3 of 12" / "Complementary" / "Tint 2 of 4"; `accessible-item-index`/`-count` as the
  list rows. Format row: label "HEX, #3a7bd5", description "Click pastes · Shift+click copies" (or
  "Click copies"). Contrast row: one label "On white: 4.22 to 1. AA text fail. AA large text pass. AAA
  text fail. AAA large text fail. Interface parts pass." Hue/alpha: `accessible-role: slider` with
  `accessible-value` ("215 degrees"), min, max, step, increment/decrement actions. S/B area: two values in
  one label "Saturation 73 %, brightness 84 %", arrow keys as the table.
- Every control has a 24×24 minimum hit (DESIGN §5), visible focus ring, tooltip on icon-only buttons
  (eyedropper: "Pick from the screen (Ctrl+I)"; copy; Swap).
- Text scale: sizes above are at 100 %; heights grow with `Type` tokens (`text_scale.rs`); the label
  column grows, values elide with the full text in the tooltip and accessible label; marks wrap; the
  tab scrolls. Golden at 175 %.
- The eyedropper overlay is mouse-and-keyboard (§6) and announces through its window title (unverified
  with Narrator); the typed input is the full alternative, so no task depends on the overlay.

### 3.3 Cards

`PanelRow` gains `swatch: color` and `has-swatch: bool` (set in `binding.rs::panel_row` from
`snipdeck_colour::literal(&row.preview)` with the D6 conditions). `item-card.slint` draws the 24 px type
tile filled with the colour (1 px `border.subtle`, checkerboard under alpha) instead of the Text glyph;
the accessible description gains "Colour swatch". Main window rows: later.

## 4. Settings (all in `settings.json`, read with `field!`, no `version` bump)

| Field | Type | Default | Notes |
|---|---|---|---|
| `show_colours_tab` | bool | true | Settings > Panel "Show Colours tab" |
| `colours_recent` | `Vec<snipdeck_colour::RecentColour>` | `[]` | `[{ "hex": "#3a7bd5", "used_at": 1789000000000 }]`, newest first, ≤ 24 |
| `colour_format` | `snipdeck_colour::Format` (serde lower-case key) | `"hex"` | `"name"` in a file falls back to `"hex"` |
| `colour_current` | String (`#rrggbb[aa]`) | `"#2870d4"` (`accent.default`) | restored when the tab opens |
| `colour_contrast_with` | String | `"#ffffff"` | the "other" contrast colour |
| `colour_scheme` | `snipdeck_colour::Scheme` | `"complementary"` | the select's value |
| `colour_adjust_open` | bool | false | the Adjust disclosure |
| `colour_copy_on_pick` | bool | true | D9; Settings > Panel "Copy a colour picked from the screen" |
| `hotkeys.eyedropper` | String | `""` (off) | `HotkeyAction::Eyedropper`, legacy chord also `""` |

Migration: none. A missing field takes its default (`field!` is lenient); a bad entry in
`colours_recent` is dropped alone (parse each element, as `no_paste_apps`); a bad `colour_current` or
`colour_contrast_with` takes the default. Older Snipdeck builds ignore unknown fields (serde default
behaviour is checked by the existing `from_json` path; slice 1 adds a round-trip test).

## 5. Colour maths: `crates/snipdeck-colour` (pure Rust, `#![forbid(unsafe_code)]`)

```rust
// lib.rs re-exports everything below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgba8 { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }
impl Rgba8 {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self;          // a = 255
    pub fn to_hex(self) -> String;                          // "#3a7bd5" or "#3a7bd580"
    pub fn from_hex(text: &str) -> Option<Self>;            // exactly "#rrggbb" / "#rrggbbaa" (settings)
}
#[derive(Debug, Clone, Copy, PartialEq)] pub struct Hsl { pub h: f64, pub s: f64, pub l: f64 }   // h 0..360, s,l 0..1
#[derive(Debug, Clone, Copy, PartialEq)] pub struct Hsv { pub h: f64, pub s: f64, pub v: f64 }
#[derive(Debug, Clone, Copy, PartialEq)] pub struct Cmyk { pub c: f64, pub m: f64, pub y: f64, pub k: f64 }
#[derive(Debug, Clone, Copy, PartialEq)] pub struct Oklch { pub l: f64, pub c: f64, pub h: f64 }  // l 0..1
pub fn to_hsl(c: Rgba8) -> Hsl;   pub fn from_hsl(v: Hsl, a: u8) -> Rgba8;
pub fn to_hsv(c: Rgba8) -> Hsv;   pub fn from_hsv(v: Hsv, a: u8) -> Rgba8;     // the S/B area and hue slider
pub fn to_cmyk(c: Rgba8) -> Cmyk; pub fn from_cmyk(v: Cmyk, a: u8) -> Rgba8;
pub fn to_oklch(c: Rgba8) -> Oklch;
pub fn from_oklch(v: Oklch, a: u8) -> Rgba8;   // gamut-maps by chroma reduction (below)

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format { Hex, Rgb, Hsl, Hsb, Cmyk, Oklch, Name }
impl Format {
    pub const ALL: [Self; 7];                  // display order = the rows
    pub const fn can_be_default(self) -> bool; // false for Name
    pub const fn label(self) -> &'static str;  // "HEX","RGB","HSL","HSB","CMYK","OKLCH"; Name's label is @tr in Slint
}
pub fn format(c: Rgba8, f: Format) -> String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError { Empty, UnknownName, BadHex, BadFunction, OutOfRange }
pub fn parse(text: &str) -> Result<Rgba8, ParseError>;  // lenient: the input fields
pub fn literal(text: &str) -> Option<Rgba8>;             // strict: card swatches (D6), ≤ 64 bytes

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Named { pub name: &'static str, pub rgb: Rgba8, pub exact: bool, pub distance: f64 }
pub fn nearest_name(c: Rgba8) -> Named;                   // ignores alpha
pub fn named(name: &str) -> Option<Rgba8>;                // ASCII case-insensitive

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scheme { Complementary, Analogous, Triadic, SplitComplementary, Tetradic, TintsShades }
impl Scheme { pub const ALL: [Self; 6]; }
pub fn scheme(c: Rgba8, s: Scheme) -> Vec<Rgba8>;         // base first, then the others (below)

pub fn relative_luminance(c: Rgba8) -> f64;               // alpha ignored
pub fn contrast_ratio(fg: Rgba8, bg: Rgba8) -> f64;       // fg composited over opaque bg first
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marks { pub aa: bool, pub aa_large: bool, pub aaa: bool, pub aaa_large: bool, pub ui: bool }
pub fn marks(ratio: f64) -> Marks;
pub fn ratio_text(ratio: f64) -> String;                  // truncated to 2 dp: "4.47"

// recents.rs (shape of snipdeck_symbols::recents)
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecentColour { pub hex: String, pub used_at: i64 }   // ms since epoch, injected clock
pub struct ColourRecents { /* private */ }
impl ColourRecents {
    pub const MAX: usize = 24;
    pub fn from_saved(saved: &[RecentColour]) -> Self;   // drops bad hex and duplicates
    pub fn push(&mut self, c: Rgba8, now_ms: i64);        // to the front; dedupe by RGBA; cap 24
    pub fn remove(&mut self, c: Rgba8);
    pub fn list(&self) -> Vec<Rgba8>;
    pub fn to_saved(&self) -> Vec<RecentColour>;
}
```

**Conversions.** sRGB transfer: linear = c/12.92 if c ≤ 0.04045 else ((c+0.055)/1.055)^2.4. OKLab: Björn
Ottosson's published M1 (linear sRGB to LMS) and M2 matrices, cube root between; OKLCH C = hypot(a, b),
h = atan2(b, a) in degrees mod 360. WCAG luminance uses the WCAG 2.x threshold 0.03928 (identical result
for 8-bit input) and 0.2126/0.7152/0.0722. HSL/HSV: the standard hexcone formulas; s = 0 when max = min;
HSL s = 0 when l is 0 or 1. CMYK: k = 1 - max; c = (1 - r - k)/(1 - k) (all 0 when k = 1).

**Rounding.** Every display number uses `f64::round` (half away from zero) at the stated precision, then
trailing zeros and a trailing dot are trimmed. 8-bit channels from floats: `round(x * 255)` clamped.
Hue, percentages (HSL, HSB, CMYK): integers; a hue that rounds to 360 is 0. OKLCH: L as a percentage to 2
dp, C to 4 dp, h to 2 dp; when C rounds to 0, print `0` for C and h (`oklch(100% 0 0)`). Alpha in
`rgba()`/`hsla()`/`oklch(/ a)`: a/255 to 2 dp (`0.5`); parsed alpha `round(a * 255)`. Contrast is
compared unrounded and displayed truncated (D5). Integer HSL/HSB/CMYK are not exact round trips (that is
the format, and what designers type); `parse(format(c, Hex|Rgb))` is exact, and `parse(format(c, Oklch))`
is within 1 per channel for every 8-bit colour (measured on 100,000 random colours: 81 off by one, none
by more).

**Parsing (`parse`).** Trim; ASCII case-insensitive. Accepts: `#rgb #rgba #rrggbb #rrggbbaa`; the same
without `#` and `0x` + 6 digits (only via `parse`); `rgb()`/`rgba()` with commas or spaces, `/ alpha`,
numbers or percentages, values clamped to range (CSS behaviour); `hsl()`/`hsla()` with hue units
`deg|turn|rad|grad` or none; `hsb()`/`hsv()`; `cmyk()` (percent or 0-1); `oklch()` (L as % or 0-1,
`none` = 0); three bare integers `58, 123, 213`; the 148 CSS keywords and `transparent`. `literal` takes
only `#` hex with 3/4/6/8 digits and the `rgb[a]`, `hsl[a]`, `oklch` functions, whole text.

**Nearest name.** Euclidean distance in OKLab over the 148 CSS Color 4 keywords (data: a `const` table
in `names.rs`, keyword and hex from the CSS Color 4 spec). Ties: the alphabetically first (so `aqua`
before `cyan`, `gray` before `grey`). `exact` = distance 0 on 8-bit values.

**Schemes (`scheme`).** Base (L, C, h) = `to_oklch(c)`; each output `from_oklch(Oklch{l: L, c: C, h: h + Δ
mod 360}, a)`. Δ: Complementary {180}; Analogous {-30, +30}; Triadic {120, 240}; SplitComplementary
{150, 210}; Tetradic (square) {90, 180, 270}. TintsShades: tints L_k = L + (0.97 - L)·k/4 and shades
L_k = L - (L - 0.20)·k/4 for k = 1..4, C and h kept; output order base, tints k=1..4, shades k=1..4 (the
UI lays them out light to dark). Achromatic input (C < 0.0002) returns copies of the base for the hue
schemes. **Gamut mapping** in `from_oklch`: if the OKLab-to-linear-sRGB result is within [-1e-6, 1+1e-6]
per channel, use it; else bisect C in [0, C] 40 times keeping the largest in-gamut C; then clamp, encode,
round. (Chroma reduction at fixed L and h, simpler than CSS Color 4's deltaEOK method and deterministic;
it keeps lightness, which D4 is about.)

### 5.1 Test vectors (`#3A7BD5`, hand-checked: hex, rgb, HSL, HSB, CMYK, luminance and ratios worked on
paper; OKLCH, schemes and the name from a reference script with the published matrices)

| Input / function | Expected |
|---|---|
| `format(Hex)` | `#3a7bd5`; with a = 128: `#3a7bd580` |
| `format(Rgb)` | `rgb(58, 123, 213)`; a = 128: `rgba(58, 123, 213, 0.5)` |
| `to_hsl` / `format(Hsl)` | h 214.839, s 0.64854, l 0.53137 / `hsl(215, 65%, 53%)` |
| `to_hsv` / `format(Hsb)` | h 214.839, s 0.72770, v 0.83529 / `hsb(215, 73%, 84%)` |
| `to_cmyk` / `format(Cmyk)` | 0.72770, 0.42254, 0, 0.16471 / `cmyk(73%, 42%, 0%, 16%)` |
| `to_oklch` / `format(Oklch)` | l 0.586186, c 0.153274, h 257.2335 / `oklch(58.62% 0.1533 257.23)` |
| `nearest_name` / `format(Name)` | royalblue #4169e1, exact false, distance 0.0515 / `royalblue` |
| `relative_luminance` | 0.19870 (lin r 0.04231, g 0.19807, b 0.66533) |
| `contrast_ratio` vs #ffffff / #000000 | 4.2220 → "4.22", marks aa ✗ aa_large ✓ aaa ✗ aaa_large ✗ ui ✓ / 4.9739 → "4.97", ✓ ✓ ✗ ✓ ✓ |
| Complementary | `#a47100` (gamut-mapped) ; vs base ratio 1.0068 |
| Analogous | `#0088b0`, `#786ad1` |
| Triadic | `#c64e5a`, `#4b9029` |
| SplitComplementary | `#c25914`, `#858000` |
| Tetradic | `#ba508d`, `#a47100`, `#00926d` |
| Tints k=1..4 | `#5799f6`, `#88b9ff`, `#bcd8ff`, `#eff6ff` |
| Shades k=1..4 | `#1b5db5`, `#00428f`, `#002a60`, `#001435` |
| `parse("3A7BD5")`, `parse("0x3a7bd5")`, `parse("58, 123, 213")` | #3a7bd5 |
| `parse("rgb(58 123 213 / 50%)")` | #3a7bd580 |
| `parse("hsl(215deg 65% 53%)")` / `parse("hsb(215, 73%, 84%)")` | #397ad5 / #3a7bd6 (lossy integers, by design) |
| `parse("oklch(58.62% 0.1533 257.23)")` | #3a7bd5 |
| `parse("RoyalBlue")` / `literal("royalblue")` | #4169e1 / None |
| `parse("#3ad")` / `parse("#12345")` / `parse("rgb(300,0,0)")` / `parse("blurple")` | #33aadd / BadHex / #ff0000 / UnknownName |

Scheme hex values: allow ±1 per channel (floating-point order across implementations). Others exact.
More: #ff0000 → `oklch(62.8% 0.2577 29.23)`; #ffffff → `oklch(100% 0 0)`, `hsl(0, 0%, 100%)`; #808080 →
`oklch(59.99% 0 0)`; #777777 on white 4.4781 → "4.47", aa ✗ (the rounding trap); #767676 → 4.5422 aa ✓;
#4169e1 → name exact. Property tests (proptest, already a workspace dep): parse∘format = id for Hex and
Rgb over all RGBA; Oklch within 1; `contrast_ratio(a, b) == contrast_ratio(b, a)` for opaque colours;
ratio in [1, 21]; every scheme output is valid sRGB; `ColourRecents` never exceeds 24 and has no
duplicates.

## 6. The Windows eyedropper

**Portable part, `crates/snipdeck-platform/src/eyedrop.rs`** (compiled and tested everywhere):

```rust
pub struct Shot { pub bounds: crate::route::ScreenRect, pub scale: f32, pub hdr: bool, pub bgra: Vec<u8> }
pub fn sample(shots: &[Shot], x: i32, y: i32) -> Option<[u8; 3]>;         // physical px; None in a gap
pub fn loupe_cells(shots: &[Shot], x: i32, y: i32, n: u32) -> Vec<Option<[u8; 3]>>; // n×n, row-major, n odd
pub fn loupe_rect(cursor: (i32, i32), monitor: crate::route::ScreenRect, scale: f32, zoom: u32) -> crate::route::ScreenRect;
pub const ZOOMS: [u32; 5] = [4, 8, 12, 16, 24];   // logical px per sampled pixel; default 8
pub const CELLS: u32 = 11;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav { Left, Right, Up, Down }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input { Moved(i32, i32), Key { nav: Nav, shift: bool }, Pick, Cancel, Wheel(i32), LostFocus, DisplayChanged, TimedOut }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step { Redraw, MoveCursor(i32, i32), Picked([u8; 3]), Cancelled, Nothing }
pub struct Session { /* cursor, zoom index, the union of shot bounds */ }
impl Session {
    pub fn new(shots: &[Shot], cursor: (i32, i32)) -> Self;
    pub fn on(&mut self, shots: &[Shot], input: Input) -> Step;  // nudge 1 px, Shift 10 px; never into a gap
    pub fn cursor(&self) -> (i32, i32);  pub fn zoom(&self) -> u32;
}
pub struct EyedropTexts { pub hint: String, pub hdr_note: String }          // translated by the app
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EyedropOutcome { Picked { rgb: [u8; 3], hdr: bool }, Cancelled, Failed(String) }
pub type EyedropDone = Box<dyn FnOnce(EyedropOutcome) + Send>;
pub trait Eyedropper {
    /// Waits `settle`, captures, shows the overlay; `done` is called once, on the eyedropper thread.
    fn start(&mut self, texts: EyedropTexts, settle: std::time::Duration, done: EyedropDone) -> Result<(), crate::error::PlatformError>;
    fn running(&self) -> bool;
}
pub struct FakeEyedropper { /* scripted outcomes, records starts */ }  // like FakeFilePicker
```

`loupe_rect`: box = CELLS·zoom + 2·4 wide and that + 40 (readout) tall, in logical px × the monitor's
`scale`; placed 24·scale right of and below the cursor, flipped left/up when it would leave the monitor.
Input mapping in `Session::on`: Moved updates the cursor (Redraw); Key nudges and returns MoveCursor
clamped to a pixel some shot covers; Pick returns Picked(sample at cursor); Cancel/LostFocus/
DisplayChanged/TimedOut return Cancelled; Wheel steps `ZOOMS` (Redraw).

**Windows part, `crates/snipdeck-platform/src/windows/eyedrop.rs`** (`WinEyedropper`), one thread per run:
1. `dpi::aware_thread()` (`windows/dpi.rs:34`): the thread is per-monitor-v2 aware, so every coordinate
   below (cursor, monitor rects, window positions) is in physical pixels on mixed-DPI desktops.
2. Sleep `settle` (150 ms when the Panel was hidden), then `DwmFlush()` so the hidden Panel is gone.
3. `EnumDisplayMonitors` → per `HMONITOR`: `GetMonitorInfoW` (`MONITORINFOEXW`: `rcMonitor`,
   `szDevice`), `GetDpiForMonitor(MDT_EFFECTIVE_DPI)` → `scale` (as `screen.rs:63`).
4. Capture each monitor separately (no buffer spans gaps or negative origins): `GetDC(None)`,
   `CreateCompatibleDC`, `CreateDIBSection` (32 bpp, `BI_RGB`, negative height = top-down,
   `DIB_RGB_COLORS`), `SelectObject`, `BitBlt(.., SRCCOPY | CAPTUREBLT)` (CAPTUREBLT includes layered
   windows), copy the bits (the alpha byte is undefined: ignored), `DeleteObject`/`DeleteDC`/`ReleaseDC`.
   Peak memory 33 MB per 4K monitor, freed when the overlay closes.
5. HDR flag per monitor: `GetDisplayConfigBufferSizes` + `QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS)`,
   match the source's `DISPLAYCONFIG_SOURCE_DEVICE_NAME` to `szDevice`, then `DisplayConfigGetDeviceInfo`
   with `DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO` → `advancedColorEnabled`.
6. One overlay per monitor: `RegisterClassExW` (cursor `IDC_CROSS`, no background brush),
   `CreateWindowExW(WS_EX_TOPMOST | WS_EX_TOOLWINDOW, .., WS_POPUP, rcMonitor...)` (created on its
   monitor, so it gets that monitor's DPI and no `WM_DPICHANGED`), `ShowWindow(SW_SHOW)`, then
   `SetForegroundWindow` + `SetFocus` on the window under the cursor.
   If `GetForegroundWindow()` is not ours 100 ms later (foreground lock), `RegisterHotKey` (no modifiers,
   thread-scoped ids) for Esc, Enter, Space and the arrows until the run ends, so the keyboard still works.
7. `WM_PAINT`: `BitBlt` the frozen shot for the update region; the loupe is drawn into an off-screen DC
   and blitted once: `SetStretchBltMode(COLORONCOLOR)` + `StretchBlt` of the 11×11 source (gaps drawn
   as a checker), 1 px grid at zoom ≥ 8, the centre cell outlined 2 px white inside 1 px black (visible on
   any colour), a readout bar (`#1e1e1e`, white `DrawTextW` Segoe UI 12 pt × scale: a 16 px swatch, the
   `#rrggbb` text, the translated hint "Click or Enter picks · Esc cancels · arrows move"; on an HDR
   monitor a second line with `hdr_note`). `WM_ERASEBKGND` returns 1. A move invalidates the old and the
   new loupe rect only (`InvalidateRect(.., FALSE)`).
8. Input → `Session::on`: `WM_MOUSEMOVE` (position from `GetCursorPos`, not lParam, so all windows agree),
   `WM_LBUTTONDOWN` Pick, `WM_RBUTTONDOWN` Cancel, `WM_MOUSEWHEEL`, `WM_KEYDOWN` (VK_LEFT.., Shift from
   `GetKeyState`, VK_RETURN/VK_SPACE Pick, VK_ESCAPE Cancel), `WM_HOTKEY` (step 6 fallback),
   `WM_ACTIVATEAPP(FALSE)` LostFocus, `WM_DISPLAYCHANGE` DisplayChanged, `SetTimer` 120 s TimedOut.
   `Step::MoveCursor` → `SetCursorPos` (physical). After each step `SetWindowTextW` "Eyedropper,
   #3a7bd5" + `NotifyWinEvent(EVENT_OBJECT_NAMECHANGE)` for Narrator (unverified).
9. End: `UnregisterHotKey`, `DestroyWindow` each, `UnregisterClassW`, drop the shots, call `done`.
   A failed capture (secure desktop, locked session: `BitBlt` fails) → `Failed` with the Win32 error text.

What is returned: the 8-bit values GDI composes, i.e. what apps drew, before Night light and colour
filters (which act after composition; unverified). **Protected content** (PlayReady video, windows with
`WDA_MONITOR` affinity) captures black and is shown black in the frozen overlay, so the user sees what
they get; `WDA_EXCLUDEFROMCAPTURE` windows are absent and show what is behind them. **HDR**: GDI gets
DWM's SDR conversion; SDR app content usually reads its own sRGB values, HDR content is tone-mapped and
clipped. Both unverified: the overlay and the toast say "HDR is on for this screen: values are
approximate". Rejected: Windows.Graphics.Capture (async, capture border on Windows 10, more code for no
SDR gain); DXGI Desktop Duplication (exact FP16 HDR, but D3D11 per output; revisit if HDR users ask).

**App side.** `controller/eyedrop_ctl.rs`: `start_eyedropper()` returns `[HidePanel]` when the Panel is
visible (settle 150 ms, else 0) and calls the host; `finish_eyedropper(outcome)`: Picked → `colour_picked`
(slice 3, §8) and `[ShowPanel, ShowColours]`; Cancelled → `[ShowPanel]` if it was visible; Failed →
toast `EyedropFailed` + Panel back. A second start while running is ignored. `platform.rs` wraps `done`
in `slint::invoke_from_event_loop` to call `finish_eyedropper` on the UI thread. `HotkeyAction::Eyedropper`
calls `start_eyedropper`. Non-Windows builds use no eyedropper (`None`); the button is hidden.

## 7. Test plan (tests first in every slice; fixed clock everywhere)

- **`snipdeck-colour` unit + proptest**: every row of §5.1; rounding edge cases (hue 359.6 → 0, alpha
  127/128, 0.5 parsing); all parse forms and errors; `literal` rejects bare names, bare hex, text with
  anything else, > 64 bytes; name ties; gamut mapping stays in gamut; recents (cap, dedupe, order, bad
  saved entries dropped) with `now_ms` passed in.
- **Settings** (`settings.rs` tests): defaults; round trip of every new field; `"name"` default format
  falls back; one bad recent entry dropped alone; missing `hotkeys.eyedropper` is off; the conflict check
  covers the new chord.
- **Platform portable** (`eyedrop.rs` tests, Linux CI): `sample` across two shots with a negative-origin
  monitor and a gap; mixed scale 1.0 / 1.5 `loupe_rect` flip at all four edges; `Session` nudges, Shift
  ×10, clamping at gaps, zoom wheel limits, every Input → Step; `FakeEyedropper` scripted runs.
- **Controller** (`controller/tests/colours.rs`, fake clipboard/keyboard/clock/eyedropper): format row
  paste with a Target and copy without; Shift copies; toast texts; Ctrl+S calls quick add with the default
  format text and the "no list" toast; recents pushed on use/save/pick only; eyedrop flow hides and
  restores the Panel, copies on pick only when `colour_copy_on_pick`, ignores a second start, Failed toast.
- **Slint goldens** (`crates/snipdeck-ui/tests/colours.rs`, `UPDATE_GOLDEN=1`, look at every PNG): 280,
  320, 420 light; 320 dark; invalid input; Tints and shades; Adjust open; 175 % text; a Panel list with a
  swatch card (List and Cards layouts). Measurements, not only pictures: the 48 px swatch's centre pixel
  is exactly #3a7bd5; a swatch has a 1 px outline pixel; pass/fail marks are different glyphs (compare
  the glyph pixels of ✓ vs ✗, both in `text.primary`); accessible labels and roles from the tree
  (`audit_final1_tree.rs` pattern); Tab order = §3.1; hit boxes ≥ 24 px.
- **Windows wizard** `tools/windows-test/tests/colour.json` (owner runs; unverified until then):
  1. Panel > Colours tab: type `#3A7BD5`, click HEX with Notepad focused: `#3a7bd5` pasted.
  2. Click the eyedropper; the Panel hides; loupe follows the mouse; pick a known colour from a test page
     (`tools/windows-test/` serves a page with #3a7bd5, #ff0000, #ffffff squares): value copied, tab shows it.
  3. Arrow keys move one pixel, Shift+arrow ten; Esc cancels and the Panel comes back.
  4. Two monitors at 100 % and 150 %: the loupe is sharp and the right size on each; pick on both; cross
     the edge with the arrow keys; a monitor left of/above the main one (negative coordinates).
  5. Pick from an HDR monitor (if any): the HDR line shows; record the value read for #3a7bd5.
  6. Play a DRM video in Edge (Netflix/any PlayReady trial): the overlay shows it black.
  7. Set a chord for "Pick a colour from the screen"; press it over Figma/Chrome; Narrator on: what it reads.
  8. Click another app's window mid-pick (focus lost) → cancels; Win+L during a pick → no stuck overlay.
  9. Night light on: the value is unchanged.
  10. Save to list (Ctrl+S): the card shows the swatch; a sensitive copy of it does not.

## 8. Slices (one seam commit, then three in parallel, disjoint files)

**Seam (first commit of slice 1, ≤ 30 min, coordinator merges it before 2 and 3 start).** It compiles,
passes the gate, and owns nothing afterwards except what the table gives slice 1. Contents:
- `crates/snipdeck-colour/` with every §5 signature, bodies `todo!()` (lint-allowed in the seam only);
  `snipdeck-colour = { path = ... }` in workspace deps; dependency added to `snipdeck-app` (Cargo.lock).
- `settings.rs`: the §4 fields with defaults and `field!` lines; `HotkeyAction::Eyedropper` in `ALL`
  (last), `default_chord`/`legacy_chord` `""`, `Hotkeys.eyedropper` + `slot`; arms in every exhaustive
  match: `headless.rs:130`, `settings_binding.rs:516/531/890`, `hotkey_health.rs:65`, `host.rs:480` (5),
  `onboarding_binding.rs:43`, `help_binding.rs:215`, `shortcuts.rs:115`, `controller.rs:818` (calls
  `self.start_eyedropper()`), with `SettingsTexts.action-eyedropper()` = `@tr("Pick a colour from the screen")`.
- `controller/views.rs`: `Effect::ShowColours`; `binding.rs` arm `Effect::ShowColours =>
  window.invoke_open_colours()`, `main.rs:650` arm; `panel.slint`: `public function open-colours() {}`
  and `in property <bool> colours-available` (both no-ops).
- `toast.rs`: `ToastMessage::EyedropFailed`, `ToastMessage::EyedropHdr { hex: String }`,
  `ToastMessage::ColourNotParsed`; `wording.rs::toast_text` arms and `toast_texts.slint` functions.
- New files with signatures only: `controller/colour_ctl.rs`, `controller/eyedrop_ctl.rs`,
  `snipdeck-platform/src/eyedrop.rs` (+ `pub mod`), `windows/eyedrop.rs` (`start` returns
  `Err(PlatformError::Unsupported)`; + `mod`), `colours_binding.rs` (+ `pub mod` in `lib.rs`),
  `crates/snipdeck-ui/ui/colours.slint` (empty `export component ColoursView`).

The controller seam signatures (bodies `todo!()`), fixed so slices 2 and 3 never talk:
```rust
// controller/colour_ctl.rs (slice 3 fills)
pub struct ColourView { pub current: Rgba8, pub other: Rgba8, pub format: Format, pub scheme: Scheme,
    pub recents: Vec<Rgba8>, pub adjust_open: bool, pub eyedrop_available: bool, pub eyedrop_running: bool }
impl Controller<S, C, K, T> {
    pub fn colour_view(&self) -> ColourView;
    pub fn set_colour(&mut self, c: Rgba8);                     // current; no recents push
    pub fn set_contrast_with(&mut self, c: Rgba8);
    pub fn swap_contrast(&mut self);
    pub fn set_colour_format(&mut self, f: Format);             // ignores Name
    pub fn set_colour_scheme(&mut self, s: Scheme);
    pub fn set_colour_adjust_open(&mut self, open: bool);
    pub fn use_colour(&mut self, c: Rgba8, f: Format, mode: UseMode) -> UseResult; // pipeline::use_item, toast, recents
    pub fn save_colour_to_list(&mut self, c: Rgba8, origin: Origin) -> bool;       // quick_add(format(c, default))
    pub fn forget_recent_colour(&mut self, c: Rgba8);
    pub(crate) fn colour_picked(&mut self, c: Rgba8, hdr: bool) -> Vec<Effect>;  // current, recents, copy if setting, HDR toast
}
// controller/eyedrop_ctl.rs (slice 2 fills)
impl Controller<S, C, K, T> {
    pub fn set_eyedropper(&mut self, host: Box<dyn snipdeck_platform::eyedrop::Eyedropper>, texts: EyedropTexts);
    pub fn set_eyedrop_texts(&mut self, texts: EyedropTexts);    // after a language change
    pub fn eyedropper_available(&self) -> bool;
    pub fn eyedropper_running(&self) -> bool;
    pub fn start_eyedropper(&mut self) -> Vec<Effect>;           // uses the stored texts
    pub fn finish_eyedropper(&mut self, outcome: snipdeck_platform::eyedrop::EyedropOutcome) -> Vec<Effect>;
}
```
`EyedropTexts` come from a Slint global `ColourTexts` (`eyedrop-hint()`, `eyedrop-hdr()`) that slice 3
writes in `colours.slint`; slice 2 reads them in `platform.rs` with `ColourTexts::get(window)` at
start-up and on a language change, and hands them to the controller (`set_eyedropper`,
`set_eyedrop_texts`), which passes clones to `Eyedropper::start`. The seam adds the global with both
functions returning their `@tr` English text.

| Slice | Owns after the seam | Acceptance |
|---|---|---|
| **1. Colour maths + settings** | `crates/snipdeck-colour/**`, `settings.rs`, `settings_binding.rs`, `settings.slint` (Settings > Panel: "Show Colours tab", "Copy a colour picked from the screen"; the Shortcuts row appears from `ALL`) | §5.1 vectors and properties pass; settings tests; `cargo doc` clean; no `todo!()` left; settings goldens updated and looked at |
| **2. Eyedropper + app wiring** | `snipdeck-platform/src/eyedrop.rs`, `windows/eyedrop.rs`, `windows/mod.rs`, `snipdeck-platform/Cargo.toml` (windows features: `Win32_Graphics_Dwm`, `Win32_Devices_Display`), `controller/eyedrop_ctl.rs`, `controller.rs`, `platform.rs`, `host.rs`, `main.rs`, `controller/tests/eyedrop.rs`, `tools/windows-test/tests/colour.json` + its test page | portable tests (§7) on Linux; controller eyedrop tests with `FakeEyedropper`; Windows `cargo check` in CI; `reviewer` on the unsafe Win32 code (every `unsafe` has SAFETY, every handle freed on every path); wizard steps 2-9 written |
| **3. Colours tab + cards + i18n** | `colours.slint`, `panel.slint`, `components/item-card.slint`, icons (`palette.svg`, `eyedropper.svg`, `arrows-left-right.svg` from Phosphor Regular, then `python3 scripts/ui_icons.py`), `controller/colour_ctl.rs`, `controller/tests/colours.rs`, `colours_binding.rs`, `binding.rs`, `commands.rs` + `controller/commands_ctl.rs` (palette "Colours", "Pick a colour from the screen"), `toast.rs`, `wording.rs`, `toast_texts.slint`, translation catalogs, `crates/snipdeck-ui/tests/colours.rs` + goldens, `DESIGN.md` (§4 Colour swatch component), `docs/SPEC.md` (5.1 Colours tab, §6 table row "Colours tab and eyedropper ✓ ✓") | goldens and measurements of §7; controller tests; every string `@tr` and `python3 scripts/i18n.py` passes for the six languages (CSS keywords and format labels untranslated); `ux-auditor` and `blind-tester` on the running tab; Narrator labels listed in the PR |

Order of merge: seam, then any order; slice 3's goldens are taken after slice 1 is merged (they show the
real format text). Each slice keeps `.claude/remaining/<agent>.md`, commits at least every 20 minutes and
runs `scripts/gate.sh --staged` before each commit.

## 9. Unverified until measured or run on Windows

Foreground activation of the overlay after a hotkey (and the `RegisterHotKey` fallback); GDI capture
values on HDR monitors and with Night light; DRM regions captured black; `CAPTUREBLT` cost and flicker;
mixed-DPI loupe size; Narrator reading the overlay title and the swatch labels; paste of `#3a7bd5` into
Figma's hex field (Figma may want it without `#`: if the owner reports that, add a "HEX (no #)" copy
in the row menu, not a new format); time to first overlay frame on three 4K monitors (target < 250 ms,
logged, not a timed test).

## Later (not in these slices)

"Copy list as CSS variables" for a palette list; swatches in Main window rows and the Picker; "Open in
Colours" on a swatch card's menu; HSL wheel option; DXGI HDR capture; APCA contrast (WCAG 3 draft).

## Sources

- WCAG 2.2 contrast (1.4.3, 1.4.6, 1.4.11) and relative luminance: https://www.w3.org/TR/WCAG22/#dfn-relative-luminance
- CSS Color 4 (`oklch()`, named colours, gamut mapping): https://www.w3.org/TR/css-color-4/
- OKLab matrices: https://bottosson.github.io/posts/oklab/
- BitBlt / CAPTUREBLT: https://learn.microsoft.com/windows/win32/api/wingdi/nf-wingdi-bitblt
- CreateDIBSection: https://learn.microsoft.com/windows/win32/api/wingdi/nf-wingdi-createdibsection
- Advanced colour info: https://learn.microsoft.com/windows/win32/api/wingdi/ns-wingdi-displayconfig_get_advanced_color_info
- SetWindowDisplayAffinity: https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity
- (Sources listed as the standard references; none was fetched for this note.)
