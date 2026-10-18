## What and why

<!-- One change per pull request. Link the issue. -->

## Checks

- [ ] Tests written first, and none weakened or removed to pass
- [ ] `scripts/gate.sh --full` passes; paste its three lines (`ran`, `failed`, `skipped`):

```text

```

- [ ] UI change: goldens regenerated with `UPDATE_GOLDEN=1`, each changed PNG looked at
- [ ] New `@tr` strings: `python3 scripts/i18n.py extract` run
- [ ] New dependency: `cargo deny check` passes
- [ ] Docs and `CHANGELOG.md` (Unreleased) updated where the change makes them wrong

## Not verified

<!-- Anything that needs a real Windows PC or a person to look at (paste into real apps, hotkeys, IME,
DPI, Narrator). Say "nothing" only if it is true. -->

## CLA

- [ ] I have signed, or will sign when asked, the Contributor License Agreement (see `CONTRIBUTING.md`).
