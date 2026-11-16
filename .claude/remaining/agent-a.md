# agent-a remaining

## Done (code written, compile in progress)
- MAIN-14 shared tile height (probe removed), MAIN-29 cached entries / group map / keyed row reuse + test
- MAIN-23 board field in list editor, Move chooser (Board · List), editor park
- MAIN-21 ThumbCache + thumb-wanted queue; MAIN-22 Detail pane + setting main_detail_open
- Docs: SPEC 5.3 / §15 and DESIGN Detail row updated

## Next
- Compile + run lib tests, main_controller tests; write controller tests (MAIN-23, MAIN-21, Detail)
- Golden/fit tests for the Detail pane; i18n extract/todo/apply for six locales

## Blocked
- cargo target dir lock held by other agents

## Windows run
- Narrator reads the Detail pane ("Details" group, "name: value" fields, Preview)
- Thumbnail look of image rows on Windows
