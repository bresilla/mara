//! Plain text label rendered through Mara's backend contract.

use crate::{
    layout::{Sense, UiBackend},
    mui::MaraResponse,
    paint::PaintCmd,
    vocab::{Align2, Color32, Pos2, Vec2},
};

/// Default body label font size.
pub const LABEL_FONT: f32 = 12.0;
/// Minimum body label row height.
pub const LABEL_ROW_H: f32 = 18.0;

/// Backend-neutral label renderer.
pub fn label_backend(backend: &mut impl UiBackend, text: &str, color: Color32) -> MaraResponse {
    let available = backend.available_rect();
    let measured = backend.measure_text(text, LABEL_FONT, false);
    let width = measured.x.min(available.width().max(0.0));
    let height = measured.y.max(LABEL_ROW_H);
    let resp = backend.allocate(Vec2::new(width, height), Sense::Hover);

    backend.paint(PaintCmd::Text {
        pos: Pos2::new(resp.rect.min.x, resp.rect.center().y),
        anchor: Align2::LEFT_CENTER,
        text: text.to_owned(),
        size: LABEL_FONT,
        color,
        mono: false,
    });
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::Rect;

    use crate::backend::record::RecordingBackend;

    #[test]
    fn label_backend_allocates_measured_text_and_emits_text_command() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 40.0)),
            paints: Vec::new(),
            ..Default::default()
        };

        let response = label_backend(&mut backend, "hello", Color32::WHITE);

        assert_eq!(response.rect.width(), 30.0);
        assert_eq!(response.rect.height(), LABEL_ROW_H);
        let [PaintCmd::Text { text, mono, .. }] = backend.paints.as_slice() else {
            panic!("label should emit one proportional text command");
        };
        assert_eq!(text, "hello");
        assert!(!mono);
    }
}
