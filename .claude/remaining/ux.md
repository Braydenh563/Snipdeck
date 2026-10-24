# agent-ux remaining work (step 13 owner findings)

## Done (written, see commits; build/test status in the last commit message)
- item 1: RegularWindow::create around X::new(), window_kind::plan, hook uses it (+ trace line)
- item 4: window + taskbar icon from resource 1 in the hook (Windows, unverified)
- item 5: editors sized to content (wanted-height), set_size on open
- item 6: Window behaviour group, panel_in_taskbar/panel_hide_on_blur, panel_ex_style, set_panel_taskbar, on_foreground_changed (400 ms grace)
- items 2/3: regression test for +, Tag manager bound (rename/delete via controller), Main window close handler (A2)
- item 9: "Save the clipboard as an item" label, sensitive help no longer promises a 30 s clear
- tier A: A8 second launch, A23 Open backups folder, audit 1 empty-state glyphs, audit 5 "Move to Trash"/danger Empty Trash

## Next
- run the gate, regenerate legitimately changed goldens (settings-*, empty-*, main-*), codemap

## Blocked / skipped
- item 7 (first-hide toast): depends on the picker branch's toast window; skipped here.
- audit 9 menu 2px ring: highlight doubles as hover fill, design call needed (skipped).
