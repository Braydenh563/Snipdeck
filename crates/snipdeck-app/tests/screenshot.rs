//! The `--screenshot` renderer: measured pixels of the demo Panel, light and dark. One process
//! for this file (the headless platform is installed once per thread).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::cast_precision_loss,
    reason = "test code: a failure is a failed test"
)]

use snipdeck_app::screenshot::{SIZE, render_demo, write_png};

fn mean_luma(rgb: &[u8]) -> f64 {
    let sum: u64 = rgb.iter().map(|b| u64::from(*b)).sum();
    sum as f64 / rgb.len() as f64
}

#[test]
fn the_demo_panel_renders_and_writes_a_png() {
    let light = render_demo(false).expect("light");
    assert_eq!((light.width, light.height), SIZE);
    let first = light.pixel(0, 0);
    assert!(
        light.rgb.chunks(3).any(|p| p != first),
        "the Panel is not a flat colour"
    );
    assert!(
        mean_luma(&light.rgb) > 128.0,
        "a light Panel is mostly light"
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("panel.png");
    write_png(&path, &light).expect("png");
    let decoder = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(&path).expect("open"),
    ));
    let reader = decoder.read_info().expect("a valid png");
    assert_eq!((reader.info().width, reader.info().height), SIZE);
}

#[test]
fn the_dark_panel_is_darker_than_the_light_one() {
    let light = render_demo(false).expect("light");
    let dark = render_demo(true).expect("dark");
    assert!(
        mean_luma(&dark.rgb) < mean_luma(&light.rgb) - 40.0,
        "dark {} vs light {}",
        mean_luma(&dark.rgb),
        mean_luma(&light.rgb)
    );
}
