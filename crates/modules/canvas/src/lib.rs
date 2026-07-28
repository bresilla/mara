//! `mara_canvas` — whiteboard/canvas proof module for Mara.
//!
//! The crate is intentionally small and host-agnostic. It proves the
//! PLAN.md model where a document-like surface can be both a top-level
//! [`mara_core::MaraView`] and an embeddable [`mara_core::MaraModule`]
//! that can enter L1/L2 module workspaces.

use mara_core::{
    MaraModule, MaraView, ModuleInlineCtx, ModuleResponse, RibbonAction, RibbonCluster, RibbonEdge,
    RibbonOverrideLayer, RibbonOverridePolicy, RibbonScope, RibbonSlot, RibbonSlotDef,
    RibbonSlotId, RibbonSlotItem, ViewCtx, ViewId, WorkspaceBar, WorkspaceBarCluster,
    WorkspaceBarEdge, WorkspaceBarItem, WorkspaceCtx,
    vocab::{
        Align2 as MaraAlign2, Color32 as MaraColor32, Pos2 as MaraPos2, Stroke as MaraStroke,
        Vec2 as MaraVec2,
    },
};

/// A retained freehand stroke in logical canvas points.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasStroke {
    pub points: Vec<MaraPos2>,
    pub color: MaraColor32,
    pub width: f32,
}

impl CanvasStroke {
    #[must_use]
    pub fn new(color: impl Into<MaraColor32>, width: f32) -> Self {
        Self {
            points: Vec::new(),
            color: color.into(),
            width,
        }
    }
}

/// Whiteboard document state.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasDocument {
    pub title: String,
    pub strokes: Vec<CanvasStroke>,
}

impl CanvasDocument {
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            strokes: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.strokes.clear();
    }
}

/// Simple whiteboard surface that can be routed as a View or embedded as a Module.
#[derive(Clone, Debug)]
pub struct CanvasSurface {
    id: mara_core::vocab::Id,
    doc: CanvasDocument,
    pen_width: f32,
}

impl CanvasSurface {
    #[must_use]
    pub fn new(id: impl std::hash::Hash, doc: CanvasDocument) -> Self {
        Self {
            id: mara_core::vocab::Id::new(id),
            doc,
            pen_width: 2.0,
        }
    }

    #[must_use]
    pub fn document(&self) -> &CanvasDocument {
        &self.doc
    }

    #[must_use]
    pub fn document_mut(&mut self) -> &mut CanvasDocument {
        &mut self.doc
    }

    fn paint_canvas(&mut self, mui: &mut mara_core::MaraUi<'_>, min_size: impl Into<MaraVec2>) {
        self.paint_canvas_surface(mui, min_size, true);
    }

    /// The stroke surface + interaction. `framed` draws the surface's own
    /// fill + border (inline/pane embeds); a `ViewNode` leaf passes
    /// `false` because the view already painted its frame over the whole
    /// region and the strokes draw directly on that surface.
    fn paint_canvas_surface(
        &mut self,
        mui: &mut mara_core::MaraUi<'_>,
        min_size: impl Into<MaraVec2>,
        framed: bool,
    ) {
        let min_size = min_size.into();
        let desired = MaraVec2::new(
            mui.available_width().max(min_size.x),
            mui.available_height().max(min_size.y),
        );
        let (painter, response) = mui.canvas(desired);
        let rect = response.rect;
        let accent = mara_core::style::active_accent();
        let radius = mara_core::style::radius_for(mara_core::style::RadiusRole::Section);

        if framed {
            painter.rect_filled(
                rect,
                radius,
                mara_core::style::fill_for(mara_core::style::FillRole::Pane, accent),
            );
            painter.rect_stroke(
                rect,
                radius,
                mara_core::style::stroke_for(mara_core::style::StrokeRole::WidgetBorder, accent),
            );
        }

        if response.drag_started() {
            let mut stroke = CanvasStroke::new(accent, self.pen_width);
            if let Some(pos) = response.interact_pointer
                && rect.contains(pos)
            {
                stroke.points.push(pos);
            }
            self.doc.strokes.push(stroke);
        } else if response.dragged()
            && let Some(pos) = response.interact_pointer
            && rect.contains(pos)
            && let Some(stroke) = self.doc.strokes.last_mut()
        {
            let should_push = stroke
                .points
                .last()
                .is_none_or(|last| last.distance(pos) >= 1.5);
            if should_push {
                stroke.points.push(pos);
            }
        }

        for stroke in &self.doc.strokes {
            for segment in stroke.points.windows(2) {
                painter.line_segment(
                    segment[0],
                    segment[1],
                    MaraStroke::new(stroke.width, stroke.color),
                );
            }
            if let Some(point) = stroke.points.first() {
                painter.circle_filled(*point, (stroke.width * 0.5).max(1.0), stroke.color);
            }
        }

        if self.doc.strokes.is_empty() {
            painter.text(
                rect.center(),
                MaraAlign2::CENTER_CENTER,
                format!("{}\ndrag to draw", self.doc.title),
                13.0,
                mara_core::style::on_panel_dim(),
            );
        }
    }

    fn tool_ribbon(&self, scope: RibbonScope) -> RibbonSlotDef {
        let pen = RibbonSlotItem::new(
            mara_core::vocab::Id::new(("canvas.pen", self.id)),
            "pen",
            "Pen",
            "Use pen tool",
            RibbonAction::Command(mara_core::vocab::Id::new(("canvas.pen.command", self.id))),
        );
        let clear = RibbonSlotItem::new(
            mara_core::vocab::Id::new(("canvas.clear", self.id)),
            "dismiss",
            "Clear",
            "Clear the whiteboard",
            RibbonAction::Command(mara_core::vocab::Id::new(("canvas.clear.command", self.id))),
        );
        RibbonSlotDef::new(
            mara_core::vocab::Id::new(("canvas.ribbon", self.id)),
            scope,
            RibbonEdge::Left,
            RibbonCluster::Middle,
            vec![
                RibbonSlot::new(
                    RibbonSlotId::new(("canvas.pen.slot", self.id)),
                    Some(pen),
                    RibbonOverridePolicy::Fixed,
                ),
                RibbonSlot::new(
                    RibbonSlotId::new(("canvas.clear.slot", self.id)),
                    Some(clear),
                    RibbonOverridePolicy::LayerOverride,
                ),
            ],
        )
    }
}

impl MaraView for CanvasSurface {
    fn id(&self) -> ViewId {
        ViewId(self.id)
    }

    fn title(&self) -> &str {
        &self.doc.title
    }

    fn icon(&self) -> &'static str {
        "pen"
    }

    fn ribbons(&mut self) -> Vec<RibbonSlotDef> {
        vec![self.tool_ribbon(RibbonScope::View(ViewId(self.id)))]
    }

    fn ribbon_overrides(&mut self) -> RibbonOverrideLayer {
        RibbonOverrideLayer::default()
    }

    fn show(&mut self, ctx: &mut ViewCtx<'_>) {
        // The view's framed surface covers the WHOLE region, edge to
        // edge — the ribbons sit ON this surface, inside the border, so
        // they are visually children of the view. Square corners: tiled
        // cells meet flush, and the border is the dividing line between
        // views. The stroke area (`body`) then insets away from the
        // ribbons.
        let region = ctx.screen_rect();
        let accent = mara_core::style::active_accent();
        {
            let painter = ctx.painter();
            painter.rect_filled(
                region,
                0.0,
                mara_core::style::fill_for(mara_core::style::FillRole::Pane, accent),
            );
            painter.rect_stroke(
                region,
                0.0,
                mara_core::style::stroke_for(mara_core::style::StrokeRole::WidgetBorder, accent),
            );
        }
        ctx.body(|mui| {
            self.paint_canvas_surface(mui, MaraVec2::new(120.0, 80.0), false);
        });
    }
}

impl MaraModule for CanvasSurface {
    fn id(&self) -> mara_core::vocab::Id {
        self.id.into()
    }

    fn title(&self) -> &str {
        &self.doc.title
    }

    fn icon(&self) -> &'static str {
        "pen"
    }

    fn inline(
        &mut self,
        mui: &mut mara_core::MaraUi<'_>,
        ctx: ModuleInlineCtx<'_>,
    ) -> ModuleResponse {
        let mut out = ModuleResponse::none();
        mui.framed(
            mara_core::style::frame_for(
                mara_core::style::FrameRole::Section,
                mara_core::style::active_accent(),
            ),
            |mui| {
                mui.horizontal(|mui| {
                    mui.label(&format!("Whiteboard: {}", self.doc.title));
                    mui.label(&format!("{} strokes", self.doc.strokes.len()));
                });
                self.paint_canvas(mui, MaraVec2::new(180.0, 120.0));
                if ctx.can_enter_workspace()
                    && mui.button("Open whiteboard workspace").clicked()
                {
                    out = ModuleResponse::enter_workspace();
                }
            },
        );
        out
    }

    fn workspace(&mut self, ws: &mut WorkspaceCtx<'_>) {
        ws.add_bar(
            WorkspaceBar::new(
                mara_core::vocab::Id::new(("canvas.workspace.bar", self.id)),
                WorkspaceBarEdge::Top,
                WorkspaceBarCluster::Middle,
            )
            .with_item(WorkspaceBarItem::command(
                mara_core::vocab::Id::new(("canvas.workspace.pen", self.id)),
                "Pen",
                Some("pen"),
            ))
            .with_item(WorkspaceBarItem::command(
                mara_core::vocab::Id::new(("canvas.workspace.clear", self.id)),
                "Clear",
                Some("dismiss"),
            )),
        );
        ws.add_ribbon(self.tool_ribbon(RibbonScope::WorkspaceLevel(ws.level.id)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_view<T: MaraView>(_value: &T) {}
    fn assert_module<T: MaraModule>(_value: &T) {}

    #[test]
    fn canvas_surface_is_both_view_and_module() {
        let surface = CanvasSurface::new("canvas", CanvasDocument::new("Whiteboard"));
        assert_view(&surface);
        assert_module(&surface);
        assert_eq!(MaraView::title(&surface), "Whiteboard");
        assert_eq!(MaraModule::icon(&surface), "pen");
    }

    #[test]
    fn document_keeps_retained_strokes() {
        let mut doc = CanvasDocument::new("Sketch");
        let mut stroke = CanvasStroke::new(MaraColor32::WHITE, 3.0);
        stroke.points.push(MaraPos2::new(1.0, 2.0));
        stroke.points.push(MaraPos2::new(3.0, 4.0));
        doc.strokes.push(stroke);
        assert_eq!(doc.strokes.len(), 1);
        doc.clear();
        assert!(doc.strokes.is_empty());
    }

    /// Portability proof: the canvas body draws only through
    /// [`mara_core::MaraPainter`], so rendering it over a headless
    /// recording backend — no egui `Ui`/`Context` — still emits
    /// `PaintCmd`s (here at least the pane background + border).
    #[test]
    fn canvas_paints_over_recording_backend() {
        use mara_core::mui::MaraRawBackend;

        let rect = mara_core::vocab::Rect::from_min_size(
            MaraPos2::new(0.0, 0.0),
            MaraVec2::new(200.0, 120.0),
        );
        let mut surface = CanvasSurface::new("canvas", CanvasDocument::new("Sketch"));
        let mut raw = MaraRawBackend::__internal_recording(rect);
        {
            let mut mui = mara_core::MaraUi::__internal_over(&mut raw, MaraColor32::WHITE);
            surface.paint_canvas(&mut mui, MaraVec2::new(200.0, 120.0));
        }

        let cmds = raw.__internal_recorded_canvas_commands();
        assert!(
            !cmds.is_empty(),
            "canvas body must emit PaintCmds headlessly — the portability contract"
        );
    }
}
