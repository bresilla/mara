//! Mara-styled slider — caption on top, full-width interactive
//! value bar below. Click or drag anywhere on the bar to set
//! the value. 2-row stacked layout, same row height as
//! [`crate::widget::progressbar`] so paired sliders + progress bars
//! line up.
//!
//! This version keeps separators in container/pod chrome rather than
//! attaching them to the slider row.

use crate::{
    layout::{Sense, UiBackend},
    mui::MaraResponse,
    paint::PaintCmd,
    style::{
        BODY_FONT_SIZE, FillRole, RadiusRole, StrokeRole, contrast_text_for, fill_for,
        on_panel_dim, on_track, radius_for, stroke_for, theme,
    },
    vocab::{Align2, Color32, Id, Pos2, Rect, Vec2},
};

/// Bar row height.
pub const SLIDER_ROW_H: f32 = 18.0;
/// Inline value-readout font size.
pub const SLIDER_VALUE_FONT: f32 = 11.0;

/// Default labelled slider (2 × [`SLIDER_ROW_H`] = 36 px total).
/// `value` is mutated in-place by drags/clicks; `range` clamps;
/// `decimals` controls the inline readout precision; `suffix` is
/// appended to the readout (e.g. `"m/s"`, `"%"`, `""`).
pub(crate) fn slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
    suffix: &str,
    accent: impl Into<Color32>,
) -> MaraResponse {
    let row_h = theme().widgets.slider.row_h;
    slider_h(ui, label, value, range, decimals, suffix, accent, row_h)
}

/// Variable-height variant — `row_height` is the height of EACH
/// row (caption + bar), so total widget height is `2 × row_height`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn slider_h(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
    suffix: &str,
    accent: impl Into<Color32>,
    row_height: f32,
) -> MaraResponse {
    let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
    slider_backend(
        &mut backend,
        label,
        value,
        range,
        decimals,
        suffix,
        accent.into(),
        row_height,
    )
}

/// Backend-neutral slider renderer.
#[allow(clippy::too_many_arguments)]
pub fn slider_backend(
    backend: &mut impl UiBackend,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
    suffix: &str,
    accent: Color32,
    row_height: f32,
) -> MaraResponse {
    let total_w = backend.available_rect().width().max(1.0);
    let total_h = row_height * 2.0;
    let rect = backend
        .allocate(Vec2::new(total_w, total_h), Sense::Hover)
        .rect;
    let scale = row_height / theme().widgets.slider.row_h;
    if !label.is_empty() {
        backend.paint(PaintCmd::Text {
            pos: Pos2::new(rect.left(), rect.top() + row_height * 0.5),
            anchor: Align2::LEFT_CENTER,
            text: label.to_owned(),
            size: (BODY_FONT_SIZE * scale).round(),
            color: on_panel_dim(),
            mono: false,
        });
    }
    let bar_rect = Rect::from_min_size(
        Pos2::new(rect.left(), rect.top() + row_height),
        Vec2::new(total_w, row_height),
    );
    let bar_id = Id::new((
        "mara_slider_bar",
        label,
        bar_rect.min.x.to_bits(),
        bar_rect.min.y.to_bits(),
    ));
    let mut bar_resp = backend.interact(bar_rect, bar_id, Sense::ClickAndDrag);
    let (lo, hi) = (*range.start(), *range.end());
    let denom = (hi - lo).max(f64::EPSILON);
    if let Some(pos) = bar_resp.interact_pointer
        && (bar_resp.dragged || bar_resp.clicked)
    {
        let t = ((pos.x - bar_rect.min.x) as f64 / bar_rect.width() as f64).clamp(0.0, 1.0);
        let new_val = (lo + t * denom).clamp(lo, hi);
        if (new_val - *value).abs() > f64::EPSILON {
            *value = new_val;
            bar_resp.changed = true;
        }
    }
    let fraction = ((*value - lo) / denom).clamp(0.0, 1.0) as f32;
    let text = format!("{:.*}{}", decimals, *value, suffix);
    paint_value_bar_backend(backend, bar_rect, fraction, &text, accent, scale);
    bar_resp
}

fn paint_value_bar_backend(
    backend: &mut impl UiBackend,
    rect: Rect,
    fraction: f32,
    text: &str,
    accent: Color32,
    scale: f32,
) {
    let f = fraction.clamp(0.0, 1.0);
    let corner = radius_for(RadiusRole::Widget);
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
    if f > 0.0 {
        let fill_w = rect.width() * f;
        let fill_rect = Rect::from_min_size(rect.min, Vec2::new(fill_w, rect.height()));
        backend.paint(PaintCmd::RectFilled {
            rect: fill_rect,
            corner,
            fill: accent,
        });
    }
    if !text.is_empty() {
        let centre = rect.center();
        let split_x = rect.left() + rect.width() * f;
        let left_half = Rect::from_min_max(rect.min, Pos2::new(split_x, rect.max.y));
        let right_half = Rect::from_min_max(Pos2::new(split_x, rect.min.y), rect.max);
        backend.push_clip(left_half);
        backend.paint(PaintCmd::Text {
            pos: centre,
            anchor: Align2::CENTER_CENTER,
            text: text.to_owned(),
            size: (theme().widgets.slider.value_font * scale).round(),
            color: contrast_text_for(accent),
            mono: true,
        });
        backend.pop_clip();
        backend.push_clip(right_half);
        backend.paint(PaintCmd::Text {
            pos: centre,
            anchor: Align2::CENTER_CENTER,
            text: text.to_owned(),
            size: (theme().widgets.slider.value_font * scale).round(),
            color: on_track(),
            mono: true,
        });
        backend.pop_clip();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingBackend {
        available: Rect,
        paints: Vec<PaintCmd>,
        clips: Vec<Rect>,
        interaction: Option<MaraResponse>,
    }

    impl UiBackend for RecordingBackend {
        fn begin_area(&mut self, _host: crate::layout::AreaHost, rect: Rect) {
            self.available = rect;
        }

        fn allocate(&mut self, size: Vec2, _sense: Sense) -> MaraResponse {
            MaraResponse::synthetic(Rect::from_min_size(self.available.min, size))
        }

        fn interact(&mut self, rect: Rect, _id: Id, _sense: Sense) -> MaraResponse {
            self.interaction
                .clone()
                .unwrap_or_else(|| MaraResponse::synthetic(rect))
        }

        fn available_rect(&self) -> Rect {
            self.available
        }

        fn push_clip(&mut self, rect: Rect) {
            self.clips.push(rect);
        }

        fn pop_clip(&mut self) {}

        fn measure_text(&self, text: &str, size: f32, _mono: bool) -> Vec2 {
            Vec2::new(text.len() as f32 * size * 0.5, size)
        }

        fn paint(&mut self, cmd: PaintCmd) {
            self.paints.push(cmd);
        }
    }

    #[test]
    fn slider_backend_emits_label_track_fill_and_clipped_text() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 40.0)),
            paints: Vec::new(),
            clips: Vec::new(),
            interaction: None,
        };
        let mut value = 0.25;

        let response = slider_backend(
            &mut backend,
            "speed",
            &mut value,
            0.0..=1.0,
            2,
            "",
            Color32::WHITE,
            SLIDER_ROW_H,
        );

        assert_eq!(response.rect.height(), SLIDER_ROW_H);
        assert_eq!(backend.clips.len(), 2);
        assert_eq!(backend.paints.len(), 6);
        let [
            PaintCmd::Text { text: label, .. },
            PaintCmd::RectFilled { .. },
            PaintCmd::RectStroke { .. },
            PaintCmd::RectFilled { .. },
            PaintCmd::Text {
                text: left,
                mono: true,
                ..
            },
            PaintCmd::Text {
                text: right,
                mono: true,
                ..
            },
        ] = backend.paints.as_slice()
        else {
            panic!("slider should emit label, track, fill, and two clipped readouts");
        };
        assert_eq!(label, "speed");
        assert_eq!(left, "0.25");
        assert_eq!(right, "0.25");
    }

    #[test]
    fn slider_backend_click_updates_value_and_marks_changed() {
        let bar = Rect::from_min_size(Pos2::new(0.0, SLIDER_ROW_H), Vec2::new(200.0, SLIDER_ROW_H));
        let mut interaction = MaraResponse::synthetic(bar);
        interaction.clicked = true;
        interaction.interact_pointer = Some(Pos2::new(150.0, SLIDER_ROW_H + 4.0));
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 40.0)),
            paints: Vec::new(),
            clips: Vec::new(),
            interaction: Some(interaction),
        };
        let mut value = 0.0;

        let response = slider_backend(
            &mut backend,
            "speed",
            &mut value,
            0.0..=1.0,
            2,
            "",
            Color32::WHITE,
            SLIDER_ROW_H,
        );

        assert!((value - 0.75).abs() < f64::EPSILON);
        assert!(response.changed);
    }
}
