# now.md
Project state: docs/HANDOVER.md. This file holds only the Inbox and Decisions.

## Decisions
- 2026-10-17: Lean workflow adopted; ledger at .claude/retro-ledger.md; gate.sh mirrors CI (fmt, clippy, doc, test).

- 2026-10-17: Owner allows exactly ONE focused Fable agent, once, at the end of full development before shipping (analyse the app, poke holes, fix bugs/security/architecture/front+back issues, polish and validate). Fable is used for nothing else; only if everything else is finished.
- 2026-10-17: Overnight run authorised (away mode, work order continue); never merge; "maximum token-efficient, thorough, effective".

## Inbox

- Inbox (2026-10-17, user, verbatim): "Im going to work, work hard to ship the application for when I get back ( wont be able to test the windows part until after my assignment is done tonight so just finish everyything upt ot that and polish as much as you can for public release)." -> Plan: finish M3/M4 as far as Linux can verify, then scans, then the one permitted Fable pass; add run-snipdeck-demo.bat once the app bin is merged.
- Inbox (2026-10-17, user, verbatim): "how do I actually run the app to test it??" -> after the app bin lands: add run-snipdeck-demo.bat (build + launch --demo), tell user.
- Inbox (2026-10-18, user, verbatim): "Also log that I want it to be able to dock to the side or edge of the screen to be opened on hover and/or click. like with the microsoft pc manager and it's panel" -> SPEC 5.x already has dock-to-edge; add edge-dock trigger modes (hover and/or click, collapsed handle/grabber strip, slide-out) as a requirement; queued in HANDOVER next steps 3a.
- Inbox (2026-10-18, user): first real Windows demo run found 9 problems (minimise, pinned row scroll with mouse/touch, dead + button, no help/tooltips, top item clipped, no way to open main window / no logo / no kebab menu, pin button dead, tray menu items dead and not in taskbar, slow rebuild). Recorded in docs/HANDOVER.md "Owner's first real Windows run".
