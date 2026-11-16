# fix-audit-main remaining (head 04fd213, pushed to claude/fix-audit-main)

## Done (tests)
- 48-54, 62: toolbar tiers (854/776/726/692, tight <480), scaled min size, content-high rows
  [ui/tests/main_fit.rs main_fits_at_every_size_and_text_scale + main_tab_walk_...; main_window_logic toolbar tests]
- 55: scrolling welcome window [ui/tests/onboarding_fit.rs, 2 tests]
- 1, 2: board shown on cross-board hits; "First {} matches in {}" when capped, 6 locales
  [app/tests/main_window.rs typing_in_the_search_box_searches_every_board; main_window.rs a_hit_on_another_board_names_its_board, a_search_that_fills_its_limit_says_so]
- 11, 12: block reorder, announced list/board moves, board menu stays open
  [app/tests/main_controller.rs alt_arrows_move_a_multi_selection_as_a_block, moving_a_list_or_a_board_says_where_it_went_and_the_board_menu_stays_open]
- 5: chooser anchored to the bulk-bar button and clamped by real size (code done; NO dedicated test yet)
- DESIGN.md toolbar thresholds; ~36 goldens regenerated, only those that moved
- Gate --changed: all green except doc link, since fixed; rerun before next push.

## Not done (in audit order)
3, 4 (search keys, Esc), 5 test, 6, 7, 8, 9, 10, 13-21, 22 (Detail pane, large), 23-25, 27-47, 56-61,
and the not-reached files (settings.rs, settings_binding.rs, help_binding.rs, toast.rs, toast_binding.rs, text/icon globals).
26 belongs to LIBRARY-11 (leave).
Pending cross-worker note: quick-add row-h change shifts Panel goldens (already regenerated).
Env: export CARGO_TARGET_DIR=/tmp/target-main CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
Update goldens per failing test only (UPDATE_GOLDEN=1 cargo test -p snipdeck-ui -- <names>).
