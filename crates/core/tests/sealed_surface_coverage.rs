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
        ctx.offscreen("editor", gpu, Vec2::new(320.0, 200.0), 2.0, |ui| {
            let _ = ui.button("inside the offscreen surface");
        })
    }
}
