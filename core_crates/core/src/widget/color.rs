//! Mara-styled colour swatch + inline picker.
//!
//! Two entry points:
//!
//! * [`color_rgb`] — opaque sRGB swatch, expands an inline HSV picker
//!   below it when clicked.
//! * [`color_rgba`] — same, but the expanded picker exposes the alpha
//!   slider and the swatch shows the alpha-over-checker preview.
//!
//! Click the swatch to toggle the picker — open state lives in egui
//! ctx data keyed off `(ui_id, label)` so every callsite remembers
//! frame-to-frame independently.
//!
//! The picker itself comes from [`egui::color_picker::color_picker_color32`]
//! — we don't reinvent the HSV / hue / saturation controls; we just
//! host them in-place rather than in a detached overlay, and widen
//! the picker's slider to the section's available width so it reads
//! at a comfortable size.

use crate::style::{RadiusRole, StrokeRole, radius_for, stroke_for, theme};

/// Swatch button height — canonical 1U row.
pub const COLOR_SWATCH_H: f32 = 20.0;

/// Labelled sRGB colour swatch with inline expansion. Returns a
/// `Response` whose `.changed()` fires whenever the picker writes
/// back to `rgb`. Each channel is normalised in `0.0..=1.0`.
pub fn color_rgb(
    ui: &mut egui::Ui,
    label: &str,
    rgb: &mut [f32; 3],
    accent: egui::Color32,
) -> egui::Response {
    let id = ui.id().with(("mara_color_expand", label));
    let mut open: bool = ui.ctx().data(|d| d.get_temp::<bool>(id).unwrap_or(false));

    let preview = egui::Color32::from_rgb(to_u8(rgb[0]), to_u8(rgb[1]), to_u8(rgb[2]));
    let mut row_resp = labelled_swatch(ui, label, preview, open, accent);

    if row_resp.clicked() {
        open = !open;
        ui.ctx().data_mut(|d| d.insert_temp(id, open));
    }

    if open {
        ui.add_space(theme().widgets.color.picker_gap);
        let mut color32 = preview;
        let changed = picker_scope(ui, |ui| {
            egui::color_picker::color_picker_color32(
                ui,
                &mut color32,
                egui::color_picker::Alpha::Opaque,
            )
        });
        if changed {
            rgb[0] = color32.r() as f32 / 255.0;
            rgb[1] = color32.g() as f32 / 255.0;
            rgb[2] = color32.b() as f32 / 255.0;
            row_resp.mark_changed();
        }
        ui.add_space(theme().widgets.color.picker_gap);
    }
    row_resp
}

/// Labelled sRGBA colour swatch with inline expansion. Like
/// [`color_rgb`] but exposes the alpha slider in the picker body and
/// renders the checker-over-alpha preview in the swatch.
pub fn color_rgba(
    ui: &mut egui::Ui,
    label: &str,
    rgba: &mut [f32; 4],
    accent: egui::Color32,
) -> egui::Response {
    let id = ui.id().with(("mara_color_expand", label));
    let mut open: bool = ui.ctx().data(|d| d.get_temp::<bool>(id).unwrap_or(false));

    let preview = egui::Color32::from_rgba_unmultiplied(
        to_u8(rgba[0]),
        to_u8(rgba[1]),
        to_u8(rgba[2]),
        to_u8(rgba[3]),
    );
    let mut row_resp = labelled_swatch(ui, label, preview, open, accent);

    if row_resp.clicked() {
        open = !open;
        ui.ctx().data_mut(|d| d.insert_temp(id, open));
    }

    if open {
        ui.add_space(theme().widgets.color.picker_gap);
        let mut color32 = preview;
        let changed = picker_scope(ui, |ui| {
            egui::color_picker::color_picker_color32(
                ui,
                &mut color32,
                egui::color_picker::Alpha::OnlyBlend,
            )
        });
        if changed {
            // CRITICAL: read via `to_srgba_unmultiplied` — the raw
            // `r()/g()/b()/a()` accessors return PREMULTIPLIED bytes.
            // Dividing those by 255 and writing back into the user's
            // unmultiplied rgba would reduce each channel by the alpha
            // factor every frame, causing the picker to "decay" on
            // each round-trip.
            let [r, g, b, a] = color32.to_srgba_unmultiplied();
            rgba[0] = r as f32 / 255.0;
            rgba[1] = g as f32 / 255.0;
            rgba[2] = b as f32 / 255.0;
            rgba[3] = a as f32 / 255.0;
            row_resp.mark_changed();
        }
        ui.add_space(theme().widgets.color.picker_gap);
    }
    row_resp
}

/// Render the row: label on the left, swatch button on the right.
/// Returns the swatch button's `Response` so the caller can react to
/// clicks (toggle the inline picker open).
fn labelled_swatch(
    ui: &mut egui::Ui,
    label: &str,
    color: egui::Color32,
    open: bool,
    accent: egui::Color32,
) -> egui::Response {
    let color_theme = theme().widgets.color;
    let row_h = color_theme.row_h;
    let avail_w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(avail_w, row_h), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.text(
        egui::pos2(rect.min.x + color_theme.label_pad_l, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(color_theme.label_font),
        crate::style::on_section(),
    );
    let swatch_rect = egui::Rect::from_min_size(
        egui::pos2(rect.max.x - color_theme.swatch_w, rect.min.y),
        egui::vec2(color_theme.swatch_w, row_h),
    );
    let resp = ui.interact(
        swatch_rect,
        ui.id().with(("mara_color_swatch", label)),
        egui::Sense::click(),
    );
    let border = if open || resp.hovered() {
        accent
    } else {
        stroke_for(StrokeRole::WidgetBorder, accent).color
    };
    egui::color_picker::show_color_at(&painter, color, swatch_rect.shrink(1.0));
    painter.rect_stroke(
        swatch_rect,
        radius_for(RadiusRole::Compact),
        egui::Stroke::new(theme().stroke.border_width, border),
        egui::epaint::StrokeKind::Inside,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Run a closure inside a child `Ui` whose `slider_width` has been
/// widened to the available row width, so `color_picker_color32`
/// renders at the container's width instead of the theme's compact
/// slider width. Scoping via `ui.scope` confines the override to this
/// call — other sliders in the parent ui keep their normal width.
fn picker_scope<R>(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let w = ui.available_width();
    ui.scope(|ui| {
        ui.spacing_mut().slider_width = w;
        // Grow the clip rect outward so the 2D picker's circular
        // indicator (whose radius scales with the picker size) doesn't
        // get sliced by the container's hard-clip when the colour
        // sits at a corner.
        let indicator_margin = (w / 10.0).ceil() + 4.0;
        let clip = ui.clip_rect().expand(indicator_margin);
        ui.set_clip_rect(clip);
        content(ui)
    })
    .inner
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}
