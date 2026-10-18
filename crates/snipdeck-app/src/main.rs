//! The Snipdeck app: parses the command line, opens the library, connects to the platform,
//! shows the Panel and runs the event loop. All behaviour lives in the library crate; this file
//! only starts things in order.
//!
//! Windows is the product. Off Windows the same binary runs on the platform fakes, which is how
//! `cargo run -p snipdeck-app -- --demo` shows the Panel on a development machine.

#![forbid(unsafe_code)]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "this is the command-line entry point: usage and start-up failures go to the terminal"
)]

use std::{cell::RefCell, process::ExitCode, rc::Rc};

use slint::{
    ComponentHandle,
    winit_030::{WinitWindowAccessor, winit},
};
use snipdeck_app::{
    binding::Binding,
    cli::{self, Args},
    controller::{Controller, Effect},
    demo, host,
    platform::{self, Callbacks, Host, Start},
    screenshot,
    settings::{Settings, Theme},
};
use snipdeck_core::{Entitlements, Tier, Timestamp};
use snipdeck_store::Store;
use snipdeck_ui::PanelWindow;

thread_local! {
    /// The binding, for events that arrive from other threads and are handed to this one.
    static BINDING: RefCell<Option<Binding>> = const { RefCell::new(None) };
}

fn main() -> ExitCode {
    let args = match cli::parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("snipdeck: {e}\n\n{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };
    if args.help {
        println!("{}", cli::USAGE);
        return ExitCode::SUCCESS;
    }
    let outcome = match &args.screenshot {
        Some(path) => take_screenshot(path, args.dark),
        None => run(&args),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("snipdeck: {e}");
            ExitCode::FAILURE
        }
    }
}

fn take_screenshot(path: &std::path::Path, dark: bool) -> Result<(), String> {
    let frame = screenshot::render_demo(dark)?;
    screenshot::write_png(path, &frame)?;
    println!("wrote {}", path.display());
    Ok(())
}

fn run(args: &Args) -> Result<(), String> {
    let (store, settings, entitlements) = open_library(args)?;

    // Events from service threads become calls on this thread, through the binding.
    let callbacks = Callbacks {
        on_hotkey: Box::new(|action| on_ui_thread(move |b| b.hotkey(action))),
        on_second: Box::new(|| on_ui_thread(Binding::show)),
        on_target_changed: Box::new(|| with_binding(Binding::target_changed)),
    };
    let host = match platform::connect(callbacks, !args.demo)? {
        Start::Run(host) => *host,
        Start::AlreadyRunning => return Ok(()),
    };
    let Host {
        services,
        target,
        mut hotkeys,
        attach_panel,
        guard: _guard,
    } = host;

    select_backend()?;
    let window = PanelWindow::new().map_err(|e| format!("creating the Panel: {e}"))?;
    let width = f32::from(u16::try_from(settings.panel_width).unwrap_or(320));
    window
        .window()
        .set_size(slint::LogicalSize::new(width, 480.0));

    let hotkey_settings = settings.hotkeys.clone();
    let controller = Controller::new(
        store,
        services,
        target,
        Box::new(host::SystemClock),
        settings,
        entitlements,
    )
    .map_err(|e| format!("loading the library: {e}"))?;
    let controller = Rc::new(RefCell::new(controller));

    // Without a tray a closed Panel could never be reopened off Windows (no hotkeys there), so
    // the dev build quits when the Panel is closed.
    let weak = window.as_weak();
    let binding = Controller::bind(&controller, &window, move |effect| match effect {
        Effect::HidePanel if cfg!(not(windows)) => {
            let _ = slint::quit_event_loop();
        }
        Effect::DragWindow => {
            if let Some(window) = weak.upgrade() {
                // The Panel has no OS caption: the header starts the move itself.
                let _ = window
                    .window()
                    .with_winit_window(winit::window::Window::drag_window);
            }
        }
        _ => {}
    });
    BINDING.with(|b| *b.borrow_mut() = Some(binding.clone()));

    let problems = host::register_all(&mut *hotkeys, &hotkey_settings);
    for problem in &problems {
        eprintln!(
            "snipdeck: cannot register {} for {:?}: {}",
            problem.chord, problem.action, problem.failure
        );
    }
    binding.hotkey_problems(&problems);

    if args.hidden {
        controller.borrow_mut().hide();
    } else {
        window
            .show()
            .map_err(|e| format!("showing the Panel: {e}"))?;
        attach_when_created(&window, attach_panel);
    }
    slint::run_event_loop_until_quit().map_err(|e| format!("event loop: {e}"))
}

/// The library, the settings and what the user may do.
fn open_library(args: &Args) -> Result<(Store, Settings, Entitlements), String> {
    let now = Timestamp::now();
    if args.demo {
        let settings = Settings {
            theme: if args.dark { Theme::Dark } else { Theme::Light },
            ..Settings::default()
        };
        let store = demo::store(now).map_err(|e| format!("demo library: {e}"))?;
        return Ok((store, settings, Entitlements::new(Tier::Pro)));
    }
    let folder = match &args.data_dir {
        Some(dir) => dir.clone(),
        None => snipdeck_app::paths::data_dir(&|name| std::env::var(name).ok(), cfg!(windows))
            .ok_or("cannot find a data folder; pass --data-dir <folder>")?,
    };
    let mut settings = snipdeck_app::paths::load_settings(&folder);
    if args.dark {
        settings.theme = Theme::Dark;
    }
    let store = Store::open(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    // The licence is not wired into the app yet: everyone is Free until it is.
    Ok((store, settings, Entitlements::new(Tier::Free)))
}

/// Winit with the software renderer; windows are created un-activated so a click on the Panel
/// never takes the focus from the app being pasted into (ADR-0003).
fn select_backend() -> Result<(), String> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_window_attributes_hook(|attributes| {
            #[cfg(windows)]
            let attributes = {
                use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
                attributes.with_skip_taskbar(true)
            };
            attributes.with_active(false)
        })
        .select()
        .map_err(|e| format!("selecting the window backend: {e}"))
}

/// The native window only exists once the event loop runs: look for its handle then.
fn attach_when_created(window: &PanelWindow, attach: Box<dyn Fn(isize)>) {
    let weak = window.as_weak();
    slint::Timer::single_shot(std::time::Duration::ZERO, move || {
        if let Some(handle) = weak.upgrade().and_then(|w| native_handle(&w)) {
            attach(handle);
        }
    });
}

#[cfg(windows)]
fn native_handle(window: &PanelWindow) -> Option<isize> {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    window
        .window()
        .with_winit_window(|w| match w.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
            _ => None,
        })
        .flatten()
}

#[cfg(not(windows))]
fn native_handle(_window: &PanelWindow) -> Option<isize> {
    None
}

/// Runs `f` with the binding, if there is one yet. UI thread only.
fn with_binding(f: impl FnOnce(&Binding)) {
    BINDING.with(|b| {
        if let Some(binding) = b.borrow().as_ref() {
            f(binding);
        }
    });
}

/// Runs `f` with the binding on the UI thread, whichever thread asks.
fn on_ui_thread(f: impl FnOnce(&Binding) + Send + 'static) {
    // Fails only when the event loop has already ended, at shutdown: nothing to deliver to.
    let _ = slint::invoke_from_event_loop(move || with_binding(f));
}
