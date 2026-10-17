# Handover (2026-10-17)

Read this first in a new session, then `docs/PLAN.md`, `docs/SPEC.md`, `DESIGN.md`.

## Where things stand

- **Repo:** `Braydenh563/Snipdeck`, local folder `/home/user/Snipdeck`. PR #1, #2 (M0/M1) and #3 (lean workflow) are merged into `main`.
- **Product:** Snipdeck, a Windows-first sticky-note-style snippet deck in Rust + Slint. Decisions are in
  `docs/SPEC.md` (grill rounds 1-2), `docs/adr/0001-0007`, `DESIGN.md` ("calm, tactile deck": warm neutrals,
  Index Blue for selection only, ink primary buttons, IBM Plex, number gutter).
- **M0 spikes:** first real Windows run 2026-10-17 (Windows 11 only, quick mode): `docs/spikes/windows-results-20261017.md`.
  **S1 = GO** (focus kept; paste into Notepad/Chrome; Shift copy-only; elevated fallback; password fields).
  S1 fix needed: Alt+click opens the target's menu hints and releasing Win opens Start (step 2c).
  S2 is **inconclusive** (no IME was installed, so nothing could be composed). S4: DWM calls ok, theme switching ok,
  but OS caption buttons overlap the app's own close button. S3 measured on Linux. Constraints: `docs/spikes/README.md`.
- **M1 (core, store, licence, crypto, portability): done and merged into the PR branch.** 488 tests, fmt/clippy/doc/deny
  clean on Rust 1.97 and 1.99.1, Windows target check passes. Opus review in `docs/reviews/M1-review.md`; all
  29 core/store findings fixed. Known gaps: `docs/PLAN.md` "M1 known gaps" (import atomic only by
  compensation, CopyQ `.cpq` not parsed, Windows clipboard-history importer, `DataKey` not `Clone`).
- **Windows test wizard:** `run-windows-tests.bat` -> `tools/windows-test/` (manual steps in `tests/*.json`,
  opt-in automation in `auto/`). The automation has **never run on Windows**; its unverified assumptions are
  listed at the end of `tools/windows-test/README.md`.
- **CI** (`.github/workflows/ci.yml`): Linux fmt/clippy/test/doc/deny + Windows `cargo check`. It uses the
  newest stable Rust, which has lints that older Rust lacks: always check with `cargo +1.99.1 ...` too.

## What is blocked on the owner

- Optional re-run on Windows: S2 after adding Japanese / Chinese / Korean keyboards (`run-windows-tests.bat`, S2 only),
  S1 step 2c after the modifier fix, and the untested S4 steps (W4-W7, Windows 10). None of these gate M2 start.

## M2 in progress (branch claude/compassionate-pascal-pod8ux, PR #4)

- Done and committed (2f21a35): `crates/snipdeck-platform` portable layer: `keys` (paste-key table, Alt/Win mask-before-release
  sequence), `route`, `payload`, `pipeline` (delayed-render paste with injected clock; 28 tests); `windows/` scaffold.
- In flight (3 agents, uncommitted, files under `crates/snipdeck-platform/src/windows/`): clipboard (Opus), keyboard +
  target + window_style (Sonnet), hotkey/autostart/single_instance/dpapi + portable `chord.rs` (Sonnet). Their
  progress files: `.claude/remaining/{clipboard,target,hotkeys}.md`. Each must pass fmt, clippy (Linux and
  `--target x86_64-pc-windows-msvc`), tests, doc before commit.
- Still to do: M2 test harness (`crates/snipdeck-platform/examples/`) + wizard steps `tools/windows-test/tests/m2-*.json`,
  Opus review of the unsafe code, tray (needs the app's event loop: M3), `scripts/gate.sh --full`, update PLAN/ADR-0003, PR.
- Windows check from Linux: `rustup target add x86_64-pc-windows-msvc`, then `cargo clippy -p snipdeck-platform --target x86_64-pc-windows-msvc --locked`
  (first build about 4-5 minutes).

## Next steps

1. **Start M2** (Windows platform layer, `docs/PLAN.md`); S1 gate is passed. Include the Alt/Win modifier masking and
   decide the caption-button question (custom frame vs OS caption) in ADR-0003 / DESIGN.md.
2. Re-run S2 with IMEs installed before building text-entry UI (M3+); if composition is unusable, see the S2 NO-GO plan.
3. Build the store transaction/batch API before the import UI (M4).
4. Set the real `SNIPDECK_LICENCE_PUBLIC_KEY` in the M6 release pipeline (see `docs/licensing.md`).

## Working notes that cost time this session

- **Disk is small.** Cargo `target/` dirs reach several GB each; use `CARGO_TARGET_DIR=/tmp/target-<name>` and delete
  it after use. Spike `target/` folders were deleted once (23 GB); the wizard rebuilds them on Windows.
- **Parallel agents:** give each its own crate/files and its own `CARGO_TARGET_DIR`; verify and commit subsets from a
  clean `git worktree` (copy only the finished files, regenerate `Cargo.lock` from HEAD's lock with
  `cargo metadata`, then run fmt/clippy/test/doc/deny/Windows check on both Rust versions before committing).
- **The stop hook** asks to commit whenever the tree is dirty; do not commit another agent's work in progress.
- **After a session restart** the `origin` remote may revert to the old `Backpack` URL:
  `git remote set-url origin https://github.com/Braydenh563/Snipdeck`.
- **Never answer `y` to the wizard's final "commit results?" question in dry-run tests.** That happened four times
  and pushed fake results files (now removed); `-DryRun` no longer touches git, but keep real result files out of
  the repo unless they came from a real Windows run.
- PowerShell 7 for parse checks and the wizard's `-SelfTest` / `-DryRun` is not installed by default (download the
  Linux tarball from the PowerShell GitHub releases into the scratchpad).
- The owner prefers terse answers and dislikes manual steps: automate what can be automated, give y/n prompts.
