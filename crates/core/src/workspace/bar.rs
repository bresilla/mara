use crate::vocab::Id as MaraId;

/// Edge where a workspace-level bar is attached.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorkspaceBarEdge {
    Top,
    Bottom,
    Left,
    Right,
}

/// Cluster along the selected edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorkspaceBarCluster {
    Start,
    Middle,
    End,
}

/// Semantic bar item kind. Painting and layout stay theme-owned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorkspaceBarItemKind {
    Command,
    Toggle,
    Mode,
    Separator,
}

#[derive(Clone, Debug)]
pub struct WorkspaceBarItem {
    pub id: MaraId,
    pub label: String,
    pub icon: Option<&'static str>,
    pub kind: WorkspaceBarItemKind,
    pub active: bool,
}

impl WorkspaceBarItem {
    #[must_use]
    pub fn command(
        id: impl Into<MaraId>,
        label: impl Into<String>,
        icon: Option<&'static str>,
    ) -> Self {
        let label = label.into();
        assert!(
            !label.trim().is_empty(),
            "workspace bar command items require a non-empty label"
        );
        assert!(
            icon.is_some_and(|icon| !icon.trim().is_empty()),
            "workspace bar command items require a non-empty icon"
        );
        Self {
            id: id.into(),
            label,
            icon,
            kind: WorkspaceBarItemKind::Command,
            active: false,
        }
    }

    #[must_use]
    pub fn toggle(
        id: impl Into<MaraId>,
        label: impl Into<String>,
        icon: Option<&'static str>,
        active: bool,
    ) -> Self {
        Self::interactive(id, label, icon, WorkspaceBarItemKind::Toggle, active)
    }

    #[must_use]
    pub fn mode(
        id: impl Into<MaraId>,
        label: impl Into<String>,
        icon: Option<&'static str>,
        active: bool,
    ) -> Self {
        Self::interactive(id, label, icon, WorkspaceBarItemKind::Mode, active)
    }

    #[must_use]
    pub fn separator(id: impl Into<MaraId>) -> Self {
        Self {
            id: id.into(),
            label: String::new(),
            icon: None,
            kind: WorkspaceBarItemKind::Separator,
            active: false,
        }
    }

    fn interactive(
        id: impl Into<MaraId>,
        label: impl Into<String>,
        icon: Option<&'static str>,
        kind: WorkspaceBarItemKind,
        active: bool,
    ) -> Self {
        let item = Self {
            id: id.into(),
            label: label.into(),
            icon,
            kind,
            active,
        };
        validate_workspace_bar_item(&item);
        item
    }
}

#[derive(Clone, Debug)]
pub struct WorkspaceBar {
    pub id: MaraId,
    pub edge: WorkspaceBarEdge,
    pub cluster: WorkspaceBarCluster,
    pub items: Vec<WorkspaceBarItem>,
}

impl WorkspaceBar {
    #[must_use]
    pub fn new(
        id: impl Into<MaraId>,
        edge: WorkspaceBarEdge,
        cluster: WorkspaceBarCluster,
    ) -> Self {
        Self {
            id: id.into(),
            edge,
            cluster,
            items: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_item(mut self, item: WorkspaceBarItem) -> Self {
        validate_workspace_bar_item(&item);
        assert!(
            !self.items.iter().any(|existing| existing.id == item.id),
            "workspace bar items require unique ids within one bar"
        );
        self.items.push(item);
        self
    }
}

pub(crate) fn validate_workspace_bar_item(item: &WorkspaceBarItem) {
    match item.kind {
        WorkspaceBarItemKind::Command
        | WorkspaceBarItemKind::Toggle
        | WorkspaceBarItemKind::Mode => {
            assert!(
                !item.label.trim().is_empty(),
                "workspace bar interactive items require a non-empty label"
            );
            assert!(
                item.icon.is_some_and(|icon| !icon.trim().is_empty()),
                "workspace bar interactive items require a non-empty icon"
            );
        }
        WorkspaceBarItemKind::Separator => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_bar_commands_require_label_and_icon() {
        let missing_label = std::panic::catch_unwind(|| {
            let _ = WorkspaceBarItem::command("missing.label", " ", Some("add"));
        });
        let missing_icon = std::panic::catch_unwind(|| {
            let _ = WorkspaceBarItem::command("missing.icon", "Add", None);
        });
        let blank_icon = std::panic::catch_unwind(|| {
            let _ = WorkspaceBarItem::command("blank.icon", "Add", Some(" "));
        });
        let valid = WorkspaceBarItem::command("valid", "Add", Some("add"));

        assert!(missing_label.is_err());
        assert!(missing_icon.is_err());
        assert!(blank_icon.is_err());
        assert_eq!(valid.label, "Add");
    }

    #[test]
    fn workspace_bar_interactive_kinds_require_label_and_icon() {
        let missing_toggle_label = std::panic::catch_unwind(|| {
            let _ = WorkspaceBarItem::toggle("missing.toggle.label", " ", Some("toggle"), false);
        });
        let missing_mode_icon = std::panic::catch_unwind(|| {
            let _ = WorkspaceBarItem::mode("missing.mode.icon", "Paint", None, true);
        });
        let toggle = WorkspaceBarItem::toggle("toggle", "Snap", Some("magnet"), true);
        let mode = WorkspaceBarItem::mode("mode", "Paint", Some("brush"), true);
        let separator = WorkspaceBarItem::separator("separator");

        assert!(missing_toggle_label.is_err());
        assert!(missing_mode_icon.is_err());
        assert_eq!(toggle.kind, WorkspaceBarItemKind::Toggle);
        assert!(toggle.active);
        assert_eq!(mode.kind, WorkspaceBarItemKind::Mode);
        assert_eq!(separator.kind, WorkspaceBarItemKind::Separator);
    }

    #[test]
    fn workspace_bars_reject_direct_invalid_interactive_items_while_building() {
        let invalid_item = WorkspaceBarItem {
            id: "direct.invalid".into(),
            label: String::new(),
            icon: Some("info"),
            kind: WorkspaceBarItemKind::Toggle,
            active: false,
        };

        let rejected = std::panic::catch_unwind(|| {
            let _ = WorkspaceBar::new("bar", WorkspaceBarEdge::Top, WorkspaceBarCluster::Middle)
                .with_item(invalid_item);
        });

        assert!(rejected.is_err());
    }

    #[test]
    fn workspace_bars_reject_duplicate_item_ids_while_building() {
        let item_id = crate::vocab::Id::new("duplicate.item");
        let duplicate = std::panic::catch_unwind(|| {
            let _ = WorkspaceBar::new("bar", WorkspaceBarEdge::Top, WorkspaceBarCluster::Middle)
                .with_item(WorkspaceBarItem::command(item_id, "First", Some("add")))
                .with_item(WorkspaceBarItem::command(
                    item_id,
                    "Second",
                    Some("dismiss"),
                ));
        });

        assert!(duplicate.is_err());
    }
}
