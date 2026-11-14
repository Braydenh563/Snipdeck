# owner-test-ctl remaining

Done (one commit, see git log): A stale-visible panel fix + hotkey re-register no-op; B toggle already existed; C re-fit after show (fit_when_settled); D image-paste toast (Ctrl+V in item editor) + i18n; picker Esc/click-away (retry activation, close on focus loss); Reset size/position inset from edges.

Verified: lib tests for new logic pass (cargo test -p snipdeck-app --lib).
NOT run: gate.sh, clippy (+1.99.1), windows msvc check (only msvc target installed, no gnu). Disk was full.

Not started / open:
- E (transparent patches): not investigated.
- F (OneNote target detection): not investigated.
- Hotkey failures: only first failure toasts (controller/toast_ctl.rs report_hotkey_problems); no Settings row error check; cause of Ctrl+Alt+Space/Shift+2/Alt+V not firing unknown (Paste next is Supporter-only; others unverified).
- Picker click-away on the same Target relies on the has-focus change in picker.slint; Windows-only to verify.
- Panel list overlap after resize: not investigated.
