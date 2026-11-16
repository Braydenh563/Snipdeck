# fix-audit-main remaining (branch claude/fix-audit-main; coordinator = session_01NKjURuyvhApCqnss7v9Php)

Env: export CARGO_TARGET_DIR=/tmp/target-main CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
Run cargo in the background (120 s tool limit). Goldens: UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test <bin> -- <names>.
Translations: python3 scripts/i18n.py extract; todo LOCALE > json; apply LOCALE json; check --strict.
Gate that counts: GATE_BASE=506333b scripts/gate.sh --changed (plain --changed compares to own upstream and skips).

## Pushed (head d56ee0a)
Code for 3, 4, 6, 7, 8, 9, 10, 13, 16, 17, 18, 20, 24, 25, 27, 28, 30, 31, 33-38, 45, 46, 61, plus earlier 1, 2, 5 (code), 11, 12, 48-55, 62.

## Written, UNCOMMITTED, NOT yet compiled or run (coordinator round 2)
- ui/tests/main_window_logic.rs: new ui_tests! block at the end (3, 4, 6, 7/8 board+trash, 9, 16, 17, 18, 20, 28).
- app/tests/toast_texts.rs: sentences for ExportedSome, ItemsTagged, TagDeleted.
- app/tests/main_controller.rs: partial export (25), tag/untag/delete toasts (24).
- app/tests/settings_window.rs: escape_closes_the_erase_question_before_anything_else (38).
- ui/tests/main_fit.rs: blanket skip replaced by scrolling_reveals() (wheels the sidebar, proves a rail button
  behind the footer becomes visible); verify it passes at 480x360@1.5 and 2.25, else fix the layout instead.
Next: compile, run these, fix, regenerate nothing unless a golden moves, commit, push.

## Coordinator's list still open
1. Real gate: GATE_BASE=506333b scripts/gate.sh --changed; full `cargo test -p snipdeck-ui`; RUSTDOCFLAGS="-D warnings" cargo doc (a gate run was started, result not read).
2. Missing tests: MAIN-5 (chooser anchored to the bulk-bar button, clamped by real size); the rest are written above.
3. Items 14, 15, 21, 22, 23, 29, 32, 39-44, 47, 56-60: each ends fixed with a test, or recorded in docs/SPEC.md / DESIGN.md as post-1.0 with a reason.
   19 stays untranslated key names (recorded in shortcut_texts.slint); 26 belongs to LIBRARY-11.
4. Audit settings_binding.rs, help_binding.rs, toast_binding.rs.
5. Nouns: make every locale use the item noun (de Eintrag, es elemento, fr élément, pt-BR item, ja 項目, zh-Hans 项目)
   in the OLD strings too (they still say Textbaustein / fragmento / extrait / trecho / スニペット / 片段).
6. Push after each gated batch; report head SHA and the real gate lines.
