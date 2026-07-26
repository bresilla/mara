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
