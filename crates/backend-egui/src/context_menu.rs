//! Mara-styled context menu — thin wrapper around egui's
//! `Popup::context_menu` with the mara glass frame + accent
//! border. Attach to any `egui::Response` (tree row body, button,
//! inspector cell) and the menu opens on right-click / long-press.

use mara_core::{
    layout::ItemSpacingSpec,
    style::{FrameRole, frame_for, glass_alpha_card},
    vocab::{Color32, Vec2},
};

/// Attach a mara-styled context menu to `resp`. Opens on
/// secondary-click, closes on outside click. `accent` drives the
/// border colour and glass-tint of the popup.
pub(crate) fn context_menu_mara(
    resp: &egui::Response,
    accent: impl Into<Color32>,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let accent = accent.into();
    let frame = frame_for(FrameRole::Popup, accent);

    egui::Popup::context_menu(resp)
        .frame(crate::egui_frame_for_style_spec(frame))
        .show(|ui| {
            crate::apply_item_spacing_spec(ui, context_menu_item_spacing_spec());
            let _ = glass_alpha_card();
            add_contents(ui);
        });
}

fn context_menu_item_spacing_spec() -> ItemSpacingSpec {
    ItemSpacingSpec::new(Vec2::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_menu_spacing_is_mara_layout_policy() {
        assert_eq!(context_menu_item_spacing_spec().item_spacing, Vec2::ZERO);
    }
}
