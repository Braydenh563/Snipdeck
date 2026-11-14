# store-ready (coordinator)

Branch claude/store-ready from origin/claude/compassionate-pascal-pod8ux (ecd3dd7). Hub brief 2026-11-14.

Done (uncommitted until the gate passes):
- Quick check cut: 12 steps, 14.5 min (retired 1,2,5,6,7,8,13,14,16,18,19; launch moved to 3; Invoke-QcHotkey/-Diagnostic and diag-render -Quick removed).
- Item 3 manifest: 7 Resource languages, MaxVersionTested 10.0.26300.0 (owner PC, windows-results-20261017.md); tests in scripts/test_msix_manifest.py.
- Item 4 privacy.md: Store data location, Recent asks first, Hello, typing hooks, what Microsoft collects.
- Item 5 text: docs/store-listing.md; logo docs/images/store-logo-300.png via scripts/make_msix_assets.py (cairosvg).

Next:
- Sonnet agent: item 1 (free-only packaged build), item 2 (Recent asks on first run), demo data without figma.com + goldens, screenshots >=1366x768.
- Item 6: store-submission.md rows; README "Recent inbox" line after item 2.
- gate --full, 1.99.1 clippy, push, report to hub.
