# builder-7a remaining

Done (uncompiled, no cargo run): onboarding.slint, tour.rs, controller/onboarding_ctl.rs (+ tests), onboarding_binding.rs,
Settings.onboarded (+ tests), main.rs wiring (first run + Effect::OpenOnboarding), goldens/logic tests, i18n (6 locales).
Next: compile batch; generate goldens (`UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test editors`) and look; Help window button calls
`snipdeck_app::onboarding_binding::request_open()` (or returns Controller::show_welcome_tour() effects).
Not done: Getting started list seed (starter.rs only makes the empty first board); window-level app test (tests/onboarding_window.rs).
