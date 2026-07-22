//! Key-chip + action-label row, used in "Controls" / "Keys" help
//! sections. Renders a small monospace key chip on the left and the
//! action description on the right; the action truncates with `…`
//! when the row is too narrow.
//!
//! Sits at the same brightness tier as the search field / dropdown
//! trigger (`track_fill`), with text colour picked by `on_track`
//! so the chip stays readable across theme + accent combos.

use crate::{
    layout::{Sense, UiBackend},
    mui::MaraResponse,
    paint::PaintCmd,
    style::{FillRole, RadiusRole, fill_for, on_section_dim, on_track, radius_for, theme},
    vocab::{Align2, Color32, Pos2, Rect, Vec2},
};

/// Canonical key-row height. One U so a `Pod::with_keybindings`
/// row matches the rhythm of every other 1U widget.
pub const KEYBINDING_ROW_H: f32 = crate::style::UNIT;

/// Backend-neutral keybinding renderer.
pub fn keybinding_row_backend(
    backend: &mut impl UiBackend,
    keys: &str,
    action: &str,
    height: f32,
    accent: Color32,
) -> MaraResponse {
    let avail_w = backend.available_rect().width().max(0.0);
    let resp = backend.allocate(Vec2::new(avail_w, height), Sense::Hover);
    let keybinding = theme().widgets.keybinding;
    let mid_y = resp.rect.center().y;

    // ── Key chip ──
    let key_text = backend.measure_text(keys, keybinding.key_font, true);
    let key_text_w = key_text.x.ceil();
    let key_text_h = key_text.y.ceil();
    let chip_rect = Rect::from_min_size(
        Pos2::new(
            resp.rect.min.x,
            mid_y - (key_text_h + keybinding.key_pad_y * 2.0) * 0.5,
        ),
        Vec2::new(
            key_text_w + keybinding.key_pad_x * 2.0,
            key_text_h + keybinding.key_pad_y * 2.0,
        ),
    );
    backend.paint(PaintCmd::RectFilled {
        rect: chip_rect,
        corner: radius_for(RadiusRole::Widget),
        fill: fill_for(FillRole::Track, accent),
    });
    backend.paint(PaintCmd::Text {
        pos: Pos2::new(chip_rect.min.x + keybinding.key_pad_x, mid_y),
        anchor: Align2::LEFT_CENTER,
        text: keys.to_owned(),
        size: keybinding.key_font,
        color: on_track(),
        mono: true,
    });

    // ── Action label (truncating) ──
    let action_x = chip_rect.max.x + keybinding.key_action_gap;
    let action_max_w = (resp.rect.max.x - action_x).max(0.0);
    if action_max_w > 0.0 {
        let action_rect = Rect::from_min_max(
            Pos2::new(action_x, resp.rect.min.y),
            Pos2::new(resp.rect.max.x, resp.rect.max.y),
        );
        backend.push_clip(action_rect);
        backend.paint(PaintCmd::Text {
            pos: Pos2::new(action_x, mid_y),
            anchor: Align2::LEFT_CENTER,
            text: action.to_owned(),
            size: keybinding.action_font,
            color: on_section_dim(),
            mono: false,
        });
        backend.pop_clip();
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::backend::record::RecordingBackend;

    #[test]
    fn keybinding_backend_emits_chip_and_action_commands() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(220.0, KEYBINDING_ROW_H)),
            paints: Vec::new(),
            clips: Vec::new(),
            ..Default::default()
        };

        let response = keybinding_row_backend(
            &mut backend,
            "Ctrl+K",
            "Command palette",
            KEYBINDING_ROW_H,
            Color32::WHITE,
        );

        assert_eq!(response.rect.width(), 220.0);
        assert_eq!(backend.paints.len(), 3);
        let [
            PaintCmd::RectFilled { .. },
            PaintCmd::Text {
                text: key,
                mono: true,
                ..
            },
            PaintCmd::Text {
                text: action,
                mono: false,
                ..
            },
        ] = backend.paints.as_slice()
        else {
            panic!("keybinding should emit key chip fill, key text and action text");
        };
        assert_eq!(key, "Ctrl+K");
        assert_eq!(action, "Command palette");
    }
}
