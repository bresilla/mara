//! Read-only information row — label on the left, monospace value
//! on the right. Use for surfaces that just *display* a piece of
//! data: "selected node", "current speed", "active tool", etc.
//!
//! Shape:
//! ```text
//!   selected                            /World/Robot/base
//!   └── label (left)                    └── value (right, monospace)
//! ```
//!
//! Stateless — caller passes the current value as `&str` each frame.
//! Returns a `Response` so callers can attach hover tooltips or
//! detect double-clicks (e.g. "double-click to copy").
//!
//! Separator chrome is owned by `Pod`, not by the readout widget.

use crate::style::{UNIT, on_section, on_section_dim, theme};

/// Default readout row height — the canonical 1U.
pub const READOUT_ROW_H: f32 = UNIT;
/// Render a readout row at the canonical 1U height.
pub fn readout(ui: &mut egui::Ui, label: &str, value: &str) -> egui::Response {
    readout_h(ui, label, value, theme().widgets.readout.row_h)
}

/// Variable-height variant — used by resizable pods.
pub fn readout_h(ui: &mut egui::Ui, label: &str, value: &str, height: f32) -> egui::Response {
    let avail_w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(avail_w, height), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let painter = ui.painter_at(rect);
    let readout = theme().widgets.readout;
    let mid_y = rect.center().y;
    // Label left — full-contrast `on_section`.
    painter.text(
        egui::pos2(rect.min.x + readout.edge_pad, mid_y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(readout.label_font),
        on_section(),
    );
    // Value right — dim monospace so it reads as auxiliary info,
    // not as a clickable control.
    painter.text(
        egui::pos2(rect.max.x - readout.edge_pad, mid_y),
        egui::Align2::RIGHT_CENTER,
        value,
        egui::FontId::monospace(readout.value_font),
        on_section_dim(),
    );
    resp
}
