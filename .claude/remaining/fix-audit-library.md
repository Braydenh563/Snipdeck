# fix-audit-library (coordinator): state

Branch claude/fix-audit-library holds everything: undo (26-28, 33-42, 46, 47), search (1-9, 50-54), tmpl (13-25), items (29, 31, 32,
44, 45, 49, 56), views (10-12, 43, 48, 55), nested tags, group nesting. All built; no agent is running.

## Done since the pause
- views merged; the Tab-order failure is fixed (see views.md); the `assert!(x.is_empty())` rewrite had dropped 36 semicolons and the
  head did not compile: fixed.
- Merging a tag now also gives the target tag to trashed carriers (`Change::LinkTag`/`UnlinkTag`, store `unlink_tag`), one undo step.
- The 10k search timing test is warm-up plus best-of-9.

## Open
- Not built, with reasons in SPEC 5.4 and HANDOVER (post-1.0): rope viewer (text over 16 KB is a read-only prefix), image editor,
  extra image formats, Replace on Image/File.
- Panel drawing of group colour and nesting (see views.md).
- Windows-unverified: GetLocaleInfoEx, {selection} copy, {key:}, clipboard history, CRLF, open link, and everything in
  `tools/windows-test/`.
- Before a push: `scripts/gate.sh --changed`, then `--full` once; `cargo deny check` and a Windows `cargo check` are CI-only here.
