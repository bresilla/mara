//! Root Mara example application.
//!
//! This crate is intentionally outside `api_crates/*`: it consumes
//! `mara`, `mara_core`, and standalone Mara modules
//! the same way downstream applications should.

pub mod app;
#[cfg(not(target_arch = "wasm32"))]
pub mod bevy_scene;
pub mod bevy_view;

pub use app::DemoApp;
