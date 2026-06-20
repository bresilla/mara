//! Mara-styled read-only progress bar. Two-row stacked layout:
//! caption on top, full-width filled bar below. Total height =
//! 2 × [`crate::style::UNIT`] (= 2U) by default.
//!
//! Digit-tumble and smoothed-fraction animations are deliberately
//! left out of the base widget; add them above this layer if needed.

use crate::{
    layout::{Sense, UiBackend},
    mui::MaraResponse,
    paint::PaintCmd,
    style::{
        BODY_FONT_SIZE, FillRole, RadiusRole, StrokeRole, contrast_text_for, fill_for,
        on_panel_dim, on_track, radius_for, stroke_for, theme,
    },
    vocab::{Align2, Color32, Pos2, Rect, Vec2},
};

/// Bar row height.
pub const PROGRESSBAR_ROW_H: f32 = 18.0;
/// Inline readout font size (monospace).
pub const PROGRESSBAR_VALUE_FONT: f32 = 11.0;

/// Backend-neutral progress bar renderer.
pub fn progressbar_backend(
    backend: &mut impl UiBackend,
    label: &str,
    fraction: f32,
    text: &str,
    accent: Color32,
    row_height: f32,
) -> MaraResponse {
    let total_w = backend.available_rect().width().max(1.0);
    let total_h = row_height * 2.0;
    let resp = backend.allocate(Vec2::new(total_w, total_h), Sense::Hover);
    let rect = resp.rect;
    let scale = row_height / PROGRESSBAR_ROW_H;
    // Caption row — left-aligned dim text. Body font size at the
    // default row height; scales linearly when the pod is resized.
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
    // Bar row.
    let bar_rect = Rect::from_min_size(
        Pos2::new(rect.left(), rect.top() + row_height),
        Vec2::new(total_w, row_height),
    );
    paint_bar_backend(backend, bar_rect, fraction, text, accent, scale);
    resp
}

fn paint_bar_backend(
    backend: &mut impl UiBackend,
    rect: Rect,
    fraction: f32,
    text: &str,
    accent: Color32,
    scale: f32,
) {
    let f = fraction.clamp(0.0, 1.0);
    let th = theme();
    let progress = th.widgets.progress;
    let corner = radius_for(RadiusRole::Widget);
    // Track background.
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
    // Filled portion. Two modes:
    //   * Smooth (PRO) — single accent rect from the left edge
    //     to `fraction × width`.
    //   * Segmented (GAME) — N discrete cells with 1-px gaps,
    //     each cell either lit (accent) or dim (track + low
    //     alpha) based on whether it falls below the fraction
    //     threshold. Mass Effect / Apex shield style.
    if f > 0.0 {
        if progress.segmented {
            let segments = progress.segments.max(1);
            let inner = shrink_rect(rect, progress.segment_inset);
            let total_gap = progress.segment_gap * (segments as f32 - 1.0);
            let cell_w = ((inner.width() - total_gap) / segments as f32).max(1.0);
            let lit_count = (f * segments as f32).round().clamp(0.0, segments as f32) as usize;
            let dim = Color32::from_rgba_unmultiplied(
                accent.r(),
                accent.g(),
                accent.b(),
                progress.dim_alpha,
            );
            for i in 0..segments {
                let x0 = inner.left() + (cell_w + progress.segment_gap) * i as f32;
                let cell = Rect::from_min_size(
                    Pos2::new(x0, inner.top()),
                    Vec2::new(cell_w, inner.height()),
                );
                let col = if i < lit_count { accent } else { dim };
                backend.paint(PaintCmd::RectFilled {
                    rect: cell,
                    corner: crate::vocab::CornerRadius::ZERO,
                    fill: col,
                });
            }
        } else {
            let fill_w = rect.width() * f;
            let fill_rect = Rect::from_min_size(rect.min, Vec2::new(fill_w, rect.height()));
            backend.paint(PaintCmd::RectFilled {
                rect: fill_rect,
                corner,
                fill: accent,
            });
        }
    }
    // Inline readout — paint twice with different colours so the
    // text reads against both halves of the bar (filled and
    // unfilled) without colour-bombing one side. Clip-rect each
    // half so the wrong-colour half doesn't bleed.
    if !text.is_empty() {
        let centre = rect.center();
        let split_x = rect.left() + rect.width() * f;
        let left_half = Rect::from_min_max(rect.min, Pos2::new(split_x, rect.max.y));
        let right_half = Rect::from_min_max(Pos2::new(split_x, rect.min.y), rect.max);
        // Over the filled portion: contrast against accent.
        backend.push_clip(left_half);
        backend.paint(PaintCmd::Text {
            pos: centre,
            anchor: Align2::CENTER_CENTER,
            text: text.to_owned(),
            size: (progress.value_font * scale).round(),
            color: contrast_text_for(accent),
            mono: true,
        });
        backend.pop_clip();
        // Over the unfilled portion: contrast against track.
        backend.push_clip(right_half);
        backend.paint(PaintCmd::Text {
            pos: centre,
            anchor: Align2::CENTER_CENTER,
            text: text.to_owned(),
            size: (progress.value_font * scale).round(),
            color: on_track(),
            mono: true,
        });
        backend.pop_clip();
    }
}

fn shrink_rect(rect: Rect, amount: f32) -> Rect {
    Rect::from_min_max(
        Pos2::new(rect.min.x + amount, rect.min.y + amount),
        Pos2::new(rect.max.x - amount, rect.max.y - amount),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::Id;

    #[derive(Default)]
    struct RecordingBackend {
        available: Rect,
        paints: Vec<PaintCmd>,
        clips: Vec<Rect>,
    }

    impl UiBackend for RecordingBackend {
        fn begin_area(&mut self, _host: crate::layout::AreaHost, rect: Rect) {
            self.available = rect;
        }

        fn allocate(&mut self, size: Vec2, _sense: Sense) -> MaraResponse {
            MaraResponse::synthetic(Rect::from_min_size(self.available.min, size))
        }

        fn interact(&mut self, rect: Rect, _id: Id, _sense: Sense) -> MaraResponse {
            MaraResponse::synthetic(rect)
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
    fn progressbar_backend_emits_label_track_fill_and_clipped_text() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 40.0)),
            paints: Vec::new(),
            clips: Vec::new(),
        };

        let response = progressbar_backend(
            &mut backend,
            "load",
            0.5,
            "50%",
            Color32::WHITE,
            PROGRESSBAR_ROW_H,
        );

        assert_eq!(response.rect.height(), PROGRESSBAR_ROW_H * 2.0);
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
            panic!("progressbar should emit label, track, fill, and two clipped readouts");
        };
        assert_eq!(label, "load");
        assert_eq!(left, "50%");
        assert_eq!(right, "50%");
    }
}
