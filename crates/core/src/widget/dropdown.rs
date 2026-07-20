//! Mara-styled dropdown — a single-select trigger with a popup list.
//!
//! Shape:
//!
//! ```text
//!   [  Selected option            ▾  ]
//!   └── trigger ─────────────────────┘
//!                                       ┌───────────────┐
//!                                       │  option a     │  ← popup
//!                                       │  option b ✓   │
//!                                       │  option c     │
//!                                       └───────────────┘
//! ```
//!
//! `selected` is the index into `options`. Clicking an option writes
//! the new index to `*selected` and the returned `Response` reports
//! `.changed() == true` for that frame.
//!
//! Two entry points:
//!
//! * [`dropdown`] — render at the canonical 1U height.
//! * [`dropdown_h`] — same, with caller-supplied height (used by
//!   resizable pods so the trigger grows with its slot).

use std::hash::Hash;

use crate::{
    layout::{PopupAlign, PopupListSpec, PopupSpec, PopupTrigger, Sense, UiBackend},
    mui::MaraResponse,
    paint::PaintCmd,
    style::{
        FillRole, FrameRole, RadiusRole, StrokeRole, fill_for, frame_for, glass_alpha_card,
        on_section, on_track, on_track_dim, radius_for, row_hover_fill, row_selected_fill,
        stroke_for, surface_lift_target, theme,
    },
    vocab::{Align2, Color32, Id, Pos2, Rect, Stroke, Vec2},
};

/// Default trigger height — the canonical 1U row used elsewhere in
/// the kit.
pub const DROPDOWN_ROW_H: f32 = crate::style::UNIT;

/// Render a dropdown at the default [`DROPDOWN_ROW_H`] height.
/// `id_salt` disambiguates this dropdown's popup id from siblings in
/// the same `Ui` (a string, an enum value, an index — anything
/// hashable).
pub(crate) fn dropdown(
    ui: &mut egui::Ui,
    id_salt: impl Hash,
    selected: &mut usize,
    options: &[&str],
    accent: impl Into<Color32>,
) -> MaraResponse {
    let accent = accent.into();
    dropdown_h(
        ui,
        id_salt,
        selected,
        options,
        accent,
        theme().widgets.dropdown.row_h,
    )
}

/// Variable-height variant. Used by resizable pods.
pub(crate) fn dropdown_h(
    ui: &mut egui::Ui,
    id_salt: impl Hash,
    selected: &mut usize,
    options: &[&str],
    accent: impl Into<Color32>,
    height: f32,
) -> MaraResponse {
    let accent = accent.into();
    let display = options.get(*selected).copied().unwrap_or("—");
    let mut resp = {
        let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
        dropdown_trigger_backend(&mut backend, display, accent, height)
    };

    // Stable id for the popup so its open-state survives across
    // frames. `id_salt` disambiguates sibling dropdowns.
    let popup_id = crate::backend::egui::ui_id(ui).with(("mara_dropdown", &id_salt));
    let trigger = dropdown_popup_trigger(resp.backend_response_id(), popup_id);
    let Some(resp_with_id) = crate::backend::egui::popup_toggle_response_for_ui(ui, trigger) else {
        return resp;
    };

    // Popup open-state is owned by Mara's `PopupState` through
    // `MaraMemory`, not egui's internal popup memory. We toggle on the
    // trigger click (the same condition egui used) and let egui apply
    // its anchoring + click-outside/Escape dismissal into the bool.
    let mut open = {
        let memory = crate::backend::egui::memory_ctx_for_ui(ui);
        crate::popup::PopupState::load(&memory, popup_id).is_open()
    };
    if resp.clicked() {
        open = !open;
    }

    let popup_spec = dropdown_popup_spec(resp.rect.width());

    let mut changed = false;
    if let Some(inner) = crate::backend::egui::show_popup_open_bool(
        &resp_with_id,
        &mut open,
        popup_spec,
        frame_for(FrameRole::Popup, accent),
        |ui| {
            crate::backend::egui::apply_popup_list_spec(ui, dropdown_popup_list_spec());
            for (idx, opt) in options.iter().enumerate() {
                let is_selected = *selected == idx;
                let row_resp = {
                    let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
                    dropdown_popup_row_backend(&mut backend, opt, is_selected, accent)
                };
                changed |= apply_dropdown_popup_pick(selected, idx, &row_resp);
            }
        },
    ) {
        drop(inner);
    }

    // Persist the (possibly egui-dismissed) open-state back into Mara
    // memory for the next frame.
    {
        let mut memory = crate::backend::egui::memory_ctx_for_ui(ui);
        crate::popup::PopupState::new(open).store(&mut memory, popup_id);
    }

    if changed {
        resp.changed = true;
    }
    resp
}

fn dropdown_popup_spec(width: f32) -> PopupSpec {
    let dropdown = theme().widgets.dropdown;
    PopupSpec::new(
        PopupAlign::BottomStart,
        dropdown.popup_gap,
        width,
        dropdown.popup_inner_margin,
    )
}

fn dropdown_popup_trigger(response_id: Id, popup_id: Id) -> PopupTrigger {
    PopupTrigger::new(response_id, popup_id)
}

fn dropdown_popup_list_spec() -> PopupListSpec {
    PopupListSpec::new(Vec2::new(0.0, theme().widgets.dropdown.item_spacing_y))
}

fn apply_dropdown_popup_pick(
    selected: &mut usize,
    idx: usize,
    row_response: &MaraResponse,
) -> bool {
    if row_response.clicked() && *selected != idx {
        *selected = idx;
        true
    } else {
        false
    }
}

pub fn dropdown_trigger_backend(
    backend: &mut impl UiBackend,
    display: &str,
    accent: Color32,
    height: f32,
) -> MaraResponse {
    let w = backend.available_rect().width().max(0.0);
    let resp = backend.allocate(Vec2::new(w, height), Sense::Click);
    paint_trigger_backend(backend, resp.rect, display, resp.hovered, accent);
    resp
}

pub fn dropdown_popup_row_backend(
    backend: &mut impl UiBackend,
    label: &str,
    selected: bool,
    accent: Color32,
) -> MaraResponse {
    let dropdown = theme().widgets.dropdown;
    let w = backend.available_rect().width().max(0.0);
    let resp = backend.allocate(Vec2::new(w, dropdown.item_h), Sense::Click);
    paint_popup_row_backend(backend, resp.rect, label, selected, resp.hovered, accent);
    resp
}

fn paint_trigger_backend(
    backend: &mut impl UiBackend,
    rect: Rect,
    display: &str,
    hovered: bool,
    accent: Color32,
) {
    let th = theme();
    let dropdown = th.widgets.dropdown;
    let tint = if hovered {
        dropdown.tint_hover
    } else {
        dropdown.tint_rest
    };
    let solid = lerp_col(
        fill_for(FillRole::Track, accent),
        surface_lift_target(accent),
        tint,
    );
    let bg = Color32::from_rgba_unmultiplied(solid.r(), solid.g(), solid.b(), glass_alpha_card());
    let border = if hovered {
        Stroke::new(th.stroke.border_width, accent)
    } else {
        stroke_for(StrokeRole::WidgetBorder, accent)
    };
    backend.paint(PaintCmd::RectFilled {
        rect,
        corner: radius_for(RadiusRole::Widget),
        fill: bg,
    });
    backend.paint(PaintCmd::RectStroke {
        rect,
        corner: radius_for(RadiusRole::Widget),
        stroke: border,
    });

    let text_rect = Rect::from_min_max(
        Pos2::new(rect.min.x + dropdown.pad_x, rect.min.y),
        Pos2::new(rect.max.x - dropdown.chevron_w - dropdown.pad_x, rect.max.y),
    );
    backend.push_clip(text_rect);
    backend.paint(PaintCmd::Text {
        pos: Pos2::new(text_rect.min.x, text_rect.center().y),
        anchor: Align2::LEFT_CENTER,
        text: display.to_owned(),
        size: dropdown.text_font,
        color: on_track(),
        mono: false,
    });
    backend.pop_clip();

    let chev_color = if hovered { accent } else { on_track_dim() };
    backend.paint(PaintCmd::Text {
        pos: Pos2::new(
            rect.max.x - dropdown.pad_x - dropdown.chevron_w * 0.5,
            rect.center().y,
        ),
        anchor: Align2::CENTER_CENTER,
        text: "▾".to_owned(),
        size: dropdown.icon_size,
        color: chev_color,
        mono: false,
    });
}

fn paint_popup_row_backend(
    backend: &mut impl UiBackend,
    rect: Rect,
    label: &str,
    selected: bool,
    hovered: bool,
    accent: Color32,
) {
    let dropdown = theme().widgets.dropdown;
    let bg = if selected {
        Some(row_selected_fill(accent))
    } else if hovered {
        Some(row_hover_fill(accent))
    } else {
        None
    };
    if let Some(fill) = bg {
        backend.paint(PaintCmd::RectFilled {
            rect,
            corner: radius_for(RadiusRole::Compact),
            fill,
        });
    }
    backend.paint(PaintCmd::Text {
        pos: Pos2::new(rect.min.x + dropdown.pad_x, rect.center().y),
        anchor: Align2::LEFT_CENTER,
        text: label.to_owned(),
        size: dropdown.text_font,
        color: on_section(),
        mono: false,
    });
}

fn lerp_col(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let blend = |x: u8, y: u8| ((x as f32) * (1.0 - t) + (y as f32) * t).round() as u8;
    Color32::from_rgb(
        blend(a.r(), b.r()),
        blend(a.g(), b.g()),
        blend(a.b(), b.b()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::Id;

    use crate::backend::record::RecordingBackend;

    #[test]
    fn dropdown_trigger_backend_emits_chrome_selected_text_and_chevron() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(220.0, DROPDOWN_ROW_H)),
            paints: Vec::new(),
            clips: Vec::new(),
            interaction: None,
            ..Default::default()
        };

        let response =
            dropdown_trigger_backend(&mut backend, "Selected", Color32::WHITE, DROPDOWN_ROW_H);

        assert_eq!(response.rect.width(), 220.0);
        assert_eq!(backend.clips.len(), 1);
        assert_eq!(backend.paints.len(), 4);
        let [
            PaintCmd::RectFilled { .. },
            PaintCmd::RectStroke { .. },
            PaintCmd::Text { text: selected, .. },
            PaintCmd::Text { text: chevron, .. },
        ] = backend.paints.as_slice()
        else {
            panic!("dropdown trigger should emit chrome, selected text, and chevron");
        };
        assert_eq!(selected, "Selected");
        assert_eq!(chevron, "▾");
    }

    #[test]
    fn dropdown_popup_row_backend_emits_selected_bg_and_label() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(220.0, DROPDOWN_ROW_H)),
            paints: Vec::new(),
            clips: Vec::new(),
            interaction: None,
            ..Default::default()
        };

        let response = dropdown_popup_row_backend(&mut backend, "Option A", true, Color32::WHITE);

        assert_eq!(response.rect.width(), 220.0);
        assert_eq!(backend.paints.len(), 2);
        let [
            PaintCmd::RectFilled { .. },
            PaintCmd::Text { text: label, .. },
        ] = backend.paints.as_slice()
        else {
            panic!("selected dropdown popup row should emit selected bg and label");
        };
        assert_eq!(label, "Option A");
    }

    #[test]
    fn dropdown_popup_spec_backend_carries_anchor_policy() {
        let spec = dropdown_popup_spec(220.0);

        assert_eq!(spec.align, PopupAlign::BottomStart);
        assert_eq!(spec.gap, theme().widgets.dropdown.popup_gap);
        assert_eq!(spec.width, 220.0);
        assert_eq!(
            spec.inner_margin,
            theme().widgets.dropdown.popup_inner_margin
        );
    }

    #[test]
    fn dropdown_popup_trigger_backend_carries_response_and_popup_ids() {
        let trigger = dropdown_popup_trigger(Id::new("response"), Id::new("popup"));

        assert_eq!(trigger.response_id, Id::new("response"));
        assert_eq!(trigger.popup_id, Id::new("popup"));
    }

    #[test]
    fn dropdown_popup_list_spec_backend_carries_row_spacing() {
        let spec = dropdown_popup_list_spec();

        assert_eq!(
            spec.item_spacing,
            Vec2::new(0.0, theme().widgets.dropdown.item_spacing_y)
        );
    }

    #[test]
    fn dropdown_popup_pick_backend_updates_selection_only_on_new_click() {
        let mut selected = 0;
        let mut clicked =
            MaraResponse::synthetic(Rect::from_min_size(Pos2::ZERO, Vec2::new(10.0, 10.0)));
        clicked.clicked = true;
        let idle = MaraResponse::synthetic(Rect::from_min_size(Pos2::ZERO, Vec2::new(10.0, 10.0)));

        assert!(apply_dropdown_popup_pick(&mut selected, 2, &clicked));
        assert_eq!(selected, 2);

        assert!(!apply_dropdown_popup_pick(&mut selected, 2, &clicked));
        assert_eq!(selected, 2);

        assert!(!apply_dropdown_popup_pick(&mut selected, 1, &idle));
        assert_eq!(selected, 2);
    }
}
