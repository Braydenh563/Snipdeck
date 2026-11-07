# ux-quick remaining work

Done (commits on wt/ux-quick): 18 tray Open Picker; 2 Recent paste/copy in the Panel (Main window Recent stays copy-only: it has no Target);
5 chips insert at caret; 11 lock badges; 9 onboarding Recent + dock choice (code default Floating kept, SPEC 5.1 says docked right);
15 done: copy cleanup + Settings search (SettingsFilter global, Rust `matches`, rows and sections hide, "No settings match", Jump-to-section dropdown; search matches row titles only, not descriptions; goldens settings-search-*). 3 done: core + UI (HotkeyHealthLine in Settings Shortcuts and onboarding step 2: status, Test/Cancel, one-click alternatives; goldens settings-hotkey-health-light, onboarding-shortcuts-health-light).

Next:
- Translations for new strings (hotkey health line, Search settings, Jump to a section, No settings match, Clear the search; plus Open Picker tray, Supporter, onboarding copy): python3 scripts/i18n.py todo LOCALE / apply.

Blocked: none. Windows-only: real hotkey registration/delivery, tray menu item, paste from Recent into a real Target.

Note: cargo +1.99.1 clippy not run for items 3/15 (the ui crate needs ~9 GB to compile; two agents on one machine get OOM-killed): run it before the push.
