# Development guide

A tour of the code and how it is tested. Process rules (gate, pull requests, golden images, i18n) are in
[`../CONTRIBUTING.md`](../CONTRIBUTING.md); design decisions are in [`architecture.md`](architecture.md) and
[`adr/`](adr/). `docs/CODEMAP.md` lists where every definition and test file lives (regenerate with
`python3 scripts/codemap.py`); grep it before opening files.

## Architecture tour

Dependency rule: `app -> ui, platform, store, core` (not `portable` yet: nothing in the app imports or exports);
`portable -> store, core`; `store -> core`;
`platform -> core`; `ui` has no internal dependencies in code, only data shapes that mirror the app's
view-models. `core` depends on nothing internal.

| Crate | Read first | What to know |
|---|---|---|
| `snipdeck-core` | `lib.rs`, `template.rs`, `search.rs`, `licence.rs` | Pure: it never reads the clock, the clipboard or the OS; callers pass `now`, the clipboard text and the entitlements. Ids are UUIDv7 newtypes, stored enums have stable string keys, errors are one `thiserror` enum. `#![forbid(unsafe_code)]`. |
| `snipdeck-store` | `store.rs`, `repo.rs`, `schema.rs`, `migrations.rs`, `crypto/` | Synchronous SQLite (`rusqlite`, WAL, FTS5), content-addressed blobs, sparse integer ranks for ordering, trash, `Store::transaction` / `Batch`, XChaCha20-Poly1305 envelopes and the key ring. Repositories are traits, so the rest can run against fakes. See [`store.md`](store.md), [`crypto.md`](crypto.md). |
| `snipdeck-portable` | `export.rs`, `import.rs`, `archive.rs`, `importers/` | `.snipdeck` zip reader and writer with hard limits, three importers, daily backups. Treats every file as untrusted; validates all of it, then writes in one transaction. See [`format.md`](format.md). |
| `snipdeck-platform` | `pipeline.rs`, `route.rs`, `keys.rs`, `windows/` | The paste sequence (snapshot, write, inject, wait, restore) is written against traits (`Clipboard`, `Keyboard`, `Timing`), with fakes in `testing`. The `windows/` module (`cfg(windows)`) is the only place `unsafe` is allowed, each block with a `// SAFETY:` note. |
| `snipdeck-ui` | `ui/theme.slint`, `ui/components/`, `ui/panel.slint`, `src/testing.rs` | Tokens generated from `design/tokens/palette.json` by `build.rs`. One component per file, colours only through `Theme.*`. Windows (Panel, Picker, fill form) take plain structs and models and leave as callbacks; nothing in the crate talks to the OS. Conventions: [`../crates/snipdeck-ui/README.md`](../crates/snipdeck-ui/README.md). |
| `snipdeck-app` | `usecase.rs`, `panel.rs`, `controller.rs`, `binding.rs`, `main.rs` | `usecase`: use an item (content, template, paste pipeline, count the use). `panel`: the view-model. `controller`: Slint-free state machine that turns events into `Effect`s. `binding`: connects the controller to the Panel window. `host`: `Clock`, `TargetSource` and `HotkeyBackend` traits with fakes. `platform`: the real Windows services, or fakes elsewhere. |

Other trees: `design/` (palette source and generator, `design/tools/palette.py`), `tools/windows-test/`
(Windows wizard), `spikes/` (throwaway M0 prototypes, their own workspaces), `docs/`.

### How a click becomes a paste

1. The Panel window (`snipdeck-ui`) raises `row-activated(i)`.
2. `binding.rs` calls `Controller::activate_row(i, Paste)`.
3. The controller asks `usecase` to resolve the item: load its formats from the store, render a template,
   reject a prompted template that needs answers (`UseError::NeedsAnswers`), and read the current
   Target.
4. `snipdeck-platform`'s route says whether the Target can take a paste and with which key; the pipeline
   writes the clipboard, injects the key, waits for the Target to ask for the data, and restores the
   previous clipboard.
5. The controller records the use, queues a toast, and returns `Effect`s; `binding.rs` pushes changed
   properties back into the window.

## Test strategy

Cloud CI is Linux, so the strategy is to push as much as possible behind traits and test it there, to
render the UI headlessly, and to leave a small, explicit set of checks for a person on Windows.

| Layer | How | Where |
|---|---|---|
| Logic | Unit and integration tests; property tests (`proptest`) for templates, ordering and parsers; fixtures for importers, a real v1 database fixture for migrations | `crates/*/src` (`#[cfg(test)]`), `crates/*/tests/` |
| Hostile input | Crafted archives, databases and folders must be refused with limits intact | `snipdeck-portable/tests/hostile.rs`, `validation.rs`, `sensitive.rs` |
| Storage safety | Remanence (plaintext left behind after delete or encrypt), key ring, crypto round-trips | `snipdeck-store/tests/remanence.rs`, `crypto.rs` |
| UI appearance | Headless harness: each window renders with the software renderer on a frozen clock and is compared with a golden PNG; specific pixels are asserted for claims like "the focus ring is 2 px" | `snipdeck-ui/tests/*.rs`, goldens in `tests/golden/`, harness in `src/testing.rs` |
| UI behaviour | Slint's testing backend (`i-slint-backend-testing`): find elements by accessible label and role, invoke the default action, click, type | `snipdeck-ui/tests/*_logic.rs`, `snipdeck-app/tests/panel_window.rs` |
| App | The controller runs with fakes: a fake clipboard, keyboard, timing, Target and hotkey backend, and a fixed clock | `snipdeck-app/src/*` tests, `snipdeck-platform/src/testing.rs` |
| Windows type-check | `cargo clippy --target x86_64-pc-windows-msvc` from Linux, and CI's Windows `cargo check` | CI, [`../CONTRIBUTING.md`](../CONTRIBUTING.md) |
| Windows behaviour | The Windows test wizard, run by a person | `tools/windows-test/` |

Rules the tests follow:

- **A fixed "now".** Time is injected (`FixedClock`, `Harness::advance`, `mock_elapsed_time`); a test never
  reads the real clock.
- **One process per Slint test file.** The testing backend and the screenshot `Harness` set a global
  platform, so a test file uses one or the other, never both. Keep window tests in separate files under
  `tests/`.
- **Goldens are reviewed, not trusted.** `UPDATE_GOLDEN=1` writes them; you look at each PNG. Tests that
  need a CJK font skip themselves where none is installed (CI).
- **Tests first.** A failing test comes before the code, and a test is never weakened to pass.

### Fake platform

`snipdeck-platform` exposes fakes behind the `testing` feature (`FakeClipboard`, `FakeKeyboard`,
`FakeTiming`, `FakeTray`, `FakeSystemLook`), and `snipdeck-app::host` adds `FakeTargets`, `FakeHotkeys` and
`FixedClock`. Off Windows, `snipdeck-app::platform` wires these in, which is why `--demo` and
`--screenshot` work on a Linux dev box. A fake can only say what we believe Windows does; it cannot show we
are right.

### Windows wizard

`run-windows-tests.bat` builds the apps and walks a person through steps in `tools/windows-test/tests/*.json`
(`s1`, `s2`, `s4` for the M0 spikes, `m2-platform`, `m3-app`), recording pass, fail or skip per step. Some
steps can run by themselves (`tools/windows-test/auto/`); that automation has never run on Windows, and the
list of its assumptions is at the end of the wizard README. `tools/windows-test/m2-harness` is a small
program that drives the platform crate for the M2 steps. When you add Windows-only behaviour, add its
steps. Results go in `docs/spikes/` (for spikes) only when they came from a real Windows run.

## Generated files

| File | Source | Command |
|---|---|---|
| `tokens.slint` (in Cargo's `OUT_DIR`, not in the repo) | `design/tokens/palette.json` | `cargo build` (`build.rs`) |
| `crates/snipdeck-ui/lang/**` | `@tr` strings in `ui/**/*.slint` | `python3 scripts/i18n.py extract` |
| `docs/CODEMAP.md` | the Rust sources | `python3 scripts/codemap.py` |
| `crates/snipdeck-ui/ui/icons.slint` | `crates/snipdeck-ui/icons/*.svg` | `python3 scripts/ui_icons.py` |
| `crates/snipdeck-ui/tests/golden/*.png` | the windows | `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui` |

## Working notes

- Debug `target/` for `snipdeck-ui` is about 4 GB. Use a scratch `CARGO_TARGET_DIR` and delete it after.
- Slint is pinned exactly (`=1.18.1`); its winit accessors are unstable, so all winit and Win32 access stays
  in `snipdeck-platform` and the app's window attach code.
- The renderer is the software renderer: Skia and FemtoVG have open transparency bugs on Windows
  ([`adr/0007-m0-toolkit-constraints.md`](adr/0007-m0-toolkit-constraints.md)).
- Release builds need `SNIPDECK_LICENCE_PUBLIC_KEY`; see [`licensing.md`](licensing.md).
- The current state, open gaps and the next steps live in [`HANDOVER.md`](HANDOVER.md).
