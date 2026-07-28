//! `Tab` — one labelled body inside a [`super::normal::Normal::show_tabs`]
//! tabbed container. Each tab carries its own title, icon, and pod
//! list; the tab strip renders one button per tab projecting from
//! the title-facing edge of the container, folder-style — the active
//! tab merges into the container body (same fill, no seam) while
//! inactive tabs are outlined empty boxes the parent pane bg shows
//! through.
//!
//! ```ignore
//! Normal::new("Transform", anchor, accent, cid).show_tabs(ui, vec![
//!     Tab::new("Position", "arrow-move").pods(vec![pod_x, pod_y, pod_z]),
//!     Tab::new("Rotation", "arrow-rotate-clockwise").pods(vec![pod_rx]),
//!     Tab::new("Scale",    "maximize").pods(vec![pod_sx, pod_sy, pod_sz]),
//! ]);
//! ```

use crate::pod::Pod;
use crate::{icons::Icon, vocab::Id as MaraId};

pub struct Tab {
    /// Stable id used by the per-pane tab-drag routing — `(tab_id →
    /// owner container)` and per-container tab order are persisted
    /// under this id. Pass a value that survives renames (e.g.
    /// `"position"`, not the user-visible title).
    pub(crate) id: MaraId,
    pub(crate) title: String,
    pub(crate) icon: Icon<'static>,
    pub(crate) pods: Vec<Pod>,
}

impl Tab {
    pub fn new(
        id: impl Into<MaraId>,
        title: impl Into<String>,
        icon: impl Into<Icon<'static>>,
    ) -> Self {
        let title = title.into();
        assert!(
            !title.trim().is_empty(),
            "tab containers require every tab to have a non-empty title"
        );
        let icon = icon.into();
        assert!(
            tab_icon_is_present(icon),
            "tab containers require every tab to have a non-empty icon"
        );
        Self {
            id: id.into(),
            title,
            icon,
            pods: Vec::new(),
        }
    }

    pub fn pods(mut self, pods: impl IntoIterator<Item = Pod>) -> Self {
        self.pods = pods.into_iter().collect();
        self
    }

    /// The stable id passed to [`Tab::new`].
    #[must_use]
    pub fn id(&self) -> MaraId {
        self.id
    }
}

fn tab_icon_is_present(icon: Icon<'_>) -> bool {
    match icon {
        Icon::Name(name) | Icon::Svg(name) => !name.trim().is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_new_requires_non_empty_icon() {
        let result = std::panic::catch_unwind(|| {
            let _ = Tab::new("tab-without-icon", "Broken", "");
        });

        assert!(result.is_err());
    }

    #[test]
    fn tab_new_requires_non_empty_title() {
        let result = std::panic::catch_unwind(|| {
            let _ = Tab::new("tab-without-title", " ", "settings");
        });

        assert!(result.is_err());
    }

    #[test]
    fn tab_new_accepts_named_icon() {
        let tab = Tab::new("tab-with-icon", "Valid", "settings");

        assert_eq!(tab.title, "Valid");
    }

    #[test]
    fn tab_public_id_uses_mara_vocab() {
        let tab = Tab::new("tab-id", "Valid", "settings");
        let id: MaraId = tab.id();

        assert_eq!(id, tab.id());
    }

    #[test]
    fn tab_new_preserves_existing_mara_id_without_rehashing() {
        let id = MaraId::new("stable-tab-id");
        let tab = Tab::new(id, "Valid", "settings");

        assert_eq!(tab.id(), id);
    }
}
