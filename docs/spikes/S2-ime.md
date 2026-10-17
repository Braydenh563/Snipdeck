# Spike S2: IME and CJK input in Slint

- Date: 2026-10-17
- Code: [`spikes/s2-ime/`](../../spikes/s2-ime/) (standalone throwaway crate with its own `[workspace]`)
- Relates to: SPEC §7 (IME-correct input, CJK fallback), §13.2 · DESIGN §3 (IBM Plex Sans, Yu Gothic UI, Microsoft YaHei UI) · [S1](S1-noactivate.md) (temporary activation for panel text entry)
- Versions reviewed: Slint 1.18.1 (`i-slint-core`, `i-slint-backend-winit`, `i-slint-renderer-{femtovg,skia,software}`), winit 0.30.13, parley 0.11.1, fontique 0.11.1, rustc 1.97

## Status: what is verified and what is not

**No IME was run anywhere.** No Windows machine was available, and the Linux container has no input method.
Every claim about runtime IME behaviour comes from reading the source and is marked **UNVERIFIED** until the
owner runs the [manual test script](#manual-test-script-real-windows).

| Claim | How it was checked |
|---|---|
| The test app builds and is lint-clean | **Verified.** `cargo check`, `cargo check --target x86_64-pc-windows-gnu`, `cargo fmt --check`, and `cargo clippy -- -D warnings` (Linux and windows-gnu) all pass. `cargo build --target x86_64-pc-windows-gnu` also **links** a Windows `.exe` with mingw. |
| The app starts and renders, and IBM Plex Sans is registered | **Verified on Linux.** Run under Xvfb with `SLINT_BACKEND=winit-software`. Screenshot checked: Plex Latin, CJK through fallback, and the heartbeat ticking. |
| The CJK fallback override is stored in Slint's shared font collection | **Verified at the data level.** The app prints the effective chain after the window is created. Its visible effect could not be shown on Linux (see [Fonts](#fonts-ibm-plex-sans--cjk-fallback-on-windows)). |
| The IME event path, rendering, candidate-window placement, and key handling during composition | **Source reading only.** File references are given inline. |
| Upstream bug status | **Researched.** GitHub issues, the Slint CHANGELOG, and `git log` / `git tag --contains` on a clone of slint-ui/slint, plus winit's changelog. |

## Recommendation: GO WITH CHANGES (provisional until the test script passes)

Slint 1.18.1 has real inline-composition support on Windows. winit's IMM32 path delivers `Ime::Preedit` and
`Ime::Commit`, Slint draws the composition inside the `TextInput`, and it tells the IME where the caret is.
All renderers and all text widgets share one code path. None of the problems found blocks Snipdeck, and each
has a contained workaround. Changes to plan for:

1. **Pin the CJK fallback order ourselves** using Slint's `unstable-fontique-011` feature
   (`slint::fontique_011::shared_collection().set_fallbacks(...)`). Without it, the font used for Han
   characters is whatever DirectWrite picks for a sample ideograph with an empty locale. That pick is
   undefined, so Japanese text may get Chinese glyph forms, and the reverse. The full recipe is [below](#recommended-setup-for-snipdeck-ui).
2. **Pin Slint exactly** (`=1.18.x`), as S1 also requires. The fontique module is explicitly unstable.
3. **Handle Enter and Esc on key *press* only.** Keys the IME consumes never reach Slint (see F4), but the
   matching key *release* does.
4. **Commit the composition before deactivating the panel.** In S1's `end_text_entry()`, call
   `ImmNotifyIME(himc, NI_COMPOSITIONSTR, CPS_COMPLETE, 0)` on the panel HWND before `SetForegroundWindow(target)`. This works around
   upstream bug slint#10861, where the composition is lost when the window loses focus.
5. **Accept the composition styling for v1, and file it upstream.** Since 1.14 the composition is drawn with
   the *selection highlight* and no underline, and the active Japanese clause is not distinguished (F1). If
   step 6 of the test script shows Japanese users would be confused, carry a roughly 30-line patch to
   `i-slint-core` (`sharedparley::draw_text_input`) via `[patch.crates-io]`, and send it upstream.
6. **Custom search field:** keep the end of a long composition visible (F6). This is a small `.slint`
   change in our own LineEdit, described under [Workarounds](#workarounds-if-something-is-broken).

**This becomes NO-GO** if test steps 1, 3, or 4 fail: the composition is not shown inline, the committed text is
wrong or doubled, Hangul syllables arrive out of order, or the UI freezes after an IME switch (step 15)
with the default renderer *and* with `winit-software`, and none of the workarounds below fixes it within
about two days. The fallback would be a custom `WindowAdapter`, as S1 describes, that handles
`WM_IME_*` itself and feeds Slint's internal composition events. That is a large fork.

## Risk rating: MEDIUM

Low risk to the architecture: IME is fully inside Slint and winit, and the fixes are local. Medium risk
because nothing has been run, two relevant upstream bugs are still open (#10861, #5206), and the composition
styling regressed in 1.14.

## How composition flows through Slint 1.18.1 on Windows

1. **winit, IMM32** (`winit-0.30.13/src/platform_impl/windows/event_loop.rs` WM_IME_*, `ime.rs`).
   - `WM_IME_STARTCOMPOSITION` → `Ime::Enabled`.
   - `WM_IME_COMPOSITION` with `GCS_COMPSTR` → `Ime::Preedit(text, Some(clause_or_cursor_byte_range))`. The range is the
     target clause from `GCS_COMPATTR`, or `GCS_CURSORPOS` before any conversion.
   - `GCS_RESULTSTR` → `Preedit("")` followed by `Commit(text)`.
   - `WM_IME_ENDCOMPOSITION` commits any leftover result (a workaround for the Hangul IME) and sends `Ime::Disabled`.
   - winit removes `ISC_SHOWUICOMPOSITIONWINDOW` in `WM_IME_SETCONTEXT` and does not pass `WM_IME_COMPOSITION` to
     `DefWindowProc`, so the IME draws **no floating composition window**. The app draws it inline.
2. **Slint winit backend** (`i-slint-backend-winit/winitwindowadapter.rs:1429-1447`) turns these into
   `KeyEventType::UpdateComposition` (with `preedit_text` and `preedit_selection`) and `CommitComposition`. `Ime::Enabled` and
   `Ime::Disabled` are ignored.
3. **TextInput** (`i-slint-core/items/text.rs:1179-1217`) stores `preedit-text` and the selection, then inserts the
   committed text through the normal `insert()` path. That path triggers `edited` and the undo stack.
4. **IME requests go back to Windows** (`text.rs:1875` `ime_properties`, `winitwindowadapter.rs:2114`):
   - On focus-in: `set_ime_allowed(true)` → `ImmAssociateContextEx(IACE_DEFAULT)`.
   - On focus-out of the last text input: `set_ime_allowed(false)` → `IACE_CHILDREN`, which detaches the IME context so
     global shortcuts are not swallowed.
   - `set_ime_cursor_area(caret rect)` → `ImmSetCompositionWindow(CFS_POINT, caret bottom)` and
     `ImmSetCandidateWindow(CFS_EXCLUDE, caret rect)`.
   - `set_ime_purpose(Password)` is a **no-op on Windows** in winit 0.30, so the IME stays active in password fields.

## Findings per requirement

**F1. Composition display and underline. Works, but no underline.**
- What happens: the composition is drawn inline at the caret, with a fill in `selection-background-color` and
  text in `selection-foreground-color` over the whole composition (`textlayout/sharedparley.rs:325-327`).
  The caret is drawn at the end of the active clause, or at the IME cursor.
- What is missing: there is no underline, and the active clause is not distinguished from the other
  clauses. In Japanese multi-clause conversion, the only cue for which clause is active is the caret
  position and the candidate list.
- History: until 1.13, Skia drew an **underline** for the composition (`renderers/skia/itemrenderer.rs`,
  `underline: true`). The move to parley in **1.14.0** (commits 13623bc15 for FemtoVG and 5ba8206ed for Skia)
  replaced it with the selection-style highlight for every renderer. FemtoVG never had an underline (caveat in
  8cba0622f, 0.3.1).
- No upstream issue tracks this. **UNVERIFIED** how it looks on Windows: test steps 1, 6, and 10.

**F2. Candidate window position. Anchored at the composition start.**
- How it works: Slint sends the caret rect of the *committed* text, in window-relative logical pixels that winit
  converts with the window's scale factor. During a composition the base caret does not move, and Slint
  re-sends only when the text, caret, or item position changes (`render()` and `update_ime()`). So the IME
  gets the caret rect at the **start of the composition** and keeps it until commit.
- Effect: the candidate list sits under the composition start, not under the clause being converted. In a
  wrapped `TextEdit`, the list sits under the first line, where it may cover the wrapped part of the
  composition (test step 9).
- Fixes so far: 1.18.0 sends updates when a focused `TextInput` moves (#13198, ac7b20ebc). 1.8.0 fixed a
  FemtoVG cursor-area bug in empty, aligned inputs (b5da1f7b0).
- Whether the new TSF-based Microsoft IME on Windows 11 honours IMM32's `CFS_EXCLUDE` / `CFS_POINT` exactly is
  **UNVERIFIED** (test steps 2, 3, and 18). Other winit apps report oddities here (iced#3189).

**F3. Committing. Correct by design.**
- Commit arrives as `Preedit("")` then `Commit(text)`. Slint inserts the text once and fires `edited` **once
  per commit, not per keystroke during composition**. This is what we want for live search: no filtering on
  romaji or jamo.
- Esc cancelling a composition clears it (fixed for #1925 in 1.1.1, a0dd643d8).
- Several older bugs are fixed:
  - Duplicate "redundant" composing text below the field with Chinese IMEs (#5982), fixed in **winit
    0.30.13** (the `WM_IME_SETCONTEXT` flag masking). Slint 1.18.1 already uses winit 0.30.13.
  - Wrong preedit cursor offset on Windows, fixed in winit 0.30.10.
  - Korean input on Windows 11 (#1706), fixed with the first IME support in 0.3.1.

**F4. Cursor movement and keys during composition. Correct by design (source-level).**
- How it works: while the IME is composing, Windows delivers key-downs as `VK_PROCESSKEY`. winit maps this to
  `Key::Named(Process)`, which has no Slint text, so `winitwindowadapter.rs:1395` **drops the event**.
- Arrow keys therefore move between clauses inside the IME, and Slint's caret does not move.
- Enter commits the composition and does **not** trigger `accepted` or a `FocusScope` `key-pressed`.
- Esc cancels and does not close the panel.
- Caveat: the matching **key-up** (for example `VK_RETURN`) is delivered as `KeyReleased`. Never act on
  release.
- Korean is different: an arrow or space key commits the syllable and is then passed through normally.
  The order must be commit first, then the move (test steps 4 and 5).
- Clicking inside the same field while composing moves the caret, and the composition is drawn at the new
  position. This is a documented caveat of the original implementation (8cba0622f), and winit does not forward
  clicks to the IME (test step 13).

**F5. Focus changes during composition. Known bugs.**
- Inside the window: on focus-out, `TextInput` discards its preedit on every platform except Android
  (`text.rs:1258-1280`; the Android commit was added in #8668, 1.12.0). Slint then calls `set_ime_allowed(false)`.
  Whether the IME then commits into the *next* field or drops the text depends on the IME (test step 12). The
  reporter of #10861 saw "committed" when clicking another LineEdit.
- Leaving the window, with Alt+Tab or a click on another app: **slint#10861 (open)**, reported on Windows 11
  with the Korean IME and Slint 1.15.1, says the last composing character is dropped. Maintainers
  reproduced it, say it does not happen with winit master, and labelled it `upstream`. This matters to
  Snipdeck because S1's panel deactivates itself after text entry (workaround: change 4).

**F6. Long compositions and wrapped lines.**
- The `TextInput`'s size includes the composition (`RenderString for TextInput`, `text.rs:1342`). But the
  std LineEdit and TextEdit scroll only in `cursor-position-changed`, which does not fire during composition.
- Effect: a composition longer than the visible width runs past the clipped edge until it is committed.
- Wrapping works, because the composition is laid out with the text (test steps 8 and 9 are **UNVERIFIED**).

**F7. Per renderer.**
- IME handling lives entirely in `i-slint-core` plus the winit backend. Since 1.14, FemtoVG, Skia, and the
  software renderer (with system fonts, which the winit backend enables) all draw text input with
  `sharedparley::draw_text_input`.
- The behaviour should therefore be identical across renderers. The only renderer-specific risk is the
  Windows 10 freeze below.

**F8. Per widget.**
- `LineEdit` and `TextEdit` (std-widgets, Fluent) are thin wrappers over `TextInput` (`lineedit-base.slint`,
  `textedit-base.slint`). They add the scroll logic from F6, and the placeholder hides while `preedit-text` is
  not empty.
- A custom `TextInput` gets exactly the same IME behaviour, and it can read the `preedit-text` property. That
  is useful for scroll fixes and for the "composing" state in S1's activation logic.

**F9. Windows 10 freeze with Microsoft Pinyin. Open, not reproducible by maintainers.**
- slint#5206 (2024, FemtoVG and software renderers, Windows 10 with Korean or Chinese locales): the UI stops
  repainting until the window is resized. #8716, a Microsoft Pinyin report, was closed as a duplicate of it.
- In Sep 2026 the maintainers said "nobody is working on this since we can't reproduce it". The test app shows a
  4 Hz heartbeat so the owner can detect the freeze (test step 15).

## Upstream issues and versions

| Ref | What | Status / version |
|---|---|---|
| 8cba0622f | Initial IME for winit + FemtoVG/Skia ("no underline on FemtoVG", "clicks not forwarded to IME") | 0.3.1 |
| #1706 | Can't input Korean on Windows 11 | closed Oct 2022 (0.3.1) |
| #1925 / a0dd643d8 | Esc doesn't clear preedit; discard preedit on focus change | fixed 1.1.1 |
| fce2a80a5, 3ffe7e7be | Only send IME requests when focused; re-send when text changes at render | 1.4.0 |
| b5da1f7b0 | FemtoVG: IME cursor position in empty, aligned TextInput | 1.8.0 |
| #8668 | Android: commit preedit on focus change (Windows still discards) | 1.12.0 |
| 13623bc15, 5ba8206ed | FemtoVG and Skia move to parley: preedit drawn as highlight, Skia loses its underline | 1.14.0 |
| #5982 | Duplicate composing text under the field (Chinese IME, Windows) | fixed by winit 0.30.13, closed Mar 2026 |
| #13198 (ac7b20ebc) | Update the IME when a focused TextInput moves | 1.18.0 |
| 66d4a13d3 | IME composition shares the shaped-text cache (no re-shape per keystroke) | 1.18.0 |
| **#10861** | Composition dropped when the window loses focus (Windows, Korean, 1.15.1) | **open**, `upstream` |
| **#5206** (dup #8716) | UI stops repainting after an IME switch (Windows 10, Microsoft Pinyin and Korean) | **open**, can't reproduce |
| #3811 | IME in the public Platform API (needed for a custom WindowAdapter) | open since 2023 (still internal) |
| #4698 | Chinese input on Linux (Sogou), iOS (winit) | open; not Windows |
| winit 0.30.10 / 0.30.13 | Windows `Ime::Preedit` cursor offset; `WM_IME_SETCONTEXT` masking | in Slint 1.18.1's lock |

Slint `main` (1.19, unreleased) changes nothing in the IME path. It changes the default font order to put
the platform UI font first (#13540), which does not affect CJK.

## Fonts: IBM Plex Sans and CJK fallback on Windows

**How Slint 1.18.1 picks a font** (`textlayout/sharedparley/shaping.rs:85-110`, `i-slint-common/sharedfontique.rs`):

- Every text run gets the stack `[font-family, generic sans-serif, generic system-ui]`.
- On Windows, fontique maps the generics to **Arial** and **Segoe UI** (`fontique/src/backend/dwrite.rs:31-37`),
  so the effective stack is IBM Plex Sans → Arial → Segoe UI. None of these has CJK glyphs.
- For each cluster that no font in the stack covers, parley asks fontique for `FallbackKey(script, locale=None)`.
  Slint never sets a locale, and there is no `lang` property on `Text` or `TextInput`.
- With nothing configured, fontique asks DirectWrite's system fallback (`IDWriteFontFallback::MapCharacters`)
  for one **sample character** of the script (for Han, `今`) with an empty locale, and caches the family it
  gets back. Which family DirectWrite returns for Han with no locale is **UNVERIFIED**. The app prints it: run
  `--cjk system` and read the `system fallback before override  Hani:` line.
- parley also always appends the Han fallback to every query, to catch CJK punctuation (fontique `query.rs`
  HACK, parley#597).
- `font-family` takes a single family name. Comma lists are **not** parsed, so CSS-style stacks are impossible.

**Bundling IBM Plex Sans:**
- Use `import "../fonts/IBMPlexSans-Regular.ttf";` (one import per weight) in a `.slint` file, and set
  `default-font-family: "IBM Plex Sans"` on every `Window`. That is what the test app does.
- The font bytes are embedded in the binary and registered with the shared collection when the component is
  created. The family name is "IBM Plex Sans", and each weight adds about 218 KB. The 3 sans weights plus 2
  mono weights come to about 1.1 MB, well inside the 15 MB installer budget.
- License: OFL 1.1. Ship `OFL.txt`, and do not rename the font ("Plex" is a Reserved Font Name). Use it
  unmodified, or subset it without the "Plex" name.

**CJK: use system fonts and do not bundle them.**
- Yu Gothic UI (ja), Microsoft YaHei UI (zh-Hans), and Malgun Gothic (ko) ship with Windows 10 and 11. The app
  prints `found (before window creation): true/false` for each one so the owner can confirm on their machine.
- fontique **memory-maps** system fonts (`memmap2`), so they don't add to private RAM (the §9 budget).
- Bundling Noto Sans CJK would cost about 16–20 MB per region and would break the installer budget.
- Avoid `SLINT_DEFAULT_FONT` and `SLINT_FONT_PATH`. They `std::fs::read` whole files onto the heap (a 10–20 MB
  `.ttc`), and they only change the *generic* families, not the per-script fallback.

**Han unification.** Japanese and Chinese share code points but use different glyph shapes (直 骨 角 今 海 …).
- One Han order applies to the whole process. Pick it from the UI locale: `ja` → Yu Gothic UI first, `zh-Hans` → YaHei first.
- For other UI locales, pick it from the first CJK language in `GetUserPreferredUILanguages`, or default to ja.
- Each chain should also list the other region's font. For example, Yu Gothic lacks many simplified-only
  characters (们, 这), so they fall through to YaHei.
- Mixed ja/zh snippets will show one region's glyph forms. Accept this for v1.

**When no font has the glyph:**
- parley falls back to the first candidate (IBM Plex Sans) and draws its `.notdef`. In the Linux run this
  rendered as **blank space**, not a visible tofu box, for U+10FFFD.
- Nothing crashes and no warning is logged: the character just looks missing.
- On Windows, a missing Yu Gothic or YaHei (for example a stripped LTSC image) leads to the same result: CJK
  text would be invisible. The startup diagnostic is the place to detect this.
- Optional for v1: log a warning when `family_id("Yu Gothic UI")` is `None`.

**Why the Linux run could not show the override visually:**
- fontique yields app-set generic families *followed by* the system ones, and Linux fontconfig's `sans-serif`
  list contains WenQuanYi Zen Hei. So Han is covered by the stack before script fallback is ever consulted.
- On Windows, Arial and Segoe UI have no CJK glyphs, so fallback is always consulted. The fallback API itself
  was checked: a fontique `Query` with the overridden key returned the overriding family.

### Recommended setup for snipdeck-ui

```rust
// Cargo: slint = { version = "=1.18.1", features = ["unstable-fontique-011"] }
use slint::fontique_011::{fontique, shared_collection};

/// Call once at startup, before the first component is created.
pub fn configure_cjk_fallback(han_first: &str /* "ja" | "zh-Hans" | "ko" */) {
    let mut c = shared_collection();
    let fam = |c: &mut fontique::Collection, names: &[&str]| -> Vec<_> {
        names.iter().filter_map(|n| c.family_id(n)).collect()
    };
    let han: &[&str] = match han_first {
        "zh-Hans" => &["Microsoft YaHei UI", "SimSun", "Yu Gothic UI", "Malgun Gothic"],
        "ko" => &["Malgun Gothic", "Yu Gothic UI", "Microsoft YaHei UI"],
        _ => &["Yu Gothic UI", "Meiryo UI", "Microsoft YaHei UI", "Malgun Gothic"],
    };
    for (tag, names) in [
        (*b"Hani", han),
        (*b"Hira", &["Yu Gothic UI", "Meiryo UI"][..]),
        (*b"Kana", &["Yu Gothic UI", "Meiryo UI"][..]),
        (*b"Hang", &["Malgun Gothic"][..]),
        (*b"Bopo", &["Microsoft YaHei UI"][..]),
    ] {
        let ids = fam(&mut c, names);
        if !ids.is_empty() {
            let key = fontique::FallbackKey::new(fontique::Script::from_bytes(tag), None);
            c.set_fallbacks(key, ids.into_iter());
        }
    }
}
```

`spikes/s2-ime/src/main.rs` (`configure_cjk_fallback`) is the tested version of this, with diagnostics.
Changing the order at runtime, for example on a locale switch, is not covered by this spike. Shaped text is
cached (`TextLayoutCache`), so expect to need a restart or a re-layout.

## Interaction with S1 (non-activating panel)

- **IME needs keyboard focus.** A `WS_EX_NOACTIVATE` panel never has it, so S1's temporary activation is
  also what enables the IME.
- **Esc and Enter during composition are safe.** They are consumed by the IME and never reach the panel's
  `FocusScope` (F4), so composing in the search box cannot accidentally close the panel or paste.
- **Deactivating mid-composition loses the text** (#10861). Use change 4: `ImmNotifyIME(..., CPS_COMPLETE)`
  first. Our own `preedit-text != ""` tells us when a composition is active.
- **IME input mode persistence is UNVERIFIED** (whether Japanese stays in hiragana mode across activations).
  winit uses the default input context per HWND (`IACE_DEFAULT`), so the mode should persist per window. Check
  this in S1's real panel once both spikes are merged.

## Workarounds if something is broken

| Symptom (test step) | Workaround | Cost |
|---|---|---|
| Wrong Han glyph forms (16) | `set_fallbacks` order as above (already in the app) | done |
| Composition lost on Alt+Tab or panel deactivation (14) | `ImmGetContext(hwnd)` + `ImmNotifyIME(himc, NI_COMPOSITIONSTR, CPS_COMPLETE, 0)` + `ImmReleaseContext` before deactivating. The HWND is available through S1's hook | small, in `snipdeck-platform` |
| Long composition hidden in a narrow field (8) | In our own search field (TextInput-based): `changed preedit-text => { if self.preedit-text != "" { computed-x = min(computed-x, parent.width - self.preferred-width - self.text-cursor-width); } }`, which keeps the end visible when composing at the end | small, `.slint` |
| Composition hard to read, no clause cue (1, 6) | Patch `sharedparley::draw_text_input`: highlight only `preedit_selection` (the active clause) and draw a 1 px underline under `preedit_range` with `layout.selection_geometry()`. Carry it via `[patch.crates-io]`, then upstream it | about 1 day plus fork upkeep |
| Candidate window misplaced (2, 3, 18) | Upstream: send the caret rect of the composition cursor (base offset + `preedit_selection.start`) during composition. Interim: patch the same function in `i-slint-backend-winit` | about 1 day |
| UI freeze after IME switch (15) | Try `SLINT_BACKEND=winit-software`, then Skia. Collect a debugger backtrace and add it to slint#5206 | unknown |
| Everything broken | Custom `WindowAdapter` (S1's fallback) handling `WM_IME_*` and calling Slint's internal composition events. **Blocked by #3811**: the IME API is not public, so this needs a Slint fork | large |

## Running the test app

```
cd spikes/s2-ime
cargo run                                   # default: winit + FemtoVG (OpenGL), Han → Yu Gothic UI
cargo run -- --cjk zh                       # Han → Microsoft YaHei UI first
cargo run -- --cjk system                   # no override: what does DirectWrite pick?
set SLINT_BACKEND=winit-software&& cargo run
cargo run --features skia                   # MSVC toolchain only; then SLINT_BACKEND=winit-skia
cargo run -- --ja-font "Meiryo UI"          # try another fallback family
```

The window contains:
- (1) a std `LineEdit` and a 160 px narrow `LineEdit`;
- (2) a 300 px word-wrapped `TextEdit`;
- (3) a custom IBM Plex Sans `TextInput`, with its live `preedit-text` shown in red underneath;
- (4) font-fallback sample rows;
- a heartbeat, the checklist, and an event log.

Every `edited`, `accepted`, `key-pressed`, and cursor event is logged with a timestamp, in the window and on
the console. The console also prints the font diagnostics at startup.

## Manual test script (real Windows)

**Setup:**
- Windows 11, and Windows 10 if available.
- Settings → Time & language → Language & region: add **Japanese** (Microsoft IME), **Chinese (Simplified, PRC)**
  (Microsoft Pinyin), and **Korean** (Microsoft IME). Switch between them with Win+Space.
- Run `cargo run` from a terminal and keep the console visible.
- Write down the startup lines `system fallback before override Hani:` and `found (before window creation)`.

| # | Steps | Expected result | Result |
|---|---|---|---|
| 1 | JA. Click LineEdit (1). Type `nihongo`. Space. Enter. | While typing, にほんご appears **inline** in the field, highlighted, with no floating IME composition box. After Space: 日本語. After Enter: text committed. Log: exactly one `edited "日本語"`. **No** `accepted`, **no** `key-pressed <Return>`. Latin placeholder is in Plex, 日本語 in Yu Gothic UI. | |
| 2 | JA. Type `kanji`, Space (candidate list opens), Space again. | Candidate list directly **below the start** of the composition in the LineEdit (not at the window corner, not at screen 0,0). Arrow Down moves within the list. | |
| 3 | ZH Pinyin. LineEdit (1): type `zhongwen`, then `1` or Space. | Inline pinyin (zhong'wen) while typing; candidate bar under the field; 中文 committed; one `edited`. | |
| 4 | KO (2-set). LineEdit (1): type `gksrmf dlqfur`. | 한글 입력 with the space in the right place. Each syllable shows inline while being composed. No doubled or reordered jamo. | |
| 5 | KO. Type `gks` (한 composing), press Left. | 한 is committed first, **then** the caret moves left of it. | |
| 6 | JA. Type `watashihanihongonobenkyouwoshiteimasu`, Space, then Left/Right, Space, Enter. | Inline composition grows. After conversion, Left/Right switch the active clause (caret jumps between clauses; the candidate list re-targets). Note how readable it is: whole composition highlighted, no underline (F1). Enter commits 私は日本語の勉強をしています or similar. | |
| 7 | JA/ZH. While composing, press Left/Right/Home/End. | Slint's caret does not move and the log shows **no** `key-pressed`. After commit, arrows move the caret normally and are logged. | |
| 8 | Narrow LineEdit: type `aaaaaaaa`, then a long JA composition (`toukyoutokkyokyokakyoku`) at the end. | Record: is the end of the composition visible before commit? (Expected per F6: it is clipped at the right edge until commit, then the field scrolls.) | |
| 9 | TextEdit (2): click at the end of the first wrapped line ("…wraps"), type `nihongonihongo`, Space. | The composition wraps to the next line correctly. Record where the candidate list appears and whether it covers the composition. | |
| 10 | PlexInput (3): repeat 1, 3, 4. | Same as 1, 3, 4. The red line below shows the raw composition while composing and is empty after commit. Latin in Plex, kana and kanji in Yu Gothic UI, hanzi in YaHei UI (with `--cjk zh`), Hangul in Malgun Gothic. | |
| 11 | JA: type `nihongo`, Esc. Then type `nihongo`, Backspace ×2, Enter. | Esc: composition disappears, field unchanged, panel-style Esc is **not** logged. Backspace edits only the composition (にほん → にほ); the commit inserts にほ. | |
| 12 | JA: type `nihongo` (don't commit), click PlexInput. | Record one of: committed into LineEdit / committed into PlexInput / dropped. Any of these is acceptable if consistent; text must not be duplicated. | |
| 13 | JA: in PlexInput type `nihongo`, click elsewhere in the same text. | Record: the composition moves with the caret (known caveat), commits, or is dropped. Nothing may be duplicated or corrupted. | |
| 14 | KO and JA: type `gks` / `nihongo`, Alt+Tab to another app and back. | Known bug #10861: the composing text is lost. Record the actual behaviour. Snipdeck works around it with `ImmNotifyIME`. | |
| 15 | With the app focused and the IME set to English, press Win+Space to Microsoft Pinyin, then type. Repeat on Windows 10 if available. | Heartbeat in the top line keeps counting and input appears. **If the heartbeat stops**, note the renderer and OS build and retry with `SLINT_BACKEND=winit-software` (#5206). | |
| 16 | Restart with `--cjk ja`, then `--cjk zh`, then `--cjk system`. Compare row "default stack" with the "Yu Gothic UI" and "Microsoft YaHei UI" rows. | `ja`: default-row Han glyphs match the Yu Gothic row. `zh`: they match the YaHei row (look at 直, 骨, 角, 今, 海). `system`: record which one DirectWrite picked. Kana always Yu Gothic, Hangul always Malgun. "no glyph" row: blank or box, no crash. | |
| 17 | Repeat 1, 3, 4 with `set SLINT_BACKEND=winit-software`, and if MSVC is available, `cargo run --features skia` + `SLINT_BACKEND=winit-skia`. | Identical behaviour and look in all renderers (F7). | |
| 18 | Display scaling 150% (and a second monitor with different scaling if available): repeat 1–3, with the window on each monitor. | Candidate list still directly under the composition; no offset that grows with the scale factor. | |

**What to send back:** the Result column, the startup diagnostic lines, and screenshots of steps 1, 6
(after conversion), 9, and 16.

## Sources

- Slint source 1.18.1 (crates.io): `i-slint-core/items/text.rs`, `i-slint-core/textlayout/sharedparley.rs`,
  `i-slint-core/textlayout/sharedparley/shaping.rs`, `i-slint-backend-winit/winitwindowadapter.rs`,
  `i-slint-common/sharedfontique.rs`, `i-slint-compiler/widgets/common/{lineedit,textedit}-base.slint`,
  `slint/lib.rs` (`fontique_011`).
- winit 0.30.13: `platform_impl/windows/{event_loop.rs,ime.rs,keyboard.rs,keyboard_layout.rs,window.rs}`, `src/changelog/v0.30.md`.
- fontique 0.11.1 (`backend/dwrite.rs`, `collection/{mod.rs,query.rs}`, `fallback.rs`) and parley 0.11.1 (`shape/mod.rs`).
- Slint CHANGELOG and git history (slint-ui/slint, `main` at 12544234e). GitHub issues
  [#10861](https://github.com/slint-ui/slint/issues/10861), [#5206](https://github.com/slint-ui/slint/issues/5206),
  [#8716](https://github.com/slint-ui/slint/issues/8716), [#5982](https://github.com/slint-ui/slint/issues/5982),
  [#3811](https://github.com/slint-ui/slint/issues/3811), [#4698](https://github.com/slint-ui/slint/issues/4698),
  [#13540](https://github.com/slint-ui/slint/issues/13540); [Slint font handling docs](https://docs.slint.dev/latest/docs/slint/guide/development/fonts/);
  [iced#3189](https://github.com/iced-rs/iced/issues/3189) (similar Windows IME candidate-window oddities in another winit app).
