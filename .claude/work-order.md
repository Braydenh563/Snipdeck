mode: continue
status: active
max-continues: 80
max-idle-minutes: 60
finish-up-line: 97%   # owner: "until 97% weekly usage"
started: 2026-11-14 13:40

- [x] Push d0186e01 render merge + Store docs once gate --changed and 1.99.1 clippy pass
- [x] colour-tool (merged locally, push pending gate): worker tidy (tests below use super::*), merge PR head; re-verify, merge, gate, push
- [x] owner-bug-sweep: rounds 1-4 merged (aa4a232b, 92aecf09 parent), reviewed twice
- [x] store-ready: merged 92aecf09 after review fixes (22176105); follow-up store-ready-2: Recent question goldens + ux-auditor
- [x] Task 5 docs: CHANGELOG covers colour tool, preview, docking, render fix, quick add, reorder, Store (checked 2026-11-15); release dispatch [!] owner (needs main); gate --full + CI green moved to the final item
- [x] all-free (store worker, claude/all-free): Supporter removed, Buy Me a Coffee link, docs, Recent goldens, ux-auditor; review (no checkout), merge, gate, push
- [ ] Full-app audit: Opus pass done (12 fixed, 62c2063a..fc6cface + cleanup); Fable audit running (owner named it 2026-11-15) -> scratchpad/fable-audit.md; Sonnet fixes all findings; blind usability tests (Sonnet); gate, push
- [x] Push audit fixes, pushed 2026-11-15 (gate --changed running -> scratchpad/gate.log; tests no-fail-fast ok, 1.99.1 clippy ok, Windows clippy ok; owner workflow edits merged 34673454)
- [x] Merge origin/claude/all-free (3c2d9c44 + G1 fix e4c60f5f; gate2 running -> push) (reviewed MERGE-WITH-FIXES; workflow part done by owner) into the PR branch, resolve .po/CHANGELOG conflicts, gate, push
- [ ] claude/edge-polish (store worker session_01NKjURwKPmGuEHMfw1o6HF4): Store 10.8.2 notes, stale msgid, collapsed-edge restyle, residual drag flicker, eyedropper leaves docked Panel in collapsed look; review, merge, gate, push
- [ ] claude/popup-dismiss (bug-sweep worker session_01NKjURt5yHRvroJQUg7zQeb): toast close button, click-outside/Esc everywhere, More menu fills small Panel; review, merge, gate, push
- [ ] claude/ui-polish-2 (bug-sweep worker, after popup-dismiss): clear emoji/colour recents, tooltip z/clipping, tinted list icons, collapse button into sidebar header, search x alignment, scroll glitches; review, merge, gate, push
- [ ] Fable audit round 2 (scratchpad/fable-audit-2.md, G1-G27 + pass 2 running): fixers fix/fable2-data and fix/fable2-ui running; fix every finding (Sonnet agents, grouped by section), review, gate, push
- [ ] README, docs and screenshots refreshed for the final UI (after ui-polish-2): regenerate screenshots, check README feature list and user guide
- [ ] Open items from scratchpad/open-items.md: dead code (A26), CJK test silent skip -> explicit skip message, scroll-into-view for all colour-tool controls, HANDOVER rewrite (with docs refresh); eyedropper global hotkey, per-swatch Ctrl+S, OLE IDropTarget drops -> decide (build or record as post-1.0 in SPEC with reason)
- [ ] Blind usability tests (Sonnet) on the merged build; fix
- [ ] gate --full, CI green on PR #5, handover morning summary + retro

## State 2026-11-15 (for the next context)
- Pushed: 73df0541 (Windows CI green). Local, unpushed: audit fixes 62c2063a..fc6cface.
- Opus audit (scratchpad/audit.md, 12) fixed; a Sonnet agent is finishing its lint cleanup, then fixing the Fable audit (scratchpad/fable-audit.md, F1-F13: 1 high, 2 med, 10 low).
- Then: full gate (--no-fail-fast tests + clippy 1.99.1 + Windows-target clippy -p snipdeck-platform), push, merge claude/all-free when the store worker (session_01NKjURwKPmGuEHMfw1o6HF4) reports, blind usability tests (Sonnet), handover + retro.
- Disk: clean stale test binaries in /tmp/target-panel/debug/deps if under 2 GB free.
