# Translations (docs/V1-PLAN.md step 10)

Task: fill every empty msgstr in the six lang/<locale>/LC_MESSAGES/snipdeck-ui.po files so
`python3 scripts/i18n.py check` reports 0 untranslated.

## Done
- First pass (156 entries, 26 per locale): de, es, fr, pt-BR, ja, zh-Hans (merged into the session branch at 84c8033)
- Second pass after merging claude/compassionate-pascal-pod8ux (189 entries per locale, 1134 total):
  - de (189 entries)
  - es (189 entries)
  - fr (189 entries)

## Next
- pt-BR, ja, zh-Hans (189 entries each), one commit per locale, `i18n: <locale> translated`

## Blocked / for human review
- Baseline is 156 untranslated (26 per locale), not 1290 as the brief said. After the merge the count was 1134
  (189 per locale).
- "Escape" and "Ctrl" are kept as "Esc" and "Ctrl" per the brief's key-name rule. Windows German uses "Strg",
  and Windows French shows "Échap" for Esc.
- "Insert" (template submit, pastes into the target) is rendered as the Paste word per locale.
- "{n} item" family: the English source is singular with {n}, so plural agreement is not handled (the i18n script
  notes plurals are not used yet).
- "Your deck is empty": "deck" is kept as the product metaphor in each locale; a native reviewer may prefer another word.
