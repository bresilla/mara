//! Top-level view routing.
//!
//! A [`MaraView`] owns one L0 workspace. Root/permanent ribbon
//! actions can switch the active view without confusing that with
//! module fullscreen, which pushes L1+ on the active view's
//! [`WorkspaceStack`](crate::workspace::WorkspaceStack).

mod context;
mod id;
mod router;
mod traits;

pub use context::ViewCtx;
pub use id::ViewId;
pub use router::{ViewEntry, ViewRouter, ViewRouterError};
pub use traits::MaraView;
