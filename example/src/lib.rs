//! Root Mara example application.
//!
//! This crate consumes `mara`, `mara_core`, and standalone Mara modules
//! the same way downstream applications should.

pub mod app;
#[cfg(not(target_arch = "wasm32"))]
pub mod bevy_content;

pub use app::DemoApp;
