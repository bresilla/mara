//! Labelled chip cluster — `name: tag1 tag2 …`.
//!
//! Strictly single-line: a fixed-width label on the left, chips on
//! the right that flow left-to-right within the remaining row
//! width. Chips that don't fit get clipped — the row is always
//! exactly [`BADGE_ROW_H`] (= 1U) tall so it lines up with every
//! other 1U widget in a section column.
//!
//! Use when a row carries multiple categorical values that read
//! better as separate pills than as a single comma-joined string —
//! e.g. `lights  [12 dir] [4 pt] [2 spot] [1 dome]`.

use egui::{Sense, vec2};

use crate::style::{UNIT, on_section, theme};
use crate::widget::chip::{chip, chip_colored};

/// Width of the label column, in px. Same value the legacy
/// `maracore::widgets::row` shipped with — wide enough for typical
/// labels at body font size.
pub const BADGE_LABEL_COL_W: f32 = 96.0;

/// Row height, in px. Matches the canonical 1U so a row of
/// `badge_row` calls aligns with neighbouring widgets.
pub const BADGE_ROW_H: f32 = UNIT;

/// Render `label: chip chip chip…` on a single 1U row. Chips paint
/// with the default accent-tinted glass fill. Returns the
/// `Response` covering the entire row.
pub fn badge_row(
    ui: &mut egui::Ui,
    label: &str,
    badges: &[&str],
    accent: egui::Color32,
) -> egui::Response {
    badge_row_with(ui, label, badges, None, accent)
}

/// Like [`badge_row`] but each chip gets its own optional fill
/// override — `Some(c)` paints `chip_colored`, `None` falls back to
/// the default accent-tinted fill. Use for status / severity badges
/// where one entry should stand out.
pub fn badge_row_colored(
    ui: &mut egui::Ui,
    label: &str,
    badges: &[(&str, Option<egui::Color32>)],
    accent: egui::Color32,
) -> egui::Response {
    let labels: Vec<&str> = badges.iter().map(|(l, _)| *l).collect();
    let fills: Vec<Option<egui::Color32>> = badges.iter().map(|(_, f)| *f).collect();
    badge_row_with(ui, label, &labels, Some(&fills), accent)
}

fn badge_row_with(
    ui: &mut egui::Ui,
    label: &str,
    badges: &[&str],
    fills: Option<&[Option<egui::Color32>]>,
    accent: egui::Color32,
) -> egui::Response {
    let badge = theme().widgets.badge;
    let total_w = ui.available_width();
    let label_w = badge.label_col_w.min(total_w * 0.5);

    // Reserve the whole 1U row up front. The label paints into the
    // left cell and the chip cluster paints into a right child Ui;
    // both share the SAME row rect so the row is guaranteed exactly
    // BADGE_ROW_H tall regardless of chip count.
    let (rect, resp) = ui.allocate_exact_size(vec2(total_w, badge.row_h), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return resp;
    }

    // ── Label cell ──
    let label_rect = egui::Rect::from_min_size(rect.min, vec2(label_w, badge.row_h));
    let painter = ui.painter_at(label_rect);
    let label_font = egui::FontId::proportional(badge.label_font);
    let mut job = egui::text::LayoutJob::single_section(
        label.to_string(),
        egui::TextFormat::simple(label_font, on_section()),
    );
    job.wrap.max_rows = 1;
    job.wrap.max_width = (label_w - badge.label_pad_x).max(0.0);
    job.wrap.break_anywhere = true;
    let galley = painter.layout_job(job);
    painter.galley(
        egui::pos2(
            label_rect.min.x + badge.label_pad_x,
            label_rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        on_section(),
    );

    // ── Badges cell — strict left-to-right, NO wrap ──
    //
    // Single-row layout: chips that overflow are simply clipped by
    // the badges cell's `painter_at` clip rect. Anchored vertically
    // centred so chips line up with the label baseline.
    let badges_x = rect.min.x + label_w + badge.label_chips_gap;
    let badges_rect = egui::Rect::from_min_max(egui::pos2(badges_x, rect.min.y), rect.max);
    if badges_rect.width() <= 0.0 {
        return resp;
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(badges_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.set_clip_rect(badges_rect);
    child.spacing_mut().item_spacing = vec2(badge.chip_gap_x, 0.0);
    for (i, b) in badges.iter().enumerate() {
        let fill = fills.and_then(|f| f.get(i).copied()).flatten();
        match fill {
            Some(c) => {
                chip_colored(&mut child, b, c, accent);
            }
            None => {
                chip(&mut child, b, accent);
            }
        }
    }

    resp
}
