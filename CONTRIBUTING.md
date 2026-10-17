# Contributing to Snipdeck

Thanks for helping. Read [`docs/SPEC.md`](docs/SPEC.md) for what Snipdeck does,
[`DESIGN.md`](DESIGN.md) for how it looks, and [`docs/architecture.md`](docs/architecture.md) for
how the code is split.

## Setup

Install Rust with [rustup](https://rustup.rs). `rust-toolchain.toml` selects the stable toolchain
with `rustfmt` and `clippy`; `rustup toolchain install` in the repo fetches it. On Windows you
also need the Visual Studio C++ Build Tools.

## Before you open a pull request

CI runs these; run them locally first:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check                              # cargo install cargo-deny --locked
```

- Lints are set once in the root `Cargo.toml`. `unwrap`/`expect` are warnings outside tests:
  return an error or explain the invariant instead. Every `#[allow]` needs a `reason`.
- Public items in library crates need doc comments.
- New dependencies must pass `cargo deny`: licences compatible with GPL-3.0-only, no known
  advisories, crates.io only.
- `snipdeck-core` stays pure: no I/O, no OS-specific dependencies.

## Testing on Windows

Cloud CI compiles for Windows but can't click, paste or use an IME. Behaviour that needs a real
Windows desktop is checked with the test wizard: double-click `run-windows-tests.bat` in the repo
root (details in [`tools/windows-test/README.md`](tools/windows-test/README.md)). A change that
adds Windows-only behaviour adds its manual steps to `tools/windows-test/tests/`.

## Licence and CLA

Snipdeck is licensed under GPL-3.0-only. Contributors sign a Contributor License Agreement before
their first contribution is merged, so the project can offer other licences later (ADR-0005). Until
the CLA bot is set up, the maintainer will ask for it in your pull request.
