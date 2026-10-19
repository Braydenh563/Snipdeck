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

use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    process::ExitCode,
    rc::Rc,
    sync::Arc,
};

use slint::{
    ComponentHandle,
    winit_030::{WinitWindowAccessor, winit},
};
use snipdeck_app::{
    binding::Binding,
    cli::{self, Args},
    controller::{Controller, Effect},
    crash, demo,
    drafts::Drafts,
    fill_binding::FillUi,
    headless,
    help_binding::{HelpUi, SystemHelpHost},
    host, key_provider,
    main_binding::{self, MainUi},
    maintenance::{self, Maintenance},
    onboarding_binding::OnboardingUi,
    picker_binding::{AnchorMemory, PickerUi},
    platform::{self, Callbacks, Host, Start},
    screenshot,
    settings::{Settings, Theme},
    settings_binding::{PanelTaskbar, SettingsUi, SystemHost},
    settings_store::{FileSink, SettingsSaver},
    toast_binding::ToastUi,
    window_kind,
};
use snipdeck_core::{Entitlements, Tier, Timestamp};
use snipdeck_store::Store;
use snipdeck_ui::PanelWindow;

thread_local! {
    /// The binding, for events that arrive from other threads and are handed to this one.
    static BINDING: RefCell<Option<Binding>> = const { RefCell::new(None) };
}

fn main() -> ExitCode {
    // First, so that a panic anywhere after this leaves a report (the data folder is told to the
    // hook once it is known).
    crash::install(Timestamp::now);
    let args = match cli::parse(std::env::args_os().skip(1)) {
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
    if args.check_db || args.troubleshoot {
        return headless(&args);
    }
    snipdeck_app::trace!(
        "start: snipdeck-app {} demo={} hidden={} dark={}",
        env!("CARGO_PKG_VERSION"),
        args.demo,
        args.hidden,
        args.dark
    );
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

/// `--check-db` and `--troubleshoot`: print and exit, without starting the app.
fn headless(args: &Args) -> ExitCode {
    let Some(folder) = headless::folder_or_default(args.data_dir.as_ref()) else {
        eprintln!("snipdeck: cannot find a data folder; pass --data-dir <folder>");
        return ExitCode::from(2);
    };
    let mut worst = 0;
    if args.check_db {
        let out = headless::check_db(&folder);
        println!("{}", out.text);
        worst = worst.max(out.code);
    }
    if args.troubleshoot {
        let today = Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::system())
            .date();
        let out = headless::troubleshoot(&folder, today, std::env::consts::OS);
        println!("{}", out.text);
        worst = worst.max(out.code);
    }
    ExitCode::from(worst)
}

fn take_screenshot(path: &std::path::Path, dark: bool) -> Result<(), String> {
    let frame = screenshot::render_demo(dark)?;
    screenshot::write_png(path, &frame)?;
    println!("wrote {}", path.display());
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "start-up is one ordered sequence: each step needs the one before"
)]
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
    // Before any service thread starts: an early event handed to the UI thread needs a backend
    // (without one `invoke_from_event_loop` fails and the event, e.g. a second launch's show
    // request, would be lost).
    select_backend()?;
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
        popups,
        tray,
        warnings,
        keep_alive: _keep_alive,
    } = host;
    for warning in &warnings {
        eprintln!("snipdeck: {warning}");
    }

    // The interface language, before any window exists ("" follows the system).
    if let Err(e) = slint::select_bundled_translation(&settings.language) {
        eprintln!("snipdeck: no translation for {:?}: {e}", settings.language);
    }
    let window = PanelWindow::new().map_err(|e| format!("creating the Panel: {e}"))?;
    let width = f32::from(u16::try_from(settings.panel_width).unwrap_or(320));
    window
        .window()
        .set_size(slint::LogicalSize::new(width, 480.0));

    let startup_chords = settings.hotkeys.clone();
    let saver = settings_folder
        .clone()
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
        // The item editor: drafts live in the data folder (the demo has none), the library key
        // is wrapped by DPAPI, Browse opens the Windows file dialog, and the crash toast's
        // "Open log" opens the reports folder.
        if let Some(folder) = &settings_folder {
            controller.set_drafts(Drafts::in_data_dir(folder));
        }
        if let Some(provider) = key_provider::provider() {
            controller.set_key_provider(provider);
        }
        if let Some(picker) = platform::file_picker() {
            controller.set_file_picker(picker);
        }
        controller.set_log_opener(Box::new(crash::open_log_folder));
        controller.unlock_library();
    }

    // The tray makes closing the Panel safe (it hides, and the icon brings it back). Whether an
    // icon is in the notification area is reported by the tray itself (`TrayEvent::IconPresent`,
    // also when Explorer restarts), so the controller needs no flag from here. Without a tray
    // (off Windows, or it failed to start) or while no icon is there, closing the Panel quits.
    let tray = Rc::new(RefCell::new(tray));
    show_tray(&mut tray.borrow_mut(), &window);

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
    let main_ui = Rc::new(MainUi::new(&controller));
    let panel_taskbar: PanelTaskbar = {
        let window = window.as_weak();
        Rc::new(move |on| {
            if let Some(window) = window.upgrade() {
                snipdeck_app::native_window::set_panel_taskbar(&window, on);
            }
        })
    };
    let settings_ui = Rc::new(SettingsUi::new(
        &controller,
        settings_folder.clone(),
        Rc::new(SystemHost::new(
            Rc::clone(&hotkeys),
            Rc::clone(&panel_taskbar),
        )),
    ));
    let help_ui = Rc::new(HelpUi::new(
        &controller,
        settings_folder.clone(),
        Rc::new(SystemHelpHost),
    ));
    // The welcome tour window, made when it is first opened (the first run, or Help).
    let onboarding_ui = OnboardingUi::new(
        &controller,
        Rc::new(SystemHost::new(
            Rc::clone(&hotkeys),
            Rc::clone(&panel_taskbar),
        )),
    );
    // The Panel's native window exists once it has been shown. Attached on the first show, which
    // a `--hidden` start reaches later, from the tray or the hotkey.
    let attach_now: Rc<dyn Fn()> = {
        let attach_panel: Rc<dyn Fn(isize, bool)> = Rc::from(attach_panel);
        let attached = Rc::new(Cell::new(false));
        let window = window.as_weak();
        let controller = Rc::downgrade(&controller);
        Rc::new(move || {
            if attached.get() {
                return;
            }
            let Some(panel) = window.upgrade() else {
                return;
            };
            let (attach, attached) = (Rc::clone(&attach_panel), Rc::clone(&attached));
            let (controller, weak) = (controller.clone(), window.clone());
            attach_when_created(
                &panel,
                Box::new(move |hwnd| {
                    let in_taskbar = controller
                        .upgrade()
                        .is_some_and(|c| c.borrow().settings().panel_in_taskbar);
                    attach(hwnd, in_taskbar);
                    attached.set(true);
                    if in_taskbar && let Some(panel) = weak.upgrade() {
                        snipdeck_app::native_window::set_panel_taskbar(&panel, true);
                    }
                }),
            );
        })
    };
    let on_effect: Rc<dyn Fn(&Effect)> = Rc::new(effect_handler(
        Rc::clone(&hotkeys),
        chords_now,
        window.as_weak(),
        Rc::clone(&main_ui),
        Rc::clone(&tray),
        Rc::clone(&settings_ui),
        Rc::clone(&help_ui),
        Rc::clone(&attach_now),
    ));
    // The Picker, the fill form and the toast window (made when first needed).
    let _popups = {
        let anchor = AnchorMemory::default();
        let others = {
            let (main_ui, settings_ui) = (Rc::clone(&main_ui), Rc::clone(&settings_ui));
            Rc::new(move || {
                main_ui.window().is_some_and(|w| w.window().is_visible())
                    || settings_ui
                        .window()
                        .is_some_and(|w| w.window().is_visible())
            })
        };
        (
            PickerUi::new(&controller, Rc::clone(&popups), Rc::clone(&anchor)),
            FillUi::new(&controller, Rc::clone(&popups), anchor),
            ToastUi::new(&controller, popups, others),
        )
    };
    let binding = Controller::bind(&controller, &window, {
        let on_effect = Rc::clone(&on_effect);
        move |effect| on_effect(effect)
    });
    BINDING.with(|b| *b.borrow_mut() = Some(binding.clone()));
    // The Panel steps down from topmost while one of the app's regular windows has the focus.
    snipdeck_app::native_window::on_own_window_active(|active| {
        with_binding(|b| b.own_window_active(active));
    });
    // The item editor opens from the Panel, the Main window and the Picker; one binding draws it.
    let _item_editor = Controller::bind_item_editor(&controller);
    // Frame changes are remembered only once the stored place has been restored.
    let frames_armed = Rc::new(Cell::new(false));
    watch_window_events(&window, binding.clone(), Rc::clone(&frames_armed));

    report_hotkey_problems(&host::register_all(
        &mut **hotkeys.borrow_mut(),
        &startup_chords,
    ));

    if args.hidden {
        controller.borrow_mut().hide();
        frames_armed.set(true);
    } else {
        window
            .show()
            .map_err(|e| format!("showing the Panel: {e}"))?;
        restore_frame(&window);
        frames_armed.set(true);
        attach_now();
        // A first run (nothing in the library, tour not seen) opens the welcome tour.
        let tour_due = controller.borrow().tour_due();
        if tour_due && let Err(e) = onboarding_ui.open() {
            eprintln!("snipdeck: cannot open the welcome tour: {e}");
        }
    }
    // The backup, the Trash purge (30 days, SPEC 5.7a) and the weekly check start 2 s after the
    // first show and repeat every 6 h. Not in the demo, which has no data folder.
    let _maintenance_timers = settings_folder.map(|folder| {
        start_maintenance(
            Maintenance::new(folder, Box::new(host::SystemClock)),
            &controller,
        )
    });
    let ran = slint::run_event_loop_until_quit().map_err(|e| format!("event loop: {e}"));
    // Settings changed in the last moments are still waiting for their quiet time.
    if let Err(e) = controller.borrow_mut().flush_settings() {
        eprintln!("snipdeck: could not save the settings: {e}");
    }
    ran
}

type SharedTray = Rc<RefCell<Option<Box<dyn snipdeck_platform::tray::Tray>>>>;

/// Starts the timers that run `maintenance` on the event loop: once [`maintenance::FIRST_DELAY`]
/// from now, then every [`maintenance::INTERVAL`]. The timers stop when the returned value drops.
fn start_maintenance<C, K, T>(
    maintenance: Maintenance,
    controller: &Rc<RefCell<Controller<Store, C, K, T>>>,
) -> (slint::Timer, slint::Timer)
where
    C: snipdeck_platform::pipeline::Clipboard + host::ClipboardText + 'static,
    K: snipdeck_platform::pipeline::Keyboard + 'static,
    T: snipdeck_platform::pipeline::Timing + 'static,
{
    let maintenance = Rc::new(RefCell::new(maintenance));
    let controller = Rc::downgrade(controller);
    let run: Rc<dyn Fn()> = Rc::new(move || {
        let Some(controller) = controller.upgrade() else {
            return;
        };
        let reported =
            maintenance::run(&mut maintenance.borrow_mut(), &mut controller.borrow_mut());
        // A toast the run set is drawn (and its timer started) by a redraw; the borrow is released.
        if reported > 0 {
            with_binding(Binding::redraw);
        }
    });
    let first = slint::Timer::default();
    let every = slint::Timer::default();
    first.start(slint::TimerMode::SingleShot, maintenance::FIRST_DELAY, {
        let run = Rc::clone(&run);
        move || run()
    });
    every.start(
        slint::TimerMode::Repeated,
        maintenance::INTERVAL,
        move || run(),
    );
    (first, every)
}

/// Gives the tray its menu words (the language the UI is in) and puts the icon in the
/// notification area. Neither failure is fatal: the app runs without the icon.
fn show_tray(tray: &mut Option<Box<dyn snipdeck_platform::tray::Tray>>, window: &PanelWindow) {
    let Some(tray) = tray.as_mut() else {
        return;
    };
    if let Err(e) = tray.set_labels(snipdeck_app::wording::tray_labels(window)) {
        eprintln!("snipdeck: cannot set the tray menu texts: {e}");
    }
    if let Err(e) = tray.show() {
        eprintln!("snipdeck: cannot show the tray icon yet: {e}");
    }
}

type Hotkeys = Rc<RefCell<Box<dyn host::HotkeyBackend>>>;

/// What the host does with the controller's [`Effect`]s: quit, pause or resume the hotkeys,
/// start a window drag, open the Main window.
#[allow(
    clippy::too_many_arguments,
    reason = "the host's handles, each used by one effect; wired once in main"
)]
fn effect_handler(
    hotkeys: Hotkeys,
    chords_now: impl Fn() -> snipdeck_app::settings::Hotkeys + 'static,
    window: slint::Weak<PanelWindow>,
    main_ui: Rc<MainUi>,
    tray: SharedTray,
    settings_ui: Rc<SettingsUi>,
    help_ui: Rc<HelpUi>,
    attach_panel: Rc<dyn Fn()>,
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
            // The Panel's menu can switch this too: the tray's check mark follows.
            if let Some(tray) = tray.borrow_mut().as_mut()
                && let Err(e) = tray.set_hotkeys_paused(*paused)
            {
                eprintln!("snipdeck: cannot update the tray's check mark: {e}");
            }
        }
        // Created the first time, then shown and raised again; the Panel stays as it is.
        Effect::OpenMainWindow => {
            if let Err(e) = main_ui.open() {
                eprintln!("snipdeck: cannot open the main window: {e}");
            }
        }
        // The Panel's "All commands…" and Ctrl+K: the Main window, with the palette open.
        Effect::OpenCommands => match main_ui.open() {
            Ok(()) => {
                if let Some(window) = main_ui.window() {
                    window.invoke_open_palette();
                }
            }
            Err(e) => eprintln!("snipdeck: cannot open the command palette: {e}"),
        },
        Effect::OpenSettings => {
            if let Err(e) = settings_ui.open() {
                eprintln!("snipdeck: cannot open the settings: {e}");
            }
        }
        Effect::OpenListEditor => {
            if let Err(e) = main_ui.bind() {
                eprintln!("snipdeck: cannot open the list editor: {e}");
            }
        }
        Effect::DragWindow => {
            if let Some(window) = window.upgrade() {
                // The Panel has no OS caption: the header starts the move itself.
                let _ = window
                    .window()
                    .with_winit_window(winit::window::Window::drag_window);
            }
        }
        Effect::ResizeWindow(edge) => {
            if let Some(window) = window.upgrade() {
                // No OS frame either: a grip starts the OS resize loop. The window's own
                // minimum and maximum width (260 and 480) hold while it runs; the size it
                // ends at reaches `Binding::frame_changed` through the window events.
                let direction = resize_direction(*edge);
                let _ = window
                    .window()
                    .with_winit_window(|w| w.drag_resize_window(direction));
            }
        }
        Effect::ShowPanel => {
            if let Some(window) = window.upgrade() {
                restore_frame(&window);
            }
            // A `--hidden` start has no native Panel window until now.
            attach_panel();
        }
        // The Help window (Troubleshoot, About), made the first time and raised after that; the
        // user guide opens in the browser.
        Effect::OpenHelp(topic) => {
            if let Err(e) = help_ui.open(*topic) {
                eprintln!("snipdeck: cannot open the help: {e}");
            }
        }
        // The binding has already acted: placed the window, or opened the item editor.
        Effect::HidePanel | Effect::PlaceWindow(_) | Effect::NewItem | Effect::EditItem(_) => {}
    }
}

fn resize_direction(edge: snipdeck_app::frame::Edge) -> winit::window::ResizeDirection {
    use snipdeck_app::frame::Edge;
    use winit::window::ResizeDirection;
    match edge {
        Edge::Left => ResizeDirection::West,
        Edge::Right => ResizeDirection::East,
        Edge::Top => ResizeDirection::North,
        Edge::Bottom => ResizeDirection::South,
        Edge::TopLeft => ResizeDirection::NorthWest,
        Edge::TopRight => ResizeDirection::NorthEast,
        Edge::BottomLeft => ResizeDirection::SouthWest,
        Edge::BottomRight => ResizeDirection::SouthEast,
    }
}

/// Puts the Panel where the user left it on the monitor it is on.
fn restore_frame(window: &PanelWindow) {
    if let Some(monitor) = window
        .window()
        .with_winit_window(snipdeck_app::native_window::monitor_of)
        .flatten()
    {
        with_binding(|b| b.restore_frame(&monitor));
    }
}

/// Watches what winit tells the Panel window: a resize or a move is reported to the binding
/// (which clamps and remembers the frame per monitor) once `armed`; and with `SNIPDECK_LOG=1`
/// the pointer presses, focus, scale and size changes are logged, so a log shows whether a click
/// reached the window and where Slint placed it.
fn watch_window_events(window: &PanelWindow, binding: Binding, armed: Rc<Cell<bool>>) {
    use slint::winit_030::{EventResult, winit::event::WindowEvent};
    let tracing = snipdeck_app::trace::enabled();
    let mut cursor = (0.0, 0.0);
    window
        .window()
        .on_winit_window_event(move |slint_window, event| {
            let moved = matches!(event, WindowEvent::Moved(_));
            if (moved || matches!(event, WindowEvent::Resized(_))) && armed.get() {
                // winit's own numbers: Slint has not seen this event yet.
                let frame = slint_window.with_winit_window(|w| {
                    let (at, size) = (w.outer_position().ok()?, w.inner_size());
                    Some((
                        snipdeck_app::native_window::monitor_of(w)?,
                        snipdeck_app::frame::Rect {
                            x: at.x,
                            y: at.y,
                            w: size.width,
                            h: size.height,
                        },
                    ))
                });
                if let Some(Some((monitor, rect))) = frame {
                    if moved {
                        binding.frame_moved(&monitor, rect);
                    } else {
                        binding.frame_changed(&monitor, rect);
                    }
                }
            }
            if !tracing {
                return EventResult::Propagate;
            }
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
        snipdeck_app::trace!("demo library");
        return Ok((store, settings, Entitlements::new(Tier::Pro), None));
    }
    let folder = match &args.data_dir {
        Some(dir) => dir.clone(),
        None => snipdeck_app::paths::data_dir(&|name| std::env::var(name).ok(), cfg!(windows))
            .ok_or("cannot find a data folder; pass --data-dir <folder>")?,
    };
    // Where a crash report goes (set for any real library, also when the settings stay unsaved).
    crash::set_data_dir(&folder);
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
    let mut store = Store::open(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    // A new library has no board, and the Panel cannot make one: without this every control of a
    // first run has nothing to act on (an empty board menu, no tabs, no rows).
    match snipdeck_app::starter::ensure_board(&mut store, now) {
        Ok(made) => snipdeck_app::trace!("real library; starter board made: {made}"),
        Err(e) => eprintln!("snipdeck: could not create the first board: {e}"),
    }
    // The saved licence key, checked again on every start; none or a bad one is the Free plan.
    let entitlements = snipdeck_app::licence_store::entitlements(&folder, now);
    Ok((store, settings, entitlements, persist.then_some(folder)))
}

/// Winit with the software renderer; the Panel and the other floating windows are created
/// un-activated so a click on them never takes the focus from the app being pasted into
/// (ADR-0003). The Main window and its editors are ordinary windows: activated, in the taskbar.
fn select_backend() -> Result<(), String> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_window_attributes_hook(|attributes| {
            // Slint runs this inside `X::new()`: a regular window is one created under
            // `RegularWindow::create`.
            let regular = main_binding::regular_window_being_shown();
            snipdeck_app::trace!("window attributes: regular={regular}");
            let plan = window_kind::plan(regular);
            #[cfg(windows)]
            let attributes = {
                use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
                let attributes = attributes.with_skip_taskbar(plan.skip_taskbar);
                if plan.window_icon {
                    with_app_icon(attributes)
                } else {
                    attributes
                }
            };
            attributes.with_active(plan.active)
        })
        .select()
        .map_err(|e| format!("selecting the window backend: {e}"))
}

/// The application icon for a regular window: the small mark for the title bar and the large one
/// for the taskbar and Alt+Tab, each asked of the executable's icon resource at its exact size.
/// A missing resource (a development build) leaves the window without an icon, never an error.
#[cfg(windows)]
fn with_app_icon(attributes: winit::window::WindowAttributes) -> winit::window::WindowAttributes {
    use winit::{
        dpi::PhysicalSize,
        platform::windows::{IconExtWindows, WindowAttributesExtWindows},
        window::Icon,
    };
    let (small, large) = snipdeck_platform::windows::dpi::window_icon_sizes();
    let icon = |(cx, cy): (u32, u32)| {
        Icon::from_resource(
            snipdeck_platform::windows::dpi::APP_ICON_RESOURCE,
            Some(PhysicalSize::new(cx, cy)),
        )
        .ok()
    };
    attributes
        .with_window_icon(icon(small))
        .with_taskbar_icon(icon(large))
}

/// The native window only exists once the event loop runs: look for its handle then.
fn attach_when_created(window: &PanelWindow, attach: Box<dyn Fn(isize)>) {
    let weak = window.as_weak();
    slint::Timer::single_shot(std::time::Duration::ZERO, move || {
        if let Some(handle) = weak
            .upgrade()
            .and_then(|w| snipdeck_app::native_window::handle(&w))
        {
            attach(handle);
        }
    });
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
    // Fails when the event loop has already ended (at shutdown: nothing to deliver to); the
    // backend is selected before the services start.
    if let Err(e) = slint::invoke_from_event_loop(move || with_binding(f)) {
        snipdeck_app::trace!("event for the UI thread dropped: {e}");
    }
}
