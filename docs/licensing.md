# Licensing and support

Snipdeck is free software under the GPL-3.0-only licence (`LICENSE`). Every feature is on for everyone, on every
build. There is no paid tier, no Supporter, no licence key and no entitlement check; owner decision 2026-11-15
([ADR-0005](adr/0005-licensing-and-pro.md) has the history).

## The optional support link

**Support Snipdeck ☕** is in Help ▸ About, Settings ▸ Support and the command palette. It opens
`https://buymeacoffee.com/braydenh563` in the default browser (the same open-URL path as Help's other links). The
text says it is optional, keeps the app free and unlocks nothing. The app handles no payment data and sends nothing;
the constant is `help_binding::SUPPORT_URL`.

## Old files

Settings and libraries from builds that had a licence key still load. `settings.json` never held a licence, and
unknown fields are ignored. A `licence.key` file left in the data folder is never read, and **Erase library**
removes it with the rest of the library. It is safe to delete by hand.

## For developers

- There is no key to build with: release builds need no environment variable for licensing.
- Do not add a second gating path for any feature. A feature that is built is on.
- The old licence issuer tool and its key guard were removed together with the key format.
