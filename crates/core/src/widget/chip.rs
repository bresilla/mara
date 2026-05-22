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

use crate::style::{
    RadiusRole, StrokeRole, contrast_text_for, font, glass_alpha_group, glass_fill, on_section,
    radius_for, stroke_for, subsection_fill, theme,
};

/// Total chip height, in px. Sits visually aligned with caption-
/// sized text rows.
pub const CHIP_H: f32 = 16.0;
/// Compact pill with a faint tinted fill + accent-tinted border.
/// Returns the `Response` so callers can attach hover tooltips or
/// react to clicks.
pub fn chip(ui: &mut egui::Ui, label: &str, accent: egui::Color32) -> egui::Response {
    let fill = glass_fill(subsection_fill(accent), accent, glass_alpha_group());
    chip_colored(ui, label, fill, accent)
}

/// Chip with an explicit fill colour. Useful for categorisation —
/// red for errors, green for OK, etc. Text colour auto-picks the
/// contrasting tone for whatever fill the caller passed (so a
/// solid `SUCCESS` chip lands black-on-green, while the default
/// faint-glass chip stays white-on-glass).
pub fn chip_colored(
    ui: &mut egui::Ui,
    label: &str,
    fill: egui::Color32,
    accent: egui::Color32,
) -> egui::Response {
    let chip = theme().widgets.chip;
    let font_id = egui::FontId::proportional(font::CAPTION);
    let text_col = if fill.a() >= 220 {
        contrast_text_for(fill)
    } else {
        on_section()
    };
    let galley = {
        let mut job = egui::text::LayoutJob::single_section(
            label.to_string(),
            egui::TextFormat::simple(font_id, text_col),
        );
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        ui.painter().layout_job(job)
    };
    let text_w = galley.size().x.ceil();
    let size = egui::vec2(text_w + chip.pad_x * 2.0, chip.height);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        ui.painter().rect(
            rect,
            radius_for(RadiusRole::Widget),
            fill,
            stroke_for(StrokeRole::WidgetBorder, accent),
            egui::StrokeKind::Inside,
        );
        ui.painter().galley(
            egui::pos2(
                rect.min.x + chip.pad_x,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            text_col,
        );
    }
    resp
}
