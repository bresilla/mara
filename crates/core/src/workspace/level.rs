use crate::vocab::Id as MaraId;

/// Owner of one workspace level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorkspaceOwner {
    /// Root application workspace (`L0`).
    Root,
    /// Module-owned workspace (`L1+`).
    Module(MaraId),
}

/// Runtime identity for one level in the workspace stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorkspaceLevelState {
    pub id: MaraId,
    pub depth: u8,
    pub owner: WorkspaceOwner,
}

impl WorkspaceLevelState {
    #[must_use]
    pub fn root(id: impl Into<MaraId>) -> Self {
        Self {
            id: id.into(),
            depth: 0,
            owner: WorkspaceOwner::Root,
        }
    }

    #[must_use]
    pub fn module(id: impl Into<MaraId>, depth: u8, module_id: impl Into<MaraId>) -> Self {
        assert!(depth > 0, "module workspace levels must be L1+");
        Self {
            id: id.into(),
            depth,
            owner: WorkspaceOwner::Module(module_id.into()),
        }
    }

    #[must_use]
    pub fn is_root(self) -> bool {
        matches!(self.owner, WorkspaceOwner::Root)
    }

    #[must_use]
    pub fn is_module(self) -> bool {
        matches!(self.owner, WorkspaceOwner::Module(_))
    }
}
