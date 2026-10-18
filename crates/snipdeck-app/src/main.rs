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
    // Only now: reading the settings may move a damaged file aside, and opening the library may
    // migrate it. A second launch (which stops just above) must not touch the running copy's files.
    let (store, settings, entitlements, settings_folder) = open_library(args)?;
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

    let startup_chords = settings.hotkeys.clone();
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
    // Resuming after "Pause hotkeys" registers the chords in force then, not the launch ones.
    let chords_now = {
        let controller = Rc::downgrade(&controller);
        let fallback = startup_chords.clone();
        move || {
            controller.upgrade().map_or_else(
                || fallback.clone(),
                |controller| controller.borrow().settings().hotkeys.clone(),
            )
        }
    };
    let binding = Controller::bind(
        &controller,
        &window,
        effect_handler(Rc::clone(&hotkeys), chords_now, window.as_weak()),
    );
    BINDING.with(|b| *b.borrow_mut() = Some(binding.clone()));
    trace_window_events(&window);

    report_hotkey_problems(&host::register_all(
        &mut **hotkeys.borrow_mut(),
        &startup_chords,
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
    chords_now: impl Fn() -> snipdeck_app::settings::Hotkeys + 'static,
    window: slint::Weak<PanelWindow>,
) -> impl Fn(&Effect) + 'static {
    move |effect| match effect {
        Effect::Quit => {
            snipdeck_app::trace!("host: quitting the event loop");
            let _ = slint::quit_event_loop();
        }
        Effect::SetHotkeysPaused(paused) => {
            // The chords are read before the backend is borrowed: both are on this thread.
            let chords = chords_now();
            let report = host::set_paused(&mut **hotkeys.borrow_mut(), *paused, &chords);
            if report.not_removed > 0 {
                eprintln!(
                    "snipdeck: {} hotkeys could not be switched off",
                    report.not_removed
                );
            }
            if !*paused {
                report_hotkey_problems(&report.problems);
            }
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

/// With `SNIPDECK_LOG=1`: the pointer presses, focus, scale and size changes the window gets
/// from winit, so a log shows whether a click reached the window and where Slint placed it.
fn trace_window_events(window: &PanelWindow) {
    use slint::winit_030::{EventResult, winit::event::WindowEvent};
    if !snipdeck_app::trace::enabled() {
        return;
    }
    let mut cursor = (0.0, 0.0);
    window
        .window()
        .on_winit_window_event(move |slint_window, event| {
            match event {
                WindowEvent::CursorMoved { position, .. } => cursor = (position.x, position.y),
                WindowEvent::MouseInput { state, button, .. } => {
                    let scale = f64::from(slint_window.scale_factor());
                    snipdeck_app::trace!(
                        "pointer {state:?} {button:?} at physical ({:.0}, {:.0}) = logical ({:.1}, {:.1}), scale {scale}",
                        cursor.0,
                        cursor.1,
                        cursor.0 / scale,
                        cursor.1 / scale
                    );
                }
                WindowEvent::Focused(focused) => snipdeck_app::trace!("window focused {focused}"),
                WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                    snipdeck_app::trace!("window scale factor {scale_factor}");
                }
                WindowEvent::Resized(size) => {
                    snipdeck_app::trace!("window resized to {}x{} physical", size.width, size.height);
                }
                WindowEvent::CloseRequested => snipdeck_app::trace!("window close requested"),
                _ => {}
            }
            EventResult::Propagate
        });
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
/// settings changes are written to (`None` when they must not be: the demo, `--dark`, and a
/// settings file that could not be read or set aside).
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
    let loaded = snipdeck_app::paths::read_settings(&folder);
    let may_save = loaded.may_save();
    // A damaged file was moved aside: write the salvaged values back now, or the next launch would
    // find no file and fall back to the defaults (re-enabling pastes into apps the user excluded).
    if may_save && let Err(e) = snipdeck_app::paths::write_salvaged(&folder, &loaded) {
        eprintln!("snipdeck: could not write the recovered settings: {e}");
    }
    let mut settings = loaded.settings;
    if !may_save {
        eprintln!(
            "snipdeck: the settings file in {} could not be read or set aside; running with the \
             defaults and not saving changes, so the file stays as it is",
            folder.display()
        );
    }
    // `--dark` is a one-run override: do not write it into the user's settings.
    let persist = !args.dark && may_save;
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
        } else {
            snipdeck_app::trace!("event before the Panel was bound: dropped");
        }
    });
}

/// Runs `f` with the binding on the UI thread, whichever thread asks.
fn on_ui_thread(f: impl FnOnce(&Binding) + Send + 'static) {
    // Fails when the event loop has already ended (at shutdown: nothing to deliver to) or when
    // no window backend is selected yet (an event from a service that started early).
    if let Err(e) = slint::invoke_from_event_loop(move || with_binding(f)) {
        snipdeck_app::trace!("event for the UI thread dropped: {e}");
    }
}
