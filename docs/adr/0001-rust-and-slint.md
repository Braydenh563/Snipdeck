# ADR-0001: Rust + Slint for the whole app

- Status: Accepted (2026-10-17)

## Context
Must be light (<40 MB idle), instant, and not a web app. Needs deep OS integration (clipboard, hotkeys, focus, input injection, capture, OCR) and a refined custom design.

## Decision
Rust for everything; **Slint** for UI (declarative `.slint`, GPU/software renderers, AccessKit, `@tr` i18n). Licensed under Slint's GPL-3 option for OSS builds; the royalty-free licence remains available if licensing ever changes.

## Considered
- Tauri 2 (+Svelte): best design ceiling, but ~40–80 MB RAM via WebView2 — rejected by owner.
- iced: flexible but theming/animation/a11y more manual.
- egui: dev-tool look.
- WinUI 3 / C#: Windows-only forever.

## Consequences
- No rich-text editor out of the box → rich text preserved on paste, edited as text in v1.
- Non-activating windows and IME need spikes (SPEC §13).
- Design system implemented as Slint globals + components.
