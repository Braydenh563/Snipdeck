# PR #4 final review (Opus, read-only, 2026-10-18)

Scope: what earlier reviews had not covered (snipdeck-app, Picker/Fill slint, store batch, portable atomic import, scripts, deny.toml) plus a check that earlier fixes were done correctly. Head reviewed: b098050. Nothing was built or run.

Verdict: **go** for merging into `main` once #1 and #2 are fixed (then #3-#6); **no-go** for a public release (see #13).

Status key: `[ ]` open, `[x]` fixed (commit in the fix log below).

## High

- [x] 1. `paths.rs:106-115` + `main.rs:117-118`: a damaged `settings.json` is moved to `.bad`; the salvaged values (e.g. `no_paste_apps`) are used for that run only and the saver's baseline is the salvaged value, so nothing is written. Next launch sees Missing and uses defaults, so Snipdeck auto-pastes into apps the user excluded (KeePass). Fix: on `LoadOutcome::SetAside` write the salvaged settings at once (or start the saver with a `Settings::default()` baseline plus a pending change). Update wizard step 14 in `m3-app.json`.

## Medium

- [x] 2. `main.rs:172-179`: with `--hidden`, `attach_when_created` is never called, so after the first ShowPanel the Panel has no `install_panel_style` / `tracker.set_panel`; a click activates it and the Target can become Snipdeck. Fix: attach once on the first ShowPanel. **Already fixed at `main.rs:448-453` (`Effect::ShowPanel` calls `attach_panel`, which attaches once).**
- [x] 3. `store/blobs.rs:212-230`, `items.rs:229-233`, `blobs.rs:144-154`: sensitive payloads from `put_transient` stay in the `plain` map after the write and after a rollback; only `clear_transient` at lock empties it. Plaintext lingers in RAM and grows with each import. Fix: remove the entry after `write_formats` and in `rewind`.
- [x] 4. `usecase.rs:93-94`: if `record_use` fails after a successful paste the use returns `Err` and the user sees "Couldn't use this item", then retries and pastes twice. Fix: count is best-effort; keep the success toast.
- [x] 5. `binding.rs:256` / `main.rs`: an OS close of the Panel (Alt+F4, WM_CLOSE) is not routed to `Controller::close`; `visible` stays true and the toggle goes out of sync; without a tray the process lives on invisibly. Fix: `window.window().on_close_requested(.. ctl.close(); KeepWindowShown)`. Needs Windows to verify. **Fixed: `binding.rs` wires `window().on_close_requested` to `Controller::close` and answers `KeepWindowShown`. Unverified on Windows.**
- [x] 6. `portable/src/backup.rs:183-189`: `restore` uses `ImportOptions::default()` so `max_boards` is None; a Free user can restore past 3 boards (earlier security finding 8 only half fixed). Fix: give `restore` a `max_boards` parameter.

## Low

- [x] 7. `template_fill.slint:104-111`: only the first 16 fields are validated; core has no cap on prompts. Cap in core or validate in the host. **Fixed in core: `MAX_PROMPTS = 16`, `TemplateErrorKind::TooManyPrompts` (`template.rs`), with a test.**
- [x] 8. `controller.rs:661-669`: every `HotkeyProblem` says "is used by another app", even Invalid / Unavailable. Word the toast by `failure`. **Fixed: `ToastMessage::HotkeyInvalid` / `HotkeyUnavailable` beside `HotkeyInUse`, chosen by `failure` in `Controller::report_hotkey_problems`.**
- [x] 9. `main.rs:41`: `std::env::args()` panics on non-UTF-8 args (Windows `--data-dir`). Use `args_os`. **Fixed: `cli::parse` takes `OsString`, `main.rs` uses `args_os`; unix test with non-UTF-8 path.**
- [x] 10. `main.rs:89` vs `109`: service threads start before `select_backend()`; an early `invoke_from_event_loop` can fail and the event is dropped (e.g. a second launch's show request). Select the backend first or buffer events. **Fixed: `select_backend()` now runs before `platform::connect` (service threads) in `main.rs`.**
- [ ] 11. `ui/tests/review_fixes_logic.rs:59-66,264`: the CJK test passes silently when `fc-list` is missing (always on CI and Windows). Make it `#[ignore]` with a reason, or bundle a CJK test font. (open: review_fixes_logic.rs:262-265 still returns silently when `fc-list` finds no Japanese font; no `#[ignore]`; no CJK font is bundled in crates/snipdeck-ui/fonts)
- [x] 12. `binding.rs:412`: a sensitive row's `preview` is passed through relying on the store keeping it empty. Blank it when `row.sensitive`. **Fixed: `panel_row` blanks `preview` when `row.sensitive`; the test now feeds a non-empty preview.**
- [ ] 13. Unfinished paths a release would show: Picker and TemplateFill have no caller in `snipdeck-app`; `OpenPicker`, `StartCapture`, `PasteNext` fall into `_ => {}` (`main.rs:226`); a prompted template only toasts "Fill in this template first."; `Used::cursor_from_end` is ignored; 156 strings untranslated. (In progress: wiring agent and translation pass.) (open: `Used::cursor_from_end` is not applied (crates/snipdeck-app/src/usecase.rs:46-48, 'Not yet applied by the pipeline'); 120 entries per locale are still untranslated in crates/snipdeck-ui/lang/*/LC_MESSAGES/snipdeck-ui.po; Picker and fill are wired at main.rs:294-295 and the 'Fill in this template first.' toast is gone)
- [ ] 14. Hygiene: `.claude/reviews/` is untracked, not ignored (could be committed by accident); HANDOVER "Next steps 1" is stale; `m3-app.json` step 14 says "default settings" but salvaged values are used; `tools/windows-test/m2-harness` has its own `Cargo.lock` that `cargo deny` does not check. (open: HANDOVER 'Next steps' item 1 is stale (docs/HANDOVER.md:189-195 says no window binding for picker or fill); tools/windows-test/tests/m3-app.json:122 expects defaults after a salvage; tools/windows-test/m2-harness/Cargo.lock is outside the workspace (Cargo.toml:4) and not in deny.toml; the .claude/reviews/ part is done (.gitignore:8))

## Earlier fixes, re-checked

Done correctly: security 1, 2, 3, 5, 6, 7, 9; tray 1, 2, 5, 6, 7 (except SetAside, see #1). Partly done: security 8 (#6). `deny.toml` ignores: acceptable (four unmaintained-crate advisories via Slint, each with a reason). Batch API: no data-loss path found. Not verified: all Win32 behaviour, real-winit key text for Ctrl+E, pixel claims.
