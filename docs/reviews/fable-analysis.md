# Fable analysis (2026-10-18, running document)

Ranked findings. Status: `[ ]` open, `[~]` ticketed in HANDOVER, `[x]` fixed (commit).

## Critical (data loss, security, broken core flows)
- [ ] C1 No daily backup, no trash purge and no integrity check are ever run by the app (SPEC 5.7a: daily zipped `.snipdeck`, keep 7; Trash 30 days). The store has `backup`, `TrashRepo::purge`, `PRAGMA integrity_check`; `snipdeck-app` never calls them. Plan: a `maintenance` module run once at startup on the frozen clock (purge older than 30 days, backup if the newest is older than 24 h, rotate to 7) + a Settings "Restore backup".
- [ ] C2 Sensitive items: no clipboard auto-clear (SPEC 5.7), no master password, no encryption key lifecycle in the app (store supports it: `crypto/keyring.rs`). Sensitive rows are shown masked and stay out of history only via the platform markers.
- [ ] C3 Licence is not wired outside `--demo` (`main.rs:336`): every real user is Free; Pro keys cannot be entered. Needs the licence dialog (SPEC 5.3) + `SNIPDECK_LICENCE_PUBLIC_KEY` at release.
- [ ] C4 No crash log / panic hook (SPEC 5.7a): a panic on the UI thread loses the process silently; nothing for "Report a problem".
- [ ] C5 Picker and TemplateFill are computed in the controller (`View.picker`/`View.fill`) but never rendered: the Picker hotkey and prompted templates do nothing visible (PR4 review #13).

## High (architecture, robustness)
- [ ] H1 `crates/snipdeck-app/src/controller.rs` is 2292 lines (limit 800) and will grow with the Main window: split into `controller/{panel,picker,fill,tray,hotkeys,toast}.rs` behind one `Controller` facade, or move window-specific logic into per-window view-models.
- [ ] H2 Panel position / size memory per monitor (SPEC 5.1) is absent from `settings.rs`.
- [ ] H3 `windows/clipboard.rs` (1268 lines), `template.rs` (1005), `import.rs` (925), `blobs.rs` (902), `chord.rs` (866): over the 800-line limit; split by responsibility where a seam exists (clipboard: owner window + jobs; template: parse vs render).

## Medium / Low
- (to be filled from the pre-Fable facts file)

## Missed features in scope (brainstorm)
- (to be filled)

## Logo
Current (`assets/logo/snipdeck-icon.svg`, rendered at 128 px): three fanned cards with a blue spine on a near-black rounded tile; wordmark in IBM Plex Sans SemiBold. It is competent, but:
- L1 The near-black tile contradicts DESIGN.md ("warm card stock", Index Blue only for selection): use neutral-850 warm (hue 80) or the light `bg.app` tile with a hairline, so the icon matches the app's chrome in both themes.
- L2 At 16-32 px (tray, taskbar) three cards + fold + three text lines blur into a grey smudge; ship size-specific variants: 16/20/24 px = one card with the blue index tab and two bars; 32-48 px = two cards; 64+ = the full stack. `scripts/make_logo.py` should emit all sizes into the `.ico` (16-256) the exe and tray need (HANDOVER 9b, still missing).
- L3 The blue spine reads as a notebook/binder. The product metaphor is an index card deck: make the blue a left index *tab* (short, rounded, like the list tabs in the Panel) rather than a full-height spine, and drop the folded corner so the only accent is the tab.
- L4 The text bars are mid-grey on white: at small sizes contrast fails; use ink (neutral-850) bars, two of them, the first longer (title + preview, matching ItemCard anatomy).
- L5 The wordmark is fine; keep letter-spacing slightly tighter (-1%) and align its x-height with the tile's optical centre (currently sits a little low).
