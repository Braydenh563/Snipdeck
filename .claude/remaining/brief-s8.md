# Slice 8 brief (queued after slice 6 lands)
- Spec: docs/reviews/v101-spec.md "### A25 cold start + search suggestions + doc ticks (slice 8)".
- A25: measure first (trace.rs ms main->first show, PanelModel::load_cached index build); defer only if >50 ms; test a_search_before_the_index_is_ready_still_finds.
- Suggestions: pure search_suggest.rs; Panel, Picker, Main fields; shared menu row; tests per spec.
- Doc ticks: U9 done (64b6283); A25 tick after build; HANDOVER v1.0.1 list.
- Base: PR branch after wt/v101-s6 merge (both touch main_window.slint, panel.slint).
