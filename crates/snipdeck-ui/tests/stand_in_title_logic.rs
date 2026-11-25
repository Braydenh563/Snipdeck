//! An untitled sensitive item shows and speaks the stand-in title "Sensitive item", never a blank
//! row (LIBRARY-47); an untitled ordinary item keeps its blank title.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code: a failure is a failed test"
)]

use i_slint_backend_testing::{AccessibleRole, ElementQuery};
use slint::ComponentHandle;
use snipdeck_ui::StandInProbe;

#[path = "support/mod.rs"]
mod support;
use support::on_ui_thread;

#[test]
fn an_untitled_sensitive_card_is_named_sensitive_item() {
    on_ui_thread(|| {
        let ui = StandInProbe::new().expect("probe");
        ui.show().expect("show");
        let mut cards = ElementQuery::from_root(&ui)
            .match_accessible_role(AccessibleRole::ListItem)
            .find_all();
        cards.sort_by(|a, b| a.absolute_position().y.total_cmp(&b.absolute_position().y));
        let labels: Vec<String> = cards
            .iter()
            .map(|c| c.accessible_label().unwrap_or_default().to_string())
            .collect();
        assert_eq!(labels.len(), 2, "{labels:?}");
        assert_eq!(
            labels[0], "Sensitive item",
            "masked: the stand-in title only"
        );
        assert!(!labels[1].contains("Sensitive item"), "{labels:?}");
        // The title text is drawn, not an empty line: the card's title element has a width.
        let titles = ElementQuery::from_root(&ui)
            .match_id("ItemCard::title-text")
            .find_all();
        let drawn: Vec<_> = titles.iter().filter(|t| t.size().width > 1.0).collect();
        assert!(
            !drawn.is_empty(),
            "a title text is measured: {}",
            titles.len()
        );
    });
}
