//! Filled Fluent UI System Icons via the [`iconflow`] crate.
//!
//! mara_core registers every Fluent UI font variant in
//! Mara's internal style font install pass so widgets can
//! render an icon glyph anywhere a `RichText` or `painter().text(..)`
//! call lands. Lookup is by string name (e.g. `"search"`,
//! `"chevron_down"`); style is filled and size is the regular
//! variant — that's the look the user asked for.
//!
//! Public entry points:
//!
//! * [`is_icon_payload`] validates Fluent icon names and raw SVG payloads.
//! * [`icon_glyph`] returns backend-neutral glyph data so callers can lower
//!   icons into Mara paint commands.
//!
//! Fonts are bundled via iconflow's `fonts()` registry — we walk it
//! once at theme-apply time and register each `(family, bytes)` pair
//! as `egui::FontFamily::Name(family)`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{
    paint::{PaintCmd, TextFamily},
    vocab::{
        Align2 as MaraAlign2, Color32 as MaraColor32, Pos2 as MaraPos2, Rect as MaraRect,
        Vec2 as MaraVec2,
    },
};
use egui;
use iconflow::{IconRef, Pack, Size, Style, fonts, try_icon};

/// `true` once `ctx.set_fonts(...)` has installed the iconflow font
/// families in egui — set from Mara's internal style font installer
/// after the `set_fonts` call. Read by the internal egui icon paint hooks so
/// they SKIP rendering when fonts aren't
/// ready (rather than panicking inside epaint when a `FontFamily`
/// hasn't been bound). Without this, ribbon buttons painted on the
/// FIRST frame — before bevy_mara's `apply_theme_system` runs —
/// would crash with "FontFamily::Name(...) is not bound to any
/// fonts".
#[doc(hidden)]
pub static ICONFLOW_FONTS_READY: AtomicBool = AtomicBool::new(false);

#[inline]
fn fonts_ready() -> bool {
    ICONFLOW_FONTS_READY.load(Ordering::Relaxed)
}

pub(crate) fn icon_fonts_ready() -> bool {
    fonts_ready()
}

/// Pull every iconflow font into `FontDefinitions` and register
/// each as a named family so `FontFamily::Name(family)` resolves to
/// the right glyph table. Called from Mara's internal style font installer.
#[doc(hidden)]
pub fn install_iconflow_fonts(fonts_def: &mut egui::FontDefinitions) {
    let fallback_fonts = fonts_def
        .families
        .get(&egui::FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for asset in fonts() {
        let key = asset.family.to_string();
        fonts_def.font_data.insert(
            key.clone(),
            Arc::new(egui::FontData::from_static(asset.bytes)),
        );
        let mut family_fonts = vec![key];
        family_fonts.extend(fallback_fonts.iter().cloned());
        fonts_def
            .families
            .insert(egui::FontFamily::Name(asset.family.into()), family_fonts);
    }
}

/// Source for a section / widget icon — either a bundled Fluent
/// glyph (looked up by name) or raw SVG content (rendered via
/// egui's image loader pipeline; the host must install a loader
/// that handles the `image/svg+xml` mime type, e.g.
/// [`egui_extras::install_image_loaders`] with the `svg` feature).
#[derive(Clone, Copy, Debug)]
pub enum Icon<'a> {
    /// Look up `&str` in the bundled Fluent UI System Icons set.
    Name(&'a str),
    /// Raw SVG markup. Painted by the internal egui icon paint hook via
    /// `egui::Image::from_bytes` + `paint_at`. No-ops silently if
    /// the host hasn't installed an SVG loader.
    Svg(&'a str),
}

impl<'a> From<&'a str> for Icon<'a> {
    fn from(s: &'a str) -> Icon<'a> {
        // Cheap heuristic: any string that looks like SVG markup
        // resolves to `Svg`; everything else is a Fluent icon name.
        // Saves callers from typing `Icon::Svg(...)` explicitly when
        // they want SVG via the `&str` shorthand.
        let trimmed = s.trim_start();
        if trimmed.starts_with("<svg") || trimmed.starts_with("<?xml") {
            Icon::Svg(s)
        } else {
            Icon::Name(s)
        }
    }
}

/// `true` when `payload` can render as a Mara icon: either raw SVG
/// markup or a bundled Fluent UI System Icon name.
#[must_use]
pub fn is_icon_payload(payload: &str) -> bool {
    let trimmed = payload.trim_start();
    trimmed.starts_with("<svg") || trimmed.starts_with("<?xml") || icon(payload).is_some()
}

/// Look up a filled Fluent UI System Icon by name. Returns the
/// glyph character + the font family to render it in. Returns
/// `None` when the icon isn't in the bundled set — caller should
/// fall back gracefully.
pub(crate) fn icon(name: &str) -> Option<(char, egui::FontFamily)> {
    let (glyph, family) = icon_glyph(name)?;
    Some((glyph, egui::FontFamily::Name(family.into())))
}

/// Look up a filled Fluent UI System Icon as backend-neutral paint
/// data: glyph character plus the named icon font family.
pub fn icon_glyph(name: &str) -> Option<(char, String)> {
    let IconRef { family, codepoint } =
        try_icon(Pack::Fluentui, name, Style::Filled, Size::Regular).ok()?;
    let glyph = char::from_u32(codepoint)?;
    Some((glyph, family.to_string()))
}

/// Lower an icon payload to backend-neutral Mara paint data.
///
/// Named Fluent icons become named-font text commands. SVG payloads become
/// raw SVG paint commands with Mara-owned placement geometry; the backend
/// adapter remains responsible for image/SVG loader integration.
pub(crate) fn icon_paint_cmd(
    icon: Icon<'_>,
    pos: MaraPos2,
    anchor: MaraAlign2,
    size: f32,
    color: MaraColor32,
) -> Option<PaintCmd> {
    match icon {
        Icon::Name(name) => {
            let (glyph, family) = icon_glyph(name)?;
            Some(PaintCmd::TextWithFamily {
                pos,
                anchor,
                text: glyph.to_string(),
                size,
                color,
                family: TextFamily::Named(family),
            })
        }
        Icon::Svg(svg) => {
            let rect = anchor.anchor_rect(MaraRect::from_min_size(pos, MaraVec2::new(size, size)));
            Some(PaintCmd::Svg {
                svg: svg.to_owned(),
                rect,
                tint: color,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iconflow_families_keep_proportional_fallbacks() {
        let mut fonts = egui::FontDefinitions::default();
        let proportional = fonts
            .families
            .get(&egui::FontFamily::Proportional)
            .cloned()
            .expect("egui default fonts should expose a proportional fallback chain");

        install_iconflow_fonts(&mut fonts);

        let (_, icon_family) = icon("search").expect("search icon should be bundled");
        let icon_chain = fonts
            .families
            .get(&icon_family)
            .expect("install_iconflow_fonts should bind the icon family");

        assert!(
            icon_chain.len() > 1,
            "icon families need normal text fallback fonts so replacement glyph lookup cannot warn or fail"
        );
        assert_eq!(
            &icon_chain[1..],
            proportional.as_slice(),
            "icon font should be first, followed by the normal proportional fallback chain"
        );
    }

    #[test]
    fn icon_payload_validation_does_not_depend_on_runtime_font_install() {
        ICONFLOW_FONTS_READY.store(false, Ordering::Relaxed);

        assert!(
            is_icon_payload("search"),
            "static icon validation should check the bundled registry, not egui runtime font state"
        );
    }

    #[test]
    fn icon_paint_cmd_lowers_svg_to_mara_svg_geometry() {
        let cmd = icon_paint_cmd(
            Icon::Svg("<svg viewBox='0 0 8 8'></svg>"),
            MaraPos2::new(10.0, 20.0),
            MaraAlign2::CENTER_CENTER,
            16.0,
            MaraColor32::WHITE,
        )
        .expect("svg icons should always lower");

        let PaintCmd::Svg { rect, tint, .. } = cmd else {
            panic!("expected Mara SVG paint command");
        };
        assert_eq!(
            rect,
            MaraRect::from_min_max(MaraPos2::new(2.0, 12.0), MaraPos2::new(18.0, 28.0))
        );
        assert_eq!(tint, MaraColor32::WHITE);
    }

    #[test]
    fn icon_paint_cmd_lowers_named_icon_to_named_font_text() {
        let cmd = icon_paint_cmd(
            Icon::Name("search"),
            MaraPos2::new(1.0, 2.0),
            MaraAlign2::CENTER_CENTER,
            18.0,
            MaraColor32::WHITE,
        )
        .expect("search icon should be bundled");

        let PaintCmd::TextWithFamily { family, text, .. } = cmd else {
            panic!("expected named-font text command");
        };
        assert_eq!(text.chars().count(), 1);
        assert!(matches!(family, TextFamily::Named(_)));
    }
}
