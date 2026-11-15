# edge-polish (claude/edge-polish from claude/all-free 2d3f338)

Done (uncommitted until gate passes):
- store-submission.md: 10.8.2 reading (donations via third-party allowed when nothing unlocked); link stays in Store build.
- Palette "Support Snipdeck" uses msgid "donate donation coffee tip"; duplicate "... optional" removed from .pot/.po.
- DockHandle restyle: bg-overlay tab, border-default inner side + ends, Radius.sm inner corners, 2px accent pill grip (20px, 32px on hover). 6 pixel tests + 3 goldens.
- Drag dark blink: erase subclass (erase.rs guard_presented_frames / frame_presented, win_logic::EraseGate) skips WM_ERASEBKGND after a presented frame inside WM_PAINT. Hooked from repaint::follow.
- Docs: DESIGN.md §5, CHANGELOG, windows-test README (20-24 diag, D24), render-flicker review follow-up.

Next: ux-auditor (Sonnet) findings; gate --changed; commit; push; report to hub.
Blocked: Windows verification (owner).

## Eyedropper (owner bug)
- done: `eyedropping` flag suspends dock events and blur-hide; outcome returns through `show()` + ShowColours; tests in controller/tests/dock.rs `mod eyedropper`; D25 quick check.
