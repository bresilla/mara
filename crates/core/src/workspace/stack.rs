use super::{WorkspaceLevelState, WorkspaceOwner, WorkspacePolicy};
use crate::vocab::Id as MaraId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceStackError {
    CannotPopRoot,
}

/// Active workspace path: `[L0]`, `[L0, L1(module)]`,
/// `[L0, L1(module), L2(module)]`, ...
#[derive(Clone, Debug)]
pub struct WorkspaceStack {
    levels: Vec<WorkspaceLevelState>,
}

impl Default for WorkspaceStack {
    fn default() -> Self {
        Self::new(MaraId::new("mara_root_workspace"))
    }
}

impl WorkspaceStack {
    #[must_use]
    pub fn new(root_id: impl Into<MaraId>) -> Self {
        Self {
            levels: vec![WorkspaceLevelState::root(root_id)],
        }
    }

    #[must_use]
    pub fn levels(&self) -> &[WorkspaceLevelState] {
        &self.levels
    }

    #[must_use]
    pub fn current(&self) -> WorkspaceLevelState {
        *self
            .levels
            .last()
            .expect("workspace stack always contains L0")
    }

    #[must_use]
    pub fn current_policy(&self) -> WorkspacePolicy {
        WorkspacePolicy::for_level(self.current())
    }

    #[must_use]
    pub fn depth(&self) -> u8 {
        self.current().depth
    }

    #[must_use]
    pub fn is_root_active(&self) -> bool {
        matches!(self.current().owner, WorkspaceOwner::Root)
    }

    pub fn push_module(&mut self, module_id: impl Into<MaraId>) -> WorkspaceLevelState {
        assert!(
            self.levels.len() <= u8::MAX as usize,
            "workspace stack depth cannot exceed L255"
        );
        let module_id = module_id.into();
        let depth = self.levels.len().min(u8::MAX as usize) as u8;
        let level_id = MaraId::new(("mara_module_workspace", module_id, depth));
        let level = WorkspaceLevelState::module(level_id, depth, module_id);
        self.levels.push(level);
        level
    }

    pub fn pop(&mut self) -> Result<WorkspaceLevelState, WorkspaceStackError> {
        if self.levels.len() <= 1 {
            return Err(WorkspaceStackError::CannotPopRoot);
        }
        Ok(self.levels.pop().expect("length checked above"))
    }

    pub fn pop_to_root(&mut self) {
        self.levels.truncate(1);
    }
}
