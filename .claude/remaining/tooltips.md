# tooltips (claude/scroll-tooltips)
Done: TipHost global + TipLayer (components/tip-host.slint); IconButton/Button publish to it; TipLayer last in Main, Panel, Settings, TemplateFill, Item/List/Board/Tag editors; Main's own Tooltip moved after the overlays; tests/tooltips.rs; CHANGELOG, README D26, DESIGN.
Next: run tooltips + existing tooltip tests (panel, main_window_logic, editors_logic, panel_logic), cargo fmt, i18n n/a.
Blocked: out-of-window tooltips (winit 1.18.1 popups are in-window child windows).
ux-audit (fixed): tip closes on keyboard activation and on Button click; TipHost.hide keyed by an owner id from show (not text); palette resets scroll on typing; a hover-set highlight no longer nudges the list.
ux-audit (open, low): tip does not follow a moving control or a changed tooltip text; a hover tip can sit above the palette scrim until the pointer moves; old Tooltip in Main (toolbar-compact, card rows) and Panel header still separate from TipHost; no Esc dismiss; 500ms delay on every control (no "instant between items"); windows without TipLayer (Picker, Help, Unlock, Onboarding, Toast, SuggestHint) would drop a future tooltip.
