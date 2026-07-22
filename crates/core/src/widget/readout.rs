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

use crate::{
    layout::{Sense, UiBackend},
    mui::MaraResponse,
    paint::PaintCmd,
    style::{UNIT, on_section, on_section_dim, theme},
    vocab::{Align2, Pos2, Rect, Vec2},
};

/// Default readout row height — the canonical 1U.
pub const READOUT_ROW_H: f32 = UNIT;

/// Backend-neutral readout row renderer.
pub fn readout_backend(
    backend: &mut impl UiBackend,
    label: &str,
    value: &str,
    height: f32,
) -> MaraResponse {
    let avail_w = backend.available_rect().width().max(0.0);
    let resp = backend.allocate(Vec2::new(avail_w, height), Sense::Hover);
    paint_readout(backend, resp.rect, label, value);
    resp
}

fn paint_readout(backend: &mut impl UiBackend, rect: Rect, label: &str, value: &str) {
    let readout = theme().widgets.readout;
    let mid_y = rect.center().y;
    // Label left — full-contrast `on_section`.
    backend.paint(PaintCmd::Text {
        pos: Pos2::new(rect.min.x + readout.edge_pad, mid_y),
        anchor: Align2::LEFT_CENTER,
        text: label.to_owned(),
        size: readout.label_font,
        color: on_section(),
        mono: false,
    });
    // Value right — dim monospace so it reads as auxiliary info,
    // not as a clickable control.
    backend.paint(PaintCmd::Text {
        pos: Pos2::new(rect.max.x - readout.edge_pad, mid_y),
        anchor: Align2::RIGHT_CENTER,
        text: value.to_owned(),
        size: readout.value_font,
        color: on_section_dim(),
        mono: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::backend::record::RecordingBackend;

    #[test]
    fn readout_backend_emits_label_and_value_text_commands() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(180.0, READOUT_ROW_H)),
            paints: Vec::new(),
            ..Default::default()
        };

        let response = readout_backend(&mut backend, "status", "ok", READOUT_ROW_H);

        assert_eq!(response.rect.width(), 180.0);
        assert_eq!(backend.paints.len(), 2);
        let [
            PaintCmd::Text {
                text: label,
                mono: false,
                ..
            },
            PaintCmd::Text {
                text: value,
                mono: true,
                ..
            },
        ] = backend.paints.as_slice()
        else {
            panic!("readout should emit proportional label and monospace value text");
        };
        assert_eq!(label, "status");
        assert_eq!(value, "ok");
    }
}
