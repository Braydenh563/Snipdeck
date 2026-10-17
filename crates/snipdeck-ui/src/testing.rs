//! Headless rendering for tests: a software-rendered Slint platform with a frozen clock, plus
//! golden-image comparison.
//!
//! Goldens live in `tests/golden/<name>.png`. `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui` rewrites
//! them; a mismatch writes the actual image to `target/screenshot-diff/<name>.png`.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test harness: a failure here is a failed test"
)]

use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

use slint::{
    ComponentHandle, PhysicalSize, Rgb8Pixel, SharedPixelBuffer,
    platform::{
        Platform, PlatformError, WindowAdapter,
        software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
    },
};

thread_local! {
    static WINDOW: RefCell<Option<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(None) };
    static NOW: Cell<Duration> = const { Cell::new(Duration::ZERO) };
}

struct HeadlessPlatform;

impl Platform for HeadlessPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        WINDOW.with(|w| *w.borrow_mut() = Some(window.clone()));
        Ok(window)
    }

    fn duration_since_start(&self) -> Duration {
        NOW.with(Cell::get)
    }
}

fn init() {
    // Slint keeps its platform per thread, and the test harness runs one test per thread.
    thread_local! { static DONE: Cell<bool> = const { Cell::new(false) }; }
    if !DONE.with(|d| d.replace(true)) {
        slint::platform::set_platform(Box::new(HeadlessPlatform))
            .unwrap_or_else(|e| panic!("another Slint platform is already installed: {e}"));
    }
}

/// A rendered frame.
pub struct Frame {
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
    /// Row-major RGB, 3 bytes per pixel.
    pub rgb: Vec<u8>,
}

impl Frame {
    /// The colour at physical pixel `(x, y)`.
    ///
    /// # Panics
    /// If the point is outside the frame.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        assert!(
            x < self.width && y < self.height,
            "pixel ({x}, {y}) outside {}x{}",
            self.width,
            self.height
        );
        let i = ((y * self.width + x) * 3) as usize;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }
}

/// A component under test: owns the component and its window; the clock is frozen at zero and only
/// moves with [`Harness::advance`].
pub struct Harness<C: ComponentHandle> {
    /// The component under test.
    pub component: C,
    window: Rc<MinimalSoftwareWindow>,
}

impl<C: ComponentHandle> Harness<C> {
    /// Creates the component at `width` x `height` logical pixels (scale factor 1).
    ///
    /// # Panics
    /// If the component cannot be created.
    pub fn new(create: impl FnOnce() -> Result<C, PlatformError>, width: u32, height: u32) -> Self {
        init();
        NOW.with(|n| n.set(Duration::ZERO));
        let component = create().expect("creating the component");
        let window = WINDOW
            .with(|w| w.borrow().clone())
            .expect("platform created a window");
        window.set_size(PhysicalSize::new(width, height));
        component.show().expect("showing the window");
        Self { component, window }
    }

    /// Moves the frozen clock forward and runs timers and animations.
    pub fn advance(&self, by: Duration) {
        NOW.with(|n| n.set(n.get() + by));
        slint::platform::update_timers_and_animations();
    }

    /// Renders the current state.
    ///
    /// # Panics
    /// If the renderer produced no frame.
    #[must_use]
    pub fn render(&self) -> Frame {
        slint::platform::update_timers_and_animations();
        let size = self.window.size();
        let mut buffer = SharedPixelBuffer::<Rgb8Pixel>::new(size.width, size.height);
        let drawn = self.window.draw_if_needed(|renderer| {
            renderer.render(buffer.make_mut_slice(), size.width as usize);
        });
        assert!(drawn, "window had nothing to draw");
        let rgb = buffer
            .as_slice()
            .iter()
            .flat_map(|p| [p.r, p.g, p.b])
            .collect();
        Frame {
            width: size.width,
            height: size.height,
            rgb,
        }
    }
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

fn diff_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/screenshot-diff")
}

fn write_png(path: &Path, frame: &Frame) {
    std::fs::create_dir_all(path.parent().expect("has parent")).expect("creating directory");
    let file = std::fs::File::create(path).expect("creating png");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), frame.width, frame.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .expect("png header")
        .write_image_data(&frame.rgb)
        .expect("png data");
}

fn read_png(path: &Path) -> Option<Frame> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    Some(Frame {
        width: info.width,
        height: info.height,
        rgb: buf,
    })
}

/// Compares `frame` to `tests/golden/<name>.png`.
///
/// Per channel up to 2 levels of difference are tolerated on up to 0.1% of pixels (text
/// anti-aliasing across platforms).
///
/// # Panics
/// On a mismatch or a missing golden (run with `UPDATE_GOLDEN=1` to create it).
pub fn assert_golden(name: &str, frame: &Frame) {
    let path = golden_dir().join(format!("{name}.png"));
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        write_png(&path, frame);
        return;
    }
    let Some(golden) = read_png(&path) else {
        write_png(&diff_dir().join(format!("{name}.png")), frame);
        panic!(
            "missing golden {}; run with UPDATE_GOLDEN=1",
            path.display()
        );
    };
    let same_size = (golden.width, golden.height) == (frame.width, frame.height);
    let differing = if same_size {
        golden
            .rgb
            .chunks_exact(3)
            .zip(frame.rgb.chunks_exact(3))
            .filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 2))
            .count()
    } else {
        usize::MAX
    };
    let allowed = (frame.rgb.len() / 3) / 1000;
    if differing > allowed {
        write_png(&diff_dir().join(format!("{name}.png")), frame);
        panic!(
            "{name}: {differing} pixels differ (allowed {allowed}); actual written to {}",
            diff_dir().join(format!("{name}.png")).display()
        );
    }
}
