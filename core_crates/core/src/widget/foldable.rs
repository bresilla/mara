//! Standalone foldable section — a chevron-on-header collapsible
//! frame with a mara-glass background. Used outside of the
//! pane/container system as a tiered group inside ad-hoc panels
//! (e.g. nested variant browsers, debug overlays).
//!
//! The pane/container path uses [`crate::container::Normal`]
//! instead — this helper is a thin shim for callers that just want
//! "an egui collapsible with mara paint".

use egui::collapsing_header::CollapsingState;

use crate::style::{FrameRole, frame_for, on_section, section_caps};

/// Render a mara-styled collapsible section. `id_salt` makes the
/// section's open/closed state distinct from siblings; `title` is
/// the header label; `default_open` is the initial state on first
/// paint.
pub fn section(
    ui: &mut egui::Ui,
    id_salt: &str,
    title: &str,
    accent: egui::Color32,
    default_open: bool,
    body: impl FnOnce(&mut egui::Ui),
) {
    let id = ui.id().with(("mara_section", id_salt));
    let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, default_open);

    let frame = frame_for(FrameRole::Section, accent);

    frame.show(ui, |ui| {
        // Header — chevron + UPPERCASE title, click-toggles open.
        let header_resp = ui
            .horizontal(|ui| {
                let openness = state.openness(ui.ctx());
                let chevron = if openness > 0.5 { "▾" } else { "▸" };
                let chevron_resp = ui.add(
                    egui::Label::new(egui::RichText::new(chevron).color(on_section()).size(11.0))
                        .sense(egui::Sense::click()),
                );
                let title_resp = ui
                    .add(egui::Label::new(section_caps(title, accent)).sense(egui::Sense::click()));
                chevron_resp.union(title_resp)
            })
            .inner;
        if header_resp.clicked() {
            state.toggle(ui);
        }
        state.show_body_indented(&header_resp, ui, |ui| {
            body(ui);
        });
    });
}
