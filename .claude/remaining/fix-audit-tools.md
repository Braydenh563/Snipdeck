# fix-audit-tools remaining (branch claude/fix-audit-tools)
Source: .claude/audit/tools.md (TOOLS-1..63). Test dir: crates/snipdeck-colour/tests/audit_tools.rs.

## Done
- snipdeck-colour crate: TOOLS-1,2,3,4 (one round-trip rule, alpha in every format), 5,6,7 (HSL rotation, tints one-sided, has_hue),
  8 (parse_live), 9 (hwb/lab/lch/oklab/color()/0xAARRGGBB/decimal/FromArgb/UIColor), 10 (extract), 11 (NFKC, decimal()),
  12 (copy_text), 13 (contrast_over/composite), 14 (nearest_passing, apca fns only), 31 (ExtraFormat, custom_format).

## Next
- App/UI wiring of the colour crate (controller colour_ctl.rs, binding/colours.rs, colours.slint): TOOLS-14 UI, 15, 16-30
- Eyedropper platform: 32-43
- Symbols: 44-61, 63; SPEC 62
- Not-reached rows: colour_block.slint, screen_clip.rs, generator scripts, harness tabs
- i18n: new strings into .pot + 6 .po

## Blocked
- Windows-only code unverified.
