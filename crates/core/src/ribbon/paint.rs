//! Shared layout constants + the single paint helper every ribbon
//! button renders through. Internal to the `ribbon` module; the
//! static ribbons and the drag-aware layout both route here so the
//! pixel-level look stays identical whichever path the caller took.


use crate::paint::PaintCmd;
use crate::style::{
    BG_1_PANEL, BG_2_RAISED, BORDER_SUBTLE, RadiusRole, StrokeRole, glass_alpha_card,
    glass_alpha_window, glass_fill, radius_for, stroke_for,
};
use crate::vocab::{
    Align2 as MaraAlign2, Color32 as MaraColor32, Rect as MaraRect, Stroke as MaraStroke,
};

/// sRGB lerp on RGB channels, alpha left at 255. Local copy so the
/// ribbon paint module doesn't reach into `style`'s private helpers.
pub(crate) fn lerp_rgb(a: MaraColor32, b: MaraColor32, t: f32) -> MaraColor32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| ((x as f32) * (1.0 - t) + (y as f32) * t).round() as u8;
    MaraColor32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// Foreground (glyph / label) colour matching the recipe in
/// [`ribbon_button_paint_cmds`]. Centralised so all ribbon chrome
/// paths pick text that
/// contrasts with the EXACT fill the button paints, rather than
/// each call site re-deriving it (and drifting out of sync the
/// next time the bg recipe changes).
/// Shared paint dispatch for the three ribbon-button glyph kinds.
/// Centred on `rect`'s middle; size = 14 px (text/icon) or rect
/// shrunk by 6 px (svg). Tinted in `fg`.
pub(crate) fn paint_ribbon_glyph(
    ui: &mut crate::MaraUi<'_>,
    rect: MaraRect,
    glyph: super::chrome::RibbonGlyph,
    fg: MaraColor32,
) {
    if matches!(glyph, super::chrome::RibbonGlyph::Icon(_)) && !crate::icons::icon_fonts_ready() {
        return;
    }
    if let Some(cmd) = ribbon_glyph_paint_cmd(rect, glyph, fg) {
        ui.paint(cmd);
    }
}

pub(crate) fn ribbon_glyph_paint_cmd(
    rect: MaraRect,
    glyph: super::chrome::RibbonGlyph,
    fg: MaraColor32,
) -> Option<PaintCmd> {
    use super::chrome::RibbonGlyph;
    match glyph {
        RibbonGlyph::Text(s) => Some(PaintCmd::Text {
            pos: rect.center(),
            anchor: MaraAlign2::CENTER_CENTER,
            text: s.to_owned(),
            size: 14.0,
            color: fg,
            mono: true,
        }),
        RibbonGlyph::Icon(name) => crate::icons::icon_paint_cmd(
            crate::icons::Icon::Name(name),
            rect.center(),
            MaraAlign2::CENTER_CENTER,
            18.0,
            fg,
        ),
        RibbonGlyph::Svg(svg) => crate::icons::icon_paint_cmd(
            crate::icons::Icon::Svg(svg),
            rect.center(),
            MaraAlign2::CENTER_CENTER,
            rect.shrink(6.0).width(),
            fg,
        ),
    }
}

pub(crate) fn ribbon_button_fg(
    accent: MaraColor32,
    is_active: bool,
    hovered: bool,
    glyph: super::chrome::RibbonGlyph,
) -> MaraColor32 {
    if crate::style::theme().ribbon.button_accent_fill {
        // GAME ladder — pick contrast text against the EXACT fill
        // `ribbon_button_paint_cmds` produced, so the glyph sits cleanly
        // against the active / hover / idle tier. Mirrors the same
        // mode-aware lerp targets the paint uses.
        let (hover_target, idle_target) = if crate::style::theme().is_light {
            (MaraColor32::BLACK, MaraColor32::WHITE)
        } else {
            (MaraColor32::WHITE, MaraColor32::BLACK)
        };
        let fill = if is_active {
            accent
        } else if hovered {
            lerp_rgb(accent, hover_target, 0.28)
        } else {
            lerp_rgb(accent, idle_target, 0.30)
        };
        return crate::style::contrast_text_for(fill);
    }
    // PRO recipe. Active button paints over an accent-tinted bg.
    // For Text / Icon glyphs render the active fg as a *brightened*
    // accent (`lerp(accent, WHITE, 0.20)`) so the glyph reads as the
    // selected tier — vivid accent letter / icon on the
    // accent-tinted button. SVG glyphs keep the contrast colour
    // because their author chose their own colours via the SVG
    // markup; tinting them accent would corrupt their look.
    use super::chrome::RibbonGlyph;
    if is_active {
        if matches!(glyph, RibbonGlyph::Svg(_)) {
            crate::style::contrast_text_for(accent)
        } else {
            lerp_rgb(accent, MaraColor32::WHITE, 0.20)
        }
    } else {
        crate::style::on_panel_dim()
    }
}

// ─── Layout constants ───────────────────────────────────────────────

/// Edge length of each square ribbon button (VS Code / Fleet size).
pub const SIDE_BTN_SIZE: f32 = 34.0;
/// Gap between adjacent ribbon buttons.
pub const SIDE_BTN_GAP: f32 = 4.0;
/// Distance from the screen edge to the near edge of each button.
///
/// Keep this tight: Mara owns the whole decorationless chrome, so
/// bars should feel attached to the window corners rather than
/// floating far inside the canvas.
pub const EDGE_GAP: f32 = 4.8;

// ─── Paint ──────────────────────────────────────────────────────────

/// Background / border recipe for every ribbon button. Per-theme
/// branch:
///
/// * `ribbon_button_accent_fill = false` (PRO) → original glass
///   look. Idle paints `BG_1_PANEL`, hover lifts to `BG_2_RAISED`,
///   active blends 25 % accent into the raised tier and adds an
///   accent stroke. Same recipe the kit shipped with.
/// * `ribbon_button_accent_fill = true` (GAME) → three-tier accent
///   ladder. Idle = accent dimmed 30 % toward black, hover = pure
///   accent, active = accent brightened 28 % toward white + 1.5 px
///   outer accent halo.
pub(crate) fn ribbon_button_paint_cmds(
    rect: MaraRect,
    accent: MaraColor32,
    is_active: bool,
    hovered: bool,
) -> Vec<PaintCmd> {
    let theme = crate::style::theme();
    let radius = radius_for(RadiusRole::Section);
    let accent_raw: MaraColor32 = accent;

    if theme.ribbon.button_accent_fill {
        // Three filled tiers, no stroke / halo / border. Active uses
        // FULL accent so the selected ribbon button reads as the same
        // colour family as the open pane's container-title banner —
        // both surfaces signal "this is the active feature" with the
        // identical accent fill.
        //
        // Hover and idle MIRROR between Dark and Light:
        // - Dark: panel is dark, hover lifts toward WHITE (visibly
        //   brighter), idle pulls toward BLACK (slightly darker than
        //   the panel's accent tier — recessed).
        // - Light: panel is bright accent, hover pulls toward BLACK
        //   (visibly darker — pop), idle lifts toward WHITE (faded
        //   into the bright panel — recessed).
        // Same relative hierarchy in both modes, just inverted target
        // colours so the brightness deltas read correctly against
        // each panel's luma.
        let (hover_target, idle_target) = if theme.is_light {
            (MaraColor32::BLACK, MaraColor32::WHITE)
        } else {
            (MaraColor32::WHITE, MaraColor32::BLACK)
        };
        let fill: MaraColor32 = if is_active {
            accent
        } else if hovered {
            lerp_rgb(accent, hover_target, 0.28)
        } else {
            lerp_rgb(accent, idle_target, 0.30)
        };
        return vec![PaintCmd::RectFilled {
            rect,
            corner: radius,
            fill,
        }];
    }

    // PRO recipe — theme-aware: idle uses the active panel fill,
    // hover lifts to bg_raised, active blends 25 % accent into the
    // raised tier. Replacing hard-coded `BG_*` constants
    // so light variants stop painting near-black ribbon buttons on
    // a white window.
    let bg_raised = theme.bg_raised;
    let bg_idle = theme.bg_panel;
    let bg: MaraColor32 = if is_active {
        let blend = |a: u8, b: u8| ((a as f32) * 0.75 + (b as f32) * 0.25).round() as u8;
        let tinted = MaraColor32::from_rgb(
            blend(bg_raised.r(), accent_raw.r()),
            blend(bg_raised.g(), accent_raw.g()),
            blend(bg_raised.b(), accent_raw.b()),
        );
        glass_fill(tinted, accent_raw, glass_alpha_window())
    } else if hovered {
        glass_fill(bg_raised, accent_raw, glass_alpha_window())
    } else {
        glass_fill(bg_idle, accent_raw, glass_alpha_window())
    };
    let stroke_color: MaraColor32 = if is_active {
        accent_raw.into()
    } else {
        crate::style::widget_border(accent_raw)
    };
    let stroke = if is_active {
        MaraStroke::new(theme.border_width, stroke_color)
    } else {
        stroke_for(StrokeRole::WidgetBorder, accent)
    };
    let _ = (BG_1_PANEL, BG_2_RAISED, BORDER_SUBTLE);
    let _ = glass_alpha_card();
    vec![
        PaintCmd::RectFilled {
            rect,
            corner: radius,
            fill: bg,
        },
        PaintCmd::RectStroke {
            rect,
            corner: radius,
            stroke,
        },
    ]
}

// `ribbon_button_area` (the static-ribbon button area helper) was
// retired with the old static/declare ribbon modules; the unified
// ribbon chrome builds draggable Areas directly.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ribbon::chrome::RibbonGlyph;
    use crate::vocab::{Color32, Pos2, Rect, Vec2};

    #[test]
    fn ribbon_text_glyph_lowers_to_mara_text_command() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(34.0, 34.0));
        let cmd = ribbon_glyph_paint_cmd(rect, RibbonGlyph::Text("A"), Color32::WHITE)
            .expect("text glyph should lower");

        let PaintCmd::Text {
            pos, anchor, mono, ..
        } = cmd
        else {
            panic!("text ribbon glyph should lower to Mara text");
        };
        assert_eq!(pos, rect.center());
        assert_eq!(anchor, MaraAlign2::CENTER_CENTER);
        assert!(mono);
    }

    #[test]
    fn ribbon_svg_glyph_lowers_to_mara_svg_command() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(34.0, 34.0));
        let cmd = ribbon_glyph_paint_cmd(
            rect,
            RibbonGlyph::Svg("<svg viewBox='0 0 8 8'></svg>"),
            Color32::WHITE,
        )
        .expect("svg glyph should lower");

        let PaintCmd::Svg { rect: svg_rect, .. } = cmd else {
            panic!("svg ribbon glyph should lower to Mara SVG");
        };
        assert_eq!(
            svg_rect,
            Rect::from_min_size(Pos2::new(16.0, 26.0), Vec2::new(22.0, 22.0))
        );
    }

    #[test]
    fn ribbon_button_chrome_lowers_to_mara_paint_commands() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(34.0, 34.0));
        let commands = ribbon_button_paint_cmds(rect, Color32::from_rgb(120, 40, 200), true, false);

        assert!(
            commands
                .iter()
                .any(|cmd| matches!(cmd, PaintCmd::RectFilled { rect: r, .. } if *r == rect))
        );
        assert!(commands.iter().all(|cmd| matches!(
            cmd,
            PaintCmd::RectFilled { .. } | PaintCmd::RectStroke { .. }
        )));
    }

    #[test]
    fn ribbon_button_foreground_policy_returns_mara_color() {
        let fg: Color32 = ribbon_button_fg(
            Color32::from_rgb(120, 40, 200),
            true,
            false,
            RibbonGlyph::Text("A"),
        );

        assert_ne!(fg.a(), 0);
    }
}
