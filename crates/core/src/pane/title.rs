//! Title-strip painter for [`super::Pane`]. Theme-aware (PRO solid
//! accent vs GAME caution stripes), with text alignment + blinking
//! pip placement driven by the pane's [`PaneAnchor`].
//!
//! Fully sealed: the module names no backend type. Drawing goes
//! through [`MaraUi`](crate::MaraUi)/[`MaraPainter`](crate::MaraPainter)
//! as [`PaintCmd`] values, and the time/animation/scramble effects read
//! frame state through [`MaraCtx`](crate::context::MaraCtx), which the
//! caller passes in — a painting surface does not vend a context.
//!
//! `make check` enforces the seal by banning any backend path in
//! this file.

use crate::vocab::{Color32, Id, Rect};

use super::anchor::{PaneAnchor, TitleSide};
use crate::paint::{PaintCmd, TextRun};
use crate::style;
use crate::vocab::{
    Align2 as MaraAlign2, Color32 as MaraColor32, CornerRadius as MaraCornerRadius,
    Pos2 as MaraPos2, Rect as MaraRect, Vec2 as MaraVec2,
};

/// Paint one blinking pip rectangle, optionally with a red/cyan
/// chromatic-aberration flash split along the strip's reading axis.
fn paint_pip(
    ui: &mut crate::MaraUi<'_>,
    r: MaraRect,
    chromatic_pulse: bool,
    is_horizontal_strip: bool,
    pip_color: MaraColor32,
) {
    if chromatic_pulse {
        const CHROM_OFFSET: f32 = 2.0;
        let chrom_red = MaraColor32::from_rgba_unmultiplied(220, 60, 70, 200);
        let chrom_cyan = MaraColor32::from_rgba_unmultiplied(60, 220, 230, 200);
        let (off_red, off_cyan) = if is_horizontal_strip {
            (
                MaraVec2::new(-CHROM_OFFSET, 0.0),
                MaraVec2::new(CHROM_OFFSET, 0.0),
            )
        } else {
            (
                MaraVec2::new(0.0, -CHROM_OFFSET),
                MaraVec2::new(0.0, CHROM_OFFSET),
            )
        };
        ui.painter().paint_cmd(
            PaintCmd::RectFilled {
                rect: r.translate(off_red),
                corner: MaraCornerRadius::ZERO,
                fill: chrom_red,
            },
        );
        ui.painter().paint_cmd(
            PaintCmd::RectFilled {
                rect: r.translate(off_cyan),
                corner: MaraCornerRadius::ZERO,
                fill: chrom_cyan,
            },
        );
    }
    ui.painter().paint_cmd(
        PaintCmd::RectFilled {
            rect: r,
            corner: MaraCornerRadius::ZERO,
            fill: pip_color,
        },
    );
}

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
    ui: &mut crate::MaraUi<'_>,
    // Theme animation state lives on the context, which a surface does
    // not vend; pass it rather than widen `UiBackend` for one caller.
    ctx: &dyn crate::context::MaraCtx,
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
    let m_rect: MaraRect = rect.into();

    // ── 1. Background ──
    if !theme.pane.fill_visible && !stripes_on {
        ui.painter().paint_cmd(
            PaintCmd::RectFilled {
                rect: m_rect,
                corner: MaraCornerRadius::same(theme.radius_lg),
                fill: style::pane_fill(accent),
            },
        );
    }
    if stripes_on {
        // Stripes are visible — keep animating. Without this egui only
        // repaints on input events and the strip would appear frozen.
        ui.request_repaint_after(std::time::Duration::from_millis(16));
        let time_s = ui.now() as f32;
        if let Some(cmd) = style::caution_stripes_paint_cmd(m_rect, accent, time_s) {
            ui.painter().paint_cmd(cmd);
        }
    }

    // ── 2. Title text colour + content ──
    let text_col: MaraColor32 = if stripes_on {
        if theme.is_light {
            MaraColor32::BLACK
        } else {
            MaraColor32::WHITE
        }
    } else {
        style::section_title_color(accent)
    };
    let title_family = style::title_font_family();
    let title_uc = if theme.pane.title_brackets {
        format!("[ {} ]", title.to_uppercase())
    } else {
        title.to_uppercase()
    };
    // Compute the scramble id up-front (always — even when
    // `scramble_titles` is off) so the chromatic-aberration helper
    // below can ask whether the cipher is currently running.
    let session_id = id.with("pane2_title_session");
    let session = style::appearance_session(ctx, session_id);
    let scramble_id = session_id.with(session);
    let displayed = if theme.scramble_titles {
        let scrambled = style::scramble_text(ctx, scramble_id, &title_uc, true);
        // Same periodic single-letter glitch the container title
        // uses — keeps the pane title alive after its decode cycle.
        style::glitch_text(ctx, session_id.with("glitch"), &scrambled)
    } else {
        title_uc.clone()
    };

    // Build the title text run(s) — a single proportional/named run
    // tinted by `color`. Reused for the main text and the red/cyan
    // chromatic-aberration ghosts.
    let make_runs = |color: MaraColor32| -> Vec<TextRun> {
        vec![TextRun {
            text: displayed.clone(),
            size: title_size,
            color,
            family: title_family.clone(),
            extra_letter_spacing: 0.0,
            leading_space: 0.0,
        }]
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
        let periodic = style::chromatic_aberration_offset(ctx, id.with("chrom_aberr"));
        let cipher_offset =
            if theme.scramble_titles && style::scramble_active(ctx, scramble_id, &title_uc) {
                const CIPHER_PEAK: f32 = 6.0;
                let now = ui.now() as f32;
                ui.request_repaint_after(std::time::Duration::from_millis(33));
                let pulse = (now * 32.0).sin().abs();
                CIPHER_PEAK * (0.55 + 0.45 * pulse)
            } else {
                0.0
            };
        periodic.max(cipher_offset)
    } else {
        0.0
    };
    let chr_red = MaraColor32::from_rgb(220, 60, 70);
    let chr_cyan = MaraColor32::from_rgb(60, 220, 230);

    if is_horizontal_strip {
        let (pos, align) = if centred {
            (m_rect.center(), MaraAlign2::CENTER_CENTER)
        } else if reversed {
            (
                MaraPos2::new(
                    (m_rect.max.x - TITLE_INSET).round(),
                    m_rect.center().y.round(),
                ),
                MaraAlign2::RIGHT_CENTER,
            )
        } else {
            (
                MaraPos2::new(
                    (m_rect.min.x + TITLE_INSET).round(),
                    m_rect.center().y.round(),
                ),
                MaraAlign2::LEFT_CENTER,
            )
        };
        if aberration > 0.0 {
            // Horizontal strip: text reads along screen-X, so the
            // chromatic split runs along X. Tiny ±1 px cross jitter
            // (Y) on each ghost for a touch of CRT-misregistration
            // grit without smearing the glyph height.
            const CROSS_JITTER: f32 = 1.0;
            ui.painter().paint_cmd(
                PaintCmd::TextRuns {
                    pos: MaraPos2::new(pos.x - aberration, pos.y - CROSS_JITTER),
                    anchor: align,
                    angle: 0.0,
                    runs: make_runs(chr_red),
                },
            );
            ui.painter().paint_cmd(
                PaintCmd::TextRuns {
                    pos: MaraPos2::new(pos.x + aberration, pos.y + CROSS_JITTER),
                    anchor: align,
                    angle: 0.0,
                    runs: make_runs(chr_cyan),
                },
            );
        }
        ui.painter().paint_cmd(
            PaintCmd::TextRuns {
                pos,
                anchor: align,
                angle: 0.0,
                runs: make_runs(text_col),
            },
        );
    } else {
        // Vertical strip: a single rotated text run drives the main
        // glyphs; the aberration ghosts reuse the same run tinted
        // red/cyan. `measure_text_runs_for_ui` gives the laid-out
        // size so the rotated origin can be centred on the strip.
        let g: MaraVec2 = ui.painter().measure_text_runs(&make_runs(text_col));
        let cx = m_rect.center().x;
        let on_right_side = title_side == TitleSide::Right;
        let top_to_bottom = on_right_side ^ reversed;
        let (text_pos, angle) = if centred {
            if top_to_bottom {
                (
                    MaraPos2::new(
                        (cx + g.y * 0.5).round(),
                        (m_rect.center().y - g.x * 0.5).round(),
                    ),
                    std::f32::consts::FRAC_PI_2,
                )
            } else {
                (
                    MaraPos2::new(
                        (cx - g.y * 0.5).round(),
                        (m_rect.center().y + g.x * 0.5).round(),
                    ),
                    -std::f32::consts::FRAC_PI_2,
                )
            }
        } else if top_to_bottom {
            (
                MaraPos2::new(
                    (cx + g.y * 0.5).round(),
                    (m_rect.min.y + TITLE_INSET).round(),
                ),
                std::f32::consts::FRAC_PI_2,
            )
        } else {
            (
                MaraPos2::new(
                    (cx - g.y * 0.5).round(),
                    (m_rect.max.y - TITLE_INSET).round(),
                ),
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
            ui.painter().paint_cmd(
                PaintCmd::TextRuns {
                    pos: MaraPos2::new(text_pos.x - CROSS_JITTER, text_pos.y - aberration),
                    anchor: MaraAlign2::LEFT_TOP,
                    angle,
                    runs: make_runs(chr_red),
                },
            );
            ui.painter().paint_cmd(
                PaintCmd::TextRuns {
                    pos: MaraPos2::new(text_pos.x + CROSS_JITTER, text_pos.y + aberration),
                    anchor: MaraAlign2::LEFT_TOP,
                    angle,
                    runs: make_runs(chr_cyan),
                },
            );
        }
        ui.painter().paint_cmd(
            PaintCmd::TextRuns {
                pos: text_pos,
                anchor: MaraAlign2::LEFT_TOP,
                angle,
                runs: make_runs(text_col),
            },
        );
    }

    // ── 4. Blinking pip(s) (GAME only) ──
    if stripes_on {
        const PIP_INSET: f32 = TITLE_INSET;
        // Per-second blink — `ON_FRAC` controls how long the pip
        // stays bright at the start of each cycle. Bumped from 0.08
        // so the on-state lingers a touch
        // longer and reads clearly between dims.
        let time = ui.now() as f32;
        const ON_FRAC: f32 = 0.16;
        let on = time.fract() < ON_FRAC;
        let alpha = if on { 255 } else { 76 };
        let pip_color =
            MaraColor32::from_rgba_unmultiplied(text_col.r(), text_col.g(), text_col.b(), alpha);
        // Every 3rd "ON" pulse, split the pip into red + cyan ghosts
        // for a CRT-misregistration flash. Phase aligns with the
        // 1 Hz blink (time.floor() ticks once per cycle).
        let pulse_idx = time.floor() as i64;
        let chromatic_pulse = on && pulse_idx.rem_euclid(3) == 0;

        if is_horizontal_strip {
            let cy = (m_rect.center().y - PIP_SIZE * 0.5).round();
            let right_x = (m_rect.max.x - PIP_INSET - PIP_SIZE).round();
            let left_x = (m_rect.min.x + PIP_INSET).round();
            let pip_size = MaraVec2::new(PIP_SIZE, PIP_SIZE);
            if centred {
                paint_pip(
                    ui,
                    MaraRect::from_min_size(MaraPos2::new(left_x, cy), pip_size),
                    chromatic_pulse,
                    is_horizontal_strip,
                    pip_color,
                );
                paint_pip(
                    ui,
                    MaraRect::from_min_size(MaraPos2::new(right_x, cy), pip_size),
                    chromatic_pulse,
                    is_horizontal_strip,
                    pip_color,
                );
            } else if reversed {
                paint_pip(
                    ui,
                    MaraRect::from_min_size(MaraPos2::new(left_x, cy), pip_size),
                    chromatic_pulse,
                    is_horizontal_strip,
                    pip_color,
                );
            } else {
                paint_pip(
                    ui,
                    MaraRect::from_min_size(MaraPos2::new(right_x, cy), pip_size),
                    chromatic_pulse,
                    is_horizontal_strip,
                    pip_color,
                );
            }
        } else {
            let cx = (m_rect.center().x - PIP_SIZE * 0.5).round();
            let top_y = (m_rect.min.y + PIP_INSET).round();
            let bottom_y = (m_rect.max.y - PIP_INSET - PIP_SIZE).round();
            let on_right_side = title_side == TitleSide::Right;
            let top_to_bottom = on_right_side ^ reversed;
            let pip_size = MaraVec2::new(PIP_SIZE, PIP_SIZE);
            if centred {
                paint_pip(
                    ui,
                    MaraRect::from_min_size(MaraPos2::new(cx, top_y), pip_size),
                    chromatic_pulse,
                    is_horizontal_strip,
                    pip_color,
                );
                paint_pip(
                    ui,
                    MaraRect::from_min_size(MaraPos2::new(cx, bottom_y), pip_size),
                    chromatic_pulse,
                    is_horizontal_strip,
                    pip_color,
                );
            } else if top_to_bottom {
                paint_pip(
                    ui,
                    MaraRect::from_min_size(MaraPos2::new(cx, bottom_y), pip_size),
                    chromatic_pulse,
                    is_horizontal_strip,
                    pip_color,
                );
            } else {
                paint_pip(
                    ui,
                    MaraRect::from_min_size(MaraPos2::new(cx, top_y), pip_size),
                    chromatic_pulse,
                    is_horizontal_strip,
                    pip_color,
                );
            }
        }
        ui.request_repaint_after(std::time::Duration::from_millis(33));
    }

    // ── 5. Divider hairline on the body-facing edge ──
    if theme.pane.show_title_divider {
        let stroke = style::stroke_for(style::StrokeRole::WidgetBorder, accent);
        let line = match title_side {
            TitleSide::Top => PaintCmd::Line {
                a: MaraPos2::new(m_rect.min.x, m_rect.max.y - 0.5),
                b: MaraPos2::new(m_rect.max.x, m_rect.max.y - 0.5),
                stroke,
            },
            TitleSide::Bottom => PaintCmd::Line {
                a: MaraPos2::new(m_rect.min.x, m_rect.min.y + 0.5),
                b: MaraPos2::new(m_rect.max.x, m_rect.min.y + 0.5),
                stroke,
            },
            TitleSide::Left => PaintCmd::Line {
                a: MaraPos2::new(m_rect.max.x - 0.5, m_rect.min.y),
                b: MaraPos2::new(m_rect.max.x - 0.5, m_rect.max.y),
                stroke,
            },
            TitleSide::Right => PaintCmd::Line {
                a: MaraPos2::new(m_rect.min.x + 0.5, m_rect.min.y),
                b: MaraPos2::new(m_rect.min.x + 0.5, m_rect.max.y),
                stroke,
            },
        };
        ui.painter().paint_cmd(line);
    }
}
