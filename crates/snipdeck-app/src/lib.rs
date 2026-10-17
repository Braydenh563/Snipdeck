//! Snipdeck application logic: the use cases and view-models that sit between the UI, the
//! store and the platform layer. Nothing here draws or touches Windows; everything runs under
//! test with the platform fakes.

pub mod panel;
pub mod toast;
pub mod usecase;
