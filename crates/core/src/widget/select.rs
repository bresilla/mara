//! Mara-styled select rows.
//!
//! Two variants ship from the same paint primitive:
//!
//! * [`select_row`] — single click target on the whole row body.
//!   Use for plain "pick one of N" lists where each row's only
//!   semantic is "select this".
//! * [`hybrid_select_row`] — body click target PLUS an independent
//!   right-edge radio. Use when a row needs *transient* selection
//!   ("what am I pointing at right now") and *durable* pinning
//!   ("which is the canonical one") — the two click targets never
//!   intersect, so a click on the radio doesn't propagate to the
//!   body and vice-versa.
//!
//! Visual:
//! ```text
//!   [ Planet                                   #3        ]    <- select_row
//!   [ Planet                                   #3    (o) ]    <- hybrid_select_row
//!     └── body click                           └─┘   └─┘
//!                                              trailing  radio
//! ```
//!
//! Both variants paint a unified hover / selected fill across the
//! whole row, so the row reads as a single visual button while the
//! click routing under it stays split.

use crate::{
    layout::{Sense, UiBackend},
    mui::MaraResponse,
    paint::PaintCmd,
    style::{
        ColorMode, RadiusRole, on_section, on_section_dim, radius_for, row_hover_fill,
        row_selected_fill, theme,
    },
    vocab::{Align2, Color32, Id, Pos2, Rect, Stroke, Vec2},
};

/// Row height — matches the Blender 4 outliner / UE5 world-outliner
/// rhythm (20 px row, 12 px label).
pub const SELECT_ROW_H: f32 = 20.0;
/// Alias for callers that previously imported the hybrid-only constant.
pub const HYBRID_SELECT_ROW_H: f32 = SELECT_ROW_H;

/// The two independent `egui::Response`s produced by one
/// [`hybrid_select_row`]. Inspect each separately: `body` for
/// click / double-click / hover on the main row, `radio` for the
/// right-edge toggle.
#[derive(Debug)]
pub struct HybridSelectResponse {
    /// Click target covering everything except the radio slot.
    pub body: crate::mui::MaraResponse,
    /// Click target for the right-edge radio circle only.
    pub radio: crate::mui::MaraResponse,
}

pub fn select_row_backend(
    backend: &mut impl UiBackend,
    id_salt: impl std::hash::Hash,
    label: &str,
    trailing: Option<&str>,
    selected: bool,
    accent: Color32,
    height: f32,
) -> MaraResponse {
    let select = theme().widgets.select;
    let w = backend.available_rect().width().max(0.0);
    let rect = backend.allocate(Vec2::new(w, height), Sense::Hover).rect;
    let resp = backend.interact(rect, Id::new(("mara_select_body", id_salt)), Sense::Click);
    paint_row_bg_backend(backend, rect, selected, resp.hovered, accent);
    paint_row_text_backend(
        backend,
        rect,
        label,
        trailing,
        select.label_pad_l,
        select.trailing_pad_r,
    );
    resp
}

#[allow(clippy::too_many_arguments)]
pub fn hybrid_select_row_backend(
    backend: &mut impl UiBackend,
    id_salt: impl std::hash::Hash,
    label: &str,
    trailing: Option<&str>,
    selected: bool,
    radio_on: bool,
    accent: Color32,
    height: f32,
) -> HybridSelectResponse {
    let select = theme().widgets.select;

    let w = backend.available_rect().width().max(0.0);
    let rect = backend.allocate(Vec2::new(w, height), Sense::Hover).rect;

    let radio_rect = Rect::from_min_size(
        Pos2::new(
            rect.max.x - select.radio_slot_w - select.radio_pad_r,
            rect.min.y,
        ),
        Vec2::new(select.radio_slot_w, rect.height()),
    );
    let body_rect = Rect::from_min_max(rect.min, Pos2::new(radio_rect.min.x, rect.max.y));

    let body = backend.interact(
        body_rect,
        Id::new(("mara_hybrid_body", &id_salt)),
        Sense::Click,
    );
    let radio = backend.interact(
        radio_rect,
        Id::new(("mara_hybrid_radio", &id_salt)),
        Sense::Click,
    );

    let any_hover = body.hovered || radio.hovered;
    paint_row_bg_backend(backend, rect, selected, any_hover, accent);
    paint_row_text_backend(
        backend,
        body_rect,
        label,
        trailing,
        select.label_pad_l,
        select.trailing_pad_r,
    );

    // Radio: outline ring + filled dot when on. Hover brightens the
    // ring to accent so the control reads as interactive.
    let mid_y = rect.center().y;
    let radio_center = Pos2::new(radio_rect.center().x, mid_y);
    let ring_color = if radio_on || radio.hovered {
        accent
    } else {
        on_section_dim()
    };
    backend.paint(PaintCmd::CircleStroke {
        center: radio_center,
        radius: select.radio_outer_r,
        stroke: Stroke::new(select.radio_stroke_w, ring_color),
    });
    if radio_on {
        // Inner dot fills with `accent` UNLESS the row is also
        // accent-derived (GAME's accent panel + accent dot would
        // collide); in that case use a contrasting solid against
        // the panel so the dot stays visible.
        let dot_col: Color32 = if matches!(theme().panel_fill_mode, ColorMode::FromAccent { .. }) {
            on_section()
        } else {
            accent
        };
        backend.paint(PaintCmd::CircleFilled {
            center: radio_center,
            radius: select.radio_outer_r - select.radio_dot_inset,
            fill: dot_col,
        });
    }

    HybridSelectResponse { body, radio }
}

fn paint_row_bg_backend(
    backend: &mut impl UiBackend,
    rect: Rect,
    selected: bool,
    hovered: bool,
    accent: Color32,
) {
    if selected {
        backend.paint(PaintCmd::RectFilled {
            rect,
            corner: radius_for(RadiusRole::Compact),
            fill: row_selected_fill(accent),
        });
    } else if hovered {
        backend.paint(PaintCmd::RectFilled {
            rect,
            corner: radius_for(RadiusRole::Compact),
            fill: row_hover_fill(accent),
        });
    }
}

fn paint_row_text_backend(
    backend: &mut impl UiBackend,
    rect: Rect,
    label: &str,
    trailing: Option<&str>,
    label_pad_l: f32,
    trailing_pad_r: f32,
) {
    let mid_y = rect.center().y;
    backend.paint(PaintCmd::Text {
        pos: Pos2::new(rect.min.x + label_pad_l, mid_y),
        anchor: Align2::LEFT_CENTER,
        text: label.to_owned(),
        size: theme().widgets.select.label_font,
        color: on_section(),
        mono: false,
    });
    if let Some(t) = trailing {
        backend.paint(PaintCmd::Text {
            pos: Pos2::new(rect.max.x - trailing_pad_r, mid_y),
            anchor: Align2::RIGHT_CENTER,
            text: t.to_owned(),
            size: theme().widgets.select.trailing_font,
            color: on_section_dim(),
            mono: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::backend::record::RecordingBackend;

    #[test]
    fn select_row_backend_emits_selected_bg_label_and_trailing() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, SELECT_ROW_H)),
            paints: Vec::new(),
            interaction: None,
            ..Default::default()
        };

        let response = select_row_backend(
            &mut backend,
            "row-1",
            "Planet",
            Some("#3"),
            true,
            Color32::WHITE,
            SELECT_ROW_H,
        );

        assert_eq!(response.rect.width(), 200.0);
        assert_eq!(backend.paints.len(), 3);
        let [
            PaintCmd::RectFilled { .. },
            PaintCmd::Text { text: label, .. },
            PaintCmd::Text { text: trailing, .. },
        ] = backend.paints.as_slice()
        else {
            panic!("select row should emit selected bg, label, and trailing text");
        };
        assert_eq!(label, "Planet");
        assert_eq!(trailing, "#3");
    }

    #[test]
    fn hybrid_select_row_backend_emits_split_responses_and_radio() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(220.0, SELECT_ROW_H)),
            paints: Vec::new(),
            interaction: None,
            ..Default::default()
        };

        let response = hybrid_select_row_backend(
            &mut backend,
            "row-1",
            "Planet",
            Some("#3"),
            true,
            true,
            Color32::WHITE,
            SELECT_ROW_H,
        );

        assert!(response.body.rect.width() < 220.0);
        assert!(response.radio.rect.width() > 0.0);
        assert_eq!(backend.paints.len(), 5);
        let [
            PaintCmd::RectFilled { .. },
            PaintCmd::Text { text: label, .. },
            PaintCmd::Text { text: trailing, .. },
            PaintCmd::CircleStroke { .. },
            PaintCmd::CircleFilled { .. },
        ] = backend.paints.as_slice()
        else {
            panic!("hybrid select should emit bg, text, radio ring, and radio dot");
        };
        assert_eq!(label, "Planet");
        assert_eq!(trailing, "#3");
    }
}
