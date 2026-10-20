# hub-small
## Done
- Step 1: merged uxb (c3f59e4) then uxa (622d930; uxa tip ef1dac9 was 21 min old); conflicts resolved (main_ctl hunks moved to main_ctl/items.rs + lists.rs, toast_texts indices 65-68 / VARIANTS 69, item-card Copy+Use buttons with more-reserve covering both, deck_logic takes uxb test, user-guide merged). i18n extract + codemap run.
- Step 2 (uncommitted at time of writing): app_menu_text 4th arg None; taskbar: None in settings_window, picker_window, panel_window, fill_window Monitor literals.
- Step 3 (uncommitted): set_paste_rules in Controller::new and update_settings; test a_program_rule_in_settings_decides_the_paste_key in controller/tests/use_paste.rs.
## Next
- Build/test (never compiled after the merges), goldens, step 5 items, step 6, step 7.
## Blocked
- Disk full (1.3G free, other agents' target dirs /tmp/target-m7, -o52, -m7-o53 are in use): cargo failed with ENOSPC on accesskit_consumer. Nothing compiled since the merge.
