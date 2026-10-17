# Snipdeck

Windows-first sticky-note snippet deck, Rust + Slint. Read `docs/HANDOVER.md` first, then `docs/PLAN.md`, `docs/SPEC.md`, `DESIGN.md`.

## Lean workflow

Tier: `max` (set `pro` on the Pro plan). Fast checks: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` (tests: `cargo test --workspace --locked`; CI also runs `cargo deny check` and a Windows `cargo check`). CI uses the newest stable Rust: also check with `cargo +1.99.1 ...`.

- Cost is turns times context. Batch independent tool calls; grep, then
  read line ranges (`wc -c` before a read; never `cat` several files in one
  command: a 20k-character output rides in every later turn); start a fresh session at each milestone with the state
  in this file or a handover note; crop or avoid screenshots; no idle gaps
  over an hour in a long session.
- Rungs: Haiku (`scout`) for lookup and mechanical work with a runnable
  check; Sonnet high for implementation; Opus medium for design, ambiguous
  bugs and risky review, high only after a lower rung failed with evidence.
  Every agent call names its model.
- Fable is opt-in only: nothing starts it unless the user names it for a
  task, and only for that task.
- `max`: up to three agents at once, one Opus. `pro`: one agent, no Opus.
- One retry per new hypothesis; the same call three times is a stop.
- Risky or UI changes get an independent check: `reviewer`, `ux-auditor`,
  `blind-tester`. The builder does not grade its own work.
- Tests first; never weaken a test to pass. Run `scripts/gate.sh` before
  every push (`--staged` before a commit, `--changed` for the routine run,
  `--full` once at the end); paste its three lines in the report.
- Start each session with `scripts/orient.sh`. Find code by grepping
  `docs/CODEMAP.md` (regenerate with `python3 scripts/codemap.py`), then read
  line ranges. Scouts do the same.
- `.claude/now.md` keeps an Inbox and a Decisions list (project state lives in
  `docs/HANDOVER.md`). A mid-task request is
  recorded word for word in the Inbox, then triaged at the next step
  boundary in one pass: fixed now, placed in a plan, or dropped. Under 20
  items. A decision is written once, with its reason, and not remade.
- Agents keep a remaining-work file in `.claude/remaining/<agent>.md` (done,
  next, blocked) and commit at least every 20 minutes, so a usage limit
  never loses work.
- Tests take a fixed "now" (an injected clock or frozen time), never the
  real clock. A date or minute read at test time is a bug waiting for its day.
- A claim about how a UI looks or behaves needs a measurement from the
  running app (computed style, bounding box, a pixel); say what you could
  not verify. Windows-only behaviour (S1/S2/S4 spikes, `tools/windows-test/`)
  is unverified until the owner runs it on Windows.
- Replace, do not layer: change the existing implementation and remove the
  old path unless persisted data or a public interface needs it.
- Never merge a PR unasked; never commit test-run output (`spikes-results/` is ignored).
- At session end run `retro`.
