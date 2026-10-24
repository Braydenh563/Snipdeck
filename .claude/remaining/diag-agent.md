# diag-agent (render glitch + hotkey diagnostics), branch claude/compassionate-pascal-pod8ux

## Done
- docs/reviews/render-glitch-analysis.md: H1-H4 ranked, evidence, result table.
- SNIPDECK_DIAG=render|fix (app diag.rs, platform windows/paint_diag.rs, ui damage.rs).
- tools/windows-test/diag-render.ps1 + README section.
- Every hotkey failure reported (HotkeysFailed toast), kept in controller, shown on Settings rows;
  tier-change re-register no longer swallowed; Free Paste next row note; PasteStack toast wording;
  3 strings translated in 6 locales.

- Verified: targeted Linux tests pass; Windows cross clippy -D warnings clean (stub C compiler
  for sqlite, check only); gate --staged green.

## Next
- Owner runs tools/windows-test/diag-render.ps1, sends Desktop\Snipdeck-diag-<date>.txt.
- Per the result table in docs/reviews/render-glitch-analysis.md: make the fix permanent.
- Not done: cargo +1.99.1, full workspace tests, Windows build/link, welcome-tour hotkey rows.

## Blocked
- Windows-only behaviour unverified until the owner runs diag-render.ps1.
