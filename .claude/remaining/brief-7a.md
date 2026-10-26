# Brief: step 7a Onboarding (v1 scope)

Source: docs/V1-PLAN.md item 7a. One regular window `onboarding.slint`, 4 steps, Skip on every step, Back/Next, Esc = Skip:
1 Welcome: what the Panel, Picker and hotkeys do (icons, not screenshots).
2 Hotkeys: the existing HotkeyRecorder component, defaults prefilled, conflict message reused from Settings.
3 Panel position: dock edge (reuse Settings dock fields) and keep on top.
4 Start with Windows + keep running in tray (reuse Settings fields).
Finishing or skipping sets `settings.onboarded = true` (serde default false; existing settings files without the field count as onboarded only if the library is non-empty). Shown on first run with an empty library; Help ▸ "Show the welcome tour again" reopens it. Seed a Getting started list only if `starter.rs` exists; else skip.
Tests: controller logic per step on the fixed clock; settings round-trip; a11y tree; one golden per step light (dark for step 1).
