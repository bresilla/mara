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
};

/// A retained freehand stroke in logical canvas points.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasStroke {
    pub points: Vec<egui::Pos2>,
    pub color: egui::Color32,
    pub width: f32,
}

impl CanvasStroke {
    #[must_use]
    pub fn new(color: egui::Color32, width: f32) -> Self {
        Self {
            points: Vec::new(),
            color,
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
    id: egui::Id,
    doc: CanvasDocument,
    pen_width: f32,
}

impl CanvasSurface {
    #[must_use]
    pub fn new(id: impl std::hash::Hash, doc: CanvasDocument) -> Self {
        Self {
            id: egui::Id::new(id),
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

    fn paint_canvas(&mut self, ui: &mut egui::Ui, min_size: egui::Vec2) {
        let desired = egui::vec2(
            ui.available_width().max(min_size.x),
            ui.available_height().max(min_size.y),
        );
        let (response, painter) = ui.allocate_painter(desired, egui::Sense::drag());
        let rect = response.rect;
        let accent = mara_core::style::active_accent();
        let radius = mara_core::style::radius_for(mara_core::style::RadiusRole::Section);

        painter.rect_filled(
            rect,
            radius,
            mara_core::style::fill_for(mara_core::style::FillRole::Pane, accent),
        );
        painter.rect_stroke(
            rect,
            radius,
            mara_core::style::stroke_for(mara_core::style::StrokeRole::WidgetBorder, accent),
            egui::StrokeKind::Inside,
        );

        if response.drag_started() {
            let mut stroke = CanvasStroke::new(accent, self.pen_width);
            if let Some(pos) = response.interact_pointer_pos()
                && rect.contains(pos)
            {
                stroke.points.push(pos);
            }
            self.doc.strokes.push(stroke);
        } else if response.dragged()
            && let Some(pos) = response.interact_pointer_pos()
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
                    [segment[0], segment[1]],
                    egui::Stroke::new(stroke.width, stroke.color),
                );
            }
            if let Some(point) = stroke.points.first() {
                painter.circle_filled(*point, (stroke.width * 0.5).max(1.0), stroke.color);
            }
        }

        if self.doc.strokes.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("{}\ndrag to draw", self.doc.title),
                egui::FontId::proportional(13.0),
                mara_core::style::on_panel_dim(),
            );
        }
    }

    fn tool_ribbon(&self, scope: RibbonScope) -> RibbonSlotDef {
        let pen = RibbonSlotItem::new(
            egui::Id::new(("canvas.pen", self.id)),
            "draw",
            "Pen",
            "Use pen tool",
            RibbonAction::Command(egui::Id::new(("canvas.pen.command", self.id))),
        );
        let clear = RibbonSlotItem::new(
            egui::Id::new(("canvas.clear", self.id)),
            "dismiss",
            "Clear",
            "Clear the whiteboard",
            RibbonAction::Command(egui::Id::new(("canvas.clear.command", self.id))),
        );
        RibbonSlotDef::new(
            egui::Id::new(("canvas.ribbon", self.id)),
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
        "draw"
    }

    fn ribbons(&mut self) -> Vec<RibbonSlotDef> {
        vec![self.tool_ribbon(RibbonScope::View(ViewId(self.id)))]
    }

    fn ribbon_overrides(&mut self) -> RibbonOverrideLayer {
        RibbonOverrideLayer::default()
    }

    fn show(&mut self, ctx: &mut ViewCtx<'_>) {
        egui::CentralPanel::default().show(ctx.egui_ctx, |ui| {
            self.paint_canvas(ui, egui::vec2(420.0, 300.0));
        });
    }
}

impl MaraModule for CanvasSurface {
    fn id(&self) -> egui::Id {
        self.id
    }

    fn title(&self) -> &str {
        &self.doc.title
    }

    fn icon(&self) -> &'static str {
        "draw"
    }

    fn inline(&mut self, ui: &mut egui::Ui, ctx: ModuleInlineCtx<'_>) -> ModuleResponse {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Whiteboard: {}", self.doc.title));
                ui.label(format!("{} strokes", self.doc.strokes.len()));
            });
            self.paint_canvas(ui, egui::vec2(180.0, 120.0));
            if ctx.can_enter_workspace() && ui.button("Open whiteboard workspace").clicked() {
                ModuleResponse::enter_workspace()
            } else {
                ModuleResponse::none()
            }
        })
        .inner
    }

    fn workspace(&mut self, ws: &mut WorkspaceCtx<'_>) {
        ws.add_bar(
            WorkspaceBar::new(
                egui::Id::new(("canvas.workspace.bar", self.id)),
                WorkspaceBarEdge::Top,
                WorkspaceBarCluster::Middle,
            )
            .with_item(WorkspaceBarItem::command(
                egui::Id::new(("canvas.workspace.pen", self.id)),
                "Pen",
                Some("draw"),
            ))
            .with_item(WorkspaceBarItem::command(
                egui::Id::new(("canvas.workspace.clear", self.id)),
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
        assert_eq!(MaraModule::icon(&surface), "draw");
    }

    #[test]
    fn document_keeps_retained_strokes() {
        let mut doc = CanvasDocument::new("Sketch");
        let mut stroke = CanvasStroke::new(egui::Color32::WHITE, 3.0);
        stroke.points.push(egui::pos2(1.0, 2.0));
        stroke.points.push(egui::pos2(3.0, 4.0));
        doc.strokes.push(stroke);
        assert_eq!(doc.strokes.len(), 1);
        doc.clear();
        assert!(doc.strokes.is_empty());
    }
}
