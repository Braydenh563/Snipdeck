# Translations (docs/V1-PLAN.md step 10)

Task: fill every empty msgstr in the six lang/<locale>/LC_MESSAGES/snipdeck-ui.po files so
`python3 scripts/i18n.py check` reports 0 untranslated.

## Done
- de (26 entries)
- es (26 entries)
- fr (26 entries)
- pt-BR (26 entries)
- ja (26 entries)

## Next
- zh-Hans (26 entries), one commit, `i18n: zh-Hans translated`

## Blocked / for human review
- Baseline is 156 untranslated (26 per locale), not 1290 as the brief said.
- "Escape" and "Ctrl" are kept as "Esc" and "Ctrl" per the brief's key-name rule. Windows German uses "Strg",
  and Windows French shows "Échap" for Esc.
- "Insert" (template submit, pastes into the target) is rendered as the Paste word per locale.
