use egui::Id;

use super::{ModuleInlineCtx, ModuleResponse, WorkspaceCtx};
use crate::RibbonAvoidance;
use crate::mui::MaraUi;

/// A recursive Mara module.
///
/// Inline mode is rendered inside a pod. Workspace mode is rendered
/// when the module owns the active L1+ workspace level.
pub trait MaraModule {
    fn id(&self) -> Id;
    fn title(&self) -> &str;
    fn icon(&self) -> &'static str;

    /// Render the module's inline (in-pod) body. The surface is a
    /// sealed [`MaraUi`]; raw egui access requires the `raw-egui`
    /// feature (`mui.raw_ui_mut()`).
    fn inline(&mut self, mui: &mut MaraUi<'_>, ctx: ModuleInlineCtx<'_>) -> ModuleResponse;

    /// Which ribbons the module's fullscreen body should avoid.
    ///
    /// The fullscreen module background remains full-window; only
    /// the inner body UI is placed in the avoided rect.
    fn fullscreen_content_avoidance(&self) -> RibbonAvoidance {
        RibbonAvoidance::none()
    }

    fn workspace(&mut self, _ws: &mut WorkspaceCtx<'_>) {}
}
