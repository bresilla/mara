use super::{WorkspaceLevelState, WorkspaceOwner};

/// Capability policy derived from the active workspace level.
///
/// L1+ workspaces should feel as capable as L0 for Mara composition
/// (panes, containers, pods, modules), but they must not expose
/// app/window-level controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkspacePolicy {
    pub allow_app_window_controls: bool,
    pub allow_module_workspace_push: bool,
    pub allow_root_ribbon: bool,
    pub allow_module_bars: bool,
    pub allow_shelves: bool,
    pub inherit_root_shelves: bool,
    pub restore_to_parent: bool,
}

impl WorkspacePolicy {
    #[must_use]
    pub const fn root() -> Self {
        Self {
            allow_app_window_controls: true,
            allow_module_workspace_push: true,
            allow_root_ribbon: true,
            allow_module_bars: false,
            allow_shelves: true,
            inherit_root_shelves: false,
            restore_to_parent: false,
        }
    }

    #[must_use]
    pub const fn module() -> Self {
        Self {
            allow_app_window_controls: false,
            allow_module_workspace_push: true,
            allow_root_ribbon: false,
            allow_module_bars: true,
            allow_shelves: true,
            inherit_root_shelves: true,
            restore_to_parent: true,
        }
    }

    #[must_use]
    pub fn for_level(level: WorkspaceLevelState) -> Self {
        match level.owner {
            WorkspaceOwner::Root => Self::root(),
            WorkspaceOwner::Module(_) => Self::module(),
        }
    }
}
