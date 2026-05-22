//! Key-chip + action-label row, used in "Controls" / "Keys" help
//! sections. Renders a small monospace key chip on the left and the
//! action description on the right; the action truncates with `…`
//! when the row is too narrow.
//!
//! Sits at the same brightness tier as the search field / dropdown
//! trigger (`track_fill`), with text colour picked by `on_track`
//! so the chip stays readable across theme + accent combos.

use crate::style::{FillRole, RadiusRole, fill_for, on_section_dim, on_track, radius_for, theme};

/// Canonical key-row height. One U so a `Pod::with_keybindings`
/// row matches the rhythm of every other 1U widget.
pub const KEYBINDING_ROW_H: f32 = crate::style::UNIT;

/// Render a single keybinding row: `[keys]  action description`.
pub fn keybinding_row(ui: &mut egui::Ui, keys: &str, action: &str) -> egui::Response {
    keybinding_row_h(ui, keys, action, theme().widgets.keybinding.row_h)
}

/// Variable-height variant — caller fixes the row height (used by
/// `Pod::with_keybindings` so all rows in a list share the same
/// metric).
pub fn keybinding_row_h(
    ui: &mut egui::Ui,
    keys: &str,
    action: &str,
    height: f32,
) -> egui::Response {
    let avail_w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(avail_w, height), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let painter = ui.painter_at(rect);
    let keybinding = theme().widgets.keybinding;
    let accent = ui.visuals().selection.stroke.color;
    let mid_y = rect.center().y;

    // ── Key chip ──
    let key_font = egui::FontId::monospace(keybinding.key_font);
    let key_galley = {
        let mut job = egui::text::LayoutJob::single_section(
            keys.to_string(),
            egui::TextFormat::simple(key_font, on_track()),
        );
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        painter.layout_job(job)
    };
    let key_text_w = key_galley.size().x.ceil();
    let key_text_h = key_galley.size().y.ceil();
    let chip_w = key_text_w + keybinding.key_pad_x * 2.0;
    let chip_h = key_text_h + keybinding.key_pad_y * 2.0;
    let chip_rect = egui::Rect::from_min_size(
        egui::pos2(rect.min.x, mid_y - chip_h * 0.5),
        egui::vec2(chip_w, chip_h),
    );
    painter.rect_filled(
        chip_rect,
        radius_for(RadiusRole::Widget),
        fill_for(FillRole::Track, accent),
    );
    painter.galley(
        egui::pos2(
            chip_rect.min.x + keybinding.key_pad_x,
            mid_y - key_text_h * 0.5,
        ),
        key_galley,
        on_track(),
    );

    // ── Action label (truncating) ──
    let action_x = chip_rect.max.x + keybinding.key_action_gap;
    let action_max_w = (rect.max.x - action_x).max(0.0);
    if action_max_w > 0.0 {
        let action_font = egui::FontId::proportional(keybinding.action_font);
        let mut job = egui::text::LayoutJob::single_section(
            action.to_string(),
            egui::TextFormat::simple(action_font, on_section_dim()),
        );
        job.wrap.max_rows = 1;
        job.wrap.max_width = action_max_w;
        job.wrap.break_anywhere = true;
        let action_galley = painter.layout_job(job);
        painter.galley(
            egui::pos2(action_x, mid_y - action_galley.size().y * 0.5),
            action_galley,
            on_section_dim(),
        );
    }
    resp
}
