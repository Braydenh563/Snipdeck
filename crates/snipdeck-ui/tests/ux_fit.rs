//! Fit checks from the UX audit: the Settings search leaves no blank gap (H1), toasts wrap instead of eliding (H3),
//! the Shortcuts list does not clip wrapped titles (M1), keycaps tell a chord from alternatives (M5) and the palette
//! scales with the text size (L3). Measured in the headless harness; sizes are logical pixels.

use i_slint_backend_testing::{AccessibleRole as Role, ElementQuery};
use slint::{ComponentHandle, Global};
use snipdeck_ui::testing::Harness;
use snipdeck_ui::{SettingsFilter, SettingsPage, SystemLook};

#[path = "support/editor_fixture.rs"]
mod editor;

/// (label, x, y, width, height) of every shown element with `role`, top to bottom.
fn find<C: ComponentHandle>(h: &Harness<C>, role: Role) -> Vec<(String, f32, f32, f32, f32)> {
    let mut out: Vec<(String, f32, f32, f32, f32)> = Vec::new();
    for e in ElementQuery::from_root(&h.component)
        .match_descendants()
        .match_accessible_role(role)
        .find_all()
    {
        let (p, s) = (e.absolute_position(), e.size());
        let n = (
            e.accessible_label().map(|l| l.to_string()).unwrap_or_default(),
            p.x,
            p.y,
            s.width,
            s.height,
        );
        if s.width > 0.5 && s.height > 0.5 && !out.iter().any(|o| o.0 == n.0 && (o.2 - n.2).abs() < 0.5) {
            out.push(n);
        }
    }
    out.sort_by(|a, b| a.2.total_cmp(&b.2));
    out
}

/// H1: with a search query, the first section that still matches starts right under the search field, at every text
/// size, instead of below the room the hidden sections used to take.
#[test]
fn settings_search_leaves_no_blank_gap() {
    let mut problems: Vec<String> = Vec::new();
    for (scale, width) in [(1.0f32, 640u32), (2.25, 765)] {
        for (query, section) in [
            ("as I type", "Typing"),
            ("backups", "Backups and your data"),
            ("supporter", "Licence"),
            ("programs", "Programs"),
        ] {
            let h = Harness::new(SettingsPage::new, width, 760);
            h.component.global::<SystemLook<'_>>().set_text_scale(scale);
            editor::settings(&h.component);
            let filter = SettingsFilter::get(&h.component);
            filter.on_matches(|text, query| text.to_lowercase().contains(&query.to_lowercase()));
            filter.set_query(query.into());
            let _ = h.render_settled();
            let field = find(&h, Role::TextInput)
                .into_iter()
                .find(|n| n.0 == "Search settings")
                .expect("search field");
            let Some(sec) = find(&h, Role::Groupbox).into_iter().find(|n| n.0 == section) else {
                problems.push(format!("{query:?} at {scale}x: section {section:?} is not on screen"));
                continue;
            };
            let gap = sec.2 - (field.2 + field.4);
            if gap > 40.0 * scale {
                problems.push(format!("{query:?} at {scale}x: {section:?} starts {gap:.0} px under the search field"));
            }
        }
    }
    assert_eq!(problems, Vec::<String>::new());
}
