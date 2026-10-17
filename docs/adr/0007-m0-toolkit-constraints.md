# ADR-0007: Slint constraints from the M0 spikes

- Status: Accepted (2026-10-17); items marked unverified depend on the Windows test scripts

## Context
Spikes S1–S4 (`docs/spikes/`) tested whether Slint can carry a focus-safe, CJK-capable, high-performance Windows panel.

## Decision
Keep Slint, with these constraints:
1. Pin Slint exactly (`=1.18.x`) and confine unstable winit/fontique APIs to `snipdeck-platform`.
2. Use the software renderer on Windows by default (transparency bugs with Skia and OpenGL).
3. Use `ListView` for every long list; cap inline text editing at 16 KB and use a rope-based read-only viewer up to 5 MB.
4. Match title, tags and the first 64 characters in memory for the panel and picker; use SQLite FTS5 for full-content search in the main window.
5. Treat Mica as an enhancement over solid chrome; solid is the baseline.
6. Set CJK font fallback order per locale; commit IME composition before deactivating the panel.

## Consequences
- A second text component (the viewer) must be built and tested.
- If S1 or S2 fail on Windows, the fallback is a custom window adapter (S1) or an upstream patch (S2); see the spike notes.
