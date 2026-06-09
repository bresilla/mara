//! Sealed-consumer proof crate.
//!
//! This crate compiles against `mara` with default-ish features and
//! **no** `egui` dependency and **no** `raw-egui` feature. It
//! exercises the full app-content surface — views, modules, pods,
//! panes, widgets, custom canvas drawing — purely through Mara's
//! typed API. If a change to Mara makes raw `egui` reachable (or
//! makes this surface insufficient), this crate is where it should
//! show up as a compile error.

use mara::ui::pod::Pod;
use mara::ui::vocab::{Align2, Color32, Id, Pos2, Rect, Stroke, Vec2};
use mara::ui::{
    MaraModule, MaraUi, MaraView, ModuleInlineCtx, ModuleResponse, RibbonAvoidance, ViewCtx,
    ViewId, WorkspaceCtx,
};

// ─── A sealed module: widgets + custom canvas drawing ─────────────

pub struct SealedGauge {
    pub value: f64,
    pub enabled: bool,
    pub query: String,
}

impl MaraModule for SealedGauge {
    fn id(&self) -> Id {
        Id::new("sealed.gauge")
    }

    fn title(&self) -> &str {
        "Sealed Gauge"
    }

    fn icon(&self) -> &'static str {
        "gauge"
    }

    fn inline(&mut self, mui: &mut MaraUi<'_>, ctx: ModuleInlineCtx<'_>) -> ModuleResponse {
        // Plain widgets through the sealed surface.
        mui.label("sealed module body");
        let _ = mui.toggle("enabled", &mut self.enabled);
        let _ = mui.slider("value", &mut self.value, 0.0..=100.0, 1, "%");
        let _ = mui.text_input(&mut self.query, "filter…");
        let resp = mui.button("apply");
        mui.context_menu(&resp, |m| {
            let _ = m.button("reset");
        });

        // Custom drawing through the sealed painter.
        let (painter, canvas_resp) = mui.canvas(Vec2::new(120.0, 60.0));
        let rect: Rect = canvas_resp.rect;
        painter.rect_filled(rect, 4.0, Color32::from_gray(30));
        painter.circle_stroke(rect.center(), 20.0, Stroke::new(2.0, mui.accent()));
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            format!("{:.0}", self.value),
            12.0,
            Color32::WHITE,
        );
        painter.line_segment(
            Pos2::new(rect.left(), rect.bottom()),
            Pos2::new(rect.right(), rect.bottom()),
            Stroke::new(1.0, Color32::GRAY),
        );

        // Sealed input snapshot.
        let input = mui.input();
        if input.primary_pressed && canvas_resp.hovered {
            self.value = (self.value + 1.0).min(100.0);
        }

        if ctx.can_enter_workspace() && resp.clicked {
            ModuleResponse::enter_workspace()
        } else {
            ModuleResponse::none()
        }
    }

    fn workspace(&mut self, _ws: &mut WorkspaceCtx<'_>) {}
}

// ─── A sealed view: backdrop painting + pods in panes ─────────────

pub struct SealedView {
    pub gauge_value: f64,
}

impl MaraView for SealedView {
    fn id(&self) -> ViewId {
        ViewId::new("sealed.view")
    }

    fn title(&self) -> &str {
        "Sealed"
    }

    fn icon(&self) -> &'static str {
        "grid"
    }

    fn content_avoidance(&self) -> RibbonAvoidance {
        RibbonAvoidance::all()
    }

    fn show(&mut self, ctx: &mut ViewCtx<'_>) {
        // Edge-to-edge backdrop through the sealed view painter.
        let painter = ctx.painter();
        let screen = ctx.screen_rect();
        painter.rect_filled(screen, 0.0, Color32::from_gray(12));

        // Widget body laid over the ribbon-avoiding content rect.
        let value = &mut self.gauge_value;
        ctx.body(|mui| {
            mui.label("sealed view body");
            let _ = mui.readout("status", "ok");
            let _ = mui.drag_value("gauge", value, 0.1, 0.0..=10.0, 2, "x");
            mui.section("sealed.section", "Details", true, |m| {
                let _ = m.keybinding_row("Ctrl+K", "Command palette");
                let _ = m.chip("sealed");
            });
        });

        // Typed pod content (the only thing containers accept).
        let accent = ctx.accent;
        let pod = Pod::new(Id::new("sealed.pod"))
            .with_search("search…", accent)
            .with_toggle("visible", accent)
            .with_button("refresh", accent);
        ctx.body(|mui| {
            let _ = mui.pod(pod);
        });
    }
}
