# review-fixes
## Done (all verified by targeted tests, clippy, fmt)
1 editor More toggle refits and stays on the work area; error-opened More refits (editor_binding.rs, editors.slint more-shown is `out`)
2 first show uses size_before_show with explicit width/height
3 picker click-away: new `dismissed` callback -> Controller::dismiss_picker (no refocus); Esc keeps close_picker
4 Ctrl+V accepts "v", "V", "\u{16}"
5 main window use-copies set from the use label (cards, tiles; not in Trash)
## Next
Windows-only check by owner: editor More toggle near screen bottom; Picker click-away keeps focus on clicked window.
Not run: full `cargo test --workspace` (disk is tight, ran targeted binaries), gate.sh.
## Blocked
none
