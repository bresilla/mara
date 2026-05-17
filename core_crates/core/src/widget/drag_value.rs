//! Mara-styled numeric drag input. Label on the left, fixed-width
//! `egui::DragValue` on the right — drag horizontally to change the
//! value, click to type. 1U row.
//!
//! Mirrors the legacy drag-value widget. Drops the
//! row-layout opinion (70/30 dual-pane); pods compose drag-value
//! widgets into rows themselves.

use std::ops::RangeInclusive;

use crate::style::{BODY_FONT_SIZE, on_panel, theme};

/// Fixed width of the value box. Retains the legacy drag-value
/// input width so multiple drag-value
/// rows stack with their boxes aligned.
pub const DRAG_VALUE_INPUT_WIDTH: f32 = 72.0;
/// Default row height — same as toggle / progressbar / slider so
/// mixed rows in a pod line up.
pub const DRAG_VALUE_ROW_H: f32 = 18.0;

/// Labelled drag-value row.
pub fn drag_value(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f64,
    speed: f64,
    range: RangeInclusive<f64>,
    decimals: usize,
    suffix: &str,
) -> egui::Response {
    let row_h = theme().widgets.drag_value.row_h;
    drag_value_h(ui, label, value, speed, range, decimals, suffix, row_h)
}

/// Variable-height drag-value row — used by resizable pods.
#[allow(clippy::too_many_arguments)]
pub fn drag_value_h(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f64,
    speed: f64,
    range: RangeInclusive<f64>,
    decimals: usize,
    suffix: &str,
    height: f32,
) -> egui::Response {
    let drag = theme().widgets.drag_value;
    let scale = height / drag.row_h;
    let total_w = ui.available_width();
    let (row_rect, _) = ui.allocate_exact_size(egui::vec2(total_w, height), egui::Sense::hover());
    let input_w = (drag.input_w * scale).round();
    let input_rect = egui::Rect::from_min_size(
        egui::pos2(row_rect.right() - input_w, row_rect.top()),
        egui::vec2(input_w, height),
    );
    if !label.is_empty() {
        ui.painter().text(
            egui::pos2(row_rect.left(), row_rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional((BODY_FONT_SIZE * scale).round()),
            on_panel(),
        );
    }
    // Place the DragValue inside its own child UI so we can override
    // text + spacing without bleeding to siblings. `add_sized` uses
    // `centered_and_justified`, which only stretches main-axis and
    // leaves the button's natural HEIGHT (~15 px) centered in the
    // 18 px row — pushing the text + frame off the row's centreline.
    // `ui.put(rect, widget)` forces the widget into the exact
    // `input_rect`, so frame + text both span the full row and
    // share a centreline with the label glyph painted on the left.
    // Button vertical padding is also zeroed so the inner text
    // anchors at the rect's geometric centre rather than 1 px below.
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(input_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.style_mut().override_font_id =
        Some(egui::FontId::proportional((BODY_FONT_SIZE * scale).round()));
    child.style_mut().spacing.button_padding.y = 0.0;
    child.put(
        input_rect,
        egui::DragValue::new(value)
            .speed(speed)
            .range(range)
            .fixed_decimals(decimals)
            .suffix(suffix),
    )
}

/// Coloured-axis drag-value row — `glyph` (e.g. `"X"`) painted in
/// `glyph_color`, fixed-width DragValue on the right.
pub fn axis_drag(
    ui: &mut egui::Ui,
    glyph: &str,
    glyph_color: egui::Color32,
    value: &mut f64,
    speed: f64,
    suffix: &str,
    decimals: usize,
) -> egui::Response {
    let row_h = theme().widgets.drag_value.row_h;
    axis_drag_h(
        ui,
        glyph,
        glyph_color,
        value,
        speed,
        suffix,
        decimals,
        row_h,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn axis_drag_h(
    ui: &mut egui::Ui,
    glyph: &str,
    glyph_color: egui::Color32,
    value: &mut f64,
    speed: f64,
    suffix: &str,
    decimals: usize,
    height: f32,
) -> egui::Response {
    let drag = theme().widgets.drag_value;
    let scale = height / drag.row_h;
    let total_w = ui.available_width();
    let (row_rect, _) = ui.allocate_exact_size(egui::vec2(total_w, height), egui::Sense::hover());
    let input_w = (drag.input_w * scale).round();
    let input_rect = egui::Rect::from_min_size(
        egui::pos2(row_rect.right() - input_w, row_rect.top()),
        egui::vec2(input_w, height),
    );
    // Bold-monospace glyph, axis-tinted.
    ui.painter().text(
        egui::pos2(row_rect.left(), row_rect.center().y),
        egui::Align2::LEFT_CENTER,
        glyph,
        egui::FontId::new(
            (BODY_FONT_SIZE * scale).round(),
            egui::FontFamily::Monospace,
        ),
        glyph_color,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(input_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.style_mut().override_font_id = Some(egui::FontId::new(
        (BODY_FONT_SIZE * scale).round(),
        egui::FontFamily::Monospace,
    ));
    child.style_mut().spacing.button_padding.y = 0.0;
    child.put(
        input_rect,
        egui::DragValue::new(value)
            .speed(speed)
            .fixed_decimals(decimals)
            .suffix(suffix),
    )
}
