//! Snipdeck application logic: the use cases and view-models that sit between the UI, the
//! store and the platform layer, and the controller that binds them to the Panel window.
//! Everything here runs under test with the platform fakes: the logic in plain Rust, the window
//! under Slint's headless testing backend.
#![forbid(unsafe_code)]

pub mod binding;
pub mod capture;
pub mod cli;
pub mod command_binding;
pub mod commands;
pub mod controller;
pub mod crash;
pub mod demo;
pub mod dock;
pub mod doctor;
pub mod drafts;
pub mod editor_binding;
pub mod fill;
pub mod fill_binding;
pub mod frame;
pub mod headless;
pub mod help_binding;
pub mod host;
pub mod item_editor;
pub mod key_provider;
pub mod licence_store;
pub mod list_text;
pub mod main_binding;
pub mod main_window;
pub mod maintenance;
pub mod native_window;
pub mod onboarding_binding;
pub mod palette;
pub mod panel;
pub mod paste_next;
pub mod paths;
pub mod picker;
pub mod picker_binding;
pub mod placement;
pub mod platform;
pub(crate) mod popup_window;
pub mod screenshot;
pub mod settings;
pub mod settings_binding;
pub mod settings_store;
pub mod starter;
pub mod toast;
pub mod toast_binding;
pub mod tour;
pub mod trace;
pub mod usecase;
pub mod window_kind;
pub mod wording;
