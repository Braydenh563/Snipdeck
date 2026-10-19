# Fable pass brief (owner-approved: ONE focused Fable agent, run once)

Owner, 2026-10-18: "use fable to round out the app, do an analysis, fix, refinement, upgrade, and more to push the application to a finished state." Earlier (2026-10-17): "analyse the application, poke holes in it, fix bugs, security issues, other issues missed, challenge architectural decisions and find front and back end issues; one to polish and validate the app."

Run it only after the feature queue in `docs/HANDOVER.md` (Main window wired, list/snippet management, Picker windows, quick edit, resize, tray fixes) has landed and CI is green, so it works on a near-complete app. One agent, one pass, on its own branch/PR.

Owner, 2026-10-19 (this run's mandate): a critical sweep before shipping that audits absolutely everything: flaws, architecture issues and weaknesses, areas of high complexity, frontend, backend, functions, UI/UX, efficiency (spec section 9 budgets), security and bugs. Report findings ranked by severity first, then fix.

Scope
1. Analyse: read SPEC/PLAN/DESIGN/ADRs, the code, the three Opus reviews in `docs/reviews/`, the open items in `docs/reviews/PR4-final-review.md`; produce `docs/reviews/fable-analysis.md` ranked by severity.
2. Fix: bugs, security (import of untrusted archives, clipboard/sensitive data, path handling, crypto/DPAPI wiring), data-loss paths, accessibility (keyboard order, focus, labels, contrast, 225% scaling, high contrast), i18n (no hard-coded English, all 7 locales complete), UX learnability (tooltips, empty states, help, first-run), performance budgets (SPEC section 9).
3. Challenge architecture: layering, duplicated logic, files over 800 lines, dead code, public API consistency.
4. Refine and upgrade: polish visuals against DESIGN.md with measured screenshots, README/docs accuracy, launcher robustness.
5. Validate: full `scripts/gate.sh --full`, both toolchains, Windows `cargo check`, `cargo deny`; list everything still unverified on Windows.

Rules: tests first, never weaken a test; Windows behaviour stays "unverified" until the owner runs the wizard; no merge; commit at least every 20 minutes; report in <=30 lines.

## Token and cost rules for the Fable agent (lean-workflow; owner: Fable is very expensive)

The parent session does all prep so Fable only does what needs Fable.
- **Prep before launch (parent, on Sonnet/Haiku):** run `scripts/orient.sh`, regenerate `docs/CODEMAP.md`, run `scripts/gate.sh --full` and put its output in `docs/reviews/pre-fable-gate.md`; Haiku `scout` agents list files over 800 lines, TODO/unwrap/expect/unsafe sites, duplicated code candidates and untranslated strings, into one file `docs/reviews/pre-fable-facts.md`. Fable reads those, not the repo wholesale.
- **Brief the agent with files, not prose:** pass the paths (`docs/fable-brief.md`, the facts file, `docs/reviews/PR4-final-review.md`, `docs/HANDOVER.md` top sections). No pasting of file contents into the prompt.
- **Read discipline:** grep `docs/CODEMAP.md` then read line ranges only; `wc -c` before any read; never `cat` several files; no screenshots except cropped ones it must judge; never read build logs whole (tail 40 lines); target dir `/tmp/target-main`, one cargo run at a time.
- **Delegate down:** Fable does analysis, design calls, security review and judgement. Implementation of mechanical fixes goes to Sonnet agents it spawns with exact file/line instructions (Haiku for renames, translations, doc fixes), each with a runnable check. Fable never edits more than it must.
- **One pass, bounded:** a single agent, a stated budget (stop at 3 hours or when the ranked findings are fixed or ticketed), findings in one file `docs/reviews/fable-analysis.md` capped at 40 lines per severity, fixes in small commits (every 20 minutes). Anything beyond the budget becomes a line in `docs/HANDOVER.md`, not more work.
- **No polling, no idle waits:** it ends when done; no foreground sleep loops. If context per turn passes 150k it writes `.claude/remaining/fable.md` and stops.
- **Report** in 15 lines: findings fixed, ticketed, unverified on Windows.
