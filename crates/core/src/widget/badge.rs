//! Labelled chip cluster — `name: tag1 tag2 …`.
//!
//! Strictly single-line: a fixed-width label on the left, chips on
//! the right that flow left-to-right within the remaining row
//! width. Chips that don't fit get clipped — the row is always
//! exactly [`BADGE_ROW_H`] (= 1U) tall so it lines up with every
//! other 1U widget in a section column.
//!
//! Use when a row carries multiple categorical values that read
//! better as separate pills than as a single comma-joined string —
//! e.g. `lights  [12 dir] [4 pt] [2 spot] [1 dome]`.

use crate::{
    layout::{Sense, UiBackend},
    mui::MaraResponse,
    paint::PaintCmd,
    style::{
        RadiusRole, StrokeRole, UNIT, contrast_text_for, font, on_section, radius_for, stroke_for,
        theme,
    },
    vocab::{Align2, Color32, Pos2, Rect, Vec2},
    widget::chip::chip_fill,
};

/// Width of the label column, in px — wide enough for typical labels
/// at body font size.
pub const BADGE_LABEL_COL_W: f32 = 96.0;

/// Row height, in px. Matches the canonical 1U so a row of
/// `badge_row` calls aligns with neighbouring widgets.
pub const BADGE_ROW_H: f32 = UNIT;

/// Backend-neutral labelled badge row renderer.
pub fn badge_row_backend(
    backend: &mut impl UiBackend,
    label: &str,
    badges: &[&str],
    fills: Option<&[Option<Color32>]>,
    accent: Color32,
) -> MaraResponse {
    let badge = theme().widgets.badge;
    let total_w = backend.available_rect().width().max(0.0);
    let label_w = badge.label_col_w.min(total_w * 0.5);

    // Reserve the whole 1U row up front. The label paints into the
    // left cell and the chip cluster paints into a clipped right
    // cell; both share the SAME row rect so the row is guaranteed exactly
    // BADGE_ROW_H tall regardless of chip count.
    let resp = backend.allocate(Vec2::new(total_w, badge.row_h), Sense::Hover);

    // ── Label cell ──
    let label_rect = Rect::from_min_size(resp.rect.min, Vec2::new(label_w, badge.row_h));
    backend.push_clip(label_rect);
    backend.paint(PaintCmd::Text {
        pos: Pos2::new(label_rect.min.x + badge.label_pad_x, label_rect.center().y),
        anchor: Align2::LEFT_CENTER,
        text: label.to_owned(),
        size: badge.label_font,
        color: on_section(),
        mono: false,
    });
    backend.pop_clip();

    // ── Badges cell — strict left-to-right, NO wrap ──
    //
    // Single-row layout: chips that overflow are simply clipped by
    // the badges cell. Anchored vertically centred so chips line up
    // with the label baseline.
    let badges_x = resp.rect.min.x + label_w + badge.label_chips_gap;
    let badges_rect = Rect::from_min_max(Pos2::new(badges_x, resp.rect.min.y), resp.rect.max);
    if badges_rect.width() <= 0.0 {
        return resp;
    }

    backend.push_clip(badges_rect);
    let chip_theme = theme().widgets.chip;
    let mut cursor_x = badges_rect.min.x;
    for (i, b) in badges.iter().enumerate() {
        let fill = fills
            .and_then(|f| f.get(i).copied())
            .flatten()
            .unwrap_or_else(|| chip_fill(accent));
        let text_size = backend.measure_text(b, font::CAPTION, false);
        let chip_rect = Rect::from_min_size(
            Pos2::new(cursor_x, badges_rect.center().y - chip_theme.height * 0.5),
            Vec2::new(
                text_size.x.ceil() + chip_theme.pad_x * 2.0,
                chip_theme.height,
            ),
        );
        paint_badge_chip(backend, chip_rect, b, fill, accent);
        cursor_x = chip_rect.max.x + badge.chip_gap_x;
    }
    backend.pop_clip();

    resp
}

fn paint_badge_chip(
    backend: &mut impl UiBackend,
    rect: Rect,
    label: &str,
    fill: Color32,
    accent: Color32,
) {
    let chip = theme().widgets.chip;
    let text_col = if fill.a() >= 220 {
        contrast_text_for(fill)
    } else {
        on_section()
    };
    backend.paint(PaintCmd::RectFilled {
        rect,
        corner: radius_for(RadiusRole::Widget),
        fill,
    });
    backend.paint(PaintCmd::RectStroke {
        rect,
        corner: radius_for(RadiusRole::Widget),
        stroke: stroke_for(StrokeRole::WidgetBorder, accent),
    });
    backend.paint(PaintCmd::Text {
        pos: Pos2::new(rect.min.x + chip.pad_x, rect.center().y),
        anchor: Align2::LEFT_CENTER,
        text: label.to_owned(),
        size: font::CAPTION,
        color: text_col,
        mono: false,
    });
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

        fn pop_clip(&mut self) {
            let _ = self.clips.pop();
        }

        fn measure_text(&self, text: &str, size: f32, _mono: bool) -> Vec2 {
            Vec2::new(text.len() as f32 * size * 0.5, size)
        }

        fn paint(&mut self, cmd: PaintCmd) {
            self.paints.push(cmd);
        }
    }

    #[test]
    fn badge_backend_emits_label_and_chip_commands() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(260.0, BADGE_ROW_H)),
            paints: Vec::new(),
            clips: Vec::new(),
        };
        let fills = [None, Some(Color32::from_rgb(0, 200, 0))];

        let response = badge_row_backend(
            &mut backend,
            "lights",
            &["12 dir", "4 pt"],
            Some(&fills),
            Color32::WHITE,
        );

        assert_eq!(response.rect.width(), 260.0);
        assert_eq!(backend.paints.len(), 7);
        let PaintCmd::Text { text, .. } = &backend.paints[0] else {
            panic!("first badge command should paint the label");
        };
        assert_eq!(text, "lights");
        assert!(
            backend
                .paints
                .iter()
                .any(|cmd| matches!(cmd, PaintCmd::Text { text, .. } if text == "12 dir"))
        );
        assert!(
            backend
                .paints
                .iter()
                .any(|cmd| matches!(cmd, PaintCmd::Text { text, .. } if text == "4 pt"))
        );
    }
}
