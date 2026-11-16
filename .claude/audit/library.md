# Deep audit: library (agent `library`, head dd4f5847)

Scope: items, lists, boards, tags, pinned, trash, search, sort and grouping, bulk acts, reorder, undo/redo,
templates and the fill form, editor logic, duplicate detection, one item on every surface.
Missing lens compared with Raycast snippets, TextExpander and Alfred.

How verified: `run` = driven in the probe test `crates/snipdeck-app/tests/probe_library.rs` (deleted after the
run, never committed) with a fixed clock; `code` = read only. Probe output is quoted in section 4.

## 1. Coverage table

Status per row: `driven` (probe ran it), `checked` (read in full or the listed part), `not reached (why)`.

| Row (file: what was inventoried) | Status | How |
|---|---|---|
| app `controller/editor_ctl.rs`: open/cancel/delete, tag toggle and add, remove formatting, trigger field, autosave, draft | driven | run |
| app `controller/editor_ctl/open.rs`: open new, open item, open from Recent, read-only over 16 KB | checked (to line 330; the rest is draft restore) | code |
| app `controller/editor_ctl/save.rs`: save checks, key creation, insert at `Placement::First`, history record rule | driven | run |
| app `controller/group_ctl.rs`: group marked, rename, collapse, ungroup, delete, drop, move by keyboard, join | checked | code |
| app `controller/history_ctl.rs`: record_step, undo, redo, apply/revert of every `Change`, clear_history | driven | run |
| app `controller/library.rs`: `Library` trait bounds | checked | code |
| app `controller/tests/main_groups.rs` | not reached (existing test file; names skimmed only, time box) | code |
| app `controller/views.rs`: view structs for editor, fill, history | checked | code |
| app `editor_binding.rs`: save/cancel/delete/close wiring, autosave timer, preview, browse | checked (wiring 220 to 470; Slint window itself not driven: no harness run in the time box) | code |
| app `fill.rs`: FillModel, fields, toggles, first_missing, preview, submit | driven | run |
| app `fill_binding.rs`: draw, placement, submit, flag missing, give up | checked (to line 200) | code |
| app `history.rs`: History (100 steps), Step, Change, Snap, take_undo(by id), redo stack | driven | run |
| app `item_editor.rs`: EditorInput, check_text/link/template, PNG check, limits | driven | run |
| app `item_preview.rs`: card_of, expand, link_display | checked (card_of; tests skimmed) | code |
| app `list_text.rs`: list as text export | checked | code |
| app `search_mark.rs`: marks, title_mark, preview_mark | driven | run |
| app `search_suggest.rs`: Index, suggest, word_at, ranking of Tag/List/Word | checked | code |
| app `tests/editor_controller.rs`, `tests/fill_window.rs` | not reached (existing tests; not re-run: one cargo target at a time and the lock was contended) | n/a |
| app `tests/history.rs` | checked (rig and helpers read; pattern reused by the probe) | code |
| core `Cargo.toml`, `lib.rs`, `error.rs`, `ids.rs` | checked (exports; `ids.rs` and `error.rs` skimmed) | code |
| core `board.rs`, `list.rs`, `group.rs`, `tag.rs`, `usage.rs`, `placement.rs`, `key.rs`, `name.rs`, `content.rs` | checked | code |
| core `format.rs` | checked (first 230 lines) | code |
| core `item.rs`: Item, ItemKind, ItemLabel, preview_of | checked | code |
| core `search.rs`, `search/tests.rs`: SearchIndex, tiers, query, limits | driven | run |
| core `template.rs`, `template/tests.rs`, `template/transform.rs`: parser, every placeholder kind, date formats, Locale, transforms, limits | driven | run |
| core `trigger.rs`: Trigger, Matcher, prefix_of | driven | run |
| store `batch.rs`: transaction, atomically, blob bookkeeping | checked (API surface; blob GC is claude/fable2-last scope) | code |
| store `boards.rs`, `groups.rs` | checked (signatures and group gather/prune; bodies skimmed) | code |
| store `fts.rs`, schema tokenizer | driven | run |
| store `items.rs`, `rank.rs`, `tags.rs`, `trash.rs`, `triggers.rs` | driven (items, rank, tags, triggers); checked (trash) | run |
| store `tests/*.rs` (batch, groups, items, ordering, properties, search, tags, trash, triggers) | not reached (existing tests; `search.rs` test names read, none re-run) | code |
| ui `components/template_field.slint`: properties, callbacks, a11y roles, `@tr("Include")` | checked (properties and a11y lines; not rendered: no harness run in the time box) | code |
| Interaction walk at min/default/large size, text scale 1.0/1.5/2.25, light/dark/high contrast | not reached (the lock on the shared target dir was held by other runs for most of the time box; the one cargo run went to the logic probe) | n/a |

## 2. Findings

No cap: every finding is listed. Severity: critical, high, medium, low.

| id | sev | lens | evidence | verified | class-level fix | effort |
|---|---|---|---|---|---|---|
| LIBRARY-1 | high | Broken, UX | Main search goes to FTS5 (`main_window.rs:426`, `:946`), which indexes title and body only (`store/src/fts.rs`, `schema.rs:136`); tag names and list names are not indexed. Yet `search_suggest.rs:292-293` offers Tag and List suggestions in the Main search field, and choosing one completes the word in the query. Measured: an item tagged "invoices" is not found by `store.search("invoices")` (0 hits); Main query "email replies" (a demo list name) gives 0 rows. | run | One search contract for every surface: add `tags` and `list` columns to the FTS row (kept by the same triggers that keep title/body), or turn a chosen Tag/List suggestion into a filter chip (section + query) and never into query text that cannot match. | M |
| LIBRARY-2 | high | Broken | FTS tokenizer is `unicode61 remove_diacritics 2` (`schema.rs:136`): a run of CJK characters is one token, so a word in the middle of a Japanese or Chinese sentence is not found; the app ships ja and zh-Hans. Mid-word text ("ature" for "Signature") is not found either. Measured: "テキスト" inside "これは日本語のテキストです" gives 0 hits (the sentence start "これは" hits); "ature" and "esign" give 0 hits; Main query "irst" for "first real line" gives 0 rows. | run | Add a trigram FTS table (SQLite `trigram` tokenizer) used for queries of 3+ characters or any CJK query, and fall back to `LIKE` for 1 to 2 CJK characters; test with ja, zh, ko, Thai fixtures. | M |
| LIBRARY-3 | medium | UX | The same query gives different results on different surfaces: Panel and Picker are fuzzy over title, tags and 64 characters of content (`core/src/search.rs:37`), Main is whole-word/prefix over title and full body. "sig" style subsequence matches exist only in Panel/Picker; body text past 64 characters only in Main; tags only in Panel/Picker. SPEC 5.2 line 88 says the Picker searches "title, content, tags". Measured: text past 64 characters: 0 hits in `SearchIndex`, found by FTS; tag "invoices": found by `SearchIndex`, 0 hits in FTS. | run | One query pipeline: fuzzy title/tag pass plus FTS body pass merged, used by Panel, Picker and Main; a shared test table of (query, expected ids) run against each surface. | L |
| LIBRARY-4 | medium | Broken | Panel/Picker: each word of a multi-word query must match inside one field; "invoice work" with "invoice" in the title and "work" as a tag finds nothing (`core/src/search.rs` matches the whole pattern per field). Measured: "invoice work" (title + tag) 0 hits, "invoice customer" (title + content) 0 hits, "invoice" 1 hit. | run | Match per word across the union of fields (score = sum of the best field per word), as fzf-style launchers do. | S |
| LIBRARY-5 | medium | Broken | Tier ranking (`core/src/search.rs:50`, tier shifted above the score): any scattered fuzzy title match outranks an exact tag or exact content match. Measured for "sig": "Shipping instructions guide" (scattered s-i-g, tier 2, score 64) ranks first, the item tagged exactly "sig" second (tier 1, score 88), the item whose text starts "sig block" third (tier 0, score 88). | run | Rank on match quality first (exact > prefix > word start > subsequence) and field second; add use count and recency as a tiebreak (Raycast and Alfred both learn from use). | M |
| LIBRARY-6 | medium | UX | Highlighting: `search_mark::title_mark` (`search_mark.rs:92`) marks only when the whole trimmed query is one contiguous substring; multi-word, fuzzy and accent-folded matches show no mark although the row is a hit. Main passes `hit_tag: None` (`main_window.rs:809`), so a tag hit is never explained there. Measured: "emsig" on "Email signature": no mark; "cafe" on "Café menu": no mark; "straße" on "STRASSE": no mark; a match on the second preview line: no mark. | run | Mark from the matcher's own indices on every surface (per word, per field), so "why is this row here" is always visible. | M |
| LIBRARY-7 | low | Performance, half-built | `Hit.title_indices` is computed and sorted for every hit on every keystroke (`core/src/search.rs:277-291`) and read by nothing outside core (grep: no user in app or ui). | code | Use it for LIBRARY-6 or stop computing it. | S |
| LIBRARY-8 | low | Missing | No search operators: no `tag:`, `list:`, `kind:`, `is:pinned`, quoted phrase or negation. FTS input is reduced to quoted words (max 32, `fts.rs:11`), so `"kind regards"` and `-word` have no special meaning; "C++" is searched as "c". Alfred, Raycast and TextExpander all filter snippets by collection/keyword. Measured: `"kind regards"` and `regards kind` return the same item (no phrase), `kind -regards` still returns it (no negation), "C++" returns the same 3 items as "c" (Café, C++ tips, Signature). | run | A small query grammar parsed once (`field:value`, quotes, minus) and applied by every surface; document it in the search field's help. | M |
| LIBRARY-9 | medium | Missing | A trigger (abbreviation) is not searchable: `SearchIndex` holds title, tags and content only (`core/src/search.rs:141-150`), FTS holds title and body. Typing ";sig" in the Picker or Main does not find the item whose trigger is ";sig". Raycast, TextExpander and Alfred all search snippets by keyword. | code | Add the trigger as an indexed field at the top tier (exact trigger = first hit) on both indexes. | S |
| LIBRARY-10 | medium | Missing | Main shows one tag at a time (`Section::Tag(TagId)`, `main_window.rs:71-85`): no multi-tag filter (AND/OR), no "untagged", no kind filter (text, image, link, template, file), no "has trigger", no "sensitive". | code | A filter model (set of tags, kinds, flags) beside the section, shared by Main and Picker scope. | M |
| LIBRARY-11 | medium | Missing | Sort: `ListSort` has Manual, Most used, Recently used only (`core/src/list.rs`); no A to Z, date created, date modified, kind. All items, Pinned and Tag views have no sort choice at all (`main_window.rs:772-775` sorts only an open list). | code | One `Sort` enum (manual, name, created, modified, most used, recently used; ascending/descending) applied by every section, stored per section. | M |
| LIBRARY-12 | low | UX | A list sorted by use shows no groups (`main_window.rs:1331` "a list that sorts itself shows no groups") and cannot be reordered; grouping is then only offered through a toast (`group_ctl.rs:342`). Groups vanish with no explanation in the list. | code | Keep group headers under any sort (sort inside each group) or show an inline note "Groups show in Manual order" with a switch. | S |
| LIBRARY-13 | medium | Broken, UX | No optional text field: `{field:CC}` and `{field:CC|}` are both required (`fill.rs:209`, `:225`: `required: default.is_none()`, and an empty default parses to none). A person cannot leave a field blank on purpose. Measured: `{field:CC|}` and `{field:CC| }` both give `required=true`, `first_missing=Some((0, Empty))`. | run | Required only when no `|` is present; `{field:CC|}` means optional and empty. Same rule for multiline and pickdate. | S |
| LIBRARY-14 | medium | Broken | A required field inside an unticked `{optional:...}` section still blocks Submit: `first_missing` (`fill.rs:154`) walks every field and does not read the section toggles. Measured: `{optional:PS}PS {field:Text}{/optional}` with the section untouched: `first_missing=Some((1, Empty))`. | run | Validation walks the rendered tree: only fields that will be printed are required; disable those fields in the form when the section is off. | S |
| LIBRARY-15 | medium | Broken (SPEC 168 "locale-aware dates in {date}") | Month and weekday names are English in every language: `%B`, `%b`, `%A`, `%a` come from a fixed English table (`core/src/template.rs`); only the default numeric order changes with `Locale::for_tag` (`template.rs:339`). A German user's `{date:%d. %B %Y}` prints "17. October 2026". Measured with `Locale::for_tag("de")`: `{date:%e %B %Y}` gives "17 October 2026", `{date:%A %d %B}` gives "Saturday 17 October". | run | Month/weekday name tables per shipped locale (en, es, de, fr, pt-BR, ja, zh-Hans) behind `Locale`, with a test per locale. | S |
| LIBRARY-16 | low | Missing | Date format lacks unpadded day/hour/month (`%-d`, `%-H`, `%-m`), week number, quarter and time-zone name; `%e` pads with a space. Measured: `{date:%-d %B}`, `{date:%V}`, `{date:%Z}` are parse errors ("unknown date format"). | run | Support the common strftime flags (`-`, `_`) and `%V %G %Z`; reject unknown ones with the list of valid ones in the error. | S |
| LIBRARY-17 | low | Broken, consistency | `{date:FORMAT}` and `{datetime:FORMAT}` take a format but `{time:FORMAT}` is refused as an unexpected argument (`template.rs:1051`). Measured: `{time:%H:%M}` gives "`{time}` takes no argument"; `{datetime}` with no format is also an error while `{date}` and `{time}` have defaults. | run | One rule for the three clock placeholders. | S |
| LIBRARY-18 | low | UX | `{pickdate}` is a text box that wants `YYYY-MM-DD`; there is no calendar and the user's locale order (DMY, MDY) is not accepted (`fill.rs` date check). | code | Accept the locale's numeric order as well as ISO, show the format as placeholder text, add a small month grid with keyboard support. | M |
| LIBRARY-19 | low | Broken | No escape for `|`, `{` or `}` inside a prompt name, default or choice option, so a default such as "a|b" or a choice containing a brace cannot be written; literal braces in template text need doubling and a code snippet saved as a Template fails to parse. Measured: Template text `function() { return 1; }` is refused ("unknown placeholder"), `fn main() { }` is refused ("empty placeholder `{}`", with no hint to double the brace); `{field:A|x|y}` is refused. | run | A backslash escape (`\|`, `\{`, `\}`) in the parser and in the editor's placeholder inserter; the parse error should name the fix ("write {{ for a literal brace"). | S |
| LIBRARY-20 | low | UX | Prompt names are case-sensitive: `{field:Name}` and `{field:name}` make two form fields with near-identical labels. Measured: `{field:Name}{field:name}` gives two fields, "Name" and "name". | run | Compare prompt names by their case-folded, trimmed key (as tags do with `name_key`). | S |
| LIBRARY-21 | low | UX | Hard limits: 16 prompts and 8 date offsets per template (`template.rs:97`, `:100`). A long form letter hits them; the error is a parse refusal at save. | code | Raise to a size limit that is about cost (for example 64 prompts) and say the limit in the editor before save. | S |
| LIBRARY-22 | low | UX | A choice always starts on its first option and cannot name a different default (`fill.rs` choice field); no "none" choice unless the author adds an empty option, which the parser then treats as a trailing separator. Measured: `{choice:Size|S|M|L|}` is refused ("a choice option is empty"). | run | `{choice:Size|S|*M|L}` style default marker, and an explicit empty option. | S |
| LIBRARY-23 | medium | Missing | Compared with TextExpander, Raycast and Alfred, templates lack: nested snippets (`{snippet:name}`), `{selection}`, clipboard history by position, key presses (`{key:tab}`, `{key:enter}`) for form filling, date arithmetic beyond the fixed offsets, a number/calculation field, and reuse of one answer with a different transform across items. Measured: `{snippet:sig}`, `{selection}`, `{clipboard|upper}` are "unknown placeholder". | run | Decide and record which of these are v1.x; nested snippets and `{key:tab}` are the two most asked-for in this class of app. | L |
| LIBRARY-24 | low | Broken | `Locale::for_tag` takes the UI language, not the Windows regional format: en-CA, en-ZA, en-IN and every unlisted tag fall to US month/day/year and 12-hour time (`template.rs:339`). Measured `{date} {time}`: en-CA and en-ZA give "10/17/2026 10:30 AM" (same as en), en-GB "17/10/2026 10:30", de-CH "17.10.2026 10:30"; unlisted languages (nl, it) get ISO "2026-10-17 10:30". | run | Read the user's regional short-date and time pattern on Windows (`GetLocaleInfoEx`) and pass it to `Locale`; the tag table stays as the fallback. Windows run must confirm. | M |
| LIBRARY-25 | low | Missing | Transforms are Plain, Upper, Lower, Title, Trimmed, Slug (`template/transform.rs`): no sentence case, camelCase, snake_case, URL-encode, or first-word/last-word, which name fields commonly need. | code | Extend the enum; the transform list in the editor help is generated from it. | S |
| LIBRARY-26 | high | Broken (undo, data loss) | "Remove formatting" in the editor writes to the store at once (`editor_ctl.rs:349-378`), records no history step, and Cancel does not put the formats back. Every other field in the editor waits for Save. The rich version of the item is gone with no way back. | code | Editor actions change editor state only; the write happens in Save inside the one recorded step (the snapshot already carries formats). | S |
| LIBRARY-27 | medium | Broken (undo) | Saving an edit to a sensitive item records no step (`editor_ctl/save.rs:332-348`), so the next Ctrl+Z undoes the previous, unrelated act while the person believes it will undo the edit. Not driven: the test rig has no library key (saving as sensitive returned `KeyUnavailable`). | code | When a step cannot be recorded, record a marker step ("Edit cannot be undone") that Undo stops at and explains, or keep the old ciphertext blob reference instead of plaintext. | M |
| LIBRARY-28 | medium | Broken (stale state) | Lost update: the editor saves the fields it loaded when it opened; a colour, tag, title or move done in Main or Panel while the editor is open is overwritten on Save (`editor_ctl/save.rs:198-215`; `stamp` is used only for drafts). Measured: a Pin made while the editor is open survives the Save (pin is not an editor field); the overwrite applies to the fields the editor owns: title, tags, colour, list, sensitive (read in code, not driven). | run (pin), code (other fields) | Save applies only the fields the person changed (diff against the opened snapshot) on top of the current item, or reloads untouched fields when the item's `updated_at` moved. | M |
| LIBRARY-29 | medium | Missing (SPEC drift) | SPEC 5.4 lists, without "not built yet": find/replace, whole-text transforms (UPPER/lower/Title, trim, dedupe lines, sort lines), monospace toggle, line numbers, a rope viewer with find for 16 KB to 5 MB text, and an image editor (crop, rotate, resize, redact). None exists: grep for `find_replace`, `dedupe`, `sort_lines`, `crop`, `redact`, `rope` finds no editor code; text over 16 KB opens as a read-only prefix (`editor_ctl/open.rs:286-288`). | code | Either build them or mark each "Not built yet" in SPEC 5.4 and HANDOVER, as 5.5 already does for capture. | L (build) / S (mark) |
| LIBRARY-30 | low | Missing | Editor image source must be a PNG (`item_editor.rs:439` `SourceNotPng`); JPEG, WebP, GIF, BMP are refused although screenshots and photos are usually JPEG. An existing Image or File item's content cannot be replaced, and a File item takes one file. | code | Decode common formats to the stored PNG on import (the `image` crate is already a dependency if used by capture; check), allow "Replace…". | M |
| LIBRARY-31 | low | UX | A Link item refuses "www.example.com" and "example.com/path" (`item_editor.rs:331`): a scheme is required. Measured: "www.example.com" and "example.com/path" give `LinkNeedsScheme`. | run | Prepend `https://` when the text has no scheme and looks like a host. | S |
| LIBRARY-32 | medium | Missing | No duplicate detection when adding: saving the same title and text twice makes two items with no notice; only import skips duplicates. Paste-as-new and the capture hotkey behave the same. Measured: the same title and text saved twice gives 2 items and the plain "Saved" toast. | run | Hash the plain text on insert (the blob store is already content-addressed) and offer "Already saved in List X: open it / save anyway". | M |
| LIBRARY-33 | low | UX | Duplicate makes a second item with exactly the same title (no " copy" suffix), so the two rows cannot be told apart,. Measured: after Duplicate there are two items titled exactly "Race". | run | Title "X copy" (translated), cursor on the new row. | S |
| LIBRARY-34 | low | UX | A new item from the editor always goes to the top of its list (`editor_ctl/save.rs:182` `Placement::First`), ignoring the cursor row and the group the person is in. | code | Insert after the cursor row and inherit its group when the list is in Manual order. | S |
| LIBRARY-35 | high | Broken (undo) | Deleting a tag is a hard delete with no undo and no history reset (`main_ctl/items.rs:379-386`, `store/src/tags.rs`). Every earlier step whose snapshot carries that tag then fails on Undo or Redo (`items.rs` `require_tags` returns NotFound) and is dropped: an unrelated Pin or Delete can no longer be undone. Measured: tag an item, pin it, delete the tag, Ctrl+Z: toast "Failed", the item is still pinned, the Pin step is gone from the stack (next undo is the older Edit, redo is empty). | run | Tag delete becomes a recorded step (snapshot of the tag and its item links), and applying a snapshot tolerates a missing tag (drop it) instead of failing the step. | M |
| LIBRARY-36 | medium | Broken (undo) | A failed Undo or Redo drops the step for good (`history_ctl.rs` undo/redo: the step is taken before it is applied and not put back on failure). One transient store error loses that step. | run (the drop: in LIBRARY-35 the failed Pin step was removed), code (the cause) | Peek, apply in a transaction, pop only on success; on failure keep the step and say what blocked it. | S |
| LIBRARY-37 | medium | Broken (undo, consistency) | Panel bulk "Colour" writes with no history step and no toast (`panel_select.rs:369-386`), while the same change made in the editor is recorded. Because steps hold whole-item snapshots, undoing an older step of the same item also silently reverts the unrecorded colour. | code | Every write to an item goes through one recorded path (`record_step`); a clippy-style test that greps `update_item` call sites outside it. | M |
| LIBRARY-38 | medium | UX (undo) | Not undoable: tag rename and delete, list and board rename/icon/sort/move, restore from Trash, group fold. Delete forever and Empty Trash clear the whole history (`main_ctl/items.rs:224`, `:428`), and so does the idle lock (`settings_ctl.rs:806`). The Undo menu gives no reason when it empties. | code | Record list/board/tag edits as steps; on a purge remove only the steps that touch purged items; keep history across a lock when the key is unchanged (snapshots hold no sensitive plaintext, see `save.rs:332`). | M |
| LIBRARY-39 | low | Privacy | Undo of "Add" moves the new item to the Trash, where it stays for 30 days; a secret pasted by mistake and undone at once is still on disk and listed in Trash. Measured: Trash count goes 0 to 1 after undoing an Add. | run | Undo of an Add that was never used purges the item (and says so), or the toast offers "Delete forever". | S |
| LIBRARY-40 | medium | Broken (undo) | The toast's Undo takes a step by id from the middle of the stack (`history.rs:315` `take_undo(Some(id))`). Steps hold whole-item snapshots and move anchors, so undoing step N after step N+1 touched the same item reverts N+1's change too, or places the item beside an anchor that has since moved. | code | A toast Undo for a step that is not on top either undoes down to it (with a count in the toast) or is disabled once a later step touches the same items. | M |
| LIBRARY-41 | low | UX | History holds 100 steps in memory and is gone on restart, lock or purge; nothing in the UI says how far Undo reaches. | code | Say it in Help; consider persisting the last steps (non-sensitive snapshots) across a restart. | M |
| LIBRARY-42 | low | UX (feedback) | Ctrl+G does nothing, silently, when the Main list is under a colour filter (`group_ctl.rs:68-70`) or when the default group name is not a valid `Name` (`:71-73`). | code | Every refused act gets a toast with the reason and the way out ("Clear the colour filter to group"). | S |
| LIBRARY-43 | low | Missing | Groups are one level, per list, Manual order only; a group cannot be moved to another list as a whole, cannot be nested and has no colour. Deleting a folded group trashes rows the person cannot see (one undo step, toast shown). | code | Confirm with the count when the group is folded ("Delete group and its 12 items?"); add "Move group to list". | S |
| LIBRARY-44 | medium | Broken, UX | A trigger fires on its last character with no delimiter (`core/src/trigger.rs`, test `fires_at_the_last_character`), needing only a boundary before it. An all-letter trigger such as "sig" fires when the person starts typing "signature"; a one-letter trigger "a" fires on every article. `Trigger::new` accepts both. Two triggers where one is a prefix of the other (";s", ";sig") are allowed with only a warning returned after the save (`store/src/triggers.rs`), and the longer one can then never fire. Measured: with ";s" and ";sig" both saved (`set_trigger` returned Ok with ";s" as a warning), typing ";sig" fires ";s" and ";sig" never fires; with triggers "sig" and "a", typing "my signature and a cat" fires 3 times ("sig", "a", "a"); `Trigger::new` accepts "a", "1", ".", "the". | run | Offer the TextExpander choice per item or globally: expand immediately or at a delimiter (space, punctuation, Enter); refuse or loudly warn about a letters-only trigger shorter than 3 characters and about a prefix clash before the save, naming the other item. | M |
| LIBRARY-45 | low | UX | Triggers are case-sensitive with no option (";Sig" and ";sig" are two codes; test `case_counts`); TextExpander and Alfred offer "ignore case" and "adapt case". Measured: ";s" and ";S" are both accepted with no warning. | run | A per-trigger case mode (exact, ignore, adapt to typed case). | M |
| LIBRARY-46 | low | Broken | "Copy list as text" joins items with `---` lines (`list_text.rs`); an item whose own text holds a `---` line cannot be told from a separator, and line endings are `\n` on Windows. | code | Use a separator that is checked against the content (lengthen it until unique) and CRLF on Windows clipboard text. | S |
| LIBRARY-47 | low | UX | An untitled sensitive item has an empty label wherever the title is the only identifier (Trash row, undo menu, duplicate toast), because its preview is withheld (`item_preview.rs:87-93`, same rule in the lists). | code | A fixed stand-in title ("Sensitive item") when both title and preview are empty. | S |
| LIBRARY-48 | low | Missing | Tags have a name only: no colour, no nesting, no merge of two tags, no rename-into-existing (rename to an existing name is refused, `main_ctl/items.rs:357`). Boards have a name only (`store/src/boards.rs:22` columns) while lists have icon and accent. | code | "Merge into…" on rename clash; optional tag colour reusing the 8 item colours. | M |
| LIBRARY-49 | low | UX | Title limits differ by path: `Name` (lists, boards, tags, groups) allows 100 characters and refuses control characters (`core/src/name.rs`), an item title is trimmed but the search index reads only its first 256 characters (`core/src/search.rs:41`); a longer title is stored and shown but its tail is not searchable in Panel/Picker. | code | One documented title limit enforced in the editor with a counter. | S |
| LIBRARY-50 | low | Performance | Search and store timings at 10,000 items: (dev test profile, not the release build; 10,000 items, 2 tags each) `SearchIndex::query`: "inv" 18.4 ms, "snippet 9999" 52.8 ms, "dear customer thank" 116 ms, no-hit "zzzz" 4.9 ms (SPEC budget 16 ms, SPEC measured 4 to 8 ms). FTS `Store::search`: "dear" 20.0 ms for 200 hits, 2 words 1.9 ms. `all_items` 38 ms and `items(list)` 36 ms per call, and Main re-reads all items on every refresh (`main_window.rs:425-436`). Insert of 10,000 items in one transaction 3.06 s; 40 inserts after one anchor 201 ms (5 ms each). | run | Keep these as bench assertions with the SPEC budgets (16 ms search) so a regression fails CI. | S |
| LIBRARY-51 | medium | Broken | Panel/Picker matching uses nucleo "smart case" (`core/src/search.rs`, `CaseMatching::Smart`): one capital letter in the query makes it case-sensitive. Measured: "cafe resume" finds "Café résumé" but "RESUME" gives 0 hits; FTS is case-insensitive ("KIND" hits). A query typed with a leading capital or Caps Lock misses lower-case titles. | run | `CaseMatching::Ignore` for a snippet launcher (case should never hide a snippet); keep case only as a ranking bonus. | S |
| LIBRARY-52 | low | Broken | Full-width input is not folded in Panel/Picker: "ｃａｆｅ" gives 0 hits for "Café résumé" (Japanese IMEs produce full-width Latin by default). | run | NFKC-normalise both the indexed text and the query (and the FTS input). | S |
| LIBRARY-53 | medium | Performance | No cap on query length in `SearchIndex::query`: a 6,000-character query of 2,000 words over 10,000 items took 3.66 s (dev profile) on the calling thread; FTS caps at 32 words (`fts.rs:11`) and took 1.8 ms. Pasting a paragraph into the Panel or Picker search field freezes it. | run | Cap the query at the same 32 words and about 256 characters on every surface, before matching. | S |
| LIBRARY-54 | low | Performance | `SearchIndex::remove` is O(n): it shifts the vector and rewrites every later position in the map (`core/src/search.rs:200-211`). Measured: 200 removes from 10,000 took 420 ms (dev profile, 2.1 ms each): a bulk delete or list delete pays this per item. | run | `swap_remove` with one position fix-up, or a batch `remove_many` that rebuilds once. | S |
| LIBRARY-55 | medium | Missing | The Trash cannot be searched: `set_query` skips the search when the section is Trash (`main_window.rs:945`) and FTS excludes trashed items (store test `trashed_items_are_not_found_until_restored`). With 30 days of deletions, finding one item to restore means scrolling. | code | Search the Trash by title and preview in memory (the Trash list is already loaded), with the same marks. | S |
| LIBRARY-56 | low | Missing, Security note | A Link item has no "Open in browser" act (grep: `open_url` is used only by Help and Settings); Raycast quicklinks and Alfred open the address. If it is added: `check_link` accepts any scheme followed by `//` (measured: `file:///C:/x` and `javascript://x` are saved as Links, `item_editor.rs:331-357`), so the open act must allow only http, https, mailto and tel, since imported libraries carry links written by other people. | run (accepted schemes), code (no open act) | An allowlist at the open site, not at save; a test with file, javascript, ms-settings and UNC addresses. | S |

## 3. Not verified, and what a Windows run must check

- Nothing here was driven through the Slint harness or rendered: the editor window, the fill form
  (`template_field.slint`) and the search field were audited as logic only. Sizes, text scales, themes,
  focus order, focus after each act and contrast in my scope are not measured; the `main`, `panel` and
  `editors` UI agents own those surfaces.
- LIBRARY-27 (sensitive edit not recorded) is read in code only: the rig has no library key, so the save was refused with `KeyUnavailable`.
- The shared target dir lock was held by other runs for about 50 minutes of the time box; one probe run was stopped by the harness while it waited and was started again. Only one cargo target was run.
- Existing test targets in my rows were not re-run (one cargo target at a time, lock contended).
- Windows run must check: (a) trigger expansion in real apps: that "sig" style letter triggers fire inside
  longer words as LIBRARY-44 predicts, with IME on and off; (b) `{date}` and `{time}` against the Windows
  regional format for en-GB, en-CA, de-CH (LIBRARY-24); (c) "Copy list as text" line endings in Notepad
  (LIBRARY-46); (d) Main search with Japanese and Chinese text typed through an IME (LIBRARY-2);
  (e) the fill form's date field with a non-ISO regional format (LIBRARY-18); (f) "Remove formatting" then
  Cancel on an item copied from Word (LIBRARY-26).

## 4. Probe output (measured)

Command: `flock /tmp/target-panel/.audit.lock env CARGO_TARGET_DIR=/tmp/target-panel CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo test -p snipdeck-app --test probe_library --locked -- --nocapture --test-threads=1`
Result: 9 passed, 0 failed, 8.08 s. Timings are from the dev test profile (unoptimised), so they bound the release build from above. Clock fixed at 2026-10-17T10:30:00Z. The run printed no warning or error log lines.

```
P1 rank 'sig': [("Shipping instructions guide", 2, 64), ("Email footer", 1, 88), ("Other", 0, 88)]
P1 'invoice work' (title+tag): 0
P1 'invoice customer' (title+content): 0
P1 'invoice' : 1
P1 content past 64 chars found: 0
P1 q="cafe resume" hits=1
P1 q="テキスト" hits=1
P1 q="strasse" hits=1
P1 q="straße" hits=1
P1 q="RESUME" hits=0
P1 q="ｃａｆｅ" hits=0
P1 q="cafe\u{301}" hits=1
P1 special q="\\" hits=0
P1 special q="a\\ b" hits=0
P1 special q="^caf" hits=0
P1 special q="caf$" hits=0
P1 special q="!caf" hits=0
P1 special q="'caf" hits=0
P1 perf 10k q.len=3 hits=50 18.365176ms
P1 perf 10k q.len=12 hits=1 52.790831ms
P1 perf 10k q.len=4 hits=0 4.891283ms
P1 perf 10k q.len=19 hits=50 116.251547ms
P1 perf 10k q.len=1000 hits=0 2.074651ms
P1 perf 10k q.len=6000 hits=50 3.664177424s
P1 remove 200 of 10k: 420.1401ms (200)
P2 q="invoices" -> []
P2 q="l" -> []
P2 q="sign" -> ["Signature"]
P2 q="ature" -> []
P2 q="esign" -> []
P2 q="example.com" -> ["Signature"]
P2 q="user@example.com" -> ["Signature"]
P2 q="テキスト" -> []
P2 q="日本語" -> ["日本語"]
P2 q="これは" -> ["日本語"]
P2 q="cafe" -> ["Café"]
P2 q="resume" -> ["Café"]
P2 q="naive" -> ["Café"]
P2 q="C++" -> ["Café", "C++ tips", "Signature"]
P2 q="c" -> ["Café", "C++ tips", "Signature"]
P2 q="#include" -> ["C++ tips"]
P2 q="std::vector" -> ["C++ tips"]
P2 q="variable" -> ["snake"]
P2 q="my_variable" -> ["snake"]
P2 q="hyphen" -> ["snake"]
P2 q="some-hyphen" -> ["snake"]
P2 q="\"kind regards\"" -> ["Signature"]
P2 q="kind -regards" -> ["Signature"]
P2 q="regards kind" -> ["Signature"]
P2 q="KIND" -> ["Signature"]
P2 q="users\\me" -> ["C++ tips"]
P2 insert 10k in one tx: 3.060098819s
P2 perf 10k q.len=4 hits=200 19.976558ms
P2 perf 10k q.len=12 hits=1 1.94998ms
P2 perf 10k q.len=4 hits=0 199.567µs
P2 perf 10k q.len=1 hits=200 19.424495ms
P2 perf 10k q.len=5000 hits=0 944.756µs
P2 perf 10k q.len=6000 hits=0 1.762922ms
P2 all_items 10006: 38.265345ms
P2 items(list) 10006: 36.201901ms
P2 insert First into 10k list: 780.658µs
P2 move_item mid 10k: 239.85µs
P2 40 inserts after one anchor (renumber of 10k expected once): 201.071279ms
P3 OK  "{field:CC|}" -> "" cursor=None fields=[("CC", Text, true, "")] first_missing=Some((0, Empty))
P3 OK  "{field:CC| }" -> "" cursor=None fields=[("CC", Text, true, "")] first_missing=Some((0, Empty))
P3 ERR "{time:%H:%M}" -> `{time}` takes no argument (bytes 0..12)
P3 ERR "{date:%-d %B}" -> unknown date format `%-`; supported: %Y %y %m %B %b %d %e %j %A %a %H %I %M %S %p %z %F %T %% (bytes 6..8)
P3 OK  "{date:%e %B %Y}" -> "17 October 2026" cursor=None fields=[] first_missing=None
P3 OK  "{date:%A %d %B}" -> "Saturday 17 October" cursor=None fields=[] first_missing=None
P3 ERR "{datetime}" -> `datetime` needs an argument: `{datetime:FORMAT}` (bytes 0..10)
P3 ERR "{date:%V}" -> unknown date format `%V`; supported: %Y %y %m %B %b %d %e %j %A %a %H %I %M %S %p %z %F %T %% (bytes 6..8)
P3 ERR "{date:%Z}" -> unknown date format `%Z`; supported: %Y %y %m %B %b %d %e %j %A %a %H %I %M %S %p %z %F %T %% (bytes 6..8)
P3 OK  "{field:Name}{field:name}" -> "" cursor=None fields=[("Name", Text, true, ""), ("name", Text, true, "")] first_missing=Some((0, Empty))
P3 ERR "{choice:Tone|a|b}{field:Tone}" -> `Tone` is used for two different prompts; use the same kind and options each time, or pick another name (bytes 17..29)
P3 ERR "fn main() { }" -> empty placeholder `{}` (bytes 10..13)
P3 OK  "{optional:PS}PS {field:Text}{/optional}" -> "" cursor=None fields=[("PS", Toggle, false, ""), ("Text", Text, true, "")] first_missing=Some((1, Empty))
P3 ERR "{clipboard|upper}" -> unknown placeholder `{clipboard|upper}`. Placeholders are {uuid}, {date}, {time}, {datetime:FORMAT}, {clipboard}, {cursor}, {field:Name}, {field:Name|default}, {choice:Name|a|b}, {multiline:Name|default}, {pickdate:Name|FORMAT} and {optional:Name}...{/optional}. To writ
P3 ERR "{snippet:sig}" -> unknown placeholder `{snippet}`. Placeholders are {uuid}, {date}, {time}, {datetime:FORMAT}, {clipboard}, {cursor}, {field:Name}, {field:Name|default}, {choice:Name|a|b}, {multiline:Name|default}, {pickdate:Name|FORMAT} and {optional:Name}...{/optional}. To write a literal 
P3 ERR "{selection}" -> unknown placeholder `{selection}`. Placeholders are {uuid}, {date}, {time}, {datetime:FORMAT}, {clipboard}, {cursor}, {field:Name}, {field:Name|default}, {choice:Name|a|b}, {multiline:Name|default}, {pickdate:Name|FORMAT} and {optional:Name}...{/optional}. To write a literal 
P3 OK  "{date+1d:%F}" -> "2026-10-18" cursor=None fields=[] first_missing=None
P3 OK  "{date +1d}" -> "18.10.2026" cursor=None fields=[] first_missing=None
P3 ERR "{DATE}" -> unknown placeholder `{DATE}`, did you mean `{date}`?. Placeholders are {uuid}, {date}, {time}, {datetime:FORMAT}, {clipboard}, {cursor}, {field:Name}, {field:Name|default}, {choice:Name|a|b}, {multiline:Name|default}, {pickdate:Name|FORMAT} and {optional:Name}...{/optional}. To wr
P3 ERR "{Date}" -> unknown placeholder `{Date}`, did you mean `{date}`?. Placeholders are {uuid}, {date}, {time}, {datetime:FORMAT}, {clipboard}, {cursor}, {field:Name}, {field:Name|default}, {choice:Name|a|b}, {multiline:Name|default}, {pickdate:Name|FORMAT} and {optional:Name}...{/optional}. To wr
P3 ERR "{field:A|x|y}" -> a prompt takes a name and one more part: `{field:Name|default}` (bytes 0..13)
P3 OK  "{field:Email|a@b.c}" -> "a@b.c" cursor=None fields=[("Email", Text, false, "a@b.c")] first_missing=None
P3 OK  "{field:Url|https://x.y}" -> "https://x.y" cursor=None fields=[("Url", Text, false, "https://x.y")] first_missing=None
P3 ERR "{choice:Size|S|M|L|}" -> a choice option is empty (bytes 0..20)
P3 OK  "{pickdate:When|%d/%m/%Y}" -> "17/10/2026" cursor=None fields=[("When", Date, false, "")] first_missing=None
P3 ERR "{date:%d}}" -> `}` closes nothing; write `}}` for a literal `}` (bytes 9..10)
P3 OK  "{{date}}" -> "{date}" cursor=None fields=[] first_missing=None
P3 OK  "{field:A:B}" -> "" cursor=None fields=[("A:B", Text, true, "")] first_missing=Some((0, Empty))
P3 OK  "{multiline:Body|line}" -> "line" cursor=None fields=[("Body", Multiline, false, "line")] first_missing=None
P3 ERR "{cursor}{cursor}" -> only one `{cursor}` is allowed (bytes 8..16)
P3 OK  "{date-1d} {time+90min}" -> "16.10.2026 12:00" cursor=None fields=[] first_missing=None
P3 locale "en": 10/17/2026 10:30 AM
P3 locale "en-GB": 17/10/2026 10:30
P3 locale "en-CA": 10/17/2026 10:30 AM
P3 locale "en-ZA": 10/17/2026 10:30 AM
P3 locale "de": 17.10.2026 10:30
P3 locale "de-CH": 17.10.2026 10:30
P3 locale "fr-CA": 17/10/2026 10:30
P3 locale "nl": 2026-10-17 10:30
P3 locale "it": 2026-10-17 10:30
P3 locale "ja": 2026/10/17 10:30
P3 locale "zh-Hans": 2026/10/17 10:30
P3 locale "es-MX": 17/10/2026 10:30
P3 locale "pt-BR": 17/10/2026 10:30
P3 locale "": 2026-10-17 10:30
P3 injection: Hi {date} {clipboard}!
P3 64x1MB clipboard: ok=false 12.409397ms
P3 parse 2000 same prompts: ok=true 3.684413ms
P4f Link "www.example.com": open=true problem=Some(LinkNeedsScheme)
P4f Link "example.com/path": open=true problem=Some(LinkNeedsScheme)
P4f Link "https://example.com/a b": open=true problem=Some(LinkHasSpace)
P4f Link "file:///C:/x": open=false problem=None
P4f Link "javascript://x": open=false problem=None
P4f Template "function() { return 1; }": open=true problem=Some(Template(Parse(Unknown { found: "return 1;", suggestion: None })))
P4f Text "   ": open=true problem=Some(ContentEmpty)
P4f Code "\tindented\n": open=false problem=None
P4g stored text "  keep lead\nand trail  \n\n" preview "keep lead\nand trail  \n\n"
P4g untitled rows: [("first real line", "first real line\nsecond")]
P4h main search 'first real': rows=1
P4h main search 'irst': rows=0
P4h main search list name 'email replies': rows=0
P4c pinned while editor open: true
P4c after editor save: title=Race2 pinned=true
P4c duplicates named exactly 'Race': 1
P4d undo of add: trash 0 -> 1
P4e identical item saved twice: count=2 toast=Some(Saved { list: "Email replies" })
P4a tags on item: 1
P4a pinned: true
P4a after tag delete: undo=Some(Pin) tags=0
P4a after undo(pin): pinned=true (want false) undo=Some(Edit) redo=None toast=Some(Failed)
P4b undo before edit = Some(Delete)
P4b save sensitive: open=true undo=Some(Delete) toast=Some(KeyUnavailable)
P5 "Email signature"/"mail sig": title=Some(("E", "mail sig")) preview=None
P5 "Email signature"/"emsig": title=None preview=None
P5 "Café menu"/"cafe": title=None preview=None
P5 "Email signature"/"sig": title=Some(("Email ", "sig")) preview=None
P5 "Email signature"/"second": title=None preview=None
P5 "STRASSE"/"straße": title=None preview=None
P5 "İstanbul"/"istanbul": title=None preview=None
P6 set ;s: Ok([])
P6 set ;sig (prefix clash): Ok([Trigger(";s")])
P6 set ;S (case twin): Ok([])
P6 typing ';sig' fires: [";s"]
P6 typing 'my signature and a cat' with triggers sig,a fires: ["sig", "a", "a"]
P6 Trigger::new("a") = Ok("a")
P6 Trigger::new("1") = Ok("1")
P6 Trigger::new("the") = Ok("the")
P6 Trigger::new(".") = Ok(".")
P6 Trigger::new(" x") = Err(TriggerHasSpace)
P6 Trigger::new("x y") = Err(TriggerHasSpace)
P6 Trigger::new("é") = Ok("é")
P6 Trigger::new("😀") = Ok("😀")
```
