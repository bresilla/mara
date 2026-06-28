//! Mara-styled binary on/off toggle in a labelled row layout.
//! Label on the left, pill track + sliding knob on the right —
//! row total height = 1U ([`crate::style::UNIT`]) by default.
//!
//! The standalone track variant is [`toggle_track_only`].

use crate::{
    layout::{Sense, UiBackend},
    mui::MaraResponse,
    paint::PaintCmd,
    style::{
        BODY_FONT_SIZE, FillRole, RadiusRole, StrokeRole, body_accent, fill_for, on_panel,
        on_track, radius_for, stroke_for, theme,
    },
    vocab::{Align2, Color32, Id, Pos2, Rect, Vec2},
};

/// Default toggle row height.
pub const TOGGLE_ROW_H: f32 = 18.0;
/// Track width.
pub const TOGGLE_TRACK_W: f32 = 38.0;

/// Backend-neutral labelled toggle renderer.
pub fn toggle_backend(
    backend: &mut impl UiBackend,
    label: &str,
    on: &mut bool,
    accent: Color32,
    height: f32,
) -> MaraResponse {
    let toggle = theme().widgets.toggle;
    let total_w = backend.available_rect().width().max(0.0);
    let row_resp = backend.allocate(Vec2::new(total_w, height), Sense::Hover);
    let row_rect = row_resp.rect;
    // Track keeps the original 38×18 proportions when
    // the row is at default height; scales linearly otherwise.
    let scale = height / toggle.row_h;
    let track_w = (toggle.track_w * scale).round();
    let track_rect = Rect::from_min_size(
        Pos2::new(row_rect.right() - track_w, row_rect.top()),
        Vec2::new(track_w, height),
    );
    let id = Id::new((
        "mara_toggle",
        label,
        row_rect.min.x.to_bits(),
        row_rect.min.y.to_bits(),
    ));
    let mut resp = backend.interact(track_rect, id, Sense::Click);
    if resp.clicked {
        *on = !*on;
        resp.changed = true;
    }

    // Label, vertically centred, font scaled to height.
    if !label.is_empty() {
        backend.paint(PaintCmd::Text {
            pos: Pos2::new(row_rect.left(), row_rect.center().y),
            anchor: Align2::LEFT_CENTER,
            text: label.to_owned(),
            size: (BODY_FONT_SIZE * scale).round(),
            color: on_panel(),
            mono: false,
        });
        // Bound the label to stop short of the track + gap (egui's
        // `text` doesn't auto-clip; we just trust the caller's
        // labels are short enough at the typical 1U row width.
        // For longer labels, render via `add(Label::new(label).truncate())`
        // — kept simple here for v1.
        let _ = toggle.label_track_gap;
    }
    paint_track_backend(backend, track_rect, *on, accent);
    resp
}

pub fn toggle_track_only_backend(
    backend: &mut impl UiBackend,
    on: &mut bool,
    accent: Color32,
    track_w: f32,
    height: f32,
) -> MaraResponse {
    let mut resp = backend.allocate(Vec2::new(track_w, height), Sense::Click);
    if resp.clicked {
        *on = !*on;
        resp.changed = true;
    }
    paint_track_backend(backend, resp.rect, *on, accent);
    resp
}

fn paint_track_backend(backend: &mut impl UiBackend, rect: Rect, on: bool, accent: Color32) {
    let toggle = theme().widgets.toggle;
    let how_on = if on { 1.0 } else { 0.0 };
    let body_acc = body_accent(accent);
    let track_bg = lerp_col(
        fill_for(FillRole::Track, accent),
        body_acc,
        how_on * toggle.track_accent_hint,
    );
    let corner = radius_for(RadiusRole::Compact);
    backend.paint(PaintCmd::RectFilled {
        rect,
        corner,
        fill: track_bg,
    });
    backend.paint(PaintCmd::RectStroke {
        rect,
        corner,
        stroke: stroke_for(StrokeRole::WidgetBorder, accent),
    });
    let knob_size = (rect.height() - toggle.knob_pad * 2.0).max(1.0);
    let x_min = rect.left() + toggle.knob_pad;
    let x_max = rect.right() - toggle.knob_pad - knob_size;
    let knob_x = x_min + (x_max - x_min) * how_on;
    let knob_rect = Rect::from_min_size(
        Pos2::new(knob_x, rect.top() + toggle.knob_pad),
        Vec2::new(knob_size, knob_size),
    );
    let knob_color = lerp_col(on_track(), body_acc, how_on);
    backend.paint(PaintCmd::RectFilled {
        rect: knob_rect,
        corner,
        fill: knob_color,
    });
    backend.paint(PaintCmd::RectStroke {
        rect: knob_rect,
        corner,
        stroke: stroke_for(StrokeRole::WidgetBorder, accent),
    });
}

fn lerp_col(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let blend = |x: u8, y: u8| ((x as f32) * (1.0 - t) + (y as f32) * t).round() as u8;
    Color32::from_rgb(
        blend(a.r(), b.r()),
        blend(a.g(), b.g()),
        blend(a.b(), b.b()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingBackend {
        available: Rect,
        paints: Vec<PaintCmd>,
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

        fn push_clip(&mut self, _rect: Rect) {}

        fn pop_clip(&mut self) {}

        fn measure_text(&self, text: &str, size: f32, _mono: bool) -> Vec2 {
            Vec2::new(text.len() as f32 * size * 0.5, size)
        }

        fn paint(&mut self, cmd: PaintCmd) {
            self.paints.push(cmd);
        }
    }

    #[test]
    fn toggle_backend_emits_label_track_and_knob_commands() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(160.0, TOGGLE_ROW_H)),
            paints: Vec::new(),
        };
        let mut on = true;

        let response = toggle_backend(
            &mut backend,
            "enabled",
            &mut on,
            Color32::WHITE,
            TOGGLE_ROW_H,
        );

        assert_eq!(response.rect.width(), TOGGLE_TRACK_W);
        assert_eq!(backend.paints.len(), 5);
        let [
            PaintCmd::Text { text, .. },
            PaintCmd::RectFilled { .. },
            PaintCmd::RectStroke { .. },
            PaintCmd::RectFilled { .. },
            PaintCmd::RectStroke { .. },
        ] = backend.paints.as_slice()
        else {
            panic!("toggle should emit label plus track/knob chrome");
        };
        assert_eq!(text, "enabled");
    }
}
