//! UI-only Mara surface.
//!
//! This module intentionally has no native-window ownership. Use it
//! from Bevy, eframe, web, or any other host that already owns its
//! window/render loop and only wants Mara UI elements.
//!
//! ## Sealed by default
//!
//! Mara does not re-export `egui` unless the `raw-egui` feature is
//! enabled. App code builds UI exclusively through Mara's typed
//! surface (`MaraUi`, `MaraPainter`, pods, panes, shelves, views,
//! modules) plus the inert data types in [`mara_core::vocab`].
//! Enabling `raw-egui` is the explicit, greppable escape hatch.

#[cfg(feature = "raw-egui")]
pub use egui;

#[cfg(feature = "three-d")]
pub use mara_3d;
#[cfg(feature = "canvas")]
pub use mara_canvas;
#[cfg(feature = "code")]
pub use mara_code;
pub use mara_core;
pub use mara_core::*;
#[cfg(feature = "graph")]
pub use mara_graph;
#[cfg(feature = "image")]
pub use mara_image;
#[cfg(feature = "map")]
pub use mara_map;

pub use crate::host::{MaraHostCtx, MaraWindowHost};

pub mod modules {
    #[cfg(feature = "three-d")]
    pub use mara_3d as three_d;
    #[cfg(feature = "canvas")]
    pub use mara_canvas as canvas;
    #[cfg(feature = "code")]
    pub use mara_code as code;
    #[cfg(feature = "graph")]
    pub use mara_graph as graph;
    #[cfg(feature = "image")]
    pub use mara_image as image;
    #[cfg(feature = "map")]
    pub use mara_map as map;

    #[cfg(feature = "bevy")]
    pub use mara_bevy as bevy;
}

pub mod prelude {
    pub use crate::host::{MaraHostCtx, MaraWindowHost};
    #[cfg(feature = "raw-egui")]
    pub use egui;
    #[cfg(feature = "three-d")]
    pub use mara_3d;
    #[cfg(feature = "canvas")]
    pub use mara_canvas;
    #[cfg(feature = "code")]
    pub use mara_code;
    pub use mara_core::*;
    #[cfg(feature = "graph")]
    pub use mara_graph;
    #[cfg(feature = "image")]
    pub use mara_image;
    #[cfg(feature = "map")]
    pub use mara_map;

    #[cfg(feature = "bevy")]
    pub use mara_bevy;
}
