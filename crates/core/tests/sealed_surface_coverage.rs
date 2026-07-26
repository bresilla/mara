//! WS-A exit criterion — every primitive added to the sealed surface is
//! exercised here with **no egui in the call path**.
//!
//! Each test drives one capability through the command-recording
//! painter/backend and asserts the emitted [`PaintCmd`] stream (or the
//! plain-data snapshot type). A module crate that can express its
//! drawing against these primitives can drop its `egui` dependency; a
//! primitive that cannot be driven from here is not sealed, whatever
//! its signature says.
//!
//! Grouped by PLAN.md workstream item so a gap is traceable back to the
//! capability that is still missing.

use mara_core::mui::{MaraKey, MaraKeySet};
use mara_core::paint::{PaintCmd, PaintVertex};
use mara_core::vocab::{Color32, CornerRadius, PointerButton, Pos2, Rect, Vec2};
use mara_core::{MaraInput, MaraPainter, MaraResponse};

fn painter() -> MaraPainter {
    MaraPainter::__internal_recording(Rect::from_min_size(
        Pos2::new(0.0, 0.0),
        Vec2::new(256.0, 128.0),
    ))
}

/// Recorded commands, unwrapped from the `Clip` envelope the recording
/// painter wraps every command in.
fn recorded(painter: &MaraPainter) -> Vec<PaintCmd> {
    painter
        .__internal_recorded_commands()
        .into_iter()
        .flat_map(|cmd| match cmd {
            PaintCmd::Clip { children, .. } => children,
            other => vec![other],
        })
        .collect()
}

// ─── A3 · paint depth ─────────────────────────────────────────────

#[test]
fn a3_mesh_emits_a_mesh_command() {
    let p = painter();
    let verts = vec![
        PaintVertex {
            pos: Pos2::new(0.0, 0.0),
            color: Color32::WHITE,
        },
        PaintVertex {
            pos: Pos2::new(10.0, 0.0),
            color: Color32::BLACK,
        },
        PaintVertex {
            pos: Pos2::new(0.0, 10.0),
            color: Color32::GRAY,
        },
    ];
    p.mesh(verts.clone(), vec![0, 1, 2]);

    match recorded(&p).as_slice() {
        [PaintCmd::Mesh { vertices, indices }] => {
            assert_eq!(vertices, &verts);
            assert_eq!(indices, &vec![0, 1, 2]);
        }
        other => panic!("expected one Mesh command, got {other:#?}"),
    }
}

#[test]
fn a3_mesh_rejects_malformed_geometry() {
    let verts = vec![PaintVertex {
        pos: Pos2::ZERO,
        color: Color32::WHITE,
    }];

    let ragged = painter();
    ragged.mesh(verts.clone(), vec![0, 0]);
    assert!(
        recorded(&ragged).is_empty(),
        "index count not a multiple of three must draw nothing, not panic"
    );

    let out_of_range = painter();
    out_of_range.mesh(verts, vec![0, 1, 2]);
    assert!(
        recorded(&out_of_range).is_empty(),
        "an index past the end of `vertices` must draw nothing, not panic"
    );
}

#[test]
fn a3_shadow_emits_a_shadow_command() {
    let p = painter();
    let rect = Rect::from_min_size(Pos2::new(4.0, 4.0), Vec2::new(40.0, 20.0));
    p.shadow(rect, CornerRadius::same(6), [0, 3], 8, 1, Color32::BLACK);

    match recorded(&p).as_slice() {
        [
            PaintCmd::Shadow {
                rect: got,
                offset,
                blur,
                spread,
                ..
            },
        ] => {
            assert_eq!(*got, rect);
            assert_eq!(*offset, [0, 3]);
            assert_eq!((*blur, *spread), (8, 1));
        }
        other => panic!("expected one Shadow command, got {other:#?}"),
    }
}

#[test]
fn a3_measure_text_is_deterministic_without_a_font_atlas() {
    let p = painter();
    let wide = p.measure_text("mmmmmmmm", 16.0, false);
    let narrow = p.measure_text("mm", 16.0, false);

    assert!(
        wide.x > narrow.x,
        "longer text must measure wider ({wide:?} vs {narrow:?})"
    );
    assert_eq!(wide.y, 16.0, "line height tracks the requested size");
    assert_eq!(
        p.measure_text("", 16.0, false).x,
        0.0,
        "empty text has no width"
    );
}

// ─── A2 · input depth ─────────────────────────────────────────────

#[test]
fn a2_key_set_round_trips_every_key() {
    for key in MaraKey::ALL {
        let mut set = MaraKeySet::empty();
        assert!(!set.contains(key));
        set.insert(key);
        assert!(set.contains(key), "{key:?} did not survive the bitset");
    }

    let set: MaraKeySet = [MaraKey::Delete, MaraKey::F12, MaraKey::Escape]
        .into_iter()
        .collect();
    assert_eq!(
        set.iter().collect::<Vec<_>>(),
        vec![MaraKey::Escape, MaraKey::Delete, MaraKey::F12],
        "iteration follows MaraKey::ALL order, not insertion order"
    );
}

/// The bitset is a `u128` indexed by `key as u8`, so the enum must stay
/// within 128 variants. This is the guard that turns a future overflow
/// into a failing test rather than silent key aliasing.
#[test]
fn a2_key_set_cannot_overflow_its_bitset() {
    assert!(
        MaraKey::ALL.len() <= 128,
        "MaraKeySet indexes a u128; MaraKey has grown past 128 variants"
    );
    for (index, key) in MaraKey::ALL.into_iter().enumerate() {
        assert_eq!(
            key as u8 as usize, index,
            "MaraKey::ALL must stay in declaration order — {key:?} is out of place"
        );
    }
}

#[test]
fn a2_input_exposes_all_three_pointer_buttons() {
    let mut input = MaraInput::default();
    assert!(PointerButton::ALL.iter().all(|&b| !input.button_down(b)));

    input.middle_down = true;
    input.secondary_pressed = true;

    assert!(input.button_down(PointerButton::Middle));
    assert!(!input.button_down(PointerButton::Primary));
    assert!(input.button_pressed(PointerButton::Secondary));
    assert!(!input.button_pressed(PointerButton::Middle));
}

#[test]
fn a2_input_reports_key_presses() {
    let mut input = MaraInput::default();
    assert!(!input.key_pressed(MaraKey::Delete));
    input.keys_pressed.insert(MaraKey::Delete);
    assert!(input.key_pressed(MaraKey::Delete));
    assert!(!input.key_pressed(MaraKey::Backspace));
}

#[test]
fn a2_synthetic_response_reports_no_button_interaction() {
    let response =
        MaraResponse::__internal_synthetic(Rect::from_min_size(Pos2::ZERO, Vec2::new(10.0, 10.0)));
    for button in PointerButton::ALL {
        assert!(!response.clicked_by(button));
        assert!(!response.dragged_by(button));
    }
}

// ─── F3 · SVG rasterisation ───────────────────────────────────────

/// `PaintCmd::Svg` is public API, but until the `svg` feature landed no
/// rasteriser existed anywhere in the workspace, so every SVG paint
/// command silently drew nothing. This drives one through a real egui
/// context and asserts the loader chain resolves it to a texture.
#[cfg(feature = "svg")]
#[test]
fn f3_svg_paint_command_reaches_a_rasteriser() {
    use mara_core::paint::PaintCmd;

    const MARKER: &str = r##"<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M12 2 22 12 12 22 2 12Z" fill="#ffffff"/></svg>"##;

    let ctx = egui::Context::default();
    mara_core::enforce::__internal_enforce_defaults(&ctx);

    // The loader rasterises off-frame, so poll a few frames before
    // concluding anything — one pass is expected to report Pending.
    let mut resolved = false;
    for _ in 0..64 {
        let _ = ctx.run_ui(Default::default(), |ui| {
            mara_core::paint::__internal_render_paint_cmd_egui(
                ui.painter(),
                PaintCmd::Svg {
                    svg: MARKER.to_owned(),
                    rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(24.0, 24.0)),
                    tint: Color32::WHITE,
                },
            );
        });
        let uri = format!(
            "bytes://mara_svg_paint_{:016x}.svg",
            MARKER.bytes().fold(5381u64, |h, b| h
                .wrapping_mul(33)
                .wrapping_add(u64::from(b)))
        );
        if matches!(
            ctx.try_load_texture(
                &uri,
                egui::TextureOptions::LINEAR,
                egui::load::SizeHint::Size {
                    width: 24,
                    height: 24,
                    maintain_aspect_ratio: true,
                },
            ),
            Ok(egui::load::TexturePoll::Ready { .. })
        ) {
            resolved = true;
            break;
        }
    }
    assert!(
        resolved,
        "SVG never rasterised — the `svg` feature's loader is not reaching PaintCmd::Svg"
    );
}

// ─── A7 · offscreen UI surface ────────────────────────────────────

/// `ViewCtx::offscreen` needs a live GPU device, which a unit test has
/// no way to obtain — so this asserts the part that *is* testable
/// headlessly: the entry point exists, is feature-gated, and speaks
/// only vocab types. The rendering path itself is exercised by the
/// consumer that replaces `mara_graph`'s private second-context
/// machinery (PLAN.md WS-D1.4).
///
/// Compile-time assertion: if the signature ever grows an egui or wgpu
/// type, this stops building.
#[cfg(feature = "gpu")]
#[test]
fn a7_offscreen_entry_point_speaks_only_vocab() {
    fn _assert_signature(
        ctx: &mut mara_core::ViewCtx<'_>,
        gpu: mara_gpu::MaraRenderState<'_>,
    ) -> Option<mara_core::vocab::TextureId> {
        ctx.offscreen(
            "editor",
            gpu,
            Rect::from_min_size(Pos2::new(40.0, 20.0), Vec2::new(320.0, 200.0)),
            2.0,
            |ui| {
                let _ = ui.button("inside the offscreen surface");
            },
        )
    }
}

// ─── A8 · multi-line text editing ─────────────────────────────────

/// The text area must render entirely through the paint IR — no egui
/// in the call path — so a code editor rewritten onto it (WS-D2) is
/// sealed by construction. Driving it over the recording backend and
/// inspecting the command stream is the proof.
#[test]
fn a8_text_area_renders_through_the_paint_ir() {
    use mara_core::MaraTextArea;
    use mara_core::backend::record::RecordingBackend;

    let mut backend =
        RecordingBackend::at(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0)));
    let mut text = String::from("fn main() {\n    let x = 1;\n}");
    let out = MaraTextArea::new("editor")
        .rows(6)
        .show(&mut backend, &mut text);

    assert_eq!(out.caret, (2, 1), "caret starts at the end of the buffer");
    assert!(!out.changed, "no input this pass, so no edit");

    let kinds: Vec<&str> = backend
        .paints
        .iter()
        .map(|cmd| match cmd {
            PaintCmd::RectFilled { .. } => "rect",
            PaintCmd::TextRuns { .. } => "runs",
            _ => "other",
        })
        .collect();
    assert!(
        kinds.contains(&"rect"),
        "expected a background and a caret rect, got {kinds:?}"
    );
    assert_eq!(
        kinds.iter().filter(|k| **k == "runs").count(),
        3,
        "one TextRuns command per line of the buffer, got {kinds:?}"
    );
}

/// A per-line highlighter is the seam a tokeniser plugs into: its runs
/// must reach the paint stream unchanged.
#[test]
fn a8_text_area_emits_highlighter_runs_verbatim() {
    use mara_core::MaraTextArea;
    use mara_core::backend::record::RecordingBackend;
    use mara_core::paint::{TextFamily, TextRun};

    let mut backend =
        RecordingBackend::at(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 100.0)));
    let mut text = String::from("let x");
    let highlight = |line: &str| {
        line.split_inclusive(' ')
            .map(|word| TextRun {
                text: word.to_owned(),
                size: 13.0,
                color: if word.starts_with("let") {
                    Color32::from_rgb(200, 120, 255)
                } else {
                    Color32::WHITE
                },
                family: TextFamily::Monospace,
                extra_letter_spacing: 0.0,
                leading_space: 0.0,
            })
            .collect()
    };
    let _ = MaraTextArea::new("hl")
        .rows(1)
        .highlight(&highlight)
        .show(&mut backend, &mut text);

    let runs = backend
        .paints
        .iter()
        .find_map(|cmd| match cmd {
            PaintCmd::TextRuns { runs, .. } => Some(runs.clone()),
            _ => None,
        })
        .expect("a TextRuns command");
    assert_eq!(runs.len(), 2, "highlighter split the line into two runs");
    assert_eq!(runs[0].text, "let ");
    assert_eq!(runs[0].color, Color32::from_rgb(200, 120, 255));
    assert_eq!(runs[1].text, "x");
}

// ─── D1.3 · frame-scoped cache ────────────────────────────────────

/// `MaraCache` replaces the egui frame-cache that `mara_graph`'s wire
/// geometry depends on (PLAN.md WS-D1.3). The contract that matters is
/// "swept once per frame, not once per access" — a cache swept on every
/// lookup would evict entries mid-frame and defeat the memoisation.
#[test]
fn d13_cache_sweeps_once_per_frame_not_once_per_access() {
    use mara_core::MaraMemoryCtx;
    use mara_core::SweptCache;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Counted {
        sweeps: u64,
        entries: HashMap<u32, u64>,
    }
    impl SweptCache for Counted {
        fn sweep(&mut self) {
            self.sweeps += 1;
            let current = self.sweeps;
            self.entries.retain(|_, stamp| *stamp + 1 >= current);
        }
    }

    let ctx = egui::Context::default();
    let key = mara_core::vocab::Id::new("wires");

    let _ = ctx.run_ui(Default::default(), |ui| {
        let mut memory = MaraMemoryCtx::__internal_from_backend_ctx(ui.ctx());
        // Three accesses inside one frame must produce exactly one sweep.
        for _ in 0..3 {
            let cache = memory.cache::<Counted>(key);
            cache.with(|c| c.entries.insert(1, c.sweeps)).unwrap();
        }
        let cache = memory.cache::<Counted>(key);
        assert_eq!(
            cache.with(|c| c.sweeps),
            Some(1),
            "repeated access within a frame must not re-sweep"
        );
    });

    let _ = ctx.run_ui(Default::default(), |ui| {
        let mut memory = MaraMemoryCtx::__internal_from_backend_ctx(ui.ctx());
        let cache = memory.cache::<Counted>(key);
        assert_eq!(
            cache.with(|c| c.sweeps),
            Some(2),
            "a new frame sweeps exactly once more"
        );
        assert_eq!(
            cache.with(|c| c.entries.len()),
            Some(1),
            "the entry touched last frame survives one sweep"
        );
    });
}

// ─── E1.4 · cursor-region layout ──────────────────────────────────

/// `mara_graph`'s node renderer computes its own geometry and places
/// children at explicit rects (`new_child` ×7, `advance_cursor_after_rect`
/// ×4). `UiBackend` had no equivalent, which is what blocks the last file
/// of WS-D1.3. These are the semantics the port depends on.
#[test]
fn e14_child_at_does_not_disturb_parent_flow() {
    use mara_core::backend::record::RecordingBackend;
    use mara_core::layout::{Sense, UiBackend};

    let mut backend =
        RecordingBackend::at(Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 200.0)));

    let first = backend.allocate(Vec2::new(50.0, 20.0), Sense::Hover).rect;
    assert_eq!(first.min.y, 0.0);

    // A child placed far away must not move the parent's cursor.
    let cursor_before = backend.cursor();
    backend.child_at(
        Rect::from_min_size(Pos2::new(120.0, 120.0), Vec2::new(40.0, 40.0)),
        &mut |child| {
            let _ = child.allocate(Vec2::new(10.0, 10.0), Sense::Hover);
        },
    );
    assert_eq!(
        backend.cursor(),
        cursor_before,
        "child_at must leave the parent's flow cursor alone"
    );

    // The next parent allocation still follows the first one.
    let second = backend.allocate(Vec2::new(50.0, 20.0), Sense::Hover).rect;
    assert_eq!(second.min.y, first.max.y);
}

#[test]
fn e14_occupied_rect_grows_to_cover_explicit_placement() {
    use mara_core::backend::record::RecordingBackend;
    use mara_core::layout::UiBackend;

    let mut backend =
        RecordingBackend::at(Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 200.0)));

    // Nothing placed yet, so nothing is occupied.
    assert_eq!(backend.occupied_rect().size(), Vec2::ZERO);

    backend.expand_to_include(Rect::from_min_size(
        Pos2::new(10.0, 10.0),
        Vec2::new(30.0, 30.0),
    ));
    backend.expand_to_include(Rect::from_min_size(
        Pos2::new(100.0, 5.0),
        Vec2::new(20.0, 20.0),
    ));

    let occupied = backend.occupied_rect();
    assert_eq!(occupied.min, Pos2::new(10.0, 5.0));
    assert_eq!(occupied.max, Pos2::new(120.0, 40.0));
}

#[test]
fn e14_advance_cursor_past_moves_flow_below_a_rect() {
    use mara_core::backend::record::RecordingBackend;
    use mara_core::layout::{Sense, UiBackend};

    let mut backend =
        RecordingBackend::at(Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 200.0)));

    backend.advance_cursor_past(Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 60.0)));
    let next = backend.allocate(Vec2::new(10.0, 10.0), Sense::Hover).rect;
    assert_eq!(next.min.y, 60.0, "flow resumes below the consumed rect");
}
