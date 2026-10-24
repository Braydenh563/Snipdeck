# State: NOT ready to release (2026-10-21, owner decision)

The owner tested on Windows and is "not comfortable releasing this as the official release yet": the app needs
reliability and UX work first. PR #5 (`claude/compassionate-pascal-pod8ux`) stays OPEN; work continues there.
Do not merge, tag or run `release.yml`. Version and CHANGELOG already say 1.0.0; keep them, but fix the CHANGELOG
date at the real release. The owner capped this session at 96% weekly usage; the limit resets 2026-10-24T07:00Z.

## Session 2026-10-24 (owner away, autonomous; resume here)
- Owner order: 1 merge fix branches + verify; 2 Panel redesign: owner picks from mock-ups BEFORE building
  (canvas https://claude.ai/artifact/QV7HBDp4a1bGCRX5BRLgde: A Spotlight search-first, B Deck cards, C Flyout
  quiet list with visible Paste/Copy/Edit and a New/Shortcuts/Keep open/Open deck bar); 3 milestone below;
  4 ux-auditor + blind-tester; 5 ship prep only after owner go. Never merge/tag/release.
- Done: owner-test-ctl + owner-test-ui merged (595f503); goldens diffed old vs new and looked at: separators OK;
  regression fixed: use-copies hid the row's ⋯ menu instead of the Copy icon (0095d64).
- Reviewer findings on the merge (to fix): editor More-options toggle grows past the screen bottom (call
  fit_to_screen after set_height_fitted; also resize when title/privacy error opens More); set_height_fitted
  before first show may read width 0 (use size_before_show); Picker click-away close calls target.refocus()
  and steals focus back from the window the user clicked (skip refocus on focus-loss close); Ctrl+V test uses
  event.text == "v" (may be "\u{16}" on winit); Main lists other than Recent lack use-copies; tests missing for
  fit_when_settled / picker focus close / hotkey no-op.
- Diag agent (Opus) on branch claude/compassionate-pascal-pod8ux in /home/user/Snipdeck: SNIPDECK_DIAG render
  logging, tools/windows-test/diag-render.ps1, every hotkey failure reported, Paste next Supporter toast;
  analysis in docs/reviews/render-glitch-analysis.md. Lead works in worktree /home/user/wt-lead (branch lead),
  own target /tmp/target-lead; build snipdeck-ui with -j1 or -j2 (-j4 OOM-kills rustc). Rust 1.99.1 installed.

- Away queue (2026-10-24 night, owner asleep; one line per finished item below):
  1. Answer artifact comments (crushed text, type logos).
  2. Panel design v2: real design pass on D (icons per type/favicons, two-line rows that never crush the
     title, states: rest/hover/keyboard/search/empty/no target), critique by an independent agent, publish
     for the owner's morning pick. Do NOT build the Panel before approval.
  3. Non-Panel UX agent (shortcuts search, Ctrl+Shift+P, theme switch, density, shortcut audit, palette): gate, push.
  4. Alt+Tab to the Panel as a setting (Windows-only; owner tests).
  5. Picker placement setting relative to the caret (below/right/above/left, auto-flip).
  6. Dock fixes (covers a third of the screen, bottom invisible).
  7. Feedback audit: every action confirms or explains failure.
  8. Show/hide/move/resize predictability (Reset size then drag broke the list layout).
- Night log: D v2 published on the canvas (12 states light+dark, 3 variants); owner comments answered and resolved;
  ux-auditor critique running. Scout (for items 4-6): Alt+Tab already exists as settings.rs:525 panel_in_taskbar
  ("Show Snipdeck in the taskbar", APPWINDOW vs TOOLWINDOW, win_logic.rs:206) -> relabel to name Alt+Tab and test
  that the paste Target stays the previous app; Picker placement: placement.rs:40 place() only below/above, caret
  from GetGUIThreadInfo (target.rs:371) -> add PickerSide enum + left/right branch with flip/clamp, tests in
  placement.rs:270+; Dock: frame.rs:233 docked_rect, likely cause native_window.rs:146 work_area_of().unwrap_or(whole)
  (whole monitor when the Win32 read fails, taskbar None) -> keep last known work area; pure test bottom never past
  the work area.
- Night log 2: D v3 published (canvas v9). Reviewer on non-Panel UX diff 7460282..b0c4d49: shortcuts list misses
  Panel keys (Ctrl+A, F2, Shift+F10, PgUp/PgDn, Ctrl+Shift+Tab, F1, Shift+arrows); Row density is a no-op in the
  Panel; Ctrl+Shift+P gives no toast. One Sonnet agent is doing items A Alt+Tab label + Target test, B PickerSide,
  C Dock work area, D those review fixes (commits on this branch, not pushed; remaining file
  .claude/remaining/night-items.md). Next session: check that file, gate --changed, push, tick work-order items.

- To wire in the Panel build (after the redesign is approved): the row density setting. It is labelled "Main list
  density" now (Settings, View menu, palette) because the Panel does not read it: `panel.slint` `compact-rows` is
  never used. When the Panel reads it, rename the label back to "Row density".

## Read first in the next session
1. `docs/reviews/owner-test-2026-10-21.md`: the owner's test notes, verbatim.
2. `.claude/now.md` Inbox, entries dated 2026-10-21: the triage (T1-T13 for v1), every request and the
   owner's verdict, word for word.
3. Fix-agent branches from 2026-10-21 (see "Fix branches" below): review, merge into the release branch, gate.

## Owner's verdict (verbatim, 2026-10-21)
"it just needs a lot of visual refining and fixing and ui/ux upgrades and making more reliable to use. right now
it is annoying to access, annoying to hide or get rid of or move or resize. it visually glitches every second it
can get, there are a lot of lacking messages and confirmation, and the layout and buttons some places and how to
use it are still a bit unintuitive and I dont want to ahve to rememer command hotkeys, they should be boosts to
make thinsg that you can already do faster, not an actual feature. I should be able to do each thing easily well
and properly and still quite fast without it"

## Next milestone: reliability and UX (before 1.0)
Product goal (owner, verbatim 2026-10-21): "it needs to be a really handy and quick tool, not something I have to put
a lot of thought into using, is should be a quick flick open and close app and then with the occasional edit and
management in the larger window". Judge every change by it: the Panel/Picker open, paste and close in one quick
motion with no thinking; editing and management live in the Main window.
Owner, 2026-10-24: "i dont like the small panel design". Redesign the Panel before polishing it: propose 2-3
directions (mock-ups or goldens) and let the owner pick before building.
1. Render glitches on Windows (transparent patches, grey boxes when opening About/Edit, dragging the Panel,
   windows dragged back on screen stay transparent): find the root cause with a Windows diagnostic the owner runs;
   CI cannot see it.
2. Global hotkeys: owner reports only Ctrl+Shift+Space works; Ctrl+Alt+Space stops showing the Panel; hotkey
   must toggle (press again hides). A failed registration must be shown, never swallowed.
3. Picker: Esc and click-away do not close it (only picking an item does). Placement option relative to the
   caret (below/right/above/left), never covering the typed or next line; caret detection in Chrome/Electron/
   OneNote.
4. Panel: easy show/hide/move/resize; "Reset size and position" then drag made it full-height and broke the list
   layout; ⋯ menu taller than the Panel; quick-add links read as one sentence; shortcuts visible from the Panel;
   Ctrl+Shift+P for the keep-open pin.
5. Child windows (Tags, Edit) never off-screen; Edit window sized to content, More options collapsed.
6. Feedback: every action confirms or explains failure (e.g. pasting a screenshot into a new item did nothing).
7. Every hotkey action has a visible control; shortcuts list and command palette searchable.
8. Theme (System/Light/Dark) in the Main View menu and command palette; row density Compact/Comfortable.
9. Dock: covers a third of the screen with its bottom invisible; v1.1 wants a collapsed edge bar that expands on
   hover, and corner docking with quick switching.
10. Settings scroll feels laggy; command palette scrollbar covers items; two copy buttons; combobox keeps focus
    border; OneNote target detection flaky.
11. Then: ux-auditor + blind-tester on the build, reviewer on the diff, owner Windows retest, then ship prep
    (docs, screenshots, PR body, shipping guide; see `.claude/now.md` "Owner test gate" and ship-prep scope).

## Fix branches (2026-10-21, Sonnet agents; check their state, they may be partial)
- `claude/owner-test-ctl` (f1f5d5e, NOT merged; 7 targeted tests pass, gate/clippy/1.99.1 never completed):
  stale "Panel visible" while minimised/hidden made the hotkey hide instead of show (`panel_window_gone()`,
  test); unchanged chord re-register is a no-op; child windows re-fit 120 ms after show (`fit_when_settled`, all
  7 open sites); Ctrl+V of an image in the editor shows a translated toast; Picker retries activation after
  80 ms and closes on focus loss; Reset size/position insets 48 px so it no longer snaps to the dock.
  Open: why Ctrl+Alt+Space / Ctrl+Shift+2 / Ctrl+Alt+V all failed on the owner's PC (note: paste-next is
  Supporter-only by design; say so in the UI instead of doing nothing); only the first failed registration
  toasts, no Settings row error; render glitches (E) and OneNote (F) not investigated; Panel list overlap after
  resize not looked at. Details: `.claude/remaining/owner-test-ctl.md` on that branch.
- Next session, first step: merge both fix branches into the release branch, build, run all tests, LOOK at
  the regenerated goldens, gate, clippy stable + 1.99.1, reviewer, then owner retest on Windows.
- `claude/owner-test-ui` (WIP ab6b277, NOT merged, never built to a result): items 1-5 coded: quick-add
  Text/Link/Template segmented control spaced with dividers; Editor `fit-height` sized on open and on More options
  (test `the_editor_wants_a_shorter_window_with_more_options_closed_and_never_past_700`, not run); palette list
  reserves the scrollbar strip; row hides the Copy icon when the use label is "Copy" (`use-copies`; Main's "Use"
  label case not covered); empty state hidden while quick-add is open (image8); Settings click-away clears focus
  (ComboBox ring unmeasured). 30 goldens regenerated but not reviewed. Next: build, run tests, LOOK at the goldens,
  gate, clippy 1.99.1, review, merge. Not started: Panel shortcuts button (current bindings), shorter ⋯ menu
  (Help/View submenus or capped height), search box in the shortcuts list, Ctrl+Shift+P for the keep-open pin.
  Details: `.claude/remaining/owner-test-ui.md` on that branch.
- `claude/o47-abc-wip`: unbuilt paste/drop-to-create and Ctrl+C on items (v1.0.1). Keep.

## v1.0.1 (from `.claude/now.md`)
Compact dock and dock grips (U22/O28), sidebar resize (O54), header select and delete (O49/O50), paste or drop
to create an item and Ctrl+C (O47 a-c), Main quick-add, multi-row drag, toast Edit button, UX-C (O30/O26/O31),
U9, A25. Search-box suggestion dropdown in Picker, Panel and Main.

## v1.1
Optional typed-trigger expansion (off by default; honours Sensitive and the never-paste list), placeholders
`{uuid}` and date maths, paste transforms, importers (TextExpander, Espanso, PhraseExpress, Beeftext), richer
fill-ins, system-wide suggest-as-you-type at the caret (opt-in), groups inside a list (O46), tips window, rich
paste, multi-panel, O56 launcher, Windows Hello unlock, colour picker with eyedropper. Later: local AI paste
action (opt-in), folder sync, per-app rules, share a board, CLI get/paste.

---

# Now (2026-10-19 13:15, away mode on, stop at 80% weekly usage)
Goal: ship v1 (`docs/V1-PLAN.md` "V1 ship list" is the queue; `.claude/work-order.md` continue mode; decisions in `.claude/now.md`). Branch `claude/compassionate-pascal-pod8ux`, PR #5 (never merge). Repo public: CI (3-5 min) is the main test runner; local cargo only targeted, `/tmp/target-main`, at most two cargo users (4 cores).

State at 88b0db2: agents A1-A3 merged; clippy green; main_window add-list tests and panel menu tests fixed; panel sensitive-reveal fixed (panel_row masked the revealed row); CI at 19c9160: app green, only snipdeck-ui goldens red (D1). Owner applied the CI proposals to ci.yml and release-build.yml; Dependabot applied too; proposals folder deleted. CodeQL: 3 test-only alerts await owner dismissal ("Used in tests", PR comment posted); the cleartext one is fixed.

In flight: Sonnet agent C8+C9 (Supporter wording in app, FREE_BOARD_LIMIT 5, mailto SUPPORTER_EMAIL_PLACEHOLDER constant, Free vs Supporter table in Settings and the get-licence dialog) plus the owner decision: encryption on disk is FREE, only the master password is Supporter (Feature::SensitiveEncryption -> MasterPassword; flip the Free-does-not-encrypt tests; tables on site/README/docs).

Next, in order: CI green on PR #5 -> translations pass for ~80 new strings per locale (Haiku; `python3 scripts/i18n.py check`) -> D1 goldens regenerate + read -> wave 3 B1-B15 -> wave 4 C1-C7 -> D2 README shots/hero, D3, D5 -> E1 docs -> F1 Fable sweep (owner-approved, one agent; brief `docs/fable-brief.md`) -> F2 Opus review -> F3 ux-auditor + blind-tester -> F4 gate --full, PR body, retro. Use cloud sessions/agents in parallel only on separate branches; one cargo-heavy agent per target dir.

## Parallel work (orchestration ledger, 2026-10-19 14:25)
Hub = this session (`claude/compassionate-pascal-pod8ux`, PR #5). It owns the release branch, merges, `.po/.pot`, `CODEMAP.md`, `HANDOVER.md`, licence/Supporter wording, goldens (D1).
- 2026-10-20 11:35 hub state: weekly 84%, ceiling 89% (ship only). Fable = auditor only (pushing O33/O32, then .claude/remaining/fable.md spec list, idle). Hub agents running: claude/hub-o41-o48 (Sonnet), claude/hub-o53 (Sonnet), claude/hub-o52 (Opus medium, O52 + O39 Panel wiring). Next: merge each, Fable audits each range; then O45+O44 (Opus), O47 (Opus), O49, O50, O40, O36/O30/O27, visual pass (Sonnet); groups O46, O28, tips -> v1.1. Then ship prep (version 1.0.0, CHANGELOG, PR #5 body, Fable final audit).
- 2026-10-20 10:21 hub state: release tip ab0bdeb (Windows test wizard merged; CI green except known CodeQL 22). Weekly 82% (owner stop 87%; reset 2026-10-24 07:00). Owner decisions: Panel is the primary surface; v1 HELD until O46 groups land; Tips window and compact dock (O28) -> v1.1. Fable (session_01EXmSAQuDs7DQihmpZJLvwc, claude/v1-fable, merged ab0bdeb as 5c5e344) builds in order: O33, O32, O45+O47 (Panel essentials, paste/drop-to-create, undo/redo), O36, O40, O41, O44, visual pass (O25 O26 O31 O34 O35 O37 O38 O42 O43, one golden regen), O46 groups, O30, O27, O28. O29 done by Fable. Hub Sonnet agent on O39 (transparent window after Alt+Tab), branch claude/fix-o39-o29: verify gate, then merge to release. After Fable: verify, merge PR #27, reviewer + blind-tester, F4 ship prep. Owner G: Windows run, licence key secret, dismiss CodeQL, merge PR #5 + tag.
- 2026-10-20 09:45 hub: Supporter email + M3 workflow patch done (owner). Fable at 2ccfd1a (F4 done; O25-O27 queued; D5 merge pending). Sonnet agent: Windows test wizard UX on claude/windows-test-ux (hub verifies, merges). Then: merge PR #27, reviewer + blind-tester, F4 ship prep. Owner left: Windows run, licence key secret, dismiss CodeQL, merge PR #5 + tag.
- 0def478/35c8eee: D5 PR #28 merged (controller.rs 4252 -> 713, controller/*.rs; doc links fixed). Stop line now 87% weekly (owner). Fable PR #27 at 75cc0f8: app Groups 1-3 done; fourth audit U1-U21 (High: U1 Panel never activated for text entry on Windows, U2 text scale not applied, U3 silent backup failure) in progress with two Sonnet agents; then [D5] items (U6 U8 U9 A6 A7 A11b A13 A22 A23 A25 A27 B13 F2), B6 B5 B12 F6, owner extra checks. After Fable: independent Sonnet reviewer on Fable diff + blind-tester (owner asked), fixes, then F4 ship prep. Wrap-up rule: Morning summary lists done, cut short, every remaining item.
- ccea9db: PR #23 merged (security pass M1, M2, L1-L9; Recent encrypted at rest, schema v5; docs/reviews/security-v1.md has the owner M3 release.yml patch and the CodeQL items to dismiss), PR #24 (settings shot crop), PR #25 (controls/glyphs follow text scale, picker footer, tab carets, one "N selected", no empty gutter), PR #26 (Windows -shm test fix). Stray pyoxipng wheel removed, *.whl ignored. Full workspace tests green on the #23 merge; app+ui green on #25. Open: 3a D5 controller split (claude/v1-wave3a-d5). Next: F1 Fable (notify owner, wait for go), F3, F2, F4.
- f0c10c2 PR #22 merged (D2: 16 README shots at 2x light+dark, site/img refreshed, lossless + oxipng; docs/images 2.1 MB). Known: settings shot cut mid-row; editor Sensitive toggle differs light/dark (fixture). Open: 3c security, 3b scale.
- e07ddea PR #20 merged (B2 Import source); caf1222 PR #21 merged (D1 goldens regenerated, D3 de-slop; snipdeck-ui 25 ok). e07ddea CI red only in goldens, fixed by caf1222. Open: 3c security, 3b scale, 3a shots (D2).
- a4b87c3 PR #18 merged (Move up/down + Panel Alt+Up/Down, WCAG 2.5.7; B11 [x]). CI red only in editors goldens. Open at 03:00: 3b claude/v1-wave3b-keys (Main Alt+Up/Down, card double-click per SPEC), 3c claude/v1-security, 3a claude/v1-wave3a-b2. Next after 3c: D1 goldens.
- 398d5d8 PR #17 merged (E1 docs; B2 back to [~]: platform reader has no UI caller). e9aacfe: flaky unlock_window worker test fixed (Slint queues a quit when the last window hides outside a loop; the test now runs the loop until the answer is applied; 7/40 -> 0/80). Open at 02:16: 3b claude/v1-wave3b-move (WCAG 2.5.7), 3c claude/v1-security (+ Recent entries encrypted at rest, privacy doc), 3a claude/v1-wave3a-b2 (B2 as an Import source). Then D1 goldens. Weekly 75%.
- ce59a53 PR #15 merged (i18n.py plurals, all 6 locales complete). 10637d4 PR #16 merged (Panel row reorder, Main tile drag). Open: 3b claude/v1-wave3b-move (WCAG 2.5.7 Move up/down + Alt+Up/Down), 3c claude/v1-security, 3a claude/v1-wave3a-docs (E1 refresh). Next: D1 goldens. Weekly 75% at 01:40.
- 29c3e8d PR #8 merged (B1, B2, B12, C1 base; apply_import lock guard + test). ecaaca2 E1 docs merged (guide TODOs for B1/B2/C1 now stale: refresh in the final E1 pass). 205ec8a B15. Open: B8 Ctrl+D (3a polish), C1 gaps (3c recent), B11+C4 (3b), 3a polish (Argon2 off UI thread). licensing.md "For developers" section holds licence internals: owner decision.
- CodeQL (first run on 5776195): 28 critical alerts, list not readable from the session (no code-scanning API). Likely Rust hard-coded-crypto-value hits on cfg(test) keys ([7;32], DEVELOPMENT_SEED, test passwords): licence testing.rs and test signers are cfg(test)-gated. F security step must confirm each; owner can view Security > Code scanning. 1.99 clippy assert_is_empty fix in master_password.rs:301 rides the B15 merge.
- 675bc75: Windows (check) green; Linux red only in goldens (D1). CodeQL 28-alert PR comment posted (F security step confirms each).
- df25d73: PR #11 merged (C4 colour label + list icon as schema V4 after Recent V3; B11 controller drag ops). Gate + 1.99.1 clippy green; tests: only the 7 golden binaries fail. PR #12 (C1 gaps) has reviewer findings 1-7 out to 3c; hub decisions: Recent hidden + capture paused while locked; Clear recent asks to confirm.
- Open: PR #12 (3c, C1 gaps): reviewer findings 1-7 being fixed. PR #13 (3a, Ctrl+D, Duplicate icon, Argon2 off UI thread): reviewer findings 1-7 sent (Panel Ctrl+D on Recent tab, stale job after lock/cancel, Set exists path, worker-failure tests, busy a11y); then toast i18n via toast kind mapped to @tr in Slint, and Duplicate palette command. 3b: B11 UI on wave3b. Next after merges: D1 goldens, D2, D3, D5, E1 refresh, translations, then the Fable pass in a NEW session.
- PR #14 merged (B11 drag and drop): gate + 1.99.1 clippy green; only goldens fail. Owner (2026-10-20): push-notify when the Fable pass is reached and wait for their go.
- PR #13 merged (4fe8f19: Ctrl+D, Duplicate icon, Argon2 off UI thread with stale-job guard, toast i18n via toast_texts.slint, Duplicate palette command). PR #12 merged (C1 gaps, locked Recent hidden, Clear confirm, roving focus); conflict fix: finish_master Unlock now calls sync_recent_gate. Security pass (3c, claude/v1-security): no High/Critical; M1 import sensitive->plain, M2 CopyQ memory bomb, lows L1-L9; M3 is a .github patch for the owner. Mac port estimate: ~5.2k Windows-only lines, 3-5k new lines; no notarization without the Apple fee (v1.x idea, not v1).
- 5776195 merged C3 master password (review fixes 3141d6d: sensitive editor closes on idle lock, backup/export/restore refuse while locked, KeyUnavailable state). C3 minors left for F: Argon2 on UI thread (brief freeze, no progress text); no unlock round-trip before UPDATE; toast strings in wording.rs not @tr. 02dacb4 PR #9 done (C5, C6, C2 + badge).
- 327d145 merged 3b B5+B7 (gate ok, app/deck/panel tests ok). C3 agent running in wt-c3. 3b on B11+C4; 3a on C2; 3c on folder picker.
- Wave 3a cloud session `session_01Haxw5UhdjTPvNo4KAUcPWG`, branch `claude/v1-wave3a`: B3, B6, B9, B13 done -> PR #6 (hub merges). It correctly refuses extra work sent by message (no owner authority), so follow-up queues go to NEW sessions whose first prompt carries the task. Owner (16:0x) is authorising the sessions directly: 3a next = C5, C6, C2 (after #6 merges).
- Wave 3c cloud session `session_01U6UTKpv5YUhk3MdMBGxHoh`, branch `claude/v1-wave3c` (started 16:00): B1, B2, B12, C1.
- Wave 3b cloud session `session_016pPMHPMEKcG9VuaY2dcBbm`, branch `claude/v1-wave3b`: B4, B8, B10, B14 -> PR. Next (owner authorising): B5, B7 + L7, B11, C4 on the same branch.
- Landed on the release branch (19 Oct): wave 3a PR #6 (B3, B6, B9, B13) as 49d7be3; review-fixes-2 (11 Opus findings) as f38375b + adb9fba (rustdoc private links fixed); C8/C9 Supporter rename, 5 free boards, free encryption, Get Supporter dialog, plan table (662b886, fad6d50, 6d2500e).
- Open PRs into the release branch: #7 wave 3b (B4, B8, B10, B14), #8 wave 3c part 1 (dry-run import, B2).
- Hub local agents: hidden-test-fixes (branch claude/hidden-test-fixes; the editors golden binary stops CI's cargo test, hiding 6 editors_logic + 1 panel_logic failures and a search proptest bug: highlight indices out of range for "0\U00011CA9$"), Opus review of C8/C9. Next: translations (Haiku), C3 master password, B15, leftovers L1-L6 and L8.
- Disk: the container ran out (19 Oct 16:44); keep one target dir per agent, /tmp/target-fix is 9 GB.
- Cloud PRs target the release branch, never main. Hub merges in landing order (merge commits, no rebase), runs CI, then tells the sessions; they merge the release branch back before their next PR.
- After all UI items land: D1 goldens once, D2 shots, D3, D5, E1, then F1-F4.
- Full assignment of the V1 ship list (every open item has an owner):
  - Wave 3a: B3, B6, B9, B13, B1, B2, B12, C1.
  - Wave 3b: B4, B8, B10, B14, B5, B7, B11, C4.
  - Hub: A1-A3 (confirm CI), C8, C9 (agent finishing), translations, C5, C3, C2, C6, C7 manifest template, B15, open review leftovers (Haiku audit of `docs/reviews/*.md` and the now.md Inbox; leftovers go to the hub), D1-D5, E1, F1-F4.
  - Owner: E3, G1-G4.
  - Done: A4, C7 docs, D4, E2.
  - Leftovers from the review-item audit (619a087), all hub unless noted:
    - L1: CJK test silent skip, make it `#[ignore]` with a reason (PR4 #11).
    - L2: `cursor_from_end` not applied (PR4 #13); the 120 untranslated entries per locale go to translations.
    - L3: `cargo deny` does not cover the m2-harness Cargo.lock, and the m3-app step is stale (PR4 #14).
    - L4: split the five files over 800 lines (H3: windows/clipboard.rs, template.rs, import.rs, blobs.rs, chord.rs), with D5.
    - L5: key provisioning now that encryption is free (S3), with C3.
    - L6: the launcher pins the release zip by tag plus SHA-256, not `releases/latest` (S7).
    - L7: More dropdown closes on pointer move (Inbox 13:1x). Wave 3b, with B7.
    - L8: icon-name audit, meatball vs kebab etc. (Inbox 13:3x), with B15.
  - Opus review of the merges (11 findings): agent fixing on `claude/review-fixes-2`; hub merges.
- Before ship (owner 2026-10-19): a security scan (claude-security or the security-reviewer over the whole diff) plus F2, with every bug and security finding fixed; then update the PR title and body (interim body set 2026-10-19).
- Release workflow (owner 2026-10-19, modelled on Tessellate): applied by the owner (1e9726e): release.yml (tag or manual), secrets.yml (gitleaks), release-build.yml dev-only. The smoke step needs B13 `--check-db` merged before the tag. Ship step: bump [workspace.package] version to 1.0.0 and rename the CHANGELOG header.
- F-phase order (owner 2026-10-19): after every A-E item lands, F1 Fable pass + audit runs in a NEW cloud session (fresh context, brief `docs/fable-brief.md`), findings in `docs/reviews/fable-v1.md`. The hub then implements EVERY flagged issue (split across the wave sessions by area, no finding dropped or deferred without the owner), then a second polish pass (D3 de-slop + F3 ux-auditor and blind-tester), then F2 Opus review, then F4 gate and ship.

## Task list (mirror of the session's tasks, 2026-10-19 13:20; details per item in `docs/V1-PLAN.md`)

- [~] A1-A3 Merge review, windows-ui, learnability agents and gate (merged; CI fixes 844a0bd..19c9160; confirm green)
- [~] C8 Supporter wording, Free 5 boards, Get Supporter mailto placeholder (agent in flight)
- [~] C9 Free vs Supporter table: Settings, get-licence dialog, README, site (agent in flight)
- [~] Encryption on disk free, master password Supporter (agent in flight; decision in now.md)
- [ ] Translations: ~80 new strings per locale (`python3 scripts/i18n.py check`)
- [ ] D1-D2 Regenerate goldens (read each PNG), README 2x shots and hero images. CI at 19c9160: snipdeck-app green; snipdeck-ui screenshots red: 18 goldens differ (form controls, item editor, list editor, settings, tag manager, onboarding) and help-about/help-troubleshoot goldens are missing. Regenerate AFTER the C8/C9 agent lands (it changes Settings and the editors), `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test editors`, read every changed PNG before committing.
- [ ] B1 Import (.snipdeck, Ditto, CopyQ, folder) with preview, also for Restore
- [ ] B2 Windows clipboard-history importer
- [ ] B3 Export a list as text
- [ ] B4 Ctrl+Z undo; Undo toast >=5 s, pause on hover
- [ ] B5 Getting started list on first run
- [ ] B6 Duplicate item
- [ ] B7 More (...) button on rows and cards
- [ ] B8 Shortcuts Ctrl+N/Shift+N/F/D/W/,
- [ ] B9 Tray double-click opens Main
- [ ] B10 Reduced-motion setting
- [ ] B11 Drag and drop (reorder, move to tab, drop to create)
- [ ] B12 Perf: cached index, budget tests, size/RAM report
- [ ] B13 CLI --check-db and --troubleshoot
- [ ] B14 Sensitive clipboard clear setting + SPEC fix
- [ ] B15 Wording and a11y leftovers (ux-audit list)
- [ ] C1 Recent inbox (opt-in clipboard history)
- [ ] C2 Paste stack (Supporter)
- [x] C3 Master password (Supporter): set/change/remove in Settings > Privacy, unlock window, 15 min idle lock; Windows behaviour unverified
- [ ] C4 Item colour label and list icon
- [ ] C5 Accent colour (Index Blue default, presets, Windows accent), HC, 225%
- [ ] C6 Edge dock ships: settings and wizard steps checked
- [x] C7 MSIX packaging and signing docs (docs/packaging.md, 58e0d1e; code gap: Store build needs the WinRT StartupTask API for autostart)
- [ ] D3 De-slop pass over every window state
- [ ] D5 Controller split under 800 lines
- [ ] E1 Docs: user guide, CHANGELOG, privacy, licensing, HANDOVER, CODEMAP
- [ ] F1 Fable critical sweep (one agent, owner-approved; `docs/fable-brief.md`)
- [ ] F2 Opus review of the whole diff; fixes
- [ ] F3 ux-auditor and blind-tester on the final build; fixes
- [ ] F4 gate --full both toolchains, Windows check, deny, CI green, PR body, retro
- [x] CI proposals applied by the owner (dependabot.yml too); `docs/ci-proposals/` deleted
- Done: D4 Pages site, E2 pre-public sweep, keygen rename, CI proposals applied by owner (ci.yml, release-build.yml), CodeQL cleartext fix.
- Owner (G): dismiss 3 test-only CodeQL alerts ("Used in tests"); supply the Supporter email; Windows test run; merge PR #5; tag v1.0.0; release SHA-256 in `tools/launcher/release-sha256.txt`; SignPath/Store later.

# Session state (2026-10-18 13:50, Fable session, away mode; branch `claude/compassionate-pascal-pod8ux`, PR #5)

- **This session runs on Fable** and is the one permitted Fable agent; it delegates implementation to Sonnet/Haiku. Queue: `.claude/work-order.md` (continue mode). Decisions and inbox: `.claude/now.md`. Plan with per-step status: `docs/V1-PLAN.md`.
- **Merged into the branch (CI green on 8dc9fd0):** wave 1 (main-window branch, pin icon, Panel UI: kebab menu, tooltips, resize grips, quick-edit hooks, pinned strip), Main window wiring (MainModel, every ItemAction, Trash + 5 s undo, list/board CRUD + reorder, editors), translations complete for 6 locales (review list `docs/reviews/translation-review.md`), launcher step 17 (prebuilt zip; `Snipdeck.bat build` is developer-only), static MSVC CRT, release zip layout (`Snipdeck.bat` + `tools/launcher` + `app/snipdeck.exe`), keygen tool, workflows (ci, audit, release-build, dependabot), DESIGN icon vocabulary.
- **Now (22:58, after an OOM container restart):** two Sonnet agents resumed with their WIP committed: step 8 ItemEditor (worktree agent-editor, head 998a4c5, /tmp/target-main) and step 5 Picker + Fill + toast windows (worktree agent-picker, head f657895, /tmp/target-b, debuginfo off). Branch head b196b43+ holds unpushed merges: step 7 Settings (256e2a8) and security fixes 1-11,13 (4e73f6d); their workspace gate was not finished (fmt, clippy, type-check and i18n green; tests, doc, 1.99.1 clippy pending). Next: when an agent reports, merge, run ONE gate in /tmp/target-main (at most two cargo builds at once on this 16 GB box), push, tick the work order, then steps 7a, 13, 7b, 11b/11, 13a, 19, 20. Owner checklists: launcher `Snipdeck.bat selftest`, tray/resize/Settings on Windows, secret + v1.0.0 tag.
- **Step 4 leftovers (Windows-unverified, owner checklist in `.claude/remaining/panel-binding.md`):** OS drag-resize with the 260..480 clamp, Resized/Moved events reaching `frame_changed`, `set_position` on restore, tray Pause check mark. Known gaps: work area ignores the taskbar until a `GetMonitorInfo` query is added; row context menus and `reveal-requested` unbound; Panel delete repeats the 6-line Trash call (`trash_items` is private to main_ctl.rs); menu text in `wording.rs` is English only; `MenuEntry` has no `checked` field for screen readers. The step 8 editor agent must also wire the crash toast's "Open the log?" action to `crash::open_log_folder()` and replace the `hotkey_problems(&[])` redraw nudge in `start_maintenance`.
- **Owner's Windows checklist (unverified here):** launcher 9 steps in `.claude/remaining/launcher.md`; tray and resize after steps 4/9 merge; GitHub settings in item 11a below; first release needs the `SNIPDECK_LICENCE_PUBLIC_KEY` secret and a `v1.0.0` tag.
- **Post-v1 (dropped for the ship date):** item colour label + list icon, accent colour + HC, edge dock, Microsoft Store (MSIX, US$19 individual account), GitHub Pages site, code signing (SignPath Foundation once public, or Azure Trusted Signing).

# Morning summary (2026-10-18)

- **Done (PR #4, CI running on daca819):** M2 Windows layer, M3 UI library + Panel + tray/settings/theme, Picker/fill UI + logic (no window binding yet), store batch API, 9 security + 13 Win32 + UI review fixes, final PR review fixes #1/#3/#4/#6, README/docs/logo, one-click launcher `Snipdeck.bat` (works on the owner's PC up to preflight).
- **Not merged:** Main window + editors + Settings page (branch `claude/snipdeck-main-window`, gate unconfirmed); launcher self-test never run.
- **Blocked on the owner:** all Windows behaviour is unverified except the owner's first demo run: see "Owner's first real Windows run" below (item 0: Panel buttons and board dropdown dead = top priority).
- **Progress:** about 55% of a v1 (M1 done, M2/M3 mostly, M4 half, M5 ~10%, M6 not started).
- **Next:** item 0 regression with tracing, then the owner's list 1-9, review doc `[ ]` items, Main window PR, translations, M4 remainder, M5, M6, then the one permitted Fable pass.
- **Start a new session:** "read docs/HANDOVER.md and continue" on branch `claude/compassionate-pascal-pod8ux` (do not merge PR #4 unasked).

# Handover (2026-10-17)

Read this first in a new session, then `docs/PLAN.md`, `docs/SPEC.md`, `DESIGN.md`.

## Resume after /clear (written 2026-10-19 02:20)
A fresh session continues from here. Do, in order:
1. `scripts/orient.sh`, then read `.claude/now.md` (queue, decisions, inbox) and `.claude/work-order.md`.
2. Subscribe to PR activity for Braydenh563/Snipdeck#5 (branch `claude/compassionate-pascal-pod8ux`, CI green on dd18b23 after a runner-loss re-run).
3. Agent branches in `.claude/worktrees/` (each has `.claude/remaining/<name>.md`): `agent-editor` (step 8 item editor, committed), `agent-picker` (step 5 Picker/Fill/toast), `agent-dock` (step 3a edge dock), `agent-ux` (step 13 per `.claude/remaining/step13-plan.md`). For each: commit any leftover changes on its branch, finish build/tests with one agent per target dir (/tmp/target-main, -b, -c; max three builds), merge into the PR branch, run the gate, push, tick the work order.
4. Merge order: editor, dock, picker (step 13 item 7 toast needs it), ux.
5. Then the rest of the work order: 7a onboarding, 7b Help & About (repo/Store links), 11b, 11, 13a, ship-bar control inventory (Haiku), ux-auditor + blind-tester, 19 docs, 20 reviewer, gate --full, PR body, morning summary, retro.
Rules: Haiku for grunt work with a runnable check, Sonnet implements, Opus only after a lower rung fails; budget line 60% weekly, stop at 65%; never merge the PR.

## Where things stand

- **Repo:** `Braydenh563/Snipdeck`. PR #1, #2 (M0/M1) and #3 (lean workflow) are merged into `main`.
- **Product:** Snipdeck, a Windows-first sticky-note-style snippet deck in Rust + Slint. Decisions are in
  `docs/SPEC.md` (grill rounds 1-2), `docs/adr/0001-0007`, `DESIGN.md` ("calm, tactile deck": warm neutrals,
  Index Blue for selection only, ink primary buttons, IBM Plex, number gutter).
- **M0 spikes:** first real Windows run 2026-10-17 (Windows 11 only, quick mode): `docs/spikes/windows-results-20261017.md`.
  **S1 = GO** (focus kept; paste into Notepad/Chrome; Shift copy-only; elevated fallback; password fields).
  S1 fix needed: Alt+click opens the target's menu hints and releasing Win opens Start (step 2c).
  S2 is **inconclusive** (no IME was installed, so nothing could be composed). S4: DWM calls ok, theme switching ok,
  but OS caption buttons overlap the app's own close button. S3 measured on Linux. Constraints: `docs/spikes/README.md`.
- **M1 (core, store, licence, crypto, portability): done and merged into the PR branch.** 488 tests, fmt/clippy/doc/deny
  clean on Rust 1.97 and 1.99.1, Windows target check passes. Opus review in `docs/reviews/M1-review.md`; all
  29 core/store findings fixed. Known gaps: `docs/PLAN.md` "M1 known gaps" (CopyQ `.cpq` not parsed, Windows clipboard-history importer, `DataKey` not `Clone`).
- **Windows test wizard:** `Test Snipdeck on Windows.bat` (repo root) -> `tools/windows-test/` (manual steps in
  `tests/*.json`, automation in `auto/`, flow logic in `flow.ps1`). Press Enter for the "v1 check" (steps flagged
  `"v1": true`: M3, M2, S1 and S2 key steps); it ends with `Snipdeck-test-results-<date>.zip` on the Desktop. Neither
  the new flow nor the automation has run on Windows; the unverified assumptions are listed at the end of
  `tools/windows-test/README.md`.
- **CI** (`.github/workflows/ci.yml`): Linux fmt/clippy/test/doc/deny + Windows `cargo check`. It uses the
  newest stable Rust, which has lints that older Rust lacks: always check with `cargo +1.99.1 ...` too.

## What is blocked on the owner

- Optional re-run on Windows: S2 after adding Japanese / Chinese / Korean keyboards (`Test Snipdeck on Windows.bat`, Advanced options, S2 only),
  S1 step 2c after the modifier fix, and the untested S4 steps (W4-W7, Windows 10). None of these gate M2 start.

## Overnight queue (away mode, started 2026-10-17)

Goal: advance M2 -> M3 -> M4 logic as far as Linux can verify; branch `claude/compassionate-pascal-pod8ux`, PR #4 (open one PR per 3-5 items; never merge).
1. Land the three M2 backend agents' work (verify on Rust stable + 1.99.1 + Windows target, Opus `reviewer` on the unsafe code, commit, push).
2. M2 test harness example + `tools/windows-test/tests/m2-*.json` wizard steps (incl. S1 2c re-run, UIA OneNote/Terminal, UIPI).
3. Pin Slint, add `snipdeck-ui`: `tokens.slint` generated from `design/tokens/palette.json`, component library (DESIGN.md), software renderer, screenshot tests under the Slint testing backend (our "end-to-end" on Linux).
4. Panel UI (ListView, number gutter, tabs scroll at large text) wired to `snipdeck-platform` traits with fakes; `snipdeck-app` skeleton (single instance, hotkeys, tray crate decision).
5. i18n scaffolding (`@tr`, 7 locales extracted), CJK font order, accessibility properties on every control.
6. Picker, Main window, editors, template fill form (logic + screenshot tests).
7. Security/tests pass: fuzz/proptest on parsers, cargo deny, review of crypto/DPAPI wiring.

Not verifiable here (stays unverified until the owner runs the wizard on Windows): every Win32 behaviour, IME, Mica, DPI, Narrator, real-app paste. Playwright is for browsers; Slint apps are tested with the Slint testing backend plus software-renderer screenshots.

- **Try the app (Windows):** double-click `run-snipdeck-demo.bat` (needs Rust). It builds and opens `snipdeck-app --demo` on a seeded in-memory library. Unverified on Windows until the owner runs it.

## State (end of M2, M3 under way; branch claude/compassionate-pascal-pod8ux, PR #4)

- **M2 landed** (`snipdeck-platform`): portable logic + Windows backend (clipboard with delayed render, keyboard, Target
  tracking + UIA, hotkeys, autostart, single instance, DPAPI, panel style) + `tools/windows-test/m2-harness` and 19 wizard
  steps in `tests/m2-platform.json`. Opus review done, fixes applied. All Win32 behaviour **unverified** until the wizard runs.
  Windows check from Linux: `cargo clippy -p snipdeck-platform --all-targets --target x86_64-pc-windows-msvc --locked`.
- **`snipdeck-ui`** (M3): tokens generated from `design/tokens/palette.json` by `build.rs`, `Theme`/`Type`/`Space`/`Radius`/`Motion`,
  Phosphor icons (`scripts/ui_icons.py`), IBM Plex bundled, Button, headless harness (`src/testing.rs`: software renderer,
  frozen clock, golden PNGs in `tests/golden/`, `UPDATE_GOLDEN=1`), accessibility-tree tests via `i-slint-backend-testing`,
  bundled translations (7 locales, `scripts/i18n.py extract|check`, wired into the gate). Conventions: `crates/snipdeck-ui/README.md`.
  Component library merged (primitives, inputs/navigation, deck; ~150 tests). Opus review findings fixed (`docs/reviews/M3-ui-components-review.md`).
- **Store batch API** landed: `Store::transaction` / `Batch` (savepoints, rollback incl. blobs); import is atomic via `Library::atomically`.
- **`snipdeck-app`** (skeleton runs): `usecase` (use an item: content -> template -> pipeline -> count use),
  `panel` (view-model), `toast` (messages), `settings` (tolerant load, chord rules), `controller` (Slint-free: hotkeys,
  Target changes, use/copy, template prompt state, toast lifecycle, `Effect` list), `binding` (`Controller::bind(this, window, on_effect)`:
  callbacks in, changed properties out), `host` (Clock / TargetSource / HotkeyBackend traits + fakes), `platform` (Windows services, or fakes
  off Windows), `demo` (seeded library), `screenshot`, `main.rs` (args, single instance, hotkeys, software renderer, event loop).
  Tests: 89 unit + 13 headless Panel window tests (`tests/panel_window.rs`) + 2 screenshot tests. Run `cargo run -p snipdeck-app -- --demo`
  (dev window on fakes) or `-- --screenshot out.png [--dark]` (no display needed). Windows check from Linux needs a stand-in
  archiver for the C build scripts: `CC_x86_64_pc_windows_msvc=true AR_x86_64_pc_windows_msvc=<script that touches the output .lib>`.
  The `cfg(windows)` code type-checks and passes clippy for the Windows target; it has **never run** on Windows.
- App shell (2026-10-18): `Controller::on_tray`, `set_system_look`, `update_settings`, `close`; `Binding::tray` / `system_look`;
  `Host` has `tray`, `warnings`, `keep_alive` (was `guard`). Wizard: `tools/windows-test/tests/m3-app.json`.
- Decisions: ADR-0008 (OS caption buttons on the main window, own chrome on the panel, Win32 tray in the platform crate).
  Default hotkeys: Ctrl+Alt+Space toggles the Panel (new, not in SPEC), Ctrl+Shift+Space Picker, capture (no default chord, off until set), Ctrl+Alt+V paste next (registered with Supporter only).
- CI note: Slint pulls `fontique` -> `yeslogic-fontconfig-sys`; `snipdeck-ui` enables `i-slint-common/fontconfig-dlopen` so CI needs no
  `libfontconfig-dev`. Debug `target/` for snipdeck-ui is about 4 GB: keep few copies, `rm -rf` old ones.

## Owner's first real Windows run (2026-10-18, `Snipdeck demo` build) - fix these first

0. **TOP PRIORITY, a regression:** the Panel buttons stopped working except Close, and the board switcher dropdown no longer rendered or worked (owner report, later 2026-10-18; earlier the dropdown worked). Hypotheses, in order: (a) the Win32 hardening (`windows/dpi.rs` per-monitor DPI awareness set in `platform::connect` before any window exists, tray window thread DPI context) makes Slint's pointer coordinates and its scale factor disagree, so clicks land elsewhere and popups are drawn outside the clipped window (X still works because it sits at the window edge) - test by running with DPI awareness off and at 100%/150%; (b) `install_panel_style` / non-activating style (WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW) breaks Slint `PopupWindow` (popups need their own HWND; check the winit/Slint popup path) and mouse activation; (c) the `Controller` dropping callbacks (frozen target / dialog-open state added with the Picker logic, `toggle` state out of sync after a close). Plan: add `SNIPDECK_LOG=1` tracing of every UI callback, controller effect and pointer position to the launcher log, add a wizard step that records which of the 9 clicks reach the controller, reproduce, then fix and add a headless regression test (click each Panel control with the testing backend and assert the controller saw it; include the board-switcher popup open and a popup item click).
   **Analysis (2026-10-18, panel-clicks agent):** the code path is not broken in the demo: new headless tests click every header control, the board menu and a menu item, a tab and a pinned chip with real pointer events (`tests/panel_window.rs` `pointer_clicks_*`) and pass. (a) is ruled out: the owner's first run, where the dropdown worked, already had 211068d, and winit sets the very same per-monitor-v2 context itself (`become_dpi_aware`). (b): the board menu is not a `PopupWindow`, it is drawn inside the Panel (`panel.slint` end). (c): no controller guard drops callbacks. What changed between the two runs is the launcher: double-clicking `Snipdeck.bat` (no command) runs `run`, the **real library**, which on a new PC is empty: no board, so the switcher is blank, its menu has no entries, there are no tabs, pinned chips or rows, and the empty state's "New list" / "Save from clipboard" and the `+` tab were not wired; only X did anything. Fixed: a library without boards gets "My snippets" / "Snippets" at launch (`starter.rs`), `+` and "New list" add a list, "Save from clipboard" runs quick capture. `SNIPDECK_LOG=1` logs every callback, effect, tray/hotkey event, dropped cross-thread event and pointer press (physical, logical, scale) to stderr (the launcher's `app-*.err.txt`). Wizard steps M3 18 (demo, click each control) and 19 (first run) check it on Windows. If the owner did run `Snipdeck.bat demo`, step 18's log tells which clicks reach the window.

The demo ran on Windows (first time ever). Owner's report (summarised), plus a first guess at the cause:

1. **Cannot minimise the Panel.** Own chrome has only close/hide. Add minimise/hide and a way to reopen (tray, hotkey).
2. **Pinned-badge row scrolls only with the trackpad**, not mouse wheel or touch. Needs wheel (and Shift+wheel), drag/touch flick, visible scroll affordance (edge fade or chevrons) or a different layout (wrap, or a "more" overflow). Component: `pinned-strip.slint`.
3. **The `+` button in the tab row does nothing** (add list not wired; no feedback). Wire it to the list editor once the Main window lands, or hide it until then. (Stale: it now adds a list, `Controller::add_list`.)
4. **No help or tooltips; unclear how to use it.** Add tooltips on every icon button, a first-run hint/coach marks, an empty-state explaining pin/paste/shift-click, a Help entry (links to docs/user-guide.md).
5. **Top item is cut off for a moment when clicked** (selection/focus ring clipped by the ListView; owner says low priority).
6. **No way to open the larger window; no logo in the Panel header; no kebab/meatball menu** for more options and settings. Add header: logo, board switcher, search, kebab menu (Open Snipdeck, Settings, Pause hotkeys, Dock, Help, About, Quit).
7. **The pin button does nothing.** Define it (keep Panel open/always-on-top vs pin item) and wire it, or remove it.
8. **Not shown in the taskbar (by design: skip-taskbar), shown in the tray, but none of the tray menu items work** (Open Panel, Open Snipdeck, Pause hotkeys, Quit). The menu itself appears. Suspect the tray-to-controller event path (`TrayEvent` handling in `main.rs` effect handler, `invoke_from_event_loop` before the backend is selected, review finding 10) or the Win32 fixes of 2026-10-18 (IconPresent, WM_CLOSE mapping). Add logging of every tray command and reproduce with `tools/windows-test` m3 wizard steps. Consider showing in the taskbar when the main window is open.
8b. **No quick edit of items in the Panel** (owner, 2026-10-18): add inline quick edit (Ctrl+E or double-click / context menu on a row: edit text, rename, move, pin, delete, with undo) using the ItemEditor from the Main window branch. SPEC has Ctrl+E; no handler exists.
8a. **The Panel cannot be resized** (owner, 2026-10-18). Add edge/corner resize (OS resize border or own grips), min/max size, remember size per monitor (SPEC: per-monitor position memory); works with the non-activating style and docking.
9. **Slow build every time the demo launcher runs.** The launcher agent (a375ca0) builds `Snipdeck.bat` with a stamped exe cache in `dist\`; until it is merged each launch runs `cargo run` (an up-to-date cargo run is still seconds-slow and rebuilds when sources change).

## Next PR: drive the app to shipping (owner, 2026-10-18)

The owner confirmed the Panel buttons work now. Missing utility the owner listed: delete and edit lists, open the larger window and its menu, manage snippets and list items, more. Plan for the next PR(s), in order: (1) merge `claude/snipdeck-main-window` and wire it (items 1-3 below), with list/board/snippet CRUD, Trash, undo, Panel kebab menu, resize, quick edit; (2) Windows verification loop with the owner using `Snipdeck.bat` and the wizard; (3) M5/M6 essentials for a first public release (licence UI, onboarding, backups, installer/prebuilt zip, signing later); (4) ONE Fable pass last, brief in `docs/fable-brief.md` (owner named Fable for this task on 2026-10-18).

**The pin icon:** in the Panel header it is the "keep on top" toggle (`pin-toggled` -> window always-on-top), but it has no tooltip or visible state, and the same push-pin icon is used for pinned snippets, so it reads as "pin an item". Fix: tooltip + pressed state, a different icon for always-on-top, and verify always-on-top works on Windows (wizard step).

## Next steps (queue for the next session, in order)

Progress vs `docs/PLAN.md`: M1 done, M2 done in code (Windows unverified), M3 mostly done, M4 about half (UI built, not wired), M5 about 10%, M6 not started. About 55% of a v1 that can be built and tested off Windows.

1. **Main window branch (the only unmerged work):** branch `worktree-agent-a5d7314cbba05d676` (worktree `.claude/worktrees/agent-a5d7314cbba05d676`, last commit cbb6d10, pushed to origin as `claude/snipdeck-main-window`; open the follow-up PR from it). Main window, editors, SettingsPage, forms, CJK font helper, toolbar right-aligned. **Gate never confirmed**: finish per its `.claude/remaining/ui-main.md` (rebuild, run plain `cargo test -p snipdeck-ui`, regenerate only its own goldens, READ every golden, check the 225% claim, fmt, clippy stable + `+1.99.1`, doc), merge `origin/claude/compassionate-pascal-pod8ux` into it first (done once, conflicts resolved in lib.slint/button.slint/po files), then wire it into `snipdeck-app`.
   Merged into the PR branch this session (verify CI): Win32 tray fixes, docs/README/logo, review fixes #1/#3/#4/#6 (+ `write_salvaged` wired in `main.rs`), Picker/fill/capture/paste-next **logic** (`picker.rs`, `fill.rs`, `capture.rs`, `paste_next.rs`; no window binding yet: `binding.rs` must render `View.picker`/`View.fill`, register their HWNDs with the target tracker, toasts must show while the Panel is hidden), launcher (`Snipdeck.bat`, `tools/launcher/`: **never run**, run `pwsh tools/launcher/snipdeck.ps1 selftest` first; README quick start, troubleshooting and user-guide still describe `cargo run`).
2. **Fix the rest of `docs/reviews/PR4-final-review.md`** (tick each `[ ]`): #2 attach panel on first ShowPanel with `--hidden`, #5 route OS close to `Controller::close`, #7 field cap, #8 hotkey toast by failure kind, #9 `args_os`, #10 select backend before services start, #11 CJK test skips silently (make it `#[ignore]` or bundle a CJK font), #12 blank sensitive preview, #14 hygiene (m2-harness lockfile not covered by `cargo deny`; `tools/windows-test/README.md` has the old `Backpack` URL).
3. **Wire the Main window** into `snipdeck-app` (add/edit items, lists, boards, tags, Settings page writing `settings.json`, Trash view, import/export UI). Right-align the toolbar buttons (view toggles + New item) was requested by the owner.
3a. **Edge dock with hover/click reveal (owner request, see SPEC 5 Panel):** collapsed handle strip on a screen edge that slides the Panel out on hover and/or click, like Microsoft PC Manager's side panel/toolbar (owner sent screenshots: a vertical strip on the right edge with a grabber, and a horizontal one at the top). Settings: trigger (hover/click/both), delays, edge per monitor. Start with a Windows spike (S5: appbar vs topmost window, hover hit-test without stealing focus), then logic + tests with fakes, then the wizard steps.
4. **Translations:** `python3 scripts/i18n.py check` lists untranslated entries (156+ and the Main window's); translate with a Haiku pass for es, de, fr, pt-BR, ja, zh-Hans; `check` must report 0. Toasts are done: `ToastTexts` (`ui/toast_texts.slint`) holds every toast; `wording::toast_text(window, message)` reads it; only the Panel item and kebab menu words in `wording.rs` are still English.
5. **UI gaps from reviews:** split Esc (hide Panel) from the close button; search-focus-changed callback on PanelWindow; full high-contrast use of `SystemLook.hc-*`; 225% icon scaling; ItemCard HC focus ring on a selected compact card; Picker match highlighting and fill-form scrolling; items 12/13 of the component review (duplicated logic, API inconsistencies).
6. **M4 remainder:** text editor (16 KB cap + rope viewer), image editor (crop/rotate/resize/redact), Trash, template fill polish.
7. **M5:** capture overlay + OCR, Recent inbox, sensitive items (mask, clipboard auto-clear (SPEC 5.7; deferred to v1.1, the editor no longer promises it), master password), paste stack, licence UI (set `SNIPDECK_LICENCE_PUBLIC_KEY` at M6), onboarding, daily backups, crash log. `Free` board limit: callers must pass `max_boards: Some(3)` to import and restore.
8. **M6:** MSIX/MSI, signing, updater, winget, perf CI, accessibility audit (Narrator, contrast), docs site, CLA bot.
9c. **Users must not need Rust, Visual Studio or git (owner, 2026-10-18).** `Snipdeck.bat` is the developer/tester launcher (builds from source; the owner's PC passed all its checks). For everyone else ship a prebuilt app: (a) CI job (owner must allow a workflow edit: agents may not touch `.github/workflows`) that builds `snipdeck-app.exe` on windows-latest and uploads a portable zip as an Actions artifact / GitHub Release (dev builds use the development licence key; releases need `SNIPDECK_LICENCE_PUBLIC_KEY`); (b) `Snipdeck.bat` prefers a downloaded prebuilt zip (verify SHA-256) and only falls back to building from source when asked (`Snipdeck.bat build`); (c) later the M6 installer (MSIX/MSI, winget). Until then the README must say clearly that building from source is for developers.
9a. **Licence tooling (owner request):** DONE: `tools/snipdeck-keygen` (`genkey`, `sign`, `verify`; guide in `docs/licensing.md` "Keys for friends"; signer in `core/src/licence/signing.rs` behind feature `keygen`). The private key never enters the repo. Any online check (activation, revocation) needs an owner decision first because of the privacy and no-telemetry promise; not in v1.
9b. **Logo:** `assets/logo/snipdeck-icon.svg`, `snipdeck-logo-light.svg`, `snipdeck-logo-dark.svg`, generated by `scripts/make_logo.py` (wordmark outlined from IBM Plex Sans SemiBold); the owner asked for further refinement. Still to do: export `.ico` (16-256 px) for the exe icon resource and the tray, and MSIX tile PNGs.
9. **Run the fable pass last** (the owner allowed ONE focused Fable agent at the very end of full development: analyse, poke holes, challenge architecture, fix bugs and security issues, polish and validate).
10. Known gaps: Windows clipboard-history importer, CopyQ `.cpq`, no first-time "hidden to tray" notice, tray menu ignores dark mode, `{clipboard}` template text is `None`, time zone is UTC, Panel row context menus and Panel reveal-in-list are unbound.
11a. **Owner, GitHub settings (Settings ▸ Code security):** (1) enable **Code scanning ▸ CodeQL default setup** (covers Rust and Actions with no workflow file; the `codeql.yml` workflow was removed because uploads fail with "Code scanning is not enabled for this repository"); (2) enable **Dependency graph**, then `actions/dependency-review-action` can be re-added; (3) OpenSSF Scorecard (`scorecard.yml`, removed) only works on a public repository with code scanning on. RustSec audit (`audit.yml`) and cargo deny run regardless.
11. **Owner on Windows** (never run, everything Win32 is unverified): `Test Snipdeck on Windows.bat` (the v1 check: M2, M3, S1, S2 key steps), `run-snipdeck-demo.bat` (builds and opens the demo app), S2 with IMEs, S1 step 2c, S4 on Windows 10.

## Working notes that cost time this session

- **Disk is small.** Cargo `target/` dirs reach several GB each; use `CARGO_TARGET_DIR=/tmp/target-<name>` and delete
  it after use. Spike `target/` folders were deleted once (23 GB); the wizard rebuilds them on Windows.
- **Parallel agents:** give each its own crate/files and its own `CARGO_TARGET_DIR`; verify and commit subsets from a
  clean `git worktree` (copy only the finished files, regenerate `Cargo.lock` from HEAD's lock with
  `cargo metadata`, then run fmt/clippy/test/doc/deny/Windows check on both Rust versions before committing).
- **The stop hook** asks to commit whenever the tree is dirty; do not commit another agent's work in progress.
- **After a session restart** the `origin` remote may revert to the old `Backpack` URL:
  `git remote set-url origin https://github.com/Braydenh563/Snipdeck`.
- **Never answer `y` to the wizard's final "commit results?" question in dry-run tests.** That happened four times
  and pushed fake results files (now removed); `-DryRun` no longer touches git, but keep real result files out of
  the repo unless they came from a real Windows run.
- PowerShell 7 for parse checks and the wizard's `-SelfTest` / `-DryRun` is not installed by default (download the
  Linux tarball from the PowerShell GitHub releases into the scratchpad).
- The owner prefers terse answers and dislikes manual steps: automate what can be automated, give y/n prompts.
