# Deep audit: extras (head dd4f5847)

## Part 1: file-to-agent map

Scope: every tracked `.rs`, `.slint`, `.po`/`.pot` and `Cargo.toml` under `crates/`, plus `scripts/`,
`packaging/`, `tools/`, `spikes/`, `.github/workflows/`, `site/`, `design/`, and the root build and licence
files. First matching rule wins (the rules are in `work/map.py` beside this file, rerunnable). Test files
follow the area of the code they test.

Shared files with more than one natural owner: `crates/snipdeck-app/src/controller.rs`, `main.rs`, `host.rs`
and `wording.rs` (extras owns the file; each area owns its own functions in it). `ui/components/*.slint` are
ux unless a single surface uses them. `platform` is large because it carries `tools/launcher/`,
`tools/windows-test/` and `spikes/` (148 files, 62 of them are launcher/test fixtures).

| area | files |
|---|---|
| extras | 96 |
| library | 60 |
| panel | 74 |
| main | 47 |
| privacy | 42 |
| data | 52 |
| tools | 62 |
| platform | 148 |
| ux | 49 |
| perf | 8 |
| total | 638 |

### extras (96)

- .cargo/config.toml
- .github/workflows/audit.yml
- .github/workflows/ci.yml
- .github/workflows/release-build.yml
- .github/workflows/release.yml
- .github/workflows/secrets.yml
- Cargo.lock
- Cargo.toml
- LICENSE
- THIRD_PARTY_NOTICES.md
- clippy.toml
- crates/snipdeck-app/Cargo.toml
- crates/snipdeck-app/build.rs
- crates/snipdeck-app/src/cli.rs
- crates/snipdeck-app/src/controller.rs
- crates/snipdeck-app/src/controller/tests.rs
- crates/snipdeck-app/src/crash.rs
- crates/snipdeck-app/src/demo.rs
- crates/snipdeck-app/src/diag.rs
- crates/snipdeck-app/src/doctor.rs
- crates/snipdeck-app/src/failure.rs
- crates/snipdeck-app/src/headless.rs
- crates/snipdeck-app/src/host.rs
- crates/snipdeck-app/src/lib.rs
- crates/snipdeck-app/src/main.rs
- crates/snipdeck-app/src/ownership.rs
- crates/snipdeck-app/src/paths.rs
- crates/snipdeck-app/src/screenshot.rs
- crates/snipdeck-app/src/starter.rs
- crates/snipdeck-app/src/tips.rs
- crates/snipdeck-app/src/tour.rs
- crates/snipdeck-app/src/trace.rs
- crates/snipdeck-app/src/window_kind.rs
- crates/snipdeck-app/src/wording.rs
- crates/snipdeck-app/tests/app_icon.rs
- crates/snipdeck-app/tests/budget.rs
- crates/snipdeck-app/tests/crash_hook.rs
- crates/snipdeck-app/tests/everything_free.rs
- crates/snipdeck-app/tests/legacy_files.rs
- crates/snipdeck-app/tests/menu_form_texts.rs
- crates/snipdeck-app/tests/screenshot.rs
- crates/snipdeck-app/tests/support/mod.rs
- crates/snipdeck-app/tests/template_texts.rs
- crates/snipdeck-app/tests/toast_texts.rs
- crates/snipdeck-ui/Cargo.toml
- crates/snipdeck-ui/build.rs
- crates/snipdeck-ui/lang/de/LC_MESSAGES/snipdeck-ui.po
- crates/snipdeck-ui/lang/es/LC_MESSAGES/snipdeck-ui.po
- crates/snipdeck-ui/lang/fr/LC_MESSAGES/snipdeck-ui.po
- crates/snipdeck-ui/lang/ja/LC_MESSAGES/snipdeck-ui.po
- crates/snipdeck-ui/lang/pt-BR/LC_MESSAGES/snipdeck-ui.po
- crates/snipdeck-ui/lang/snipdeck-ui.pot
- crates/snipdeck-ui/lang/zh-Hans/LC_MESSAGES/snipdeck-ui.po
- crates/snipdeck-ui/src/fonts.rs
- crates/snipdeck-ui/src/lib.rs
- crates/snipdeck-ui/src/testing.rs
- crates/snipdeck-ui/tests/i18n.rs
- crates/snipdeck-ui/tests/lib_units.rs
- crates/snipdeck-ui/tests/no_paid_tier_strings.rs
- crates/snipdeck-ui/tests/support/mod.rs
- crates/snipdeck-ui/ui/gallery/i18n_probe.slint
- deny.toml
- design/tokens/palette.json
- design/tools/palette.py
- rust-toolchain.toml
- scripts/bump_version.py
- scripts/codemap.py
- scripts/gate.sh
- scripts/i18n.py
- scripts/make_logo.py
- scripts/orient.sh
- scripts/readme_shots.py
- scripts/test_bump_version.py
- scripts/test_i18n.py
- scripts/third_party_notices.py
- scripts/tips_images.py
- site/README.txt
- site/favicon.svg
- site/fonts/IBMPlexMono-Medium.woff2
- site/fonts/IBMPlexMono-Regular.woff2
- site/fonts/IBMPlexSans-Medium.woff2
- site/fonts/IBMPlexSans-Regular.woff2
- site/fonts/IBMPlexSans-SemiBold.woff2
- site/fonts/OFL.txt
- site/img/fill-dark.png
- site/img/fill-light.png
- site/img/hero-dark.png
- site/img/hero-light.png
- site/img/main-dark.png
- site/img/main-light.png
- site/img/panel-dark.png
- site/img/panel-light.png
- site/img/picker-dark.png
- site/img/picker-light.png
- site/index.html
- site/style.css

### library (60)

- crates/snipdeck-app/src/controller/editor_ctl.rs
- crates/snipdeck-app/src/controller/editor_ctl/open.rs
- crates/snipdeck-app/src/controller/editor_ctl/save.rs
- crates/snipdeck-app/src/controller/group_ctl.rs
- crates/snipdeck-app/src/controller/history_ctl.rs
- crates/snipdeck-app/src/controller/library.rs
- crates/snipdeck-app/src/controller/tests/main_groups.rs
- crates/snipdeck-app/src/controller/views.rs
- crates/snipdeck-app/src/editor_binding.rs
- crates/snipdeck-app/src/fill.rs
- crates/snipdeck-app/src/fill_binding.rs
- crates/snipdeck-app/src/history.rs
- crates/snipdeck-app/src/item_editor.rs
- crates/snipdeck-app/src/item_preview.rs
- crates/snipdeck-app/src/list_text.rs
- crates/snipdeck-app/src/search_mark.rs
- crates/snipdeck-app/src/search_suggest.rs
- crates/snipdeck-app/tests/editor_controller.rs
- crates/snipdeck-app/tests/fill_window.rs
- crates/snipdeck-app/tests/history.rs
- crates/snipdeck-core/Cargo.toml
- crates/snipdeck-core/src/board.rs
- crates/snipdeck-core/src/content.rs
- crates/snipdeck-core/src/error.rs
- crates/snipdeck-core/src/format.rs
- crates/snipdeck-core/src/group.rs
- crates/snipdeck-core/src/ids.rs
- crates/snipdeck-core/src/item.rs
- crates/snipdeck-core/src/key.rs
- crates/snipdeck-core/src/lib.rs
- crates/snipdeck-core/src/list.rs
- crates/snipdeck-core/src/name.rs
- crates/snipdeck-core/src/placement.rs
- crates/snipdeck-core/src/search.rs
- crates/snipdeck-core/src/search/tests.rs
- crates/snipdeck-core/src/tag.rs
- crates/snipdeck-core/src/template.rs
- crates/snipdeck-core/src/template/tests.rs
- crates/snipdeck-core/src/template/transform.rs
- crates/snipdeck-core/src/trigger.rs
- crates/snipdeck-core/src/usage.rs
- crates/snipdeck-store/src/batch.rs
- crates/snipdeck-store/src/boards.rs
- crates/snipdeck-store/src/fts.rs
- crates/snipdeck-store/src/groups.rs
- crates/snipdeck-store/src/items.rs
- crates/snipdeck-store/src/rank.rs
- crates/snipdeck-store/src/tags.rs
- crates/snipdeck-store/src/trash.rs
- crates/snipdeck-store/src/triggers.rs
- crates/snipdeck-store/tests/batch.rs
- crates/snipdeck-store/tests/groups.rs
- crates/snipdeck-store/tests/items.rs
- crates/snipdeck-store/tests/ordering.rs
- crates/snipdeck-store/tests/properties.rs
- crates/snipdeck-store/tests/search.rs
- crates/snipdeck-store/tests/tags.rs
- crates/snipdeck-store/tests/trash.rs
- crates/snipdeck-store/tests/triggers.rs
- crates/snipdeck-ui/ui/components/template_field.slint

### panel (74)

- crates/snipdeck-app/src/binding.rs
- crates/snipdeck-app/src/command_binding.rs
- crates/snipdeck-app/src/commands.rs
- crates/snipdeck-app/src/controller/commands_ctl.rs
- crates/snipdeck-app/src/controller/create_ctl.rs
- crates/snipdeck-app/src/controller/dock_ctl.rs
- crates/snipdeck-app/src/controller/panel.rs
- crates/snipdeck-app/src/controller/panel_ctl.rs
- crates/snipdeck-app/src/controller/panel_select.rs
- crates/snipdeck-app/src/controller/tests/dock.rs
- crates/snipdeck-app/src/controller/tests/menus_frame.rs
- crates/snipdeck-app/src/controller/tests/panel_select.rs
- crates/snipdeck-app/src/controller/tests/picker.rs
- crates/snipdeck-app/src/controller/tests/predictable.rs
- crates/snipdeck-app/src/controller/tests/use_paste.rs
- crates/snipdeck-app/src/controller/trigger_ctl.rs
- crates/snipdeck-app/src/controller/use_ctl.rs
- crates/snipdeck-app/src/dock.rs
- crates/snipdeck-app/src/dock_drag_tests.rs
- crates/snipdeck-app/src/drop_batch.rs
- crates/snipdeck-app/src/drop_read.rs
- crates/snipdeck-app/src/frame.rs
- crates/snipdeck-app/src/palette.rs
- crates/snipdeck-app/src/panel.rs
- crates/snipdeck-app/src/paste_create.rs
- crates/snipdeck-app/src/paste_next.rs
- crates/snipdeck-app/src/picker.rs
- crates/snipdeck-app/src/picker_binding.rs
- crates/snipdeck-app/src/placement.rs
- crates/snipdeck-app/src/suggest_hint.rs
- crates/snipdeck-app/src/typing_worker.rs
- crates/snipdeck-app/src/usecase.rs
- crates/snipdeck-app/tests/panel_window.rs
- crates/snipdeck-app/tests/picker_window.rs
- crates/snipdeck-app/tests/suggest_hint.rs
- crates/snipdeck-platform/src/pipeline.rs
- crates/snipdeck-platform/src/route.rs
- crates/snipdeck-platform/src/typing.rs
- crates/snipdeck-platform/src/windows/keyboard.rs
- crates/snipdeck-platform/src/windows/target.rs
- crates/snipdeck-platform/src/windows/typing_hook.rs
- crates/snipdeck-ui/src/layout.rs
- crates/snipdeck-ui/src/selection.rs
- crates/snipdeck-ui/tests/deck.rs
- crates/snipdeck-ui/tests/deck_logic.rs
- crates/snipdeck-ui/tests/fill.rs
- crates/snipdeck-ui/tests/fill_logic.rs
- crates/snipdeck-ui/tests/panel.rs
- crates/snipdeck-ui/tests/panel_logic.rs
- crates/snipdeck-ui/tests/picker.rs
- crates/snipdeck-ui/tests/picker_logic.rs
- crates/snipdeck-ui/tests/quiet_card.rs
- crates/snipdeck-ui/tests/quiet_card_logic.rs
- crates/snipdeck-ui/tests/suggest_hint.rs
- crates/snipdeck-ui/tests/support/fill_fixture.rs
- crates/snipdeck-ui/tests/support/panel_fixture.rs
- crates/snipdeck-ui/tests/support/picker_fixture.rs
- crates/snipdeck-ui/ui/command_texts.slint
- crates/snipdeck-ui/ui/components/board-switcher.slint
- crates/snipdeck-ui/ui/components/command-palette.slint
- crates/snipdeck-ui/ui/components/deck-edge.slint
- crates/snipdeck-ui/ui/components/dock-handle.slint
- crates/snipdeck-ui/ui/components/edge-scroll.slint
- crates/snipdeck-ui/ui/components/lists-menu.slint
- crates/snipdeck-ui/ui/components/pinned-strip.slint
- crates/snipdeck-ui/ui/components/quick-add.slint
- crates/snipdeck-ui/ui/components/suggest-dropdown.slint
- crates/snipdeck-ui/ui/components/tab.slint
- crates/snipdeck-ui/ui/gallery/deck.slint
- crates/snipdeck-ui/ui/panel.slint
- crates/snipdeck-ui/ui/picker.slint
- crates/snipdeck-ui/ui/suggest_hint.slint
- crates/snipdeck-ui/ui/template_fill.slint
- crates/snipdeck-ui/ui/template_texts.slint

### main (47)

- crates/snipdeck-app/src/controller/main_ctl.rs
- crates/snipdeck-app/src/controller/main_ctl/editors.rs
- crates/snipdeck-app/src/controller/main_ctl/items.rs
- crates/snipdeck-app/src/controller/main_ctl/lists.rs
- crates/snipdeck-app/src/controller/onboarding_ctl.rs
- crates/snipdeck-app/src/controller/settings_ctl.rs
- crates/snipdeck-app/src/controller/tests/look_settings.rs
- crates/snipdeck-app/src/controller/tests/onboarding.rs
- crates/snipdeck-app/src/controller/tests/toasts.rs
- crates/snipdeck-app/src/controller/toast_ctl.rs
- crates/snipdeck-app/src/help_binding.rs
- crates/snipdeck-app/src/main_binding.rs
- crates/snipdeck-app/src/main_window.rs
- crates/snipdeck-app/src/onboarding_binding.rs
- crates/snipdeck-app/src/settings.rs
- crates/snipdeck-app/src/settings_binding.rs
- crates/snipdeck-app/src/text_scale.rs
- crates/snipdeck-app/src/toast.rs
- crates/snipdeck-app/src/toast_binding.rs
- crates/snipdeck-app/tests/editor_window.rs
- crates/snipdeck-app/tests/help_window.rs
- crates/snipdeck-app/tests/main_controller.rs
- crates/snipdeck-app/tests/main_window.rs
- crates/snipdeck-app/tests/settings_window.rs
- crates/snipdeck-ui/tests/editors.rs
- crates/snipdeck-ui/tests/editors_logic.rs
- crates/snipdeck-ui/tests/group_header_measure.rs
- crates/snipdeck-ui/tests/main_window.rs
- crates/snipdeck-ui/tests/main_window_logic.rs
- crates/snipdeck-ui/tests/support/editor_fixture.rs
- crates/snipdeck-ui/tests/support/main_fixture.rs
- crates/snipdeck-ui/tests/tray_texts.rs
- crates/snipdeck-ui/ui/editors.slint
- crates/snipdeck-ui/ui/form_texts.slint
- crates/snipdeck-ui/ui/help.slint
- crates/snipdeck-ui/ui/icons.slint
- crates/snipdeck-ui/ui/import_texts.slint
- crates/snipdeck-ui/ui/list-icons.slint
- crates/snipdeck-ui/ui/main_window.slint
- crates/snipdeck-ui/ui/menu_texts.slint
- crates/snipdeck-ui/ui/onboarding.slint
- crates/snipdeck-ui/ui/settings.slint
- crates/snipdeck-ui/ui/shortcut_texts.slint
- crates/snipdeck-ui/ui/tip_texts.slint
- crates/snipdeck-ui/ui/toast_texts.slint
- crates/snipdeck-ui/ui/toast_window.slint
- crates/snipdeck-ui/ui/tray_texts.slint

### privacy (42)

- crates/snipdeck-app/src/capture.rs
- crates/snipdeck-app/src/controller/lock_ctl.rs
- crates/snipdeck-app/src/controller/lock_ctl/hello_ctl.rs
- crates/snipdeck-app/src/controller/recent_ctl.rs
- crates/snipdeck-app/src/controller/tests/access.rs
- crates/snipdeck-app/src/controller/tests/capture_stack.rs
- crates/snipdeck-app/src/controller/tests/recent.rs
- crates/snipdeck-app/src/hello_job.rs
- crates/snipdeck-app/src/key_provider.rs
- crates/snipdeck-app/src/master_job.rs
- crates/snipdeck-app/src/master_password.rs
- crates/snipdeck-app/src/recent.rs
- crates/snipdeck-app/src/unlock_binding.rs
- crates/snipdeck-app/tests/master_password.rs
- crates/snipdeck-app/tests/recent_at_rest.rs
- crates/snipdeck-app/tests/unlock_window.rs
- crates/snipdeck-platform/src/cf_html.rs
- crates/snipdeck-platform/src/clipboard_history.rs
- crates/snipdeck-platform/src/clipboard_watch.rs
- crates/snipdeck-platform/src/hello.rs
- crates/snipdeck-platform/src/payload.rs
- crates/snipdeck-platform/src/win_formats.rs
- crates/snipdeck-platform/src/windows/clipboard.rs
- crates/snipdeck-platform/src/windows/clipboard_history.rs
- crates/snipdeck-platform/src/windows/dpapi.rs
- crates/snipdeck-platform/src/windows/erase.rs
- crates/snipdeck-platform/src/windows/hello.rs
- crates/snipdeck-portable/tests/sensitive.rs
- crates/snipdeck-store/src/crypto.rs
- crates/snipdeck-store/src/crypto/aead.rs
- crates/snipdeck-store/src/crypto/envelope.rs
- crates/snipdeck-store/src/crypto/error.rs
- crates/snipdeck-store/src/crypto/hello.rs
- crates/snipdeck-store/src/crypto/key.rs
- crates/snipdeck-store/src/crypto/keyring.rs
- crates/snipdeck-store/src/crypto/provider.rs
- crates/snipdeck-store/src/keys.rs
- crates/snipdeck-store/src/recent.rs
- crates/snipdeck-store/tests/crypto.rs
- crates/snipdeck-store/tests/recent.rs
- crates/snipdeck-store/tests/remanence.rs
- crates/snipdeck-ui/ui/master_password.slint

### data (52)

- crates/snipdeck-app/src/drafts.rs
- crates/snipdeck-app/src/maintenance.rs
- crates/snipdeck-app/src/settings_store.rs
- crates/snipdeck-app/tests/import_preview.rs
- crates/snipdeck-app/tests/rebuild.rs
- crates/snipdeck-portable/Cargo.toml
- crates/snipdeck-portable/src/archive.rs
- crates/snipdeck-portable/src/backup.rs
- crates/snipdeck-portable/src/dto.rs
- crates/snipdeck-portable/src/error.rs
- crates/snipdeck-portable/src/export.rs
- crates/snipdeck-portable/src/import.rs
- crates/snipdeck-portable/src/importers.rs
- crates/snipdeck-portable/src/importers/beeftext.rs
- crates/snipdeck-portable/src/importers/copyq.rs
- crates/snipdeck-portable/src/importers/csvtable.rs
- crates/snipdeck-portable/src/importers/ditto.rs
- crates/snipdeck-portable/src/importers/espanso.rs
- crates/snipdeck-portable/src/importers/expander.rs
- crates/snipdeck-portable/src/importers/folder.rs
- crates/snipdeck-portable/src/importers/history.rs
- crates/snipdeck-portable/src/importers/phraseexpress.rs
- crates/snipdeck-portable/src/importers/textexpander.rs
- crates/snipdeck-portable/src/lib.rs
- crates/snipdeck-portable/src/limits.rs
- crates/snipdeck-portable/src/plan.rs
- crates/snipdeck-portable/src/report.rs
- crates/snipdeck-portable/tests/archive_roundtrip.rs
- crates/snipdeck-portable/tests/backup.rs
- crates/snipdeck-portable/tests/common/mod.rs
- crates/snipdeck-portable/tests/expanders.rs
- crates/snipdeck-portable/tests/groups.rs
- crates/snipdeck-portable/tests/hostile.rs
- crates/snipdeck-portable/tests/importers.rs
- crates/snipdeck-portable/tests/triggers.rs
- crates/snipdeck-portable/tests/validation.rs
- crates/snipdeck-store/Cargo.toml
- crates/snipdeck-store/src/aside.rs
- crates/snipdeck-store/src/blobs.rs
- crates/snipdeck-store/src/codec.rs
- crates/snipdeck-store/src/error.rs
- crates/snipdeck-store/src/lib.rs
- crates/snipdeck-store/src/migrations.rs
- crates/snipdeck-store/src/repo.rs
- crates/snipdeck-store/src/schema.rs
- crates/snipdeck-store/src/store.rs
- crates/snipdeck-store/tests/blobs.rs
- crates/snipdeck-store/tests/change_counter.rs
- crates/snipdeck-store/tests/common/mod.rs
- crates/snipdeck-store/tests/fixture.rs
- crates/snipdeck-store/tests/open.rs
- crates/snipdeck-store/tests/release_guard.rs

### tools (62)

- crates/snipdeck-app/src/accent.rs
- crates/snipdeck-app/src/binding/colours.rs
- crates/snipdeck-app/src/colour_words.rs
- crates/snipdeck-app/src/colours_panel.rs
- crates/snipdeck-app/src/controller/colour_ctl.rs
- crates/snipdeck-app/src/controller/colour_main.rs
- crates/snipdeck-app/src/controller/eyedrop_ctl.rs
- crates/snipdeck-app/src/controller/screen_clip_ctl.rs
- crates/snipdeck-app/src/controller/symbol_ctl.rs
- crates/snipdeck-app/src/controller/tests/colour_main.rs
- crates/snipdeck-app/src/controller/tests/colours.rs
- crates/snipdeck-app/src/controller/tests/screen_clip.rs
- crates/snipdeck-app/src/controller/tests/symbols.rs
- crates/snipdeck-app/src/screen_clip.rs
- crates/snipdeck-app/src/symbols_binding.rs
- crates/snipdeck-app/src/symbols_panel.rs
- crates/snipdeck-app/tests/colours_window.rs
- crates/snipdeck-colour/Cargo.toml
- crates/snipdeck-colour/src/contrast.rs
- crates/snipdeck-colour/src/format.rs
- crates/snipdeck-colour/src/lib.rs
- crates/snipdeck-colour/src/model.rs
- crates/snipdeck-colour/src/names.rs
- crates/snipdeck-colour/src/parse.rs
- crates/snipdeck-colour/src/picker.rs
- crates/snipdeck-colour/src/recents.rs
- crates/snipdeck-colour/src/saved.rs
- crates/snipdeck-colour/src/scheme.rs
- crates/snipdeck-colour/tests/parsing.rs
- crates/snipdeck-colour/tests/properties.rs
- crates/snipdeck-colour/tests/saved_recents_picker.rs
- crates/snipdeck-colour/tests/vectors.rs
- crates/snipdeck-platform/src/eyedrop.rs
- crates/snipdeck-platform/src/windows/eyedrop.rs
- crates/snipdeck-platform/src/windows/screen.rs
- crates/snipdeck-symbols/Cargo.toml
- crates/snipdeck-symbols/src/art.rs
- crates/snipdeck-symbols/src/catalog.rs
- crates/snipdeck-symbols/src/data.rs
- crates/snipdeck-symbols/src/fold.rs
- crates/snipdeck-symbols/src/icon.rs
- crates/snipdeck-symbols/src/lib.rs
- crates/snipdeck-symbols/src/recents.rs
- crates/snipdeck-symbols/src/search.rs
- crates/snipdeck-symbols/tests/art.rs
- crates/snipdeck-symbols/tests/catalog.rs
- crates/snipdeck-symbols/tests/icon.rs
- crates/snipdeck-symbols/tests/lazy.rs
- crates/snipdeck-symbols/tests/recents.rs
- crates/snipdeck-symbols/tests/search.rs
- crates/snipdeck-ui/src/symbol_nav.rs
- crates/snipdeck-ui/tests/symbol_nav.rs
- crates/snipdeck-ui/tests/symbols.rs
- crates/snipdeck-ui/ui/colours.slint
- crates/snipdeck-ui/ui/components/colour_block.slint
- crates/snipdeck-ui/ui/components/swatch.slint
- crates/snipdeck-ui/ui/symbols.slint
- data/symbols/kaomoji.tsv
- scripts/symbols.lock
- scripts/symbols.py
- scripts/test_symbols.py
- scripts/ui_icons.py

### platform (148)

- Snipdeck.bat
- Test Snipdeck on Windows.bat
- crates/snipdeck-app/src/controller/tests/tray.rs
- crates/snipdeck-app/src/hotkey_health.rs
- crates/snipdeck-app/src/native_window.rs
- crates/snipdeck-app/src/platform.rs
- crates/snipdeck-app/src/popup_window.rs
- crates/snipdeck-app/src/shortcuts.rs
- crates/snipdeck-platform/Cargo.toml
- crates/snipdeck-platform/src/chord.rs
- crates/snipdeck-platform/src/command_line.rs
- crates/snipdeck-platform/src/error.rs
- crates/snipdeck-platform/src/file_dialog.rs
- crates/snipdeck-platform/src/instance_names.rs
- crates/snipdeck-platform/src/keys.rs
- crates/snipdeck-platform/src/lib.rs
- crates/snipdeck-platform/src/startup_task.rs
- crates/snipdeck-platform/src/system_look.rs
- crates/snipdeck-platform/src/testing.rs
- crates/snipdeck-platform/src/tray.rs
- crates/snipdeck-platform/src/win_logic.rs
- crates/snipdeck-platform/src/windows/autostart.rs
- crates/snipdeck-platform/src/windows/console.rs
- crates/snipdeck-platform/src/windows/dll_search.rs
- crates/snipdeck-platform/src/windows/dpi.rs
- crates/snipdeck-platform/src/windows/file_dialog.rs
- crates/snipdeck-platform/src/windows/hidden_window.rs
- crates/snipdeck-platform/src/windows/hotkey.rs
- crates/snipdeck-platform/src/windows/message_box.rs
- crates/snipdeck-platform/src/windows/mod.rs
- crates/snipdeck-platform/src/windows/package.rs
- crates/snipdeck-platform/src/windows/paint_diag.rs
- crates/snipdeck-platform/src/windows/shell_open.rs
- crates/snipdeck-platform/src/windows/single_instance.rs
- crates/snipdeck-platform/src/windows/system_look.rs
- crates/snipdeck-platform/src/windows/tray.rs
- crates/snipdeck-platform/src/windows/update_region.rs
- crates/snipdeck-platform/src/windows/window_style.rs
- packaging/inno/snipdeck.iss
- packaging/msix/.gitignore
- packaging/msix/AppxManifest.xml
- packaging/msix/README.md
- run-snipdeck-demo.bat
- scripts/installer_check.py
- scripts/make_msix_assets.py
- scripts/msix_manifest.py
- scripts/test_installer_check.py
- scripts/test_msix_manifest.py
- spikes/s1-noactivate/Cargo.lock
- spikes/s1-noactivate/Cargo.toml
- spikes/s1-noactivate/src/main.rs
- spikes/s1-noactivate/src/platform/mod.rs
- spikes/s1-noactivate/src/platform/stub.rs
- spikes/s1-noactivate/src/platform/windows.rs
- spikes/s2-ime/--cjk
- spikes/s2-ime/.gitignore
- spikes/s2-ime/Cargo.lock
- spikes/s2-ime/Cargo.toml
- spikes/s2-ime/build.rs
- spikes/s2-ime/fonts/IBMPlexSans-Medium.ttf
- spikes/s2-ime/fonts/IBMPlexSans-Regular.ttf
- spikes/s2-ime/fonts/OFL.txt
- spikes/s2-ime/src/main.rs
- spikes/s2-ime/ui/app.slint
- spikes/s3-longtext/Cargo.lock
- spikes/s3-longtext/Cargo.toml
- spikes/s3-longtext/build.rs
- spikes/s3-longtext/src/editor.rs
- spikes/s3-longtext/src/gen.rs
- spikes/s3-longtext/src/harness.rs
- spikes/s3-longtext/src/list.rs
- spikes/s3-longtext/src/main.rs
- spikes/s3-longtext/src/search.rs
- spikes/s3-longtext/src/viewer.rs
- spikes/s3-longtext/ui/bench.slint
- spikes/s4-mica/.gitignore
- spikes/s4-mica/Cargo.lock
- spikes/s4-mica/Cargo.toml
- spikes/s4-mica/build.rs
- spikes/s4-mica/src/appearance.rs
- spikes/s4-mica/src/main.rs
- spikes/s4-mica/src/platform/stub.rs
- spikes/s4-mica/src/platform/windows.rs
- spikes/s4-mica/ui/panel.slint
- tools/launcher/README.md
- tools/launcher/build.ps1
- tools/launcher/cli.ps1
- tools/launcher/commands/Back up my library.bat
- tools/launcher/commands/Build from source (developers only).bat
- tools/launcher/commands/Check my setup (doctor).bat
- tools/launcher/commands/Help (all commands).bat
- tools/launcher/commands/Open the logs.bat
- tools/launcher/commands/Reinstall Snipdeck (keep my data).bat
- tools/launcher/commands/Restore a backup.bat
- tools/launcher/commands/Start without rebuilding (after a pull).bat
- tools/launcher/commands/Uninstall Snipdeck (keep my data).bat
- tools/launcher/commands/Uninstall Snipdeck and erase my data.bat
- tools/launcher/commands/Update Snipdeck.bat
- tools/launcher/core.ps1
- tools/launcher/data.ps1
- tools/launcher/lib.ps1
- tools/launcher/release-sha256.txt
- tools/launcher/release.ps1
- tools/launcher/selftest.ps1
- tools/launcher/snipdeck.ps1
- tools/launcher/tests/accessdenied.log
- tools/launcher/tests/corrupt.log
- tools/launcher/tests/exelocked.log
- tools/launcher/tests/expect.txt
- tools/launcher/tests/filelock.log
- tools/launcher/tests/lockmismatch.log
- tools/launcher/tests/longpath.log
- tools/launcher/tests/msvc.log
- tools/launcher/tests/network.log
- tools/launcher/tests/no-toolchain.log
- tools/launcher/tests/nospace.log
- tools/launcher/tests/ok.log
- tools/launcher/tests/oom.log
- tools/launcher/tests/release.json
- tools/launcher/tests/revoke.log
- tools/launcher/tests/toolchain-old.log
- tools/windows-test/README.md
- tools/windows-test/assets/colour-test.html
- tools/windows-test/auto/lib.ps1
- tools/windows-test/auto/mock/primitives.mock.ps1
- tools/windows-test/auto/primitives.ps1
- tools/windows-test/auto/qc.ps1
- tools/windows-test/auto/s1.ps1
- tools/windows-test/auto/s2.ps1
- tools/windows-test/auto/s4.ps1
- tools/windows-test/auto/tests/selftest.ps1
- tools/windows-test/diag-render.ps1
- tools/windows-test/flow.ps1
- tools/windows-test/m2-harness/Cargo.lock
- tools/windows-test/m2-harness/Cargo.toml
- tools/windows-test/m2-harness/src/app.rs
- tools/windows-test/m2-harness/src/items.rs
- tools/windows-test/m2-harness/src/main.rs
- tools/windows-test/m2-harness/src/sys.rs
- tools/windows-test/run-tests.ps1
- tools/windows-test/run-tests.sh
- tools/windows-test/tests/colour.json
- tools/windows-test/tests/m2-platform.json
- tools/windows-test/tests/m3-app.json
- tools/windows-test/tests/quick-check.json
- tools/windows-test/tests/s1-noactivate.json
- tools/windows-test/tests/s2-ime.json
- tools/windows-test/tests/s4-mica.json

### ux (49)

- crates/snipdeck-ui/tests/audit_final1.rs
- crates/snipdeck-ui/tests/audit_final1_keys.rs
- crates/snipdeck-ui/tests/audit_final1_tree.rs
- crates/snipdeck-ui/tests/button_logic.rs
- crates/snipdeck-ui/tests/buttons.rs
- crates/snipdeck-ui/tests/focus_traversal_repro.rs
- crates/snipdeck-ui/tests/inputs.rs
- crates/snipdeck-ui/tests/inputs_logic.rs
- crates/snipdeck-ui/tests/placeholder_align_logic.rs
- crates/snipdeck-ui/tests/primitives.rs
- crates/snipdeck-ui/tests/primitives_logic.rs
- crates/snipdeck-ui/tests/primitives_pixels.rs
- crates/snipdeck-ui/tests/review_fixes_logic.rs
- crates/snipdeck-ui/tests/review_fixes_pixels.rs
- crates/snipdeck-ui/tests/ux_fit.rs
- crates/snipdeck-ui/ui/components.slint
- crates/snipdeck-ui/ui/components/button.slint
- crates/snipdeck-ui/ui/components/card-depth.slint
- crates/snipdeck-ui/ui/components/choice.slint
- crates/snipdeck-ui/ui/components/chooser.slint
- crates/snipdeck-ui/ui/components/dialog.slint
- crates/snipdeck-ui/ui/components/empty_state.slint
- crates/snipdeck-ui/ui/components/feedback.slint
- crates/snipdeck-ui/ui/components/form.slint
- crates/snipdeck-ui/ui/components/group-header.slint
- crates/snipdeck-ui/ui/components/hotkey_health.slint
- crates/snipdeck-ui/ui/components/hotkey_recorder.slint
- crates/snipdeck-ui/ui/components/icon-button.slint
- crates/snipdeck-ui/ui/components/icon.slint
- crates/snipdeck-ui/ui/components/item-card.slint
- crates/snipdeck-ui/ui/components/keycap.slint
- crates/snipdeck-ui/ui/components/label-chip.slint
- crates/snipdeck-ui/ui/components/menu.slint
- crates/snipdeck-ui/ui/components/scrollbar.slint
- crates/snipdeck-ui/ui/components/shortcuts-sheet.slint
- crates/snipdeck-ui/ui/components/sidebar-item.slint
- crates/snipdeck-ui/ui/components/state-color.slint
- crates/snipdeck-ui/ui/components/tag.slint
- crates/snipdeck-ui/ui/components/text_field.slint
- crates/snipdeck-ui/ui/components/tooltip.slint
- crates/snipdeck-ui/ui/gallery/buttons.slint
- crates/snipdeck-ui/ui/gallery/focus_probe.slint
- crates/snipdeck-ui/ui/gallery/forms.slint
- crates/snipdeck-ui/ui/gallery/inputs.slint
- crates/snipdeck-ui/ui/gallery/primitives.slint
- crates/snipdeck-ui/ui/gallery/review_probe.slint
- crates/snipdeck-ui/ui/gallery/scroll.slint
- crates/snipdeck-ui/ui/lib.slint
- crates/snipdeck-ui/ui/theme.slint

### perf (8)

- crates/snipdeck-app/src/index_cache.rs
- crates/snipdeck-app/src/repaint.rs
- crates/snipdeck-app/tests/repaint_hook.rs
- crates/snipdeck-ui/src/damage.rs
- crates/snipdeck-ui/tests/colours_scroll_logic.rs
- crates/snipdeck-ui/tests/focus_scroll_logic.rs
- crates/snipdeck-ui/tests/repaint.rs
- crates/snipdeck-ui/tests/scrollbar.rs


## Part 2: extras audit

Method: every claim is marked `run` (a command or test executed here, number given) or `code` (read only).
Analysis scripts are beside this file in `work/` and can be rerun: `po.py` (catalogue parser and checks),
`term.py` (term tallies), `half2.py` (Slint callbacks, properties, globals with no Rust side), `dead.py` (public
Rust with no production caller), `settings.py` (settings fields with no reader). One probe test was written
(`crates/snipdeck-ui/tests/probe_extras.rs`), never got the shared cargo lock, and was deleted unrun.

### 1. Coverage table

| Row | State | How |
|---|---|---|
| `crash.rs` (hook, report, prune, unacknowledged, open folder) | checked | code |
| `trace.rs` (`SNIPDECK_LOG`), `diag.rs` (`SNIPDECK_DIAG`) | checked (trace), skimmed (diag paint hooks are Windows-only) | code |
| Every `eprintln!` site (29 in production code, 24 in `main.rs`) | checked: where each goes in a release build | code |
| `cli.rs` all 9 flags, `main.rs` `main`, `headless`, `run` to line 260, `open_library`, `report_startup_failure` | checked | code |
| `main.rs` lines 260 to 870 (window wiring, tray, timers) | not reached (owned in practice by panel, main, platform) | - |
| `headless.rs` (`--check-db`, `--troubleshoot`) | checked | code |
| `doctor.rs` (7 checks, report) | checked for wording and privacy; check logic not driven | code |
| `paths.rs` (`data_dir`, `saves_settings`, load outcomes) | checked; salvage logic not driven (data agent) | code |
| `starter.rs`, `wording.rs`, `commands.rs` `caps`/`spoken` | checked for translation plumbing | code |
| `failure.rs`, `ownership.rs`, `host.rs`, `tips.rs`, `tour.rs`, `window_kind.rs`, `demo.rs`, `screenshot.rs` | not reached (time box); `host.rs` test-only accessors listed by `dead.py` | - |
| `crates/snipdeck-ui/build.rs`, `crates/snipdeck-app/build.rs` | checked | code |
| `scripts/i18n.py` | checked, `check --strict` run: exit 0 | run |
| `scripts/third_party_notices.py --check`, `codemap.py --check`, `symbols.py --check`, `installer_check.py` | run: all pass | run |
| `scripts/test_i18n.py`, `test_symbols.py`, `test_installer_check.py`, `test_msix_manifest.py`, `test_bump_version.py` | run: all `OK` | run |
| `scripts/gate.sh`, `orient.sh` | checked | code |
| `scripts/bump_version.py` version sites | checked | code |
| `scripts/readme_shots.py`, `make_logo.py`, `tips_images.py`, `design/tools/palette.py` | not reached | - |
| `.github/workflows/ci.yml`, `audit.yml`, `release.yml` (build, zip, MSIX stage), `release-build.yml`, `dependabot.yml` | checked | code |
| `LICENSE`, `THIRD_PARTY_NOTICES.md` (sections and what ships), `deny.toml`, `Cargo.toml` profiles | checked | code + run |
| `.pot` and six `.po` files: 1,337 entries each, 27 plural entries | checked by parser: completeness, fuzzy, obsolete, placeholders (all `{...}`), edge spaces, newlines, end punctuation, doubled words, dashes, half-width punctuation, plural header, length ratio, 14 core terms | run |
| Translation fit on screen | not reached: no window was rendered per locale; candidates listed (EXTRAS-15) | code |
| Language selection at start-up | checked against Slint 1.18.1 source; probe written, not run (cargo lock held by another agent) | code (see EXTRAS-1) |
| App-wide half-built scan: 173 Slint components, 30 globals, 55 settings fields, all `pub` Rust items | checked by script, hits verified by grep | run |
| `site/` | links and metadata checked; not rendered | code |
| `TODO`/`FIXME`/`todo!`/`unimplemented!`/`allow(dead_code)` | run: none in production code; `dead_code` allowances are test fixtures and three Windows-only functions | run |

### 2. Findings

| Id | Sev | Lens | Evidence | Verified | Fix for the class | Effort |
|---|---|---|---|---|---|---|
| EXTRAS-1 | high | Broken, i18n | The saved Language is never applied. `main.rs:247` calls `slint::select_bundled_translation(&settings.language)` before the first Slint component exists (`PanelWindow::new()` is line 250). Slint registers the bundled catalogues only inside `X::new()` (`i-slint-compiler-1.18.1/generator/rust.rs:435,547` emits `sp::set_bundled_languages`), so the call returns `NoTranslationsBundled` (`i-slint-core-1.18.1/translations.rs:431-434`), the error goes to `eprintln!` (invisible in the release build), and `set_bundled_languages` then picks the Windows locale (`translations.rs:388-393`). Result: Settings > Language ("Takes effect the next time Snipdeck starts") changes nothing, ever; the UI always follows Windows. Rust-side `Locale::for_tag(&settings.language)` (`controller.rs:619`, `use_ctl.rs:540`) does use the saved tag, so template dates switch language while the UI does not. No test runs the real start-up order (`crates/snipdeck-ui/tests/i18n.rs` selects after a component exists). | code (Slint 1.18.1 source read line by line); the probe test was written but never ran: the shared cargo lock was held by another agent for the rest of the time box | Apply the language in one function called after the first component is created, and skip the call for `""` (calling it with `""` after registration forces English, `translations.rs:443-445`, which would break "Follow Windows"; `report_startup_failure` `main.rs:143` has this latent bug: after a window exists it shows the message in English, not the system language). Add an app-level test that runs the real start-up order with a saved tag different from the process locale. Treat a failed selection as a toast, not a lost stderr line. | S |
| EXTRAS-2 | high | Broken (process) | CI does not run most of the gate. `ci.yml` runs fmt, clippy, test, doc, deny. Only `scripts/gate.sh` runs: `i18n.py check --strict`, `test_i18n.py`, `codemap.py --check`, `symbols.py --check`, `test_symbols.py`, `installer_check.py` and its tests, `test_msix_manifest.py`, `test_bump_version.py`, `third_party_notices.py --check` (`grep python .github/workflows/*.yml`: only `bump_version`, `make_msix_assets`, `msix_manifest` in `release.yml`). A PR, or the weekly Dependabot cargo bump (`dependabot.yml`), merges with stale notices, untranslated strings or a broken installer script and stays green. Second gap: the Windows job runs `cargo test -p snipdeck-core -p snipdeck-store -p snipdeck-portable` only, so the 35 `#[test]`s under `crates/snipdeck-platform/src/windows/` (clipboard 8, dpapi 4, eyedrop 5, hotkey 4, system_look 4, dpi 3, hidden_window 3, tray 3, keyboard 1) never run anywhere: Linux cannot compile them. The comment at `ci.yml:57-58` says DPAPI is exercised; it is not. | code (workflow files), run (counts) | One CI job that runs the same script the gate runs (split `gate.sh` so its non-cargo block is `scripts/checks.sh`, called by both). Add `-p snipdeck-platform` (and the app crate's non-window tests) to the Windows job. A gate check that fails when a `scripts/test_*.py` exists that neither CI nor the gate names. | S |
| EXTRAS-3 | high | Licence | (a) Five IBM Plex TTFs are compiled into the exe (`crates/snipdeck-ui/fonts/`), under SIL OFL 1.1, which requires the copyright notice and licence in every copy. `THIRD_PARTY_NOTICES.md` (the file that ships) has no Plex or OFL section: `grep -c "Plex\|Open Font" THIRD_PARTY_NOTICES.md` = 0. `OFL.txt` exists only in the source tree. (b) The MSIX stage holds `snipdeck.exe`, `Assets`, the manifest and nothing else (`release.yml:309-313`): no `LICENSE`, no notices, so the Store package ships GPL-3.0 code and MIT, Unicode-3.0 and OFL material with no licence text, and About hides its "third-party notices" row there (`help_binding.rs:149-156,408`), while the doc comment at `help_binding.rs:154` says the Store package has the file. HANDOVER owner step 4 mentions copying the notices by hand: still open, re-verified. (c) The zip step copies the files with `-ErrorAction SilentlyContinue` (`release.yml:278`), so a missing notices file ships silently. | run (grep), code | Make the notices impossible to omit: generate a "bundled assets" section (fonts, icons, emoji art, CLDR) from a small manifest next to `symbols.lock`, checked by `third_party_notices.py --check`; embed `LICENSE` and the notices in the exe with `include_str!` and show them in Help > About, so every package has them without a copy step. Fail the release step if a listed file is absent. | M |
| EXTRAS-4 | medium | i18n, Accessibility | SPEC line 167 says all strings go through `@tr`. Rust still puts English on screen in every locale: `doctor.rs` Troubleshoot (7 labels `LABELS`, about 20 detail sentences such as `doctor.rs:204,213,219`, status words "OK", "Warning", "Problem" that the screen reader speaks, `doctor.rs:48-54`; its header says "literal English, like the toasts", but the toasts are translated now); `starter.rs:13-49` first-run content (board "My snippets", lists "Snippets" and "Getting started", five teaching cards, default names "New list" and "New board"); `wording.rs:59-99` the 32 list-icon and label-colour names used as accessible names (`main_binding.rs:1523`); `commands.rs:272-286` spoken shortcuts ("Control Slash", "Delete", "Escape"); the reason inside the start-up failure box (`main.rs:250` "creating the Panel: ..."). | code | Move each to a `*Texts` global as `ToastTexts` did, and add a test that fails when a production Rust file passes a sentence literal to a Slint setter or model (a scan like `work/dead.py`), so the class cannot come back. | M |
| EXTRAS-5 | medium | Copy, i18n | Terminology drifts inside each locale; measured over all 1,337 entries with `term.py` (source strings that contain the English term, grouped by the word used). "item": de Textbaustein 95, Eintrag 23, Element 14; fr extrait 91, élément 43; es elemento 88, fragmento 45; pt-BR item 108, trecho 28; ja スニペット 75, 項目 20, アイテム 12; zh-Hans 片段 84, 项 45. "trigger": de Kürzel 16, Auslöser 7; fr raccourci 15, déclencheur 8; es atajo 15, activador 7, and in fr and es the same word (raccourci, atajo) is also the only word for "shortcut" (24 each), so "trigger" and "keyboard shortcut" cannot be told apart. "Sensitive": de vertraulich 24, sensibel 3; es sensible 21, confidencial 6; fr sensible 25, confidentiel 2 (and the nav label is "Confidentiel"/"Confidencial"). "Settings": es Configuración 21, Ajustes 6. "Recent": de Zuletzt 15 against the nav label "Zuletzt verwendet"; es Recientes 14, Reciente 4; fr Récents 11, Récent 4. "pin": zh-Hans 置顶 14, 固定 3. "Trash": ja ゴミ箱 16, ごみ箱 3. Key names: de Ctrl 11, Strg 11; Enter 10, Eingabe 5; Shift 6, Umschalt 2; es Enter 6, Intro 8; fr Enter 5, Entrée 7, while the keycaps drawn from Rust are always English. The English source has the same split: 151 msgids say "item", 31 say "snippet"; "the Panel" 25 times and "the panel" in Settings; `shortcut_texts.slint:19` "Show or hide the Panel" against `form_texts.slint:148` "Show or hide the panel". | run | A glossary file (one row per term, one chosen word per locale) and an `i18n.py check` rule that fails when a source string containing the term has a translation without the chosen word. Fix the English source first (pick "item" or "snippet", "Panel" capitalised), then the six locales. Decide once whether key names are localised and apply it to the Rust keycaps too. | M |
| EXTRAS-6 | medium | Logs | There is no log file. All 29 production `eprintln!` sites write to stderr, which a release Windows GUI build does not have (`main.rs:91-93` only borrows a parent console when one exists). So these never reach the user or a bug report: language selection failed (`main.rs:248`), hotkey could not be registered (`main.rs:871`), tray update failed (`main.rs:70`), recovered settings not written (`main.rs:919`), settings file unreadable and nothing will be saved (`main.rs:929`), first board not created (`main.rs:943`), host warnings (`main.rs:243`). `SNIPDECK_LOG=1` also goes to stderr only (`trace.rs:44-56`) and works only through the launcher. The Troubleshoot report cannot include any of it. | code | One sink: a small rotating `logs/snipdeck.log` in the data folder (same no-content rule as `trace.rs`, a size cap, two files), every warning written through one `warn!` macro instead of `eprintln!`, last N lines added to "Copy report". "Open the logs" already opens that folder. | M |
| EXTRAS-7 | medium | Logs, crash hook | Crash reports from shipped builds will carry almost nothing. `[profile.release]` has `strip = true`, `debug = false` (`Cargo.toml:120-125`), so `Backtrace::force_capture()` (`crash.rs:233`) has no symbols; `release.yml` keeps no PDB. `message_of` (`crash.rs:212-220`) drops every `String` payload, which is what `unwrap`, `expect` on a `Result`, index and slice panics produce, so the usual report is one `file:line` and "(message omitted, N bytes)". Also: a panic between `crash::install` (`main.rs:88`) and `set_data_dir` (`main.rs:911`) writes no file; aborts, stack overflow and access violations in FFI are not covered; a worker-thread panic is only reported at the next start (`maintenance.rs:344`). | code; not run on Windows | Build the shipped exe with `debug = "line-tables-only"` and `split-debuginfo = "packed"`, keep the PDB as a release artifact (not in the installer) and print module-relative addresses, or ship the existing `release-debug` profile. For `String` payloads keep the part before the first `: ` when it comes from an `expect("literal")`. Windows run: force a panic in a release build and read the file. | S |
| EXTRAS-8 | medium | Broken, data safety | `--check-db` and `--troubleshoot` are diagnostics that write. `headless.rs:41-43` uses `Store::open`, which creates the folder and an empty database (`store.rs:142-144`) and migrates an older schema; `headless.rs:160` calls `paths::read_settings`, which renames a damaged `settings.json` to `.bad` (`paths.rs:139`). Both run before the single-instance check (`main.rs:108-110`), so they do this next to a running Snipdeck, which `main.rs:226-228` says a second launch must never do. A mistyped `--data-dir` creates a library there and prints "ok: the integrity check found no problems", exit 0. The store has no read-only open at all (`grep read_only crates/snipdeck-store/src`: none). | code | Add `Store::open_read_only` (no create, no migrate, `SQLITE_OPEN_READ_ONLY`) and a settings read that never sets aside; use both in every headless path. A missing library is exit 2 "no library in <folder>". | S |
| EXTRAS-9 | medium | Missing (half-built) | Found by `half2.py`, `dead.py`, each checked by grep. Slint state nothing drives: `PanelWindow.loading` and `MainWindow.loading` (skeleton rows, `panel.slint:280,2594`, `main_window.slint:310,2322`) have no `set_loading` in production Rust, so the loading state never shows. About's build date row (`help.slint:425`) needs `SNIPDECK_BUILD_DATE` (`help_binding.rs:290`), which no build script or workflow sets, so it never shows. `STORE_URL = None` (`help_binding.rs:58`) keeps "Open the Microsoft Store page" hidden. `ColourTexts.no-name` (`colours.slint:55`, "no close name") is never called. `Radio` (`choice.slint`) is never instantiated. Unused tokens: `Type.xxl`, `Type.line-xl`, `Theme.search-mark-text`, `Theme.success-bg`, `Theme.warning-bg`, `Theme.danger-bg`, `CardDepth.lift`. Rust with no caller: `main_window::trash_entry` (`main_window.rs:899`), `history::next_undo_id` (`history.rs:303`), `host::trigger_lists` (`host.rs:330`), `tray::is_wanted` (`tray.rs:409`), `windows/clipboard.rs:193 format_count`, `snipdeck_core::APP_NAME`. Controller methods only tests call, so the tests cover a path the app does not take: `picker_use` (`use_ctl.rs:143`), `submit_fill` (`use_ctl.rs:258`), `drop_create` (`create_ctl.rs:110`), `main_show_panel` (`main_ctl/lists.rs:322`). Limits written twice and never fed from Rust: `trash-days: 30` in two windows against `TRASH_DAYS`, and seven `SettingsPage` min/max defaults against `settings.rs:13-28,214-216` (equal today). No `.slint` callback on a Rust-created window lacks an `on_` handler (0 hits). | run (scripts), code | Put the two scans in the gate (`scripts/halfbuilt.py --check` with an allow-list that needs a reason), delete what is dead, set the build date in `build.rs` from `SOURCE_DATE_EPOCH` or the release workflow, and set the Slint limits from the Rust constants at window creation. | M |
| EXTRAS-10 | low | i18n tooling, Copy | `scripts/i18n.py` holes: `PLACEHOLDER = \{\d*\}` (line 41) ignores `{n}`, used by 12 source strings, so a translation that drops `{n}` passes; the extractor is line-based (no multi-line `@tr` today, 0 hits); nothing flags a translation identical to its source, wrong end punctuation or glossary drift. Catalogue defects found by `po.py`: em dash in the source `suggest_hint.slint:39` "{} — press Tab to insert" and its pt-BR, ja and zh-Hans translations (house rule: no em dashes); ja and zh-Hans use half-width "?" after CJK text in 5 strings (for example "ボード「{}」を削除しますか?"); ja "このテンプレートを読めません: {}" drops the final "。"; source variants "Nothing matches “{}”" with and without a full stop, "Saved {}" and "Saved {}."; "Tag" (singular) is translated plural in es, de, fr, pt-BR; 107 source strings use a straight apostrophe while quotes are curly. Slint matches the system locale by base language, so zh-TW and zh-HK get Simplified Chinese and pt-PT gets Brazilian Portuguese. All six locales are otherwise complete: 0 missing, 0 extra, 0 fuzzy, 0 empty, 0 obsolete, 0 placeholder or newline mismatches, plural headers correct (fr and pt-BR `n > 1`, ja and zh one form). | run | Widen the placeholder regex to `\{[^}]*\}`, add checks for dashes, half-width punctuation in CJK, end punctuation parity and identical-to-source (with an allow-list for "Emoji", "Panel"), and fix the listed strings. | S |
| EXTRAS-11 | low | UX, Missing (CLI) | No `--version`. A bad argument prints the usage to stderr and exits 2 (`main.rs:94-100`); in the installed GUI build that is invisible (only `run` failures get a message box), so a shortcut with a typo, or a file passed by "Open with", starts nothing and says nothing. `--dark` silently stops every settings change of that run from being saved (`paths.rs:172-174`), while the usage only says "use the dark theme". `--data-dir --hidden` takes `--hidden` as the folder. The usage says `snipdeck-app`; the installed program is `snipdeck.exe`. No headless export, backup or import, which the launcher's "Back up my library" and scheduled tasks would use. | code | Show argument errors in the same message box as start-up failures; add `--version`, `--backup <file>`; make `--dark` override only the theme field in the saver; reject a value that starts with `--`. | S |
| EXTRAS-12 | low | Missing | Against Ditto, CopyQ and Beeftext: no portable mode (the "portable zip" keeps data in `%LOCALAPPDATA%\Snipdeck`, `paths.rs:27-31`; only `--data-dir` moves it); Trash retention is fixed at 30 days (`main_window.rs:25`), not a setting; the language needs a restart (and see EXTRAS-1). | code | A `portable` marker file beside the exe selects `.\data`; a Trash days setting; apply the language live (Slint re-evaluates `@tr` on `select_bundled_translation`). | M |
| EXTRAS-13 | low | UX, unowned | `site/README.txt` says the site is published by a Pages workflow; `.github/workflows/` has none (ci, audit, secrets, release, release-build), so the site is never deployed and its screenshots (`site/img`, copied from `docs/images`) have no drift check. The installer and zip ship `README.md`, `CHANGELOG.md`, `user-guide.md` and `THIRD_PARTY_NOTICES.md`; a stock Windows has no program for `.md`, so About's "Open the third-party notices" (`help_binding.rs:679`) is likely to end in "How do you want to open this file?". | code; Windows run must confirm | Add the Pages workflow or delete the claim; ship the notices as `.txt` or show them in-app (see EXTRAS-3). | S |
| EXTRAS-14 | low | Build | The exe's version resource sets ProductName, FileDescription, OriginalFilename, InternalName only (`crates/snipdeck-app/build.rs:36-41`): no CompanyName or LegalCopyright, which Explorer's Details tab and SmartScreen show. `rust-toolchain.toml` is `channel = "stable"` and the release job installs "stable" (`release.yml:209`), so a release is built with whatever stable exists that day, not the toolchain the tag was tested with; `rust-version = "1.97"` is not enforced by any job. | code | Add the two resource strings; pin the release toolchain to an exact version recorded in the repo and bumped by Dependabot-style PRs. | S |
| EXTRAS-15 | low | i18n (fit) | Not measured on screen. Longest translations of short labels, by display width ratio: "Trimmed" es "Sin espacios en los extremos" 4.0x, fr 3.9x, de 3.0x; "Hover" es "Pasar el puntero" 3.2x; "Backed up" es 3.1x; "Ungroup" de "Gruppierung aufheben" 2.9x; "Recent" de "Zuletzt verwendet" 2.8x (a sidebar and tab label); "Hover delay" de "Verzögerung beim Darüberfahren" 2.7x; "Most used" de 2.6x; "Paste as…" ja 2.6x; "Tray icon click opens" fr 2.5x. Counts of candidates: fr 32, de 29, es 17, ja 10, pt-BR 6, zh-Hans 1 (full list in `work/po.out`). Only `settings_window.rs:1832` renders a window in every locale; no test renders Panel, Main, picker or editors in a non-English locale at minimum width. | run (ratios), not rendered | A golden-free fit test: for each locale and each window at minimum width and text scale 1.0 and 2.25, assert through the accessibility tree that no text element is wider than its parent. The ux agent should measure these labels. | M |

### 3. Not verified, and what a Windows run must check

- EXTRAS-1 on Windows: set Windows to English, choose Deutsch in Settings, restart: the UI should stay English
  (bug). After the fix, also check "Follow Windows" on a German Windows.
- EXTRAS-7: force a panic in a release build (`strip = true`) and read `logs/crash-*.txt`: expect no usable
  backtrace. Check what a panic inside an `extern "system"` callback writes (it aborts; likely nothing).
- EXTRAS-3 and EXTRAS-13: install from Setup and from the MSIX; check which licence files exist beside the exe,
  whether Help > About shows the notices row, and what opening a `.md` does on a clean PC.
- EXTRAS-6: start the installed build with a hotkey taken by another program and with a read-only
  `settings.json`; confirm that nothing but a toast records it.
- EXTRAS-11: start `snipdeck.exe --wat` from a shortcut: expect no window and no message.
- The 35 Windows-only platform tests (EXTRAS-2) have never run in CI; run `cargo test -p snipdeck-platform` on
  Windows once and record the result.
- Translation quality itself (meaning, tone) was not judged; HANDOVER already lists native-speaker review.
- Not reached in the time box: `failure.rs`, `ownership.rs`, `host.rs`, `tips.rs`, `tour.rs`, `demo.rs`,
  `screenshot.rs`, `readme_shots.py`, `make_logo.py`, `tips_images.py`, `palette.py`, `main.rs` 260-870, and
  on-screen fit per locale.
