# Trimmed v1.1: build spec (2026-11-10)

Scope: HANDOVER.md "## v1.1" minus the 2026-10-26 cuts (colour picker, multi-panel, O56, image editing; now.md).
Read-only survey of d1daa43; nothing built or run; line numbers from that tip. Tests use a fixed clock
(`"2026-10-18T09:00:00Z"`) and fixed random bytes, never the real clock or RNG.

## 0. Already there (reuse, do not rebuild)

| Piece | Evidence |
|---|---|
| Importer backend (4 tools, 22 tests) | `portable/src/importers/{beeftext,espanso,textexpander,phraseexpress}.rs`, `tests/expanders.rs` |
| Templates: date, time, datetime:FMT, clipboard, cursor, field, choice; 16 prompts; 16 MiB | `core/src/template.rs:19-58`, parser `:695`, `RenderContext` `:241` |
| Fill-form kinds `text, choice, date, multiline` in the UI (engine knows text and choice only) | `ui/components/template_field.slint:6`, `fill_binding.rs:277` |
| HTML/RTF stored, exported, pasted; `capture_rich_text` used by Recent only; watcher reads CF_HTML | `core/src/format.rs:25`, `usecase.rs:214-221`, `capture.rs:108`, `windows/clipboard.rs:955` |
| Never-paste list, routing; data key opened by key; no-activate windows; Help tabs | `route.rs:63,122`; `store/src/keys.rs:194`; `window_style.rs:53`; `help.slint:125` |

## 1. Slices (Sonnet high, ~2 h each, tests first, `scripts/gate.sh --staged` per commit)

| # | Slice | Risk | Lane | Needs |
|---|---|---|---|---|
| 1 | Importer Settings wiring | Low | UI | - |
| 2a | `{uuid}`, date maths, paste transforms | Low | UI | - |
| 2b | Richer fill-ins (multi-line, date, optional sections) | Medium (parser) | UI | 2a |
| 3a | Groups: store, core, portable (no UI) | Medium (migration v8) | Light | - |
| 3b | Groups: controller, Panel, Main | Medium (drag, a11y) | UI | 3a |
| 4 | Tips (Help tab) | Low | UI | - |
| 5a | Triggers: matcher, store, importers (no UI) | Medium (migration v9) | Light | 3a |
| 5b | Typed-trigger expansion: hook, Settings, editor | **High** (global keyboard hook) | UI | 5a |
| 6 | Suggest-as-you-type at the caret | **High** (swallows Tab) | UI | 5b |
| 7 | Rich paste | Medium (clipboard) | UI | - |
| 8 | Windows Hello unlock | **High** (crypto, WinRT) | UI | - |
| 9 | serde_yaml_ng -> serde-saphyr | Low | Light | - |

**Parallel groups.** now.md 2026-11-07 allows one snipdeck-ui/app build at a time and two cargo users, so two lanes:
- *UI lane* (one agent, serial): 1 -> 2a -> 2b -> 4 -> 7 -> 3b -> 5b -> 6 -> 8.
- *Light lane* (one agent beside it; builds core, store, portable only): 9 -> 3a -> 5a.
- The UI lane never edits `store/`, `portable/`, `migrations.rs`, `schema.rs`; the Light lane never edits `app/`, `ui/`,
  `platform/`. 3a and 5a share `migrations.rs`, `schema.rs`, `dto.rs`, so they stay serial. 1 and 9 can run together:
  `import_espanso`'s signature does not change.
- Decision: builders never commit `docs/CODEMAP.md` (coordinator regenerates per merge). Reason: it always conflicts.
- Decision: wizard ids (`m3-app.json`, last 46): 1: 47-48, 2a: 49, 2b: 50, 4: 51, 7: 52-53, 3b: 54, 5b: 55-60, 6: 61-62,
  8: 63-66. Reason: no renumbering. UI slices run `i18n.py extract` and every touched window's tests (now.md 11-08).

## 2. Items

### Slice 1. Importer Settings wiring (risk low)
- **User gets:** Settings > Import... lists Espanso, Beeftext, TextExpander (CSV), PhraseExpress (CSV) beside Ditto and
  CopyQ, with the same preview, report and Undo.
- **Decision:** Espanso opens the folder picker. Reason: a config is a folder and `import_espanso` takes either.
- **Decision:** eight choices; history keeps index 4 (`i < 4 ? i : i + 1`). Reason: binding and tests use 4 for it.
- **Decision:** `Warning::Note` stays English (`wording.rs:128`). Reason: notes quote the other tool's macro syntax.
- **Touches:** `controller/settings_ctl.rs:35` (`ImportSource` +4), `:83-85` (`run`), `:144` (`ImportKind` +4),
  `:158-175`, `:180` (`problem_of`); `settings_binding.rs:287` (`pick_kind`), `:1440-1444`; `platform/src/
  file_dialog.rs:16` (`PickKind::Beeftext` `*.json`, `Csv` `*.csv;*.txt`); `ui/settings.slint:448`, `:1368-1373`;
  `ui/import_texts.slint:57-68` (four `problem-not-*`); `wording.rs:377-380`.
- **Tests:** `each_import_kind_round_trips_through_of_and_kind`, `a_malformed_beeftext_file_is_not_this_kind`;
  `tests/settings_window.rs`: `import_choices_list_eight_sources`, `history_keeps_choice_four`; `file_dialog.rs` table.
- **Windows (owner):** 47 import a real Espanso folder; 48 a TextExpander CSV export (header names unverified).

### Slice 2a. `{uuid}`, date maths, paste transforms (risk low)
- **User gets:** `{uuid}`; `{date+3d}`, `{date-1w:%Y-%m-%d}`, `{time+90min}`, `{datetime+1mo:%F %H:%M}`; item menu
  "Paste as..." with Plain text, UPPERCASE, lowercase, Title Case, Trimmed, slug.
- **Decision:** offsets `[+-]N unit`, chainable, after `date`/`time`/`datetime`; units `d w mo y h min`. Reason: no `m`/`M` trap.
- **Decision:** N bounded at parse (y 100, mo 1200, d 36500, h 876000 ...). Reason: render stays infallible (`template.rs:57`).
- **Decision:** `{date:FMT}` = `{datetime:FMT}`; `datetime` stays. Reason: owner's example; saved templates use `datetime`.
- **Decision:** `{uuid}` = v4, new per occurrence, Free; app passes `getrandom` bytes via `RenderContext::with_uuids`,
  core uses `uuid::Builder::from_random_bytes`. Reason: render stays pure; no new uuid feature.
- **Decision:** transforms are paste-time only (`template::transform::apply`), no placeholder filters. Reason: `|` is taken.
- **Decision:** a transformed paste is plain text, caret at the end. Reason: HTML would disagree; `ß` -> `SS` moves offsets.
- **Decision:** slug keeps Unicode letters, other runs -> `-`; Title = first letter up, rest down. Reason: no translit table.
- **Decision:** "Paste as..." opens a chooser like Colour. Reason: menus have no submenus.
- **Touches:** `core/src/template.rs:19-58` (docs), `:87` (`KEYWORDS`), `:105` (`Part` offsets, `Part::Uuid`),
  `:241-288` (`uuids`), `:695-720`, `:974-993`; new `core/src/template/transform.rs`; `usecase.rs:21`
  (`UseRequest.transform`), `:200-225`; `controller/panel.rs:80` (`ItemMenuEntry::PasteAs`), `:279`;
  `controller/main_ctl/items.rs`; `binding.rs:1398-1429`, `main_binding.rs`; `ui/menu_texts.slint`;
  `editor_binding.rs:411` (chips).
- **Tests:** `template/tests.rs`: `uuid_renders_the_given_bytes_as_v4`, `each_uuid_occurrence_is_new`,
  `date_offsets_add_days_weeks_months_years`, `month_end_clamps`, `time_offsets_cross_midnight`,
  `date_takes_a_format_like_datetime`, `offsets_past_the_bound_are_errors_with_a_span`, `unknown_units_get_a_hint`;
  `transform.rs`: `the_transform_table` (incl. `ß`, emoji, CRLF); `use_paste.rs`:
  `paste_as_upper_is_plain_with_caret_at_end`, `paste_as_on_a_sensitive_item_still_needs_reveal`.
- **Windows (owner):** 49 "Paste as > UPPERCASE" into Word pastes plain upper text.

### Slice 2b. Richer fill-ins (risk medium)
- **User gets:** `{multiline:Name|default}`, `{pickdate:Name|FORMAT}` (today by default), and
  `{optional:Name}...{/optional}` (a tick box; the inside pastes only when ticked).
- **Decision:** reuse the `multiline` and `date` field kinds; the date box is typed (`YYYY-MM-DD`, jiff), no calendar
  popup. Reason: the component exists; a popup is new UI.
- **Decision:** sections do not nest; a `{cursor}` inside counts only when ticked. Reason: flat parser, exact spans.
- **Decision:** all three are prompts: Supporter, inside `MAX_PROMPTS`. Reason: same rule as field and choice.
- **Touches:** `template.rs:147-169` (`PromptKind::Multiline`, `Date`, `Toggle`), `:105` (`Part::Section`), `:695`,
  `:791`; `app/src/fill.rs:14`, `:89-139` (date check); `fill_binding.rs:277`; `template_field.slint` (kind `toggle`);
  `template_fill.slint:213`; `editor_binding.rs:411`.
- **Tests:** `multiline_answers_keep_their_newlines`, `pickdate_defaults_to_today_in_format`,
  `an_unticked_section_renders_nothing`, `sections_do_not_nest`, `an_unclosed_section_is_an_error`,
  `a_cursor_in_an_unticked_section_is_ignored`; `fill.rs`: `a_bad_date_is_missing_with_a_reason`;
  `tests/fill_window.rs`: `toggle_field_shows_a_check_box`, `multiline_enter_adds_a_line`.
- **Windows (owner):** 50 fill a template with all three and paste into Outlook.

### Slice 3a. Groups inside a list, data (O46, `fable.md:88`; risk medium)
- **User gets:** nothing visible; groups survive export, import, backup and undo.
- **Decision:** migration v8: `item_groups(id, list_id, name, collapsed)` + nullable `items.group_id`; members are
  contiguous, the header sits above the first. Reason: no second ordering; moving a group moves one run.
- **Decision:** membership is read by `Store::groups_of(list)`, not an `Item` field. Reason: `Item` literals in six
  crates would make a light slice compile snipdeck-app (9 GB rustc).
- **Decision:** deleting a group keeps or deletes its items (one undo step); an emptied group goes. Reason: no empty headers.
- **Touches:** `store/src/migrations.rs:71` (v8; test steps `:295-307` move to v9), `schema.rs`, `items.rs` (`group_id`,
  `move_run`), new `groups.rs`, `repo.rs`; new `core/src/group.rs`, `lib.rs`; `portable/src/dto.rs:17` (minor +1), `:87`
  (`ListDto.groups`), `:169` (`ItemDto.group`), `export.rs`, `import.rs`, `plan.rs`.
- **Tests:** `v8_keeps_items_and_adds_no_groups`, `an_older_binary_refuses_v8`, `a_group_keeps_its_members_contiguous`,
  `move_run_moves_a_whole_group`, `groups_round_trip_through_export`, `an_older_archive_imports_without_groups`.

### Slice 3b. Groups inside a list, Panel and Main (risk medium)
- **User gets:** "Group..." (Ctrl+G) on the selection; a header with name, count, chevron; click or Left/Right
  collapses; inline rename; drag a header; Ungroup; Delete group with Undo.
- **Decision:** search shows matches inside collapsed groups, under their header. Reason: a hidden match reads as lost.
- **Decision:** quick numbers 1-9 count item rows only. Reason: a number must paste.
- **Decision:** Slint name `group-header`, not `row-groups` (`panel.slint:585` is the Cards row count).
- **Touches:** `app/src/panel.rs:32` (`Row` header), `:368`, `:399`, `:541`; new `controller/group_ctl.rs`;
  `controller/panel_select.rs:45`; `history.rs`; `ui/panel.slint:58`, `:1848`, `:981-1060`; `main_window.slint:1803`;
  `binding.rs`, `main_binding.rs`; `components/item-card.slint`.
- **Tests:** `panel_select.rs`: `group_from_selection_is_one_undo_step`, `delete_group_keep_items_ungroups`,
  `collapsed_group_hides_rows_but_search_shows_them`, `quick_numbers_skip_headers`; `tests/panel_window.rs`:
  `ctrl_g_groups_the_marked_rows`, `left_collapses_and_right_expands_a_header`, `header_is_announced_with_its_count`;
  one Panel golden with a group, regenerated and opened.
- **Windows (owner):** 54 Narrator reads a header as "group, N items, collapsed".

### Slice 4. Tips (risk low)
- **User gets:** Help > Tips: 10 cards (title, two lines, a picture, "Try it"), from the tray, Help, the last onboarding
  page and a Getting started card.
- **Decision:** a fourth Help tab, not a new window. Reason: one less window to place, theme, own and test.
- **Decision:** pictures are crops of golden renders, refreshed with them. Reason: no new art; they follow UI changes.
- **Decision:** no unread badge or prompt. Reason: Supporter decision "never nag".
- **Touches:** new `app/src/tips.rs` (pure list); `controller/panel.rs:19` (`HelpTopic::Tips`); `help_binding.rs:727`;
  `ui/help.slint:125`; `platform/src/tray.rs`, `ui/tray_texts.slint`; `starter.rs:19`; `ui/onboarding.slint`;
  `ui/images/tips/`.
- **Tests:** `every_tip_has_an_image_that_exists`, `every_action_maps_to_a_command`; `tests/help_window.rs`:
  `tips_topic_opens_the_fourth_tab`, `try_it_runs_the_tip_action`.
- **Windows (owner):** 51 tray > Tips opens the Tips tab.

### Slice 5a. Triggers, matcher and data (risk medium)
- **User gets:** imported abbreviations are kept as triggers (no UI yet).
- **Decision:** v9 `triggers(trigger TEXT PRIMARY KEY, item_id UNIQUE)`, case-sensitive, 1-32 chars, no spaces. Reason: schema invariant.
- **Decision:** pure `core::trigger::Matcher` keeps the last 32 characters and fires on the last character of a trigger
  preceded by a boundary (start, space, punctuation); no space needed. Reason: as Espanso and Beeftext.
- **Decision:** importers store the abbreviation as the trigger and title = name or first line (the `Name (abbr)` rule
  goes); a clash keeps the first, with a note. Reason: replace, do not layer.
- **Touches:** new `core/src/trigger.rs`, `lib.rs`; `store/src/migrations.rs` (v9), `schema.rs`, new `triggers.rs`,
  `repo.rs`; `portable/src/dto.rs:169` (`ItemDto.trigger`), `export.rs`, `import.rs`; `importers/expander.rs`.
- **Tests:** `fires_at_the_last_character`, `needs_a_boundary_before`, `backspace_pops_one`, `reset_clears_the_buffer`,
  `longest_trigger_wins`, `buffer_never_exceeds_32`, proptest `feed_never_panics`; `a_duplicate_trigger_is_refused`,
  `trashing_an_item_frees_its_trigger`; `tests/expanders.rs`: `espanso_triggers_become_triggers`,
  `a_clashing_trigger_is_noted`.

### Slice 5b. Typed-trigger expansion (risk HIGH; Windows-only parts marked [W])
- **User gets:** Settings > Typing > "Expand abbreviations as I type" (off by default); a Trigger field in the editor;
  typing `;sig` in any app replaces it with the item.
- **Decision [W]:** `WH_KEYBOARD_LL` is installed only while on, on its own thread. Reason: off means no hook; a slow
  hook must not delay hotkeys (`windows/hotkey.rs:202`).
- **Decision [W]:** the callback only translates (`ToUnicodeEx` flag `0x4`, foreground layout) and posts; skips
  `LLKHF_INJECTED`. Reason: Windows drops slow hooks; flag 4 keeps dead keys; our own paste must not feed the matcher.
- **Decision:** reset on foreground change, arrows, Home/End, PgUp/PgDn, Enter, Esc, Tab, `VK_PROCESSKEY` (IME); no
  mouse hook. Reason: a second hook doubles the latency risk.
- **Decision:** expansion = Backspace per trigger grapheme, then the item's normal use (route, paste rules, clipboard
  restore, fill form, use count). Reason: one injection path.
- **Decision:** no expansion unless `decide_with` says Paste (never-paste, elevated, no focus), nor in a password box
  (UIA `IsPassword`, new `Target.password`), nor while Snipdeck is foreground, nor for a Sensitive item (no Trigger
  field there). Reason: a short code must not release a secret without the reveal gesture.
- **Decision:** typed characters live only in the 32-char buffer, never logged or traced. Reason: else it is a key logger.
- **Touches:** new `platform/src/typing.rs` (portable key->event table) and [W] `platform/src/windows/typing_hook.rs`;
  `route.rs:37` (`Target.password`), [W] `windows/target.rs` (UIA); [W] `windows/keyboard.rs:61` (Backspaces);
  `settings.rs:682` (`expand_triggers`, false in `new_install` `:839`); new `controller/trigger_ctl.rs`; `use_ctl.rs`;
  `item_editor.rs`, `editor_ctl/save.rs`, `editor_binding.rs`, `ui/editors.slint`; `ui/settings.slint`,
  `settings_binding.rs`.
- **Tests (Linux):** `typing.rs`: `the_key_event_table` (Shift, AltGr, Caps, dead key, IME -> Reset); `use_paste.rs`:
  `a_typed_trigger_backspaces_then_pastes`, `never_paste_app_does_not_expand`, `password_box_does_not_expand`,
  `sensitive_item_never_expands`, `off_by_default_installs_no_hook`, `a_prompted_trigger_opens_the_fill_form`;
  `tests/editor_window.rs`: `trigger_clash_shows_an_error`, `trigger_field_hidden_for_sensitive`.
- **Windows (owner, unverified until run):** 55 Notepad, Word, Chrome, Terminal expand `;sig`; 56 German layout dead key
  `^` `e` still types `ê`; 57 an Edge password box does not expand; 58 a never-paste app does not; 59 an elevated window
  does not; 60 Defender does not flag the hook and typing feels no slower.

### Slice 6. Suggest-as-you-type at the caret (risk HIGH)
- **User gets:** Settings > Typing > "Suggest snippets as I type" (off by default): after two characters that start a
  trigger, a hint at the caret shows the title and "Tab to insert".
- **Decision:** trigger prefixes only, never titles. Reason: titles match ordinary prose on most words.
- **Decision:** one suggestion (most used); Tab inserts; any other key dismisses and passes through; the hook swallows
  Tab only while the hint shows. Reason: arrows and Esc belong to the app.
- **Decision [W]:** a no-activate hint window at `TargetTracker`'s caret; no caret, no hint. Reason: focus stays in the app.
- **Decision:** same hook and the same Sensitive, never-paste and password gates as 5b. Reason: one gate.
- **Touches:** `core/src/trigger.rs` (`prefix_of`); [W] `typing_hook.rs` (swallow flag); `trigger_ctl.rs`;
  `settings.rs:682` (`suggest_triggers`); new `ui/suggest_hint.slint` + binding; [W] `window_style.rs:53`;
  `popup_window.rs`; `placement.rs:308`.
- **Tests:** `prefix_needs_two_characters`, `prefix_picks_the_most_used`; `use_paste.rs`:
  `tab_inserts_and_backspaces_the_prefix`, `other_keys_dismiss_without_swallowing`, `no_caret_no_hint`,
  `sensitive_items_are_never_suggested`.
- **Windows (owner):** 61 hint at the caret in Notepad and Word, never takes focus; 62 Tab in a browser form without a
  hint still moves focus.

### Slice 7. Rich paste (risk medium)
- **User gets:** Ctrl+V into the Panel or Main and the capture hotkey keep formatting; the item pastes formatted in
  Word/Outlook and as text in Notepad; rows show a "Formatted" mark; "Paste as > Plain text" (2a) strips it.
- **Decision:** the target picks from HTML, RTF and text offered together; no per-program "always plain". Reason:
  Windows does this; per-app rules are "Later" (HANDOVER v1.1).
- **Decision:** editing a formatted item's text says "Saving changed text removes the formatting" and drops HTML/RTF;
  "Remove formatting" does it alone. Reason: `editor_ctl/save.rs:194` drops them silently today.
- **Touches:** `host.rs:62` (`ClipboardText::read_rich`), [W] `windows/clipboard.rs:256` (reuse `:955`);
  `paste_create.rs:13` (`Clip.html`, `Clip.rtf`); `create_ctl.rs:43`, `:174`; `use_ctl.rs:298` (capture hotkey);
  `editor_ctl/save.rs:194`, `editor_ctl/open.rs:333`; `ui/editors.slint`; `panel.rs:32`; `item-card.slint`.
- **Tests:** `panel_select.rs`: `paste_create_keeps_html_and_rtf`, `a_secret_rich_paste_still_opens_the_sensitive_editor`;
  `capture_stack.rs`: `capture_hotkey_keeps_formatting`; `tests/editor_controller.rs`:
  `editing_text_drops_formatting_after_the_notice`, `remove_formatting_keeps_text`.
- **Windows (owner):** 52 bold table from Word -> Panel -> Outlook (formatted) and Notepad (text); 53 RTF from WordPad.

### Slice 8. Windows Hello unlock (Supporter, now.md 2026-10-20; risk HIGH)
- **User gets:** with a master password, Settings > Security > "Unlock with Windows Hello"; the unlock prompt offers
  Hello; the password is asked after a restart and every 7 days.
- **Decision [W]:** Hello protects an in-memory sealed copy of the data key: KEK = SHA-256 of the `KeyCredentialManager`
  signature (RSA PKCS#1 v1.5, deterministic) of a fixed challenge, sealed with `crypto/aead.rs`, opened into
  `open_with_key` (`keys.rs:194`). Nothing new on disk. Reason: the key is protected, not only the UI; a restart forgets
  the copy, as decided.
- **Decision:** after each password unlock Hello is asked once to seal the copy (turning Hello on asks twice, to check the signature repeats). Reason: the KEK needs a signature,
  which needs the user; password unlocks are rare.
- **Decision:** 7 days fixed, from the last password unlock, injected clock. Reason: no new setting.
- **Decision:** Hello cancel, failure or a removed PIN falls back to the password and drops the copy.
- **Touches:** [W] new `platform/src/windows/hello.rs` (WinRT features `Security_Credentials`, `Security_Cryptography`,
  `Storage_Streams` in `Cargo.toml:43`; blocking wait on a worker like `master_job.rs`), portable trait + fake in
  `platform/src/hello.rs`; `lock_ctl.rs:76` (`LockState.hello`), `:286`, `:299`; `unlock_binding.rs:147`;
  `ui/master_password.slint`; `settings.rs:682` (`hello_unlock`); `settings_binding.rs`; `docs/crypto.md`.
- **Tests (fake Hello):** `access.rs`: `hello_unlock_opens_with_the_sealed_copy`, `restart_needs_the_password`,
  `after_seven_days_the_password_is_needed`, `hello_cancel_falls_back`, `free_tier_has_no_hello`,
  `removing_the_master_password_drops_hello`; `tests/unlock_window.rs`: `hello_button_shows_only_when_ready`.
- **Windows (owner):** 63 enable with a PIN; 64 lock, unlock by face or fingerprint; 65 the Hello prompt is in front of
  Snipdeck (unverified); 66 a PC without Hello hides the option.

### Slice 9. serde_yaml_ng -> serde-saphyr (risk low)
- **User gets:** nothing visible; `unsafe-libyaml` (RUSTSEC-2024-0449) leaves the build.
- **Decision:** `serde-saphyr = { version = "2.1", default-features = false, features = ["deserialize"] }`. Reason:
  parsing only; drops serialize and comment deps; licence is in deny.toml's list.
- **Decision:** drive the existing `Seed` through `with_deserializer_from_str_with_options`, set `Budget`/`AliasLimits`,
  and remove the 4x budget only if `espanso_alias_bombs_and_deep_nesting_are_refused_cheaply` passes without it. Reason:
  the test, not the docs, proves the limit.
- **Unsure:** `Options`/`Budget` field names, default depth limit, licences of its deps (`granit-parser` etc.); MSRV
  1.89 is fine. Read docs.rs 2.1.1 first; `cargo deny check` settles the licences.
- **Touches:** `portable/Cargo.toml:21`; `importers/espanso.rs:36-37`, `:246`, `:329-345`, `:422-430`; `deny.toml:23`
  (remove the ignore); `Cargo.lock`; `THIRD_PARTY_NOTICES.md`; `.claude/remaining/importers.md`.
- **Tests:** every `espanso_*` in `tests/expanders.rs:659-947` unchanged and green; new
  `espanso_merge_keys_and_anchors_still_resolve`; CI `cargo deny check`.

