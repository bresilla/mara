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

use crate::style::{
    ColorMode, RadiusRole, on_section, on_section_dim, radius_for, row_hover_fill,
    row_selected_fill, theme,
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
    pub body: egui::Response,
    /// Click target for the right-edge radio circle only.
    pub radio: egui::Response,
}

/// Plain select row — one click target across the whole row.
///
/// `id_salt` disambiguates this row from siblings (an index, an entity
/// id, a string). `selected` paints the body's selection tint;
/// `trailing` is rendered dim-right (e.g. an index, a hotkey).
/// Caller owns the state — wire `resp.clicked()` / `resp.double_clicked()`
/// up to your selection logic.
pub fn select_row(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash,
    label: &str,
    trailing: Option<&str>,
    selected: bool,
    accent: egui::Color32,
) -> egui::Response {
    select_row_h(
        ui,
        id_salt,
        label,
        trailing,
        selected,
        accent,
        theme().widgets.select.row_h,
    )
}

/// Variable-height plain select row — used by resizable pods.
pub fn select_row_h(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash,
    label: &str,
    trailing: Option<&str>,
    selected: bool,
    accent: egui::Color32,
    height: f32,
) -> egui::Response {
    let select = theme().widgets.select;
    let w = ui.available_width();
    let resp = ui.interact(
        egui::Rect::from_min_size(ui.cursor().min, egui::vec2(w, height)),
        ui.id().with(("mara_select_body", &id_salt)),
        egui::Sense::click(),
    );
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, height), egui::Sense::hover());
    paint_row_bg(ui, rect, selected, resp.hovered(), accent);
    paint_row_text(
        ui,
        rect,
        label,
        trailing,
        select.label_pad_l,
        select.trailing_pad_r,
    );
    resp
}

/// Hybrid select row — body click target + right-edge radio.
///
/// `radio_on` paints the radio's filled dot. Body and radio sub-rects
/// never intersect; their `Response` ids are independent so the two
/// click sources stay separate.
pub fn hybrid_select_row(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash,
    label: &str,
    trailing: Option<&str>,
    selected: bool,
    radio_on: bool,
    accent: egui::Color32,
) -> HybridSelectResponse {
    hybrid_select_row_h(
        ui,
        id_salt,
        label,
        trailing,
        selected,
        radio_on,
        accent,
        theme().widgets.select.row_h,
    )
}

/// Variable-height hybrid select row.
#[allow(clippy::too_many_arguments)]
pub fn hybrid_select_row_h(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash,
    label: &str,
    trailing: Option<&str>,
    selected: bool,
    radio_on: bool,
    accent: egui::Color32,
    height: f32,
) -> HybridSelectResponse {
    let select = theme().widgets.select;

    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, height), egui::Sense::hover());

    let radio_rect = egui::Rect::from_min_size(
        egui::pos2(
            rect.max.x - select.radio_slot_w - select.radio_pad_r,
            rect.min.y,
        ),
        egui::vec2(select.radio_slot_w, rect.height()),
    );
    let body_rect = egui::Rect::from_min_max(rect.min, egui::pos2(radio_rect.min.x, rect.max.y));

    let body = ui.interact(
        body_rect,
        ui.id().with(("mara_hybrid_body", &id_salt)),
        egui::Sense::click(),
    );
    let radio = ui.interact(
        radio_rect,
        ui.id().with(("mara_hybrid_radio", &id_salt)),
        egui::Sense::click(),
    );

    let any_hover = body.hovered() || radio.hovered();
    paint_row_bg(ui, rect, selected, any_hover, accent);
    paint_row_text(
        ui,
        body_rect,
        label,
        trailing,
        select.label_pad_l,
        select.trailing_pad_r,
    );

    // Radio: outline ring + filled dot when on. Hover brightens the
    // ring to accent so the control reads as interactive.
    let mid_y = rect.center().y;
    let painter = ui.painter_at(rect);
    let radio_center = egui::pos2(radio_rect.center().x, mid_y);
    let ring_color = if radio_on || radio.hovered() {
        accent
    } else {
        on_section_dim()
    };
    painter.circle_stroke(
        radio_center,
        select.radio_outer_r,
        egui::Stroke::new(select.radio_stroke_w, ring_color),
    );
    if radio_on {
        // Inner dot fills with `accent` UNLESS the row is also
        // accent-derived (GAME's accent panel + accent dot would
        // collide); in that case use a contrasting solid against
        // the panel so the dot stays visible.
        let dot_col = if matches!(theme().panel_fill_mode, ColorMode::FromAccent { .. }) {
            on_section()
        } else {
            accent
        };
        painter.circle_filled(
            radio_center,
            select.radio_outer_r - select.radio_dot_inset,
            dot_col,
        );
    }

    HybridSelectResponse { body, radio }
}

fn paint_row_bg(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    selected: bool,
    hovered: bool,
    accent: egui::Color32,
) {
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(
            rect,
            radius_for(RadiusRole::Compact),
            row_selected_fill(accent),
        );
    } else if hovered {
        painter.rect_filled(
            rect,
            radius_for(RadiusRole::Compact),
            row_hover_fill(accent),
        );
    }
}

fn paint_row_text(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    label: &str,
    trailing: Option<&str>,
    label_pad_l: f32,
    trailing_pad_r: f32,
) {
    let painter = ui.painter_at(rect);
    let mid_y = rect.center().y;
    painter.text(
        egui::pos2(rect.min.x + label_pad_l, mid_y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(theme().widgets.select.label_font),
        on_section(),
    );
    if let Some(t) = trailing {
        painter.text(
            egui::pos2(rect.max.x - trailing_pad_r, mid_y),
            egui::Align2::RIGHT_CENTER,
            t,
            egui::FontId::proportional(theme().widgets.select.trailing_font),
            on_section_dim(),
        );
    }
}
