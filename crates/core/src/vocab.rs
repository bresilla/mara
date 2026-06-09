//! # Vocabulary types — the only egui items Mara re-exports by default
//!
//! Mara's public API speaks egui's plain *data* vocabulary (colors,
//! points, rects, ids) without handing out egui *capabilities*
//! (`Ui`, `Context`, `Painter`, `Response`). Everything here is
//! inert: holding a [`Color32`] or a [`Rect`] gives a consumer no
//! way to paint raw egui widgets into a Mara surface.
//!
//! The full `egui` crate re-export only exists behind the
//! `raw-egui` feature — see the crate-level docs.

pub use egui::{
    Align, Align2, Color32, CornerRadius, Id, Margin, Pos2, Rangef, Rect, Sense, Stroke,
    StrokeKind, TextureId, Vec2, Vec2b, lerp, pos2, remap, remap_clamp, vec2,
};

// Texture data plumbing: pure pixel data plus the retained handle
// egui returns for it. A `TextureHandle` can update or free its own
// pixels but cannot reach the widget tree, so it stays vocabulary.
pub use egui::{ColorImage, TextureFilter, TextureHandle, TextureOptions, TextureWrapMode};
