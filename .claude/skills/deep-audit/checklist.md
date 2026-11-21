# Deep audit checklist (every deep-audit agent reads this first)

The owner's ask: find everything that is wrong or missing, every little and humongous thing, and name the
fix for the class, not the case. Read-only: no repo edits except temporary probe tests (below) and your one
findings file.

## Coverage first, then findings

1. **Inventory before judging.** From `docs/CODEMAP.md` (grep it by file), list every function, Slint
   component, property, callback, `@tr` string group, setting and window in your scope. That list is your
   coverage table; every row ends `checked`, `driven`, or `not reached (why)`. A finding count without a
   coverage table is not a deep audit.
2. **Then walk every interaction a person can do in your scope:** each button, menu item, shortcut, drag,
   context menu, hover, empty, loading, error, locked and first-run state; at the window's minimum size, its
   default size and a large size; at text scale 1.0, 1.5 and 2.25; light, dark and high contrast; mouse,
   keyboard only and touch where Slint supports it.
3. **Skip what is already recorded** in `docs/HANDOVER.md`, `SPEC.md` (post-1.0 list), `DESIGN.md`
   (known limits) and `.claude/now.md`, unless it is still broken on this head (then say
   "still open, re-verified"). In-flight branches are listed in your brief; do not re-report their items.

## Lenses (every one, every surface)

- **Broken:** does it do what its label says; races, double submits, stale state, lost input, undo and redo,
  data loss, a restart restoring state, two windows showing the same data disagreeing.
- **Missing:** what the best two apps of this kind have that this does not (name them, e.g. Raycast
  snippets, TextExpander, Ditto, Alfred, PowerToys Advanced Paste, Beeftext); half-built features: a control
  with no handler, a callback with no caller, a setting nothing reads (grep the call site).
- **UX:** discoverability ("where is" and "how do I" answerable from the UI), feedback after every act
  (toast or visible change), confirm or undo on destructive acts, where focus lands after every act, error
  text that says what to do next, consistency between Main, Panel, picker, palette and editors.
- **UI and design:** `DESIGN.md` recipes and tokens (spacing, radius, type, colour), alignment, contrast,
  dark-mode parity, icon meaning, every width and text scale, no clipped or overlapping text.
- **Accessibility, WCAG 2.2 AA:** accessible names, roles and states on every control, focus order and a
  visible focus ring, 2.5.7 drag alternatives, 2.5.8 target size (24px), 1.4.3 and 1.4.11 contrast
  (compute it from rendered pixels), live announcements, reduced motion, high-contrast mode.
- **Copy:** sentence case, no em dashes, no exclamation marks, no "Oops", plurals right, no doubled words,
  the same term for the same thing everywhere, every string translated in all six `.po` files and the
  translation fits.
- **Performance:** measure with a seeded library (10,000 items, 50 lists, 5 boards, 200 tags, some 1 MB
  image items): UI-thread work over 50 ms per act, whole-model rebuilds where a diff would do, O(n^2) over
  items, timers that run while hidden or idle, redraws when nothing changed, memory that grows per act,
  start-up time. Use the test harness and `std::time::Instant` in a probe test; report numbers.
- **Security and privacy:** Sensitive items and master-password data never leaking (Recent, search index,
  logs, crash reports, exports, clipboard history, temp files, freed memory where avoidable), path
  traversal and zip-slip in imports, size caps, injection through templates or importers, unsafe FFI
  soundness, panics across `extern "system"` callbacks.
- **Logs:** run with the app's logging on during probes; every warning, error or slow-operation line is a
  finding with its cause.

## Method rules

- Drive the UI through the Slint test harness: `crates/snipdeck-ui/src/testing.rs` (`Harness`, `render`,
  `render_settled`, `advance`, `Frame::pixel`), Slint's `ElementHandle` accessibility queries and
  `mock_elapsed_time` (see `crates/snipdeck-ui/tests/main_window_logic.rs`), and for whole-app behaviour the
  controller tests with `crates/snipdeck-app/tests/support/mod.rs` (`press`, `type_text`, `chord`,
  `on_ui_thread`). Time is always stepped, never the real clock.
- Probe tests: you may add ONE temporary file `crates/<crate>/tests/probe_<agent>.rs` in your worktree,
  never committed. Delete it when done.
- **A screenshot is not a measurement:** report pixel values, element boxes from the accessibility tree,
  timings, counts. Mark every row `run` (driven and measured) or `code` (read only).
- Windows-only code (`crates/snipdeck-platform/src/windows/`) cannot run here: read it (`code`), and say
  what a Windows run must confirm.
- Cargo: always `flock /tmp/target-panel/.audit.lock cargo ...` with
  `CARGO_TARGET_DIR=/tmp/target-panel CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
  CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0`, one targeted test target at a time, never the whole suite. If the
  disk is under 1.5 GB free (`df -h /tmp`), stop running cargo and finish by reading.
- Do not trust comments, docs or plans that say something works; only what you reproduce.

## Output (one file, `<scratchpad>/deep-audit/<agent>.md`)

1. Coverage table (row, `checked` / `driven` / `not reached (why)`, `run` / `code`).
2. Findings, one row each: id (`<AGENT>-n`), severity (critical, high, medium, low), lens, evidence
   (file:line or element label, plus the number measured), how verified (`run` or `code`), the fix aimed at
   the whole class, effort (S, M, L).
3. What could not be verified and what a Windows run must check.
