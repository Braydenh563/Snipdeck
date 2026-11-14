# render-flicker (branch claude/render-flicker)
## Done
- Traced: per-pixel-alpha windows (Slint with_transparent(true) -> winit DwmEnableBlurBehindWindow) + GDI erase (alpha 0) + partial present.
## Next
- Tests first: window_kind opaque plan; repaint Watch owes a whole frame after resize/show/OS-invalidated paint; drag emits only Move/Place.
- Fix: with_transparent(false); repaint follow on every window; GetUpdateRect probe; drop diag `fix`.
- Write docs/reviews/render-flicker-2026-11-14.md; wizard steps in quick-check.json; diag-render single pass.
- Merge origin/claude/compassionate-pascal-pod8ux; gate --full; 1.99.1 clippy; push; report to hub.
## Blocked
- Nothing. Windows behaviour unverified until owner runs it.
