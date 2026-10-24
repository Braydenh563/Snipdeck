# Brief: step 7b Help, About and Troubleshoot (v1 scope)

Source: docs/V1-PLAN.md item 7b. v1 cuts (decision, ship first): no Restore flow, no launcher-tool migration, no `--doctor` CLI. Keep:

- One regular window `help.slint` (tabs or segmented: Troubleshoot | About), opened by `Effect::OpenHelp(topic)` (main.rs arm currently traces "not built yet (step 7b)"), the Main window Help entries, and the crash toast. Create with `RegularWindow::create(...)`, size via `native_window::size_before_show`, `fit_to_screen` after show, single instance (raise if open).
- `doctor.rs`: pure checks over a host trait with fakes, each `Check { label, status: Ok|Warn|Fail, detail, fix: Option<Fix> }`: data folder writable + free space; database integrity_check; backups newest date + count (Fix: Back up now); encryption key present when sensitive items exist; hotkeys registered (Fix: open Settings); tray icon present; last crash log (Open logs folder). "Check again" re-runs.
- Buttons: Open data folder, Open logs folder, Copy diagnostic report (versions, OS build, DPI, settings flags, check results; never item content, never paths outside the data folder).
- About: name, version, build date, licence tier, links: Source code (repo URL constant), Releases page "Check for updates" (opens releases URL; offline promise: nothing fetched by the app), Microsoft Store link hidden until a constant is set. Third-party notices button if a notices file exists.
- `HelpTopic::Guide`: opens docs/user-guide on the repo URL in the browser.
- All strings @tr + 6 translations; accessible roles/labels; keyboard: Tab order, Esc closes.
- Tests: each check pass + fail with the fake host; window a11y tree; goldens light/dark incl. one failing row (UPDATE_GOLDEN=1, read each PNG).
