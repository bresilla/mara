//! Typed module integration primitives.
//!
//! Modules are heavier work surfaces than ordinary widgets. They
//! render a compact inline representation inside a pod and can enter
//! a full workspace level through [`crate::workspace::WorkspaceStack`].

mod context;
mod traits;

pub use context::{ModuleInlineCtx, ModuleInlineOptions, ModuleResponse, WorkspaceCtx};
pub use traits::MaraModule;
