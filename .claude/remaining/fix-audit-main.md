# fix-audit-main remaining (branch claude/fix-audit-main; coordinator = session_01NKjURuyvhApCqnss7v9Php)

Env: export CARGO_TARGET_DIR=/tmp/target-main CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
Run cargo in the background (120 s tool limit). Goldens: UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test <bin> -- <names>.
Translations: python3 scripts/i18n.py extract; todo LOCALE > json; apply LOCALE json; check --strict.
Gate that counts: GATE_BASE=506333b scripts/gate.sh --changed (plain --changed compares to own upstream and skips).

## Pushed (head d56ee0a)
Code for 3, 4, 6, 7, 8, 9, 10, 13, 16, 17, 18, 20, 24, 25, 27, 28, 30, 31, 33-38, 45, 46, 61, plus earlier 1, 2, 5 (code), 11, 12, 48-55, 62.

## Verified and pushed (head 5a63156)
Real gate (GATE_BASE=506333b --changed): all ok, failed none. Full `cargo test -p snipdeck-ui` ok; rustdoc -D warnings ok;
main_fit, main_window_logic, main_controller, toast_texts, settings_window ok. Dedicated tests exist for 3,4,6,7,8,9,13,16,17,18,20,24,25,28,38.
main_fit blanket skip replaced by scrolling_reveals() (passes).

## Coordinator's list still open
1. DONE. Real gate: GATE_BASE=506333b scripts/gate.sh --changed; full `cargo test -p snipdeck-ui`; RUSTDOCFLAGS="-D warnings" cargo doc .
2. Missing tests: MAIN-5 (chooser anchored to the bulk-bar button, clamped by real size); the rest are written above.
3. Items 14, 15, 21, 22, 23, 29, 32, 39-44, 47, 56-60: each ends fixed with a test, or recorded in docs/SPEC.md / DESIGN.md as post-1.0 with a reason.
   19 stays untranslated key names (recorded in shortcut_texts.slint); 26 belongs to LIBRARY-11.
4. Audit settings_binding.rs, help_binding.rs, toast_binding.rs.
5. Nouns: make every locale use the item noun (de Eintrag, es elemento, fr élément, pt-BR item, ja 項目, zh-Hans 项目)
   in the OLD strings too (they still say Textbaustein / fragmento / extrait / trecho / スニペット / 片段).
6. Push after each gated batch; report head SHA and the real gate lines.
