# Pre-Fable facts (tip 37d1d88, 2026-10-20)

CI on 5bb0241: Linux green; Windows red only on the Recent at-rest test, fixed in 37d1d88. Full workspace tests green on the PR #23 merge (Linux).

## Rust files over 800 lines
- crates/snipdeck-app/src/controller.rs 4252
- crates/snipdeck-app/tests/settings_window.rs 2228
- crates/snipdeck-ui/ui/main_window.slint 2072
- crates/snipdeck-app/src/settings_binding.rs 1864
- crates/snipdeck-app/tests/main_controller.rs 1779
- crates/snipdeck-ui/tests/panel_logic.rs 1513
- crates/snipdeck-ui/ui/panel.slint 1507
- crates/snipdeck-ui/ui/editors.slint 1469
- crates/snipdeck-ui/tests/editors_logic.rs 1467
- crates/snipdeck-app/tests/editor_controller.rs 1434
- crates/snipdeck-platform/src/windows/clipboard.rs 1426
- crates/snipdeck-app/src/main_binding.rs 1397
- crates/snipdeck-app/tests/panel_window.rs 1394
- crates/snipdeck-app/src/controller/main_ctl.rs 1316
- crates/snipdeck-app/tests/main_window.rs 1277
- crates/snipdeck-ui/ui/settings.slint 1271
- crates/snipdeck-ui/tests/main_window_logic.rs 1266
- crates/snipdeck-app/src/binding.rs 1263
- crates/snipdeck-portable/tests/importers.rs 1203
- crates/snipdeck-app/src/main_window.rs 1186
- crates/snipdeck-app/tests/master_password.rs 1112
- crates/snipdeck-app/src/controller/editor_ctl.rs 1057
- crates/snipdeck-app/src/settings.rs 1037
- crates/snipdeck-core/src/template.rs 1021
- crates/snipdeck-portable/src/import.rs 974
- crates/snipdeck-store/src/blobs.rs 902
- crates/snipdeck-platform/src/win_formats.rs 866
- crates/snipdeck-platform/src/chord.rs 866
- crates/snipdeck-core/src/template/tests.rs 851
- crates/snipdeck-app/tests/editor_window.rs 851
- crates/snipdeck-app/src/main.rs 827
- crates/snipdeck-store/src/crypto/keyring.rs 802

## unwrap/expect/panic in crates/*/src (includes #[cfg(test)] modules; top 15)
- crates/snipdeck-app/src/controller.rs:107
- crates/snipdeck-store/src/blobs.rs:72
- crates/snipdeck-store/src/schema.rs:59
- crates/snipdeck-store/src/migrations.rs:48
- crates/snipdeck-store/src/crypto/keyring.rs:44
- crates/snipdeck-app/src/panel.rs:42
- crates/snipdeck-app/src/main_window.rs:42
- crates/snipdeck-app/src/starter.rs:41
- crates/snipdeck-app/src/paths.rs:40
- crates/snipdeck-portable/src/plan.rs:35
- crates/snipdeck-platform/src/win_formats.rs:31
- crates/snipdeck-core/src/template/tests.rs:30
- crates/snipdeck-app/src/item_editor.rs:30
- crates/snipdeck-app/src/drafts.rs:29
- crates/snipdeck-app/src/capture.rs:29

## unsafe blocks (count per file)
- crates/snipdeck-platform/src/windows/clipboard.rs:61
- crates/snipdeck-platform/src/windows/target.rs:41
- crates/snipdeck-platform/src/windows/hidden_window.rs:23
- crates/snipdeck-platform/src/windows/tray.rs:19
- crates/snipdeck-portable/src/archive.rs:16
- crates/snipdeck-platform/src/windows/window_style.rs:12
- crates/snipdeck-platform/src/windows/single_instance.rs:11
- crates/snipdeck-platform/src/windows/hotkey.rs:8
- crates/snipdeck-platform/src/windows/system_look.rs:6
- crates/snipdeck-platform/src/windows/file_dialog.rs:6
- crates/snipdeck-platform/src/windows/autostart.rs:6
- crates/snipdeck-platform/src/windows/screen.rs:4
- crates/snipdeck-platform/src/windows/dpi.rs:4
- crates/snipdeck-platform/src/windows/dpapi.rs:4
- crates/snipdeck-platform/src/windows/keyboard.rs:3
- crates/snipdeck-platform/src/lib.rs:3
- crates/snipdeck-ui/src/lib.rs:1
- crates/snipdeck-store/src/lib.rs:1
- crates/snipdeck-portable/src/lib.rs:1
- crates/snipdeck-portable/src/error.rs:1
- crates/snipdeck-core/src/lib.rs:1
- crates/snipdeck-app/src/platform.rs:1
- crates/snipdeck-app/src/main.rs:1
- crates/snipdeck-app/src/lib.rs:1

## TODO/FIXME/XXX
- crates/snipdeck-app/src/settings_binding.rs:52:// TODO(owner): put the real support address here, and change the placeholder pinned in
- Binary file crates/snipdeck-ui/fonts/IBMPlexMono-Regular.ttf matches
- Binary file crates/snipdeck-ui/fonts/IBMPlexSans-Regular.ttf matches
- Binary file crates/snipdeck-ui/tests/golden/board-editor-last-dark.png matches
- Binary file crates/snipdeck-ui/tests/golden/deck-light.png matches
- Binary file crates/snipdeck-ui/tests/golden/deck-scale-dark.png matches
- Binary file crates/snipdeck-ui/tests/golden/deck-scale-light.png matches
- Binary file crates/snipdeck-ui/tests/golden/dialog-destructive-light.png matches
- Binary file crates/snipdeck-ui/tests/golden/dialog-light.png matches
- Binary file crates/snipdeck-ui/tests/golden/empty-light.png matches
- Binary file crates/snipdeck-ui/tests/golden/fill-dark.png matches
- Binary file crates/snipdeck-ui/tests/golden/fill-light.png matches
- Binary file crates/snipdeck-ui/tests/golden/fill-long-light.png matches
- Binary file crates/snipdeck-ui/tests/golden/formctl-focus-stepper-light.png matches
- Binary file crates/snipdeck-ui/tests/golden/formctl-light.png matches
- Binary file crates/snipdeck-ui/tests/golden/formctl-select-open-dark.png matches
- Binary file crates/snipdeck-ui/tests/golden/forms-focus-segmented-light.png matches
- Binary file crates/snipdeck-ui/tests/golden/forms-focus-stepper-light.png matches
- Binary file crates/snipdeck-ui/tests/golden/forms-light.png matches
- Binary file crates/snipdeck-ui/tests/golden/forms-select-open-dark.png matches

## i18n

## Prior reviews
- M1-review.md
- M3-security-review.md
- M3-ui-components-review.md
- M3-win32-tray-review.md
- PR4-final-review.md
- competitors-2026-10-18.md
- fable-analysis.md
- pre-fable-facts.md
- security-review-2026-10-18.md
- security-v1.md
- translation-review.md
