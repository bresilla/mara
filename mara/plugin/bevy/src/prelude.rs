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
    BevyEmbeddedView, BevyViewportAppConfigure, BevyViewportBridge, BevyViewportInput,
    BevyViewportPickedColor, BevyViewportRenderTarget, BevyViewportSet, BevyViewportTexture,
    BevyViewportWgpuResources, CapturedBevyFrame, EguiInputAbsorbPlugin, MaraBevyViewport,
    MaraPlugin, MaraShellPlugin, RibbonGhostSet, RibbonPlugin, ThemePlugin,
    gizmo_material::GizmoMaterial,
    make_viewport_render_target,
    node_view_backend::{
        BevyNodeViewBackend, NodeViewCopy, NodeViewPlugin, NodeViewSlots, PendingNodeViewCopies,
    },
    spawn_viewport_camera,
    window_chrome::{
        MaraWindowChromeInputClaim, MaraWindowChromePlugin, MaraWindowChromeSet,
        MaraWindowChromeSettings,
    },
};
