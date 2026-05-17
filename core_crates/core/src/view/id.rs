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
