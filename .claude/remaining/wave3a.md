# wave3a remaining work

## Done
- B3 export list as text, B6 duplicate item, B9 tray double-click, B13 CLI checks (PR #6).
- C6 edge dock: settings + tests were present; added the wizard position-step test.

## Next
- C5 Accent colour (owner authorised hub-directed work): AccentChoice setting (Blue, Teal, Green, Amber, Rose, Violet, Windows),
  colours resolved in Rust from design/tokens/palette.json (blue = no override), Windows accent read + nudge, SystemLook override in Slint
  Theme, Settings row, contrast test for every pair, 225 % text size check.
- C2 Paste stack (Supporter): multi-select, Start stack, queue shown in the Panel.

## Blocked
Nothing. Windows-only behaviour (accent registry read, tray double-click) is unverified.
