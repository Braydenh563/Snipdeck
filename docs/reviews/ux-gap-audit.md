# UX gap audit (2026-10-25)

Owner goal: "it needs to be a small handy tool, it doesnt need to be extravagant but it needs to be the best at
what it does" and "ux and ui must be maximised".

Method: I read SPEC, DESIGN, `panel-v2-spec.md`, the user guide, `feedback-audit.md`, `shortcut-visibility.md`
and the HANDOVER v1.0.1/v1.1 lists. I looked at 24 goldens (panel-light, -search, -row-hover, -empty, -no-target,
-app-menu, -no-results; picker-light, -copy-board; main-light, -tile, -empty-all; settings-light; item-editor-text,
-template; fill-light; onboarding-welcome, -shortcuts, -panel, -startup) and grepped the code for defaults and
behaviour. I did not run cargo or the app. I did five web searches.

**Not verified.** Goldens are software-renderer images built from test fixtures. Some fixture data does not
match real behaviour: All items shows 42 but the status bar says "8 items"; the Picker "This board" scope lists
other boards; "Retry helper" has no search mark. I did not count those as bugs. Nothing that only happens on
Windows is checked here: paste and clipboard restore, Target detection, hotkey registration, focus handover,
render glitches, dock behaviour and real window heights. Pixel positions below are read off the goldens at
100 % scale.

## 1. Core jobs and speed today

An "action" is one chord, one click or one Enter. Typed search letters count as one action. The Snipdeck numbers
use today's defaults from `settings.rs`: `panel_pinned: true`, `panel_hide_after_use: false`,
`recent_history: false`, Capture chord `""`.

| # | Job | Snipdeck today | Best competitor | Gap |
|---|---|---|---|---|
| J1 | Paste a saved snippet into the app I'm in | Panel: hotkey, click (2). The Panel then **stays on screen** (pinned by default, no hide-after-use), so closing it takes a 3rd action (Esc, hotkey or X) | Win+V: Win+V, click (2, closes itself). Paste/Maccy: chord, Cmd+1 (2) | **+1 action, every use.** Fails "quick flick open and close" |
| J2 | Paste something I copied earlier (history) | **Not possible by default** (Recent is off). With Recent on: hotkey, clock tab, row (this **copies only**, `recent_ctl.rs:217`), Esc, Ctrl+V = 5 | Win+V / Ditto / Maccy auto-paste: 2 | **Biggest gap.** This is the job most people hire a clipboard tool for |
| J3 | Keep what's on the clipboard as a snippet | Capture chord is off by default. Panel: hotkey, New, Save from clipboard, (Esc) = 3-4 | Win+V: Win+V, ⋯, Pin = 3. Ditto: automatic | Par. Ctrl+V into the Panel (O47) would make it 2 |
| J4 | Find one snippet among many and paste it | Picker: hotkey, type, Enter = 3; closes itself; searches every board | Raycast / Ditto: the same 3 | Par. Picker look lags the Panel (§2) |
| J5 | Paste a template with fill-ins | hotkey, click, fill N fields, Ctrl+Enter = 3+N | TextExpander: trigger, fill, Enter = 2+N | ~Par. The form is bulky (§2) |
| J6 | Paste a secret without leaking it | hotkey, click (2). Marked private, kept out of Win+V history, auto-clear when copied | Win+V has nothing like it. Ditto: manual | **Ahead.** Picker still shows trailing characters (§2) |
| J7 | Expand an abbreviation while typing (`;addr`) | Not available (v1.1, off by default) | Espanso / Beeftext / TextExpander: type 4-5 keys, no window | Missing. The typed-trigger crowd's whole job |
| J8 | Fix the text of a snippet | Panel hover, pencil, edit, Ctrl+Enter = 3 + typing. The editor window is mostly empty (§2) | Raycast: Ctrl+K, Edit = same | Par |
| J9 | Paste several snippets in order | Supporter: ⋯ then Add to stack for each (2n), Start, Ctrl+Alt+V × n | Paste: Shift+Cmd+C, then paste × n | Heavier setup (2n clicks). Acceptable for a paid boost |

The headline: J1 and J2, the two most frequent jobs, are each slower than Win+V, the tool that ships with Windows.
J1 costs an extra close every time. J2 is impossible by default and costs 5 actions when on. Fixing those two
does more for "the best at what it does" than anything else on this list.

## 2. Poor UX found in the goldens and flows (most user impact first)

1. **The Panel never leaves after use** (J1). On a default install `panel_pinned: true` makes the "Hide the panel
   when it loses focus" toggle (shown ON in onboarding step 3) do nothing. Panel v2's `panel_hide_after_use`
   defaults to `false`. The owner's verdict was "annoying to hide or get rid of". Win+V, Ditto, Maccy and
   Raycast all close after a paste.
2. **Recent rows copy and never paste** (J2). The code says so ("Pasting a Recent entry straight into the Target
   is not in v1"). The guide says a Panel click pastes, so a Recent click that only copies breaks the one rule
   people learn. It also makes Recent the only list with no "Pastes into Outlook" behaviour.
3. **Picker is a generation behind the Panel** (`picker-light`, `picker-copy-board`):
   - Plain-text rows have no icon, so their titles start at x=41 while other rows start at x=65 (24 px ragged).
   - The file row shows the "PDF" text and an icon twice.
   - The sensitive row shows `•••• •••• 7f3a`. Panel decision 7 removed trailing characters because always-on-top
     windows end up in screen shares, and the Picker opens over the app being shared.
   - The board/list separator is "/" in the Picker and "·" in the Panel.
   - Matches have no highlight.
   - The selected row has no Edit control (the known open gap in shortcut-visibility).
4. **Main window doesn't look like the same product** (`main-light`, `main-tile`):
   - Bare glyphs instead of the Panel's 32 px tiles, and plain text has no icon.
   - No visible Use/Paste on rows (only Copy and ⋯).
   - Seven unlabelled toolbar icons in a row: sidebar, undo, redo, commands, eye, three view modes. The eye and
     the keyboard-like icon can't be told apart without a tooltip.
   - "All items" and "Pinned" don't say which list an item lives in, although the Panel search does.
   - Tile view: every card is about 150 px tall with roughly half of it empty, and the type glyph sits above
     the title instead of in line with it (`{ }` at x≈548, title at x≈521).
5. **Editor windows are mostly empty space**:
   - `item-editor-text`: 760 px tall, content ends at y≈276, so about 60 % is blank.
   - `item-editor-template`: 980 px, about 40 % blank.
   - The "More options" button is a full-width outlined box that reads as an input.
   - Title and Kind are not visible at rest, although the guide says to "Pick a Kind".
   - Whether the window actually fits its height on Windows (owner item 5) is not verified.
6. **Placeholder chips insert at the end of the text, not at the cursor** (`editors.slint:546`: "Choose one to
   add it at the end of the text"). Every template writer has to cut and paste after each insert. The editor also
   shows the raw syntax `{field:Number}` while Main shows `{Number}`.
7. **Search can't explain a match**. `title_mark` marks the title only and returns `None` when the match came
   from the preview or a tag (`panel.rs:128`). A row that matched on its content or tag gets no hint why.
8. **Too many ways to create, in one place.**
   - The empty Panel (`panel-empty`) offers four create controls at once: header New, Save from clipboard,
     New note and "+ New item".
   - The populated Panel keeps "+ New item" in the list as well as header New.
   - Main's empty state (`main-empty-all`) has the opposite problem: only "New item", with no Save from
     clipboard and no Import… (the fastest way to fill an empty deck).
9. **Fill form is bulky** (`fill-light`, 440×664):
   - A `{ }` glyph is repeated before every one of the 5 labels.
   - A 3-option choice is shown as an always-open 90 px list, where a 36 px segmented control would fit.
   - The date field is typed `YYYY-MM-DD` with no default of today and no picker.
10. **Settings is one long scroll with no search** (`settings-light`, 1000+ px; `settings-full` is longer):
    - Copy leaks internals: "The Panel keeps its own row height for now".
    - Capture's description reads "Off until you set a shortcut" right next to a set chord.
    - "Paste the next item … Works with Supporter" sits as an ordinary editable row, with no lock state.
11. **Panel ⋯ menu (`panel-app-menu`) is still the old menu**: Open Snipdeck (already in the footer), Settings,
    Pause hotkeys, Help, Quit. The spec's Undo, Redo, Paste stack, Dock, Keyboard shortcuts are missing. The menu
    also covers the tab row. It may simply not be built yet (slices 5-9). The header ⋯ glyph renders noticeably
    paler than its neighbours and reads as disabled (unmeasured; check its token).
12. **Pinned chips truncate hard at 320 px**: "Thanks, rece…" fits 13 of 16 characters. With 2 chips and "+1
    more", the strip spends 36 px to show about 1.5 titles. Acceptable; check it at 260 px.

## 3. Missing features (and what to leave out)

**What a best-in-class small tool needs**

- **One-click paste from history.** Recent behaves like any list: click pastes, Shift copies. Images and files
  come later (already queued as "Recent captures images, files, rich text").
- **Close after use** when the Panel was opened by a hotkey or the tray. A pinned Panel stays. This is how every
  competitor behaves.
- **Paste or drop into the Panel to create an item** (O47, v1.0.1). This makes J3 two actions without a global
  Capture chord.
- **Hotkey health you can see.** Per chord: Working, "Taken by another app" or Off, with one-click alternatives.
  Show it in onboarding and Settings. The owner's own Ctrl+Alt+Space failed silently. Default chords that use
  Ctrl+Alt are AltGr on many European layouts, and the code already notes that Ctrl+Alt+V is Paste Special in
  Word (`settings.rs:491`).
- **Typed-trigger expansion** (v1.1, opt-in). This is the one feature Espanso, Beeftext, TextExpander and
  PhraseExpress users will look for first. Keep it to plain `;abbr` plus the existing templates, nothing more.
- **Importers for TextExpander, Espanso and Beeftext** (v1.1). Ditto, CopyQ and Win+V import already exist.
  Switching is only free if the old library comes with you.
- **Match highlight in content** and a "matched tag" hint (§2.7).

**Deliberately do not add (stay small)**

- PhraseExpress-style macros, conditional logic or scripting.
- Team or cloud sync and sharing ("share a board", folder sync stay later and optional).
- AI paste actions.
- Image editing (crop, redact): SPEC 5.4 lists it, but it is a separate app.
- The colour picker and eyedropper.
- Multi-panel and the O56 launcher.
- Rich-text editing.
- Per-app rules beyond the existing never-paste list.

Each of these adds settings and teaching cost to a tool whose value is "no thinking". I recommend moving the
colour picker, multi-panel, the O56 launcher and image editing out of v1.1.

## 4. First run and learnability

- **The tour teaches by reading, not doing.** Steps 1-4 are text and settings (`onboarding-*`). Nothing asks the
  user to press the Panel chord and watch it open. Make step 2 "Press Ctrl+Alt+Space now" with a live tick
  (or "Didn't work? Pick another"). That one interaction would have caught the owner's silent hotkey failure.
- **Step 3 asks for decisions before any use.** Dock, hide on focus loss and taskbar/Alt+Tab are three
  paragraphs of policy. Replace it with one visual choice (Floating or Docked right) and leave the rest to
  Settings. SPEC 5.1 says the first run docks right at full height, but the onboarding golden shows Floating:
  decide which one and make both say it.
- **Recent is offered last, as a toggle under a 4-line paragraph.** It powers job J2. Offer it as a clear choice
  on step 1 ("Also keep what I copy? Yes / Not now"), still opt-in.
- **Start with Windows defaults to off.** A tray tool that isn't there after a reboot fails "always handy". The
  toggle is on step 4, which is fine, but consider defaulting it on there, visibly, with the user able to turn
  it off.
- **Getting started list (seeded)** is good: it teaches by doing. Make sure it covers Recent and the Picker, not
  only the Panel.
- **Supporter features read as broken.**
  - Paste next is a default chord (Ctrl+Alt+V) that does nothing on Free.
  - `{field}` and `{choice}` chips are offered on Free.
  - Show a lock badge and "Supporter" on the control itself, not only in a toast after the fact.
- **Hotkeys as boosts** (owner rule) is mostly met (`shortcut-visibility.md`). The Picker has no visible
  entry point except the palette. Add "Open Picker" to the tray menu.

## 5. Ranked fix list

Sizes: S under half a day, M one to two days, L more. Queue: Panel v2, shared look ("Carry the Panel v2 look to
Main, Picker, palette, editors"), v1.0.1, v1.1, or NEW.

1. Close the Panel after paste/copy when it was opened by hotkey or tray; a pinned Panel stays; default on. S. Panel v2 (opening options; `panel_hide_after_use` semantics and default).
2. Recent: click/Enter pastes into the Target, Shift copies, same footer as other lists. S. NEW (joins "Recent captures").
3. Hotkey health: live per-chord status with "press it now" test in onboarding step 2 and Settings, plus one-click alternative chords. M. NEW (milestone item 2).
4. Picker shared look: icon column for every kind (fixes the 24 px ragged titles), mask with no trailing characters, "·" separator, match highlight, Edit pencil on the selected row. M. shared look.
5. Template editor: placeholder chips insert at the caret, not at the end. S. NEW.
6. Main shared look: Panel tiles, visible Use on row hover, "Board · List" line in All items/Pinned/search, tooltips or labels on the 7 toolbar icons. M. shared look.
7. Editors fit their content (goldens show 40-60 % empty), and "More options" restyled as a disclosure row. S. v1.0.1 (milestone item 5; verify on Windows).
8. Paste or drop into the Panel creates an item (makes J3 two actions). M. v1.0.1 (O47).
9. Onboarding step 3: one visual Floating / Docked choice; offer Recent on step 1; resolve the SPEC "docked right" vs Floating default. S. NEW.
10. Search explains matches: mark preview text too, and "tag: billing" when only a tag matched. S. Panel v2.
11. Supporter-gated controls show a lock badge in place (Paste next chord, `{field}`/`{choice}` chips). S. NEW.
12. Empty states: Panel shows two create actions (hide "+ New item" while empty); Main empty adds Save from clipboard and Import…. S. Panel v2 / shared look.
13. Panel ⋯ menu per spec (Undo, Redo, Paste stack, Dock, Keyboard shortcuts, All commands); fix the pale ⋯ glyph. S. Panel v2.
14. Fill form: drop the repeated `{ }` glyph, choices of 4 or fewer as a segmented control, date defaults to today with a picker. S. v1.1 (richer fill-ins).
15. Settings: search field plus section jump links; remove "for now" and the static "Off until you set" copy. M. NEW.
16. Default chords: avoid Ctrl+Alt (AltGr) for new installs; needs an owner decision. S. NEW.
17. Main tile view: card height fits content, glyph inline with the title. S. shared look.
18. Add "Open Picker" to the tray menu. S. NEW.
19. Typed-trigger expansion, opt-in, `;abbr` only. L. v1.1.
20. TextExpander, Espanso and Beeftext importers. M. v1.1.
21. Trim v1.1: drop colour picker, multi-panel, O56 launcher, image editing. S (a decision). v1.1.

## Sources

- [Maccy (README mirror)](https://github.com/HAOGRE/Maccy): copy by default, "paste automatically" option, Cmd/Option+number.
- [Paste keyboard shortcuts](https://pasteapp.io/help/keyboard-shortcuts): Quick Paste Cmd+1-9, paste stack, Shift+Return plain text.
- [Best clipboard managers 2026 (Blaze)](https://blaze.today/blog/best-clipboard-managers-windows-mac/).
- [Beeftext alternatives (TextExpander blog)](https://textexpander.com/blog/best-beeftext-alternatives) and [Espanso alternatives](https://textexpander.com/blog/best-espanso-alternatives): competitor-written, used only for feature lists.
- Win+V, Ditto, Raycast and PowerToys: the sources already cited in `panel-v2-spec.md` §0.
