# Final pre-release audit 2 (1.0.0)

Audited the committed branch `origin/claude/compassionate-pascal-pod8ux` at b20e19eb. Every file was read from a `git archive`
snapshot, so the other agent's uncommitted edits did not affect what I read. No cargo was run. Nothing was compiled, tested
or seen in a running app. The one script I ran was `scripts/i18n.py check` (and `--strict`) on the snapshot; both passed.
Windows-only code was read, not run.

Range of the change review: `2fed013f..b20e19eb`, 15 commits. The app/core/store/portable diff (24 KB) and the UI diff
(Slint, tests, .pot/.po) were read line by line.

## Findings, most severe first

### 1. medium: in the Symbols text group strip, every hidden grid row still takes a layout gap, so the buttons scatter across a wide scroll strip and never fill the width
`crates/snipdeck-ui/ui/symbols.slint:274-296` (f7a0bf93)
- **Scenario:** the user opens Symbols > Icons or Characters with an empty search. The strip should show the group buttons sharing the row (HANDOVER: "group buttons fill the strip with a gap"). Instead the buttons sit at their minimum width. Between two buttons is a gap of 4 px for every grid row in the first group. The strip's content width grows by about 4 px per grid row, which comes to hundreds of px for Icons (1,512 icons, 7 per row in the 320 golden) and more for Characters. The later groups can then be reached only by scrolling sideways. Kaomoji has the same problem on a smaller scale.
- **Evidence:**
  - The strip repeats over the whole grid model: `for grow in root.rows`, with `visible: grow.group >= 0` and `min-width`/`max-width` 0 for rows that are not headers.
  - `rows_of` puts every grid row in that model (`crates/snipdeck-app/src/symbols_binding.rs:227-293`). Only title rows carry `group >= 0`.
  - In Slint 1.18.1, `visible` is lowered to a `Clip` element (`i-slint-compiler-1.18.1/passes/visible.rs`), and the child stays in the layout.
  - The box solver counts `spacing * (len - 1)` over all children (`i-slint-core-1.18.1/layout.rs:1121-1124`, `:271`). So `textstrip.preferred-width` includes one `Space.s1` for every grid row.
  - `width: max(parent.width, self.preferred-width)` then always takes the preferred width, so `alignment: stretch` has nothing to share out.
- **Cost:** the strip also builds a Rectangle, FocusScope, Text, TouchArea and a conditional ring for every grid row. Each kind switch rebuilds them. This is a likely contributor to the owner's 2026-11-23 report that switching kinds froze the app (now.md Inbox). Unmeasured.
- **Why the tests miss it:** the fixtures set `group: -1` on every row (`crates/snipdeck-ui/tests/symbols.rs:36,103`), so no golden draws the strip. `symbols-icons-320-dark` and `symbols-kaomoji-320-light` show an empty band where it would be.
- **Fix direction:** give the strip its own model of the group headers only, built in Rust (or use an `if` per element outside a layout). Add a render test with real group headers.
- Not verified in a running app. The mechanism is from the Slint source.

### 2. medium: in high contrast, the sidebar's keyboard focus ring vanishes on the selected row
`crates/snipdeck-ui/ui/components/sidebar-item.slint:104-112` (168a429b)
- **Scenario:** with Windows high contrast on, the user presses Tab into the Main window sidebar. `focus-gained` puts the cursor on the open view (`main_window.slint:1605-1608`; also `open-drawer`, `:839`), so the ring is drawn on the selected row. That row's fill is `bg-selected` = `SystemLook.hc-highlight` (`theme.slint:125`). The ring is `focus-ring` = the same `hc-highlight` (`theme.slint:155`).
- Now that the ring sits inside the row, it is drawn on a fill of its own colour and cannot be seen. The user cannot tell whether the sidebar has focus (WCAG 2.4.7).
- **Before the change:** the ring was 4 px outside the row, on the window colour, and could be seen.
- Normal themes are fine: `#2870D4` on `#EBF2FE`.
- **Fix direction:** in high contrast, draw the ring in `hc-window-text` (or inset it 2 px) when the row is selected. Add a high-contrast render test.

### 3. medium: in high contrast and in the Cards layout, the Panel's Enter-target row is marked only by a 2 px bar
`crates/snipdeck-ui/ui/components/item-card.slint:472-476, 611-618`; `crates/snipdeck-ui/ui/panel.slint:2360-2362` (5cbddbe4, b20e19eb)
- **High contrast:** the cursor row now takes `bg-hover`, which is `SystemLook.hc-window` in high contrast (`theme.slint:124`), the same as every other row. Its border is `border-subtle` (`hc-window-text`), as on every row. Before this change, the row was filled with the system Highlight colour and its text used HighlightText. Now the only sign of the row that Enter, Delete and Space-reveal act on is a 2 px Highlight bar at the left edge.
- **Cards layout:** the fill rule is `(hovered || cursor) && !tile`, so a tile never takes the cursor fill. The accent border is reserved for `selected`. The `panel-cards-light` golden shows the "Invoice reminder" tile picked out only by a thin bar and its action icons.
- **Tests:** no test renders the cursor row in high contrast or Cards. The high-contrast test (`tests/panel.rs:1538`) measures only the title mark.
- The owner asked for a "subtler" cursor in the light, list-layout case. These two looks were not considered.
- **Fix direction:** keep `bg-selected` for the cursor row when high contrast is on. Give the tile cursor an accent border or a ring.

### 4. low: the Recent list and the Picker still show their cursor row with the full selection fill
- **Code:** `crates/snipdeck-ui/ui/panel.slint:2577` (`selected: i == root.rsel` on the Recent ItemCard) and `crates/snipdeck-ui/ui/picker.slint:667` (`selected: i == root.sel`).
- **Effect:** these are the look the owner rejected for the Panel list ("is it highlighted because it is the latest entry??"). In the Recent tab, the same Panel window now shows two different cursor looks.
- **Fix direction:** decide once (DESIGN.md §4) and use `cursor` in both places, or record why they differ.

### 5. low: when a copy arrives while Recent is scrolled down, the list does not keep its place, and the keyboard cursor moves to a different entry
`crates/snipdeck-ui/ui/panel.slint:2547-2554` (6ad28d7b)
- **Scrolling:** the comment says a list the user scrolled down "keeps its place". The handler leaves `viewport-y` alone, and the whole model is replaced with the new entry at index 0 (`binding.rs:1735`). So the visible rows shift down by one row. The inbox plan (now.md, 2026-11-23) asked for the inserted row's height to be added to `viewport-y` instead.
- **Keyboard cursor (pre-existing, now more likely to matter):**
  1. With a docked Panel on Recent, the user arrows to row 3 and copies something in another app.
  2. `recent-selected` stays 3 (`panel.slint:289, 664`) and now points at the entry that was row 2.
  3. Enter pastes that entry.
- **Fix direction:** keep the cursor on the same entry id (shift `recent-selected` by the number of rows inserted above it).

### 6. low: the revealed text of a sensitive row is cut differently from every other preview (Picker and Panel)
- **Code:** `crates/snipdeck-app/src/controller/use_ctl.rs:244-258` (new `picker_view`) and `crates/snipdeck-app/src/controller.rs:772-781` (Panel). Both use `text.chars().take(Item::PREVIEW_CHARS)`.
- **Difference:** `Item::preview_of` (`snipdeck-core/src/item.rs:127-150`) drops leading whitespace and never cuts inside a grapheme cluster.
- **Scenario:** a secret that starts with a newline or a blank line shows an empty first line in the one-line row (`item-card.slint:802-813`: one line high, `no-wrap`), so the reveal looks blank.
- **Fix direction:** both places call `Item::preview_of(&text, false)` (one helper, not two copies).

### 7. low: two more controller commands are called only by tests
- **The commands:**
  - `Controller::paste_create` and the private `read_inline` it uses (`crates/snipdeck-app/src/controller/create_ctl.rs:68-73, 107`). The windows call `paste_begin` (`binding.rs:1330`, `main_binding.rs:941`). The only caller is `controller/tests/panel_select.rs`.
  - `Controller::submit_fill` (`crates/snipdeck-app/src/controller/use_ctl.rs:451`). The only callers are in `controller/tests/use_paste.rs`.
- They are the same class as the last audit's finding 4 ("replace, do not layer"). The tests drive a path users never take.

### 8. low: the docs have drifted from the new looks
- **DESIGN.md §4 ItemCard (`DESIGN.md:273`):** says "`bg.selected` when selected. Selection is a fill ... No coloured edge, except the 4px item label bar". It does not describe the 2 px accent bar or the new cursor state (hover fill and bar, read as selected by screen readers).
- **`DESIGN.md:89`:** the focus ring is "2px ring, 2px offset". The sidebar ring is now inset, and only a code comment records the exception.
- **`docs/SPEC.md:63`:** still says "**quick new note** (inline)"; the UI says "New item".
- **`docs/HANDOVER.md:5-7`:** says the group buttons "fill the strip". See finding 1.

### 9. low (tests): two render tests keep their "selected row" names but now check the cursor row
- **The tests:** `panel_row_selected_sensitive_dark` and `the_selected_row_wears_a_two_px_accent_bar_on_its_left_edge` (`crates/snipdeck-ui/tests/panel.rs:224-233, 1227-1240`).
- **Not weakened:** they assert the new look, and `panel_row_hover_light` adds a guard that the selection fill is absent.
- **Gap:** no render now covers a sensitive row on the selection fill (a marked row). The select-mode golden at `:294-305` marks rows 1 and 3, which are not sensitive. So the masked-text contrast on `bg-selected` is untested.
- **Test-only crash:** `first_row_of` computes `bar_top - 4` in u32, which underflows (a test panic) if the bar is ever found above y=4.

## Changes since 2fed013f, reviewed

| Change | Verdict |
|---|---|
| Picker sensitive reveal (`picker_view`, `use_ctl.rs:244-270`) | Correct. The decrypted text reaches only the revealed row. `picker_reveal` reveals only `sensitive` rows when `reveal_allowed()` (`use_ctl.rs:215-226`). Locked, `item_text` fails and the preview stays "". `picker_row` (`picker_binding.rs:391-399`) shows the text only when revealed. The detail pane still masks. Cut rule: see finding 6. |
| Lock clears `picker_revealed` (`lock_ctl.rs:695`) | Correct. The new test checks it (`controller/tests/picker.rs:357-387`). |
| Backup folder (`paths::open_library_folder` takes `backups`; `main.rs:1297-1302`; `settings_binding.rs:1986-1993`) | Correct. `settings.backup_folder(&folder)` is the same resolver every other caller uses. Every `backups_dir(` outside tests is now inside `Settings::backup_folder` (`settings.rs:1296`). New tests cover both paths. |
| Removed `rename_saved_colour`, `drop_create`, `main_show_panel`, `picker_use` | No non-test callers remain. The rewritten tests use helpers that copy the bindings exactly: `pick_and_use` = `picker_binding.rs:177-193`; `drop_inline` = the windows' `drop_begin`/`next_job`/`drop_read_done` loop. Not weakened, except that `main_controller.rs:1962` dropped "already up: brings forward". That behaviour belonged to the deleted function; the toolbar toggle hides instead. |
| Sidebar focus ring inside the row | Fine in light and dark. High contrast regressed: finding 2. |
| "New note" -> "New item" | All six .po files have "New item". "New note" is gone from the .pot and .po files. `i18n.py check --strict` passes. The .pot `#:` lines for "New item" do not list `panel.slint` (cosmetic; the check ignores references). Old wording remains in SPEC.md:63 and in `docs/reviews/*` (historical). |
| Symbols strip full width | Finding 1. |
| Panel cursor row (`cursor` property) | Nothing relied on ItemCard `selected` meaning the cursor. Enter, Delete, Space-reveal, F2, drag and group toggling all use `root.sel` / `selected-index`, not the card property. Select, delete and marked modes still use `selected: item.marked` with `show-focus` for the cursor. `show-actions` keeps the same condition. `accessible-item-selected` is `selected \|\| cursor`, so screen readers are unchanged. Text colours use `selected` only, which is right on the hover fill. Looks: findings 3 and 4. |
| Recent `changed rows` resets `viewport-y` | Fires: the binding sets a fresh `ModelRc` on every change (`binding.rs:1731-1743`). Behaviour gaps: finding 5. |
| Tests in `crates/snipdeck-ui/tests/panel.rs` and `panel_logic.rs` | Updated, not weakened. Gaps: finding 9. |
| Goldens (about 45) | Viewed: `panel-cards-light`, `symbols-icons-320-dark`, `symbols-kaomoji-320-light`. They match the code. The two symbol goldens never draw the group strip (finding 1). |

## Coverage

| Area | What I looked at | Result |
|---|---|---|
| snipdeck-core | Scans for `unwrap`/`expect` and slicing outside tests; `item.rs` (`preview_of`); `trigger.rs:323-340`; `template/locale.rs:495-510`; `search/query.rs` slices | Clean |
| snipdeck-store | Key ring derivation (`crypto/keyring.rs:149-160`, runs on `master_job` off the UI thread); preview CHECK constraint | Clean; earlier deep audit in `data.md` |
| snipdeck-portable | `backup.rs` (name-prefix matching for listing and retention); `limits.rs`/importer caps; `export_to_path` from `headless.rs` | Clean |
| snipdeck-colour | `parse.rs` and `model.rs` slices (`digits[i..i+2]` guarded by the length check) | Clean |
| snipdeck-symbols | Row data as consumed by `symbols_binding.rs:227-293` | See finding 1 (UI side) |
| snipdeck-platform | `cf_html.rs` (bounds), `handover.rs` (untrusted second-launch input: allow-list, caps, `parse_file`), `clipboard_watch.rs` privacy markers | Clean; Windows paths not run |
| snipdeck-platform `windows/` | `eyedrop.rs:453` and `hello.rs:137` allocation sizes; `single_instance`, `typing_hook` by reference to `platform.md` | Read only, unverified |
| snipdeck-ui: `panel.slint` | ItemCard instantiation (2320-2440), Recent list (2544-2615), New menu, empty state | Findings 3, 4, 5 |
| snipdeck-ui: `picker.slint` | Row ItemCard (646-692) | Finding 4 |
| snipdeck-ui: `symbols.slint` | Kind tabs, emoji strip, text strip, grid | Finding 1 |
| snipdeck-ui: `components/item-card.slint` | Fill, accent bar, actions, accessibility label and description, eye | Findings 3, 6 |
| snipdeck-ui: `components/sidebar-item.slint` | Focus ring, fill | Finding 2 |
| snipdeck-ui: `theme.slint` | High-contrast mappings of `bg-hover`, `bg-selected`, `focus-ring`, borders | Used for findings 2 and 3 |
| snipdeck-ui: `main_window.slint` | Sidebar focus and cursor (1600-1700, 835-842), tag-delete confirmation, board menu | Clean |
| snipdeck-ui: other windows (`settings`, `editors`, `colours`, `help`, `master_password`, `onboarding`, `template_fill`) and components | Callback scan (script; flagged nothing new beyond global or non-window false positives); no diff since the last audit | Clean, carried over from `final.md` |
| snipdeck-ui tests and goldens | Diff of `panel.rs`, `panel_logic.rs`; three goldens viewed | Finding 9 |
| i18n | `scripts/i18n.py check` and `--strict` on the snapshot; "New item" in 6 locales | Clean |
| snipdeck-app: controller (use, lock, create, colour, settings, archive, main_ctl lists and editors, panel) | New `picker_view`, `lock_now`, `paste_create`/`paste_begin`, restore, rebuild, backup now and daily backup, board delete and undo toast, `main_reorder_list` bounds | Findings 6, 7 |
| snipdeck-app: bindings | `picker_binding` (row, render), `binding.rs` recent rows, `settings_binding` (backup folder, hotkey rows), `main_binding` paste | Clean |
| snipdeck-app: `main.rs` and `paths.rs` | `open_library`, set-aside notice | Clean |
| snipdeck-app: logging | Every log macro argument that names text, title, query, clipboard or password | Clean |
| snipdeck-app: dead code | Script over `pub fn` with no non-test caller | Finding 7 (the rest are test fakes and accessors) |
| Panel | Cursor and selection, Recent tab, New menu, empty state | Findings 3, 4, 5 |
| Picker | Reveal, lock, row rendering, test helpers | Findings 4, 6 |
| Main window | Sidebar focus, board delete, palette "Show Panel" toggle | Finding 2 |
| Item editor, fill form | Unchanged since the last audit; `insert_at_caret` and `prefix_of` boundary handling | Clean |
| Settings | Backup folder, Open backups folder, backups to keep, hotkey rows | Clean |
| Colours | Saved-colour edit goes through the item editor; `colours_panel::commit` index guard | Clean |
| Symbols | Group strip, grid model | Finding 1 |
| Help and doctor | `backup_dates` (help and headless) use the chosen folder | Clean |
| Unlock and master password | Lock clears all three reveals; key derivation off the UI thread | Clean |
| Import, export, backup, restore | Restore and rebuild flow (`settings_ctl.rs:386-560`), retention, size limits, handover file arguments | Clean |
| Trash and undo | Board, list and group delete with Undo (`editors.rs:381-394`) | Clean in code; see "Not verified" |
| Tray | Not changed; Windows only | Unverified |
| Triggers and typing expansion | `trigger::prefix_of` skips sensitive items; hook by reference | Unverified (Windows) |
| Recent clipboard | Privacy markers honoured (`clipboard_watch.rs:29-30, 108-122`); list behaviour | Finding 5 |
| Docking | Not changed since the last audit; the owner's open item (DWM border on the collapsed strip) is already in the Inbox | Unverified (Windows) |
| Docs and spec | SPEC, user guide, HANDOVER, DESIGN against this diff | Finding 8 |

## Checked and clean

- **The last audit's four findings are fixed:** the Picker reveal shows the text and a lock clears it; the chosen backup folder is used in both places; the group-delete docs now say "acts at once with Undo" (SPEC.md:66 and user-guide.md:224-233); the four test-only commands are removed.
- **No `unwrap`/`expect` on user data outside tests** in any crate. The only one is the settings JSON serialise, which cannot fail. The string slices I sampled clamp to a character boundary first: `search_suggest::word_at`, `item_editor::insert_at_caret`/`prefix_of`, `panel.rs:1059`.
- **Untrusted input is capped:** second-launch requests (`handover.rs`) and archive imports (`limits.rs`, per-importer caps).
- **Retention and listing touch only `snipdeck-backup-*` names**, so a chosen folder shared with other files is safe.
- **Screen readers:** a masked sensitive row's accessible label is its title only, and its description is the hint (`item-card.slint:485-489, 404`).
- **No sensitive text in logs** from the paths sampled.
- **Translations are complete:** `i18n.py check --strict` passes.

## Not verified

- Nothing was compiled or run, and no tests were executed. Every visual claim (findings 1-5) comes from code and three golden PNGs, not from the running app or a measurement.
- Finding 1 rests on the Slint 1.18.1 source: the layout solver counts spacing for children hidden with `visible: false`. A render test with real group headers would settle it.
- **Windows-only paths:** tray, typing hook and expansion, `in_password_box`, docking and DWM, eyedropper, Windows Hello, single-instance handover, clipboard restore.
- **Owner report in the Inbox (2026-11-23):** "no confirmation or anything appears when I delete" a board. In code, `trash_board` shows an Undo toast, and the Main window draws the shared toast. Not reproduced here.
- **Not re-read:** most of the about 59,000 lines changed since the area audits of 2026-11-16 (eyedrop, template, main window, the search pipeline). This pass sampled them with scripts and spot reads. A fresh area audit of `main_window.slint`/`main_window.rs` and `eyedrop.rs` would close the gap.
