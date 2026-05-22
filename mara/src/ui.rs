//! UI-only Mara surface.
//!
//! This module intentionally has no native-window ownership. Use it
//! from Bevy, eframe, web, or any other host that already owns its
//! window/render loop and only wants Mara UI elements.

pub use egui;

pub use mara_canvas;
pub use mara_code;
pub use mara_core;
pub use mara_core::*;
pub use mara_graph;
pub use mara_image;
pub use mara_map;

pub use crate::host::{MaraHostCtx, MaraWindowHost};

pub mod modules {
    pub use mara_canvas as canvas;
    pub use mara_code as code;
    pub use mara_graph as graph;
    pub use mara_image as image;
    pub use mara_map as map;

    #[cfg(feature = "bevy")]
    pub use mara_bevy as bevy;
}

pub mod prelude {
    pub use crate::host::{MaraHostCtx, MaraWindowHost};
    pub use egui;
    pub use mara_canvas;
    pub use mara_code;
    pub use mara_core::*;
    pub use mara_graph;
    pub use mara_image;
    pub use mara_map;

    #[cfg(feature = "bevy")]
    pub use mara_bevy;
}
