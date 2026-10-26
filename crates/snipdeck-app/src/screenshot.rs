//! `--screenshot`: renders the demo Panel with Slint's software renderer, no display needed.
//! It reuses the headless harness of `snipdeck-ui` (a frozen clock, scale factor 1), so the
//! image is the same one the UI crate's golden tests would see.

use std::{cell::RefCell, path::Path, rc::Rc};

use snipdeck_core::Timestamp;
use snipdeck_ui::{
    PanelWindow,
    testing::{Frame, Harness},
};

use crate::{
    controller::Controller,
    demo,
    settings::{Settings, Theme},
};

/// The Panel's default size in logical pixels.
pub const SIZE: (u32, u32) = (320, 480);

/// Renders the demo Panel. Slint's headless platform is per thread and can be installed once,
/// so call this at most once per process.
///
/// # Errors
///
/// A message when the demo library cannot be built.
pub fn render_demo(dark: bool) -> Result<Frame, String> {
    let now: Timestamp = "2026-10-17T10:30:00Z"
        .parse()
        .map_err(|e| format!("timestamp: {e}"))?;
    let settings = Settings {
        theme: if dark { Theme::Dark } else { Theme::Light },
        ..Settings::new_install()
    };
    let controller = demo::controller(settings, now).map_err(|e| format!("demo library: {e}"))?;
    let controller = Rc::new(RefCell::new(controller));
    let harness = Harness::new(PanelWindow::new, SIZE.0, SIZE.1);
    let _binding = Controller::bind(
        &controller,
        &harness.component,
        std::rc::Rc::new(crate::placement::NoPopups),
        |_| {},
    );
    Ok(harness.render_settled())
}

/// Writes `frame` as an 8-bit RGB PNG.
///
/// # Errors
///
/// A message naming the file when it cannot be written.
pub fn write_png(path: &Path, frame: &Frame) -> Result<(), String> {
    let fail = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let file = std::fs::File::create(path).map_err(|e| fail(&e))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), frame.width, frame.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(&frame.rgb))
        .map_err(|e| fail(&e))
}
