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

// ─── E1.4 · pan/zoom transform ────────────────────────────────────

/// The last capability blocking `ui.rs`: a region that pans and zooms.
/// The gesture logic is Mara's own, so it is fully testable headlessly;
/// only applying the result to a layer needs a backend.
#[test]
fn e14_pan_zoom_drives_a_layer_transform_headlessly() {
    use mara_core::MaraInput;
    use mara_core::backend::record::RecordingBackend;
    use mara_core::layout::UiBackend;
    use mara_core::transform::PanZoom;

    let mut backend =
        RecordingBackend::at(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0)));
    assert!(
        backend.layer_transform.is_none(),
        "nothing applied before the first gesture"
    );

    let mut pan_zoom = PanZoom::new(0.25, 4.0);
    let dragged = MaraInput {
        pointer: Some(Pos2::new(200.0, 150.0)),
        pointer_delta: Vec2::new(12.0, -8.0),
        ..MaraInput::default()
    };
    assert!(pan_zoom.update(&dragged, true));
    backend.set_layer_transform(pan_zoom.transform());

    let applied = backend
        .layer_transform
        .expect("transform reached the layer");
    assert_eq!(applied.translation, Vec2::new(12.0, -8.0));
    assert_eq!(applied.scaling, 1.0, "dragging pans without zooming");
}

/// Content space is what hit testing runs in, so the inverse mapping
/// has to be exact — a wrong inverse means clicks land on the wrong node.
#[test]
fn e14_transform_inverse_maps_screen_back_to_content() {
    use mara_core::transform::Transform;

    let t = Transform::new(Vec2::new(-40.0, 15.0), 2.0);
    let content = Pos2::new(33.0, -21.0);
    let screen = t.mul_pos(content);
    let back = t.inverse().mul_pos(screen);

    assert!((back.x - content.x).abs() < 1e-3, "{back:?}");
    assert!((back.y - content.y).abs() < 1e-3, "{back:?}");
}

// ─── E1.1 · overlay / menu model ──────────────────────────────────

/// `mara_graph`'s viewer opens menus 7 times; `MaraUi` had `context_menu`
/// but nothing that anchors a menu under a button. This is the last
/// capability that blocked porting `DemoViewer` (PLAN.md WS-D1.4).
///
/// The click itself is not simulated: `RecordingBackend::allocate`
/// always returns a synthetic non-clicked response (only `interact`
/// honours the injected one), so the open case is driven by seeding
/// the popup state — which is what a click would have written anyway.
#[test]
fn e11_menu_button_renders_its_body_only_while_open() {
    use mara_core::MaraUi;
    use mara_core::backend::record::RecordingBackend;
    use mara_core::layout::UiBackend;
    use mara_core::popup::PopupState;
    use mara_core::vocab::Id;

    let region = Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 120.0));
    let menu_id = Id::new("graph_menu");

    let mut closed = RecordingBackend::at(region);
    MaraUi::__internal_over_backend(&mut closed, Color32::WHITE, &mut |ui| {
        ui.menu_button(menu_id, "Add node", |menu| {
            let _ = menu.button("Number");
        });
    });
    assert!(
        closed.overlays.is_empty(),
        "a shut menu must not place an overlay"
    );

    let mut opened = RecordingBackend::at(region);
    {
        let mut memory = opened.memory();
        let mut state = PopupState::load(&memory, menu_id);
        state.open();
        state.store(&mut memory, menu_id);
    }
    MaraUi::__internal_over_backend(&mut opened, Color32::WHITE, &mut |ui| {
        ui.menu_button(menu_id, "Add node", |menu| {
            let _ = menu.button("Number");
        });
    });

    assert_eq!(opened.overlays.len(), 1, "an open menu places one overlay");
    let (id, anchor) = opened.overlays[0];
    assert_eq!(id, menu_id, "the overlay is keyed by the menu's id");
    assert!(
        anchor.y > 0.0,
        "the menu anchors below the button, not on top of it"
    );
}

/// Two menus must not share open state — a single key would make one
/// button toggle the other's menu.
#[test]
fn e11_menu_state_is_keyed_per_menu() {
    use mara_core::popup::PopupState;
    use mara_core::vocab::Id;

    let mut memory = mara_core::backend::record::RecordingMemory::default();
    let a = Id::new("menu_a");
    let b = Id::new("menu_b");

    let mut open_a = PopupState::load(&memory, a);
    open_a.open();
    open_a.store(&mut memory, a);

    assert!(PopupState::load(&memory, a).is_open());
    assert!(
        !PopupState::load(&memory, b).is_open(),
        "opening one menu must not open another"
    );
}

// ─── A6/E1.4 · aligned rows ───────────────────────────────────────

/// The last capability `DemoViewer` needed: a fixed-size row whose
/// contents sit centred rather than hanging from the top edge. Pin rows
/// are built from these, so getting the cross-axis offset wrong would
/// misplace every pin label in the graph.
#[test]
fn a6_row_centres_contents_and_restores_parent_flow() {
    use mara_core::CrossAlign;
    use mara_core::backend::record::RecordingBackend;
    use mara_core::layout::{Sense, UiBackend};

    let mut backend =
        RecordingBackend::at(Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 200.0)));

    let mut centred_y = 0.0;
    backend.in_row(Vec2::new(120.0, 40.0), CrossAlign::Center, &mut |row| {
        centred_y = row.allocate(Vec2::new(10.0, 10.0), Sense::Hover).rect.min.y;
    });
    assert_eq!(centred_y, 20.0, "Center starts mid-row, not at its top");

    let mut top_y = 0.0;
    backend.in_row(Vec2::new(120.0, 40.0), CrossAlign::Start, &mut |row| {
        top_y = row.allocate(Vec2::new(10.0, 10.0), Sense::Hover).rect.min.y;
    });
    assert_eq!(top_y, 40.0, "Start hugs the row's own top edge");

    // The parent's flow resumed below both rows, not inside them.
    let after = backend.allocate(Vec2::new(10.0, 10.0), Sense::Hover).rect;
    assert_eq!(after.min.y, 80.0, "two 40px rows consumed 80px of flow");
}

#[test]
fn a6_row_flows_its_contents_rightward() {
    use mara_core::CrossAlign;
    use mara_core::backend::record::RecordingBackend;
    use mara_core::layout::{Sense, UiBackend};

    let mut backend =
        RecordingBackend::at(Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 100.0)));

    let mut xs = Vec::new();
    backend.in_row(Vec2::new(200.0, 30.0), CrossAlign::Center, &mut |row| {
        for _ in 0..3 {
            xs.push(row.allocate(Vec2::new(25.0, 10.0), Sense::Hover).rect.min.x);
        }
    });
    assert_eq!(xs, vec![0.0, 25.0, 50.0], "items advance rightward");
}

// ─── D1.3 · framed surfaces ───────────────────────────────────────

/// `ui.rs` draws node bodies and headers with the backend's frame
/// widget — the last primitive its render path needs. The contract that
/// matters is paint order: the frame must land *behind* its content, or
/// every node would occlude its own pins.
#[test]
fn d13_frame_paints_behind_its_content() {
    use mara_core::MaraUi;
    use mara_core::backend::record::RecordingBackend;
    use mara_core::style::{FrameSpec, MarginSpec};

    let mut backend =
        RecordingBackend::at(Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 120.0)));
    let spec = FrameSpec::new(
        Color32::from_rgb(10, 20, 30),
        mara_core::vocab::Stroke::new(1.0, Color32::WHITE),
        CornerRadius::same(4),
        MarginSpec::symmetric(6, 6),
    );

    let rect = MaraUi::__internal_over_backend_ret(&mut backend, Color32::WHITE, |ui| {
        ui.framed(spec, |ui| {
            ui.label("inside");
        })
    });

    assert!(rect.height() > 0.0, "the frame reports the rect it took");

    let kinds: Vec<&str> = backend
        .paints
        .iter()
        .map(|cmd| match cmd {
            PaintCmd::RectFilled { .. } => "frame",
            PaintCmd::Text { .. } => "text",
            _ => "other",
        })
        .collect();
    let frame_at = kinds.iter().position(|k| *k == "frame");
    let text_at = kinds.iter().position(|k| *k == "text");
    assert!(
        matches!((frame_at, text_at), (Some(f), Some(t)) if f < t),
        "frame must precede its content in the paint stream, got {kinds:?}"
    );
}

/// A reserved slot keeps its place in paint order, and filling it puts
/// the command at that depth rather than at the end.
///
/// This is what lets a node renderer draw wires *behind* nodes it has
/// not measured yet.
#[test]
fn d13_filled_paint_slot_lands_at_the_reserved_depth() {
    use mara_core::backend::record::RecordingBackend;
    use mara_core::layout::UiBackend;

    let mut backend =
        RecordingBackend::at(Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 120.0)));

    let slot = backend.reserve_paint_slot();
    backend.paint(PaintCmd::CircleFilled {
        center: Pos2::new(10.0, 10.0),
        radius: 4.0,
        fill: Color32::WHITE,
    });
    backend.fill_paint_slot(
        slot,
        Some(PaintCmd::RectFilled {
            rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(5.0, 5.0)),
            corner: CornerRadius::same(0),
            fill: Color32::BLACK,
        }),
    );

    let kinds: Vec<&str> = backend
        .paints
        .iter()
        .map(|cmd| match cmd {
            PaintCmd::RectFilled { .. } => "rect",
            PaintCmd::CircleFilled { .. } => "circle",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        vec!["rect", "circle"],
        "the late-filled slot must still paint first, got {kinds:?}"
    );
}

/// A batch fills one slot. Without this a renderer would have to pick
/// between "one slot per command" and losing paint order entirely.
#[test]
fn d13_a_group_fills_a_single_slot() {
    use mara_core::backend::record::RecordingBackend;
    use mara_core::layout::UiBackend;
    use mara_core::vocab::Stroke;

    let mut backend =
        RecordingBackend::at(Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 120.0)));
    let slot = backend.reserve_paint_slot();
    backend.fill_paint_slot(
        slot,
        Some(PaintCmd::Group(vec![
            PaintCmd::Line {
                a: Pos2::ZERO,
                b: Pos2::new(1.0, 1.0),
                stroke: Stroke::new(1.0, Color32::WHITE),
            },
            PaintCmd::Line {
                a: Pos2::ZERO,
                b: Pos2::new(2.0, 2.0),
                stroke: Stroke::new(1.0, Color32::WHITE),
            },
        ])),
    );

    match backend.paints.first() {
        Some(PaintCmd::Group(children)) => assert_eq!(children.len(), 2),
        other => panic!("expected a group in the reserved slot, got {other:?}"),
    }
}

/// `lerp` must not clamp — animation curves rely on overshoot outside
/// `0..=1` — and must agree with the backend term for term, since the
/// two are used interchangeably during the port.
#[cfg(feature = "backend-egui-conv")]
#[test]
fn e4_lerp_matches_the_backend_including_overshoot() {
    for &(a, b) in &[(0.0_f32, 1.0_f32), (-3.5, 7.25), (12.0, 12.0), (100.0, -50.0)] {
        for step in -4..=14 {
            let t = step as f32 / 10.0;
            assert_eq!(
                mara_core::vocab::lerp(a, b, t),
                egui::lerp(a..=b, t),
                "lerp({a}, {b}, {t}) disagrees with the backend"
            );
        }
    }
}

/// WS-E4/G1 constraint, pinned as a test because it decides whether
/// `Id` can go native.
///
/// An `Id` must survive a round trip through the backend unchanged —
/// Mara's state store is keyed by it, and some ids originate on the
/// backend side (a `Ui`'s own id, auto-ids). The obvious native shape,
/// "store the u64 hash", cannot work: the backend's constructor from a
/// raw hash is private, so the only way back is to hash the value a
/// second time, which lands somewhere else entirely. This asserts both
/// halves — the round trip that must hold, and the re-hash that breaks
/// it.
#[cfg(feature = "backend-egui-conv")]
#[test]
fn e4_id_must_round_trip_through_the_backend() {
    use mara_core::vocab::Id;

    for source in ["a_widget", "another", "mara.shelf.layout"] {
        let mara = Id::new(source);
        let there: egui::Id = mara.into();
        let back: Id = there.into();
        assert_eq!(back, mara, "an Id must survive the backend unchanged");

        // Why `Id` stays wrapped: re-hashing the value is not identity,
        // so a native `Id` holding only the hash could not be converted
        // back, and every state lookup keyed by a backend-origin id
        // would silently miss.
        let rehashed = egui::Id::new(there.value());
        assert_ne!(
            rehashed, there,
            "if this ever became equal, a native Id holding the hash would be viable"
        );
    }
}

/// `Color32` stores premultiplied bytes and does the premultiply
/// arithmetic itself now. A rounding difference would tint every
/// translucent surface in the UI by a step — invisible in review, and
/// no other test would catch it. So check against the backend's own
/// result for **every** alpha across a spread of colours, both
/// directions.
#[cfg(feature = "backend-egui-conv")]
#[test]
fn e4_color32_premultiply_matches_the_backend_for_every_alpha() {
    const RGB: [(u8, u8, u8); 6] = [
        (0, 0, 0),
        (255, 255, 255),
        (1, 2, 3),
        (254, 128, 7),
        (17, 200, 99),
        (128, 128, 128),
    ];

    for (r, g, b) in RGB {
        for a in 0..=255u8 {
            let mine = Color32::from_rgba_unmultiplied(r, g, b, a);
            let theirs = egui::Color32::from_rgba_unmultiplied(r, g, b, a);
            assert_eq!(
                [mine.r(), mine.g(), mine.b(), mine.a()],
                [theirs.r(), theirs.g(), theirs.b(), theirs.a()],
                "premultiply differs for rgba({r},{g},{b},{a})"
            );

            assert_eq!(
                mine.to_srgba_unmultiplied(),
                theirs.to_srgba_unmultiplied(),
                "un-premultiply differs for rgba({r},{g},{b},{a})"
            );

            // And the conversion is lossless in both directions.
            assert_eq!(Color32::from(egui::Color32::from(mine)), mine);
        }
    }
}

/// `Align2` is native now, so `anchor_rect` is Mara's arithmetic
/// rather than a delegation. Checked against the backend's own result
/// for all nine alignments — a sign error in one axis would otherwise
/// only show up as text drifting half a label off-target.
#[cfg(feature = "backend-egui-conv")]
#[test]
fn e4_align2_anchoring_matches_the_backend() {
    use mara_core::vocab::Align2;

    let rect = Rect::from_min_size(Pos2::new(30.0, 40.0), Vec2::new(100.0, 20.0));
    let all = [
        Align2::LEFT_TOP,
        Align2::LEFT_CENTER,
        Align2::LEFT_BOTTOM,
        Align2::CENTER_TOP,
        Align2::CENTER_CENTER,
        Align2::CENTER_BOTTOM,
        Align2::RIGHT_TOP,
        Align2::RIGHT_CENTER,
        Align2::RIGHT_BOTTOM,
    ];

    for align in all {
        let mine = align.anchor_rect(rect);
        let theirs: Rect = egui::Align2::from(align)
            .anchor_rect(egui::Rect::from(rect))
            .into();
        assert_eq!(mine, theirs, "anchor_rect disagrees for {align:?}");
    }
}

/// WS-E4/G1: a vocab type must be usable without any backend
/// conversion in scope. `CornerRadius` is the first native one — it
/// owns its data instead of wrapping the backend's type, which is the
/// shape every other vocab newtype has to take before `mara_core` can
/// be split from its backend.
#[test]
fn e4_corner_radius_is_backend_free() {
    let r = CornerRadius::from_corners(1, 2, 3, 4);
    assert_eq!(r.nw, 1);
    assert_eq!(r.ne, 2);
    assert_eq!(r.sw, 3);
    assert_eq!(r.se, 4);

    // Clockwise from north-west, so `sw` is last, not third.
    assert_eq!(r.corners(), [1, 2, 4, 3]);

    let same = CornerRadius::same(7);
    assert_eq!(same.corners(), [7, 7, 7, 7]);
    assert_eq!(CornerRadius::ZERO.corners(), [0, 0, 0, 0]);
}

/// The layout-flow group a node renderer needs: where the next item
/// goes, how big the surface has become, and how to place something
/// outside the flow without the parent losing track of it.
#[test]
fn d13_layout_flow_group_is_reachable_from_maraui() {
    use mara_core::MaraUi;
    use mara_core::backend::record::RecordingBackend;

    let mut backend =
        RecordingBackend::at(Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 120.0)));
    MaraUi::__internal_over_backend(&mut backend, Color32::WHITE, &mut |ui| {
        let start = ui.cursor();

        let outside = Rect::from_min_size(Pos2::new(150.0, 90.0), Vec2::new(20.0, 20.0));
        ui.expand_to_include(outside);
        assert!(
            ui.occupied_rect().contains(Pos2::new(160.0, 100.0)),
            "content placed outside the flow must still count toward the surface's size"
        );

        ui.advance_cursor_past(Rect::from_min_size(start, Vec2::new(10.0, 30.0)));
        assert!(
            ui.cursor().y > start.y,
            "the flow cursor moves past what was placed"
        );
    });
}

/// A clip scope cannot be left unbalanced — that is the whole reason it
/// is a scope and not a push/pop pair.
#[test]
fn d13_clip_scope_restores_the_previous_clip() {
    use mara_core::MaraUi;
    use mara_core::backend::record::RecordingBackend;

    let full = Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 120.0));
    let mut backend = RecordingBackend::at(full);
    MaraUi::__internal_over_backend(&mut backend, Color32::WHITE, &mut |ui| {
        let before = ui.clip_rect();
        let narrow = Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(20.0, 20.0));

        let inner = ui.clipped(narrow, |inner| inner.clip_rect());
        assert_eq!(inner, narrow, "drawing inside the scope is clipped");
        assert_eq!(
            ui.clip_rect(),
            before,
            "and the previous clip is restored on the way out"
        );
    });
}

/// A margin is per-edge. Collapsing it to one number would misplace
/// anything anchored to an edge — which is exactly what a node graph
/// does when it sizes a body from its frame.
#[test]
fn d13_rect_grows_and_shrinks_per_edge() {
    use mara_core::style::MarginSpec;

    let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(100.0, 50.0));
    let margin = MarginSpec {
        left: 1,
        right: 2,
        top: 3,
        bottom: 4,
    };

    let grown = rect.expand_by(margin);
    assert_eq!(grown.min, Pos2::new(9.0, 17.0));
    assert_eq!(grown.max, Pos2::new(112.0, 74.0));

    assert_eq!(
        grown.shrink_by(margin).min,
        rect.min,
        "shrinking by the same margin returns the original rect"
    );
    assert_eq!(grown.shrink_by(margin).max, rect.max);
}

/// A frame's rect is only known after its body runs, so a caller that
/// needs both the geometry and something computed inside must get the
/// body's value back rather than smuggling it through a captured
/// variable.
#[test]
fn d13_framed_with_returns_the_bodys_value() {
    use mara_core::MaraUi;
    use mara_core::backend::record::RecordingBackend;
    use mara_core::style::{FrameSpec, MarginSpec};

    let spec = FrameSpec::new(
        Color32::BLACK,
        mara_core::vocab::Stroke::new(1.0, Color32::WHITE),
        CornerRadius::same(2),
        MarginSpec::symmetric(4, 4),
    );

    let mut backend =
        RecordingBackend::at(Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 120.0)));
    let (rect, inner) =
        MaraUi::__internal_over_backend_ret(&mut backend, Color32::WHITE, |ui| {
            ui.framed_with(spec, |inner| {
                inner.label("inside");
                "computed inside the frame"
            })
        });

    assert_eq!(
        inner, "computed inside the frame",
        "the body's value survives the frame"
    );
    assert!(rect.height() > 0.0, "and the frame still reports its rect");
}

/// A sealed style must survive a round trip through serde, or a module
/// holding `FrameSpec` cannot offer the persistence it could when it
/// held the backend's frame type. This is the prerequisite that lets
/// `mara_graph` drop its hand-written frame serde shim.
#[cfg(feature = "serde")]
#[test]
fn d13_frame_spec_round_trips_through_serde() {
    use mara_core::style::{FrameShadowSpec, FrameSpec, MarginSpec};

    let spec = FrameSpec::new(
        Color32::from_rgba_unmultiplied(10, 20, 30, 200),
        mara_core::vocab::Stroke::new(1.5, Color32::WHITE),
        CornerRadius::same(7),
        MarginSpec::symmetric(3, 4),
    )
    .with_outer_margin(MarginSpec::symmetric(5, 6))
    .with_shadow(FrameShadowSpec::new([1, 2], 8, 1, Color32::BLACK));

    let json = serde_json::to_string(&spec).expect("FrameSpec must serialize");
    let back: FrameSpec = serde_json::from_str(&json).expect("FrameSpec must deserialize");

    assert_eq!(back, spec, "the round trip must preserve every field");
}

/// The graph's node and background frames used to come from the
/// backend's `window`/`canvas` presets, derived from a live backend
/// style. These are the sealed replacements — a surface that wants a
/// panel or a drawing field asks by role, not by backend preset.
#[test]
fn d13_window_and_canvas_frame_roles_are_distinct_and_themed() {
    use mara_core::style::{FrameRole, frame_for};

    let accent = Color32::WHITE;
    let window = frame_for(FrameRole::Window, accent);
    let canvas = frame_for(FrameRole::Canvas, accent);

    assert!(
        window.shadow.is_some(),
        "a floating panel casts a shadow; that is what makes it read as floating"
    );
    assert!(
        canvas.shadow.is_none(),
        "a recessed drawing field must not cast one"
    );
    assert_ne!(
        window.fill, canvas.fill,
        "a panel and the field behind it cannot share a fill or the panel vanishes"
    );
    assert!(
        window.inner_margin.left > canvas.inner_margin.left,
        "a panel pads its content; a canvas gives the drawing its room"
    );
}

/// Inner and outer margin do different jobs: the outer holds the
/// border away from the parent's cursor, the inner holds content away
/// from the border. A frame that folded them together would place the
/// border in the wrong place.
#[test]
fn d13_frame_outer_margin_offsets_the_border_itself() {
    use mara_core::backend::record::RecordingBackend;
    use mara_core::layout::{Sense, UiBackend};
    use mara_core::style::{FrameSpec, MarginSpec};

    let spec = FrameSpec::new(
        Color32::BLACK,
        mara_core::vocab::Stroke::new(1.0, Color32::WHITE),
        CornerRadius::same(2),
        MarginSpec::symmetric(4, 4),
    )
    .with_outer_margin(MarginSpec::symmetric(10, 10));

    assert_eq!(spec.total_margin().left, 14, "total is inner plus outer");

    let mut backend =
        RecordingBackend::at(Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 120.0)));
    let mut inner_min = Pos2::ZERO;
    let rect = backend.framed(spec, &mut |inner| {
        inner_min = inner.allocate(Vec2::new(10.0, 10.0), Sense::Hover).rect.min;
    });

    assert_eq!(
        rect.min,
        Pos2::new(10.0, 10.0),
        "the border starts after the outer margin"
    );
    assert_eq!(
        inner_min,
        Pos2::new(14.0, 14.0),
        "content starts after both margins"
    );
}

/// Content is inset by the frame's margin, so a body cannot draw over
/// its own border.
#[test]
fn d13_frame_insets_content_by_its_margin() {
    use mara_core::backend::record::RecordingBackend;
    use mara_core::layout::{Sense, UiBackend};
    use mara_core::style::{FrameSpec, MarginSpec};

    let mut backend =
        RecordingBackend::at(Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 120.0)));
    let spec = FrameSpec::new(
        Color32::BLACK,
        mara_core::vocab::Stroke::new(1.0, Color32::WHITE),
        CornerRadius::same(2),
        MarginSpec::symmetric(8, 8),
    );

    let mut inner_min = Pos2::ZERO;
    backend.framed(spec, &mut |inner| {
        inner_min = inner.allocate(Vec2::new(10.0, 10.0), Sense::Hover).rect.min;
    });
    assert_eq!(
        inner_min,
        Pos2::new(8.0, 8.0),
        "content starts inside the margin"
    );
}
