use egui::Color32;

use crate::ribbon::{RibbonAvoidance, ribbon_avoiding_rect};
use crate::workspace::WorkspaceStack;

/// Rendering context for the active L0 view.
///
/// This starts deliberately small. Typed helpers for panes,
/// full-canvas surfaces, and view-local ribbons will layer on top
/// after the router and ribbon slot model are stable.
pub struct ViewCtx<'a> {
    pub egui_ctx: &'a egui::Context,
    pub workspace: &'a mut WorkspaceStack,
    pub accent: Color32,
    pub content_avoidance: RibbonAvoidance,
}

impl ViewCtx<'_> {
    #[must_use]
    pub fn content_rect(&self) -> egui::Rect {
        ribbon_avoiding_rect(self.egui_ctx, self.content_avoidance)
    }

    #[must_use]
    pub fn ribbon_avoiding_rect(&self, avoidance: RibbonAvoidance) -> egui::Rect {
        ribbon_avoiding_rect(self.egui_ctx, avoidance)
    }

    /// Current responsive size class for this frame. Views consult
    /// this to reflow on small screens (collapse chrome, stack panes).
    #[must_use]
    pub fn breakpoint(&self) -> crate::style::Breakpoint {
        crate::style::screen_class()
    }

    /// Convenience: phone-class (the most aggressive reflow tier).
    #[must_use]
    pub fn is_compact(&self) -> bool {
        self.breakpoint().is_compact()
    }

    /// Convenience: phone or tablet (not the full desktop shell).
    #[must_use]
    pub fn is_handheld(&self) -> bool {
        self.breakpoint().is_handheld()
    }
}
