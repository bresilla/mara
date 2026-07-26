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
//! The picker body is drawn from paint primitives (WS-E1.3) — we
//! don't reinvent the HSV / hue / saturation controls yet; we just
//! keep the widget surface and colour data on Mara contracts.

use crate::{
    layout::{ColorPickerAlpha, InlinePickerSpec, Sense, SpaceSpec, UiBackend},
    mui::MaraResponse,
    paint::PaintCmd,
    style::{RadiusRole, StrokeRole, radius_for, stroke_for, theme},
    vocab::{Align2, Color32, Id, Pos2, Rect, Stroke, Vec2},
};

/// Swatch button height — canonical 1U row.
pub const COLOR_SWATCH_H: f32 = 20.0;

/// Labelled sRGB colour swatch with inline expansion. Returns a
/// `Response` whose `.changed()` fires whenever the picker writes
/// back to `rgb`. Each channel is normalised in `0.0..=1.0`.
pub(crate) fn color_rgb(
    ui: &mut egui::Ui,
    label: &str,
    rgb: &mut [f32; 3],
    accent: impl Into<Color32>,
) -> MaraResponse {
    let accent = accent.into();
    let id = color_picker_memory_id(crate::backend::egui::ui_id(ui), label);
    let mut open = {
        let memory = crate::backend::egui::memory_ctx_for_ui(ui);
        crate::popup::PopupState::load(&memory, id).is_open()
    };

    let preview = rgb_preview_color(rgb);
    let mut row_resp = labelled_swatch(ui, label, preview, open, accent);

    if apply_color_picker_toggle(&mut open, &row_resp) {
        let mut memory = crate::backend::egui::memory_ctx_for_ui(ui);
        crate::popup::PopupState::new(open).store(&mut memory, id);
    }

    if open {
        crate::backend::egui::add_space_for_spec(
            ui,
            SpaceSpec::vertical(theme().widgets.color.picker_gap),
        );
        let mut color32 = preview;
        let changed = picker_scope(ui, |ui| {
            let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
            crate::widget::color_picker::color_picker_backend(
                &mut backend,
                &mut color32,
                ColorPickerAlpha::Opaque,
            )
        });
        if changed {
            apply_rgb_picker_color(rgb, color32);
            row_resp.changed = true;
        }
        crate::backend::egui::add_space_for_spec(
            ui,
            SpaceSpec::vertical(theme().widgets.color.picker_gap),
        );
    }
    row_resp
}

/// Labelled sRGBA colour swatch with inline expansion. Like
/// [`color_rgb`] but exposes the alpha slider in the picker body and
/// renders the checker-over-alpha preview in the swatch.
pub(crate) fn color_rgba(
    ui: &mut egui::Ui,
    label: &str,
    rgba: &mut [f32; 4],
    accent: impl Into<Color32>,
) -> MaraResponse {
    let accent = accent.into();
    let id = color_picker_memory_id(crate::backend::egui::ui_id(ui), label);
    let mut open = {
        let memory = crate::backend::egui::memory_ctx_for_ui(ui);
        crate::popup::PopupState::load(&memory, id).is_open()
    };

    let preview = rgba_preview_color(rgba);
    let mut row_resp = labelled_swatch(ui, label, preview, open, accent);

    if apply_color_picker_toggle(&mut open, &row_resp) {
        let mut memory = crate::backend::egui::memory_ctx_for_ui(ui);
        crate::popup::PopupState::new(open).store(&mut memory, id);
    }

    if open {
        crate::backend::egui::add_space_for_spec(
            ui,
            SpaceSpec::vertical(theme().widgets.color.picker_gap),
        );
        let mut color32 = preview;
        let changed = picker_scope(ui, |ui| {
            let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
            crate::widget::color_picker::color_picker_backend(
                &mut backend,
                &mut color32,
                ColorPickerAlpha::OnlyBlend,
            )
        });
        if changed {
            apply_rgba_picker_color(rgba, color32);
            row_resp.changed = true;
        }
        crate::backend::egui::add_space_for_spec(
            ui,
            SpaceSpec::vertical(theme().widgets.color.picker_gap),
        );
    }
    row_resp
}

/// Render the row: label on the left, swatch button on the right.
/// Returns the swatch button's `Response` so the caller can react to
/// clicks (toggle the inline picker open).
fn labelled_swatch(
    ui: &mut egui::Ui,
    label: &str,
    color: Color32,
    open: bool,
    accent: Color32,
) -> MaraResponse {
    let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
    labelled_swatch_backend(&mut backend, label, color, open, accent)
}

pub fn labelled_swatch_backend(
    backend: &mut impl UiBackend,
    label: &str,
    color: Color32,
    open: bool,
    accent: Color32,
) -> MaraResponse {
    let color_theme = theme().widgets.color;
    let row_h = color_theme.row_h;
    let avail_w = backend.available_rect().width().max(0.0);
    let rect = backend
        .allocate(Vec2::new(avail_w, row_h), Sense::Hover)
        .rect;
    backend.paint(PaintCmd::Text {
        pos: Pos2::new(rect.min.x + color_theme.label_pad_l, rect.center().y),
        anchor: Align2::LEFT_CENTER,
        text: label.to_owned(),
        size: color_theme.label_font,
        color: crate::style::on_section(),
        mono: false,
    });
    let swatch_rect = Rect::from_min_size(
        Pos2::new(rect.max.x - color_theme.swatch_w, rect.min.y),
        Vec2::new(color_theme.swatch_w, row_h),
    );
    let resp = backend.interact(
        swatch_rect,
        Id::new((
            "mara_color_swatch",
            label,
            swatch_rect.min.x.to_bits(),
            swatch_rect.min.y.to_bits(),
        )),
        Sense::Click,
    );
    let border = if open || resp.hovered {
        accent
    } else {
        stroke_for(StrokeRole::WidgetBorder, accent).color
    };
    backend.paint(PaintCmd::RectFilled {
        rect: shrink_rect(swatch_rect, 1.0),
        corner: radius_for(RadiusRole::Compact),
        fill: color,
    });
    backend.paint(PaintCmd::RectStroke {
        rect: swatch_rect,
        corner: radius_for(RadiusRole::Compact),
        stroke: Stroke::new(theme().stroke.border_width, border),
    });
    resp
}

fn apply_color_picker_toggle(open: &mut bool, response: &MaraResponse) -> bool {
    if response.clicked() {
        *open = !*open;
        true
    } else {
        false
    }
}

fn color_picker_memory_id(scope: Id, label: &str) -> Id {
    scope.with(("mara_color_expand", label))
}

fn rgb_preview_color(rgb: &[f32; 3]) -> Color32 {
    Color32::from_rgb(to_u8(rgb[0]), to_u8(rgb[1]), to_u8(rgb[2]))
}

fn rgba_preview_color(rgba: &[f32; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(
        to_u8(rgba[0]),
        to_u8(rgba[1]),
        to_u8(rgba[2]),
        to_u8(rgba[3]),
    )
}

fn apply_rgb_picker_color(rgb: &mut [f32; 3], color: Color32) {
    rgb[0] = color.r() as f32 / 255.0;
    rgb[1] = color.g() as f32 / 255.0;
    rgb[2] = color.b() as f32 / 255.0;
}

fn apply_rgba_picker_color(rgba: &mut [f32; 4], color: Color32) {
    // CRITICAL: read via `to_srgba_unmultiplied` — raw channel
    // accessors on the underlying colour are premultiplied. Dividing
    // those by 255 and writing back into the user's unmultiplied rgba
    // would reduce each channel by the alpha factor every frame.
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    rgba[0] = r as f32 / 255.0;
    rgba[1] = g as f32 / 255.0;
    rgba[2] = b as f32 / 255.0;
    rgba[3] = a as f32 / 255.0;
}

/// Run a closure inside a child `Ui` whose `slider_width` has been
/// widened to the available row width, so `color_picker_color32`
/// renders at the container's width instead of the theme's compact
/// slider width. Scoping via `ui.scope` confines the override to this
/// call — other sliders in the parent ui keep their normal width.
fn picker_scope<R>(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui) -> R) -> R {
    crate::backend::egui::show_inline_picker_scope(
        ui,
        picker_scope_spec(crate::backend::egui::ui_available_width(ui)),
        content,
    )
}

fn picker_scope_spec(available_width: f32) -> InlinePickerSpec {
    // Grow the clip rect outward so the 2D picker's circular
    // indicator (whose radius scales with the picker size) doesn't
    // get sliced by the container's hard-clip when the colour sits at
    // a corner.
    InlinePickerSpec::new(available_width, (available_width / 10.0).ceil() + 4.0)
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn shrink_rect(rect: Rect, amount: f32) -> Rect {
    Rect::from_min_max(
        Pos2::new(rect.min.x + amount, rect.min.y + amount),
        Pos2::new(rect.max.x - amount, rect.max.y - amount),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MaraMemory;
    use crate::popup::PopupState;

    use crate::backend::record::{RecordingBackend, RecordingMemory};

    #[test]
    fn labelled_swatch_backend_emits_label_fill_and_border() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(180.0, COLOR_SWATCH_H)),
            paints: Vec::new(),
            ..Default::default()
        };

        let response = labelled_swatch_backend(
            &mut backend,
            "tint",
            Color32::from_rgb(10, 20, 30),
            true,
            Color32::WHITE,
        );

        assert_eq!(response.rect.width(), theme().widgets.color.swatch_w);
        assert_eq!(backend.paints.len(), 3);
        let [
            PaintCmd::Text { text: label, .. },
            PaintCmd::RectFilled { .. },
            PaintCmd::RectStroke { .. },
        ] = backend.paints.as_slice()
        else {
            panic!("color swatch should emit label, swatch fill, and swatch border");
        };
        assert_eq!(label, "tint");
    }

    #[test]
    fn color_picker_toggle_backend_flips_only_on_click() {
        let mut open = false;
        let mut clicked =
            MaraResponse::synthetic(Rect::from_min_size(Pos2::ZERO, Vec2::new(10.0, 10.0)));
        clicked.clicked = true;
        let idle = MaraResponse::synthetic(Rect::from_min_size(Pos2::ZERO, Vec2::new(10.0, 10.0)));

        assert!(apply_color_picker_toggle(&mut open, &clicked));
        assert!(open);
        assert!(!apply_color_picker_toggle(&mut open, &idle));
        assert!(open);
    }

    #[test]
    fn color_picker_open_state_uses_mara_memory_key() {
        let mut memory = RecordingMemory::default();
        let id = color_picker_memory_id(Id::new("scope"), "albedo");

        // Open-state now flows through the shared `PopupState` contract;
        // storage is unchanged (a bool under the picker's memory id).
        assert!(!PopupState::load(&memory, id).is_open());

        PopupState::new(true).store(&mut memory, id);

        assert!(PopupState::load(&memory, id).is_open());
        assert!(memory.get_temp::<bool>(id).unwrap());
    }

    #[test]
    fn color_picker_scope_spec_backend_derives_width_and_clip_margin() {
        let spec = picker_scope_spec(240.0);

        assert_eq!(spec.slider_width, 240.0);
        assert_eq!(spec.clip_expand, 28.0);
    }

    #[test]
    fn color_picker_rgb_backend_round_trips_preview_and_picker_color() {
        let mut rgb = [0.25, 0.5, 1.0];
        let preview = rgb_preview_color(&rgb);

        assert_eq!(preview, Color32::from_rgb(64, 128, 255));

        apply_rgb_picker_color(&mut rgb, Color32::from_rgb(10, 20, 30));

        assert_eq!(rgb, [10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0]);
    }

    #[test]
    fn color_picker_rgba_backend_uses_unmultiplied_channels() {
        let mut rgba = [0.5, 0.25, 0.125, 0.5];
        let preview = rgba_preview_color(&rgba);

        assert_eq!(preview.to_srgba_unmultiplied(), [128, 64, 32, 128]);

        apply_rgba_picker_color(&mut rgba, Color32::from_rgba_unmultiplied(120, 60, 30, 128));

        assert_eq!(
            rgba,
            [120.0 / 255.0, 60.0 / 255.0, 30.0 / 255.0, 128.0 / 255.0]
        );
    }
}
