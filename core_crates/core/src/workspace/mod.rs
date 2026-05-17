//! Recursive Mara workspace primitives.
//!
//! A workspace level is one full composition environment: bars,
//! panes, containers, pods, widgets, and modules. `L0` is the root
//! app workspace. Fullscreening a module pushes an `L1` module
//! workspace; modules inside it can push `L2`, and so on.

mod bar;
mod level;
mod policy;
mod stack;

pub(crate) use bar::validate_workspace_bar_item;
pub use bar::{
    WorkspaceBar, WorkspaceBarCluster, WorkspaceBarEdge, WorkspaceBarItem, WorkspaceBarItemKind,
};
pub use level::{WorkspaceLevelState, WorkspaceOwner};
pub use policy::WorkspacePolicy;
pub use stack::{WorkspaceStack, WorkspaceStackError};
