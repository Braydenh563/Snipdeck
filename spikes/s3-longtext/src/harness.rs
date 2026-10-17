//! Headless Slint platform: software renderer into an in-memory buffer, plus timing/RSS helpers.

use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Platform, WindowAdapter};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static IN_FRAME: AtomicBool = AtomicBool::new(false);
/// Number of frames whose rendering panicked (caught in `Harness::frame`).
pub static RENDER_PANICS: AtomicUsize = AtomicUsize::new(0);

struct BenchPlatform {
    window: Rc<MinimalSoftwareWindow>,
    start: Instant,
}

impl Platform for BenchPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }
    fn duration_since_start(&self) -> Duration {
        self.start.elapsed()
    }
}

pub struct Harness {
    pub window: Rc<MinimalSoftwareWindow>,
    buf: RefCell<Vec<PremultipliedRgbaColor>>,
    pub w: u32,
    pub h: u32,
}

impl Harness {
    /// Installs the platform. Must be called once per process, before any component is created.
    /// `NewBuffer` = every frame repaints the whole window (closest to what a GPU renderer does).
    pub fn init(w: u32, h: u32) -> Rc<Harness> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        slint::platform::set_platform(Box::new(BenchPlatform {
            window: window.clone(),
            start: Instant::now(),
        }))
        .expect("platform already set");
        Rc::new(Harness {
            window,
            buf: RefCell::new(vec![PremultipliedRgbaColor::default(); (w * h) as usize]),
            w,
            h,
        })
    }

    /// Call after `component.show()`.
    pub fn size_window(&self) {
        self.window
            .set_size(slint::PhysicalSize::new(self.w, self.h));
    }

    /// Renders one frame if anything is dirty. Returns the time spent in the renderer
    /// (this includes any lazy property/layout evaluation the frame triggers).
    ///
    /// A panic inside the renderer is caught and counted (see `RENDER_PANICS`); every metric
    /// reported after it is suffixed with `!` (not valid).
    pub fn frame(&self) -> Duration {
        slint::platform::update_timers_and_animations();
        let t = Instant::now();
        IN_FRAME.store(true, Ordering::SeqCst);
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.window.draw_if_needed(|r| {
                let mut buf = self.buf.borrow_mut();
                r.render(&mut buf, self.w as usize);
            });
        }));
        IN_FRAME.store(false, Ordering::SeqCst);
        if res.is_err() {
            RENDER_PANICS.fetch_add(1, Ordering::SeqCst);
        }
        t.elapsed()
    }

    pub fn force_frame(&self) -> Duration {
        self.window.request_redraw();
        self.frame()
    }

    pub fn dispatch(&self, ev: slint::platform::WindowEvent) {
        use slint::platform::WindowAdapter as _;
        self.window.window().dispatch_event(ev);
    }

    pub fn key(&self, text: &str) {
        use slint::platform::WindowEvent;
        self.dispatch(WindowEvent::KeyPressed { text: text.into() });
        self.dispatch(WindowEvent::KeyReleased { text: text.into() });
    }

    pub fn key_press(&self, text: &str) {
        self.dispatch(slint::platform::WindowEvent::KeyPressed { text: text.into() });
    }

    pub fn key_release(&self, text: &str) {
        self.dispatch(slint::platform::WindowEvent::KeyReleased { text: text.into() });
    }

    pub fn click(&self, x: f32, y: f32) {
        use slint::platform::{PointerEventButton, WindowEvent};
        use slint::LogicalPosition;
        let position = LogicalPosition::new(x, y);
        self.dispatch(WindowEvent::PointerMoved { position });
        self.dispatch(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        self.dispatch(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
    }
}

/// (VmRSS, VmHWM) in MiB from /proc/self/status.
pub fn rss_mb() -> (f64, f64) {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let get = |k: &str| -> f64 {
        s.lines()
            .find(|l| l.starts_with(k))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<f64>().ok())
            .map(|kb| kb / 1024.0)
            .unwrap_or(f64::NAN)
    };
    (get("VmRSS:"), get("VmHWM:"))
}

pub fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

pub fn time<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let r = f();
    (r, t.elapsed())
}

/// Summary of a set of samples in milliseconds.
pub struct Stats {
    pub median: f64,
    pub p95: f64,
    pub max: f64,
}

pub fn stats(samples: &[Duration]) -> Stats {
    let mut v: Vec<f64> = samples.iter().map(|d| ms(*d)).collect();
    if v.is_empty() {
        return Stats {
            median: f64::NAN,
            p95: f64::NAN,
            max: f64::NAN,
        };
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let idx = |q: f64| ((v.len() - 1) as f64 * q).round() as usize;
    Stats {
        median: v[idx(0.5)],
        p95: v[idx(0.95)],
        max: v[v.len() - 1],
    }
}

/// Result line emitted by a child scenario process: `RESULT\t<group>\t<label>\tk=v\tk=v...`
///
/// Fields live in a process-global so that a panic hook can still emit what was measured
/// before the crash (Slint's software renderer panics on some inputs, see the report).
pub struct Report;

struct ReportData {
    group: &'static str,
    label: String,
    fields: Vec<(String, String)>,
}

static CURRENT: std::sync::Mutex<Option<ReportData>> = std::sync::Mutex::new(None);

fn push_field(k: &str, mut v: String) {
    if let Some(d) = CURRENT.lock().unwrap().as_mut() {
        // Sticky: once the renderer has panicked, later frames are not trustworthy
        // (draw_if_needed stops drawing), so every later value is flagged.
        let p = RENDER_PANICS.load(Ordering::SeqCst);
        if p > 0 && k != "render_panics" {
            v.push('!');
        }
        d.fields.push((k.to_string(), v));
    }
}

fn emit_current(extra: Option<(&str, String)>) {
    if let Some(d) = CURRENT.lock().unwrap().take() {
        let mut line = format!("RESULT\t{}\t{}", d.group, d.label);
        for (k, v) in d
            .fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .chain(extra)
        {
            line.push('\t');
            line.push_str(k);
            line.push('=');
            line.push_str(&v);
        }
        println!("{line}");
    }
}

/// On panic: emit the fields measured so far plus `panic_after=<last field>`.
pub fn install_panic_report() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if IN_FRAME.load(Ordering::SeqCst) {
            if RENDER_PANICS.load(Ordering::SeqCst) == 0 {
                eprintln!("    (render panic, caught; further ones silenced): {info}");
            }
            return;
        }
        let last = CURRENT
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|d| d.fields.last().map(|f| f.0.clone())))
            .flatten()
            .unwrap_or_else(|| "start".into());
        let msg = info
            .location()
            .map(|l| format!("{}:{}", l.file().rsplit('/').next().unwrap_or(""), l.line()))
            .unwrap_or_default();
        emit_current(Some(("panic_after", format!("{last} ({msg})"))));
        prev(info);
    }));
}

impl Report {
    pub fn new(group: &'static str, label: impl Into<String>) -> Self {
        *CURRENT.lock().unwrap() = Some(ReportData {
            group,
            label: label.into(),
            fields: Vec::new(),
        });
        Report
    }
    pub fn f(&mut self, k: &str, v: f64) {
        let s = if v.is_nan() {
            "-".to_string()
        } else if v >= 100.0 {
            format!("{v:.0}")
        } else if v >= 10.0 {
            format!("{v:.1}")
        } else {
            format!("{v:.2}")
        };
        push_field(k, s);
    }
    pub fn d(&mut self, k: &str, d: Duration) {
        self.f(k, ms(d));
    }
    pub fn emit(&self) {
        let p = RENDER_PANICS.load(Ordering::SeqCst);
        if p > 0 {
            push_field("render_panics", p.to_string());
        }
        emit_current(None);
    }
}
