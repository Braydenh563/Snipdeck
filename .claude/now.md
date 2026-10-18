# now.md
Project state: docs/HANDOVER.md. This file holds only the Inbox and Decisions.

## Decisions
- 2026-10-17: Lean workflow adopted; ledger at .claude/retro-ledger.md; gate.sh mirrors CI (fmt, clippy, doc, test).

- 2026-10-17: Owner allows exactly ONE focused Fable agent, once, at the end of full development before shipping (analyse the app, poke holes, fix bugs/security/architecture/front+back issues, polish and validate). Fable is used for nothing else; only if everything else is finished.
- 2026-10-17: Overnight run authorised (away mode, work order continue); never merge; "maximum token-efficient, thorough, effective".

## Inbox

- Inbox (2026-10-17, user, verbatim): "Im going to work, work hard to ship the application for when I get back ( wont be able to test the windows part until after my assignment is done tonight so just finish everyything upt ot that and polish as much as you can for public release)." -> Plan: finish M3/M4 as far as Linux can verify, then scans, then the one permitted Fable pass; add run-snipdeck-demo.bat once the app bin is merged.
- Inbox (2026-10-17, user, verbatim): "how do I actually run the app to test it??" -> after the app bin lands: add run-snipdeck-demo.bat (build + launch --demo), tell user.
