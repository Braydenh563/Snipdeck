//! UX-35, 41, 42, 43, 44: every window's smallest size comes from its content and grows with the text size; a
//! footer never overlaps the content above it; the colour and icon grids wrap inside the window. Measured on the
//! accessible tree at the window's own minimum size, at 1x and 2.25x, and in the six languages where the finding
//! names them. Time is mocked, never the real clock.

#![allow(
    dead_code,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use i_slint_backend_testing::{AccessibleRole, ElementQuery, ElementRoot};
use slint::{ComponentHandle, ModelRc, VecModel};
use snipdeck_ui::{
    BoardEditor, HelpWindow, ItemEditor, ListEditor, OnboardingWindow, PickerWindow, SettingsPage,
    SystemLook, TagManager,
};

#[path = "support/mod.rs"]
mod support;
use support::on_ui_thread;

#[path = "support/editor_fixture.rs"]
mod editor;
#[path = "support/picker_fixture.rs"]
mod picker;

const ROLES: [AccessibleRole; 8] = [
    AccessibleRole::Button,
    AccessibleRole::Checkbox,
    AccessibleRole::Switch,
    AccessibleRole::RadioButton,
    AccessibleRole::Tab,
    AccessibleRole::Combobox,
    AccessibleRole::TextInput,
    AccessibleRole::Spinbox,
];

const LOCALES: [&str; 6] = ["", "de", "fr", "es", "pt-BR", "ja"];
const SCALES: [f32; 2] = [1.0, 2.25];

struct El {
    name: String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

fn elements(root: &impl ElementRoot) -> Vec<El> {
    let mut out = Vec::new();
    for role in ROLES {
        for e in ElementQuery::from_root(root)
            .match_descendants()
            .match_accessible_role(role)
            .find_all()
        {
            let (p, s) = (e.absolute_position(), e.size());
            if s.width < 0.5 || s.height < 0.5 {
                continue;
            }
            out.push(El {
                name: e
                    .accessible_label()
                    .map(|l| l.to_string())
                    .unwrap_or_default(),
                x: p.x,
                y: p.y,
                w: s.width,
                h: s.height,
            });
        }
    }
    out
}

/// Problems at a window of `w` x `h`: an element past the left or right edge; a footer button (`footer` names the
/// footer's buttons, matched by `starts_with`) past the bottom edge, overlapping another footer button, or crossed by
/// a control of the content above it.
fn problems(root: &impl ElementRoot, w: f32, h: f32, footer: &[&str], scene: &str) -> Vec<String> {
    let els = elements(root);
    let is_footer = |e: &El| footer.iter().any(|f| e.name.starts_with(f));
    let band_top = els
        .iter()
        .filter(|e| is_footer(e))
        .map(|e| e.y)
        .fold(f32::MAX, f32::min);
    let mut out = Vec::new();
    for (i, e) in els.iter().enumerate() {
        if e.x < -0.5 || e.x + e.w > w + 0.5 {
            out.push(format!(
                "{scene}: {:?} spans x {:.0}..{:.0} of {w}",
                e.name,
                e.x,
                e.x + e.w
            ));
        }
        if is_footer(e) {
            if e.y < -0.5 || e.y + e.h > h + 0.5 {
                out.push(format!(
                    "{scene}: footer {:?} spans y {:.0}..{:.0} of {h}",
                    e.name,
                    e.y,
                    e.y + e.h
                ));
            }
            for o in els.iter().skip(i + 1).filter(|o| is_footer(o)) {
                let dx = (e.x + e.w).min(o.x + o.w) - e.x.max(o.x);
                let dy = (e.y + e.h).min(o.y + o.h) - e.y.max(o.y);
                // A button and the label inside it share a name and nest: that is not an overlap.
                let nested = |a: &El, b: &El| {
                    b.x >= a.x - 0.5
                        && b.y >= a.y - 0.5
                        && b.x + b.w <= a.x + a.w + 0.5
                        && b.y + b.h <= a.y + a.h + 0.5
                };
                if dx > 1.0 && dy > 1.0 && !nested(e, o) && !nested(o, e) {
                    out.push(format!(
                        "{scene}: footer {:?} overlaps {:?} by {dx:.0}x{dy:.0}",
                        e.name, o.name
                    ));
                }
            }
        } else if band_top < f32::MAX && e.y < band_top - 0.5 && e.y + e.h > band_top + 0.5 {
            out.push(format!(
                "{scene}: {:?} spans y {:.0}..{:.0} across the footer at {band_top:.0}",
                e.name,
                e.y,
                e.y + e.h
            ));
        }
    }
    out
}

fn with_locales(f: impl Fn(&str, f32) -> Vec<String> + Send + 'static) {
    on_ui_thread(move || {
        let mut all = Vec::new();
        for lang in LOCALES {
            // The source language needs no selecting before any other was selected.
            let selected = slint::select_bundled_translation(lang);
            assert!(selected.is_ok() || lang.is_empty(), "{lang}: {selected:?}");
            for s in SCALES {
                all.extend(f(lang, s));
            }
        }
        slint::select_bundled_translation("").expect("back to the source language");
        assert_eq!(all.len(), 0, "{all:#?}");
    });
}

fn settle<C: ComponentHandle>(ui: &C, s: f32, w: f32, h: f32)
where
    for<'a> SystemLook<'a>: slint::Global<'a, C>,
{
    ui.global::<SystemLook<'_>>().set_text_scale(s);
    ui.window().set_size(slint::LogicalSize::new(w, h));
    ui.show().unwrap();
    slint::platform::update_timers_and_animations();
}

#[test]
fn tag_manager_footer_clears_the_rows_at_its_minimum_size() {
    with_locales(|lang, s| {
        let ui = TagManager::new().unwrap();
        ui.global::<SystemLook<'_>>().set_text_scale(s);
        editor::tag_manager(&ui);
        // Two tags fill the list area at the smallest size, so every row is on screen.
        ui.set_tags(ModelRc::new(VecModel::from(
            editor::tag_entries()
                .into_iter()
                .take(2)
                .collect::<Vec<_>>(),
        )));
        settle(&ui, s, 0.0, 0.0);
        let (w, h) = (ui.get_min_window_width(), ui.get_min_window_height());
        settle(&ui, s, w, h);
        let scene = format!("tag manager {lang:?} {s}x at {w}x{h}");
        let footer = ["Close", "Schließen", "Fermer", "Cerrar", "Fechar", "閉じる"];
        problems(&ui, w, h, &footer, &scene)
    });
}

#[test]
fn tag_manager_minimum_height_grows_with_the_text_size() {
    on_ui_thread(|| {
        let mut heights = Vec::new();
        for s in SCALES {
            let ui = TagManager::new().unwrap();
            settle(&ui, s, 440.0, 300.0);
            heights.push(ui.get_min_window_height());
        }
        assert!(heights[1] > heights[0] * 1.5, "{heights:?}");
    });
}

fn entity_scene<C: ComponentHandle + ElementRoot>(
    ui: &C,
    s: f32,
    lang: &str,
    name: &str,
    size: (f32, f32),
    footer: &[&str],
) -> Vec<String>
where
    for<'a> SystemLook<'a>: slint::Global<'a, C>,
{
    settle(ui, s, size.0, size.1);
    problems(
        ui,
        size.0,
        size.1,
        footer,
        &format!("{name} {lang:?} {s}x at {}x{}", size.0, size.1),
    )
}

const SAVE_CANCEL: [&str; 12] = [
    "Save",
    "Cancel",
    "Delete",
    "Speichern",
    "Abbrechen",
    "Löschen",
    "Enregistrer",
    "Annuler",
    "Supprimer",
    "Guardar",
    "Cancelar",
    "Eliminar",
];

#[test]
fn list_and_board_editors_hold_every_element_at_minimum_and_preferred_size() {
    with_locales(|lang, s| {
        let mut out = Vec::new();
        let ui = ListEditor::new().unwrap();
        ui.global::<SystemLook<'_>>().set_text_scale(s);
        editor::list_editor_existing(&ui);
        ui.set_show_colour(true);
        ui.set_show_sort(true);
        ui.set_can_delete(true);
        ui.set_icon_names(editor::strings(&["Briefcase"; 24]));
        settle(&ui, s, 460.0, 440.0);
        let min = (ui.get_min_window_width(), ui.get_min_window_height());
        let pref = (460.0f32.max(min.0), ui.get_wanted_height());
        out.extend(entity_scene(&ui, s, lang, "list editor", min, &SAVE_CANCEL));
        out.extend(entity_scene(
            &ui,
            s,
            lang,
            "list editor",
            pref,
            &SAVE_CANCEL,
        ));
        let ui = BoardEditor::new().unwrap();
        ui.global::<SystemLook<'_>>().set_text_scale(s);
        ui.set_is_new(false);
        ui.set_show_colour(true);
        ui.set_show_sort(true);
        ui.set_can_delete(true);
        settle(&ui, s, 460.0, 440.0);
        let min = (ui.get_min_window_width(), ui.get_min_window_height());
        let pref = (460.0f32.max(min.0), ui.get_wanted_height());
        out.extend(entity_scene(
            &ui,
            s,
            lang,
            "board editor",
            min,
            &SAVE_CANCEL,
        ));
        out.extend(entity_scene(
            &ui,
            s,
            lang,
            "board editor",
            pref,
            &SAVE_CANCEL,
        ));
        out
    });
}

#[test]
fn list_editor_grids_show_every_cell_inside_the_window_at_every_width() {
    on_ui_thread(|| {
        let mut out = Vec::new();
        for s in SCALES {
            for w in [380.0f32, 460.0, 560.0, 800.0] {
                let ui = ListEditor::new().unwrap();
                ui.global::<SystemLook<'_>>().set_text_scale(s);
                editor::list_editor_existing(&ui);
                ui.set_show_colour(true);
                ui.set_show_sort(true);
                ui.set_icon_names(editor::strings(&["Briefcase"; 24]));
                settle(&ui, s, w, 3000.0);
                let w = w.max(ui.get_min_window_width());
                settle(&ui, s, w, 3000.0);
                let radios = elements(&ui).into_iter().filter(|e| e.w > 0.5).count();
                out.extend(problems(
                    &ui,
                    w,
                    3000.0,
                    &[],
                    &format!("list editor {s}x at {w}"),
                ));
                assert!(radios > 30, "{radios} elements at {s}x {w}");
            }
        }
        assert_eq!(out.len(), 0, "{out:#?}");
    });
}

#[test]
fn onboarding_footer_clears_the_step_in_every_language() {
    with_locales(|lang, s| {
        let mut out = Vec::new();
        for step in 0..4 {
            let ui = OnboardingWindow::new().unwrap();
            ui.global::<SystemLook<'_>>().set_text_scale(s);
            editor::onboarding(&ui, step);
            settle(&ui, s, 0.0, 0.0);
            let (w, h) = (ui.get_min_window_width(), ui.get_min_window_height());
            settle(&ui, s, w, h);
            let footer = [
                "Skip",
                "Next",
                "Back",
                "Get started",
                "Überspringen",
                "Weiter",
                "Zurück",
                "Los geht",
                "Passer",
                "Suivant",
                "Retour",
                "Commencer",
                "Omitir",
                "Siguiente",
                "Atrás",
                "Empezar",
                "Pular",
                "Avançar",
                "Voltar",
                "Começar",
                "スキップ",
                "次へ",
                "戻る",
                "始める",
            ];
            out.extend(problems(
                &ui,
                w,
                h,
                &footer,
                &format!("tour step {} {lang:?} {s}x at {w}x{h}", step + 1),
            ));
        }
        out
    });
}

#[test]
fn picker_and_help_buttons_are_not_cut_at_the_minimum_height() {
    with_locales(|lang, s| {
        let mut out = Vec::new();
        let ui = PickerWindow::new().unwrap();
        ui.global::<SystemLook<'_>>().set_text_scale(s);
        picker::populate(&ui);
        settle(&ui, s, 0.0, 0.0);
        let (w, h) = (ui.get_min_window_width(), ui.get_min_window_height());
        settle(&ui, s, w, h);
        for e in elements(&ui) {
            if e.y + e.h > h + 0.5 {
                out.push(format!(
                    "picker {lang:?} {s}x: {:?} ends at {:.0} of {h}",
                    e.name,
                    e.y + e.h
                ));
            }
        }
        let ui = HelpWindow::new().unwrap();
        ui.global::<SystemLook<'_>>().set_text_scale(s);
        editor::help(&ui);
        editor::shortcuts(&ui);
        editor::tips(&ui);
        ui.set_tab(2);
        settle(&ui, s, 0.0, 0.0);
        let (w, h) = (ui.get_min_window_width(), ui.get_min_window_height());
        settle(&ui, s, w, h);
        // The first tip's own button is on screen at the smallest size.
        let first = elements(&ui)
            .into_iter()
            .filter(|e| {
                e.name.starts_with("Try")
                    || e.name.starts_with("Probier")
                    || e.name.starts_with("Essayer")
                    || e.name.starts_with("Probar")
                    || e.name.starts_with("Experimente")
                    || e.name.starts_with("試")
            })
            .map(|e| e.y)
            .fold(f32::MAX, f32::min);
        for e in elements(&ui) {
            if (e.y - first).abs() < 0.5 && e.y + e.h > h + 0.5 {
                out.push(format!(
                    "help tips {lang:?} {s}x: {:?} ends at {:.0} of {h}",
                    e.name,
                    e.y + e.h
                ));
            }
        }
        out
    });
}

#[test]
fn item_editor_and_settings_minimum_height_grows_with_the_text_size() {
    on_ui_thread(|| {
        let mut item = Vec::new();
        let mut settings = Vec::new();
        let mut help = Vec::new();
        for s in SCALES {
            let ui = ItemEditor::new().unwrap();
            settle(&ui, s, 700.0, 700.0);
            item.push(ui.get_min_window_height());
            let ui = SettingsPage::new().unwrap();
            settle(&ui, s, 800.0, 800.0);
            settings.push(ui.get_min_window_height());
            let ui = HelpWindow::new().unwrap();
            settle(&ui, s, 700.0, 700.0);
            help.push(ui.get_min_window_height());
        }
        for (name, v) in [("item", item), ("settings", settings), ("help", help)] {
            assert!(v[1] > v[0] * 1.4, "{name}: {v:?}");
        }
    });
}
