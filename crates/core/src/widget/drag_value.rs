//! Mara-styled numeric drag input. Label on the left, fixed-width
//! `egui::DragValue` on the right — drag horizontally to change the
//! value, click to type. 1U row.
//!
//! Pods compose drag-value widgets into rows themselves; this file
//! only owns the single input row.

use std::ops::RangeInclusive;

use crate::{
    layout::{Sense, UiBackend},
    mui::MaraResponse,
    paint::PaintCmd,
    style::{
        BODY_FONT_SIZE, FillRole, RadiusRole, StrokeRole, fill_for, on_panel, on_track, radius_for,
        stroke_for, theme,
    },
    vocab::{Align2, Color32, Id, Pos2, Rect, Vec2},
};

/// Fixed width of the value box, so multiple drag-value rows stack
/// with their boxes aligned.
pub const DRAG_VALUE_INPUT_WIDTH: f32 = 72.0;
/// Default row height — same as toggle / progressbar / slider so
/// mixed rows in a pod line up.
pub const DRAG_VALUE_ROW_H: f32 = 18.0;

/// Labelled drag-value row.
pub(crate) fn drag_value(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f64,
    speed: f64,
    range: RangeInclusive<f64>,
    decimals: usize,
    suffix: &str,
) -> MaraResponse {
    let row_h = theme().widgets.drag_value.row_h;
    drag_value_h(ui, label, value, speed, range, decimals, suffix, row_h)
}

/// Variable-height drag-value row — used by resizable pods.
#[allow(clippy::too_many_arguments)]
pub(crate) fn drag_value_h(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f64,
    speed: f64,
    range: RangeInclusive<f64>,
    decimals: usize,
    suffix: &str,
    height: f32,
) -> MaraResponse {
    let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
    drag_value_backend(
        &mut backend,
        label,
        value,
        speed,
        range,
        decimals,
        suffix,
        height,
    )
}

/// Backend-neutral non-text-editing drag-value row.
///
/// This keeps the immediate drag behavior but intentionally does not
/// implement click-to-type editing. Text editing remains a later
/// backend/runtime concern.
#[allow(clippy::too_many_arguments)]
pub fn drag_value_backend(
    backend: &mut impl UiBackend,
    label: &str,
    value: &mut f64,
    speed: f64,
    range: RangeInclusive<f64>,
    decimals: usize,
    suffix: &str,
    height: f32,
) -> MaraResponse {
    drag_value_row_backend(
        backend,
        label,
        on_panel(),
        false,
        value,
        speed,
        Some(range),
        decimals,
        suffix,
        height,
    )
}

#[allow(clippy::too_many_arguments)]
fn drag_value_row_backend(
    backend: &mut impl UiBackend,
    label: &str,
    label_color: Color32,
    label_mono: bool,
    value: &mut f64,
    speed: f64,
    range: Option<RangeInclusive<f64>>,
    decimals: usize,
    suffix: &str,
    height: f32,
) -> MaraResponse {
    let drag = theme().widgets.drag_value;
    let scale = height / drag.row_h;
    let total_w = backend.available_rect().width().max(0.0);
    let row_rect = backend
        .allocate(Vec2::new(total_w, height), Sense::Hover)
        .rect;
    let input_w = (drag.input_w * scale).round();
    let input_rect = Rect::from_min_size(
        Pos2::new(row_rect.right() - input_w, row_rect.top()),
        Vec2::new(input_w, height),
    );
    if !label.is_empty() {
        backend.paint(PaintCmd::Text {
            pos: Pos2::new(row_rect.left(), row_rect.center().y),
            anchor: Align2::LEFT_CENTER,
            text: label.to_owned(),
            size: (BODY_FONT_SIZE * scale).round(),
            color: label_color,
            mono: label_mono,
        });
    }
    let id = Id::new((
        "mara_drag_value",
        label,
        input_rect.min.x.to_bits(),
        input_rect.min.y.to_bits(),
    ));
    let mut resp = backend.interact(input_rect, id, Sense::ClickAndDrag);
    if resp.dragged && resp.drag_delta.x != 0.0 {
        let mut next = *value + resp.drag_delta.x as f64 * speed;
        if let Some(range) = &range {
            next = next.clamp(*range.start(), *range.end());
        }
        if (next - *value).abs() > f64::EPSILON {
            *value = next;
            resp.changed = true;
        }
    }
    paint_value_box(
        backend,
        input_rect,
        &format!("{:.*}{}", decimals, *value, suffix),
        scale,
    );
    resp
}

#[allow(clippy::too_many_arguments)]
pub fn axis_drag_backend(
    backend: &mut impl UiBackend,
    glyph: &str,
    glyph_color: Color32,
    value: &mut f64,
    speed: f64,
    suffix: &str,
    decimals: usize,
    height: f32,
) -> MaraResponse {
    drag_value_row_backend(
        backend,
        glyph,
        glyph_color,
        true,
        value,
        speed,
        None,
        decimals,
        suffix,
        height,
    )
}

fn paint_value_box(backend: &mut impl UiBackend, rect: Rect, text: &str, scale: f32) {
    let accent = theme().palette.text_primary;
    let corner = radius_for(RadiusRole::Compact);
    backend.paint(PaintCmd::RectFilled {
        rect,
        corner,
        fill: fill_for(FillRole::Track, accent),
    });
    backend.paint(PaintCmd::RectStroke {
        rect,
        corner,
        stroke: stroke_for(StrokeRole::WidgetBorder, accent),
    });
    backend.push_clip(rect);
    backend.paint(PaintCmd::Text {
        pos: rect.center(),
        anchor: Align2::CENTER_CENTER,
        text: text.to_owned(),
        size: (BODY_FONT_SIZE * scale).round(),
        color: on_track(),
        mono: true,
    });
    backend.pop_clip();
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::backend::record::RecordingBackend;

    #[test]
    fn drag_value_backend_emits_label_box_and_value_text() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(180.0, DRAG_VALUE_ROW_H)),
            paints: Vec::new(),
            clips: Vec::new(),
            interaction: None,
        };
        let mut value = 12.5;

        let response = drag_value_backend(
            &mut backend,
            "size",
            &mut value,
            0.1,
            0.0..=100.0,
            1,
            " px",
            DRAG_VALUE_ROW_H,
        );

        assert_eq!(response.rect.width(), DRAG_VALUE_INPUT_WIDTH);
        assert_eq!(backend.clips.len(), 1);
        assert_eq!(backend.paints.len(), 4);
        let [
            PaintCmd::Text { text: label, .. },
            PaintCmd::RectFilled { .. },
            PaintCmd::RectStroke { .. },
            PaintCmd::Text {
                text: readout,
                mono: true,
                ..
            },
        ] = backend.paints.as_slice()
        else {
            panic!("drag value should emit label, input chrome, and value text");
        };
        assert_eq!(label, "size");
        assert_eq!(readout, "12.5 px");
    }

    #[test]
    fn drag_value_backend_drag_updates_value_and_marks_changed() {
        let input = Rect::from_min_size(
            Pos2::new(108.0, 0.0),
            Vec2::new(DRAG_VALUE_INPUT_WIDTH, DRAG_VALUE_ROW_H),
        );
        let mut interaction = MaraResponse::synthetic(input);
        interaction.dragged = true;
        interaction.drag_delta = Vec2::new(10.0, 0.0);
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(180.0, DRAG_VALUE_ROW_H)),
            paints: Vec::new(),
            clips: Vec::new(),
            interaction: Some(interaction),
        };
        let mut value = 1.0;

        let response = drag_value_backend(
            &mut backend,
            "size",
            &mut value,
            0.5,
            0.0..=10.0,
            1,
            "",
            DRAG_VALUE_ROW_H,
        );

        assert_eq!(value, 6.0);
        assert!(response.changed);
    }
}
