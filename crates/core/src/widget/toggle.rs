//! Mara-styled binary on/off toggle in a labelled row layout.
//! Label on the left, pill track + sliding knob on the right —
//! row total height = 1U ([`crate::style::UNIT`]) by default.
//!
//! Mirrors the legacy row-with-label toggle; the standalone track
//! variant corresponds to [`toggle_track_only`] here.

use crate::style::{
    BODY_FONT_SIZE, FillRole, RadiusRole, StrokeRole, body_accent, fill_for, on_panel, on_track,
    radius_for, stroke_for, theme,
};

/// Default toggle row height. Retained from the legacy toggle
/// metric so migrated UIs keep the same scale.
pub const TOGGLE_ROW_H: f32 = 18.0;
/// Track width — retained from the legacy toggle metric.
pub const TOGGLE_TRACK_W: f32 = 38.0;

/// Default labelled toggle row.
pub fn toggle(
    ui: &mut egui::Ui,
    label: &str,
    on: &mut bool,
    accent: egui::Color32,
) -> egui::Response {
    toggle_h(ui, label, on, accent, theme().widgets.toggle.row_h)
}

/// Variable-height variant. Track aspect (~2:1 of height) and
/// label font scale with `height` so the row reads consistently
/// regardless of pod resize.
pub fn toggle_h(
    ui: &mut egui::Ui,
    label: &str,
    on: &mut bool,
    accent: egui::Color32,
    height: f32,
) -> egui::Response {
    let toggle = theme().widgets.toggle;
    let total_w = ui.available_width();
    let (row_rect, _) = ui.allocate_exact_size(egui::vec2(total_w, height), egui::Sense::hover());
    // Track keeps the original 38×18 proportions when
    // the row is at default height; scales linearly otherwise.
    let scale = height / toggle.row_h;
    let track_w = (toggle.track_w * scale).round();
    let track_rect = egui::Rect::from_min_size(
        egui::pos2(row_rect.right() - track_w, row_rect.top()),
        egui::vec2(track_w, height),
    );
    let id = ui.id().with(("mara_toggle", label));
    let mut resp = ui
        .interact(track_rect, id, egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    resp.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *on, label)
    });
    if !ui.is_rect_visible(row_rect) {
        return resp;
    }

    // Label, vertically centred, font scaled to height.
    if !label.is_empty() {
        let label_font = egui::FontId::proportional((BODY_FONT_SIZE * scale).round());
        ui.painter().text(
            egui::pos2(row_rect.left(), row_rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            label_font,
            on_panel(),
        );
        // Bound the label to stop short of the track + gap (egui's
        // `text` doesn't auto-clip; we just trust the caller's
        // labels are short enough at the typical 1U row width.
        // For longer labels, render via `add(Label::new(label).truncate())`
        // — kept simple here for v1.
        let _ = toggle.label_track_gap;
    }
    paint_track(ui, track_rect, *on, resp.id, accent);
    resp
}

/// Standalone track + knob with no label, no row. For custom
/// compositions (e.g. an inline status row that already has its
/// own label rendering).
pub fn toggle_track_only(
    ui: &mut egui::Ui,
    on: &mut bool,
    accent: egui::Color32,
) -> egui::Response {
    let toggle = theme().widgets.toggle;
    let height = toggle.row_h;
    let track_w = toggle.track_w;
    let (rect, mut resp) =
        ui.allocate_exact_size(egui::vec2(track_w, height), egui::Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        paint_track(ui, rect, *on, resp.id, accent);
    }
    resp
}

fn paint_track(ui: &egui::Ui, rect: egui::Rect, on: bool, id: egui::Id, accent: egui::Color32) {
    let toggle = theme().widgets.toggle;
    let how_on = ui.ctx().animate_bool_responsive(id, on);
    let painter = ui.painter_at(rect);
    let body_acc = body_accent(accent);
    let track_bg = lerp_col(
        fill_for(FillRole::Track, accent),
        body_acc,
        how_on * toggle.track_accent_hint,
    );
    let corner = radius_for(RadiusRole::Compact);
    painter.rect(
        rect,
        corner,
        track_bg,
        stroke_for(StrokeRole::WidgetBorder, accent),
        egui::epaint::StrokeKind::Inside,
    );
    let knob_size = (rect.height() - toggle.knob_pad * 2.0).max(1.0);
    let x_min = rect.left() + toggle.knob_pad;
    let x_max = rect.right() - toggle.knob_pad - knob_size;
    let knob_x = egui::lerp(x_min..=x_max, how_on);
    let knob_rect = egui::Rect::from_min_size(
        egui::pos2(knob_x, rect.top() + toggle.knob_pad),
        egui::vec2(knob_size, knob_size),
    );
    let knob_color = lerp_col(on_track(), body_acc, how_on);
    painter.rect(
        knob_rect,
        corner,
        knob_color,
        stroke_for(StrokeRole::WidgetBorder, accent),
        egui::epaint::StrokeKind::Inside,
    );
}

fn lerp_col(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let blend = |x: u8, y: u8| ((x as f32) * (1.0 - t) + (y as f32) * t).round() as u8;
    egui::Color32::from_rgb(
        blend(a.r(), b.r()),
        blend(a.g(), b.g()),
        blend(a.b(), b.b()),
    )
}
