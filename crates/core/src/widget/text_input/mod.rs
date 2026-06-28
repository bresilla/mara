//! Mara-styled single-line text input.
//!
//! Shape:
//!
//! ```text
//!   🔍  query text…                    ✕
//!   └── leading glyph       trailing clear button
//! ```
//!
//! A thin wrapper around Mara text-edit layout/policy data, currently
//! rendered by the egui backend's single-line editor adapter, with:
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
//! Separator chrome belongs to `Pod`, not to the text field.

use egui;

use crate::{
    layout::{Sense, TextEditRegion, TextEditSpec, UiBackend},
    mui::MaraResponse,
    paint::PaintCmd,
    style,
    vocab::{Align2, Color32, Id, Pos2, Rect, Vec2},
};

/// Width of the leading / trailing glyph columns.
const GLYPH_W: f32 = 18.0;
/// Padding between the text and the glyph columns.
const TEXT_PAD: f32 = 4.0;

/// Render a text input at the canonical 1U height
/// ([`crate::style::UNIT`]). See [`text_input_h`] for the
/// variable-height variant used by resizable pods.
pub(crate) fn text_input(
    ui: &mut egui::Ui,
    text: &mut String,
    placeholder: &str,
    accent: impl Into<Color32>,
) -> MaraResponse {
    text_input_h(ui, text, placeholder, accent, crate::style::UNIT)
}

/// Render a text input bound to `text` at the requested `height`.
/// `placeholder` shows as a dim hint when the buffer is empty.
/// Returns the inner `TextEdit`'s `Response` (with `.rect` extended
/// to cover the full field — leading icon + text strip + clear
/// button); the clear-button click marks it `.changed()` too.
pub(crate) fn text_input_h(
    ui: &mut egui::Ui,
    text: &mut String,
    placeholder: &str,
    accent: impl Into<Color32>,
    height: f32,
) -> MaraResponse {
    let accent = accent.into();
    let chrome = {
        let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
        text_input_chrome_backend(&mut backend, !text.is_empty(), accent, height)
    };
    let mut cleared = false;
    if chrome
        .clear_response
        .as_ref()
        .is_some_and(MaraResponse::clicked)
    {
        text.clear();
        cleared = true;
    }

    // Dim the typed text to 50 % alpha so the field reads as a soft
    // input rather than punching out at full contrast.
    let dim_text = {
        let base = style::on_track();
        Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 128)
    };
    let text_spec = TextEditSpec::singleline(
        TextEditRegion::new(chrome.rect, chrome.text_rect, style::BODY_FONT_SIZE),
        placeholder,
        dim_text,
        style::on_track_dim(),
    );
    let edit_output = crate::backend::egui::show_singleline_text_edit_for_spec(ui, text, text_spec);
    // Extend the response's rect from the inner TextEdit to the
    // full painted field — the leading magnifier glyph, the trailing
    // clear button, and the rounded glass border are all PART of
    // the widget the caller sees, so callers / inspectors should
    // receive the full field rect, not just the carved-out text
    // strip in the middle.
    let mut response = edit_output.response;
    response.rect = chrome.rect;
    if cleared {
        response.changed = true;
    }
    response
}

#[derive(Clone, Debug)]
pub struct TextInputChrome {
    pub rect: Rect,
    pub text_rect: Rect,
    pub clear_response: Option<MaraResponse>,
}

pub fn text_input_chrome_backend(
    backend: &mut impl UiBackend,
    has_text: bool,
    accent: Color32,
    height: f32,
) -> TextInputChrome {
    let w = backend.available_rect().width().max(0.0);
    let rect = backend.allocate(Vec2::new(w, height), Sense::Hover).rect;

    backend.paint(PaintCmd::RectFilled {
        rect,
        corner: style::radius_for(style::RadiusRole::Widget),
        fill: style::fill_for(style::FillRole::Track, accent),
    });
    backend.paint(PaintCmd::RectStroke {
        rect,
        corner: style::radius_for(style::RadiusRole::Widget),
        stroke: style::stroke_for(style::StrokeRole::WidgetBorder, accent),
    });

    let mid_y = rect.center().y;
    backend.paint(PaintCmd::Text {
        pos: Pos2::new(rect.min.x + GLYPH_W * 0.5, mid_y),
        anchor: Align2::CENTER_CENTER,
        text: "⌕".to_owned(),
        size: 13.0,
        color: style::on_track_dim(),
        mono: false,
    });

    let clear_rect = Rect::from_min_size(
        Pos2::new(rect.max.x - GLYPH_W, rect.min.y),
        Vec2::new(GLYPH_W, height),
    );
    let clear_response = if has_text {
        let response = backend.interact(
            clear_rect,
            Id::new((
                "mara_text_input_clear",
                clear_rect.min.x.to_bits(),
                clear_rect.min.y.to_bits(),
            )),
            Sense::Click,
        );
        let color = if response.hovered {
            accent
        } else {
            style::on_track_dim()
        };
        backend.paint(PaintCmd::Text {
            pos: clear_rect.center(),
            anchor: Align2::CENTER_CENTER,
            text: "×".to_owned(),
            size: 13.0,
            color,
            mono: false,
        });
        Some(response)
    } else {
        None
    };

    let text_rect = Rect::from_min_max(
        Pos2::new(rect.min.x + GLYPH_W + TEXT_PAD, rect.min.y),
        Pos2::new(rect.max.x - GLYPH_W - TEXT_PAD, rect.max.y),
    );

    TextInputChrome {
        rect,
        text_rect,
        clear_response,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingBackend {
        available: Rect,
        paints: Vec<PaintCmd>,
        interaction: Option<MaraResponse>,
    }

    impl UiBackend for RecordingBackend {
        fn begin_area(&mut self, _host: crate::layout::AreaHost, rect: Rect) {
            self.available = rect;
        }

        fn allocate(&mut self, size: Vec2, _sense: Sense) -> MaraResponse {
            MaraResponse::synthetic(Rect::from_min_size(self.available.min, size))
        }

        fn interact(&mut self, rect: Rect, _id: Id, _sense: Sense) -> MaraResponse {
            self.interaction
                .clone()
                .unwrap_or_else(|| MaraResponse::synthetic(rect))
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
    fn text_input_chrome_backend_emits_field_search_and_clear() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(240.0, crate::style::UNIT)),
            paints: Vec::new(),
            interaction: None,
        };

        let chrome =
            text_input_chrome_backend(&mut backend, true, Color32::WHITE, crate::style::UNIT);

        assert_eq!(chrome.rect.width(), 240.0);
        assert!(chrome.clear_response.is_some());
        assert_eq!(backend.paints.len(), 4);
        let [
            PaintCmd::RectFilled { .. },
            PaintCmd::RectStroke { .. },
            PaintCmd::Text { text: search, .. },
            PaintCmd::Text { text: clear, .. },
        ] = backend.paints.as_slice()
        else {
            panic!("text input chrome should emit field chrome plus search and clear glyphs");
        };
        assert_eq!(search, "⌕");
        assert_eq!(clear, "×");
    }

    #[test]
    fn text_input_chrome_backend_omits_clear_for_empty_text() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(240.0, crate::style::UNIT)),
            paints: Vec::new(),
            interaction: None,
        };

        let chrome =
            text_input_chrome_backend(&mut backend, false, Color32::WHITE, crate::style::UNIT);

        assert!(chrome.clear_response.is_none());
        assert_eq!(backend.paints.len(), 3);
    }
}
