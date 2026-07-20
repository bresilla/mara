//! Standalone foldable section — a chevron-on-header collapsible
//! frame with a mara-glass background. Used outside of the
//! pane/container system as a tiered group inside ad-hoc panels
//! (e.g. nested variant browsers, debug overlays).
//!
//! The pane/container path uses [`crate::container::Normal`]
//! instead. The body is still hosted by the current egui `Ui`, but
//! disclosure state and header chrome now go through Mara memory and
//! backend contracts so future non-egui backends have the same row
//! vocabulary/state model to implement.

use crate::{
    layout::{IndentedBodySpec, Sense, UiBackend},
    memory::MaraMemory,
    mui::MaraResponse,
    paint::PaintCmd,
    style::{FrameRole, frame_for, on_section, theme},
    vocab::{Align2, Color32, Id, Pos2, Vec2},
};

/// Render a mara-styled collapsible section. `id_salt` makes the
/// section's open/closed state distinct from siblings; `title` is
/// the header label; `default_open` is the initial state on first
/// paint.
pub(crate) fn section(
    ui: &mut egui::Ui,
    id_salt: &str,
    title: &str,
    accent: impl Into<Color32>,
    default_open: bool,
    body: impl FnOnce(&mut egui::Ui),
) {
    let accent = accent.into();
    let id = section_memory_id(crate::backend::egui::ui_id(ui), id_salt);

    let frame = frame_for(FrameRole::Section, accent);

    crate::backend::egui::egui_frame_for_style_spec(frame).show(ui, |ui| {
        let mut open = {
            let memory = crate::backend::egui::memory_ctx_for_ui(ui);
            section_open(&memory, id, default_open)
        };
        let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
        let header_resp = section_header_backend(&mut backend, title, open, accent);

        if apply_section_toggle(&mut open, &header_resp) {
            let mut memory = crate::backend::egui::memory_ctx_for_ui(ui);
            set_section_open(&mut memory, id, open);
        }

        if let Some(spec) = section_body_spec(id, open) {
            crate::backend::egui::show_indented_body_for_spec(ui, spec, |ui| body(ui));
        }
    });
}

/// Backend-neutral foldable-section header. It intentionally covers
/// only the interactive header row; [`section`] owns the disclosure
/// state through Mara memory while the body is still hosted by the
/// current egui backend.
pub fn section_header_backend(
    backend: &mut impl UiBackend,
    title: &str,
    open: bool,
    accent: Color32,
) -> MaraResponse {
    let th = theme();
    let available = backend.available_rect();
    let height = th
        .container
        .title_zone_thickness
        .max(th.container.title_size + 4.0);
    let response = backend.allocate(Vec2::new(available.width().max(0.0), height), Sense::Click);
    let chevron = if open { "▾" } else { "▸" };
    let chevron_x = response.rect.min.x;
    let text_x = chevron_x + 14.0;
    let center_y = response.rect.center().y;

    backend.paint(PaintCmd::Text {
        pos: Pos2::new(chevron_x, center_y),
        anchor: Align2::LEFT_CENTER,
        text: chevron.to_owned(),
        size: th.container.title_size,
        color: on_section(),
        mono: false,
    });
    backend.paint(PaintCmd::Text {
        pos: Pos2::new(text_x, center_y),
        anchor: Align2::LEFT_CENTER,
        text: title.to_uppercase(),
        size: th.container.title_size,
        color: accent,
        mono: false,
    });
    response
}

fn section_memory_id(scope: Id, id_salt: &str) -> Id {
    scope.with(("mara_section", id_salt))
}

fn section_open(memory: &impl MaraMemory, id: Id, default_open: bool) -> bool {
    memory.get_persisted::<bool>(id).unwrap_or(default_open)
}

fn set_section_open(memory: &mut impl MaraMemory, id: Id, open: bool) {
    memory.set_persisted(id, open);
}

fn apply_section_toggle(open: &mut bool, response: &MaraResponse) -> bool {
    if response.clicked() {
        *open = !*open;
        true
    } else {
        false
    }
}

fn section_body_spec(id: Id, open: bool) -> Option<IndentedBodySpec> {
    open.then(|| IndentedBodySpec::new(id.with("body")))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::vocab::Rect;

    use crate::backend::record::{RecordingBackend, RecordingMemory};

    #[test]
    fn section_header_backend_emits_chevron_and_caps_title() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(160.0, 24.0)),
            paints: Vec::new(),
            ..Default::default()
        };

        let response =
            section_header_backend(&mut backend, "backend agnostic", true, Color32::WHITE);

        assert_eq!(response.rect.width(), 160.0);
        let [
            PaintCmd::Text { text: chevron, .. },
            PaintCmd::Text { text: title, .. },
        ] = backend.paints.as_slice()
        else {
            panic!("section header should emit chevron and title text commands");
        };
        assert_eq!(chevron, "▾");
        assert_eq!(title, "BACKEND AGNOSTIC");
    }

    #[test]
    fn section_open_state_uses_mara_memory_with_default_fallback() {
        let mut memory = RecordingMemory::default();
        let id = section_memory_id(Id::new("scope"), "details");

        assert!(section_open(&memory, id, true));
        assert!(!section_open(&memory, id, false));

        set_section_open(&mut memory, id, false);

        assert!(!section_open(&memory, id, true));
        assert_eq!(memory.get_persisted::<bool>(id), Some(false));
    }

    #[test]
    fn section_toggle_flips_only_when_header_clicked() {
        let mut open = false;
        let mut clicked =
            MaraResponse::synthetic(Rect::from_min_size(Pos2::ZERO, Vec2::new(10.0, 10.0)));
        clicked.clicked = true;
        let idle = MaraResponse::synthetic(Rect::from_min_size(Pos2::ZERO, Vec2::new(10.0, 10.0)));

        assert!(apply_section_toggle(&mut open, &clicked));
        assert!(open);
        assert!(!apply_section_toggle(&mut open, &idle));
        assert!(open);
    }

    #[test]
    fn section_body_spec_exists_only_when_open() {
        let id = section_memory_id(Id::new("scope"), "details");

        assert_eq!(section_body_spec(id, false), None);
        assert_eq!(
            section_body_spec(id, true),
            Some(IndentedBodySpec::new(id.with("body")))
        );
    }
}
