//! Top-level view routing.
//!
//! A [`MaraView`] owns one L0 workspace. Root/permanent ribbon
//! actions can switch the active view without confusing that with
//! module fullscreen, which pushes L1+ on the active view's
//! [`WorkspaceStack`](crate::workspace::WorkspaceStack).
//!
//! Multiple top-level views can opt into the same hidden
//! [`SharedSurfaceId`]. That is the "Coreviz" shape: Zones, Graph,
//! and Management are real persistent-bar views, but they render and
//! mutate one shared map/canvas/document surface.

mod context;
mod id;
mod layout;
mod multi;
mod router;
mod traits;

pub use context::ViewCtx;
pub use id::{SharedSurfaceId, ViewId};
pub use layout::{CellId, Layout, SplitAxis};
pub use multi::MultiView;
pub use router::{ViewEntry, ViewRouter, ViewRouterError};
pub use traits::MaraView;
