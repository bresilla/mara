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

/// How a styled label should render — PLAN.md WS-A6.
///
/// The sealed replacement for reaching at the backend's rich-text
/// builder. `mara_graph`'s node headers, and any app drawing a title
/// or a dimmed caption, need size and colour on a label that still
/// participates in layout; [`label_backend`] only offers the body font.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelSpec {
    pub size: f32,
    pub color: Color32,
    /// Render in the monospace family rather than the proportional one.
    pub mono: bool,
    /// Truncate to the available width instead of overflowing it.
    /// Layout still reserves only what fits, so a long label cannot
    /// push siblings out of the row.
    pub truncate: bool,
}

impl LabelSpec {
    #[must_use]
    pub fn new(size: f32, color: impl Into<Color32>) -> Self {
        Self {
            size,
            color: color.into(),
            mono: false,
            truncate: true,
        }
    }

    #[must_use]
    pub fn mono(mut self, mono: bool) -> Self {
        self.mono = mono;
        self
    }

    #[must_use]
    pub fn truncate(mut self, truncate: bool) -> Self {
        self.truncate = truncate;
        self
    }
}

/// Label with an explicit size, colour and family.
pub fn label_spec_backend(
    backend: &mut dyn UiBackend,
    text: &str,
    spec: &LabelSpec,
) -> MaraResponse {
    let available = backend.available_rect();
    let measured = backend.measure_text(text, spec.size, spec.mono);
    let width = if spec.truncate {
        measured.x.min(available.width().max(0.0))
    } else {
        measured.x
    };
    let height = measured.y.max(spec.size);
    let resp = backend.allocate(Vec2::new(width, height), Sense::Hover);

    backend.paint(PaintCmd::Text {
        pos: Pos2::new(resp.rect.min.x, resp.rect.center().y),
        anchor: Align2::LEFT_CENTER,
        text: text.to_owned(),
        size: spec.size,
        color: spec.color,
        mono: spec.mono,
    });
    resp
}

#[cfg(test)]
mod spec_tests {
    use super::*;
    use crate::backend::record::RecordingBackend;
    use crate::vocab::Rect;

    fn backend() -> RecordingBackend {
        RecordingBackend::at(Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 60.0)))
    }

    #[test]
    fn spec_drives_size_colour_and_family() {
        let mut b = backend();
        let spec = LabelSpec::new(22.0, Color32::from_rgb(9, 8, 7)).mono(true);
        let _ = label_spec_backend(&mut b, "title", &spec);

        match b.paints.as_slice() {
            [
                PaintCmd::Text {
                    size, color, mono, ..
                },
            ] => {
                assert_eq!(*size, 22.0);
                assert_eq!(*color, Color32::from_rgb(9, 8, 7));
                assert!(*mono);
            }
            other => panic!("expected one Text command, got {other:#?}"),
        }
    }

    /// A styled label must not be able to push its siblings out of a
    /// row — that is what `truncate` protects, and it is the difference
    /// between a header that fits and one that breaks the node layout.
    #[test]
    fn truncation_bounds_the_allocation_to_available_width() {
        let long = "an extremely long node title that will not fit at all";

        let mut truncating = backend();
        let clipped =
            label_spec_backend(&mut truncating, long, &LabelSpec::new(20.0, Color32::WHITE));
        assert!(clipped.rect.width() <= 200.0);

        let mut overflowing = backend();
        let full = label_spec_backend(
            &mut overflowing,
            long,
            &LabelSpec::new(20.0, Color32::WHITE).truncate(false),
        );
        assert!(
            full.rect.width() > clipped.rect.width(),
            "opting out of truncation keeps the full measured width"
        );
    }

    #[test]
    fn height_never_collapses_below_the_font_size() {
        let mut b = backend();
        let resp = label_spec_backend(&mut b, "", &LabelSpec::new(30.0, Color32::WHITE));
        assert!(resp.rect.height() >= 30.0);
    }
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
