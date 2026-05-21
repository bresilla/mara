//! Glob-import for apps building on top of `bevy_mara`.
//!
//! ```ignore
//! use bevy::prelude::*;
//! use bevy_mara::prelude::*;
//! ```
//!
//! Brings in everything `mara_core` exports (panes, ribbons,
//! containers, pods, widgets, theme primitives) plus the
//! Bevy-specific additions from this crate — `MaraPlugin`,
//! `ThemePlugin`, `RibbonPlugin`, `RibbonGhostSet`, and
//! `GizmoMaterial`.

pub use mara_core::*;

pub use crate::{
    EguiInputAbsorbPlugin, MaraPlugin, RibbonGhostSet, RibbonPlugin, ThemePlugin,
    embedded_view::{
        BevyEmbeddedView, BevyViewportBridge, BevyViewportInput, BevyViewportTexture,
        BevyViewportWgpuResources, CapturedBevyFrame, make_viewport_render_target,
        spawn_viewport_camera,
    },
    gizmo_material::GizmoMaterial,
    node_view_backend::{
        BevyNodeViewBackend, NodeViewCopy, NodeViewPlugin, NodeViewSlots, PendingNodeViewCopies,
    },
    window_chrome::{
        MaraWindowChromeInputClaim, MaraWindowChromePlugin, MaraWindowChromeSet,
        MaraWindowChromeSettings,
    },
};
