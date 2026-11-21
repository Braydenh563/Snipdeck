# v1.1 slice 4: Tips content (draft for the builder)

Spec: `v11-spec.md` slice 4. Ten cards, each: title (≤ 32 chars), two lines (≤ 90 chars each), a picture
(a crop of an existing golden, light theme, so it refreshes with the goldens), and a "Try it" that runs one
`Command` (`crates/snipdeck-app/src/commands.rs:16`). Order = the order a new user meets the features.
Words follow DESIGN.md voice: plain, second person, no exclamation marks.

| # | Title | Line 1 | Line 2 | Picture (golden) | Try it |
|---|---|---|---|---|---|
| 1 | Open the Panel from anywhere | Press your Panel shortcut in any app to bring up your snippets. | Pick one and it pastes where you were typing. | `panel-light.png` | `ShowPanel` |
| 2 | Find anything by typing | Start typing in the Panel or Main to filter every list at once. | Titles, text and labels all count. | `panel-search-light.png` | `Search` |
| 3 | Keep favourites in reach | Pin the snippets you use most; they stay at the top of the Panel. | Unpin them the same way. | `panel-pinned-overflow-light.png` | `GoPinned` |
| 4 | Paste several in order | Paste next pastes the next item each time you press it. | Handy for forms you fill field by field. | `panel-cards-light.png` | `PasteNext` |
| 5 | Fill in the blanks | Templates ask for names, dates and options before they paste. | Add `{field:Name}` or `{choice:Tone|warm|firm}` to a snippet. | `fill-light.png` | `NewItem` |
| 6 | Grab text as you go | Quick capture saves what you copied as a new snippet. | Recent shows what you used and captured lately. | `panel-recent-rows-light.png` | `QuickCapture` |
| 7 | Symbols at your fingertips | Emoji, kaomoji and icons live in the Panel's Symbols tab. | Search them by name, in your language. | `panel-focus-tab-light.png` (Symbols tab crop) | `Symbols` |
| 8 | Every command, one search away | Press Ctrl+K in Main to find any action by name. | The shortcut for each is shown beside it. | `palette-light.png` | `Shortcuts` |
| 9 | Make it fit your screen | Choose comfortable rows, compact rows or tiles. | Light, dark or follow Windows. | `main-tile-light.png` | `ViewTile` |
| 10 | Keep secrets secret | Mark a snippet Sensitive to hide it until you reveal it. | A master password locks the whole deck. | `panel-row-selected-sensitive-dark.png` | `Settings` |

Notes for the builder
- Tip 7 picture: crop `panel-focus-tab-light.png` only if it shows the Symbols tab; otherwise render a
  Symbols-tab golden first (the Symbols tab has goldens under the symbols tests; grep `symbols-` in goldens).
- Tip 8 "Try it" should open the palette itself if a command for it exists by then; `Shortcuts` is the
  fallback because the palette is not in `Command::ALL`.
- Tip 10 uses a dark golden on purpose? No: keep all pictures light (decision: one theme for crops); find or
  render a light Sensitive row.
- Crops: 2x software-renderer goldens are large; crop to the feature (about 360 x 200 logical) and store
  under `crates/snipdeck-ui/ui/images/tips/NN-name.png`; a small script under `scripts/` regenerates them
  from the goldens so they never drift (test `every_tip_has_an_image_that_exists`).
- All strings go through `@tr` / the .po files (six locales). Picture text stays English (now.md decision).
