# ux-a (Sonnet): settings sweep U2, U4, U19, U11, U13, U20, U3 (settings half), U14 (settings half)

Branch `claude/v1-fable-uxa` (worktree /tmp/fable-uxa). Not pushed.

## Done (uncommitted until the first commit below)
- U13 settings.slint order: Shortcuts, Appearance, Panel (window behaviour + dock), Privacy (Recent, clipboard clear, master password), Programs, Licence, Backups and your data. No "About" section exists in Settings (About is in the Help window), so none was added.
- U2 text size: `TextSize` + `Settings.text_size`, row in Appearance, `text_scale.rs` (process-wide factor applied by every window's render), Windows registry read in the platform crate.
- U4 Paste next registered with Supporter only (`Hotkeys::registrable`).
- U19 Capture default chord empty; docs aligned.
- U11 `Settings.programs: Vec<ProgramRule>`; old `no_paste_apps` migrated on load; Programs section.
- U20 Export asks for a folder (`SettingsHost::pick_export_folder`, Windows folder picker; Documents when no dialog).
- U3 settings half: "Last backup: <date>" / "No backup yet".
- U14 settings half: F1 + "Help" button in Settings.

## Queue (scope cut by the lead; nothing below is started except where noted)
- Verify the ui tests I edited: `cargo test -p snipdeck-ui --test editors --test editors_logic --test i18n` (fixtures moved to `paste_rules`; new tests `each_program_has_a_paste_key...`, `the_text_size_row_reports...`, `the_backups_section_says...`, `help_is_one_button_or_f1_away`). Not run to the end: the shared disk filled during the build. Edits are written; expect small fixes at most.
- Settings goldens (`settings-demo-light`, `settings-conflict-narrow-light`, any dark/recording ones in `tests/editors.rs`) changed (new Text size row, Programs table, section order). Regenerate with the repo's golden update command and review the diff; pixel counts not measured.
- Run clippy `-D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc`, `cargo test --workspace --locked -j 2` (fails today at `deck_logic::deck_behaviour` "nothing is hovered or focused": item-card/kebab, not touched here, its test changed in the hub tip 761d12e), then `python3 scripts/codemap.py`.
- F1 in Settings needs keyboard focus inside the window (test Tabs first). If a freshly opened Settings has no focus, add `forward-focus: scope;` on the window in settings.slint and measure with the running app.
- Optional: add a "Text size" line to the Help window diagnostics (`help_binding.rs`).

## Blocked (controller.rs)
Per-program paste keys are stored, shown and tested up to `PanelModel::set_paste_rules` (src/panel.rs) but the
controller does not call it yet, so the built-in table still decides the key (Never paste already works through the
derived `Settings.no_paste_apps`). Apply this exact patch to `crates/snipdeck-app/src/controller.rs`:

1. after `model.set_policy(policy_of(&settings));` (line ~672, in `Controller::new`) add
   `model.set_paste_rules(settings.paste_rules());`
2. after `self.model.set_policy(policy_of(&self.settings));` (line ~1084, in `update_settings`) add
   `self.model.set_paste_rules(self.settings.paste_rules());`
3. Optional clean-up once 1 and 2 are in: `policy_of` may read the "never" rules from `settings.programs`
   instead of the derived mirror `Settings.no_paste_apps`, and the mirror (settings.rs, `sanitize_programs`,
   `fold_legacy_no_paste`'s output) can then go; `route::decide` (no rules, built-in table) has no caller left.
   Test to add in controller tests: `update_settings(|s| s.programs.push(rule("wt.exe", ProgramPaste::CtrlShiftV)))`
   then activate a row with a `wt.exe` Target and expect Ctrl+Shift+V in `services().keyboard.sent`.
