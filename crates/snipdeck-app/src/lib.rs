//! Snipdeck application logic: the use cases and view-models that sit between the UI, the
//! store and the platform layer, and the controller that binds them to the Panel window.
//! Everything here runs under test with the platform fakes: the logic in plain Rust, the window
//! under Slint's headless testing backend.
#![forbid(unsafe_code)]

pub mod binding;
pub mod capture;
pub mod cli;
pub mod controller;
pub mod crash;
pub mod demo;
pub mod dock;
pub mod fill;
pub mod frame;
pub mod host;
pub mod licence_store;
pub mod main_binding;
pub mod main_window;
pub mod maintenance;
pub mod native_window;
pub mod palette;
pub mod panel;
pub mod paste_next;
pub mod paths;
pub mod picker;
pub mod platform;
pub mod screenshot;
pub mod settings;
pub mod settings_binding;
pub mod settings_store;
pub mod starter;
pub mod toast;
pub mod trace;
pub mod usecase;
pub mod wording;
