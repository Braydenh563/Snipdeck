# ADR-0002: Local-first storage in SQLite + blob folder

- Status: Accepted (2026-10-17)

## Decision
Single SQLite DB (WAL, FTS5) in the data folder; large payloads (images, long text > 64 KB, files) in a content-addressed `blobs/` folder (BLAKE3 hash). No account, no server, no telemetry. Portability via `.snipdeck` zip export. Data folder relocatable for DIY sync; single-writer lock file warns on concurrent use.

## Consequences
- Instant startup, trivial backup, open format.
- No built-in multi-device sync in v1.
