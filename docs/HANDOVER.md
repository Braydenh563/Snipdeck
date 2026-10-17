# Handover (2026-10-17)

Read this first in a new session, then `docs/PLAN.md`, `docs/SPEC.md`, `DESIGN.md`.

## Where things stand

- **Repo:** `Braydenh563/Snipdeck` (renamed from Backpack; the local folder is still `/home/user/Backpack`).
  Branch `claude/clipboard-assistant-design-o1eool`, open **PR #2** (not merged). `main` holds only PR #1
  (spec, ADRs, DESIGN.md, palette tokens, GPL-3.0).
- **Product:** Snipdeck, a Windows-first sticky-note-style snippet deck in Rust + Slint. Decisions are in
  `docs/SPEC.md` (grill rounds 1-2), `docs/adr/0001-0007`, `DESIGN.md` ("calm, tactile deck": warm neutrals,
  Index Blue for selection only, ink primary buttons, IBM Plex, number gutter).
- **M0 spikes (S1 focus-safe paste, S2 IME, S3 long text, S4 Mica):** all GO WITH CHANGES. Only S3 is
  measured (Linux). S1/S2/S4 are **unverified on Windows** except one informal S1 check in Notepad (paste,
  clipboard restore and Win+V exclusion worked; OneNote and Windows Terminal showed no focused control, so a UI
  Automation fallback is planned). Results and constraints: `docs/spikes/README.md`.
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

Run the S1 test script on a real Windows PC (`run-windows-tests.bat`, quick mode, S1 first, about 10 minutes)
and paste the clipboard summary into the chat. **M2 (Windows platform layer) should not start until S1 passes.**
The owner is short on time; the opt-in automation may cover S1 steps 1-2d and 5.

## Next steps

1. When S1 results arrive: GO -> start M2 (`docs/PLAN.md`); NO-GO -> use the fallback in `docs/spikes/S1-noactivate.md`
   (custom window adapter, or "re-focus last window" paste mode) and amend ADR-0003.
2. Merge PR #2 once CI is green and the owner agrees (ask first; never merge unasked).
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
