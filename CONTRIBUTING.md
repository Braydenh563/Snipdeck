# Contributing to Snipdeck

Thanks for helping. Snipdeck is a work in progress ([`README.md`](README.md#what-works-today) says what
exists). Read [`docs/SPEC.md`](docs/SPEC.md) for what it does, [`DESIGN.md`](DESIGN.md) for how it looks,
[`docs/architecture.md`](docs/architecture.md) and [`docs/development.md`](docs/development.md) for how
the code is split, and [`docs/PLAN.md`](docs/PLAN.md) for what is next. Everyone taking part follows the
[Code of Conduct](CODE_OF_CONDUCT.md). For a security problem, do not open an issue: see
[`SECURITY.md`](SECURITY.md).

## Setup

1. Install Rust with [rustup](https://rustup.rs). `rust-toolchain.toml` selects the stable toolchain with
   `rustfmt` and `clippy`; `rustup toolchain install` in the repo fetches it. The minimum supported Rust is
   in `Cargo.toml` (`rust-version`).
2. On Windows, install the Visual Studio C++ Build Tools. On Linux, a C compiler is needed (SQLite is
   built from source). Slint loads fontconfig at run time, so CI installs no `libfontconfig-dev`.
3. For `cargo deny`: `cargo install cargo-deny --locked`.
4. Python 3 (standard library only) runs the helper scripts in `scripts/`; bash runs `scripts/gate.sh`.

`target/` directories are large (a debug `snipdeck-ui` build is about 4 GB). If disk is tight, point
`CARGO_TARGET_DIR` at a scratch folder and delete it when you are done.

Try the app with `cargo run -p snipdeck-app -- --demo` (Windows: `run-snipdeck-demo.bat`). It opens the
Panel on a seeded in-memory library.

## Before you open a pull request

Run the gate:

```sh
scripts/gate.sh            # fast: cargo fmt --check, clippy -D warnings, cargo doc -D warnings,
                           # code map up to date, translation catalogues consistent
scripts/gate.sh --changed  # plus the tests of the crates you touched
scripts/gate.sh --staged   # check what is staged, not the working tree (use before a commit)
scripts/gate.sh --full     # plus the whole test suite; run once before you push
```

The fast checks are these three commands, and the gate runs them for you:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

Tests are `cargo test --workspace --locked`.

It prints three lines at the end (`ran`, `failed`, `skipped`); paste them in the pull request. CI also
runs `cargo deny check` and a Windows `cargo check`:

```sh
cargo deny check
```

CI uses the newest stable Rust, which has lints older versions lack. If a lint fires in CI only, check
with the same toolchain: `cargo +1.99.1 clippy --workspace --all-targets --locked -- -D warnings`.

### Rules

- **Tests first.** Write the failing test, then the code. Never weaken or delete a test to make it pass;
  if a test is wrong, say so in the pull request.
- **Time is injected.** Tests take a fixed "now" (an injected clock or `Harness::advance`), never the real
  clock. A date or minute read at test time breaks on its day.
- Lints are set once in the root `Cargo.toml`. `unwrap`/`expect` are warnings outside tests: return an
  error or explain the invariant. Every `#[allow]` needs a `reason`. Prints (`println!`) are warnings.
- Public items in library crates need doc comments (`cargo doc` runs with `-D warnings`).
- `unsafe` is only for `snipdeck-platform`, each block with a `// SAFETY:` comment. Other crates use
  `#![forbid(unsafe_code)]`.
- `snipdeck-core` stays pure: no I/O, no OS-specific dependencies.
- New dependencies must pass `cargo deny`: licences compatible with GPL-3.0-only, no known advisories,
  crates.io only. Slint stays pinned to an exact version.
- Replace, do not layer: change the existing implementation and remove the old path, unless stored data or a
  public interface needs it.
- Claims about how the UI looks or behaves need a measurement (a pixel, a size, an accessibility-tree
  assertion), not a guess. Say what you could not verify, in the pull request.

### UI changes and golden images

The UI is tested without a display. `crates/snipdeck-ui/tests/` renders each window with Slint's software
renderer and compares the result with a PNG in `crates/snipdeck-ui/tests/golden/`.

```sh
cargo test -p snipdeck-ui                      # compare with the goldens
UPDATE_GOLDEN=1 cargo test -p snipdeck-ui      # rewrite goldens that changed (and create new ones)
```

After updating, open every changed PNG and look at it before you commit it; a golden you did not look at
is not a test. Review the PNG diff in the pull request. Component conventions (tokens only, focus ring,
accessible labels, 24 px hit targets) are in [`crates/snipdeck-ui/README.md`](crates/snipdeck-ui/README.md).

### Translations

User-visible strings in `.slint` files use `@tr("...")`. After adding or changing one:

```sh
python3 scripts/i18n.py extract    # update lang/snipdeck-ui.pot and add empty entries to each locale
python3 scripts/i18n.py check      # fails if the catalogues are stale (also part of the gate)
```

Locales: en (source), es, de, fr, pt-BR, ja, zh-Hans. A string without a translation shows in English.
Other generated files: `python3 scripts/ui_icons.py` after adding an icon SVG, and
`python3 scripts/codemap.py` to refresh `docs/CODEMAP.md` (the gate fails if it is stale).

## Testing on Windows

Cloud CI compiles for Windows but cannot click, paste or use an IME. Treat any Windows behaviour as
**unverified** until the test wizard has run it on a real Windows PC: double-click `run-windows-tests.bat`
in the repo root (details in [`tools/windows-test/README.md`](tools/windows-test/README.md)). A change that
adds Windows-only behaviour adds its manual steps to `tools/windows-test/tests/`.

From Linux you can still type-check and lint the Windows code:

```sh
rustup target add x86_64-pc-windows-msvc
cargo clippy -p snipdeck-platform --all-targets --target x86_64-pc-windows-msvc --locked -- -D warnings
```

Checking `snipdeck-app` or the whole workspace for Windows also compiles C build scripts (SQLite's, for one),
which need a stand-in archiver on Linux. Set `CC_x86_64_pc_windows_msvc=true` and
`AR_x86_64_pc_windows_msvc=<a script that creates the output .lib>` (see `docs/HANDOVER.md`). CI's
Windows job does the real thing with `cargo check --workspace --all-targets --locked`.

## Pull requests

1. Open or find an issue first for anything bigger than a small fix, so the design is agreed before the
   work is done.
2. Branch from `main`. Keep a pull request to one change; separate refactors from behaviour changes.
3. Commit messages: a short imperative subject, prefixed with the area (`store: ...`, `ui: ...`,
   `docs: ...`), and a body that says why when it is not obvious.
4. Run `scripts/gate.sh --full` and paste its three result lines in the description. Fill in the pull
   request template, including what you could not verify.
5. A maintainer reviews. Risky changes (crypto, parsing of untrusted files, Win32 code, UI) get an
   independent review in addition. Address comments with new commits; the maintainer squashes or merges.
6. Pull requests are merged by the maintainer once CI is green. Please do not merge your own.

Update the docs that your change makes wrong (`docs/`, the SPEC, the changelog under `Unreleased`).

## Licence and CLA

Snipdeck is licensed under GPL-3.0-only. Contributors sign a Contributor License Agreement before
their first contribution is merged, so the project can offer other licences later (ADR-0005). Until
the CLA bot is set up, the maintainer will ask for it in your pull request.
