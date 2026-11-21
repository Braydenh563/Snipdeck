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

## Round 2
Code + test: MAIN-5 test, 15 (F2/Delete on sidebar list/tag; splitter Home/End, double-click reset), 39 (unused-tag wording), 42 (restored notice in list/board editors), 47 (tour import row), 59 (palette checks). Nouns: six .po files + i18n test.
Recorded post-1.0 in docs/SPEC.md section 15 (and the DESIGN.md Detail row): 14, 15 drop targets, 21, 22, 23, 29, 32, 39 New tag/merge, 40, 41, 43, 44, 56, 57, 58, 60.
Binding audit: .claude/audit/bindings.md (agent).
Next: green run (editors_logic, main_window_logic, i18n, snipdeck-app main_controller + palette); UPDATE_GOLDEN=1 for onboarding-welcome goldens (--test editors); GATE_BASE=506333b scripts/gate.sh --changed; push; send_message to coordinator.

## Round 3 (paused by owner, usage limit)
Un-deferred all MAIN-14,21,22,23,29,32,39,40,41,43,44,56,57,58,60, MAIN-15 drops, binding audit SET/HELP/TOAST.
- Merged + pushed: agent-c (SET-1..3, HELP-1..5, TOAST-1..2, MAIN-43,44,56,57,58,60; see agent-c.md) and agent-b (MAIN-39,40,41,15 drops,32; see agent-b.md) as WIP d5729dd, no gate run.
- agent-a merged; all agents merged and verified (gate green at head below).
- Next: merge agent-a; python3 scripts/i18n.py extract; regenerate goldens (tag-manager-delete-light conflicted c vs b; editors + help-tips); touch crates/snipdeck-store/src/*.rs before cargo; fmt, clippy (+1.99.1 if installed), full tests, codemap, GATE_BASE=506333b scripts/gate.sh --changed; push; send_message the coordinator.
- Windows run: see Windows sections in agent-a/b/c.md.
