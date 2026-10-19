# Documentation index

Start with the [project README](../README.md) for status. Items below are grouped by what you want to do.

## Using Snipdeck

| Doc | What |
|---|---|
| [`user-guide.md`](user-guide.md) | Hotkeys, Panel, Picker, boards, templates, sensitive items, import/export, settings, with what exists today |
| [`troubleshooting.md`](troubleshooting.md) | Build, test and run problems, and known limits |

## Understanding the product

| Doc | What |
|---|---|
| [`SPEC.md`](SPEC.md) | The v1 product specification: features, Free vs Supporter, performance budget |
| [`../DESIGN.md`](../DESIGN.md) | Design language: colour, type, components, layout, motion |
| [`PLAN.md`](PLAN.md) | Milestones M1-M6, verification labels, Windows gates, risks |
| [`HANDOVER.md`](HANDOVER.md) | Current state, open gaps, next steps (for whoever works on it next) |
| [`../CHANGELOG.md`](../CHANGELOG.md) | What changed |

## Working on the code

| Doc | What |
|---|---|
| [`../CONTRIBUTING.md`](../CONTRIBUTING.md) | Setup, gate, rules, goldens, i18n, Windows checks, pull requests |
| [`development.md`](development.md) | Code tour by crate, how a click becomes a paste, test strategy |
| [`architecture.md`](architecture.md) | Crate responsibilities and the `snipdeck-core` contract decisions |
| [`store.md`](store.md) | Data folder, schema, migrations, search, blobs, trash |
| [`crypto.md`](crypto.md) | Encryption of sensitive items: key hierarchy, formats, limits |
| [`format.md`](format.md) | The `.snipdeck` archive format and the importers |
| [`packaging.md`](packaging.md) | MSIX, Microsoft Store and code-signing steps (owner, on Windows) |
| [`licensing.md`](licensing.md) | Licence key format, verification, build-time public key, release procedure |
| [`CODEMAP.md`](CODEMAP.md) | Generated: where every definition and test lives |
| [`../crates/snipdeck-ui/README.md`](../crates/snipdeck-ui/README.md) | UI crate conventions and test harness |
| [`../tools/windows-test/README.md`](../tools/windows-test/README.md) | The Windows test wizard |

## Decisions and evidence

| Doc | What |
|---|---|
| [`adr/`](adr/) | Architecture decision records: 0001 Rust and Slint, 0002 local-first SQLite, 0003 non-activating panel paste, 0004 OCR engines, 0005 GPL-3.0 / Supporter / CLA, 0006 distribution, 0007 toolkit constraints from the M0 spikes, 0008 window chrome and tray |
| [`spikes/`](spikes/README.md) | M0 spike findings (S1 paste, S2 IME, S3 long text, S4 Mica) and the first Windows run results |
| [`reviews/`](reviews/) | Independent reviews: M1 core and store, M3 security, M3 UI components, M3 Win32 tray |

## Project policy

[`../SECURITY.md`](../SECURITY.md), [`../CODE_OF_CONDUCT.md`](../CODE_OF_CONDUCT.md), and the licence in
[`../LICENSE`](../LICENSE) (GPL-3.0-only).
