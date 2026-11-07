# ux-quick remaining work

Done (commits on wt/ux-quick): 18 tray Open Picker; 2 Recent paste/copy in the Panel (Main window Recent stays copy-only: it has no Target);
5 chips insert at caret; 11 lock badges; 9 onboarding Recent + dock choice (code default Floating kept, SPEC 5.1 says docked right);
15 copy cleanup only; 3 core only (hotkey_health.rs, Controller::hotkey_health / start_hotkey_test, swallowed test press, alternatives pool).

Next:
- 3 UI: HotkeySetting gets status/alternatives fields; "Test" + alternative buttons in settings.slint and onboarding.slint step 2; bind in settings_binding.rs / onboarding_binding.rs (redraw after on_hotkey); one-click alternative = existing hotkey_recorded path; goldens.
- 15 search: global SettingsFilter with a Rust `matches` callback; SettingRow hides when no match (min/max-height 0); section jump links via page.reveal; empty section headers and a "no match" note still need a solution (Slint has no string contains).
- Translations for new strings (Open Picker tray, Supporter, onboarding copy): python3 scripts/i18n.py todo LOCALE / apply.

Blocked: none. Windows-only: real hotkey registration/delivery, tray menu item, paste from Recent into a real Target.
