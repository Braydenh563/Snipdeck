# hub-small remaining work

## Done
- Merges: uxb, uxa (final tip 0ec9cfe), release branch (6e0babe, RepaintNudge fix); i18n and codemap regenerated.
- Compile fixes; paste rules set in `Controller::new` and `update_settings` (+ test).
- U6, U8, A6, A7, F2, A22, A23, U13, organise-failure cause; ticked in docs/reviews/fable-analysis.md.
- Owner bug: first paste injected old clipboard; `write_confirmed` guard in pipeline + wizard step 27.
- Owner UI: Main Panel button is a Hide/Show toggle (palette too); sidebar list reorder 2px drop line.
- All 6 locales translated; `i18n.py check` exits 0.

## Next
- Run the full gate and `cargo +1.99.1 clippy` once disk is free (last build hit ENOSPC at link; the Rust compiled).
- Run ui and app tests, in particular main_window_logic (list drop line) and main_controller (panel toggle): never executed.
- Step 4 goldens (UPDATE_GOLDEN=1), step 6 UX-C (O30, O26, O31).

## Skipped / blocked
- U9, A25 (over the size budget). O55 is not defined anywhere.
- F2 deviation: error toast hold is 10 s.
