//! Glob-import for Bevy content embedded by Mara.
//!
//! ```ignore
//! use bevy::prelude::*;
//! use bevy_mara::prelude::*;
//! ```
//!
//! Mara owns egui. This prelude contains the Bevy viewport helpers and
//! scene utilities, but no Bevy-owned egui bridge.

pub use mara_core::*;

pub use crate::{
    BevyEmbeddedView, BevyViewportAppConfigure, BevyViewportBridge, BevyViewportInput,
    BevyViewportPickedColor, BevyViewportRenderTarget, BevyViewportSet, BevyViewportTexture,
    BevyViewportWgpuResources, CapturedBevyFrame, ChaseCamera, GroundGrid, GroundGridPlugin,
    MaraBevySceneHelpersPlugin, MaraBevyViewport, MaraPlugin, apply_rig,
    apply_viewport_camera_input_system, gizmo_material::GizmoMaterial, make_viewport_render_target,
    spawn_viewport_camera,
};
