# Fable pass brief (owner-approved: ONE focused Fable agent, run once)

Owner, 2026-10-18: "use fable to round out the app, do an analysis, fix, refinement, upgrade, and more to push the application to a finished state." Earlier (2026-10-17): "analyse the application, poke holes in it, fix bugs, security issues, other issues missed, challenge architectural decisions and find front and back end issues; one to polish and validate the app."

Run it only after the feature queue in `docs/HANDOVER.md` (Main window wired, list/snippet management, Picker windows, quick edit, resize, tray fixes) has landed and CI is green, so it works on a near-complete app. One agent, one pass, on its own branch/PR.

Scope
1. Analyse: read SPEC/PLAN/DESIGN/ADRs, the code, the three Opus reviews in `docs/reviews/`, the open items in `docs/reviews/PR4-final-review.md`; produce `docs/reviews/fable-analysis.md` ranked by severity.
2. Fix: bugs, security (import of untrusted archives, clipboard/sensitive data, path handling, crypto/DPAPI wiring), data-loss paths, accessibility (keyboard order, focus, labels, contrast, 225% scaling, high contrast), i18n (no hard-coded English, all 7 locales complete), UX learnability (tooltips, empty states, help, first-run), performance budgets (SPEC section 9).
3. Challenge architecture: layering, duplicated logic, files over 800 lines, dead code, public API consistency.
4. Refine and upgrade: polish visuals against DESIGN.md with measured screenshots, README/docs accuracy, launcher robustness.
5. Validate: full `scripts/gate.sh --full`, both toolchains, Windows `cargo check`, `cargo deny`; list everything still unverified on Windows.

Rules: tests first, never weaken a test; Windows behaviour stays "unverified" until the owner runs the wizard; no merge; commit at least every 20 minutes; report in <=30 lines.
