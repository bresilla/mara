//! Mara-styled context menu — thin wrapper around egui's
//! `Popup::context_menu` with the mara glass frame + accent
//! border. Attach to any `egui::Response` (tree row body, button,
//! inspector cell) and the menu opens on right-click / long-press.

use crate::style::{FrameRole, frame_for, glass_alpha_card};

/// Attach a mara-styled context menu to `resp`. Opens on
/// secondary-click, closes on outside click. `accent` drives the
/// border colour and glass-tint of the popup.
pub fn context_menu_mara(
    resp: &egui::Response,
    accent: egui::Color32,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let frame = frame_for(FrameRole::Popup, accent);

    egui::Popup::context_menu(resp).frame(frame).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let _ = glass_alpha_card();
        add_contents(ui);
    });
}
