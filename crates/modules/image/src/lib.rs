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

    fn paint_placeholder(ui: &mut egui::Ui, doc: &ImageDocument) {
        let avail = ui.available_size_before_wrap();
        let size = egui::vec2(avail.x.max(160.0), avail.y.max(120.0));
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(
            rect,
            mara_core::style::radius_for(mara_core::style::RadiusRole::Section),
            mara_core::style::fill_for(
                mara_core::style::FillRole::Pane,
                mara_core::style::active_accent(),
            ),
        );
        painter.rect_stroke(
            rect,
            mara_core::style::radius_for(mara_core::style::RadiusRole::Section),
            mara_core::style::stroke_for(
                mara_core::style::StrokeRole::WidgetBorder,
                mara_core::style::active_accent(),
            ),
            egui::StrokeKind::Inside,
        );

        let detail = match &doc.source {
            ImageSource::Empty => "no image loaded".to_owned(),
            ImageSource::Uri(uri) => uri.clone(),
        };
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("{}\n{}", doc.title, detail),
            egui::FontId::proportional(13.0),
            mara_core::style::on_panel(),
        );
    }
}

impl MaraView for ImageSurface {
    fn id(&self) -> ViewId {
        ViewId(self.id)
    }

    fn title(&self) -> &str {
        &self.doc.title
    }

    fn icon(&self) -> &'static str {
        "image"
    }

    fn ribbons(&mut self) -> Vec<RibbonSlotDef> {
        let fit = RibbonSlotItem::new(
            egui::Id::new(("image.fit", self.id)),
            "fit",
            "Fit",
            "Fit image to view",
            RibbonAction::Command(egui::Id::new(("image.fit.command", self.id))),
        );
        vec![RibbonSlotDef::new(
            egui::Id::new(("image.view.ribbon", self.id)),
            RibbonScope::View(ViewId(self.id)),
            RibbonEdge::Top,
            RibbonCluster::Middle,
            vec![RibbonSlot::new(
                RibbonSlotId::new(("image.fit.slot", self.id)),
                Some(fit),
                RibbonOverridePolicy::Fixed,
            )],
        )]
    }

    fn show(&mut self, ctx: &mut ViewCtx<'_>) {
        egui::CentralPanel::default().show(ctx.egui_ctx, |ui| {
            Self::paint_placeholder(ui, &self.doc);
        });
    }
}

impl MaraModule for ImageSurface {
    fn id(&self) -> egui::Id {
        self.id
    }

    fn title(&self) -> &str {
        &self.doc.title
    }

    fn icon(&self) -> &'static str {
        "image"
    }

    fn inline(&mut self, ui: &mut egui::Ui, ctx: ModuleInlineCtx<'_>) -> ModuleResponse {
        ui.group(|ui| {
            ui.label(format!("Image: {}", self.doc.title));
            match &self.doc.source {
                ImageSource::Empty => ui.label("No image loaded"),
                ImageSource::Uri(uri) => ui.label(uri),
            };
            if ctx.can_enter_workspace() && ui.button("Open image workspace").clicked() {
                ModuleResponse::enter_workspace()
            } else {
                ModuleResponse::none()
            }
        })
        .inner
    }

    fn workspace(&mut self, ws: &mut WorkspaceCtx<'_>) {
        ws.add_bar(mara_core::WorkspaceBar::new(
            egui::Id::new(("image.workspace.bar", self.id)),
            mara_core::WorkspaceBarEdge::Top,
            mara_core::WorkspaceBarCluster::Middle,
        ));
        ws.add_ribbon(RibbonSlotDef::new(
            egui::Id::new(("image.workspace.ribbon", self.id)),
            RibbonScope::WorkspaceLevel(ws.level.id),
            RibbonEdge::Top,
            RibbonCluster::Middle,
            vec![RibbonSlot::new(
                RibbonSlotId::new(("image.workspace.fit.slot", self.id)),
                Some(RibbonSlotItem::new(
                    egui::Id::new(("image.workspace.fit", self.id)),
                    "fit",
                    "Fit",
                    "Fit image to workspace",
                    RibbonAction::Command(egui::Id::new(("image.workspace.fit.command", self.id))),
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
}
