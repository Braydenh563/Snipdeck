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

## Next
- Run fmt, clippy, doc, full tests, i18n check, codemap; commit.

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
