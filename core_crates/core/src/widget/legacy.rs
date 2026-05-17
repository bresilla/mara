//! Legacy free-function widget aliases — keeps source compatibility
//! with apps written against `maracore::widgets::*` while the new
//! pod-driven API stabilises. Each function here delegates to its
//! mara_core equivalent (or wraps a small composite) so consumers can
//! keep calling the old names verbatim.

use egui::Sense;

use crate::style::{caption, on_section, space, theme};

// ─── Caption ────────────────────────────────────────────────────────

/// Subtle caption text — italic, small, tertiary colour. Use
/// between related sub-blocks inside a section to describe what
/// follows. GAME theme prepends `// ` so the text reads as a
/// code-style annotation; PRO leaves it bare.
pub fn sub_caption(ui: &mut egui::Ui, text: &str) {
    let displayed: String = match theme().subcaption_prefix {
        Some(prefix) => format!("{}{}", prefix, text),
        None => text.to_string(),
    };
    ui.label(caption(&displayed));
}

// ─── Layout helper ──────────────────────────────────────────────────

/// Width of the label column for [`labelled_row`]. Same value the
/// legacy `maracore::widgets::row` shipped with — wide enough for
/// typical labels at body font size.
pub const LABEL_COL_WIDTH: f32 = 140.0;

/// Two-cell row: a fixed-width truncating label on the left, a
/// caller-painted right cell that right-aligns its content. Same
/// layout language as [`crate::widget::badge::badge_row`] but
/// generic over what fills the right side — toggles, drag values,
/// chip clusters, …
pub fn labelled_row(ui: &mut egui::Ui, label: &str, right: impl FnOnce(&mut egui::Ui)) {
    labelled_row_custom_left(
        ui,
        |ui| {
            ui.add(
                egui::Label::new(egui::RichText::new(label).color(on_section()).size(11.0))
                    .truncate(),
            );
        },
        right,
    );
}

/// [`labelled_row`] variant where the LEFT cell is rendered by a
/// caller-supplied closure. Used by rows that want a coloured glyph
/// in the label slot, a chip, or any composite label.
pub fn labelled_row_custom_left(
    ui: &mut egui::Ui,
    left: impl FnOnce(&mut egui::Ui),
    right: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        let row_h = ui.spacing().interact_size.y;
        ui.allocate_ui_with_layout(
            egui::vec2(LABEL_COL_WIDTH, row_h),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                left(ui);
            },
        );
        let remaining = ui.available_width().max(0.0);
        ui.allocate_ui_with_layout(
            egui::vec2(remaining, row_h),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.set_max_width(remaining);
                right(ui);
            },
        );
    });
}

// ─── Standalone widget aliases ─────────────────────────────────────
//
// These are name-only redirects to the canonical mara_core widgets
// — provided so the old `maracore::widgets::*` call sites compile
// without source edits. New code should call the canonical name
// (`button`, `readout`, `slider`, `dropdown`, `text_input`).

/// Full-width primary button — alias for [`crate::widget::button`].
pub fn wide_button(ui: &mut egui::Ui, label: &str, accent: egui::Color32) -> egui::Response {
    crate::widget::button(ui, label, accent)
}

/// Read-only "label : value" row — alias for
/// [`crate::widget::readout`].
pub fn readout_row(ui: &mut egui::Ui, label: &str, value: &str) -> egui::Response {
    crate::widget::readout(ui, label, value)
}

/// Search-styled text input. Mirrors the legacy
/// `maracore::widgets::search_field` signature — caller provides a
/// mutable buffer + placeholder + accent and gets the underlying
/// `egui::Response` back.
pub fn search_field(
    ui: &mut egui::Ui,
    buf: &mut String,
    placeholder: &str,
    accent: egui::Color32,
) -> egui::Response {
    crate::widget::text_input(ui, buf, placeholder, accent)
}

/// Pretty slider — alias for [`crate::widget::slider`]. The label,
/// suffix, and decimal count match the legacy 3-arg slider.
pub fn pretty_slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
    suffix: &str,
    accent: egui::Color32,
) -> egui::Response {
    crate::widget::slider(ui, label, value, range, decimals, suffix, accent)
}

/// Dropdown trigger — alias for [`crate::widget::dropdown`].
/// Same 5-arg shape: caller passes a hashable `id_salt` so
/// multiple dropdowns sharing the same option list don't collide
/// on persisted open/closed state.
pub fn dropdown_control<H: std::hash::Hash>(
    ui: &mut egui::Ui,
    id_salt: H,
    selected: &mut usize,
    options: &[&str],
    accent: egui::Color32,
) -> egui::Response {
    crate::widget::dropdown(ui, id_salt, selected, options, accent)
}

/// 1px hairline separator across the available row width — matches
/// the legacy `maracore::widgets::row_separator` look.
pub fn row_separator(ui: &mut egui::Ui) {
    let avail_w = ui.available_width();
    let h = 1.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(avail_w, h + space::TIGHT), Sense::hover());
    let line_y = rect.center().y;
    ui.painter().hline(
        rect.min.x..=rect.max.x,
        line_y,
        egui::Stroke::new(h, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
}

/// Mara glass key chip used inline by [`crate::widget::keybinding_row`].
/// Exposed standalone so apps can drop a single key chip into a
/// custom row layout.
pub fn key_chip(ui: &mut egui::Ui, keys: &str) -> egui::Response {
    let accent = ui.visuals().selection.stroke.color;
    let chip = egui::RichText::new(keys)
        .monospace()
        .small()
        .color(crate::style::on_track());
    let frame = crate::style::frame_for(crate::style::FrameRole::KeyChip, accent);
    frame.show(ui, |ui| ui.label(chip)).response
}
