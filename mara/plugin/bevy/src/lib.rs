//! # bevy_mara — Bevy helpers for Mara-owned applications.
//!
//! Mara owns egui and the application shell. This crate deliberately
//! does **not** depend on a Bevy-owned egui bridge: Bevy is content that Mara can
//! embed as a viewport, not the owner of Mara's UI context.
//!
//! The remaining Bevy-specific surface is intentionally small:
//!
//! * re-exports from [`mara_bevy`] for the egui-owned embedded Bevy
//!   viewport;
//! * [`MaraPlugin`], a lightweight helper plugin for Bevy scene
//!   utilities used by embedded content;
//! * [`GizmoMaterial`], the always-on-top Bevy material wrapper.

pub mod gizmo_material;
pub mod prelude;

pub use mara_bevy::{
    BevyEmbeddedView, BevyViewportAppConfigure, BevyViewportBridge, BevyViewportInput,
    BevyViewportPickedColor, BevyViewportRenderTarget, BevyViewportSet, BevyViewportTexture,
    BevyViewportWgpuResources, CapturedBevyFrame, ChaseCamera, GroundGrid, GroundGridPlugin,
    MaraBevySceneHelpersPlugin, MaraBevyViewport, apply_rig, apply_viewport_camera_input_system,
    make_viewport_render_target, spawn_viewport_camera,
};

// Re-export `mara_core` for consumers that were importing Mara types
// from this adapter. The canonical app-facing path remains `mara::ui`.
pub use mara_core::*;

use bevy::prelude::*;

/// Lightweight Bevy-side helper install.
///
/// This does not render Mara UI and does not create/manage an egui
/// context. Add it only to Bevy apps used as Mara viewport content.
/// Mara-owned hosts should usually go through
/// [`MaraBevyViewport`](mara_bevy::MaraBevyViewport) instead.
pub struct MaraPlugin;

impl Plugin for MaraPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<MaraBevySceneHelpersPlugin>() {
            app.add_plugins(MaraBevySceneHelpersPlugin);
        }
    }
}
