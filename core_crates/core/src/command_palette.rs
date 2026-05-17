//! Cmd-K / Ctrl-P style command palette.
//!
//! A centred floating overlay with a search field at the top and
//! a fuzzy-matched list of named actions below. Kept open by
//! caller-owned state ([`CommandPaletteState`]) so the host
//! controls the key binding that opens it.
//!
//! Semantics:
//!
//! * **Open**: caller sets `state.open = true` — usually from a
//!   keyboard shortcut in the host app.
//! * **Dismiss**: Escape, clicking outside, or selecting an item.
//! * **Select**: Enter picks the currently-highlighted item; Up /
//!   Down moves the highlight. The id of the picked item is
//!   returned so the caller can dispatch.
//!
//! Matching: substring + initials ("otp" → "Open Timeline
//! Panel"). Simple scoring is enough for most command sets — for
//! sublime-grade ranking, wrap this palette and pre-filter
//! `items` yourself before passing them in.

use std::collections::HashSet;

use egui;

use crate::style::{font, glass_alpha_card, glass_alpha_window, glass_fill, widget_border};

/// One entry in the palette's action list.
pub struct PaletteItem {
    pub id: &'static str,
    pub label: &'static str,
    /// Optional secondary hint — dim right-aligned text shown on
    /// each row. Use for keybindings ("Ctrl+P") or categories
    /// ("Layout").
    pub hint: Option<&'static str>,
}

impl PaletteItem {
    #[must_use]
    pub fn new(id: &'static str, label: &'static str) -> Self {
        assert_palette_text(id, "command palette items require a non-empty id");
        assert_palette_text(label, "command palette items require a non-empty label");
        Self {
            id,
            label,
            hint: None,
        }
    }

    #[must_use]
    pub fn with_hint(mut self, hint: &'static str) -> Self {
        assert_palette_text(
            hint,
            "command palette item hints must be non-empty when provided",
        );
        self.hint = Some(hint);
        self
    }
}

/// Persistent state the palette owns. Wrap in whatever the host
/// prefers (bevy: `Resource`; plain egui: app field).
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
#[derive(Debug, Clone, Default)]
pub struct CommandPaletteState {
    /// Master toggle. Set from a keyboard-shortcut handler in the
    /// host. The palette also clears this on Escape / outside
    /// click / selection.
    pub open: bool,
    /// Current search query.
    pub query: String,
    /// Index into the filtered-items list of the row currently
    /// highlighted. Moved by Up / Down keys.
    pub selected: usize,
}

/// Draw the palette overlay when `state.open == true`. Returns
/// `Some(id)` on the frame an item is picked (via Enter /
/// click); otherwise `None`.
pub fn command_palette(
    ctx: &egui::Context,
    state: &mut CommandPaletteState,
    items: &[PaletteItem],
    accent: egui::Color32,
) -> Option<&'static str> {
    validate_palette_items(items);
    if !state.open {
        return None;
    }

    // Filter + score. `matcher` is a simple case-insensitive
    // substring check; initials match on tokens. Keep the cost
    // negligible even with thousands of items.
    let filtered: Vec<&PaletteItem> = if state.query.is_empty() {
        items.iter().collect()
    } else {
        let q = state.query.to_lowercase();
        items.iter().filter(|it| matches(it.label, &q)).collect()
    };

    // Clamp the selected index against the CURRENT filtered view
    // — the query may have just shrunk the list.
    if filtered.is_empty() {
        state.selected = 0;
    } else {
        state.selected = state.selected.min(filtered.len() - 1);
    }

    let mut picked: Option<&'static str> = None;

    // Keyboard input — Up / Down / Enter / Escape. Consumed
    // before the palette body draws so the text field doesn't
    // swallow them.
    ctx.input_mut(|i| {
        if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
            state.open = false;
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) && !filtered.is_empty() {
            state.selected = (state.selected + 1).min(filtered.len() - 1);
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
            state.selected = state.selected.saturating_sub(1);
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) && !filtered.is_empty() {
            picked = Some(filtered[state.selected].id);
        }
    });

    let screen = ctx.content_rect();
    // Full-screen scrim so clicks outside the palette dismiss it.
    // `Order::Foreground` places it above panes, below the
    // palette itself (which we paint at `Tooltip`).
    let scrim_clicked = egui::Area::new(egui::Id::new("mara_palette_scrim"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            ui.allocate_exact_size(screen.size(), egui::Sense::click())
        })
        .inner
        .1
        .clicked();
    if scrim_clicked {
        state.open = false;
    }

    // Palette window — centred, fixed width, content-driven
    // height. Painted at `Order::Tooltip` so it sits above the
    // scrim.
    //
    // The Area + inner ScrollArea IDs fold in a **content
    // fingerprint** of the item slice — a hash of every item id —
    // so switching between palette contexts (e.g. graph-maximised
    // palette vs. general palette) gives the new context a fresh
    // Area / ScrollArea identity instead of re-using the previous
    // context's remembered dimensions. Without this, going from a
    // 3-item graph palette back to the 11-item general palette
    // would stay "tight" for a frame because egui remembered the
    // smaller content size from the previous render.
    let items_sig = items_fingerprint(items);
    const WIDTH: f32 = 560.0;
    let pos = egui::pos2(
        screen.center().x - WIDTH * 0.5,
        screen.min.y + screen.height() * 0.22,
    );
    egui::Area::new(egui::Id::new(("mara_palette", items_sig)))
        .order(egui::Order::Tooltip)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            ui.set_max_width(WIDTH);
            let frame = egui::Frame::new()
                .fill(glass_fill(
                    crate::style::popup_fill(accent),
                    accent,
                    glass_alpha_window(),
                ))
                .stroke(egui::Stroke::new(
                    crate::style::theme().border_width,
                    widget_border(accent),
                ))
                .corner_radius(egui::CornerRadius::same(crate::style::theme().radius_lg))
                .inner_margin(egui::Margin::symmetric(8, 6))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 10],
                    blur: 28,
                    spread: 0,
                    color: egui::Color32::from_black_alpha(150),
                });
            let frame_inner = frame.show(ui, |ui| {
                ui.set_width(WIDTH - 16.0);
                // Search input — themes that filled the section
                // title strip with accent (GAME) get the same
                // treatment here: full accent background, contrast
                // text on top, accent-darkened hint text. PRO falls
                // back to the original raised glass fill.
                let theme_now = crate::style::theme();
                let (input_bg, input_text_col, hint_col) = if theme_now.title_strip_filled {
                    let bg = accent;
                    let text = crate::style::contrast_text_for(bg);
                    let hint =
                        egui::Color32::from_rgba_unmultiplied(text.r(), text.g(), text.b(), 160);
                    (bg, text, hint)
                } else {
                    (
                        glass_fill(crate::style::theme().bg_raised, accent, glass_alpha_card()),
                        crate::style::on_section(),
                        crate::style::on_section_dim(),
                    )
                };
                let hint_text = egui::WidgetText::from(
                    egui::RichText::new("Type a command…")
                        .color(hint_col)
                        .size(13.0),
                );
                let edit = egui::TextEdit::singleline(&mut state.query)
                    .desired_width(f32::INFINITY)
                    .frame(true)
                    .hint_text(hint_text)
                    .text_color(input_text_col)
                    .background_color(input_bg)
                    .font(egui::FontId::proportional(13.0));
                let edit_resp = ui.add(edit);
                if edit_resp.changed() {
                    // Query changed — reset selection to the top
                    // of the filtered list so the highlight stays
                    // sensible.
                    state.selected = 0;
                }
                // Focus the text field the frame the palette
                // opens so the user can type immediately.
                if !edit_resp.has_focus() {
                    edit_resp.request_focus();
                }

                ui.add_space(4.0);

                // Dashed separator between the input and the result
                // list — matches the row-separator language used
                // inside section bodies in the GAME theme. PRO falls
                // back to the existing 4 px gap (the dash recipe is
                // None there).
                if let Some((on, off)) = crate::style::theme().row_separator_dash {
                    let w = ui.available_width();
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(w, 1.0), egui::Sense::hover());
                    // Input-to-results divider — kit-shared
                    // `outline_base` + `row_separator_alpha`. Drops
                    // the previous `.max(60)` alpha floor and the
                    // raw `border_subtle` lookup; both were the
                    // pre-unification fallback path.
                    let alpha = crate::style::theme().row_separator_alpha;
                    let base = crate::style::outline_base();
                    let col =
                        egui::Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), alpha);
                    crate::style::paint_dashed_line(
                        ui.painter(),
                        rect.left_center(),
                        rect.right_center(),
                        on,
                        off,
                        egui::Stroke::new(1.0, col),
                    );
                    ui.add_space(4.0);
                }

                // Results list.
                egui::ScrollArea::vertical()
                    .id_salt(("mara_palette_list", items_sig))
                    .auto_shrink([false, true])
                    .max_height(320.0)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        if filtered.is_empty() {
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label(
                                    egui::RichText::new("No matches")
                                        .color(crate::style::on_section_dim())
                                        .size(font::BODY),
                                );
                            });
                        }
                        let dash = crate::style::theme().row_separator_dash;
                        let row_alpha = crate::style::theme().row_separator_alpha;
                        // Use kit-shared `outline_base` so the
                        // inter-item rule matches every other row
                        // separator across the kit.
                        let row_base = crate::style::outline_base();
                        for (i, it) in filtered.iter().enumerate() {
                            if paint_row(ui, it, i == state.selected, accent).clicked() {
                                picked = Some(it.id);
                            }
                            // Dashed inter-item rule — only in themes
                            // that opted into dashed row separators
                            // (GAME). PRO continues with the implicit
                            // `item_spacing.y` gap.
                            if let Some((on, off)) = dash
                                && i + 1 < filtered.len()
                                && row_alpha > 0
                            {
                                let w = ui.available_width();
                                let (rect, _) = ui
                                    .allocate_exact_size(egui::vec2(w, 1.0), egui::Sense::hover());
                                let col = egui::Color32::from_rgba_unmultiplied(
                                    row_base.r(),
                                    row_base.g(),
                                    row_base.b(),
                                    row_alpha,
                                );
                                crate::style::paint_dashed_line(
                                    ui.painter(),
                                    rect.left_center(),
                                    rect.right_center(),
                                    on,
                                    off,
                                    egui::Stroke::new(1.0, col),
                                );
                            }
                        }
                    });
            });

            // L-bracket corner ticks at the palette's four corners,
            // matching the section-header language. Theme-gated via
            // `section_corner_ticks`; PRO sets it to `0.0` so this
            // is a no-op there.
            let tick_len = crate::style::theme().section_corner_ticks;
            if tick_len > 0.0 {
                let r = frame_inner.response.rect;
                let inset = crate::style::theme().section_corner_ticks_inset;
                let r = if inset > 0.0 { r.shrink(inset) } else { r };
                let snap_low = |v: f32| v.round() + 0.5;
                let snap_high = |v: f32| v.round() - 0.5;
                let lx = snap_low(r.min.x);
                let ty = snap_low(r.min.y);
                let rx = snap_high(r.max.x);
                let by = snap_high(r.max.y);
                let len = tick_len;
                let col =
                    egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 220);
                let s = egui::Stroke::new(1.0, col);
                let p = ui.painter();
                p.line_segment([egui::pos2(lx, ty), egui::pos2(lx + len, ty)], s);
                p.line_segment([egui::pos2(lx, ty), egui::pos2(lx, ty + len)], s);
                p.line_segment([egui::pos2(rx - len, ty), egui::pos2(rx, ty)], s);
                p.line_segment([egui::pos2(rx, ty), egui::pos2(rx, ty + len)], s);
                p.line_segment([egui::pos2(lx, by - len), egui::pos2(lx, by)], s);
                p.line_segment([egui::pos2(lx, by), egui::pos2(lx + len, by)], s);
                p.line_segment([egui::pos2(rx - len, by), egui::pos2(rx, by)], s);
                p.line_segment([egui::pos2(rx, by - len), egui::pos2(rx, by)], s);
            }
        });

    if picked.is_some() {
        state.open = false;
        state.query.clear();
        state.selected = 0;
    }

    picked
}

/// Paint one row: label on the left, optional dim hint on the
/// right. Selected row gets an accent-tinted fill so keyboard
/// navigation is visible.
fn paint_row(
    ui: &mut egui::Ui,
    item: &PaletteItem,
    selected: bool,
    accent: egui::Color32,
) -> egui::Response {
    const ROW_H: f32 = 24.0;
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, ROW_H), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let bg = if selected {
            Some(crate::style::row_selected_fill(accent))
        } else if resp.hovered() {
            Some(crate::style::row_hover_fill(accent))
        } else {
            None
        };
        if let Some(c) = bg {
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(crate::style::theme().radius_md),
                c,
            );
        }
        let mid_y = rect.center().y;
        // Palette rows sit on the palette frame (panel-style fill).
        // Selected/hovered rows are accent-blended; pick contrast
        // against whatever the row ended up coloured.
        let row_bg = bg.unwrap_or(crate::style::pane_fill(accent));
        let row_text = crate::style::contrast_text_for(row_bg);
        let row_text_dim = crate::style::contrast_text_for(row_bg);
        ui.painter().text(
            egui::pos2(rect.min.x + 10.0, mid_y),
            egui::Align2::LEFT_CENTER,
            item.label,
            egui::FontId::proportional(font::BODY + 2.0),
            row_text,
        );
        if let Some(hint) = item.hint {
            ui.painter().text(
                egui::pos2(rect.max.x - 10.0, mid_y),
                egui::Align2::RIGHT_CENTER,
                hint,
                egui::FontId::proportional(font::CAPTION),
                row_text_dim,
            );
        }
    }
    resp
}

/// Fold every item's static `id` into a single `u64`. Used as
/// an Area / ScrollArea id discriminator so egui's cached sizes
/// / scroll offsets for one palette context don't bleed into a
/// different context the next frame.
fn items_fingerprint(items: &[PaletteItem]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    items.len().hash(&mut h);
    for it in items {
        it.id.hash(&mut h);
    }
    h.finish()
}

fn validate_palette_items(items: &[PaletteItem]) {
    let mut seen = HashSet::with_capacity(items.len());
    for item in items {
        assert_palette_text(item.id, "command palette items require a non-empty id");
        assert_palette_text(
            item.label,
            "command palette items require a non-empty label",
        );
        if let Some(hint) = item.hint {
            assert_palette_text(
                hint,
                "command palette item hints must be non-empty when provided",
            );
        }
        assert!(
            seen.insert(item.id),
            "command palette items require unique ids"
        );
    }
}

fn assert_palette_text(value: &str, message: &str) {
    assert!(!value.trim().is_empty(), "{message}");
}

/// Substring + initials match. Returns true if the LOWERCASE
/// `label` contains `q` as a substring, OR if `q` matches the
/// initials of the label's whitespace-separated tokens.
fn matches(label: &str, q: &str) -> bool {
    let lower = label.to_lowercase();
    if lower.contains(q) {
        return true;
    }
    // Build initials: first char of each alphabetic token.
    let initials: String = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter_map(|w| w.chars().next())
        .collect();
    if initials.contains(q) {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_items_require_visible_metadata() {
        let blank_id = std::panic::catch_unwind(|| {
            let _ = PaletteItem::new(" ", "Open");
        });
        let blank_label = std::panic::catch_unwind(|| {
            let _ = PaletteItem::new("open", " ");
        });
        let blank_hint = std::panic::catch_unwind(|| {
            let _ = PaletteItem::new("open", "Open").with_hint(" ");
        });
        let valid = PaletteItem::new("open", "Open").with_hint("Ctrl+O");

        assert!(blank_id.is_err());
        assert!(blank_label.is_err());
        assert!(blank_hint.is_err());
        assert_eq!(valid.hint, Some("Ctrl+O"));
    }

    #[test]
    fn palette_validation_rejects_duplicate_or_directly_invalid_items() {
        let duplicate = std::panic::catch_unwind(|| {
            validate_palette_items(&[
                PaletteItem::new("open", "Open"),
                PaletteItem::new("open", "Open Again"),
            ]);
        });
        let direct_invalid = std::panic::catch_unwind(|| {
            validate_palette_items(&[PaletteItem {
                id: "direct-invalid",
                label: "",
                hint: None,
            }]);
        });

        assert!(duplicate.is_err());
        assert!(direct_invalid.is_err());
    }
}
