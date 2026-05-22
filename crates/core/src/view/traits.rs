use crate::ribbon::{RibbonAvoidance, RibbonOverrideLayer, RibbonSlotDef};

use super::{SharedSurfaceId, ViewCtx, ViewId};

/// Top-level routable L0 screen/mode.
///
/// A view is selected by root/permanent chrome. It owns the L0
/// workspace for that screen. If something is also embeddable, it
/// can implement both this trait and [`crate::module::MaraModule`].
pub trait MaraView {
    fn id(&self) -> ViewId;
    fn title(&self) -> &str;
    fn icon(&self) -> &'static str;

    /// Optional hidden surface shared with other top-level views.
    ///
    /// The surface itself is not a selectable view. It is app-owned
    /// state (map/canvas/document/etc.) that several visible views
    /// can render and mutate while each visible view keeps its own
    /// tools, panes, bars, and workspace stack.
    fn shared_surface(&self) -> Option<SharedSurfaceId> {
        None
    }

    fn ribbons(&mut self) -> Vec<RibbonSlotDef> {
        Vec::new()
    }

    fn ribbon_overrides(&mut self) -> RibbonOverrideLayer {
        RibbonOverrideLayer::default()
    }

    /// Which ribbons the view's main content should avoid.
    ///
    /// This does not force the view background to shrink. Views that
    /// want an edge-to-edge backdrop can paint the full screen and
    /// lay out their body inside `ViewCtx::content_rect()`.
    fn content_avoidance(&self) -> RibbonAvoidance {
        RibbonAvoidance::none()
    }

    fn show(&mut self, ctx: &mut ViewCtx<'_>);
}
