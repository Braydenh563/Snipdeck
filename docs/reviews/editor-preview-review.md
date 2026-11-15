# Editor preview review (2026-11-12)

Independent review of origin/claude/editor-preview at 6ca6cb5. Two opus reviewer lenses ran, and a sonnet refuter checked each finding. These survived.

## 1. [major] Sensitive item: Preview shows the secret's first line as the title, plus the full template result and link host. The real card shows none of this.

- **At:** `crates/snipdeck-app/src/item_preview.rs:76`
- **Evidence:** card_of picks the title from `text.trim_start().lines().next()` whenever the title is blank, even when `sensitive` is true. The real row builder (panel.rs:551-555) takes the title from `item.preview.lines().next()`, and preview_of returns "" for a sensitive item, so a real untitled sensitive card has an empty title. Example: an untitled sensitive Text item whose content is `4111 1111 1111 1111` shows those digits as the Preview card title, in both the Panel card and the Main card. The `expanded` branch (line 101) and the `link` branch (line 96) also ignore `sensitive`, so a sensitive Template shows its full expansion and a sensitive Link shows its host in clear. This contradicts decision (4) in now.md: "a sensitive item shows the masked card (twelve dots), never its text". The only test, a_sensitive_item_shows_no_excerpt (line 211), uses a non-empty title ("Card"), so it never reaches this path.
- **Fix:** When `sensitive` is true, take the title only from input.title. That gives an empty title when it is blank, which matches panel.rs. Also leave `expanded` and `link` as None. Add tests for an untitled sensitive Text item, a sensitive Template and a sensitive Link.

## 2. [major] A Save refused while Preview is on gives no visible feedback, because the error appears on the hidden form

- **At:** `crates/snipdeck-ui/ui/editors.slint:808`
- **Evidence:** The footer (Save button) and Ctrl+Enter both stay active while preview-on is true. try-save (line 808) sets show-errors and returns when content-missing, title-error or content-error is set. Those messages render under fields inside `flick`, which has `visible: !root.preview-on` (line 906). Nothing turns preview-on off: the only writers are toggle-preview and fill() (editor_binding.rs:570). Host-side refusals also leave the pane up, because apply_state sets content_error/title_error without leaving Preview. Example: a new item with empty content, Ctrl+P, then Save. Nothing happens and nothing is shown. For a Template or Link, the pane's `note` was computed when Preview was turned on and covers only parse and link errors, not ContentEmpty, TitleTooLong or NeedsPro. The window test only covers a Save that succeeds while previewing.
- **Fix:** In try-save, and when the host reports a refusal, set preview-on false before showing the errors, so the user lands on the field with the problem. Add a window test: Preview on, Save with empty content, assert that the form is shown and the error is visible.

## 3. [minor] Ctrl+P is missing from the Editors shortcuts list and the user guide

- **At:** `crates/snipdeck-app/src/shortcuts.rs:156`
- **Evidence:** The Editors scope lists only `row(Editors, "Ctrl+Enter")` and `row(Editors, "Esc")` (lines 155-157). docs/user-guide.md documents Ctrl+P only as Pin for the Panel and Main (lines 394 and 412). The new editor key in editors.slint:837 appears only in the button tooltip. There is no clash, because the editor is its own window and capture-key-pressed handles the key before Pin could. But the same chord now means two things depending on the window, and the shortcuts list does not say so. The handler also does not check Shift, so Ctrl+Shift+P toggles Preview as well.
- **Fix:** Add `row(Editors, "Ctrl+P")` with its translated title, a guide row, and the matching guide-table test. Optionally add `!event.modifiers.shift` to the handler.

## 4. [minor] Preview cards copy the card setup instead of reusing it, and ignore the user's Panel layout and Main density

- **At:** `crates/snipdeck-ui/ui/editors.slint:551`
- **Evidence:** The ItemCard component is shared, but PreviewPane writes its own property mapping. The Panel card is hard-coded to `ItemCardVariant.compact` with `number: 1` and `numbered` left at its default of true (line 554). The real Panel (panel.slint:1934-1940) uses eff-layout (compact/comfortable/tile) and `numbered: root.show-numbers || root.select-mode`. The Main card is hard-coded to comfortable (line 576). The real Main list (main_window.slint:1882) follows `root.view` (compact/comfortable) and uses MainCard, which also sets masked-hint and pick-mode. Label names come from a separate @tr list (label-names) while real rows use wording::item_label_name. The title is also not limited to PREVIEW_CHARS, while the real title is the first line of the stored preview, which is limited. These differences break "exactly as its card would look" and will drift as either side changes.
- **Fix:** Pass the user's panel layout, show-numbers and main density into the editor and use them. Better, put the card-from-row mapping in one shared component that PreviewPane, MainCard and the Panel all use. Take the title from the first line of preview_of so it is limited the same way.

## 5. [minor] Template preview misses the NeedsPro refusal, despite claiming it gives "the same refusal Save would give"

- **At:** `crates/snipdeck-app/src/item_preview.rs:101`
- **Evidence:** card_of only calls Template::parse. check_template in item_editor.rs:349-360 also returns NeedsPro(PromptedFields) when a Free user adds prompted placeholders. A Free user who types `{field:Name}` sees a full sample result with no note, then Save refuses. The CardPreview::problem doc comment says it is the same refusal Save would give.
- **Fix:** Pass the prompted-placeholder permission (and whether the text was introduced) into card_of, or call item_editor::check_template, and set `problem` from its result.

## 6. [major] Save from Preview fails silently: the field errors are drawn in the hidden form

- **At:** `crates/snipdeck-ui/ui/editors.slint:808`
- **Evidence:** Save and Ctrl+Enter both still call try-save() while preview-on (lines 813, 833, footer save). try-save sets show-errors and returns early when content-missing, source-missing, content-error, title-error or source-error is set. All of those messages are FieldErrors inside `flick`, which has `visible: !root.preview-on` (line 906). Nothing turns preview-on off, and nothing moves focus to the error. Only the banner sits outside the flick. Scenario: open New item, press Ctrl+P on the empty form, then press Save. Nothing visible happens and a screen reader hears nothing. The new window test saves from Preview only with valid input (editor_window.rs, `save(&ed)` after a valid link), so this path is untested. Slint 1.18.1 does skip invisible items (i-slint-core window.rs:1608), so the errors cannot be reached by Tab either.
- **Fix:** In try-save, when a refusal applies and preview-on is true, set preview-on = false first, then focus the first field that has an error. Do the same when the host sends back title-error, content-error or privacy-error. Add a window test: Preview on, empty content, Ctrl+Enter, then assert preview-on is false and the error is visible.

## 7. [major] The preview cards are not drawn the way the user's Panel and Main list draw them (fixed variant, no image, no list line, no masked hint)

- **At:** `crates/snipdeck-ui/ui/editors.slint:554`
- **Evidence:** The Panel card is always `ItemCardVariant.compact` with `number: 1` (lines 551-566). The Main card is always `comfortable` (line 576). The real Panel picks its variant from `eff-layout` (compact, comfortable or tile cards) and sets `numbered` from show-numbers (panel.slint:2014, 2021). The real Main list switches compact or comfortable on `root.view` (main_window.slint:2244). The preview also never passes `image`, so an Image item shows the generic icon tile where the real card shows the thumbnail (item-card.slint:644-653). It never passes `list-name`, so the Main card's 'where' line is missing and the card height differs (item-card.slint:383-384). It never passes `masked-hint`, so a sensitive card lacks MainCard's 'Hidden. Hold Space to reveal.' (main_window.slint:203). This contradicts the intended 'exactly as its card would look'. The golden covers only one Template at 560 px.
- **Fix:** Pass the user's Panel layout, show-numbers and Main view into the editor and use them for `variant` and `numbered`. Pass the image thumbnail, list name and colour, and masked-hint the same way MainCard does. Add a golden for a sensitive item and one for an Image item.

## 8. [minor] The on state uses the primary (ink) fill and puts state only in the label, against DESIGN.md

- **At:** `crates/snipdeck-ui/ui/editors.slint:874`
- **Evidence:** `variant: root.preview-on ? ButtonVariant.primary : ButtonVariant.secondary`. DESIGN.md:18 and :369 say ink is reserved for commitment, and the footer Save is already primary. With Preview on, the window shows two ink buttons, and the Preview button competes with Save. DESIGN.md:176 says a toggle (Keep on top) shows a pressed state with 'the tooltip and checked state carry the meaning'. Here the on/off state exists only as words in a11y-label (line 877, 'Preview, on. Back to editing'). The Button component has no accessible-checkable or accessible-checked (button.slint:80-83), so UIA reports a plain button and no toggle state.
- **Fix:** Use a secondary or ghost button with a pressed or selected fill when on, as the Keep-on-top pin does. Expose `accessible-checkable: true` and `accessible-checked: preview-on` (add them to Button, or use a toggle variant). Keep the label as just 'Preview'.

## 9. [minor] Ctrl+P ignores an open tag chooser; the next Esc then acts on the hidden chooser

- **At:** `crates/snipdeck-ui/ui/editors.slint:837`
- **Evidence:** The Ctrl+P guard checks only `!root.discard-open`. With tags-open true (the Chooser at line 1244 lives inside the hidden flick), Ctrl+P hides the form but leaves tags-open true. In preview, Esc runs `if (root.tags-open) { root.tags-open = false; }` (line 852). So the first Esc changes nothing the user can see, and a second Esc is needed to cancel.
- **Fix:** In toggle-preview, set tags-open = false when turning Preview on, or block Ctrl+P while tags-open is true.

## 10. [minor] Ctrl+P is missing from the Help shortcuts list and the user guide

- **At:** `crates/snipdeck-app/src/shortcuts.rs:156`
- **Evidence:** The Editors scope lists only Ctrl+Enter and Esc (shortcuts.rs:155-157). `git grep` finds no Editor Ctrl+P row and no Preview mention in docs/user-guide.md (the guide's Ctrl+Enter line is at user-guide.md:29). The only places the key appears are the tooltip strings. Elsewhere Ctrl+P means Pin (Main and Panel rows at shortcuts.rs:108 and :134), so users who look it up will find only Pin.
- **Fix:** Add row(Editors, "Ctrl+P") with translated wording, add the guide-table entry, and add a sentence on Preview to the item-editor section of docs/user-guide.md.

## 11. [minor] de translations switch to formal 'Sie' and to the noun 'Eintrag' instead of 'Textbaustein'

- **At:** `crates/snipdeck-ui/lang/de/LC_MESSAGES/snipdeck-ui.po:727`
- **Evidence:** New msgstr: "...ein Feld, das Sie beim Verwenden ausfüllen...". The de catalogue uses 'du' about 41 times and ' Sie ' as a pronoun of address in only 2 strings. The other 3 matches are 'sie'='it', capitalised after a colon or at sentence start. New strings also use 'Eintrag' ('Vorschau, wie dieser Eintrag aussehen wird'), while the catalogue calls an item 'Textbaustein' 147 times and 'Eintrag' 15 times. ja has the same split: the new strings use 項目 (34 in the catalogue) where スニペット appears 131 times.
- **Fix:** de: "ein Feld, das du beim Verwenden ausfüllst", and use 'Textbaustein' in the four new item strings. Check ja against the term the editor already uses.

## 12. [minor] The 'Unsaved' caption is shown even when nothing has changed

- **At:** `crates/snipdeck-ui/ui/editors.slint:541`
- **Evidence:** The text 'Unsaved: this is how the item will look once you save it.' does not depend on `root.dirty`. If you open an existing item and press Ctrl+P without editing, the caption still says the item is unsaved, which is wrong and may push the user to Save for nothing.
- **Fix:** Show the caption only when root.dirty is true (pass it into PreviewPane), or reword it to cover both cases.

## 13. [minor] No golden or test covers the Preview at narrow widths, a dark theme, large text, or the focus order

- **At:** `crates/snipdeck-ui/tests/editors.rs:0`
- **Evidence:** The only new golden is item_editor_preview_of_a_template at 560 px, light, 1x text. The window min-width is 420 px (editors.slint:640), and the Panel card is min(parent.width, 360px). The expanded Text is mono with word-wrap, and a long unbroken token in it was not exercised. The 'Opens' value elides with no tooltip, so a long host is cut off and cannot be read in full. The window test checks only preview-on, the values and Ctrl+P. It does not check where focus goes when Preview turns off for Image or File kinds: focus-title() focuses nothing there unless More options is open, so focus stays on the toggle.
- **Fix:** Add goldens for 420 px and 2.25x text, one in dark, and one with a long link host and a template that has a long unbroken token. Add a key-walk check of the Tab order with Preview on, and a test that turns Preview off for an Image item.
