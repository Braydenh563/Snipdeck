//! Spike S1 (throwaway): can a Slint window be clicked without taking keyboard focus from the
//! foreground app, and can we then paste into that app? See `docs/spikes/S1-noactivate.md`.

mod platform;

use std::rc::Rc;

use slint::{ComponentHandle, Model, SharedString, VecModel};

slint::slint! {
    import { Button, LineEdit, VerticalBox } from "std-widgets.slint";

    export component Panel inherits Window {
        title: "S1 panel";
        always-on-top: true;
        width: 300px;
        height: 380px;

        in property <[string]> items;
        in property <string> footer;
        in property <string> status;

        // The search field has (true) or lost (false) Slint focus.
        callback search-focus(bool);
        callback search-edited(string);
        callback use-item(string);
        callback use-first();
        callback pointer-entered();

        public function clear-search-focus() {
            search.clear-focus();
        }

        // Refresh the target footer when the pointer arrives, i.e. right before a likely click.
        TouchArea {
            changed has-hover => {
                if self.has-hover {
                    root.pointer-entered();
                }
            }
            // Escape leaves the search field, which hands focus back to the target app.
            FocusScope {
                focus-on-click: false;
                capture-key-pressed(event) => {
                    if event.text == Key.Escape {
                        search.clear-focus();
                        return accept;
                    }
                    reject
                }
                VerticalBox {
                    search := LineEdit {
                        placeholder-text: "Search (activates the panel)";
                        changed has-focus => {
                            root.search-focus(self.has-focus);
                        }
                        edited(text) => {
                            root.search-edited(text);
                        }
                        accepted => {
                            root.use-first();
                            search.clear-focus();
                        }
                    }
                    for item in root.items: Button {
                        text: item;
                        clicked => {
                            search.clear-focus();
                            root.use-item(item);
                        }
                    }
                    Rectangle {
                        vertical-stretch: 1;
                    }
                    Text {
                        text: root.status;
                        wrap: word-wrap;
                    }
                    Text {
                        text: root.footer;
                        font-weight: 700;
                        wrap: word-wrap;
                    }
                }
            }
        }
    }
}

const SAMPLE_ITEMS: [&str; 4] = [
    "Hello from Snipdeck",
    "Kind regards,\nAlex",
    "ünïcødé ✓ 日本語 🙂",
    "SELECT * FROM items;",
];

fn main() -> Result<(), slint::PlatformError> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_window_attributes_hook(platform::window_attributes)
        .select()?;

    let ui = Panel::new()?;
    let items = Rc::new(VecModel::from_iter(SAMPLE_ITEMS.map(SharedString::from)));
    ui.set_items(items.clone().into());

    ui.on_search_edited(move |query| {
        let query = query.to_lowercase();
        items.set_vec(
            SAMPLE_ITEMS
                .iter()
                .filter(|item| item.to_lowercase().contains(&query))
                .map(|item| SharedString::from(*item))
                .collect::<Vec<_>>(),
        );
    });
    ui.on_search_focus(|focused| {
        if focused {
            platform::begin_text_entry();
        } else {
            platform::end_text_entry();
        }
    });

    let weak = ui.as_weak();
    ui.on_use_item(move |text| use_item(&weak, &text));
    let weak = ui.as_weak();
    ui.on_use_first(move || {
        let ui = weak.unwrap();
        if let Some(text) = ui.get_items().row_data(0) {
            use_item(&weak, &text);
        }
    });
    let weak = ui.as_weak();
    ui.on_pointer_entered(move || refresh(&weak.unwrap()));

    // The foreground hook fires inside the Win32 message loop, possibly in a nested modal loop,
    // so the UI update is deferred to the Slint event loop instead of running re-entrantly.
    let weak = ui.as_weak();
    platform::install(&ui, move || {
        let _ = weak.upgrade_in_event_loop(|ui| {
            // A foreign window became foreground: any search session is over.
            ui.invoke_clear_search_focus();
            refresh(&ui);
        });
    });

    ui.window().on_close_requested(|| {
        let _ = slint::quit_event_loop();
        slint::CloseRequestResponse::HideWindow
    });
    ui.show()?;
    refresh(&ui);
    // Not `run()`: the panel hides itself over fullscreen apps and must not end the loop.
    slint::run_event_loop_until_quit()
}

fn use_item(weak: &slint::Weak<Panel>, text: &str) {
    // Shift is read from the OS: Slint's modifier state is only updated from keyboard
    // messages, which a never-activated window does not receive.
    let copy_only = platform::shift_held();
    let weak = weak.clone();
    platform::use_text(text, copy_only, move |outcome| {
        if let Some(ui) = weak.upgrade() {
            ui.set_status(outcome.into());
        }
    });
}

fn refresh(ui: &Panel) {
    ui.set_footer(platform::route().to_string().into());
    // SPEC 5.1: the panel steps aside while a fullscreen app is in front.
    let _ = if platform::fullscreen_in_front() {
        ui.hide()
    } else {
        ui.show()
    };
}
