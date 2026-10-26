# Panel v2 UX audit (2026-10-26, measured from goldens; no live app)

Measured by WCAG contrast from rendered pixels vs measured fills. 25 of 29 panel goldens predate later code but pass the golden tolerance, so the tolerance does not catch a doubled contrast change.

1. Keep-on-top "on" invisible in light: fill #EBF2FE vs bg.app 1.02:1, glyph text.primary (spec: accent.text). icon-button.slint:77. Fix: accent.text + filled pin, or 1px accent border >=3:1.
2. Menu switcher header shows "W... · Em..." (24 px / 37 px); spec wants >=6 chars each. Fix: New icon-only in Menu mode, list name priority, board in Chooser/tooltip.
3. Selected row light 1.08:1 (hover 1.16), dark 1.41: the only Enter-target cue. Fix: stronger light selected token (>=1.3:1) or 2px accent left bar.
4. QuickAdd "+ New item" accent.default: dark 3.76:1 (fails 4.5), not in spec, overlaps card 4 (cards-280) and cuts "Q3 report" (scale). Fix: accent.text; last scroll item or reserved padding with border.subtle top.
5. 225% scale: header icons mixed (13-14 vs 28), chips one letter, "Addre" tab cut without ellipsis, footer glyph at x358/359. Fix: scale icons with k, hide chips under 6 chars, ellipsize tabs.
6. Cards: selected preview clipped with no ellipsis, no 6 px gap to actions; selected preview text.primary instead of secondary.
7. Toast Undo 6 px above the message baseline (panel-undo-toast-dark). Centre it.
8. Focus order: pinned strip N+1 Tab stops (spec: one); QuickAdd extra stop; spec tests focus_order_matches_spec and hit_targets_at_least_24 missing.
9. 25 goldens stale (old More glyph 2.50:1). Regenerate all and tighten tolerance.
10. Missing goldens panel-260-light, panel-min-height-light (slice 9).
11. Search field with a query shows no 2px focus ring (verify focus in the render).
12. Chooser: boards and lists unlabelled, no check on current board, "Recent" entry missing.
13. No-results footer "↵ creates "zzz"": spec says "Enter creates "zzz" in <list>".
14. Left edges vary (header x16/x14, tab x21, chip x19, number x21, footer x9): align to x16.
15. Quick-use numbers text.tertiary 2.99:1 on dark selected: use text.secondary.

Passed: titles 16-17.5:1, previews 6.6-6.8, tabs 6.3-7.2, kind glyphs 4.5-7.9, search marks, geometry per spec (40/32/36/56/36/96/32), hit targets >=24 in code, tab focus ring 2px.
Not verifiable from images: live tab order, focus-visible vs click, motion, tooltips, DPI, Mica, IME, screen reader names.
