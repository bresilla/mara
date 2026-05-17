//! Title-strip painter for [`super::Pane`]. Theme-aware (PRO solid
//! accent vs GAME caution stripes), with text alignment + blinking
//! pip placement driven by the pane's [`PaneAnchor`].

use egui::{Color32, Id, Rect, pos2};

use super::anchor::{PaneAnchor, TitleSide};
use crate::style;

/// Paint the title strip background + text inside `rect`. Five
/// pieces:
///
/// 1. Background: theme-driven panel fill (PRO) or animated
///    caution stripes (GAME) restricted to the title rect.
/// 2. Title text: scramble-decoded when `scramble_titles` is on,
///    aligned per anchor (centred for Middle zones, reversed for
///    TS / RS / RE / BE so the first letter sits next to the
///    pane's own button).
/// 3. Blinking pip(s) — single pip opposite the text on corner
///    anchors; two pips (one each end) for Middle anchors.
/// 4. Divider hairline on the body-facing edge of the strip
///    (`pane_show_title_divider`).
pub(crate) fn paint_pane_title(
    ui: &mut egui::Ui,
    rect: Rect,
    id: Id,
    title: &str,
    anchor: PaneAnchor,
    accent: Color32,
) {
    const TITLE_INSET: f32 = 8.0;
    const PIP_SIZE: f32 = 6.0;
    let title_size = 15.0 * 1.15;
    let theme = style::theme();
    let stripes_on = theme.pane.title_stripes;

    // ── 1. Background ──
    if !theme.pane.fill_visible && !stripes_on {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(theme.radius_lg),
            style::pane_fill(accent),
        );
    }
    if stripes_on {
        style::paint_caution_stripes(ui.painter(), rect, accent);
    }

    // ── 2. Title text colour + content ──
    let text_col = if stripes_on {
        if theme.is_light {
            Color32::BLACK
        } else {
            Color32::WHITE
        }
    } else {
        style::section_title_color(accent)
    };
    let title_family = style::title_font_family();
    let title_family = if ui.fonts(|fonts| fonts.families().contains(&title_family)) {
        title_family
    } else {
        egui::FontFamily::Proportional
    };
    let font = egui::FontId::new(title_size, title_family);
    let title_uc = if theme.pane.title_brackets {
        format!("[ {} ]", title.to_uppercase())
    } else {
        title.to_uppercase()
    };
    // Compute the scramble id up-front (always — even when
    // `scramble_titles` is off) so the chromatic-aberration helper
    // below can ask whether the cipher is currently running.
    let session_id = id.with("pane2_title_session");
    let session = style::appearance_session(ui.ctx(), session_id);
    let scramble_id = session_id.with(session);
    let displayed = if theme.scramble_titles {
        let scrambled = style::scramble_text(ui.ctx(), scramble_id, &title_uc, true);
        // Same periodic single-letter glitch the container title
        // uses — keeps the pane title alive after its decode cycle.
        style::glitch_text(ui.ctx(), session_id.with("glitch"), &scrambled)
    } else {
        title_uc.clone()
    };

    let title_side = anchor.title_side();
    let is_horizontal_strip = title_side.is_horizontal_strip();
    let reversed = anchor.title_reversed();
    let centred = anchor.is_middle();

    // ── 3. Title text paint ──
    // Chromatic-aberration ghosts (gated on
    // `theme.pane_title_chromatic_aberration`). Two contributing
    // sources, taken as the MAX so they layer naturally:
    //
    //   • Periodic firing — `chromatic_aberration_offset` runs a
    //     0→peak→0 triangular pulse every 5–13 s. Always on while
    //     the flag is set.
    //   • Cipher-driven — while the title's scramble cycle is still
    //     decoding, paint a continuously pulsing offset so the
    //     aberration reads as ON throughout the cipher rather than
    //     just at random moments. Modulated by a fast sine for a
    //     CRT-misregistration shimmer.
    //
    // PRO leaves the flag false → both branches collapse to 0.0.
    let aberration = if theme.pane.title_chromatic_aberration {
        let periodic = style::chromatic_aberration_offset(ui.ctx(), id.with("chrom_aberr"));
        let cipher_offset =
            if theme.scramble_titles && style::scramble_active(ui.ctx(), scramble_id, &title_uc) {
                const CIPHER_PEAK: f32 = 6.0;
                let now = ui.ctx().input(|i| i.time) as f32;
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(33));
                let pulse = (now * 32.0).sin().abs();
                CIPHER_PEAK * (0.55 + 0.45 * pulse)
            } else {
                0.0
            };
        periodic.max(cipher_offset)
    } else {
        0.0
    };
    let chr_red = Color32::from_rgb(220, 60, 70);
    let chr_cyan = Color32::from_rgb(60, 220, 230);

    if is_horizontal_strip {
        let (pos, align) = if centred {
            (rect.center(), egui::Align2::CENTER_CENTER)
        } else if reversed {
            (
                pos2((rect.max.x - TITLE_INSET).round(), rect.center().y.round()),
                egui::Align2::RIGHT_CENTER,
            )
        } else {
            (
                pos2((rect.min.x + TITLE_INSET).round(), rect.center().y.round()),
                egui::Align2::LEFT_CENTER,
            )
        };
        if aberration > 0.0 {
            // Horizontal strip: text reads along screen-X, so the
            // chromatic split runs along X. Tiny ±1 px cross jitter
            // (Y) on each ghost for a touch of CRT-misregistration
            // grit without smearing the glyph height.
            const CROSS_JITTER: f32 = 1.0;
            ui.painter().text(
                pos2(pos.x - aberration, pos.y - CROSS_JITTER),
                align,
                &displayed,
                font.clone(),
                chr_red,
            );
            ui.painter().text(
                pos2(pos.x + aberration, pos.y + CROSS_JITTER),
                align,
                &displayed,
                font.clone(),
                chr_cyan,
            );
        }
        ui.painter().text(pos, align, displayed, font, text_col);
    } else {
        // Vertical strip: lay out a single galley with placeholder
        // colour so the same `Arc<Galley>` can drive three
        // `TextShape`s tinted differently for the aberration ghosts
        // and the main text. Cheap clone (Arc bump) instead of
        // re-laying out three separate galleys.
        let galley = ui
            .painter()
            .layout_no_wrap(displayed, font, Color32::PLACEHOLDER);
        let g = galley.size();
        let cx = rect.center().x;
        let on_right_side = title_side == TitleSide::Right;
        let top_to_bottom = on_right_side ^ reversed;
        let (text_pos, angle) = if centred {
            if top_to_bottom {
                (
                    pos2(
                        (cx + g.y * 0.5).round(),
                        (rect.center().y - g.x * 0.5).round(),
                    ),
                    std::f32::consts::FRAC_PI_2,
                )
            } else {
                (
                    pos2(
                        (cx - g.y * 0.5).round(),
                        (rect.center().y + g.x * 0.5).round(),
                    ),
                    -std::f32::consts::FRAC_PI_2,
                )
            }
        } else if top_to_bottom {
            (
                pos2((cx + g.y * 0.5).round(), (rect.min.y + TITLE_INSET).round()),
                std::f32::consts::FRAC_PI_2,
            )
        } else {
            (
                pos2((cx - g.y * 0.5).round(), (rect.max.y - TITLE_INSET).round()),
                -std::f32::consts::FRAC_PI_2,
            )
        };
        // For rotated text (vertical-strip pane title) the reading
        // direction is screen-Y, so the chromatic split must run
        // along Y to land along the text's length, not across its
        // height. Tiny ±1 px cross jitter (X) on each ghost adds a
        // CRT-misregistration touch without distorting the glyph
        // height after rotation.
        if aberration > 0.0 {
            const CROSS_JITTER: f32 = 1.0;
            let r_pos = pos2(text_pos.x - CROSS_JITTER, text_pos.y - aberration);
            let c_pos = pos2(text_pos.x + CROSS_JITTER, text_pos.y + aberration);
            let mut s_red = egui::epaint::TextShape::new(r_pos, galley.clone(), chr_red);
            s_red.angle = angle;
            ui.painter().add(s_red);
            let mut s_cyan = egui::epaint::TextShape::new(c_pos, galley.clone(), chr_cyan);
            s_cyan.angle = angle;
            ui.painter().add(s_cyan);
        }
        let mut shape = egui::epaint::TextShape::new(text_pos, galley, text_col);
        shape.angle = angle;
        ui.painter().add(shape);
    }

    // ── 4. Blinking pip(s) (GAME only) ──
    if stripes_on {
        const PIP_INSET: f32 = TITLE_INSET;
        // Per-second blink — `ON_FRAC` controls how long the pip
        // stays bright at the start of each cycle. Bumped from 0.08
        // so the on-state lingers a touch
        // longer and reads clearly between dims.
        let time = ui.ctx().input(|i| i.time) as f32;
        const ON_FRAC: f32 = 0.16;
        let on = time.fract() < ON_FRAC;
        let alpha = if on { 255 } else { 76 };
        let pip_color =
            Color32::from_rgba_unmultiplied(text_col.r(), text_col.g(), text_col.b(), alpha);
        // Every 3rd "ON" pulse, split the pip into red + cyan ghosts
        // for a CRT-misregistration flash. Phase aligns with the
        // 1 Hz blink (time.floor() ticks once per cycle).
        let pulse_idx = time.floor() as i64;
        let chromatic_pulse = on && pulse_idx.rem_euclid(3) == 0;
        let paint_pip = |r: Rect| {
            if chromatic_pulse {
                const CHROM_OFFSET: f32 = 2.0;
                let chrom_red = Color32::from_rgba_unmultiplied(220, 60, 70, 200);
                let chrom_cyan = Color32::from_rgba_unmultiplied(60, 220, 230, 200);
                let (off_red, off_cyan) = if is_horizontal_strip {
                    (
                        egui::vec2(-CHROM_OFFSET, 0.0),
                        egui::vec2(CHROM_OFFSET, 0.0),
                    )
                } else {
                    (
                        egui::vec2(0.0, -CHROM_OFFSET),
                        egui::vec2(0.0, CHROM_OFFSET),
                    )
                };
                ui.painter()
                    .rect_filled(r.translate(off_red), egui::CornerRadius::ZERO, chrom_red);
                ui.painter().rect_filled(
                    r.translate(off_cyan),
                    egui::CornerRadius::ZERO,
                    chrom_cyan,
                );
            }
            ui.painter()
                .rect_filled(r, egui::CornerRadius::ZERO, pip_color);
        };

        if is_horizontal_strip {
            let cy = (rect.center().y - PIP_SIZE * 0.5).round();
            let right_x = (rect.max.x - PIP_INSET - PIP_SIZE).round();
            let left_x = (rect.min.x + PIP_INSET).round();
            if centred {
                paint_pip(Rect::from_min_size(
                    pos2(left_x, cy),
                    egui::vec2(PIP_SIZE, PIP_SIZE),
                ));
                paint_pip(Rect::from_min_size(
                    pos2(right_x, cy),
                    egui::vec2(PIP_SIZE, PIP_SIZE),
                ));
            } else if reversed {
                paint_pip(Rect::from_min_size(
                    pos2(left_x, cy),
                    egui::vec2(PIP_SIZE, PIP_SIZE),
                ));
            } else {
                paint_pip(Rect::from_min_size(
                    pos2(right_x, cy),
                    egui::vec2(PIP_SIZE, PIP_SIZE),
                ));
            }
        } else {
            let cx = (rect.center().x - PIP_SIZE * 0.5).round();
            let top_y = (rect.min.y + PIP_INSET).round();
            let bottom_y = (rect.max.y - PIP_INSET - PIP_SIZE).round();
            let on_right_side = title_side == TitleSide::Right;
            let top_to_bottom = on_right_side ^ reversed;
            if centred {
                paint_pip(Rect::from_min_size(
                    pos2(cx, top_y),
                    egui::vec2(PIP_SIZE, PIP_SIZE),
                ));
                paint_pip(Rect::from_min_size(
                    pos2(cx, bottom_y),
                    egui::vec2(PIP_SIZE, PIP_SIZE),
                ));
            } else if top_to_bottom {
                paint_pip(Rect::from_min_size(
                    pos2(cx, bottom_y),
                    egui::vec2(PIP_SIZE, PIP_SIZE),
                ));
            } else {
                paint_pip(Rect::from_min_size(
                    pos2(cx, top_y),
                    egui::vec2(PIP_SIZE, PIP_SIZE),
                ));
            }
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }

    // ── 5. Divider hairline on the body-facing edge ──
    if theme.pane.show_title_divider {
        let stroke = style::stroke_for(style::StrokeRole::WidgetBorder, accent);
        match title_side {
            TitleSide::Top => {
                ui.painter()
                    .hline(rect.min.x..=rect.max.x, rect.max.y - 0.5, stroke);
            }
            TitleSide::Bottom => {
                ui.painter()
                    .hline(rect.min.x..=rect.max.x, rect.min.y + 0.5, stroke);
            }
            TitleSide::Left => {
                ui.painter()
                    .vline(rect.max.x - 0.5, rect.min.y..=rect.max.y, stroke);
            }
            TitleSide::Right => {
                ui.painter()
                    .vline(rect.min.x + 0.5, rect.min.y..=rect.max.y, stroke);
            }
        }
    }
}
