//! Mara-styled slider — caption on top, full-width interactive
//! value bar below. Click or drag anywhere on the bar to set
//! the value. 2-row stacked layout, same row height as
//! [`crate::widget::progressbar`] so paired sliders + progress bars
//! line up.
//!
//! This version keeps separators in container/pod chrome rather than
//! attaching them to the slider row.

use crate::style::{
    BODY_FONT_SIZE, FillRole, RadiusRole, StrokeRole, contrast_text_for, fill_for, on_panel_dim,
    on_track, radius_for, stroke_for, theme,
};

/// Bar row height.
pub const SLIDER_ROW_H: f32 = 18.0;
/// Inline value-readout font size.
pub const SLIDER_VALUE_FONT: f32 = 11.0;

/// Default labelled slider (2 × [`SLIDER_ROW_H`] = 36 px total).
/// `value` is mutated in-place by drags/clicks; `range` clamps;
/// `decimals` controls the inline readout precision; `suffix` is
/// appended to the readout (e.g. `"m/s"`, `"%"`, `""`).
pub fn slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
    suffix: &str,
    accent: egui::Color32,
) -> egui::Response {
    let row_h = theme().widgets.slider.row_h;
    slider_h(ui, label, value, range, decimals, suffix, accent, row_h)
}

/// Variable-height variant — `row_height` is the height of EACH
/// row (caption + bar), so total widget height is `2 × row_height`.
#[allow(clippy::too_many_arguments)]
pub fn slider_h(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
    suffix: &str,
    accent: egui::Color32,
    row_height: f32,
) -> egui::Response {
    let total_w = ui.available_width().max(1.0);
    let total_h = row_height * 2.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(total_w, total_h), egui::Sense::hover());
    let scale = row_height / theme().widgets.slider.row_h;
    if !label.is_empty() {
        ui.painter().text(
            egui::pos2(rect.left(), rect.top() + row_height * 0.5),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional((BODY_FONT_SIZE * scale).round()),
            on_panel_dim(),
        );
    }
    let bar_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left(), rect.top() + row_height),
        egui::vec2(total_w, row_height),
    );
    let bar_id = ui.id().with(("mara_slider_bar", label));
    let mut bar_resp = ui.interact(bar_rect, bar_id, egui::Sense::click_and_drag());
    let (lo, hi) = (*range.start(), *range.end());
    let denom = (hi - lo).max(f64::EPSILON);
    if let Some(pos) = bar_resp.interact_pointer_pos()
        && (bar_resp.dragged() || bar_resp.clicked())
    {
        let t = ((pos.x - bar_rect.min.x) as f64 / bar_rect.width() as f64).clamp(0.0, 1.0);
        let new_val = (lo + t * denom).clamp(lo, hi);
        if (new_val - *value).abs() > f64::EPSILON {
            *value = new_val;
            bar_resp.mark_changed();
        }
    }
    let bar_resp = bar_resp.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    if ui.is_rect_visible(bar_rect) {
        let fraction = ((*value - lo) / denom).clamp(0.0, 1.0) as f32;
        let text = format!("{:.*}{}", decimals, *value, suffix);
        paint_value_bar(ui, bar_rect, fraction, &text, accent, scale);
    }
    bar_resp
}

fn paint_value_bar(
    ui: &egui::Ui,
    rect: egui::Rect,
    fraction: f32,
    text: &str,
    accent: egui::Color32,
    scale: f32,
) {
    let f = fraction.clamp(0.0, 1.0);
    let painter = ui.painter_at(rect);
    let corner = radius_for(RadiusRole::Widget);
    painter.rect(
        rect,
        corner,
        fill_for(FillRole::Track, accent),
        stroke_for(StrokeRole::WidgetBorder, accent),
        egui::epaint::StrokeKind::Inside,
    );
    if f > 0.0 {
        let fill_w = rect.width() * f;
        let fill_rect = egui::Rect::from_min_size(rect.min, egui::vec2(fill_w, rect.height()));
        painter.rect_filled(fill_rect, corner, accent);
    }
    if !text.is_empty() {
        let font = egui::FontId::new(
            (theme().widgets.slider.value_font * scale).round(),
            egui::FontFamily::Monospace,
        );
        let centre = rect.center();
        let split_x = rect.left() + rect.width() * f;
        let left_half = egui::Rect::from_min_max(rect.min, egui::pos2(split_x, rect.max.y));
        let right_half = egui::Rect::from_min_max(egui::pos2(split_x, rect.min.y), rect.max);
        let left_painter = ui.painter().clone().with_clip_rect(left_half);
        left_painter.text(
            centre,
            egui::Align2::CENTER_CENTER,
            text,
            font.clone(),
            contrast_text_for(accent),
        );
        let right_painter = ui.painter().clone().with_clip_rect(right_half);
        right_painter.text(centre, egui::Align2::CENTER_CENTER, text, font, on_track());
    }
}
