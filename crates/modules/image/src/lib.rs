//! `mara_image` — first proof module for the View + Module model.
//!
//! This crate intentionally starts lightweight: it does not pull in
//! image decoding or HTTP/file loaders yet. It proves the structural
//! contract from `PLAN.md`: the same surface can be launched as a
//! top-level [`mara_core::MaraView`] or embedded as a
//! [`mara_core::MaraModule`].

use mara_core::{
    MaraModule, MaraView, ModuleInlineCtx, ModuleResponse, RibbonAction, RibbonCluster, RibbonEdge,
    RibbonOverridePolicy, RibbonScope, RibbonSlot, RibbonSlotDef, RibbonSlotId, RibbonSlotItem,
    ViewCtx, ViewId, WorkspaceCtx,
    vocab::Align2 as MaraAlign2,
};

#[derive(Clone, Debug, PartialEq)]
pub enum ImageSource {
    Empty,
    Uri(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImageDocument {
    pub title: String,
    pub source: ImageSource,
}

impl ImageDocument {
    #[must_use]
    pub fn empty(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            source: ImageSource::Empty,
        }
    }

    #[must_use]
    pub fn uri(title: impl Into<String>, uri: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            source: ImageSource::Uri(uri.into()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ImageSurface {
    id: egui::Id,
    doc: ImageDocument,
}

impl ImageSurface {
    #[must_use]
    pub fn new(id: impl std::hash::Hash, doc: ImageDocument) -> Self {
        Self {
            id: egui::Id::new(id),
            doc,
        }
    }

    #[must_use]
    pub fn document(&self) -> &ImageDocument {
        &self.doc
    }

}

impl MaraView for ImageSurface {
    fn id(&self) -> ViewId {
        ViewId::from(self.id)
    }

    fn title(&self) -> &str {
        &self.doc.title
    }

    fn icon(&self) -> &'static str {
        "image"
    }

    fn ribbons(&mut self) -> Vec<RibbonSlotDef> {
        let fit = RibbonSlotItem::new(
            mara_core::vocab::Id::new(("image.fit", self.id)),
            "resize",
            "Fit",
            "Fit image to view",
            RibbonAction::Command(mara_core::vocab::Id::new(("image.fit.command", self.id))),
        );
        vec![RibbonSlotDef::new(
            mara_core::vocab::Id::new(("image.view.ribbon", self.id)),
            RibbonScope::View(ViewId::from(self.id)),
            RibbonEdge::Bottom,
            RibbonCluster::Middle,
            vec![RibbonSlot::new(
                RibbonSlotId::new(("image.fit.slot", self.id)),
                Some(fit),
                RibbonOverridePolicy::Fixed,
            )],
        )]
    }

    fn show(&mut self, ctx: &mut ViewCtx<'_>) {
        // The view's framed surface covers the WHOLE region, edge to
        // edge — the ribbons sit ON this surface, inside the border, so
        // they are visually children of the view. Square corners: tiled
        // cells meet flush, and the border is the dividing line between
        // views.
        let region = ctx.screen_rect();
        let accent = mara_core::style::active_accent();
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
        let detail = match &self.doc.source {
            ImageSource::Empty => "no image loaded".to_owned(),
            ImageSource::Uri(uri) => uri.clone(),
        };
        // Center the placeholder on the CONTENT rect (the region minus
        // this view's own ribbons), so the text never sits under a rail.
        painter.text(
            ctx.content_rect().center(),
            MaraAlign2::CENTER_CENTER,
            format!("{}\n{}", self.doc.title, detail),
            13.0,
            mara_core::style::on_panel(),
        );
    }
}

impl MaraModule for ImageSurface {
    fn id(&self) -> mara_core::vocab::Id {
        self.id.into()
    }

    fn title(&self) -> &str {
        &self.doc.title
    }

    fn icon(&self) -> &'static str {
        "image"
    }

    fn inline(
        &mut self,
        mui: &mut mara_core::MaraUi<'_>,
        ctx: ModuleInlineCtx<'_>,
    ) -> ModuleResponse {
        mui.label(&format!("Image: {}", self.doc.title));
        match &self.doc.source {
            ImageSource::Empty => {
                mui.label("No image loaded");
            }
            ImageSource::Uri(uri) => {
                mui.label(uri);
            }
        };
        if ctx.can_enter_workspace() && mui.button("Open image workspace").clicked() {
            ModuleResponse::enter_workspace()
        } else {
            ModuleResponse::none()
        }
    }

    fn workspace(&mut self, ws: &mut WorkspaceCtx<'_>) {
        ws.add_bar(mara_core::WorkspaceBar::new(
            egui::Id::new(("image.workspace.bar", self.id)),
            mara_core::WorkspaceBarEdge::Top,
            mara_core::WorkspaceBarCluster::Middle,
        ));
        ws.add_ribbon(RibbonSlotDef::new(
            mara_core::vocab::Id::new(("image.workspace.ribbon", self.id)),
            RibbonScope::WorkspaceLevel(ws.level.id),
            RibbonEdge::Top,
            RibbonCluster::Middle,
            vec![RibbonSlot::new(
                RibbonSlotId::new(("image.workspace.fit.slot", self.id)),
                Some(RibbonSlotItem::new(
                    mara_core::vocab::Id::new(("image.workspace.fit", self.id)),
                    "resize",
                    "Fit",
                    "Fit image to workspace",
                    RibbonAction::Command(mara_core::vocab::Id::new((
                        "image.workspace.fit.command",
                        self.id,
                    ))),
                )),
                RibbonOverridePolicy::Fixed,
            )],
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_view<T: MaraView>(_value: &T) {}
    fn assert_module<T: MaraModule>(_value: &T) {}

    #[test]
    fn image_surface_is_both_view_and_module() {
        let surface = ImageSurface::new("image", ImageDocument::empty("Image"));
        assert_view(&surface);
        assert_module(&surface);
        assert_eq!(MaraView::title(&surface), "Image");
        assert_eq!(MaraModule::icon(&surface), "image");
    }

    /// Portability proof: the image placeholder draws only through
    /// [`mara_core::MaraPainter`], so it renders over a headless
    /// recording backend (no egui) and still emits `PaintCmd`s
    /// (background + border + title text).
    #[test]
    fn image_placeholder_paints_over_recording_backend() {
        use mara_core::mui::MaraRawBackend;

        let rect = mara_core::vocab::Rect::from_min_size(
            mara_core::vocab::Pos2::new(0.0, 0.0),
            MaraVec2::new(200.0, 140.0),
        );
        let doc = ImageDocument::uri("Photo", "file://x.png");
        let mut raw = MaraRawBackend::__internal_recording(rect);
        {
            let mut mui =
                mara_core::MaraUi::__internal_over(&mut raw, mara_core::vocab::Color32::WHITE);
            ImageSurface::paint_placeholder(&mut mui, &doc);
        }

        let cmds = raw.__internal_recorded_canvas_commands();
        assert!(
            !cmds.is_empty(),
            "image placeholder must emit PaintCmds headlessly — the portability contract"
        );
    }
}
