# fix-audit-extras (branch claude/fix-audit-extras)

## Done (committed)
- EXTRAS-1 language applied after first component: `language::apply`, test `tests/language.rs`
- EXTRAS-6 log file `logs/snipdeck.log` + `warn!`, report tail: tests in `log.rs`, `headless.rs`
- EXTRAS-7 (part) release keeps line tables, string panics keep std wording, early panics queued, anchor line: tests in `crash.rs`
- EXTRAS-8 read-only store + `peek_settings`: tests in `headless.rs`, `store tests/open.rs`
- EXTRAS-11 `--version`, `--backup`, `--` values, usage name, `--dark` no longer stops saving, bad args in message box

## Next
EXTRAS-2, 3, 4, 5, 9, 10, 12, 13, 14, 15, 7 (SEH filter), not-reached files

## Blocked / for owner
- Toolchain 1.99.1: see gate notes
- .github/workflows edits (EXTRAS-2, 7 PDB artifact, 13 Pages, 14 toolchain pin): exact changes will be listed here
