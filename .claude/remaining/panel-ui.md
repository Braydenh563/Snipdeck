# panel-ui remaining work

Done: kebab menu (app-menu-*), tooltips via tip-set for every icon-only control, resize grips (resize-requested), quick-edit hooks
(edit/rename/delete-requested), pinned strip wheel + floating carets, i18n extract, goldens (panel-*, deck-parts-*), CODEMAP.
Merged claude/compassionate-pascal-pod8ux; replaced the coordinator's two-button tooltip block with the general tip-set.
Next: gate result; hand over.
Blocked: gate fast-checks fail on files owned by others (cargo fmt on snipdeck-app/tests/panel_window.rs,
clippy in snipdeck-ui/tests/main_window_logic.rs:411, picker golden flaky search caret).
