# v1.0.1 and Recent rich captures: build spec (2026-11-08)

Covers `.claude/work-order.md` lines 25-28. Read-only survey of afc9d29 (shared look slices 1-2 merged); nothing
built or run; line numbers from that tip. Tests use a fixed clock (`"2026-10-18T09:00:00Z"`, `recent.rs:288`).

## 0. Already done, dropped, renamed

| Code | State | Evidence |
|---|---|---|
| U9 undo until next change | **Done** (by O47 d) | `history.rs:19` (100 steps), `controller/history_ctl.rs`, Ctrl+Z/Y in `panel.slint:1031-1035` and `main_window.slint:797-804`, `tests/history.rs`. Only the tick in `docs/reviews/fable-analysis.md:44` is left (slice 8). |
| O47 d undo/redo | **Done** | commit 743ccbc is in this tip (`history.rs`). |
| U22 part 1 drag-to-edge snap, O32 | **Done** | `frame.rs:413` `nearest_dock_edge`, `frame.rs:444` `DOCK_SNAP`, `dock_ctl.rs:356` `frame_settled` docks on release. |
| O45 multi-select + bulk bar | **Done** | `controller/panel_select.rs:45-128`. O50 builds on it. |
| Toast Edit (controller) | **Done**, UI not wired | `toast_ctl.rs:98` `toast_edit`, `views.rs:100` `edit: bool`; no Slint button, no `on_toast_edit` in any binding. |
| Main quick-add (controller) | **Done**, UI not wired | `use_ctl.rs:217` `quick_add(.., Origin::Main)`; `components/quick-add.slint:12` used only by `panel.slint:1790`. |
| O26 spacing sweep, O31 button grouping | **Dropped**: duplicate | shared-look spec §1 decisions 1, 9 and §3 toolbar groups (slice 6) cover row spacing and toolbar grouping. |
| O49 / O50 names | **Renamed** | `fable.md:96` calls O49 "rich paste" (v1.1); hub 10:59 (`fable.md:102`) renamed: O49 = header select/delete, O50 = corner. The work order means the "fast removal" text at `fable.md:97`; this spec calls it O50. O51 corner position is not in scope. |

Branch `claude/o47-abc-wip`: only 392473a is new (the rest is old history); two drafts, never built:
`paste_create.rs` (`classify`, `kind_of_text`, `looks_secret` + tests) and `controller/create_ctl.rs`
(`paste_create`, `drop_create`, `drop_hover`, `drop_end`, `ask_about_secret`, a duplicate `toast_edit`: delete it).
Take them with `git show 392473a:<path>`.

## 1. Slices (one at a time, Sonnet high, ~1.5 h each, tests first, `scripts/gate.sh --staged` per commit)

| # | Slice | Why here |
|---|---|---|
| 1 | Paste to create (text, link), Ctrl+C on items, toast Edit button | Owner's top ask (J3 becomes 2 actions); pure code mostly drafted |
| 2 | Clipboard image/file readers, paste-create image/files, file drop from Explorer | Readers are reused by slice 5; Windows risk surfaced early |
| 3 | Compact dock (U22 parts 2-3) | Owner pain ("takes up soo much of the screen"); pure frame maths |
| 4 | Dock grips (O28 addendum) and O50 Select/Delete header | Grips need slice 3; O50 is small UI on existing selection |
| 5 | Recent rich captures (store v7, policy, watcher) | Store migration: medium risk, after readers exist |
| 6 | Main quick-add, multi-row drag | Controller exists; wiring and one pure fn |
| 7 | O54 sidebar resize/collapse, O30 sort menu | Settings fields + Main header; low risk |
| 8 | A25 cold start, search suggestion dropdown, doc ticks | Lowest value; A25 measure first |

Slice 1 runs after shared-look slice 5 (ink toasts; both touch the `feedback.slint` Toast). Each slice adds
wizard steps to `tools/windows-test/tests/m3-app.json` for what it cannot verify, then `i18n.py extract`, `codemap.py`.

## 2. Items
### O47 a, c + toast Edit (slice 1)
- **User gets:** Ctrl+V on the Panel or Main list (no text field focused) adds the clipboard as an item;
  Ctrl+C copies the selected item(s); the "Added to Work" toast has Undo and Edit.
- **Decision:** Ctrl+V only when the search and quick-add fields are not focused (a focused field pastes normally).
  A single URL is a Link, else Text (`paste_create::kind_of_text`). Text that `looks_secret` is never saved
  silently: it opens the editor with Sensitive on and a toast says why (`ask_about_secret`). Ctrl+C with one item
  = the item's Copy (all its formats, Sensitive clear rules); with several = `bulk_copy` (`panel_select.rs:95`,
  joined by `\n`, Sensitive skipped). Reason: one copy rule per count, already tested.
- **Touches:** new `paste_create.rs`, `controller/create_ctl.rs` (from 392473a; add `mod`s), `toast.rs:44`
  (`ToastMessage::AddedTo { list, count }`, `LooksSecret`), `panel.slint:981-1060` (Ctrl+V, Ctrl+C in the
  `control` block), `main_window.slint:780-805` (same), `binding.rs:777` and `main_binding.rs:640` (wire
  `paste-create`, `copy-selection`, `toast-edit`), `components/feedback.slint` Toast (second `toast-action`
  text button "Edit", shown when `edit`), `toast_window.slint`. Main copy: generalise `bulk_copy` to
  `copy_items(&[ItemId])` and call it from a new `main_copy_selection` in `controller/main_ctl/items.rs`.
- **Tests:** `paste_create.rs` unit tests (from the draft: `the_kind_detection_table`,
  `a_single_address_is_a_link_and_prose_is_not`, `secrets_are_flagged_and_ordinary_text_is_not`);
  `controller/tests/panel_select.rs`: `paste_create_adds_to_the_shown_list_with_undo_and_edit` (one history
  step; undo trashes it), `a_secret_paste_opens_the_editor_sensitive`, `ctrl_c_one_item_uses_item_copy`,
  `ctrl_c_many_joins_like_copy_all`; `tests/panel_window.rs` + `tests/main_window.rs`:
  `ctrl_v_in_search_field_does_not_create`; `toast_edit_button_shows_only_with_edit` (ui logic).
### O47 a (images, files) + b (slice 2)
- **User gets:** pasting a screenshot or files copied in Explorer adds Image/File items; dropping files from
  Explorer on the Panel or Main adds them to the list under the pointer.
- **Decision:** external drops are **files only** in v1.0.1, through winit `WindowEvent::HoveredFile` /
  `DroppedFile` in the existing `on_winit_window_event` hooks (`main.rs:681`, `main_binding.rs:800`). Text,
  image and browser-link drops from other apps need an OLE `IDropTarget`, which winit already registers for the
  window: v1.1. Reason: Slint 1.18.1 delivers no OS drag events (`.claude/remaining/o47.md`), and replacing
  winit's target is not buildable or checkable here. Several files = several items, one history step.
- **Touches:** `snipdeck-platform/src/win_formats.rs` new pure `dib_to_png(&[u8]) -> Option<Vec<u8>>` (inverse of
  `png_to_dib:223`, honours `DIB_DECODE_LIMIT:195`, BI_RGB and BI_BITFIELDS, bottom-up and top-down) and
  `parse_drop_files(&[u8]) -> Vec<String>` (inverse of `drop_files:59`); `windows/clipboard.rs` new
  `read_png()` (registered `PNG` first, else `CF_DIBV5`, else `CF_DIB`) and `read_files()` (`CF_HDROP`)
  next to `read_text:251`; `host.rs:61` trait `ClipboardText` gains both (the draft's `read_clip` builds a
  `Clip`); `create_ctl.rs` `drop_create`/`drop_hover`/`drop_end`; `controller.rs` `drop_hint: Option<ListId>`
  for "Drop to add to <list>". Files go through `item_editor::read_source:395` (64 MiB cap, `max_attachment_bytes:30`).
- **Tests:** `win_formats` round trips `png_to_dib -> dib_to_png` (pixels equal) for 1x1, odd width, alpha;
  `dib_to_png_rejects_truncated_and_oversized`; `parse_drop_files_round_trips_drop_files` incl. non-ASCII;
  controller: `pasting_two_files_adds_two_file_items_one_step`, `a_file_over_the_limit_is_skipped_and_said`,
  `drop_on_a_tab_targets_that_list`. Windows-only: wizard steps for Snipping Tool paste, Explorer copy/paste and drag.
### U22 compact dock (slice 3)
- **User gets:** a Panel docked to the left or right edge keeps its own height at the height you dropped it, like
  the PC Manager toolbar; "Docked panel uses the full edge" (off) gives back today's full-height dock.
- **Decision:** the along-edge offset is stored per monitor and edge; the Panel is clamped inside the work area;
  top/bottom docks keep their centred size unless dropped elsewhere along the edge (same rule, x instead of y).
  The collapsed sliver (`handle_rect:220`) spans the Panel's own extent (already takes `along`). Snap-to-edge
  fade (U22 part 4) stays v1.1. Reason: owner 2026-10-20 (`inbox-archive.md:142`).
- **Touches:** `frame.rs:266` `docked_rect` gains `along: Option<i32>` and `full_edge: bool`;
  `settings.rs:34` `PanelGeometry` gains `#[serde(default)] along: Option<i32>` (old files read as None = centred
  or full); `settings.rs:145` `DockSettings` gains `full_edge: bool` (default false); callers
  `dock_ctl.rs:150`, `dock_ctl.rs:295`, `panel.rs:689`; `dock_ctl.rs:356` `frame_settled` passes the dropped
  frame's start; `remember_frame` (`panel.rs:601`) updates `along` while docked; Settings > Panel toggle.
- **Tests (pure, `frame.rs`):** `a_side_dock_keeps_its_height_at_the_drop_point`,
  `a_dock_dropped_past_the_bottom_is_pulled_inside`, `full_edge_gives_the_work_area_height`,
  `along_none_centres_like_before`; settings: `old_geometry_without_along_reads_as_none`; controller
  (`controller/tests/dock.rs`): `dropping_at_y_300_docks_at_y_300_and_remembers_it`, `undock_restores_pre_dock`.
### O28 dock grips (slice 4)
- **User gets:** a docked Panel resizes from its inner edge, its two ends and the inner corners; a grip pill in
  the header slides it along the edge without undocking.
- **Decision:** the outer (screen-side) edge and its corners do not resize; the pill drag is axis-locked and
  never undocks; a drag on the rest of the header still undocks (today's rule). Reason: PC Manager behaviour
  (`inbox-archive.md:144`), and one way off the edge.
- **Touches:** new pure `frame::dock_resize_allowed(edge: ResizeEdge, dock: DockEdge) -> bool`; `panel.rs:574`
  `resize` refuses disallowed edges; `panel.slint:2538-2600` hides those `ResizeGrip`s (new `in property
  <int> dock-edge`); pill = new `GripPill` in the header (`panel.slint` near the kebab, 1421), callback
  `along-drag(delta)` -> new `Controller::dock_slide(delta)` updating `along`.
- **Tests:** `dock_resize_allowed` table (4 docks x 8 edges); `sliding_clamps_to_the_work_area`;
  `panel_logic.rs` `outer_grips_hidden_while_docked`. Windows-only: OS resize while docked.
### O50 header Select and Delete (slice 4)
- **User gets:** labelled [Select] and [Delete] buttons in the Panel header for fast removal; "Clear list…" in ⋯.
- **Decision:** Select mode shows a checkbox on each row and uses the existing marks and bulk bar (no second
  selection model). Delete mode puts a delete button on each row; a deleted row collapses to "Deleted · Undo"
  while the mode is open; Done or Esc ends it; each delete is one history step. "Clear list…" asks once (item
  count), then an Undo toast; one history step. Reason: `fable.md:97`, reuses `trash_with_undo:242`.
- **Touches:** `panel.slint` header (near 1313-1440) and row template (1688); `controller/panel_select.rs`
  (`panel_select_mode`, `panel_delete_mode`, `panel_delete_row`, `panel_clear_list`); `panel.rs:137`
  `LIST_MENU` and `APP_MENU:141` gain `ClearList`; `views.rs` row state `deleted_pending`.
- **Tests:** `delete_mode_row_undo_restores_it`, `clear_list_is_one_step_and_undoes_whole`,
  `select_mode_checkbox_marks_like_ctrl_click`, `esc_leaves_delete_mode`; `panel_logic.rs` header buttons labelled.
### Main quick-add + multi-row drag (slice 6)
- **User gets:** the Panel's "Add a snippet… (Enter to save)" box at the top of a Main list; dragging one of
  several selected rows moves them all.
- **Decision:** in Main, Ctrl+N focuses quick-add when a list is open (else opens the editor as today);
  Ctrl+Shift+N stays New list (`main_window.slint:786`), so "More options" is a button only. Multi-row drag:
  dragging a marked row moves every marked item, in their current order, to the drop row or list; one
  `Action::Reorder`/`Move` step. An unmarked row drags alone. Reason: no chord remap; one gesture rule for both windows.
- **Touches:** `main_window.slint` list area (~1875) adds `QuickAdd`; `main_binding.rs:746` wire
  `quick-add-requested` -> `quick_add(.., Origin::Main)`; new pure `reorder_many(order, moved: &[ItemId], onto)`
  beside `reorder` used by `main_ctl/lists.rs:151` `main_drop_row`, `:193` `main_drop_on_list`,
  `panel_ctl.rs:204` `panel_drop_row`, `:172` `panel_drop_on_tab`.
- **Tests:** `reorder_many` table (block up, block down, non-adjacent marks, drop on a marked row = no-op);
  `dragging_three_marked_rows_is_one_undo_step`; `main_quick_add_saves_to_the_open_list`.
### O54 sidebar resize + O30 sort (slice 7)
- **User gets:** a draggable Main sidebar (200-320 px, remembered), a collapse button and Ctrl+B; "Sort: Manual"
  menu in the Main list header and "Sort by" in the Panel list menu.
- **Decision (O54):** stored width is at 100 % text scale; the existing scale term
  (`main_window.slint:343`) is added on top; the auto rail below 720 px stays and wins. Splitter: 8 px hit
  area, Left/Right move 8 px, accessible name "Sidebar width". Ctrl+B is free in both key handlers.
- **Decision (O30):** one source of truth = the list's sort field the List editor already edits
  (`editors.slint:1159`). A drag or Alt+Up/Down in a non-Manual list (today a silent no-op,
  `main_ctl/lists.rs:153`) shows a toast "Switch this list to Manual order?" with the action.
- **Touches:** `settings.rs:632` `Settings` gains `main_sidebar_width: u32` (232) and `main_sidebar_collapsed`
  (sanitize clamps); `main_window.slint` 343, 823; `main_binding.rs`; Panel `LIST_MENU` gains `SortBy`;
  `main_ctl/lists.rs` `set_list_sort`.
- **Tests:** `sidebar_width_clamps_200_320`, `old_settings_get_232`; `main_window_logic.rs`:
  `ctrl_b_toggles_sidebar`, `splitter_keys_step_8px`; `sorting_from_the_header_matches_the_editor`,
  `dragging_in_a_sorted_list_offers_manual`.
### A25 cold start + search suggestions + doc ticks (slice 8)
- **A25 user gets:** the Panel shows sooner after a cold start. **Decision:** measure first: log ms from
  `main` to the first `window.show()` and the index build in `PanelModel::load_cached` (`controller.rs:362`)
  via `trace.rs`; only if the build is over 50 ms on the owner's machine, defer it to a zero-delay Slint Timer
  after the first show, and a search typed before then builds it synchronously. Reason: unmeasured
  (`fable-analysis.md:65`); do not restructure for nothing. Test: `a_search_before_the_index_is_ready_still_finds`.
- **Search suggestions user gets:** a dropdown under the Panel, Picker and Main search that completes the word
  being typed. **Decision:** suggestions are tag names, list names and item-title words (prefix, case-folded,
  at most 6, Sensitive titles never), not items (the results already show items). Down/Tab enters the
  dropdown, Enter or click replaces the word, Esc closes it. Rows use the shared menu row (shared-look
  decision 9). **Touches:** new pure `search_suggest.rs` `suggest(query, caret, &Index) -> Vec<Suggestion>`,
  `panel.slint`, `picker.slint`, `main_window.slint` search fields, three bindings. **Tests:**
  `suggests_tags_lists_and_title_words_by_prefix`, `never_suggests_sensitive_titles`, `caps_at_six`,
  `replaces_only_the_word_at_the_caret`.
- **Doc ticks:** U9, A25 in `fable-analysis.md`; HANDOVER v1.0.1 list.

## 3. Recent rich captures (slice 5; owner 2026-10-26, `inbox-archive.md:161`)
**User gets:** Recent also keeps copied images, files and formatted text; one click re-pastes them with their
formats, or saves them to a list as Image/File/Text items.
**What the code honours today** (`clipboard_watch.rs:55-75`, `windows/clipboard.rs:886-943`): before anything
is read, with the clipboard open, it checks `ExcludeClipboardContentFromMonitorProcessing` (presence),
`Clipboard Viewer Ignore` (presence) and `CanIncludeInClipboardHistory` = 0; then reads `CF_UNICODETEXT` only.
Own writes (`own`), Sensitive items, excluded apps, pause and lock skip (`recent.rs:157-200`).
`CanUploadToCloudClipboard` is registered (`clipboard.rs:498`) but only set on writes: it is a cloud-sync
marker and Snipdeck syncs nothing, so it is **not** an exclusion. **Decision:** the same marker check gates
every new format, unchanged, still before any read; no new marker. Reason: one gate, already tested.
**Size caps (decision, with reason: bounded memory on the clipboard thread and bounded disk):**
- text 64 KiB (`MAX_ENTRY_BYTES`, unchanged); HTML fragment (via `cf_html.rs:52`) plus RTF together 1 MiB,
  over it the entry keeps the text only;
- image: at most 16 Mpx (`PREVIEW_MAX_PIXELS`) and 8 MiB as PNG, else `Skipped::TooLarge`;
- files: at most 100 paths; paths only, no bytes are copied at capture (copying file bytes on every Explorer
  Ctrl+C is churn and a surprise); Use and promote read them then, under the 64 MiB attachment cap, and say
  "File moved or deleted" if gone. Virtual files (`FileGroupDescriptorW`, Outlook attachments) are skipped;
- Recent blob budget 256 MiB total; prune drops oldest first past it, as well as the 200 / 7-day rules.
**Storage (store migration v7, `migrations.rs:33`, frozen `V7` in `snipdeck-store/src/recent.rs`):**
`recent_entries` gains `kind TEXT NOT NULL DEFAULT 'text' CHECK (kind IN ('text','image','files'))`,
`rich BLOB` (sealed envelope of `{html, rtf, paths}`) and `blob TEXT` (a `BlobRef`). Image bytes are sealed
with the data key (SNENC2, bound to the row hash, as `text` is) **before** `Blobs::put`, so `blobs/` holds only
ciphertext. With no key ring, images and files are **not captured** (text keeps today's plain fallback).
`text` holds a label for non-text rows ("Image 1280 × 720", "3 files: a.pdf, …") so the `length > 0` check
and in-memory search keep working. `delete_recent`, `clear_recent`, `prune_recent` release the blob;
`Store::gc_blobs` (`store.rs:263`) counts `recent_entries.blob` as live. `RecentEntry` (`recent.rs:78`) gains
`kind` and an accessor that decrypts `rich`/`blob` on demand (the list view never loads image bytes, only a
thumbnail made once at capture, ≤ 64 KiB inline in `rich`).
**Dedup (decision):** the row's keyed hash is over a kind tag plus: text entries, the plain text only (the same
words copied from Word then Notepad are one entry, the newest formats win); images, width, height and the
decoded RGBA pixels (stable across PNG encoders); files, the sorted, case-folded paths. Reason: matches what a
user sees as "the same copy".
**Touches:** `clipboard_watch.rs:12` `ClipboardChange` gains `image: Option<Vec<u8>>` (PNG), `html`, `rtf`,
`files: Vec<String>`; `windows/clipboard.rs:886` `observe_change` reads them with slice 2's readers after the
marker gate (text first, image only when no text or a lone `CF_DIB`); `recent.rs:84` `ClipboardObservation`
and `consider:157` (new `Skipped::NoKeyForRich`); `snipdeck-store/src/recent.rs` `add_recent` takes a
`RecentContent` enum; `controller/recent_ctl.rs:225` `recent_use` writes `Data::Text` + `Html` + `Rtf`, or
`Png`, or re-read `Files`; `:130` `recent_promote_to` creates Image/File/Text items; Panel/Main Recent rows use
the ItemCard image thumbnail and file glyph (`item-card.slint:376`).
**Tests (fixed clock):** store: `v7_migration_keeps_v6_text_rows` (migration test pattern
`migrations.rs:337`), `an_image_blob_on_disk_is_ciphertext`, `pruning_an_image_row_releases_its_blob`,
`gc_keeps_recent_blobs`, `no_key_ring_skips_images`; policy (`recent.rs` table at :319 extended):
`markers_skip_images_and_files_before_read`, `an_image_over_16_mpx_is_too_large`,
`rich_over_1_mib_keeps_text_only`, `same_pixels_dedupe`, `same_files_any_order_dedupe`,
`text_from_two_apps_is_one_entry`, `budget_prunes_oldest_image_first`; controller
(`controller/tests/recent.rs`): `using_a_rich_entry_writes_text_html_rtf`, `promoting_an_image_makes_an_image_item`,
`a_moved_file_says_so`; `tests/recent_at_rest.rs`: `rich_payloads_are_sealed_at_rest`.
**Unverified until run on Windows (wizard steps in `m3-app.json`):** which formats Word, Chrome, Edge,
Snipping Tool and Explorer actually put; `CF_DIBV5` alpha and premultiplication; delayed rendering cost when
reading `CF_DIB`/`PNG` from Excel or Paint (the read stalls the clipboard thread: confirm `READ_OPEN_BUDGET`
holds); whether KeePassXC, 1Password and Bitwarden mark images or only text; elevated-app clipboards;
`CF_HDROP` from Explorer, network paths and OneDrive placeholders; the 8 MiB / 16 Mpx caps on a 4K screenshot.
