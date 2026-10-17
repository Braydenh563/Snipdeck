# Spike S3: long text, 10k-row list and fuzzy search in Slint

- Date: 2026-10-17
- Code: [`spikes/s3-longtext/`](../../spikes/s3-longtext/) (standalone, throwaway crate)
- Relates to: SPEC §5.3, §5.4, §9 (search over 10k items < 16 ms, idle RAM < 40 MB), §13.3 · DESIGN "ItemCard anatomy"
- Versions: Slint 1.18.1 (`renderer-software`, fluent style), nucleo 0.5.0 / nucleo-matcher 0.3.1, ropey 1.6.1, rustc 1.97

## Status

Everything below was **measured on Linux** in a cloud VM (4 vCPU Intel Xeon @ 2.80 GHz, shared and noisy), with
Slint's **software renderer** drawing into an in-memory buffer through a custom `slint::platform::Platform`.
Nothing was run on Windows. The [caveats](#what-transfers-to-windows-and-what-does-not) section says which numbers
should carry over to Windows and which won't. `cargo fmt --check`, `cargo clippy -- -D warnings` (debug and release),
`cargo test` and `cargo check --target x86_64-pc-windows-gnu` all pass.

## Recommendation: GO WITH CHANGES

Slint can do everything here, but the stock `TextEdit` cannot be the only editor. A multi-MB text needs its own
virtualized viewer, and fuzzy search has to run on a bounded key instead of the whole item.

1. **Cap inline `TextEdit` editing at 16 KB.** Every keystroke in a `TextEdit` re-shapes the whole text. The cost grows
   linearly, at about 0.65–1.1 ms per KB: 8 KB ≈ 9–11 ms, **16 KB ≈ 15–21 ms**, 32 KB ≈ 28–70 ms,
   100 KB ≈ 65–95 ms, 1 MB ≈ 0.65–0.9 s and 5 MB ≈ 3.3–4.9 s per key. 16 KB is the largest size at which typing
   still fits in about one 60 Hz frame on this CPU.
2. **Above 16 KB, open the virtualized rope viewer** (prototype in `viewer.rs`). It stayed at **≤ 12 ms for every
   interaction up to 5 MB**, including scrolling, typing, caret movement, select-all, shift-selection, find-next and
   click. A 5 MB text loads in 18–55 ms and adds ~7 MB of RSS. The same 5 MB in a `TextEdit` added 373 MB, with a
   738 MB peak. For v1, ship it as a **read-only viewer with find, selection, copy and whole-text transforms**. All of
   the SPEC §5.4 transforms and replace-all run as batch operations on the rope (replace-all at 5 MB: 31–82 ms),
   so they don't need character-level editing. In-place typing above 16 KB can wait for v1.x, once IME, undo and
   accessibility exist for the custom view.
3. **Force `wrap: word-wrap` in `TextEdit` whenever any line exceeds ~1,000 chars.** With wrap off and a line wider
   than 32,767 physical pixels, the software renderer **panics** as soon as that line is selected (details below).
   Separately, no-wrap frames get slow on very long lines: 60 ms per frame for a 1 MB single line and ~300 ms
   for 5 MB.
4. **Use `ListView` for item lists.** It scrolled 10k ItemCard rows at a 2–3 ms median (≤ 8 ms max) and added
   ~9 MB of RSS. Do not put rows in a plain `for` inside a `Flickable`: that measured 298 ms for the first frame and
   69 ms per scroll step.
5. **Fuzzy search fails the < 16 ms budget when it matches whole ~230-char items on one thread.** Queries that match most items took a 22–57 ms median (`json`, which matches 27% of items, took 8 ms). To meet it, (a) fuzzy-match a bounded key (title plus the first ~64 chars of the
   body or OCR, ≈ 110 chars) and leave full-body search to SQLite FTS5, and (b) score off the UI thread on 4
   threads. Together these measured a **4–8 ms median** (max 5–24 ms; see the table). Whatever the speed,
   filtering must never block the UI thread.

**This becomes NO-GO** if Windows testing finds that the stock `TextEdit` is also too slow *below* 16 KB on target
hardware (for example a keystroke over 33 ms at 8 KB). Every editor would then need the custom view, which is a much
bigger job (see [complexity](#fallback-virtualized-rope-viewereditor-complexity)).

## Method

`cargo run --release` (in `spikes/s3-longtext/`) runs every scenario **in its own child process**. That gives a clean RSS
baseline, one Slint platform per process and a 300 s timeout per scenario. The parent prints Markdown tables. To run a
subset, use `-- --only editor:1mb --only search`.

- **Platform:** `MinimalSoftwareWindow` with `RepaintBufferType::NewBuffer`, so every frame repaints the whole window
  as a GPU renderer would. Frames render into a `Vec<PremultipliedRgbaColor>`. There is no event loop:
  `update_timers_and_animations()` and then `draw_if_needed()` run once per measured frame. Input arrives
  through `Window::dispatch_event` (keys, pointer, wheel). Scale factor is 1.0. Fonts are Linux system fonts through
  fontique (DejaVu Sans / DejaVu Sans Mono).
- **Editor window:** 640×720, stock fluent `TextEdit` at font-size 14px, wrap on or off.
  - `set` is the `set_text` call. Slint evaluates lazily, so this only stores the property.
  - `layout` reads `content-height`, which forces shaping and line breaking of the whole `TextInput`.
  - `first_frame` is the first render after that.
  - `keystroke` is dispatching `KeyPressed/Released "x"` at a caret in the middle of the document (set by a click),
    plus the next frame, as a median of 5. With a layout over 2 s, the median is of 2 samples and scrolls use 5 steps.
  - Scroll steps are 60 px (one wheel notch), 30 from the top and 30 from the middle.
  - `find_scan` is `str::match_indices` on the Rust side. `find_select` is `set-selection-offsets` plus a frame.
  - `replace_all` is `str::replace` plus `set_text`, layout and a frame.
- **Text shapes** are deterministic ASCII built from a 120-word vocabulary:
  - `short`: lines of 10–100 chars.
  - `long`: lines of 8–16 KB with spaces, so they can wrap.
  - `single`: one line with no newline, like minified JSON.
- **Viewer (fallback):** the same window size and the same metric names. Text is DejaVu Sans Mono at 13px with 18px rows.
  The wheel goes through Slint dispatch to a `TouchArea` and then to Rust. Keys go through a `FocusScope` to Rust.
  `set` is `Rope::from_str`. `layout` builds the visual-row index.
- **List window:** 400×800 with 10,000 synthetic items (title of 2–6 words, body of 160–240 chars, list name), each
  shown as a 64px "comfortable" ItemCard: number gutter, rule, title, list label and a 2-line preview, using
  real `Text` items with elide and wrap. Scroll steps are 40px ×150, then a jump to the middle and to the end.
  - *Filter* types `d, de, dep, depl, deplo, deploy` and then clears. Each step runs a nucleo rank over all 10k
    (full ~230-char haystacks, one thread), rebuilds the model and renders a frame.
- **Search:** 10k items averaging 230 chars, with 40 iterations per query. Haystacks are pre-converted to `Utf32String`
  ("cached"); "onthefly" converts each `&str` per query. All variants include the full sort by score.
- **Memory:** `VmRSS` and `VmHWM` (peak) from `/proc/self/status`, in MiB. The generator's `String` is dropped
  before `rss_loaded`, so the delta covers only Slint's copy of the text and its layout, or the rope and its index.
- **Render panics** are caught per frame (`catch_unwind`) and counted. Once one happens, `draw_if_needed` stops
  drawing, so every later value in that row is marked `!` and **is not valid**.
- Numbers are from one full run ("run 2"). A second partial run ("run 3": 8–32 KB sweep, search, lists) was used to
  check variance. Where the two differ the ranges above quote both, because the VM is noisy and max values in
  particular jump around.

## Results

### 1. Stock `TextEdit`: load, typing and memory (run 2, ms / MiB)

| text | layout (load) | first frame | keystroke | scroll step (median) | select all | replace all | RSS Δ loaded | RSS peak |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 2 KB short, wrap | 2.2 | 4.9 | 4.0 | 3.3 | 2.7 | 4.4 | +0.9 | 13.4 |
| 4 KB short, wrap | 5.0 | 11.6 | 8.9 | 5.1 | 4.0 | 8.7 | +1.1 | 13.6 |
| 8 KB short, wrap | 10.5 | 13.7 | 10.7 | 3.6 | 3.0 | 11.0 | +1.3 | 14.3 |
| **16 KB short, wrap** | 17.9 | 5.4 | **16.6** (run 3: 16.4) | 3.7 | 2.6 | 18.3 | +2.0 | 15.7 |
| 16 KB long, wrap | 10.5 | 7.0 | 20.8 (run 3: 15.0) | 4.8 | 6.0 | 21.5 | +1.8 | 14.9 |
| 32 KB short, wrap | 25.5 | 4.9 | 28.8 (run 3: 70.4) | 3.5 | 4.3 | 42.6 | +3.0 | 18.1 |
| 64 KB short, wrap | 89.7 | 6.9 | 77.9 | 5.0 | 4.3 | 80.1 | +5.3 | 22.7 |
| 100 KB short, wrap | 107 | 5.8 | 95.3 | 3.4 | 2.7 | 85.5 | +8.0 | 26.9 |
| 100 KB long, wrap | 88.6 | 9.5 | 93.6 | 7.0 | 6.2 | 91.9 | +4.2 | 19.3 |
| 100 KB single, wrap | 63.2 | 6.2 | 73.0 | 4.6 | 6.5 | 68.1 | +9.2 | 26.6 |
| 1 MB short, wrap | 862 | 4.9 | 840 | 3.4 | 3.6 | 931 | +75 | 168 |
| 1 MB short, no-wrap | 881 | 5.5 | 880 | 3.8 | 3.5 | 837 | +75 | 168 |
| 1 MB long, wrap | 644 | 6.7 | 653 | 4.7 | 4.1 | 645 | +30 | 81 |
| 1 MB single, wrap | 892 | 6.6 | 832 | 5.0 | 4.3 | 723 | +89 | 151 |
| 5 MB short, wrap | 4,997 | 7.5 | 4,940 | 5.2 | 11.0 | 5,242 | +373 | 738 |
| 5 MB short, no-wrap | 4,641 | 8.0 | 4,910 | 4.3 | 7.8 | 4,813 | +373 | 738 |
| 5 MB long, wrap | 3,683 | 6.5 | 3,289 | 4.9 | 5.3 | 3,276 | +140 | 336 |
| 5 MB single, wrap | 4,540 | 10.7 | 4,802 | 7.0 | 7.1 | 4,382 | +437 | 596 |

Idle baseline (empty `TextEdit` window, same harness): **12.7 MiB RSS**, first frame 2.7 ms.

What the table shows:
- **Typing cost equals load cost.** `key_dispatch` makes up almost the whole keystroke (for example 4,935 of 4,940 ms
  at 5 MB), and the following frame is only 3–5 ms. Inserting one character invalidates the `TextInput`'s
  layout-cache entry, which is keyed per item (`i-slint-core/textlayout/sharedparley/cache.rs`). The whole text is then
  re-shaped when the caret position is recomputed.
- **Once laid out, scrolling, caret movement, select-all and shift-selection are cheap at every size** (2–11 ms).
  The draw path only visits the visible band of lines.
- `set_text` itself costs ≈ 0 ms. The cost moves to whatever reads layout next: `content-height`, a frame or a
  key event.
- **Memory is the other blocker:** a 1 MB text in a `TextEdit` costs +75 MiB with a 168 MiB peak, nearly 2× the
  whole-app 40 MB budget on its own.

### 2. Stock `TextEdit` with wrap off and long lines: renderer panic and slow frames

| text (no-wrap) | layout | first frame | scroll step | keystroke | render panics |
|---|---:|---:|---:|---:|---:|
| 100 KB long (8–16 KB lines) | 138 | 12.2 | 10.1 | 65.1 | 1 (at select all) |
| 100 KB single line | 0.02 | 7.8 | 7.3 | 72.4 | 1 (at select all) |
| 1 MB long | 613 | 35.5 | 35.1 | 682 | 1 (at select all) |
| 1 MB single line | 0.24 | 60.5 | 66.3 | 864 | 1 (at select all) |
| 5 MB long | 3,290 | 36.3 | 36.6 | 3,389 | 1 (at select all) |
| 5 MB single line | 1.1 | 304 | 322 | 4,471 | 1 (at select all) |

- **Panic:** `called Option::unwrap() on a None value` at `euclid point.rs:471` / `size.rs:335`, raised from
  `SceneBuilder::fill_rectangle` via `sharedparley::draw_text_input`. The software renderer stores physical
  coordinates as **`i16`** (`i-slint-renderer-software-1.18.1/lib.rs:63-66`). A selection highlight on a no-wrap
  line wider than 32,767 px doesn't fit, and the cast unwraps `None`. With wrap on, the highlight is limited to
  visible lines and no panic happened in any scenario. This is worth filing upstream. Slint does not catch it, so in
  the app it would crash the UI thread.
- With no-wrap and 1–5 MB texts of very long lines, every frame costs 35–320 ms even while only scrolling. All glyphs of each
  visible line are processed, and nothing outside the viewport is culled horizontally.
- With no-wrap, a single 100 KB–5 MB line lays out in ≈ 0 ms, but the first frame and the keystrokes pay the full
  shaping cost.

### 3. Fallback: virtualized rope viewer/editor (run 2, ms / MiB)

| text | load (`set`+index+first frame) | scroll step (median / max) | jump to middle | keystroke | arrow down | select all | shift+down | find scan / select | replace all | RSS Δ loaded | RSS peak |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 100 KB short, wrap | 14.7 | 3.0 / 8.2 | 5.1 | 2.9 | 2.6 | 2.5 | 2.7 | 0.16 / 4.9 | 5.6 | +0.7 | 12.4 |
| 1 MB short, wrap | 22.0 | 4.6 / 5.0 | 8.7 | 4.4 | 4.3 | 4.2 | 3.9 | 1.9 / 8.6 | 21.7 | +1.7 | 15.7 |
| 1 MB long, wrap | 14.3 | 4.0 / 5.7 | 6.7 | 5.0 | 3.9 | 5.3 | 3.8 | 1.8 / 9.3 | 12.1 | +1.8 | 16.2 |
| 1 MB single, no-wrap | 3.5 | 0.6 / 1.7 | 0.7 | 0.8 | 0.6 | 0.5 | 0.6 | 1.5 / 1.1 | 6.9 | +1.6 | 16.2 |
| 5 MB short, wrap | 44.6 | 3.0 / 4.1 | 5.2 | 2.6 | 2.7 | 2.9 | 2.6 | 8.3 / 5.4 | 81.8 | +6.7 | 32.5 |
| 5 MB short, no-wrap | 54.6 | 3.0 / 3.3 | 5.9 | 2.9 | 2.8 | 2.7 | 3.1 | 7.6 / 6.8 | 77.5 | +6.4 | 32.2 |
| 5 MB long, wrap | 17.8 | 3.9 / 4.7 | 6.3 | 5.0 | 3.3 | 3.4 | 3.6 | 8.0 / 7.9 | 33.9 | +6.2 | 31.0 |
| 5 MB single, wrap | 20.9 | 3.9 / 8.0 | 6.0 | 5.0 | 3.4 | 3.4 | 3.4 | 7.8 / 7.1 | 30.6 | +6.3 | 30.9 |

All 18 viewer scenarios had **no render panics and no interaction over 12 ms** (load and replace-all excluded). The peak RSS (~31 MiB at 5 MB) is
transient: during load the generator `String` and the rope both exist, and replace-all holds one temporary `String` copy.

### 4. 10,000 ItemCard rows (run 2; run 3 in brackets)

| variant | set model | first frame | scroll step median / p95 / max | jump mid / end | filter step median (match + model + frame) | filter max | RSS Δ (cards) |
|---|---:|---:|---:|---:|---:|---:|---:|
| **`ListView`** (built-in virtualization) | 12.0 [8.2] | 16.7 [13.8] | **2.1 / 2.5 / 7.8** [3.4 / 3.9 / 4.1] | 4.9 / 5.2 | 46.3 = 30.9 + 7.2 + 8.3 [64.8] | 54.8 [73.9] | +9.0 MiB |
| `Flickable` + Rust row pool (manual culling) | 0.1 | 7.8 | 5.0 / 6.1 / 9.8 [5.1 / 6.7 / 11.7] | 5.0 / 5.8 | 39.7 = 33.4 + 0.1 + 3.4 [40.6] | 63.7 [52.7] | +3.9 MiB |
| `Flickable` with every row instantiated | 6.9 | **298** | **69 / 91 / 96** | 58 / 51 | 497 (frame 452) | 640 | +53 MiB |

`ListView` meets the frame budget for scrolling. Filtering is dominated by the nucleo match (31–45 ms with full
haystacks, see below). Rebuilding the model and the frame together cost ~15 ms. The manual pool is leaner, but it
needs Rust-side scroll handling and gains nothing over `ListView` in smoothness, so use `ListView`.

### 5. nucleo over 10,000 items of ~230 chars (run 2; ms)

| query | hits (full) | 1 thread, full hay: median / p95 / max | 4 threads, full: median / max | hits (title+64) | 1 thread, title+64: median / max | **4 threads, title+64: median / max** |
|---|---:|---:|---:|---:|---:|---:|
| `s` | 10,000 | 2.3 / 7.4 / 14.3 | 4.0 / 16.0 | 9,939 | 1.8 / 9.8 | 4.0 / 4.9 |
| `sn` | 9,996 | 31.4 / 43.9 / 45.7 | 27.4 / 45.3 | 9,246 | 13.8 / 25.9 | 8.2 / 13.5 |
| `snip` | 9,839 | 36.9 / 54.0 / 61.6 | 37.8 / 96.7 | 5,676 | 13.0 / 17.6 | 7.8 / 23.6 |
| `json` | 2,717 | 8.1 / 12.4 / 16.7 | 6.9 / 30.0 | 1,180 | 1.8 / 5.9 | 4.1 / 7.6 |
| `deploy rel` | 7,425 | 57.3 / 73.0 / 76.4 | 53.4 / 78.5 | 2,065 | 9.3 / 17.2 | 7.3 / 16.1 |
| `kbrnts` (typo) | 5,943 | 22.0 / 30.8 / 62.2 | 17.5 / 39.8 | 1,450 | 2.6 / 10.4 | 4.1 / 7.3 |
| `zzqx` (no match) | 51 | 0.5 / 0.7 / 0.8 | 4.0 / 30.4 | 1 | 0.3 / 0.5 | 4.0 / 7.8 |

- Converting 10k haystacks to `Utf32String` once costs 3–8 ms and +2.7 MiB. Converting on the fly per query costs
  about the same as using the cache, so caching is optional.
- `nucleo::Nucleo` (worker pool with 4 threads, `append=true` while typing "deploy"): 22 ms per key median (max 29) in
  run 2 and 38 ms (max 48) in run 3. It is asynchronous, so the UI never blocks, but results arrive 1–3 frames
  later. Injecting 10k items took 4–5 ms.
- The cost is in fuzzy *scoring*: short queries that match almost every 230-char item are the worst case.
  The 4-thread variant has a floor of ~4 ms even with no matches (`zzqx`), which is spawn and join cost, so
  trivial queries come out slower on 4 threads. The truncated key
  returns fewer hits by design (for example `snip`: 5,676 vs 9,839), which is a product decision: body-only matches
  should then come from FTS5.

## What transfers to Windows and what does not

**Transfers (same code, CPU-bound):**
- **Text shaping and line breaking**, and with them `TextEdit` load and keystroke cost, the 16 KB threshold and the
  linear ~1 ms/KB slope. In Slint 1.18.1 the software, FemtoVG and Skia renderers all call the same
  `i_slint_core::textlayout::sharedparley::draw_text_input`. Checked in source:
  `i-slint-renderer-femtovg-1.18.1/itemrenderer.rs:246` and `i-slint-renderer-skia-1.18.1/itemrenderer.rs:636`.
  These costs scale with CPU speed and the font. Segoe UI on Windows is not DejaVu, so expect a similar order of
  magnitude, not the same numbers.
- **Memory growth** of a `TextEdit` holding large text (same parley data structures). The absolute values will
  differ with Windows' allocator.
- **nucleo, ropey, model updates and `ListView` instantiation.** These are pure Rust or Slint core.
- **Frame and scroll costs, and the `i16` panic, if Snipdeck uses the software renderer on Windows.** S4 suggests it
  for the panel and picker (`docs/spikes/S4-mica.md`), and it is Slint's fallback when no GPU context is available.

**Does not transfer:**
- **Rasterization times** (`first_frame`, `repaint`, scroll steps) under FemtoVG or Skia, which do glyph and rect
  drawing on the GPU. They may be faster or slower and need a Windows run.
- **The `i16` panic** under GPU renderers, which use `f32` coordinates. Not tested.
- **Scale factor:** all runs used 1.0. At 150% (common on Windows laptops), software rasterization has 2.25× the
  pixels, and the 32,767 px limit is reached at ~2/3 of the line length.
- **Presenting to the screen:** frames went to memory, with no blit, vsync or compositor.
- **Accessibility:** the `accessibility` feature (AccessKit) was off. `TextEdit` exposes its whole text as
  `accessible-value`, so a 1 MB edit could also push 1 MB to UI Automation per change. Unmeasured.
- **The VM is noisy:** max values vary up to 2–3× between runs (compare run 2 and run 3 above). Medians are steadier.

## Fallback: virtualized rope viewer/editor complexity

Prototype: `spikes/s3-longtext/src/viewer.rs` (~320 non-blank Rust lines) plus `ViewerWindow` in `ui/bench.slint`
(~110 lines).
- Rust owns the text as a `ropey::Rope` and keeps a prefix-sum index of visual rows per line (monospace char-wrap).
- Slint gets only a ring-buffer pool of 41 rows `{y, text, line-no}` in absolute document coordinates, plus a list of
  selection rectangles and the caret position.
- On scroll, only the container offset moves and only newly exposed rows get new text.
- Rebuilding the index is O(lines): 30–36 ms for 5 MB with ~95k short lines. Each operation is O(visible rows) plus
  O(log n) rope access. Typing fixes the prefix sums in O(lines) when a line's row count changes; a Fenwick tree would
  make that O(log n), but at these sizes it isn't needed.
- Find scans rope chunks with a carry for matches that span chunk boundaries (unit-tested against `str::match_indices`).

Not in the prototype; needed for a product (rough estimates):
- **Read-only viewer, v1 scope, ~1–1.5 weeks:** mouse drag selection and double-click word selection, copy
  (`slint` clipboard), a draggable scrollbar, horizontal scroll for no-wrap, word-wrap instead of char-wrap (still
  O(line) in Rust with monospace), East-Asian wide chars and tabs (`unicode-width`), keyboard paging, and theming
  through `Tokens`.
- **Editable, v1.x, +2–3 weeks:** IME preedit (a custom view gets none of `TextInput`'s IME handling and has to
  wire `InputMethodRequest` itself), undo/redo, clipboard paste of large text, and accessibility. AccessKit would only
  see the visible rows unless a custom text provider is added.
- **Proportional fonts** need per-glyph advances from Rust, by shaping visible lines with a font crate, which loses
  the cheap monospace math. Keep the large-text view **monospace-only**. SPEC §5.4 already has a monospace toggle.
- **Not supported:** RTL/bidi, ligature-aware caret positions, rich text.

## Thresholds (where it breaks, on this machine)

| Behaviour | Fine up to | Breaks at |
|---|---|---|
| `TextEdit` keystroke ≤ 16 ms | ~8–16 KB | 32 KB (28–70 ms), 100 KB (65–95 ms) |
| `TextEdit` open (layout + first frame) ≤ 100 ms (panel-show budget) | ~64 KB (60–97 ms) | 100 KB (98–150 ms), 1 MB (~0.65–0.9 s) |
| `TextEdit` memory ≤ +10 MiB | ~100 KB (+4–9 MiB) | 1 MB (+30–89 MiB, peak 81–168 MiB) |
| `TextEdit` no-wrap with a line > 32,767 px | — | panic when the line is selected (software renderer) |
| Viewer, all interactions ≤ 16 ms | 5 MB (largest tested) | not reached |
| 10k list scroll ≤ 16 ms | `ListView`, culled pool | non-virtualized `Flickable` (69 ms) |
| Fuzzy search ≤ 16 ms (median) | bounded ~110-char key on 4 threads (4–8 ms) | full ~230-char items on 1 thread (22–57 ms for queries that match most items) |

**Cap suggestion:**
- **≤ 16 KB:** edit inline in the stock `TextEdit` (word-wrap forced if any line is over 1,000 chars).
- **16 KB – 5 MB:** open the monospace virtualized viewer (read-only plus find, copy and batch transforms in v1).
- **> 5 MB:** untested. Show the first 5 MB in the viewer, offer save-to-file, and keep the full blob in storage.
- **Fuzzy matching:** match on title plus the first ~64 chars (on a worker), with FTS5 for full text.

## Reproduce

```sh
cd spikes/s3-longtext
cargo run --release                       # everything, ~5 min; Markdown tables on stdout
cargo run --release -- --only editor:16kb --only list:listview --only search
cargo run --release -- --scenario editor:1mb:short:wrap   # one scenario in-process
```
Scenario ids: `idle`, `search`, `editor|viewer:<2kb..5mb>:<short|long|single>:<wrap|nowrap>`,
`list:<listview|culled|flickall>`. RSS reads `/proc`, so it shows `-` on Windows.

## Appendix: raw output of run 2

Values ending in `!` were measured after a caught render panic and are **not valid**.

<details><summary>Full tables (all columns)</summary>

#### idle

| scenario | first_frame | rss_base | rss_ui | hwm |
|---|---:|---:|---:|---:|
| empty TextEdit window | 2.74 | 8.98 | 12.7 | 12.7 |

#### search

| scenario | hits | cached_med | cached_p95 | cached_max | onthefly_med | onthefly_max | par4_med | par4_max | trunc_hits | trunc_med | trunc_max | trunc_par4_med | trunc_par4_max | inject_10k | per_key_med | per_key_max | threads | utf32_convert | utf32_rss_delta_mb |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 10000 items, avg 230 chars, q="s" | 10000 | 2.27 | 7.35 | 14.3 | 1.94 | 6.11 | 4.03 | 16.0 | 9939 | 1.80 | 9.83 | 4.04 | 4.93 | - | - | - | - | - | - |
| 10000 items, avg 230 chars, q="sn" | 9996 | 31.4 | 43.9 | 45.7 | 34.8 | 48.3 | 27.4 | 45.3 | 9246 | 13.8 | 25.9 | 8.24 | 13.5 | - | - | - | - | - | - |
| 10000 items, avg 230 chars, q="snip" | 9839 | 36.9 | 54.0 | 61.6 | 36.4 | 60.8 | 37.8 | 96.7 | 5676 | 13.0 | 17.6 | 7.75 | 23.6 | - | - | - | - | - | - |
| 10000 items, avg 230 chars, q="json" | 2717 | 8.08 | 12.4 | 16.7 | 6.55 | 15.8 | 6.94 | 30.0 | 1180 | 1.83 | 5.94 | 4.09 | 7.63 | - | - | - | - | - | - |
| 10000 items, avg 230 chars, q="deploy rel" | 7425 | 57.3 | 73.0 | 76.4 | 57.0 | 93.8 | 53.4 | 78.5 | 2065 | 9.29 | 17.2 | 7.26 | 16.1 | - | - | - | - | - | - |
| 10000 items, avg 230 chars, q="kbrnts" | 5943 | 22.0 | 30.8 | 62.2 | 30.3 | 36.0 | 17.5 | 39.8 | 1450 | 2.58 | 10.4 | 4.05 | 7.25 | - | - | - | - | - | - |
| 10000 items, avg 230 chars, q="zzqx" | 51.0 | 0.54 | 0.65 | 0.84 | 0.86 | 1.15 | 3.97 | 30.4 | 1.00 | 0.33 | 0.52 | 3.99 | 7.83 | - | - | - | - | - | - |
| 10000 items, nucleo::Nucleo pool, typing "deploy" | - | - | - | - | - | - | - | - | - | - | - | - | - | 3.77 | 22.1 | 28.8 | 4.00 | - | - |
| 10000 items: index cost | - | - | - | - | - | - | - | - | - | - | - | - | - | - | - | - | - | 7.92 | 2.70 |

#### editor

| scenario | set | layout | first_frame | load_total | repaint | scroll_med | scroll_max | jump_mid | scroll_mid_med | click | keystroke | key_dispatch | key_frame | arrow_down | select_all | scroll_sel_med | shift_down | find_scan | find_select | replace_all | rss_ui | rss_loaded | hwm | render_panics |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 100kb short wrap | 0.00 | 107 | 5.75 | 112 | 3.36 | 3.35 | 5.33 | 3.33 | 3.47 | 4.88 | 95.3 | 92.4 | 3.11 | 2.97 | 2.74 | 3.12 | 2.64 | 0.14 | 2.51 | 85.5 | 12.2 | 20.2 | 26.9 | - |
| 100kb short nowrap | 0.01 | 110 | 6.07 | 116 | 3.89 | 4.07 | 6.62 | 3.74 | 3.97 | 3.07 | 83.8 | 80.5 | 3.47 | 3.24 | 3.01 | 2.99 | 2.93 | 0.10 | 2.60 | 79.8 | 12.2 | 20.1 | 26.8 | - |
| 100kb long wrap | 0.01 | 88.6 | 9.49 | 98.1 | 7.02 | 7.03 | 11.7 | 6.91 | 4.73 | 4.69 | 93.6 | 87.5 | 6.11 | 5.66 | 6.20 | 6.05 | 5.89 | 0.13 | 5.77 | 91.9 | 12.2 | 16.4 | 19.3 | - |
| 100kb long nowrap | 0.01 | 138 | 12.2 | 150 | 10.4 | 10.1 | 12.9 | 9.41 | 8.35 | 6.84 | 65.1 | 58.0 | 6.91 | 6.78 | 2.25! | 0.00! | 0.05! | 0.10! | 0.03! | 56.3! | 12.2! | 16.3! | 21.5! | 1 |
| 100kb single wrap | 0.00 | 63.2 | 6.18 | 69.4 | 4.56 | 4.61 | 4.80 | 6.83 | 5.17 | 3.91 | 73.0 | 68.2 | 4.74 | 5.95 | 6.50 | 6.43 | 6.29 | 0.11 | 4.08 | 68.1 | 12.2 | 21.4 | 26.6 | - |
| 100kb single nowrap | 0.00 | 0.02 | 7.78 | 7.79 | 7.15 | 7.29 | 8.01 | 7.95 | 7.28 | 6.10 | 72.4 | 65.9 | 6.49 | 6.19 | 1.56! | 0.00! | 0.02! | 0.10! | 0.00! | 0.33! | 12.2! | 21.2! | 24.5! | 1 |
| 1mb short wrap | 0.01 | 862 | 4.91 | 867 | 3.24 | 3.37 | 4.41 | 3.39 | 3.41 | 2.71 | 840 | 837 | 3.08 | 2.66 | 3.56 | 3.18 | 2.91 | 1.06 | 3.00 | 931 | 12.3 | 87.3 | 168 | - |
| 1mb short nowrap | 0.01 | 881 | 5.47 | 886 | 4.12 | 3.75 | 5.80 | 4.83 | 3.80 | 3.03 | 880 | 876 | 3.19 | 2.67 | 3.52 | 3.33 | 3.28 | 1.02 | 3.12 | 837 | 12.3 | 87.2 | 168 | - |
| 1mb long wrap | 0.01 | 644 | 6.74 | 651 | 4.73 | 4.67 | 4.89 | 4.76 | 4.67 | 3.72 | 653 | 648 | 4.28 | 3.87 | 4.11 | 3.91 | 3.69 | 1.07 | 3.60 | 645 | 12.3 | 42.0 | 81.2 | - |
| 1mb long nowrap | 0.01 | 613 | 35.5 | 648 | 33.3 | 35.1 | 41.1 | 38.2 | 38.5 | 34.3 | 682 | 649 | 33.5 | 33.5 | 8.08! | 0.00! | 0.08! | 1.06! | 0.08! | 743! | 12.2! | 39.5! | 76.4! | 1 |
| 1mb single wrap | 0.01 | 892 | 6.58 | 899 | 6.46 | 5.01 | 8.14 | 4.81 | 4.86 | 3.66 | 832 | 828 | 4.16 | 3.84 | 4.28 | 4.41 | 4.08 | 1.13 | 3.77 | 723 | 12.2 | 101 | 151 | - |
| 1mb single nowrap | 0.01 | 0.24 | 60.5 | 60.7 | 69.6 | 66.3 | 77.0 | 69.8 | 67.5 | 59.6 | 864 | 805 | 61.2 | 61.7 | 8.84! | 0.00! | 0.02! | 1.08! | 0.08! | 2.74! | 12.2! | 99.3! | 149! | 1 |
| 5mb short wrap | 0.01 | 4997 | 7.50 | 5004 | 5.33 | 5.22 | 5.78 | 5.54 | 5.55 | 5.62 | 4940 | 4935 | 4.98 | 6.36 | 11.0 | 9.16 | 10.5 | 6.26 | 10.2 | 5242 | 12.1 | 385 | 738 | - |
| 5mb short nowrap | 0.01 | 4641 | 8.04 | 4649 | 5.54 | 4.27 | 5.60 | 4.75 | 4.29 | 6.63 | 4910 | 4906 | 3.95 | 4.28 | 7.81 | 7.40 | 11.0 | 6.07 | 8.89 | 4813 | 12.3 | 385 | 738 | - |
| 5mb long wrap | 0.01 | 3683 | 6.54 | 3689 | 5.04 | 4.90 | 5.37 | 4.90 | 5.10 | 5.60 | 3289 | 3285 | 5.00 | 5.40 | 5.30 | 6.00 | 5.65 | 6.84 | 6.14 | 3276 | 12.2 | 152 | 336 | - |
| 5mb long nowrap | 0.01 | 3290 | 36.3 | 3326 | 40.2 | 36.6 | 41.9 | 35.9 | 37.5 | 37.0 | 3389 | 3352 | 41.0 | 38.6 | 14.4! | 0.00! | 0.22! | 7.14! | 0.16! | 3268! | 12.2! | 142! | 305! | 1 |
| 5mb single wrap | 0.01 | 4540 | 10.7 | 4551 | 6.75 | 7.04 | 8.95 | 9.43 | 5.61 | 4.23 | 4802 | 4797 | 5.02 | 7.56 | 7.07 | 6.33 | 4.84 | 6.07 | 5.96 | 4382 | 12.1 | 449 | 596 | - |
| 5mb single nowrap | 0.02 | 1.09 | 304 | 305 | 289 | 322 | 367 | 295 | 306 | 298 | 4471 | 4134 | 322 | 334 | 50.2! | 0.00! | 0.02! | 5.53! | 0.09! | 12.6! | 12.2! | 444! | 583! | 1 |
| 2kb short wrap | 0.00 | 2.19 | 4.94 | 7.13 | 3.38 | 3.27 | 12.4 | 3.53 | 3.39 | 2.06 | 3.95 | 1.72 | 2.23 | 2.17 | 2.74 | 2.38 | 2.27 | 0.00 | 2.29 | 4.38 | 12.3 | 13.2 | 13.4 | - |
| 2kb long wrap | 0.00 | 1.41 | 4.89 | 6.30 | 3.57 | 3.74 | 4.95 | 3.66 | 3.44 | 2.19 | 3.91 | 1.52 | 2.38 | 2.27 | 2.39 | 2.42 | 2.59 | 0.00 | 2.49 | 4.01 | 12.2 | 13.0 | 13.3 | - |
| 4kb short wrap | 0.00 | 4.99 | 11.6 | 16.6 | 5.22 | 5.06 | 5.43 | 3.40 | 3.62 | 2.25 | 8.94 | 5.17 | 3.74 | 3.52 | 3.97 | 3.74 | 3.46 | 0.01 | 3.19 | 8.68 | 12.1 | 13.2 | 13.6 | - |
| 4kb long wrap | 0.00 | 2.72 | 8.85 | 11.6 | 4.88 | 5.08 | 7.18 | 4.85 | 4.79 | 3.73 | 6.41 | 2.53 | 3.79 | 3.51 | 3.80 | 3.89 | 5.29 | 0.01 | 5.95 | 10.2 | 12.1 | 13.2 | 13.7 | - |
| 8kb short wrap | 0.00 | 10.5 | 13.7 | 24.2 | 7.40 | 3.57 | 7.52 | 3.52 | 3.49 | 3.31 | 10.7 | 7.33 | 3.39 | 2.75 | 2.95 | 2.81 | 2.39 | 0.01 | 2.69 | 11.0 | 12.3 | 13.6 | 14.3 | - |
| 8kb long wrap | 0.00 | 9.18 | 14.5 | 23.7 | 7.44 | 6.49 | 8.32 | 5.98 | 5.01 | 4.01 | 9.15 | 5.14 | 4.05 | 3.68 | 4.13 | 4.03 | 3.75 | 0.01 | 3.53 | 8.89 | 12.3 | 13.7 | 14.2 | - |
| 16kb short wrap | 0.00 | 17.9 | 5.43 | 23.3 | 3.75 | 3.67 | 8.23 | 3.65 | 3.53 | 2.48 | 16.6 | 13.9 | 2.80 | 2.41 | 2.55 | 2.53 | 2.33 | 0.02 | 2.14 | 18.3 | 12.4 | 14.4 | 15.7 | - |
| 16kb long wrap | 0.00 | 10.5 | 6.96 | 17.4 | 4.74 | 4.84 | 5.11 | 4.87 | 4.88 | 5.77 | 20.8 | 15.0 | 5.94 | 5.75 | 6.01 | 6.43 | 5.97 | 0.08 | 6.02 | 21.5 | 12.4 | 14.2 | 14.9 | - |
| 32kb short wrap | 0.00 | 25.5 | 4.94 | 30.4 | 3.54 | 3.48 | 4.12 | 3.68 | 3.67 | 2.46 | 28.8 | 25.9 | 2.89 | 3.95 | 4.31 | 4.51 | 4.04 | 0.04 | 3.88 | 42.6 | 12.4 | 15.4 | 18.1 | - |
| 32kb long wrap | 0.01 | 28.6 | 9.81 | 38.5 | 6.99 | 7.19 | 7.54 | 7.73 | 6.97 | 5.45 | 35.2 | 29.1 | 6.19 | 5.63 | 5.82 | 5.98 | 5.74 | 0.04 | 5.74 | 34.1 | 12.3 | 14.6 | 15.8 | - |
| 64kb short wrap | 0.01 | 89.7 | 6.86 | 96.6 | 4.83 | 4.96 | 5.76 | 5.08 | 4.86 | 3.45 | 77.9 | 74.2 | 3.49 | 3.48 | 4.30 | 4.02 | 3.89 | 0.09 | 3.54 | 80.1 | 12.2 | 17.5 | 22.7 | - |
| 64kb long wrap | 0.00 | 50.0 | 9.97 | 60.0 | 6.56 | 4.82 | 6.64 | 4.59 | 4.70 | 3.68 | 42.1 | 38.0 | 3.92 | 3.62 | 3.83 | 3.87 | 3.68 | 0.07 | 3.67 | 40.4 | 12.5 | 15.6 | 18.0 | - |

#### viewer

| scenario | set | layout | first_frame | load_total | repaint | scroll_med | scroll_max | jump_mid | scroll_mid_med | click | keystroke | key_dispatch | key_frame | arrow_down | select_all | scroll_sel_med | shift_down | find_scan | find_select | replace_all | rss_ui | rss_loaded | hwm |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 100kb short wrap | 6.23 | 0.53 | 7.96 | 14.7 | 3.64 | 3.04 | 8.20 | 5.06 | 2.84 | 2.92 | 2.87 | 0.09 | 2.77 | 2.58 | 2.50 | 2.79 | 2.70 | 0.16 | 4.86 | 5.56 | 11.4 | 12.1 | 12.4 |
| 100kb short nowrap | 0.13 | 4.63 | 6.39 | 11.2 | 2.74 | 3.44 | 8.91 | 5.79 | 3.50 | 3.27 | 3.17 | 0.10 | 3.07 | 2.99 | 3.07 | 3.90 | 3.03 | 0.16 | 6.18 | 4.12 | 11.9 | 12.5 | 12.9 |
| 100kb long wrap | 0.15 | 0.01 | 7.85 | 8.02 | 3.17 | 4.48 | 8.24 | 11.0 | 3.80 | 3.32 | 3.97 | 0.12 | 3.85 | 3.39 | 3.49 | 3.92 | 3.42 | 0.16 | 6.58 | 6.86 | 11.7 | 12.4 | 12.8 |
| 100kb long nowrap | 0.15 | 0.02 | 2.74 | 2.91 | 1.25 | 1.26 | 5.71 | 1.99 | 1.32 | 1.25 | 1.31 | 0.06 | 1.25 | 1.16 | 1.12 | 1.26 | 1.55 | 0.14 | 2.21 | 2.79 | 11.6 | 12.3 | 12.6 |
| 100kb single wrap | 1.92 | 0.01 | 13.6 | 15.5 | 3.46 | 5.42 | 11.9 | 6.25 | 5.59 | 4.12 | 5.33 | 0.15 | 5.19 | 3.86 | 3.51 | 4.30 | 3.51 | 0.24 | 6.64 | 7.31 | 11.7 | 12.4 | 12.7 |
| 100kb single nowrap | 0.40 | 0.01 | 6.54 | 6.95 | 1.05 | 0.94 | 5.38 | 1.37 | 0.93 | 0.85 | 1.15 | 0.05 | 1.09 | 0.86 | 0.86 | 0.95 | 0.91 | 0.22 | 1.00 | 1.97 | 11.7 | 12.3 | 12.6 |
| 1mb short wrap | 2.64 | 8.58 | 10.8 | 22.0 | 4.21 | 4.57 | 5.03 | 8.65 | 4.65 | 4.25 | 4.41 | 0.22 | 4.26 | 4.26 | 4.20 | 5.13 | 3.85 | 1.90 | 8.62 | 21.7 | 11.7 | 13.4 | 15.7 |
| 1mb short nowrap | 2.34 | 6.11 | 7.80 | 16.3 | 3.26 | 3.81 | 7.31 | 5.94 | 3.40 | 2.90 | 3.18 | 0.10 | 3.09 | 2.98 | 3.32 | 3.79 | 3.19 | 1.62 | 7.16 | 13.8 | 11.5 | 13.2 | 15.6 |
| 1mb long wrap | 2.26 | 0.19 | 11.9 | 14.3 | 3.93 | 4.04 | 5.70 | 6.74 | 4.80 | 5.11 | 5.00 | 0.17 | 4.84 | 3.87 | 5.28 | 4.74 | 3.83 | 1.75 | 9.25 | 12.1 | 11.6 | 13.4 | 16.2 |
| 1mb long nowrap | 5.75 | 0.16 | 12.1 | 18.0 | 3.60 | 5.91 | 8.06 | 9.25 | 5.80 | 3.47 | 4.00 | 0.18 | 3.83 | 3.46 | 3.75 | 3.76 | 3.62 | 1.52 | 7.12 | 12.2 | 11.7 | 13.3 | 15.9 |
| 1mb single wrap | 2.33 | 0.04 | 9.38 | 11.7 | 3.56 | 3.88 | 4.62 | 10.7 | 5.99 | 3.54 | 4.85 | 0.13 | 4.72 | 3.47 | 3.46 | 3.91 | 4.43 | 1.75 | 9.55 | 12.5 | 11.8 | 13.4 | 16.3 |
| 1mb single nowrap | 1.85 | 0.03 | 1.59 | 3.47 | 0.78 | 0.64 | 1.69 | 0.67 | 0.59 | 0.52 | 0.78 | 0.03 | 0.75 | 0.62 | 0.53 | 0.60 | 0.56 | 1.49 | 1.08 | 6.90 | 11.8 | 13.4 | 16.2 |
| 5mb short wrap | 8.52 | 29.7 | 6.40 | 44.6 | 2.53 | 2.98 | 4.07 | 5.17 | 2.98 | 2.46 | 2.58 | 0.08 | 2.48 | 2.66 | 2.87 | 2.89 | 2.63 | 8.28 | 5.40 | 81.8 | 11.7 | 18.4 | 32.5 |
| 5mb short nowrap | 12.3 | 35.9 | 6.41 | 54.6 | 2.47 | 3.00 | 3.32 | 5.90 | 3.24 | 2.74 | 2.90 | 0.09 | 2.82 | 2.76 | 2.74 | 3.19 | 3.07 | 7.63 | 6.81 | 77.5 | 11.7 | 18.1 | 32.2 |
| 5mb long wrap | 8.82 | 0.91 | 8.06 | 17.8 | 3.34 | 3.88 | 4.65 | 6.32 | 3.80 | 3.36 | 4.96 | 0.16 | 4.80 | 3.33 | 3.35 | 3.97 | 3.61 | 7.95 | 7.90 | 33.9 | 11.8 | 18.0 | 31.0 |
| 5mb long nowrap | 8.04 | 0.86 | 8.40 | 17.3 | 3.56 | 5.91 | 6.56 | 10.4 | 5.92 | 3.54 | 3.76 | 0.19 | 3.55 | 3.55 | 3.73 | 4.03 | 3.84 | 7.68 | 6.41 | 33.7 | 11.7 | 18.1 | 31.1 |
| 5mb single wrap | 12.6 | 0.19 | 8.11 | 20.9 | 3.55 | 3.85 | 7.97 | 6.04 | 3.83 | 3.42 | 4.95 | 0.13 | 4.81 | 3.37 | 3.35 | 4.04 | 3.44 | 7.77 | 7.05 | 30.6 | 11.7 | 18.0 | 30.9 |
| 5mb single nowrap | 14.9 | 0.23 | 2.56 | 17.7 | 1.05 | 0.58 | 3.79 | 0.53 | 1.05 | 1.16 | 1.34 | 0.06 | 1.30 | 0.60 | 0.60 | 0.91 | 0.92 | 10.9 | 1.95 | 31.5 | 11.7 | 17.9 | 31.0 |

#### list

| scenario | set_model | first_frame | scroll_med | scroll_p95 | scroll_max | jump_mid | jump_end | repaint | filter_match_med | filter_model_med | filter_frame_med | filter_total_med | filter_total_max | rss_data | rss_loaded | hwm |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| ListView (built-in virtualization), 10000 rows | 12.0 | 16.7 | 2.12 | 2.52 | 7.77 | 4.90 | 5.19 | 1.87 | 30.9 | 7.19 | 8.29 | 46.3 | 54.8 | 14.9 | 23.9 | 27.9 |
| Flickable + Rust row pool (manual culling), 10000 rows | 0.10 | 7.81 | 4.95 | 6.06 | 9.78 | 5.04 | 5.75 | 2.00 | 33.4 | 0.09 | 3.42 | 39.7 | 63.7 | 14.8 | 18.7 | 19.3 |
| Flickable, all rows instantiated, 10000 rows | 6.85 | 298 | 69.1 | 90.8 | 96.0 | 57.5 | 50.9 | 52.0 | 33.5 | 7.53 | 452 | 497 | 640 | 14.9 | 68.3 | 75.8 |

</details>
