# Panel D v2 critique (ux-auditor, 2026-10-24, measured on a stand-in render)

Passes: text contrast >= 5.43:1 everywhere; titles never truncate (246/246 px); controls >= 32 px; footer 32 px.

Findings sent back to the designer for v3:
1. Hover/selected actions crush the preview (246 -> 146-178 px; search where-label ~47% visible).
2. One-click paste not visible on the row; Copy/Edit read as the primary actions (DESIGN.md says a "Paste ↵" ghost button).
3. Reveal only on the selected sensitive row; Copy/Edit hidden until hover.
4. Empty search footer still says "Pastes into Outlook"; two Create button styles.
5. Non-text contrast < 3:1 on pin chips, row action chips, keycaps, outline buttons.
6. Selection ring 4.26:1 on the amber tint (passes); row tints 1.07:1 vs surface, type carried by the tile.
7. Redo faked disabled at ~2:1.
8. Chrome 185 px of 560 (33%): header with 7 buttons, tabs, pins, footer, bottom bar.
9. "+1" pinned button unlabeled visibly.
10. Search highlight underline reads as a hyperlink.
11. No :focus-visible styles.
12. Two-line rows: much low-weight text; compact as default is an owner call.
13. 2 px x-inset mismatch header/tabs vs pins/rows; bottom bar touches the footer.
14. Keycaps are static spans with tooltips.
15. Sensitive preview shows the last 4 characters.
