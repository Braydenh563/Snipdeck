//! Spike S4: Mica chrome + frameless DWM window + theme/DPI/text-scale handling in Slint.
//!
//! Windows-only code lives in `platform/windows.rs`; `platform/stub.rs` keeps the spike
//! building and runnable (with the solid fallback chrome) on other hosts.
//!
//! Test knobs (environment variables, no rebuild needed):
//! - `SLINT_BACKEND=winit-femtovg | winit-software`  renderer (Slint's own variable)
//! - `S4_BACKDROP=mica | mica-alt | acrylic | none`  DWM system backdrop for the chrome
//! - `S4_EXTEND=full | 1px | none`                   DwmExtendFrameIntoClientArea margins
//! - `S4_SHADOW=1 | 0`                                winit undecorated shadow
//! - `S4_NOACTIVATE=1`                                add WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW (ADR-0003)
//! - `S4_KEEP_ACTIVE_LOOK=1`                          experimental: keep the frame "active" for Mica
//! - `S4_FORCE_SOLID=1`                               preview the solid fallback chrome

mod appearance;

#[cfg_attr(windows, path = "platform/windows.rs")]
#[cfg_attr(not(windows), path = "platform/stub.rs")]
mod platform;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use appearance::{choose_material, ChromeMaterial, Config, SystemAppearance};
use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, Global};

slint::include_modules!();

type RefreshFn = Rc<dyn Fn(&'static str)>;

thread_local! {
    static ON_SYSTEM_CHANGE: RefCell<Option<RefreshFn>> = const { RefCell::new(None) };
}

/// Coalesces bursts (one theme toggle sends several WM_SETTINGCHANGE messages).
static REFRESH_PENDING: AtomicBool = AtomicBool::new(false);

/// Re-reads the system appearance on the UI thread. Safe to call from a window procedure or
/// from a WinRT thread-pool callback: it only queues work on the Slint event loop, so it never
/// re-enters Slint or winit while they are dispatching a message.
pub(crate) fn schedule_refresh(reason: &'static str) {
    if REFRESH_PENDING.swap(true, Ordering::AcqRel) {
        return;
    }
    let queued = slint::invoke_from_event_loop(move || {
        REFRESH_PENDING.store(false, Ordering::Release);
        if let Some(f) = ON_SYSTEM_CHANGE.with(|f| f.borrow().clone()) {
            f(reason);
        }
    });
    if queued.is_err() {
        REFRESH_PENDING.store(false, Ordering::Release);
    }
}

fn to_slint((r, g, b): appearance::Rgb) -> slint::Color {
    slint::Color::from_rgb_u8(r, g, b)
}

/// Pushes theme, material and text scale into the Slint globals.
fn apply_to_ui(app: &PanelWindow, a: &SystemAppearance, material: ChromeMaterial) {
    Palette::get(app).set_dark(a.use_dark_tokens());
    let look = SystemLook::get(app);
    look.set_mica_active(matches!(material, ChromeMaterial::Backdrop(_)));
    look.set_high_contrast(a.high_contrast);
    look.set_text_scale(a.text_scale);
    // Windows 11 draws the 1px border via DWMWA_BORDER_COLOR; elsewhere Slint draws it.
    look.set_draw_own_border(!a.is_win11() || a.high_contrast);
    if let Some(hc) = a.hc_colors {
        look.set_hc_window(to_slint(hc.window));
        look.set_hc_window_text(to_slint(hc.window_text));
        look.set_hc_highlight(to_slint(hc.highlight));
        look.set_hc_highlight_text(to_slint(hc.highlight_text));
        look.set_hc_gray_text(to_slint(hc.gray_text));
    }
}

fn refresh(app: &PanelWindow, cfg: &Config, reason: &str) {
    let a = platform::read_appearance();
    let wanted = choose_material(&a, cfg);
    apply_to_ui(app, &a, wanted);

    let border = Palette::get(app).get_border_default();
    let border = (border.red(), border.green(), border.blue());
    let (actual, dwm_report) =
        platform::apply_window_material(app.window(), &a, wanted, cfg, border);
    if actual != wanted {
        apply_to_ui(app, &a, actual);
    }

    let renderer =
        std::env::var("SLINT_BACKEND").unwrap_or_else(|_| "default (winit-femtovg)".into());
    let diagnostics = format!(
        "last refresh: {reason}\n\
         OS build {build} · theme {theme} · transparency {transp} · high contrast {hc}\n\
         DPI scale {dpi:.2} · text scale {text:.2}\n\
         chrome: {actual:?}\n\
         DWM: {dwm_report}\n\
         renderer: {renderer}\n\
         config: {cfg:?}\n\
         sources: {sources}",
        build = a
            .os_build
            .map_or_else(|| "n/a".to_string(), |b| b.to_string()),
        theme = if a.use_dark_tokens() { "dark" } else { "light" },
        transp = if a.transparency_effects { "on" } else { "off" },
        hc = if a.high_contrast { "on" } else { "off" },
        dpi = app.window().scale_factor(),
        text = a.text_scale,
        sources = a.sources,
    );
    eprintln!("[s4] {}", diagnostics.replace('\n', " | "));
    SystemLook::get(app).set_diagnostics(diagnostics.into());
}

fn wire_window_controls(app: &PanelWindow) {
    let weak = app.as_weak();
    app.on_start_drag(move || {
        if let Some(app) = weak.upgrade() {
            app.window().with_winit_window(|w| {
                let _ = w.drag_window();
            });
        }
    });

    let weak = app.as_weak();
    app.on_start_resize(move |direction| {
        use winit::window::ResizeDirection;
        let direction = match direction {
            0 => ResizeDirection::East,
            1 => ResizeDirection::West,
            2 => ResizeDirection::South,
            _ => ResizeDirection::SouthEast,
        };
        if let Some(app) = weak.upgrade() {
            app.window().with_winit_window(|w| {
                let _ = w.drag_resize_window(direction);
            });
        }
    });

    app.on_close_panel(|| {
        let _ = slint::quit_event_loop();
    });
}

fn main() -> Result<(), slint::PlatformError> {
    let cfg = Config::from_env();
    let initial = platform::read_appearance();
    let initial_material = choose_material(&initial, &cfg);

    // Creation-time attributes (backdrop, corners, shadow) go through winit, so the first
    // frame already has them; everything else is applied once the HWND exists.
    platform::configure_backend(
        slint::BackendSelector::new().backend_name("winit".into()),
        cfg,
        initial_material,
    )
    .select()?;

    let app = PanelWindow::new()?;
    wire_window_controls(&app);
    apply_to_ui(&app, &initial, initial_material);

    let weak = app.as_weak();
    let on_change: RefreshFn = Rc::new(move |reason| {
        if let Some(app) = weak.upgrade() {
            refresh(&app, &cfg, reason);
        }
    });
    ON_SYSTEM_CHANGE.with(|f| *f.borrow_mut() = Some(on_change));

    // winit already turns WM_SETTINGCHANGE into ThemeChanged (Slint updates Palette.color-scheme
    // from it) and WM_DPICHANGED into ScaleFactorChanged (Slint relayouts). We only re-read.
    app.window().on_winit_window_event(|_, event| {
        match event {
            winit::event::WindowEvent::ThemeChanged(_) => schedule_refresh("winit ThemeChanged"),
            winit::event::WindowEvent::ScaleFactorChanged { .. } => {
                schedule_refresh("winit ScaleFactorChanged")
            }
            _ => {}
        }
        EventResult::Propagate
    });

    let weak = app.as_weak();
    slint::spawn_local(async move {
        let Some(window) = weak.upgrade() else { return };
        // Resolves once winit has created the native window (the HWND).
        let created = window.window().winit_window().await.is_ok();
        drop(window);
        let Some(app) = weak.upgrade() else { return };
        if created {
            platform::on_window_created(&app, &cfg);
        }
        refresh(&app, &cfg, "window created");
    })
    .map_err(|e| slint::PlatformError::Other(e.to_string()))?;

    app.run()
}
