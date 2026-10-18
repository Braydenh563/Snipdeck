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

use std::{cell::RefCell, path::PathBuf, process::ExitCode, rc::Rc, sync::Arc};

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
    settings_store::{FileSink, SettingsSaver},
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
    let (store, settings, entitlements, settings_folder) = open_library(args)?;

    // Events from service threads become calls on this thread, through the binding.
    let look = platform::system_look();
    let callbacks = Callbacks {
        on_hotkey: Box::new(|action| on_ui_thread(move |b| b.hotkey(action))),
        on_second: Box::new(|| on_ui_thread(Binding::show)),
        on_target_changed: Box::new(|| with_binding(Binding::target_changed)),
        on_tray: Box::new(|event| on_ui_thread(move |b| b.tray(event))),
        on_look_changed: {
            let look = Arc::clone(&look);
            // Read on the UI thread: the value is fresher than the one the watcher saw.
            Box::new(move || {
                let look = Arc::clone(&look);
                on_ui_thread(move |b| b.system_look(look.read()));
            })
        },
    };
    let host = match platform::connect(callbacks, !args.demo)? {
        Start::Run(host) => *host,
        Start::AlreadyRunning => return Ok(()),
    };
    let Host {
        services,
        target,
        hotkeys,
        attach_panel,
        tray,
        warnings,
        keep_alive: _keep_alive,
    } = host;
    for warning in &warnings {
        eprintln!("snipdeck: {warning}");
    }

    select_backend()?;
    let window = PanelWindow::new().map_err(|e| format!("creating the Panel: {e}"))?;
    let width = f32::from(u16::try_from(settings.panel_width).unwrap_or(320));
    window
        .window()
        .set_size(slint::LogicalSize::new(width, 480.0));

    let hotkey_settings = settings.hotkeys.clone();
    let saver = settings_folder
        .map(|folder| SettingsSaver::new(Box::new(FileSink::new(folder)), settings.clone()));
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
    {
        let mut controller = controller.borrow_mut();
        controller.set_system_look(look.read());
        if let Some(saver) = saver {
            controller.set_settings_saver(saver);
        }
    }

    // The tray makes closing the Panel safe (it hides, and the icon brings it back). Whether an
    // icon is in the notification area is reported by the tray itself (`TrayEvent::IconPresent`,
    // also when Explorer restarts), so the controller needs no flag from here. Without a tray
    // (off Windows, or it failed to start) or while no icon is there, closing the Panel quits.
    let mut tray = tray;
    if let Some(tray) = &mut tray
        && let Err(e) = tray.show()
    {
        eprintln!("snipdeck: cannot show the tray icon yet: {e}");
    }

    let hotkeys = Rc::new(RefCell::new(hotkeys));
    let binding = Controller::bind(
        &controller,
        &window,
        effect_handler(
            Rc::clone(&hotkeys),
            hotkey_settings.clone(),
            window.as_weak(),
        ),
    );
    BINDING.with(|b| *b.borrow_mut() = Some(binding.clone()));

    report_hotkey_problems(&host::register_all(
        &mut **hotkeys.borrow_mut(),
        &hotkey_settings,
    ));

    if args.hidden {
        controller.borrow_mut().hide();
    } else {
        window
            .show()
            .map_err(|e| format!("showing the Panel: {e}"))?;
        attach_when_created(&window, attach_panel);
    }
    let ran = slint::run_event_loop_until_quit().map_err(|e| format!("event loop: {e}"));
    // Settings changed in the last moments are still waiting for their quiet time.
    if let Err(e) = controller.borrow_mut().flush_settings() {
        eprintln!("snipdeck: could not save the settings: {e}");
    }
    ran
}

type Hotkeys = Rc<RefCell<Box<dyn host::HotkeyBackend>>>;

/// What the host does with the controller's [`Effect`]s: quit, pause or resume the hotkeys,
/// start a window drag. (The Picker, capture and paste-next effects have no feature behind them
/// yet.)
fn effect_handler(
    hotkeys: Hotkeys,
    chords: snipdeck_app::settings::Hotkeys,
    window: slint::Weak<PanelWindow>,
) -> impl Fn(&Effect) + 'static {
    move |effect| match effect {
        Effect::Quit => {
            let _ = slint::quit_event_loop();
        }
        Effect::SetHotkeysPaused(true) => {
            let failed = host::unregister_all(&mut **hotkeys.borrow_mut());
            if failed > 0 {
                eprintln!("snipdeck: {failed} hotkeys could not be switched off");
            }
        }
        Effect::SetHotkeysPaused(false) => {
            report_hotkey_problems(&host::register_all(&mut **hotkeys.borrow_mut(), &chords));
        }
        // There is no main window yet (M4): the Panel stands in for it.
        Effect::OpenMainWindow => with_binding(Binding::show),
        Effect::DragWindow => {
            if let Some(window) = window.upgrade() {
                // The Panel has no OS caption: the header starts the move itself.
                let _ = window
                    .window()
                    .with_winit_window(winit::window::Window::drag_window);
            }
        }
        _ => {}
    }
}

/// Prints the hotkeys that could not be registered and tells the user in the Panel.
fn report_hotkey_problems(problems: &[host::HotkeyProblem]) {
    for problem in problems {
        eprintln!(
            "snipdeck: cannot register {} for {:?}: {}",
            problem.chord, problem.action, problem.failure
        );
    }
    with_binding(|b| b.hotkey_problems(problems));
}

/// What [`open_library`] returns: the library, the settings, what the user may do, and the folder
/// settings changes are written to (`None` when they must not be: the demo, and `--dark`).
type Library = (Store, Settings, Entitlements, Option<PathBuf>);

/// The library, the settings and what the user may do.
fn open_library(args: &Args) -> Result<Library, String> {
    let now = Timestamp::now();
    if args.demo {
        let settings = Settings {
            theme: if args.dark { Theme::Dark } else { Theme::Light },
            ..Settings::default()
        };
        let store = demo::store(now).map_err(|e| format!("demo library: {e}"))?;
        return Ok((store, settings, Entitlements::new(Tier::Pro), None));
    }
    let folder = match &args.data_dir {
        Some(dir) => dir.clone(),
        None => snipdeck_app::paths::data_dir(&|name| std::env::var(name).ok(), cfg!(windows))
            .ok_or("cannot find a data folder; pass --data-dir <folder>")?,
    };
    let mut settings = snipdeck_app::paths::load_settings(&folder);
    // `--dark` is a one-run override: do not write it into the user's settings.
    let persist = !args.dark;
    if args.dark {
        settings.theme = Theme::Dark;
    }
    let store = Store::open(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    // The licence is not wired into the app yet: everyone is Free until it is.
    Ok((
        store,
        settings,
        Entitlements::new(Tier::Free),
        persist.then_some(folder),
    ))
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
