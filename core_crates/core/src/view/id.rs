use std::hash::Hash;

/// Stable identifier for a top-level Mara view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ViewId(pub egui::Id);

impl ViewId {
    #[must_use]
    pub fn new(source: impl Hash) -> Self {
        Self(egui::Id::new(source))
    }
}

impl From<egui::Id> for ViewId {
    fn from(value: egui::Id) -> Self {
        Self(value)
    }
}

/// Stable identifier for a hidden shared surface/state root.
///
/// A shared surface is deliberately not routable and never appears
/// in the persistent bar. Top-level [`ViewId`] entries can point at
/// the same surface so they behave like independent views while
/// drawing/editing the same underlying canvas/map/document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SharedSurfaceId(pub egui::Id);

impl SharedSurfaceId {
    #[must_use]
    pub fn new(source: impl Hash) -> Self {
        Self(egui::Id::new(source))
    }
}

impl From<egui::Id> for SharedSurfaceId {
    fn from(value: egui::Id) -> Self {
        Self(value)
    }
}
