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
    layout::{Sense, SpaceSpec, UiBackend},
    memory::MaraMemory,
    mui::MaraResponse,
    paint::PaintCmd,
    style::{FrameRole, frame_for, on_section, theme},
    vocab::{Align2, Color32, Id, Pos2, Rect, Vec2},
};

/// Extra body indent beyond the frame's left margin, in px.
const SECTION_BODY_INDENT: f32 = 4.0;

/// Render a mara-styled collapsible section through the backend
/// contract — headless-capable (PLAN.md Phase 4). `id_salt` makes the
/// section's open/closed state distinct from siblings; `title` is the
/// header label; `default_open` is the initial state on first paint.
/// `body` renders the (indented) content when open.
///
/// The glass frame is emitted as paint commands: a background fill
/// reserved *before* the content (so it sits behind it) and a border
/// stroke painted *after*, both sized to the content the section
/// occupied (measured via `available_rect` before/after).
pub(crate) fn section_backend(
    mut backend: &mut dyn UiBackend,
    id_salt: &str,
    title: &str,
    accent: Color32,
    default_open: bool,
    body: &mut dyn FnMut(&mut dyn UiBackend),
) {
    let id = section_memory_id(backend.id(), id_salt);
    let frame = frame_for(FrameRole::Section, accent);
    let margin = frame.inner_margin;

    let mut open = section_open(&backend.memory(), id, default_open);

    let start = backend.available_rect();
    let bg_slot = backend.reserve_paint_slot();
    backend.add_space(SpaceSpec::vertical(f32::from(margin.top)));

    let header_resp = section_header_backend(&mut backend, title, open, accent);
    if apply_section_toggle(&mut open, &header_resp) {
        set_section_open(&mut backend.memory(), id, open);
    }
    if open {
        backend.in_child(
            id.with("body"),
            f32::from(margin.left) + SECTION_BODY_INDENT,
            body,
        );
    }
    backend.add_space(SpaceSpec::vertical(f32::from(margin.bottom)));

    let end_y = backend.available_rect().min.y;
    let frame_rect = Rect::from_min_max(start.min, Pos2::new(start.max.x, end_y));
    backend.fill_paint_slot(
        bg_slot,
        Some(PaintCmd::RectFilled {
            rect: frame_rect,
            corner: frame.corner,
            fill: frame.fill,
        }),
    );
    backend.paint(PaintCmd::RectStroke {
        rect: frame_rect,
        corner: frame.corner,
        stroke: frame.stroke,
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
    fn section_backend_emits_frame_fill_stroke_and_header_headless() {
        let mut backend =
            RecordingBackend::at(Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 200.0)));
        section_backend(
            &mut backend,
            "grp",
            "Details",
            Color32::WHITE,
            true,
            &mut |child| {
                let _ = child.allocate(Vec2::new(80.0, 16.0), Sense::Hover);
            },
        );
        // First command is the frame fill (reserved before content),
        // then chevron + title text, then a body allocation, and the
        // last is the border stroke.
        assert!(
            matches!(backend.paints.first(), Some(PaintCmd::RectFilled { .. })),
            "frame fill must sit behind content"
        );
        assert!(
            matches!(backend.paints.last(), Some(PaintCmd::RectStroke { .. })),
            "border stroke must paint on top"
        );
        assert!(
            backend
                .paints
                .iter()
                .any(|c| matches!(c, PaintCmd::Text { text, .. } if text == "DETAILS")),
            "header title renders headlessly"
        );
    }
}
