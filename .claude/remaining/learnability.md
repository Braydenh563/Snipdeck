# learnability agent

## Done (uncommitted until the first build passes)
- command_texts.slint, components/command-palette.slint, Main window palette (Ctrl+K, Ctrl+Shift+P, toolbar button)
- commands.rs (catalogue, filter, caps), command_binding.rs, controller/commands_ctl.rs, Effect::OpenCommands
- Help "Shortcuts" tab (components/shortcuts-sheet.slint), Ctrl+/, Ctrl+, in Main; HelpTopic::Shortcuts
- Panel: Ctrl+K, Ctrl+/, kebab "All commands…" + "Keyboard shortcuts", shortcuts in kebab/item menu, footer Picker hint
- Main empty-state hint

## Next
- first build + tests; i18n extract/check; clippy 1.99; commits
- tray "All commands…" (only if simple), docs note

## Blocked
- nothing (cargo lock waits on other agents)
