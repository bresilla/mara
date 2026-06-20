//! Compact pill / tag for inline status labels.
//!
//! Use these for per-row feature flags (`anim`, `var`, `inst`, …),
//! category labels in info panels, or any "small chunk of text
//! that belongs next to another thing". They sit inline — call
//! from inside `ui.horizontal(|ui| { ... })` or
//! `ui.horizontal_wrapped(|ui| { ... })` to chain them.
//!
//! Two variants ship:
//!
//! * [`chip`] — faint accent-tinted fill + accent-tinted border.
//!   The default, neutral "there's a property here" chip.
//! * [`chip_colored`] — caller supplies a fill colour (e.g.
//!   [`SUCCESS`](crate::style::SUCCESS) /
//!   [`WARNING`](crate::style::WARNING) /
//!   [`DANGER`](crate::style::DANGER)) for chips that categorise
//!   (status, severity). Border still uses `widget_border(accent)`
//!   so the family remains coherent.

use crate::{
    layout::{Sense, UiBackend},
    mui::MaraResponse,
    paint::PaintCmd,
    style::{
        RadiusRole, StrokeRole, contrast_text_for, font, glass_alpha_group, glass_fill, on_section,
        radius_for, stroke_for, subsection_fill, theme,
    },
    vocab::{Align2, Color32, Pos2},
};

/// Total chip height, in px. Sits visually aligned with caption-
/// sized text rows.
pub const CHIP_H: f32 = 16.0;

/// Default faint-glass chip fill derived from the ambient accent.
#[must_use]
pub fn chip_fill(accent: impl Into<Color32>) -> Color32 {
    let accent = accent.into();
    glass_fill(subsection_fill(accent), accent, glass_alpha_group())
}

/// Backend-neutral chip renderer.
pub fn chip_colored_backend(
    backend: &mut impl UiBackend,
    label: &str,
    fill: Color32,
    accent: Color32,
) -> MaraResponse {
    let chip = theme().widgets.chip;
    let text_col = if fill.a() >= 220 {
        contrast_text_for(fill)
    } else {
        on_section()
    };
    let text_size = backend.measure_text(label, font::CAPTION, false);
    let resp = backend.allocate(
        crate::vocab::Vec2::new(text_size.x.ceil() + chip.pad_x * 2.0, chip.height),
        Sense::Click,
    );
    backend.paint(PaintCmd::RectFilled {
        rect: resp.rect,
        corner: radius_for(RadiusRole::Widget),
        fill,
    });
    backend.paint(PaintCmd::RectStroke {
        rect: resp.rect,
        corner: radius_for(RadiusRole::Widget),
        stroke: stroke_for(StrokeRole::WidgetBorder, accent),
    });
    backend.paint(PaintCmd::Text {
        pos: Pos2::new(resp.rect.min.x + chip.pad_x, resp.rect.center().y),
        anchor: Align2::LEFT_CENTER,
        text: label.to_owned(),
        size: font::CAPTION,
        color: text_col,
        mono: false,
    });
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::{Id, Rect, Vec2};

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
    fn chip_backend_emits_fill_stroke_and_text_commands() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, CHIP_H)),
            paints: Vec::new(),
        };

        let response = chip_colored_backend(
            &mut backend,
            "sealed",
            Color32::from_rgb(20, 30, 40),
            Color32::WHITE,
        );

        assert!(response.rect.width() > 0.0);
        assert_eq!(backend.paints.len(), 3);
        let [
            PaintCmd::RectFilled { .. },
            PaintCmd::RectStroke { .. },
            PaintCmd::Text { text, .. },
        ] = backend.paints.as_slice()
        else {
            panic!("chip should emit fill, stroke and label text commands");
        };
        assert_eq!(text, "sealed");
    }

    #[test]
    fn chip_fill_stays_in_mara_color_vocabulary() {
        let fill = chip_fill(Color32::from_rgb(64, 128, 255));

        assert!(fill.a() > 0);
    }
}
