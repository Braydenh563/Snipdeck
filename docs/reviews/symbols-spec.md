# Symbols library: build spec (2026-10-25)

Owner ask (verbatim): "Can we include an emoji, japanese emoticons, and icons library as well so the
suer can easily access them and quick add them to a list ot copy them or download them idk."
Goal: small, fast, polished; better than Win+. for a Snipdeck user, not bigger. Sizes are logical px
at 100 %, colours are DESIGN.md tokens, layout follows `docs/reviews/panel-v2-spec.md` (P2 below).
Nothing here was run: no cargo, no Windows. Claims marked *unverified* need a measurement.

## 0. What we have to beat, and what we take

- **Win+. (Windows emoji panel)** already offers emoji, kaomoji and symbols with skin tones. Its gaps,
  which are our reason to exist: no "save to my list", no icons, no SVG/PNG export, it pastes only at
  the caret it opened from, and it is not where the user's snippets are. We take its three-kind split
  and its "recently used first" row. We do not copy its skin-tone picker (deferred, D7).
- This feature **replaces** competitor idea 12 ("Starter boards": a Symbols board and an Emoji board,
  `docs/reviews/competitors-2026-10-18.md`). One library, not seeded boards that go stale (CLAUDE.md
  "replace, do not layer"). Record that in `.claude/now.md` Decisions in slice 7.
- **Licence of the app**: the brief says "MIT/Apache app"; the repo is **GPL-3.0-only** (ADR-0005,
  `Cargo.toml` `license`). Every source chosen below is permissive and GPL-3.0-compatible, so this
  changes nothing except the wording of the notices.

## 1. Decisions (written once, with reasons)

**D1. Emoji data: Unicode `emoji-test.txt` (v18.0) for the set and order, CLDR annotations for names
and keywords.** `emoji-test.txt` gives groups, subgroups, order and the fully-qualified sequences
(header: "Version: 18.0", "For terms of use and license, see https://www.unicode.org/terms_of_use.html").
CLDR `annotations` + `annotationsDerived` give the short name (tts) and keywords per language; CLDR
files carry `SPDX-License-Identifier: Unicode-3.0`. Unicode License v3 is OSI-approved and permissive;
obligation: keep the copyright and permission notice with the data. Languages bundled: en plus the six
UI languages (de, es, fr, ja, pt, zh-Hans: CLDR `pt` and `zh` locales), so a Japanese user can type
ねこ. Search always matches English too.

**D2. Emoji art: Microsoft Fluent Emoji, "Flat" style, pre-rasterised to 48 px PNG, MIT.**
`microsoft/fluentui-emoji` LICENSE: "MIT License / Copyright (c) Microsoft Corporation." Reasons:
(a) it is the look Windows 11 gives the pasted character, so what the grid shows is what the target app
shows (Windows-first); (b) MIT, same obligation as Phosphor; (c) Flat is drawn for small sizes.
Rejected: Twemoji (CC-BY 4.0, GPL-compatible per FSF, but attribution must be visible and the look does
not match Windows), Noto Emoji (OFL, Android look), OpenMoji (CC BY-SA: share-alike on art, avoid).
Coverage is *unverified*: third parties report ~1,500 to ~3,100 assets and the repo states no Unicode
version. Rule: **the bundled set is the intersection of `emoji-test.txt` and Fluent Flat**; the script
prints what it dropped. A dropped emoji is an emoji Windows probably cannot show either. Country flags
fall out this way (Segoe UI Emoji has none; Windows shows two letters), leaving ~6 flags in "Symbols".

**D3. Why bundled art and not the system font: Slint 1.18.1's software renderer draws glyphs as
monochrome alpha masks.** `i-slint-renderer-software-1.18.1/fonts/vectorfont.rs:207-208` rasterises with
`swash::scale::Render::new(&[Source::Outline]).format(Format::Alpha)`: no `ColorOutline` (COLR) or
`ColorBitmap` source, so Segoe UI Emoji's COLR layers are ignored and each emoji draws as a one-colour
silhouette in the text colour. No upstream fix is in reach from this repo. **Fallback**: if a cell's art
is missing or fails to decode, the cell draws the character as Text (the monochrome silhouette from the
base outline; *unverified on Windows*). Never a blank cell.

**D4. Kaomoji: hand-curated, ~300, authored in this repo (GPL-3.0 like the app).** Public lists
(`emoticon-data` on npm says MIT; `node-kaomoji` and most JSON sets are scraped from sites with no
licence on the data) have unclear provenance. A short curated set is also the better product: ~10
categories (joy, love, sad, angry, surprise, shrug, greet, sorry, animals, actions such as table flip),
each kaomoji with 2 to 5 English tags. Source of truth: `data/symbols/kaomoji.tsv`, reviewed by hand.

**D5. Icons: Phosphor Regular, all ~1,512 glyphs, MIT.** It is already the app's only icon family
(DESIGN.md §3, `crates/snipdeck-ui/icons/LICENSE-phosphor.txt`, "Copyright (c) 2023 Phosphor Icons"),
so no new licence, no second visual language, and the user gets the icons they see in Snipdeck.
Measured here: the 74 committed Phosphor SVGs are 32,806 bytes (443 B each), 8,831 B gzipped; so the
full set is ~670 KB raw, ~180 KB deflated. Tags come from Phosphor's own catalog (MIT, same repo
family); pinned in `scripts/symbols.lock`. Lucide (ISC) and Tabler (MIT) were not considered further:
a second family breaks DESIGN.md's one-family rule.

**D6. One entry point: a "Symbols" tab in the Panel tab row.** A smiley tab (Phosphor `smiley`), 24×32,
right of the Recent clock tab (P2 §2.2), same states as the clock tab. Also reachable by Ctrl+Tab
cycling (it behaves as the Recent tab does) and a command-palette entry "Symbols" (Ctrl+K), which are
routes to the same tab, not new surfaces. **Not** in the Main window, not in the Picker, no global
hotkey in this spec. Reason: the Panel is the 2-second window and the one with a paste target; the Main
window manages the library. A setting "Show Symbols tab" (on by default) sits in Settings > Panel for
users who want the tab row back.

**D7. Skin tones deferred.** Default (yellow) only. Reason: ~5x the people art (~+1.2 MB est.) plus a
long-press/variant UI; Win+. covers it today. Revisit with owner feedback. Hair/gender ZWJ variants that
are separate fully-qualified entries in `emoji-test.txt` stay (they are their own emoji).

**D8. Symbols search is its own box, not Panel search.** P2 D5 makes Panel search span every board;
mixing ~3,300 symbols into those results would bury the user's own items. Inside the Symbols tab one
field searches all three kinds.

**D9. Actions are the item actions, nothing new to learn.** Click / Enter pastes into the Target (same
route as a snippet); Shift+click / Shift+Enter copies; no Target = click copies (P2 D3). "Add to list"
(Ctrl+S, and in the menu) saves the symbol as a normal item in the list last shown in the Panel.
Icons add three menu entries: Copy SVG, Copy PNG, Save as… Emoji and kaomoji have no export: they are
text. That is the whole action set.

**D10. Recents: most recently used, 24 max, shown first.** A "Recently used" section opens every kind's
grid (only that kind's recents) and the browse view of "All" results. Ordered by last-used time from
the injected `Clock` (`crates/snipdeck-app/src/host.rs:17`), persisted in settings as
`symbols.recent = [{id, used_at}]`. Not frecency: Win+. is MRU and users expect it. Ids that vanish
after a data update are dropped on load.

## 2. Data model and code placement

**New crate `crates/snipdeck-symbols`** (pure Rust, no Slint, no platform): catalog, search, recents,
art decode, icon render. `snipdeck-app` owns the controller; `snipdeck-ui` owns the view. Reason: the
logic is testable without a window, and `snipdeck-core` stays about the user's library.

```
SymbolId   = Emoji("1f469-200d-1f4bb") | Kaomoji("table-flip") | Icon("arrow-clockwise")   // stable, persisted
Symbol     { id, kind, text: &str /* what is pasted; icons: none */, name: &str, group: u8, keywords: &[&str] }
Catalog    { symbols: Vec<Symbol>, groups: [Group] }      // built once, OnceLock, on first open of the tab
search(&Catalog, query, lang, kind: Option<Kind>) -> Vec<Hit>   // Hit = index + rank
Recents    { push(id, now), list(kind) -> Vec<SymbolId> }  // pure, clock passed in
art(id, px) -> Option<Vec<u8>>  /* RGBA */                         // decode + box-filter resample to device px
icon_svg(id) -> String ; icon_png(id, px, rgba) -> Vec<u8>       // resvg (already in Cargo.lock, 0.48.1)
```

**Generated data, committed** (`crates/snipdeck-symbols/data/`), embedded with `include_bytes!`:
| File | Content | Budget |
|---|---|---|
| `emoji.tsv.z` | seq, group, subgroup, en name, keywords (deflate) | ≤ 60 KB |
| `names-<lang>.tsv.z` | seq, name, keywords for each of the 6 UI languages | ≤ 50 KB each |
| `kaomoji.tsv.z` | id, text, group, tags (copied from `data/symbols/kaomoji.tsv`) | ≤ 10 KB |
| `icons.bin.z` | name, tags, path data only (the `<svg>` wrapper is rebuilt) | ≤ 200 KB |
| `emoji48.bin` | index (seq → offset, len) + one 48×48 palette-quantised, oxipng'd PNG each | ≤ 2.5 MB |

Deflate via `miniz_oxide` (already in the tree through `png`), PNG decode via the existing `png` crate:
**no new crates except `resvg` as a direct dep at the version Slint already pulls**. Binary cost
≤ 3.2 MB against SPEC's installer < 15 MB; slice 2 measures and stops if the art is over 2.5 MB (then
drop to 40 px). One art size (48 = 24 logical at 200 %), resampled down at decode to the cell's physical
size, so 100/125/150 % are sharp without three sets.

**Startup cost ~0.** `include_bytes!` data sits in the image and is paged in only when read. Nothing is
inflated or parsed until the Symbols tab is first shown (`OnceLock`); estimate < 10 ms on first open
(*unverified*; slice 3 adds a logged measurement, not a timed test). Art is decoded per visible cell
into an LRU of 256 cells (48×48×4 = 9 KB each, ≤ 2.4 MB). RAM: tab never opened = 0; open ≤ 5 MB
(SPEC idle budget < 40 MB). Search is a linear scan of ~3,300 entries with pre-folded keywords:
sub-millisecond, no index structure.

**Generator `scripts/symbols.py`** (stdlib + Pillow + resvg CLI or cairosvg, owner/dev machine only):
downloads the pinned inputs listed with sha256 in `scripts/symbols.lock` (emoji-test.txt 18.0, CLDR
release tag, fluentui-emoji commit, Phosphor core commit), refuses on a hash mismatch, writes the files
above, prints counts and the dropped list. `python3 scripts/symbols.py --check` (CI, no network)
verifies: outputs decode, counts match `data/manifest.json`, sizes are within budget, every source in
the lock has a section in `THIRD_PARTY_NOTICES.md` with its licence text.

**Notices.** New `THIRD_PARTY_NOTICES.md` at the root, shipped beside the exe and opened from About:
Unicode-3.0 full text (emoji-test.txt, CLDR), MIT for Fluent Emoji (Microsoft) and Phosphor (reusing
`LICENSE-phosphor.txt`). About gains one line: "Emoji art: Microsoft Fluent Emoji (MIT). Emoji names:
Unicode CLDR (Unicode License v3). Icons: Phosphor (MIT)." Do not use "Fluent" as a feature name.
`deny.toml` covers crates only, so the script is the check for data.

## 3. Paths through the existing code

- **Paste/copy**: build `Payload::new(vec![Data::Text(symbol.text)])` and call
  `snipdeck_platform::pipeline::use_item(env, &payload, mode, &route)`
  (`crates/snipdeck-platform/src/pipeline.rs:143`), with the same refocus-and-wait as
  `Controller::run_use` (`controller/use_ctl.rs:405`). Do not fake an `ItemId`; do not count uses in
  the store. New `controller/symbol_ctl.rs`: `use_symbol(id, UseMode) -> UseResult`, same toasts
  ("Pasted into Outlook" / "Copied").
- **Add to list**: `Controller::quick_add(text, Some(ItemKind::Text), false, Origin::Panel)`
  (`use_ctl.rs:217`), which already picks the shown list via `capture::default_list` and toasts the
  "no list" case. Icons are added as an **Image** item: a 64 px PNG in `text.primary`, through the same
  store path the item editor uses for Image items (`item_editor.rs`, PNG-only source).
- **Copy SVG**: `Data::Text(svg)` with `fill="currentColor"` kept (works in HTML, Figma, Office SVG
  paste). **Copy PNG**: `Data::Png(icon_png(id, 64, text.primary))`; the Windows backend synthesises
  CF_DIB from PNG (`payload.rs`); whether transparency survives in Word/Paint is *unverified*.
- **Save as…**: `FilePicker` (`file_dialog.rs:49`) only opens files. Add `save(kind: SaveKind,
  suggested: &str, owner) -> Result<Option<PathBuf>>` (IFileSaveDialog on Windows, *unverified*), with
  `SaveKind::{Svg, Png}`. The size is picked before the dialog, in a menu submenu
  "Save as PNG ▸ 24 / 32 / 48 / 64 / 128 / 256" and "Save as SVG". File name = icon name.
- **Recents** live in settings (`settings_ctl.rs`), written on each use, debounced like other settings.

## 4. UI (Panel widths 260-420; P2 tokens)

Panel with the Symbols tab active, top to bottom:

| Band | Height | Content |
|---|---|---|
| Header | 40 | unchanged (P2 §2.1) |
| Tab row | 32 | Symbols tab active: 2 px `accent.text` underline (no list hue) |
| Search | 40 | SearchField h32, margin 4 8, radius 6, placeholder "Search emoji, kaomoji, icons"; always shown |
| Kind tabs | 32 | "All · Emoji · Kaomoji · Icons", `type.base`, P2 §2.2 tab style, 1 px `border.subtle` bottom |
| Group strip | 28 | Emoji only: 9 group buttons (smiley, hand-waving, paw-print, hamburger, airplane, soccer-ball, lightbulb, heart, flag), glyph 16 `text.secondary`, share the width, 24-32 each |
| Grid | flex | sections; DeckEdge overflow (P2 §2.5) |
| Footer | 32 | left: name of the hovered/selected symbol (`type.sm` `text.secondary`, e.g. "grinning face"); right: P2's target line ("Click pastes into Outlook") |

Chrome = 204 (176 off the Emoji kind). At 320 × 560 the grid is 356: 8 rows of 7 = 56 emoji.

**Cells (emoji, icons)**: 40 × 40 hit box, radius 6, glyph 24 (emoji art; icons in `text.primary`).
Columns = floor((W − 16) / 40), leftover split into the side padding: **260 → 6, 280 → 6, 320 → 7,
360 → 8, 420 → 10**. Rest transparent on `bg.surface`; hover `bg.hover`; selected `bg.selected`;
focus-visible adds the 2 px inset `focus.ring` (P2 §2.5); pressed = the card-pull check (P2 §2.5),
140 ms. **Kaomoji**: chips h32, radius 6, padding 0 8, `type.base`, 2 columns with 4 gap; a kaomoji
over 10 characters (marked `wide` by the script) spans both. Kaomoji need CJK and box-drawing glyphs
that Plex lacks: rely on the system fallback (Yu Gothic UI, DESIGN §3; *unverified on Windows*).
**Section headers**: h24, `type.xs` 500 uppercase `text.secondary`, padding 0 8 ("RECENTLY USED",
"SMILEYS & EMOTION"). **All** kind shows Recent, then the first 2 rows of each kind with a "Show all"
link that switches the kind tab. With a query: one flat ranked grid per kind section, kaomoji chips in
their own section; empty result = EmptyState "No symbols match 'xyz'" (P2 §2.7, no button).
Virtualisation: the Slint model is grid **rows**, rebuilt when the column count changes.

**Keyboard** (one focus stop for the grid, roving selection like the list):
| Key | Action |
|---|---|
| type (anywhere in the tab) | goes to the search field |
| arrows | move in 2D; Up from the first row goes to the strip/tabs |
| Home / End, PageUp / PageDown | first/last, ± a visible page |
| Ctrl+Left / Ctrl+Right | previous / next kind tab |
| Enter / Shift+Enter | paste / copy |
| Ctrl+S | add to the shown list |
| Menu, Shift+F10, right-click | Paste, Copy, Add to "List", (icons) Copy SVG, Copy PNG, Save as ▸ |
| Esc | clear search; else hide the Panel (P2 §2.10) |
Focus order: search, kind tabs (one stop), group strip (one stop), grid, footer. Group buttons scroll
the grid to the section and move the selection to its first cell.

**a11y**: each cell `accessible-role: list-item`, `accessible-label` = the name in the UI language
(CLDR tts; kaomoji = its first tag, e.g. "table flip"; icon = humanised name "arrow clockwise"), with
`accessible-description` "Emoji" / "Kaomoji" / "Icon". Tooltip (500 ms, DESIGN §4) shows the name plus
"Click pastes · Shift+click copies". Group buttons are icon-only: tooltip and label with the group
name (DESIGN §4 rule). Every string through `scripts/i18n.py` in all six languages.

DESIGN.md amendment (slice 7): "No emoji as icons" stays and is clarified: **emoji are content in the
Symbols tab, never UI chrome.**

## 5. Search

Fold query and keywords once (lowercase, NFKD minus marks, full-width to half-width, katakana to
hiragana) so "cafe" finds ☕ via "café" and カ/か match. Split the query on spaces; every term must
prefix-match some word of the name or keywords (AND). Rank: exact name, name prefix, keyword exact,
keyword prefix, substring (min 3 chars), then catalog order. Pasting an emoji into the box finds it.
Keywords searched: UI language + English. Kaomoji: English tags plus category names in the UI
language. No shortcodes (`:smile:`): not a Windows habit, and gemoji is another source to track.

## 6. Tests (tests first in every slice; fixed clock everywhere)

**Logic (`crates/snipdeck-symbols/tests/`)**
- `catalog_counts_match_the_manifest`, `every_symbol_has_a_name_in_every_bundled_language_or_falls_back_to_en`
- `ids_are_stable_across_regeneration` (golden list of 20 ids)
- `search_cat_ranks_cat_face_before_cat_tagged_emoji`, `search_folds_accents_and_kana` (café, ねこ/ネコ),
  `search_terms_are_anded`, `search_finds_an_emoji_pasted_as_the_query`, `kaomoji_shrug_finds_the_shrug`,
  `icon_trash_finds_trash_and_tags`, `empty_query_is_browse_order`
- `recents_are_most_recent_first_without_duplicates` (FixedClock t0, t1, t2), `recents_cap_at_24`,
  `recents_drop_unknown_ids_on_load`, `recents_round_trip_through_settings`
- `art_decodes_and_resamples_to_24_30_36_48`, `missing_art_falls_back_to_text`,
  `icon_png_is_the_requested_size_and_colour`, `icon_svg_keeps_current_color`
- `nothing_is_parsed_before_first_use` (a counter on the `OnceLock` initialiser)

**Controller (`crates/snipdeck-app/src/controller/tests/symbols.rs`, existing fakes)**
- click with a Target pastes `Data::Text("😀")` via the paste route; Shift copies; no Target copies
- add to list creates a Text item at the top of the shown list (and the "no list" toast)
- icon: Copy SVG writes text, Copy PNG writes a decodable 64×64 PNG, Save as writes the file the fake
  `FilePicker::save` returns, Cancel writes nothing; each use pushes a recent with the fixed `now`

**UI (`crates/snipdeck-ui/tests/symbols.rs`, goldens in `tests/golden/`)**
- goldens: `symbols-emoji-{280,320,420}-{light,dark}.png`, `symbols-kaomoji-320-light.png`,
  `symbols-icons-320-dark.png`, `symbols-search-empty-320-light.png`, `symbols-focus-320-light.png`
- measurements (not just pixels): column count and cell bounding box (40 × 40) per width, glyph 24,
  focus ring pixel = `focus.ring`, footer shows the selected name, group click scrolls to the section
- arrow-key navigation moves the selection as the 2D table above (row wrap, Up into the strip)

**Licence**: `scripts/test_symbols.py` (like `test_i18n.py`) unit-tests the parsers on a 30-line
fixture of emoji-test.txt and CLDR XML; `symbols.py --check` runs in CI and in `scripts/gate.sh --full`.

## 7. Slices (each one commit, tests first, `scripts/gate.sh --staged` before it)

1. **Data pipeline (text).** `scripts/symbols.py`, `symbols.lock`, `test_symbols.py`,
   `data/symbols/kaomoji.tsv` (~300, hand-written), generated `emoji/names/kaomoji/icons` files,
   `manifest.json`, `THIRD_PARTY_NOTICES.md`, `--check` in CI. Prints the Fluent coverage numbers.
2. **Emoji art.** Rasterise Fluent Flat to 48 px, quantise, oxipng, `emoji48.bin`. Gate: ≤ 2.5 MB or
   fall back to 40 px; record the measured size and count in the commit message.
3. **`snipdeck-symbols` crate.** Catalog (lazy), search, recents, art decode/resample, icon SVG/PNG
   (resvg pinned to the Cargo.lock version). All logic tests above. No UI.
4. **Controller.** `symbol_ctl.rs`: use, add to list, copy SVG/PNG, recents in settings;
   `FilePicker::save` + `SaveKind` with the fake and a Windows IFileSaveDialog implementation.
5. **Panel UI.** Symbols tab, search, kind tabs, group strip, grid, chips, footer name, keyboard,
   a11y, goldens at 280/320/420. Independent check: `ux-auditor` and `blind-tester`.
6. **Menu and export UI.** Context menu, Save as submenu, toasts, tooltips, command-palette entry,
   Settings > Panel "Show Symbols tab". `reviewer` on slices 4 and 6 (clipboard and file writes).
7. **Docs and Windows check.** DESIGN.md (§4 Symbols cell, §7 emoji-as-content), SPEC 5.1 (tab),
   About line, i18n strings for six languages, `.claude/now.md` decision (replaces starter boards),
   `tools/windows-test/` steps for the owner: paste 😀 and a ZWJ emoji into Notepad and Word; kaomoji
   glyph fallback; Copy PNG into Word and Paint (transparency); Save as dialog; monochrome fallback
   by deleting one art entry in a debug build; 125 % and 150 % sharpness.

## 8. Unverified until measured or run on Windows

Fluent coverage of Emoji 18.0 (slice 1 prints it); art size (slice 2); first-open parse time; the
monochrome fallback actually drawing Segoe UI Emoji's base outline; kaomoji fallback fonts; CF_DIB
transparency for Copy PNG; IFileSaveDialog; flags pasting as letter pairs on Windows 10/11.

## Sources

- Unicode emoji-test.txt v18.0 header: https://www.unicode.org/Public/emoji/latest/emoji-test.txt
- Unicode License v3: https://spdx.org/licenses/Unicode-3.0.html ; FAQ https://www.unicode.org/faq/unicode_license
- CLDR annotation charts: https://www.unicode.org/cldr/charts/48/annotations/
- Fluent Emoji MIT licence: https://raw.githubusercontent.com/microsoft/fluentui-emoji/main/LICENSE ;
  repo https://github.com/microsoft/fluentui-emoji ; coverage figures (third party, unverified):
  https://best-of-web.builder.io/library/microsoft/fluentui-emoji , https://superdevpro.com/icons/fluent-emoji-flat
- Twemoji CC-BY 4.0 attribution: https://ctan.org/tex-archive/macros/latex/contrib/twemojis
- CC BY 4.0 / GPL compatibility (FSF list, quoted second-hand):
  https://r-mailing-lists.thecoatlessprofessor.com/lists/r-devel/msg/msg-2e69c48152a5 ;
  CC BY-SA one-way: https://creativecommons.org/2015/10/08/cc-by-sa-4-0-now-one-way-compatible-with-gplv3/
- Windows GDI lacks COLRv1: https://blog.yuo.be/2026/03/26/im-not-a-fan-of-how-direct2d-renders-windows-11-emojis/
- Slint software renderer glyph path: local source `i-slint-renderer-software-1.18.1/fonts/vectorfont.rs:207`
- Phosphor count/licence: https://composables.com/icons/icon-libraries/phosphor ,
  https://www.nuget.org/packages/Enigma.Icons.Phosphor
- Kaomoji lists considered: https://npmjs.com/package/emoticon-data ,
  https://codeclimate.com/github/omnidan/node-kaomoji/README.md/source
