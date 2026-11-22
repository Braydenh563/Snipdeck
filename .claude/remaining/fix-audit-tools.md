# fix-audit-tools remaining (branch claude/fix-audit-tools)
Source: .claude/audit/tools.md (TOOLS-1..63). Round 1 delivered (2bfa16c).

## MERGE-WITH-FIXES round (done)
- TOOLS-54: Down in the Symbols search field bumps symbol-enter-grid-ticks (test symbols.rs).
- HSL/HSB one-decimal: SPEC post-1.0 note + CHANGELOG line.
- Name row on a non-keyword colour: NameNotExact toast (not a bare failure); the row already shows "Use <name> (<hex>)". i18n in six locales.
- symbols.lock re-pin note in SPEC.

## Next
- gate (GATE_BASE=169ad183), push, report head + gate lines.

## Blocked / left to others
- Symbols footer shows name only (no group names in i18n.tsv).
- Windows-only code unverified: eyedropper cloak wait, DISPLAYCONFIG struct, DIB reads, touch, fallback hotkeys; Symbols fonts.
