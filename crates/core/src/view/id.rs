use std::hash::Hash;

use crate::vocab::Id;

/// Stable identifier for a top-level Mara view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ViewId(pub Id);

impl ViewId {
    #[must_use]
    pub fn new(source: impl Hash) -> Self {
        Self(Id::new(source))
    }
}

impl From<egui::Id> for ViewId {
    fn from(value: egui::Id) -> Self {
        Self(value.into())
    }
}

impl From<ViewId> for egui::Id {
    fn from(value: ViewId) -> Self {
        value.0.into()
    }
}

/// Stable identifier for a hidden shared surface/state root.
///
/// A shared surface is deliberately not routable and never appears
/// in the persistent bar. Top-level [`ViewId`] entries can point at
/// the same surface so they behave like independent views while
/// drawing/editing the same underlying canvas/map/document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SharedSurfaceId(pub Id);

impl SharedSurfaceId {
    #[must_use]
    pub fn new(source: impl Hash) -> Self {
        Self(Id::new(source))
    }
}

impl From<egui::Id> for SharedSurfaceId {
    fn from(value: egui::Id) -> Self {
        Self(value.into())
    }
}

impl From<SharedSurfaceId> for egui::Id {
    fn from(value: SharedSurfaceId) -> Self {
        value.0.into()
    }
}
