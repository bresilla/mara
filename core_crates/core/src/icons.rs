//! Filled Fluent UI System Icons via the [`iconflow`] crate.
//!
//! maracore registers every Fluent UI font variant in
//! [`crate::style::apply_theme`]'s font install pass so widgets can
//! render an icon glyph anywhere a `RichText` or `painter().text(..)`
//! call lands. Lookup is by string name (e.g. `"search"`,
//! `"chevron_down"`); style is filled and size is the regular
//! variant — that's the look the user asked for.
//!
//! Two entry points:
//!
//! * [`icon`] — returns `Option<(char, FontFamily)>` so callers that
//!   need the codepoint directly (custom painters) can place it
//!   themselves.
//! * [`icon_text`] — wraps the same lookup in a `RichText` ready to
//!   drop into `ui.label(...)` / `ui.add(Label::new(...))`.
//!
//! Fonts are bundled via iconflow's `fonts()` registry — we walk it
//! once at theme-apply time and register each `(family, bytes)` pair
//! as `egui::FontFamily::Name(family)`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use egui;
use iconflow::{IconRef, Pack, Size, Style, fonts, try_icon};

/// `true` once `ctx.set_fonts(...)` has installed the iconflow font
/// families in egui — set from [`crate::style::install_fonts`]
/// after the `set_fonts` call. Read by [`paint_icon`] /
/// [`paint_section_icon`] so they SKIP rendering when fonts aren't
/// ready (rather than panicking inside epaint when a `FontFamily`
/// hasn't been bound). Without this, ribbon buttons painted on the
/// FIRST frame — before bevy_mara's `apply_theme_system` runs —
/// would crash with "FontFamily::Name(...) is not bound to any
/// fonts".
pub(crate) static ICONFLOW_FONTS_READY: AtomicBool = AtomicBool::new(false);

#[inline]
fn fonts_ready() -> bool {
    ICONFLOW_FONTS_READY.load(Ordering::Relaxed)
}

/// Pull every iconflow font into `FontDefinitions` and register
/// each as a named family so `FontFamily::Name(family)` resolves to
/// the right glyph table. Called from [`crate::style::install_fonts`].
pub(crate) fn install_iconflow_fonts(fonts_def: &mut egui::FontDefinitions) {
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
    /// Raw SVG markup. Painted by `paint_section_icon` via
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

/// Paint a [`Icon`] at `pos`, aligned via `align`, sized to `size`
/// pixels, tinted by `color`. Dispatches to the Fluent painter for
/// `Icon::Name`, and to egui's image loader for `Icon::Svg`.
pub fn paint_section_icon(
    ui: &mut egui::Ui,
    pos: egui::Pos2,
    align: egui::Align2,
    icon: Icon<'_>,
    size: f32,
    color: egui::Color32,
) {
    match icon {
        Icon::Name(name) => {
            if !fonts_ready() {
                return;
            }
            paint_icon(ui.painter(), pos, align, name, size, color);
        }
        Icon::Svg(svg) => {
            let rect = align.anchor_rect(egui::Rect::from_min_size(pos, egui::vec2(size, size)));
            // Stable URI per SVG content so egui's loader can cache;
            // a tiny djb2 hash keeps the URI short without pulling
            // in `std::collections::hash_map::DefaultHasher`.
            let mut h: u64 = 5381;
            for b in svg.as_bytes() {
                h = h.wrapping_mul(33).wrapping_add(*b as u64);
            }
            let uri = format!("bytes://mara_svg_icon_{:016x}.svg", h);
            let img = egui::Image::from_bytes(uri, svg.as_bytes().to_vec())
                .tint(color)
                .fit_to_exact_size(rect.size());
            img.paint_at(ui, rect);
        }
    }
}

/// Look up a filled Fluent UI System Icon by name. Returns the
/// glyph character + the font family to render it in. Returns
/// `None` when the icon isn't in the bundled set — caller should
/// fall back gracefully.
pub fn icon(name: &str) -> Option<(char, egui::FontFamily)> {
    let IconRef { family, codepoint } =
        try_icon(Pack::Fluentui, name, Style::Filled, Size::Regular).ok()?;
    let glyph = char::from_u32(codepoint)?;
    Some((glyph, egui::FontFamily::Name(family.into())))
}

/// Build a `RichText` rendering the named filled Fluent UI icon at
/// `size` px in `color`. Returns `None` if the icon isn't bundled —
/// callers can `.unwrap_or_else(|| RichText::new("?"))` or similar.
///
/// ```ignore
/// if let Some(t) = maracore::icons::icon_text("search", 14.0, accent) {
///     ui.label(t);
/// }
/// ```
pub fn icon_text(name: &str, size: f32, color: egui::Color32) -> Option<egui::RichText> {
    if !fonts_ready() {
        return None;
    }
    let (glyph, family) = icon(name)?;
    Some(
        egui::RichText::new(glyph.to_string())
            .font(egui::FontId::new(size, family))
            .color(color),
    )
}

/// Paint a named filled Fluent UI icon at `pos` aligned by `align`
/// in `color` at `size` px. No-op when the icon isn't bundled.
pub fn paint_icon(
    painter: &egui::Painter,
    pos: egui::Pos2,
    align: egui::Align2,
    name: &str,
    size: f32,
    color: egui::Color32,
) {
    // Fonts not yet installed → skip silently. The next frame
    // `apply_theme` has run, [`ICONFLOW_FONTS_READY`] is `true`, and
    // the icon will paint. This guard turns the crash into a
    // one-frame visual blip on initial startup.
    if !fonts_ready() {
        return;
    }
    if let Some((glyph, family)) = icon(name) {
        if !painter.fonts(|fonts| fonts.families().contains(&family)) {
            return;
        }
        painter.text(
            pos,
            align,
            glyph.to_string(),
            egui::FontId::new(size, family),
            color,
        );
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
}
