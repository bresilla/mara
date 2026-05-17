//! Mara-styled single-line text input.
//!
//! Shape:
//!
//! ```text
//!   🔍  query text…                    ✕
//!   └── leading glyph       trailing clear button
//! ```
//!
//! A thin wrapper around `egui::TextEdit::singleline` with:
//!
//! * A leading magnifier glyph painted inside the field.
//! * A trailing `✕` glyph that clears the buffer when clicked.
//! * Accent-tinted border using the same [`widget_border`] recipe
//!   every other mara input wears.
//! * Height = 20 px so it sits flush with a pane title row or a
//!   container header.
//!
//! Returns the `TextEdit`'s `Response`; call `.changed()` to react
//! to each keystroke (the clear button also marks the response as
//! changed when clicked).
//!
//! Ported verbatim from `maracore::widgets::search::search_field`,
//! minus the `flush_pending_separator` glue (mara_core doesn't carry
//! that side-channel).

use egui;

use crate::icons;
use crate::style;

/// Width of the leading / trailing glyph columns.
const GLYPH_W: f32 = 18.0;
/// Padding between the text and the glyph columns.
const TEXT_PAD: f32 = 4.0;

/// Render a text input at the canonical 1U height
/// ([`crate::style::UNIT`]). See [`text_input_h`] for the
/// variable-height variant used by resizable pods.
pub fn text_input(
    ui: &mut egui::Ui,
    text: &mut String,
    placeholder: &str,
    accent: egui::Color32,
) -> egui::Response {
    text_input_h(ui, text, placeholder, accent, crate::style::UNIT)
}

/// Render a text input bound to `text` at the requested `height`.
/// `placeholder` shows as a dim hint when the buffer is empty.
/// Returns the inner `TextEdit`'s `Response` (with `.rect` extended
/// to cover the full field — leading icon + text strip + clear
/// button); the clear-button click marks it `.changed()` too.
pub fn text_input_h(
    ui: &mut egui::Ui,
    text: &mut String,
    placeholder: &str,
    accent: egui::Color32,
    height: f32,
) -> egui::Response {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, height), egui::Sense::hover());

    // Background + border — accent-tinted glass surface, same recipe
    // a dropdown trigger / DragValue input would use.
    if ui.is_rect_visible(rect) {
        ui.painter().rect(
            rect,
            style::radius_for(style::RadiusRole::Widget),
            style::fill_for(style::FillRole::Track, accent),
            style::stroke_for(style::StrokeRole::WidgetBorder, accent),
            egui::StrokeKind::Inside,
        );
    }

    // Leading magnifier glyph.
    let mid_y = rect.center().y;
    let glyph_pos = egui::pos2(rect.min.x + GLYPH_W * 0.5, mid_y);
    let glyph_color = style::on_track_dim();
    icons::paint_icon(
        ui.painter(),
        glyph_pos,
        egui::Align2::CENTER_CENTER,
        "search",
        13.0,
        glyph_color,
    );

    // Trailing clear (✕) hit-rect + glyph. Only visible / clickable
    // when the buffer is non-empty so an empty field doesn't show a
    // dead button.
    let clear_rect = egui::Rect::from_min_size(
        egui::pos2(rect.max.x - GLYPH_W, rect.min.y),
        egui::vec2(GLYPH_W, height),
    );
    let mut cleared = false;
    if !text.is_empty() {
        let clear_resp = ui
            .interact(
                clear_rect,
                ui.id().with("mara_text_input_clear"),
                egui::Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let color = if clear_resp.hovered() {
            accent
        } else {
            style::on_track_dim()
        };
        icons::paint_icon(
            ui.painter(),
            clear_rect.center(),
            egui::Align2::CENTER_CENTER,
            "dismiss",
            13.0,
            color,
        );
        if clear_resp.clicked() {
            text.clear();
            cleared = true;
        }
    }

    // Inner TextEdit rect — carved out of the full rect minus the
    // two glyph columns and their padding.
    let text_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x + GLYPH_W + TEXT_PAD, rect.min.y),
        egui::pos2(rect.max.x - GLYPH_W - TEXT_PAD, rect.max.y),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(text_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    // Dim the typed text to 50 % alpha so the field reads as a soft
    // input rather than punching out at full contrast.
    let dim_text = {
        let base = style::on_track();
        egui::Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 128)
    };
    let mut resp = child.add(
        egui::TextEdit::singleline(text)
            .desired_width(text_rect.width())
            .frame(false)
            .background_color(egui::Color32::TRANSPARENT)
            .text_color(dim_text)
            .hint_text(placeholder),
    );
    if cleared {
        resp.mark_changed();
    }
    // Extend the response's rect from the inner TextEdit to the
    // full painted field — the leading magnifier glyph, the trailing
    // clear button, and the rounded glass border are all PART of
    // the widget the caller sees, so callers / inspectors should
    // receive the full field rect, not just the carved-out text
    // strip in the middle.
    resp.rect = rect;
    resp
}
