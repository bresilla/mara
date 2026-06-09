//! Shared layout constants + the single paint helper every ribbon
//! button renders through. Internal to the `ribbon` module; the
//! static ribbons and the drag-aware layout both route here so the
//! pixel-level look stays identical whichever path the caller took.

use egui;

use crate::style::{
    BG_1_PANEL, BG_2_RAISED, BORDER_SUBTLE, RadiusRole, StrokeRole, glass_alpha_card,
    glass_alpha_window, glass_fill, radius_for, stroke_for,
};

/// sRGB lerp on RGB channels, alpha left at 255. Local copy so the
/// ribbon paint module doesn't reach into `style`'s private helpers.
pub(crate) fn lerp_rgb(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| ((x as f32) * (1.0 - t) + (y as f32) * t).round() as u8;
    egui::Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// Foreground (glyph / label) colour matching the recipe in
/// [`paint_ribbon_button`]. Centralised so the static-ribbon path
/// (`ribbon_button_area`) and the dynamic drag-aware path
/// all ribbon chrome paths pick text that
/// contrasts with the EXACT fill the button paints, rather than
/// each call site re-deriving it (and drifting out of sync the
/// next time the bg recipe changes).
/// Shared paint dispatch for the three ribbon-button glyph kinds.
/// Centred on `rect`'s middle; size = 14 px (text/icon) or rect
/// shrunk by 6 px (svg). Tinted in `fg`.
pub(crate) fn paint_ribbon_glyph(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    glyph: super::chrome::RibbonGlyph,
    fg: egui::Color32,
) {
    use super::chrome::RibbonGlyph;
    match glyph {
        RibbonGlyph::Text(s) => {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                s,
                egui::FontId::new(14.0, egui::FontFamily::Monospace),
                fg,
            );
        }
        RibbonGlyph::Icon(name) => {
            crate::icons::paint_icon(
                ui.painter(),
                rect.center(),
                egui::Align2::CENTER_CENTER,
                name,
                18.0,
                fg,
            );
        }
        RibbonGlyph::Svg(svg) => {
            crate::icons::paint_section_icon(
                ui,
                rect.center(),
                egui::Align2::CENTER_CENTER,
                crate::icons::Icon::Svg(svg),
                rect.shrink(6.0).width(),
                fg,
            );
        }
    }
}

pub(crate) fn ribbon_button_fg(
    accent: egui::Color32,
    is_active: bool,
    hovered: bool,
    glyph: super::chrome::RibbonGlyph,
) -> egui::Color32 {
    if crate::style::theme().ribbon.button_accent_fill {
        // GAME ladder — pick contrast text against the EXACT fill
        // `paint_ribbon_button` produced, so the glyph sits cleanly
        // against the active / hover / idle tier. Mirrors the same
        // mode-aware lerp targets the paint uses.
        let (hover_target, idle_target) = if crate::style::theme().is_light {
            (egui::Color32::BLACK, egui::Color32::WHITE)
        } else {
            (egui::Color32::WHITE, egui::Color32::BLACK)
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
            lerp_rgb(accent, egui::Color32::WHITE, 0.20)
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
pub(crate) fn paint_ribbon_button(
    painter: &egui::Painter,
    rect: egui::Rect,
    accent: egui::Color32,
    is_active: bool,
    hovered: bool,
) {
    let theme = crate::style::theme();
    let radius = radius_for(RadiusRole::Section);

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
            (egui::Color32::BLACK, egui::Color32::WHITE)
        } else {
            (egui::Color32::WHITE, egui::Color32::BLACK)
        };
        let fill = if is_active {
            accent
        } else if hovered {
            lerp_rgb(accent, hover_target, 0.28)
        } else {
            lerp_rgb(accent, idle_target, 0.30)
        };
        painter.rect(
            rect,
            radius,
            fill,
            egui::Stroke::NONE,
            egui::StrokeKind::Inside,
        );
        return;
    }

    // PRO recipe — theme-aware: idle uses the active panel fill,
    // hover lifts to bg_raised, active blends 25 % accent into the
    // raised tier. Replacing hard-coded `BG_*` constants
    // so light variants stop painting near-black ribbon buttons on
    // a white window.
    let bg_raised = theme.bg_raised;
    let bg_idle = theme.bg_panel;
    let bg = if is_active {
        let blend = |a: u8, b: u8| ((a as f32) * 0.75 + (b as f32) * 0.25).round() as u8;
        let tinted = egui::Color32::from_rgb(
            blend(bg_raised.r(), accent.r()),
            blend(bg_raised.g(), accent.g()),
            blend(bg_raised.b(), accent.b()),
        );
        glass_fill(tinted, accent, glass_alpha_window())
    } else if hovered {
        glass_fill(bg_raised, accent, glass_alpha_window())
    } else {
        glass_fill(bg_idle, accent, glass_alpha_window())
    };
    let stroke = if is_active {
        accent
    } else {
        crate::style::widget_border(accent)
    };
    let _ = (BG_1_PANEL, BG_2_RAISED, BORDER_SUBTLE);
    painter.rect(
        rect,
        radius,
        bg,
        if is_active {
            egui::Stroke::new(theme.border_width, stroke)
        } else {
            stroke_for(StrokeRole::WidgetBorder, accent)
        },
        egui::StrokeKind::Inside,
    );
    let _ = glass_alpha_card();
}

// `ribbon_button_area` (the static-ribbon button area helper) was
// retired with the old static/declare ribbon modules; the unified
// ribbon chrome builds draggable Areas directly.
