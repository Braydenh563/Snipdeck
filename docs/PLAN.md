# Snipdeck implementation plan

Inputs: [`SPEC.md`](SPEC.md), [`../DESIGN.md`](../DESIGN.md), [`adr/`](adr/), [`spikes/README.md`](spikes/README.md).

**Verification labels.** **[L]** verified automatically in cloud CI (Linux). **[W]** needs a Windows run:
CI builds the Windows artifact and compiles it, but a human must run the Windows test wizard
(`run-windows-tests.bat`). Every milestone that touches Windows adds its steps to `tools/windows-test/tests/`
so the owner runs one wizard, once per milestone, instead of reading instructions.

**Agents.** Opus: architecture, contracts, unsafe/Win32 code, reviews. Sonnet: feature implementation per crate.
Haiku: mechanical work (translations, test fixtures, docs, CI chores). One PR per milestone; an Opus reviewer
runs on every PR before merge.

## Milestones

| | Milestone | Contents | Verified |
|---|---|---|---|
| **M1** | Foundation (Linux-buildable) | Cargo workspace, CI (Linux + Windows jobs, `fmt`, `clippy -D warnings`, `cargo deny`, tests), `snipdeck-core` (domain types, template engine, fuzzy search, use-pipeline logic, Free/Pro entitlements), `snipdeck-store` (SQLite + migrations, blob store, FTS5, trash, backup, `.snipdeck` export/import, encryption of sensitive items), licence verification (Ed25519) | **[L]** fully (unit + integration tests, property tests for templates, forward-only migrations with a pre-migration backup, tested from a real v1 fixture) |
| **M2** | Windows platform layer | `snipdeck-platform` Windows implementation: multi-format clipboard read/write/listener with delayed rendering, global hotkeys, foreground/Target tracking with UIA fallback and paste-key table, paste pipeline, tray, autostart, single instance | Compiles **[L]**; behaviour **[W]** (wizard steps S1 test script + new) |
| **M3** | UI foundation + Panel | `tokens.slint` generated from `design/tokens/palette.json`, component library, software-renderer setup, non-activating Panel with ListView, i18n scaffolding (`@tr`, 7 locales extracted), CJK fallback | Screenshot tests **[L]** (software renderer under Xvfb); focus/IME/DPI **[W]** |
| **M4** | Picker + Main window + Editors | Hotkey Picker, 3-pane Main window, text editor (16 KB cap + rope viewer), image editor (crop/rotate/resize/redact), template fill form, Trash, import/export UI | **[L]** logic and screenshots; interaction **[W]** |
| **M5** | Capture, History, Sensitive, Supporter | Snip overlay, OCR trait + Windows OCR (+ optional RapidOCR pack), Recent inbox, sensitive items (mask, auto-clear, encryption + master password), paste stack, floating lists, licence UI, onboarding, settings, daily backups, crash log | Logic **[L]**; capture/OCR/hotkeys **[W]** |
| **M6** | Release | MSIX + MSI packaging, SignPath signing, updater, winget manifest, Store listing assets, perf CI (budgets from SPEC §9), accessibility audit (Narrator, contrast, keyboard), docs site/README, CLA bot | Packaging **[L]**; install, perf on real hardware, a11y **[W]** |

## M1 detail (starts now)

1. **Scaffold (Opus).** Workspace layout from SPEC §10, `rust-toolchain.toml`, `deny.toml`, GitHub Actions
   (`ci.yml`: Linux test job; Windows `cargo check` job), `.editorconfig`, `CONTRIBUTING.md` with the CLA note.
   Defines the `snipdeck-core` public types (ids, `Board`, `List`, `Item`, `Format`, `Tag`, `UseOutcome`, error types)
   as the contract the other crates build on.
2. **`snipdeck-core` logic (Sonnet, TDD).** Template parser/renderer (`{date}`, `{time}`, `{datetime:FMT}`,
   `{clipboard}`, `{cursor}`; prompted `{field:Name}`, `{field:Name|default}`, `{choice:Name|a|b}` behind an entitlement check),
   fuzzy search index (title + tags + 64 chars, `nucleo`), Free/Supporter entitlement rules (5 boards, prompted fields,
   floating lists, paste stack, sensitive encryption).
3. **`snipdeck-store` (Sonnet, TDD).** Schema and migrations, repositories behind traits, content-addressed blob
   store, FTS5 search, trash with 30-day purge, ordering, `.snipdeck` export/import, Ditto/CopyQ/text-folder importers (fixtures by Haiku),
   XChaCha20-Poly1305 for sensitive items with key-provider trait (DPAPI implementation lives in M2).
4. **Licence (Sonnet).** Ed25519 key format, offline verify, test vectors.
5. **Review (Opus).** Contract conformance, schema review, security review of crypto and import parsing.

**Done when:** `cargo test --workspace` passes on Linux CI, `cargo clippy -D warnings` and `cargo deny` are clean,
the Windows job compiles the workspace, and the SPEC's M1-relevant behaviours each have a test.

## M1 known gaps (carry into M2 or later)

| Gap | Why it matters | Plan |
|---|---|---|
| CopyQ: the binary `.cpq` export is not parsed; JSON from `copyq eval` is, and is untested against a live CopyQ | Importer may not match real CopyQ output | Test with a real CopyQ install (Windows wizard step) or drop CopyQ from v1 |
| Windows clipboard history importer not done | Listed in SPEC §5.9 | Needs the Windows platform layer (M2) |
| `DataKey` is not `Clone`; portable uses `Library::data_key` / `KeySource::Library` | Awkward borrow | Revisit when the app crate owns the key lifecycle (M2/M5) |
| Release builds need `SNIPDECK_LICENCE_PUBLIC_KEY` (guard, see `docs/licensing.md`); release-mode tests need a throwaway key | Intentional fail-closed behaviour | Set the real key in the M6 release pipeline; keep CI debug-only until then |
| DPAPI `KeyProvider`, Argon2 timing on Windows | Provider contract is in `docs/crypto.md` | M2 |

## Windows gates

| After | Owner action | Decides |
|---|---|---|
| M0 (S1 done: GO; S2 re-run pending, needs IMEs installed) | Re-run S2 with Japanese, Chinese, Korean keyboards; S4 W4-W7 | Slint stays, or the S2 fallback in `docs/spikes/` |
| M2 | Run the platform steps | Paste, target detection, restore, hotkeys work on real apps |
| M3 | Run the panel steps | Focus, IME in the real panel, DPI, Mica-or-solid choice |
| M5 | Run capture/OCR steps | Snip overlay, OCR quality, history exclusion |
| M6 | Install the MSIX, run the release checklist | Ship |

## Risks and fallbacks

| Risk | Fallback |
|---|---|
| S1 fails on Windows (panel steals focus) | Custom `WindowAdapter` on our own HWND with the software renderer; or "re-focus last window" paste mode |
| S2 fails (IME unusable in Slint text fields) | Upstream patch via `[patch.crates-io]`, or a custom text field fed by `WM_IME_*` |
| UIA detection too unreliable | Default to explicit Paste vs Copy buttons instead of smart click, with the per-app table |
| Mica unreliable | Solid chrome (already the baseline) |
| Slint API churn | Exact pin; unstable APIs confined to `snipdeck-platform` |
