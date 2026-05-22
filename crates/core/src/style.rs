//! One-shot egui theme setup — palette, typography, and a single
//! `apply_theme` function. **Framework-agnostic** — no bevy or
//! bevy_egui imports here. The `bevy_mara` crate wraps these in
//! Bevy `Resource`s + a `Plugin` that runs `apply_theme` every
//! frame; plain-egui callers call `apply_theme(ctx, accent,
//! opacity)` directly from their UI body.
//!
//! Palette + typography follow the 2024-2026 editor convergence
//! (Blender 4, UE5.4, Godot 4, Unity 6, Fleet). All values are
//! centralised here so individual panels never hard-code colours —
//! the full palette is published even if not every token has a
//! current caller, so new UI pulls from the same reference set.

#![allow(dead_code)]

// ─── Sizing fundamentals ───────────────────────────────────────────

/// Body text size, in pixels. Drives the [`UNIT`] derivation below
/// — change here only if the entire Mara type ramp moves. The value
/// is kept at the established 13 px body size so migrated widgets
/// preserve their scale.
pub const BODY_FONT_SIZE: f32 = 13.0;

/// Foundational row-height unit, in pixels. The canonical "1 row"
/// measurement for every mara widget — derived as body font size
/// plus 25 % padding above and 25 % padding below
/// (`BODY_FONT_SIZE × 1.5`). With body text at 14 px, `UNIT = 21 px`.
///
/// Every widget's height is a multiple of `UNIT`. A search bar /
/// button / single-line control is 1U; a future labelled control
/// could be 2U; a stacked editor could be 3U; etc. The resizable
/// [`crate::pod::Pod`] floor is 1U — dragging the resize handle
/// can never shrink a widget below `UNIT`. Whenever you add a new
/// widget, size it as `UNIT * n` for some integer `n` so it lines
/// up with the rest of the kit's vertical rhythm.
pub const UNIT: f32 = BODY_FONT_SIZE * 1.5;

// ─── Neutrals ───────────────────────────────────────────────────────
// PRO Dark palette. Wider tier deltas than the previous pass — each
// "step up" or "step down" from the panel is now ~20+ units instead
// of ~10, so sections, hover lifts, and inputs read as obviously
// different surfaces instead of subtle adjacencies.
//
// Window sits ~14 below panel, raised sits ~28 above, hover sits
// ~50 above, input ~16 below. With borders / separators dialed
// down on Dark, hierarchy now comes from the surface tier alone,
// which needs the colour delta to do the work.
pub const BG_0_WINDOW: egui::Color32 = egui::Color32::from_rgb(0x06, 0x08, 0x0E);
pub const BG_1_PANEL: egui::Color32 = egui::Color32::from_rgb(0x14, 0x16, 0x1D);
pub const BG_2_RAISED: egui::Color32 = egui::Color32::from_rgb(0x2C, 0x30, 0x3D);
pub const BG_3_HOVER: egui::Color32 = egui::Color32::from_rgb(0x40, 0x46, 0x55);
pub const BG_4_INPUT: egui::Color32 = egui::Color32::from_rgb(0x06, 0x08, 0x0C);

// ─── Glass opacity (slider-driven) ──────────────────────────────────
//
// One user-facing opacity knob, range 1..=100. Internally mapped to
// window opacity 80..=100 % so the UI never becomes so transparent
// it stops being readable. Card + group alphas scale proportionally
// via `CARD_FACTOR` / `GROUP_FACTOR` below.

use core::sync::atomic::{AtomicU8, Ordering};

/// Shadow copy of the current opacity value. Plain helper functions
/// (`section`, `floating_window`, etc.) read this to derive glass
/// alphas without plumbing state through every UI call. Hosts
/// (for example `bevy_mara` or `mara::window`) are responsible for keeping it in sync
/// with their chosen source of truth — either call
/// [`set_glass_opacity`] every frame or on change.
static GLASS_OPACITY: AtomicU8 = AtomicU8::new(100);

/// Plain-data opacity value, range `1..=100`. With the `bevy`
/// crate feature enabled, this derives `Resource` so it slots
/// directly into a Bevy `App`. Without the feature, it's just a
/// plain struct.
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct GlassOpacity(pub u8);

impl Default for GlassOpacity {
    fn default() -> Self {
        Self(100)
    }
}

/// Push a new opacity value into the shared atomic. Call every
/// frame before laying out UI (`bevy_mara` does this via a Bevy
/// system; other hosts can call it from their app's update loop).
pub fn set_glass_opacity(value: u8) {
    GLASS_OPACITY.store(value.clamp(1, 100), Ordering::Relaxed);
}

/// Read the currently-active glass opacity (1..=100). Mirrors
/// [`active_accent`] for the second per-frame theme parameter —
/// callers that need to pass the live theme state to a secondary
/// `egui::Context` (e.g. `node_view::show`) read both via these
/// getters instead of threading them through every signature.
pub fn glass_opacity() -> GlassOpacity {
    GlassOpacity(GLASS_OPACITY.load(Ordering::Relaxed))
}

/// Map the slider's `1..=100` onto a window opacity fraction.
/// Mode-aware floor:
///
/// * Dark: `0.55..=1.00`. A dark panel at 30 % over a brighter scene
///   reads as nearly missing — too aggressive. 55 % keeps the
///   panel readable while still giving the slider perceptual room.
/// * Light: `0.10..=1.00`. A near-white panel needs to drop much
///   lower before the scene shows through, so the floor goes deeper
///   on Light to match the *perceived* see-through Dark gets at 55 %.
///
/// `1 → floor`, `100 → 1.00`, linear in between.
fn opacity_frac() -> f32 {
    let floor = if theme().is_light { 0.10 } else { 0.55 };
    let t = (GLASS_OPACITY.load(Ordering::Relaxed).max(1) as f32 - 1.0) / 99.0;
    floor + (1.0 - floor) * t
}

pub fn glass_alpha_window() -> u8 {
    (opacity_frac() * 255.0).round().clamp(0.0, 255.0) as u8
}
pub fn glass_alpha_card() -> u8 {
    let f = theme().glass.card_factor;
    (opacity_frac() * f * 255.0).round().clamp(0.0, 255.0) as u8
}
pub fn glass_alpha_group() -> u8 {
    let f = theme().glass.group_factor;
    (opacity_frac() * f * 255.0).round().clamp(0.0, 255.0) as u8
}

// ─── Glassy variants ────────────────────────────────────────────────
//
// Panel / card / group surfaces get progressive transparency so the
// 3D scene peeks through the stack, plus a very faint accent tint
// that shifts hue with the selection.
//
// Alphas are DECREASING with depth on purpose: the outermost panel
// holds almost all the opacity; each deeper layer only adds a small
// extra veil so overlap doesn't compound into "effectively solid".
// Opacity stacks as `1 − (1-a)·(1-b)·(1-c)`, so card+group ≈ 16 %
// on top of the panel — just enough to read as "another surface".
// Alphas are computed each frame from `GLASS_OPACITY` so the
// single UI slider (General Properties → Theme → opacity) drives
// every glass surface proportionally. See `glass_alpha_*()` below.
//
/// How much of the accent colour to blend into each glass fill,
/// kept as a fallback for callers reading the const directly.
/// `theme().glass_accent_tint` is the active value; this constant
/// matches the PRO profile's value so older code paths keep working
/// when the theme is PRO.
pub const GLASS_ACCENT_TINT: f32 = 0.03;

/// Produce a glass-style fill: base RGB lightly tinted toward
/// `accent`, with the given alpha. Use with any `egui::Frame::fill`.
/// Uses *unmultiplied* alpha so the painted surface blends at
/// `alpha/255` opacity over the scene. The accent-tint fraction is
/// read from the active [`Theme`] — GAME-style themes can set it to
/// `0.0` to flatten the fill into a pure neutral tone.
pub fn glass_fill(base: egui::Color32, accent: egui::Color32, alpha: u8) -> egui::Color32 {
    let f = theme().glass.accent_tint;
    let blend = |a: u8, b: u8| ((a as f32) * (1.0 - f) + (b as f32) * f).round() as u8;
    egui::Color32::from_rgba_unmultiplied(
        blend(base.r(), accent.r()),
        blend(base.g(), accent.g()),
        blend(base.b(), accent.b()),
        alpha,
    )
}

pub const BORDER_SUBTLE: egui::Color32 = egui::Color32::from_rgb(0x0E, 0x0E, 0x10);
pub const BORDER_INNER: egui::Color32 = egui::Color32::from_rgb(0x3A, 0x3A, 0x42);

/// Single shared base colour for **every** outline + separator in the
/// kit (widget borders, row hairlines, dividers, palette rules). The
/// active theme's `border_subtle` is pulled 50 % toward the OPPOSITE
/// luma extreme of the panel — black-leaning on Light themes, white-
/// leaning on Dark themes — so the resulting line always reads against
/// whatever brightness the panel sits at.
///
/// Without this lerp, PRO Dark paints a near-black `BORDER_SUBTLE` on
/// a dark panel (low contrast → invisible), and PRO Light paints a
/// pale-grey on a white panel (also invisible). Centralising the
/// "luma flip" here means every consumer (`widget_border`,
/// `paint_hairline`, `divider`, `thin_divider`) gets a contrasting
/// base for free; each consumer just chooses its own alpha.
pub fn outline_base() -> egui::Color32 {
    let th = theme();
    let target = if th.is_light {
        egui::Color32::BLACK
    } else {
        egui::Color32::WHITE
    };
    let blend = |a: u8, b: u8| ((a as f32) * 0.5 + (b as f32) * 0.5).round() as u8;
    egui::Color32::from_rgb(
        blend(th.palette.border_subtle.r(), target.r()),
        blend(th.palette.border_subtle.g(), target.g()),
        blend(th.palette.border_subtle.b(), target.b()),
    )
}

/// The canonical border colour used by **every** mara surface —
/// foldable cards, sub-section frames, inputs, toggles, buttons.
/// Built on top of [`outline_base`]: starts with the mode-aware base,
/// blends in `border_accent_tint` of accent (PRO 6 %, GAME 0 %),
/// applies `border_alpha`. GAME themes pin `border_alpha = 0` so no
/// border paints; PRO themes use a high alpha so the line reads.
pub fn widget_border(accent: egui::Color32) -> egui::Color32 {
    let th = theme();
    let base = outline_base();
    let t = th.stroke.border_accent_tint;
    let blend = |b: u8, a: u8| ((b as f32) * (1.0 - t) + (a as f32) * t).round() as u8;
    egui::Color32::from_rgba_unmultiplied(
        blend(base.r(), accent.r()),
        blend(base.g(), accent.g()),
        blend(base.b(), accent.b()),
        th.stroke.border_alpha,
    )
}

// ─── Text — shared tones used by every theme variant ───────────────
//
// Two parallel triplets — one for Dark variants (light text on dark
// panels) and one for Light variants (dark text on light panels).
// Every theme preset (PRO_DARK, PRO_LIGHT, GAME_DARK, GAME_LIGHT)
// picks the matching triplet so text colour is consistent across
// aesthetic variants and only varies on the brightness axis.
//
// Light-mode tones are deliberately deeper than the previous
// Primer-derived `#1F2328 / #6B7078` pair: a `text_secondary` at
// luma 0.43 looked "almost invisible" on a white panel; the new
// `#4A4D54` at luma 0.31 lifts the contrast without shouting.

/// Primary body text for **Dark variants** (paint on dark panels).
pub const TEXT_PRIMARY: egui::Color32 = egui::Color32::from_rgb(0xE6, 0xE6, 0xE8);
/// Secondary / dim body text for Dark variants.
pub const TEXT_SECONDARY: egui::Color32 = egui::Color32::from_rgb(0x9A, 0x9A, 0xA2);
/// Disabled-state text for Dark variants.
pub const TEXT_DISABLED: egui::Color32 = egui::Color32::from_rgb(0x5A, 0x5A, 0x62);

/// Primary body text for **Light variants** (paint on light panels).
/// Slightly darker than Primer's `#1F2328` so it still reads as
/// "ink-on-paper" rather than a soft grey.
pub const TEXT_PRIMARY_LIGHT: egui::Color32 = egui::Color32::from_rgb(0x18, 0x18, 0x1C);
/// Secondary / dim body text for Light variants. Bumped from the
/// originally-shipping `#6B7078` to give body labels and captions
/// real contrast on white panels.
pub const TEXT_SECONDARY_LIGHT: egui::Color32 = egui::Color32::from_rgb(0x4A, 0x4D, 0x54);
/// Disabled-state text for Light variants.
pub const TEXT_DISABLED_LIGHT: egui::Color32 = egui::Color32::from_rgb(0x8A, 0x8E, 0x96);

// ─── Accent (selection / focus) — violet / purple ──────────────────
//
// Default accent picked to read in BOTH dark and light variants:
// `#7C5CFF` (luma ≈ 0.36) is dark enough to contrast against light
// panels (Δ ≈ 0.55 vs `#FFFFFF`) and bright enough to pop on dark
// panels (Δ ≈ 0.30 vs `#0E0E10`). The previous default `#A78BFA`
// (luma 0.60) was a pastel that disappeared on white panels.
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x7C, 0x5C, 0xFF);
pub const ACCENT_HOVER: egui::Color32 = egui::Color32::from_rgb(0x9D, 0x84, 0xFF);
pub const ACCENT_PRESSED: egui::Color32 = egui::Color32::from_rgb(0x62, 0x42, 0xE6);
/// Subtle purple-tinted surface for the active side button and the
/// selected outliner row. 18 % of `ACCENT` over `BG_2_RAISED`.
pub const ACCENT_TINT: egui::Color32 = egui::Color32::from_rgb(0x42, 0x3A, 0x5A);
pub const SELECTION_ROW: egui::Color32 = egui::Color32::from_rgb(0x4A, 0x3C, 0x72);

// ─── Axes (vivid: gizmos + inspector labels) ────────────────────────
pub const AXIS_X: egui::Color32 = egui::Color32::from_rgb(0xE0, 0x43, 0x3B);
pub const AXIS_Y: egui::Color32 = egui::Color32::from_rgb(0x7F, 0xB4, 0x35);
pub const AXIS_Z: egui::Color32 = egui::Color32::from_rgb(0x2E, 0x83, 0xE6);

// ─── Status ─────────────────────────────────────────────────────────
pub const SUCCESS: egui::Color32 = egui::Color32::from_rgb(0x34, 0xC7, 0x59);
pub const WARNING: egui::Color32 = egui::Color32::from_rgb(0xF5, 0xA5, 0x24);
pub const DANGER: egui::Color32 = egui::Color32::from_rgb(0xEF, 0x44, 0x44);

/// Plain-data accent colour. With the `bevy` crate feature
/// enabled, this derives `Resource` so it can be used directly as
/// a Bevy state type.
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct AccentColor(pub egui::Color32);

/// Neutral accent used when no vehicle is selected.
pub const ACCENT_NEUTRAL: egui::Color32 = egui::Color32::from_rgb(0xE6, 0xE6, 0xE8);

impl Default for AccentColor {
    fn default() -> Self {
        Self(ACCENT_NEUTRAL)
    }
}

// ─── Embedded UI font ───────────────────────────────────────────────
//
// Three Iosevka Term weights baked into the binary via `include_bytes!`:
// Thin, Light, Medium. Each is face 0 of the matching upstream
// `SGr-IosevkaTerm-{Thin,Light,Medium}.ttc`, extracted via fontTools
// and subset to Latin + common symbol blocks (~310 KB each). The host
// picks one with [`set_font_weight`]; the choice is read by
// `apply_theme` and re-applied to egui via `ctx.set_fonts`.
//
// We deliberately stick with the stock egui font families
// (`Proportional` + `Monospace`) and do NOT register `FontFamily::Name`
// variants: `ctx.set_fonts` only takes effect on the NEXT `begin_pass`,
// and bevy_egui 0.39 spawns the primary egui context entity late
// enough that we can't race ahead of frame 0's draw. Looking up an
// unbound `FontFamily::Name("…")` on frame 0 is a hard panic in
// epaint, so we give up per-text weight selection and use size +
// colour + `.strong()` for hierarchy instead.

const IOSEVKA_THIN_TTF: &[u8] = include_bytes!("fonts/iosevka-thin.ttf");
const IOSEVKA_EXTRALIGHT_TTF: &[u8] = include_bytes!("fonts/iosevka-extralight.ttf");
const IOSEVKA_LIGHT_TTF: &[u8] = include_bytes!("fonts/iosevka-light.ttf");
const IOSEVKA_REGULAR_TTF: &[u8] = include_bytes!("fonts/iosevka-regular.ttf");
const IOSEVKA_MEDIUM_TTF: &[u8] = include_bytes!("fonts/iosevka-medium.ttf");
const IOSEVKA_SEMIBOLD_TTF: &[u8] = include_bytes!("fonts/iosevka-semibold.ttf");
const IOSEVKA_BOLD_TTF: &[u8] = include_bytes!("fonts/iosevka-bold.ttf");
const IOSEVKA_EXTRABOLD_TTF: &[u8] = include_bytes!("fonts/iosevka-extrabold.ttf");
const IOSEVKA_HEAVY_TTF: &[u8] = include_bytes!("fonts/iosevka-heavy.ttf");

/// Selected Iosevka weight for the body font. Nine weights, ordered
/// thinnest → heaviest exactly as upstream ships them
/// (`Thin → ExtraLight → Light → Regular → Medium → SemiBold → Bold
/// → ExtraBold → Heavy`). Default is [`FontWeight::Medium`] —
/// visually a touch heavier than `Regular`, easier to read on the
/// saturated accent fills. Switch with [`set_font_weight`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum FontWeight {
    Thin,
    ExtraLight,
    Light,
    Regular,
    #[default]
    Medium,
    SemiBold,
    Bold,
    ExtraBold,
    Heavy,
}

impl FontWeight {
    fn as_u8(self) -> u8 {
        match self {
            FontWeight::Thin => 0,
            FontWeight::ExtraLight => 1,
            FontWeight::Light => 2,
            FontWeight::Regular => 3,
            FontWeight::Medium => 4,
            FontWeight::SemiBold => 5,
            FontWeight::Bold => 6,
            FontWeight::ExtraBold => 7,
            FontWeight::Heavy => 8,
        }
    }

    fn from_u8(v: u8) -> Self {
        match v {
            0 => FontWeight::Thin,
            1 => FontWeight::ExtraLight,
            2 => FontWeight::Light,
            3 => FontWeight::Regular,
            4 => FontWeight::Medium,
            5 => FontWeight::SemiBold,
            6 => FontWeight::Bold,
            7 => FontWeight::ExtraBold,
            _ => FontWeight::Heavy,
        }
    }

    fn ttf(self) -> &'static [u8] {
        match self {
            FontWeight::Thin => IOSEVKA_THIN_TTF,
            FontWeight::ExtraLight => IOSEVKA_EXTRALIGHT_TTF,
            FontWeight::Light => IOSEVKA_LIGHT_TTF,
            FontWeight::Regular => IOSEVKA_REGULAR_TTF,
            FontWeight::Medium => IOSEVKA_MEDIUM_TTF,
            FontWeight::SemiBold => IOSEVKA_SEMIBOLD_TTF,
            FontWeight::Bold => IOSEVKA_BOLD_TTF,
            FontWeight::ExtraBold => IOSEVKA_EXTRABOLD_TTF,
            FontWeight::Heavy => IOSEVKA_HEAVY_TTF,
        }
    }

    fn name(self) -> &'static str {
        match self {
            FontWeight::Thin => "iosevka-thin",
            FontWeight::ExtraLight => "iosevka-extralight",
            FontWeight::Light => "iosevka-light",
            FontWeight::Regular => "iosevka-regular",
            FontWeight::Medium => "iosevka-medium",
            FontWeight::SemiBold => "iosevka-semibold",
            FontWeight::Bold => "iosevka-bold",
            FontWeight::ExtraBold => "iosevka-extrabold",
            FontWeight::Heavy => "iosevka-heavy",
        }
    }
}

/// Active body-font weight. Sentinel value `u8::MAX` means
/// "never installed", so the very first `apply_theme` call always
/// pushes the font onto the egui context.
static ACTIVE_FONT_WEIGHT: AtomicU8 = AtomicU8::new(u8::MAX);

/// Active title-font weight (pane title + section header). Defaults
/// to [`FontWeight::Heavy`] so titles read clearly against any
/// accent fill without relying on per-glyph outlines or shadow
/// tricks. Same sentinel scheme as the body weight.
static ACTIVE_TITLE_WEIGHT: AtomicU8 = AtomicU8::new(u8::MAX);

/// Replace the active body-font weight. Takes effect on the next
/// `apply_theme` call — `apply_theme` notices the change and
/// re-issues `ctx.set_fonts` once.
pub fn set_font_weight(w: FontWeight) {
    ACTIVE_FONT_WEIGHT.store(w.as_u8(), Ordering::Relaxed);
}

/// Read the currently-selected body-font weight. Returns
/// [`FontWeight::default`] before the first `set_font_weight` /
/// `apply_theme` call.
pub fn font_weight() -> FontWeight {
    let v = ACTIVE_FONT_WEIGHT.load(Ordering::Relaxed);
    if v == u8::MAX {
        FontWeight::default()
    } else {
        FontWeight::from_u8(v)
    }
}

/// Replace the active title-font weight (pane title + section
/// header). Takes effect on the next `apply_theme` call.
pub fn set_title_weight(w: FontWeight) {
    ACTIVE_TITLE_WEIGHT.store(w.as_u8(), Ordering::Relaxed);
}

/// Read the currently-selected title-font weight. Default is
/// [`FontWeight::Heavy`].
pub fn title_weight() -> FontWeight {
    let v = ACTIVE_TITLE_WEIGHT.load(Ordering::Relaxed);
    if v == u8::MAX {
        FontWeight::Heavy
    } else {
        FontWeight::from_u8(v)
    }
}

/// Named font family the title-paint sites (`floating::pane_title`,
/// `widgets::foldable::section_header`) ask for. The body family
/// stays as `FontFamily::Proportional` so every other widget keeps
/// the body weight without changes.
pub const TITLE_FAMILY_NAME: &str = "mara-title";

/// `true` once `install_fonts` has pushed a `FontDefinitions` that
/// binds [`TITLE_FAMILY_NAME`] AND egui has begun the next pass
/// (i.e. `set_fonts` actually took effect). Title paint sites read
/// this to decide whether to ask for the named family or fall back
/// to `Proportional` — looking up an unbound `FontFamily::Name` is
/// a hard panic in epaint.
pub static TITLE_FONT_READY: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Resolve the [`egui::FontFamily`] the title sites should use this
/// frame. Returns the named family once the title font is bound;
/// falls back to `Proportional` until then so frame 0 doesn't panic.
pub fn title_font_family() -> egui::FontFamily {
    if TITLE_FONT_READY.load(Ordering::Acquire) {
        egui::FontFamily::Name(TITLE_FAMILY_NAME.into())
    } else {
        egui::FontFamily::Proportional
    }
}

/// Push a `FontDefinitions` that binds:
///
/// * The selected body weight as **face 0** of `Proportional` and
///   `Monospace` — every native egui widget (Label, Button, …)
///   picks it up automatically.
/// * The selected title weight under [`TITLE_FAMILY_NAME`] as
///   `FontFamily::Name(...)` so the pane / section title sites can
///   paint with a heavier face independently of the body.
/// * Every iconflow Fluent UI variant under its own named family
///   (`crate::icons::install_iconflow_fonts`).
///
/// Called from `apply_theme` whenever either weight changes; the
/// dedup atomic in `apply_theme` keeps the cost to a single
/// `ctx.set_fonts` per change.
pub fn install_fonts(ctx: &egui::Context, body: FontWeight, title: FontWeight) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        body.name().into(),
        std::sync::Arc::new(egui::FontData::from_static(body.ttf())),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, body.name().into());
    }
    // Title family — only adds a second `FontData` if the title
    // weight differs from the body weight. egui de-dups by key so
    // re-using the body's `font_data` entry for both registrations
    // would also work, but inserting a separate entry keeps the
    // ownership semantics clean and matches what FontDefinitions
    // expects.
    if title != body {
        fonts.font_data.insert(
            title.name().into(),
            std::sync::Arc::new(egui::FontData::from_static(title.ttf())),
        );
    }
    fonts
        .families
        .entry(egui::FontFamily::Name(TITLE_FAMILY_NAME.into()))
        .or_default()
        .insert(0, title.name().into());

    crate::icons::install_iconflow_fonts(&mut fonts);
    ctx.set_fonts(fonts);
    // Do NOT flip the ready flags here — `ctx.set_fonts` queues the
    // new `FontDefinitions` for the NEXT pass, so any paint that
    // happens in the rest of THIS pass would still find the
    // FontFamily::Name unbound and panic ("FontFamily::Name(...) is
    // not bound to any fonts"). The flags are flipped one frame
    // later by `apply_theme` on its `else` branch (no install
    // needed → fonts have been alive on the ctx for at least one
    // pass), and by then egui has actually accepted the binding.
    ctx.request_repaint();
}

/// Apply the mara theme to the given egui context. Pure egui —
/// no framework deps. Hosts call this once per frame (`bevy_mara`
/// does it from a system; `mara::window` does it through
/// `MaraHostCtx`). The function de-dupes internally via a
/// static cache so re-calling with the same `(accent, opacity)`
/// skips the `ctx.set_style` / `ctx.set_fonts` work.
pub fn apply_theme(ctx: &egui::Context, accent: AccentColor, opacity: GlassOpacity) {
    use core::sync::atomic::{AtomicU32, AtomicUsize};

    // Packed (r, g, b, a) cache. `u32::MAX` is used as the
    // "never-applied" sentinel — no real colour hashes to that,
    // so the first call always passes the dedup check.
    static LAST_ACCENT: AtomicU32 = AtomicU32::new(u32::MAX);
    static LAST_OPACITY: AtomicU8 = AtomicU8::new(0);
    static LAST_THEME_NAME_PTR: AtomicUsize = AtomicUsize::new(0);
    // 0 = pastel off, 1 = pastel on, u8::MAX = never-applied. Flips
    // here force a re-push of the Visuals because surface fills
    // sample the toggle at paint time.
    static LAST_PASTEL: AtomicU8 = AtomicU8::new(u8::MAX);
    // Body + title weights currently bound on the egui context.
    // `u8::MAX` is the "never-installed" sentinel; the first
    // `apply_theme` call always installs fonts, and any later
    // `set_font_weight` / `set_title_weight` change is detected by
    // comparing these against the live atomics.
    static LAST_BODY_WEIGHT: AtomicU8 = AtomicU8::new(u8::MAX);
    static LAST_TITLE_WEIGHT: AtomicU8 = AtomicU8::new(u8::MAX);

    let th = theme();
    // Two accent streams:
    //   • `accent_col` — pastelized when `Theme::pastel_accent` is
    //     on. Flows through every chrome derivation (panel / section
    //     fill, glass tint, widget border, ribbon button paint).
    //     This is what `set_active_accent` / `active_accent()`
    //     publishes, so callers that already use that getter
    //     automatically pick up the pastel pull.
    //   • `accent_raw` — the user's pick verbatim. Stored under
    //     `set_raw_accent`; `section_title_color` reads it for the
    //     `TextColorMode::Accent` branch so titles never pastelize.
    let accent_raw = accent.0;
    let accent_col = if th.pastel_accent {
        adapt_accent_to_mode(accent_raw, th.is_light)
    } else {
        accent_raw
    };
    set_raw_accent(accent_raw);
    let body_w = font_weight();
    let title_w = title_weight();
    let body_u8 = body_w.as_u8();
    let title_u8 = title_w.as_u8();
    if LAST_BODY_WEIGHT.load(Ordering::Relaxed) != body_u8
        || LAST_TITLE_WEIGHT.load(Ordering::Relaxed) != title_u8
    {
        install_fonts(ctx, body_w, title_w);
        LAST_BODY_WEIGHT.store(body_u8, Ordering::Relaxed);
        LAST_TITLE_WEIGHT.store(title_u8, Ordering::Relaxed);
    } else {
        // No install needed → set_fonts (if any) ran on a PREVIOUS
        // frame, so by now egui has bound the FontFamily::Name(...)
        // entries we registered. Flip the ready flags now so paint
        // sites stop falling back to `Proportional` and start using
        // the iconflow + title families.
        if !crate::icons::ICONFLOW_FONTS_READY.load(Ordering::Relaxed) {
            crate::icons::ICONFLOW_FONTS_READY.store(true, Ordering::Release);
        }
        if !TITLE_FONT_READY.load(Ordering::Relaxed) {
            TITLE_FONT_READY.store(true, Ordering::Release);
        }
    }

    // Pack the accent Color32 as u32: (r << 24) | (g << 16) | (b << 8) | a.
    let packed = ((accent_col.r() as u32) << 24)
        | ((accent_col.g() as u32) << 16)
        | ((accent_col.b() as u32) << 8)
        | (accent_col.a() as u32);
    // Use the `&'static str` pointer as the theme identity — names
    // are interned `&'static str`s built from string literals, so
    // pointer equality matches name equality for built-ins and any
    // user theme using a literal.
    let theme_ptr = th.name.as_ptr() as usize;
    let pastel_u8 = th.pastel_accent as u8;
    if LAST_ACCENT.load(Ordering::Relaxed) == packed
        && LAST_OPACITY.load(Ordering::Relaxed) == opacity.0
        && LAST_THEME_NAME_PTR.load(Ordering::Relaxed) == theme_ptr
        && LAST_PASTEL.load(Ordering::Relaxed) == pastel_u8
    {
        return;
    }
    LAST_ACCENT.store(packed, Ordering::Relaxed);
    LAST_OPACITY.store(opacity.0, Ordering::Relaxed);
    LAST_THEME_NAME_PTR.store(theme_ptr, Ordering::Relaxed);
    LAST_PASTEL.store(pastel_u8, Ordering::Relaxed);
    // Publish the accent / opacity globals BEFORE applying the
    // visuals — every paint site downstream (titles, borders,
    // glass-alpha helpers) reads from these atomics rather than
    // re-deriving from `theme()`.
    set_raw_accent(accent_raw);
    set_glass_opacity(opacity.0);
    set_active_accent(accent_col);
    apply_theme_to(ctx, accent, opacity);
}

/// Apply the mara theme's *visuals* to `ctx` unconditionally,
/// bypassing `apply_theme`'s global de-dup cache. Useful for
/// *secondary* `egui::Context`s — `apply_theme`'s cache is keyed
/// on the theme state, not on the context, so once the parent
/// ctx has been styled the cache early-returns and any sibling
/// sub-context (e.g. the one `node_view::show` runs graph in)
/// never receives the visuals. Calling this directly skips that
/// gate.
///
/// **Does NOT publish globals** (`set_raw_accent` /
/// `set_glass_opacity` / `set_active_accent`). The caller is
/// expected to have already written those — `apply_theme` does
/// it for the primary ctx, and a sub-ctx caller should be
/// passing values *already published* (typically via
/// `active_accent()` / `glass_opacity()`), so re-writing here
/// would just double-apply pastel adaptation and corrupt the
/// downstream paint sites that read the same globals.
pub fn apply_theme_to(
    ctx: &egui::Context,
    accent: AccentColor,
    // Argument retained for API symmetry with [`apply_theme`]; the
    // alpha levels in the visuals below read directly from the
    // `glass_opacity()` global, so the caller's value is informational
    // only. Renamed `_opacity` to silence the unused-arg lint.
    _opacity: GlassOpacity,
) {
    let th = theme();
    let accent_raw = accent.0;
    let accent_col = if th.pastel_accent {
        adapt_accent_to_mode(accent_raw, th.is_light)
    } else {
        accent_raw
    };
    let _ = accent_raw;

    // Glass variants of every neutral bg, so EVERY egui widget that
    // pulls from `Visuals` (buttons, inputs, sliders, text fields,
    // combo boxes, progress bars, ...) inherits the look from the
    // active theme automatically. `pane_fill` / `section_fill`
    // resolve the panel/section ColorMode so the GAME profile's
    // accent-derived panel actually flows into Visuals.panel_fill.
    let glass_panel = glass_fill(pane_fill(accent_col), accent_col, glass_alpha_window());
    let glass_card = glass_fill(section_fill(accent_col), accent_col, glass_alpha_card());
    let glass_hover = glass_fill(th.bg_hover, accent_col, glass_alpha_card());

    let unified_border = widget_border(accent_col);
    let stroke_w = th.stroke.border_width;

    // Pick the egui visual base matching the active theme's
    // brightness mode. Light variants need `Visuals::light()` so
    // the host's default text / hyperlink / faint_bg colours don't
    // come back as light-on-light from the dark base.
    let mut visuals = if th.is_light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    visuals.panel_fill = glass_panel;
    visuals.window_fill = glass_panel;
    visuals.window_stroke = egui::Stroke::new(stroke_w, unified_border);
    // `extreme_bg_color` is the egui visual every native input
    // (DragValue, TextEdit, ScrollArea track, …) pulls from. Route
    // it through `track_fill` so PRO keeps the dark sunken look and
    // GAME blends into the accent panel.
    visuals.extreme_bg_color = track_fill(accent_col);
    visuals.faint_bg_color = glass_card;
    visuals.code_bg_color = glass_card;
    visuals.override_text_color = Some(th.palette.text_primary);
    // Force the gamma-correct (linear) coverage→alpha curve for text in
    // both modes. egui's dark-mode default is `TwoCoverageMinusCoverageSq`,
    // which deliberately fattens glyph edges to make light text on dark
    // backgrounds look bolder. On a saturated accent fill (yellow / cyan
    // / lime) that fattened edge reads as a visible coloured halo around
    // every glyph — the "border around the text" the user sees only when
    // the accent is applied. `Linear` blends the coverage straight, so
    // the AA edge is a single 1-px transition between text and bg.
    visuals.text_alpha_from_coverage = egui::epaint::AlphaFromCoverage::Linear;
    visuals.selection.bg_fill = tinted_surface(accent_col);
    visuals.selection.stroke = egui::Stroke::new(stroke_w.max(1.0), accent_col);
    visuals.hyperlink_color = accent_col;

    let r = egui::CornerRadius::same(th.shape.radius_widget);
    let widget = |bg: egui::Color32, fg_stroke: egui::Color32, bg_stroke: egui::Color32| {
        egui::style::WidgetVisuals {
            bg_fill: bg,
            weak_bg_fill: bg,
            bg_stroke: egui::Stroke::new(stroke_w, bg_stroke),
            fg_stroke: egui::Stroke::new(1.0, fg_stroke),
            corner_radius: r,
            expansion: 0.0,
        }
    };
    // Native egui interactive widgets (Button, DragValue,
    // Checkbox, RadioButton, ComboBox header, …) all paint their
    // background from `widgets.inactive.bg_fill` / `weak_bg_fill`.
    // Routing it through `track_fill` keeps these inputs at the
    // same brightness tier as the mara search field / dropdown
    // trigger / slider track instead of dropping to the dark
    // `bg_raised` panel colour. PRO unchanged (track_fill returns
    // `bg_input`); GAME now lifts inputs to `panel + 10 % white`.
    let input_bg = track_fill(accent_col);
    let glass_input = glass_fill(input_bg, accent_col, glass_alpha_card());
    visuals.widgets.noninteractive = widget(glass_panel, th.palette.text_secondary, unified_border);
    visuals.widgets.inactive = widget(glass_input, th.palette.text_primary, unified_border);
    visuals.widgets.hovered = widget(
        glass_hover,
        th.palette.text_primary,
        th.palette.border_inner,
    );
    visuals.widgets.active = widget(accent_col, th.palette.text_primary, accent_col);
    visuals.widgets.open = widget(
        glass_hover,
        th.palette.text_primary,
        th.palette.border_inner,
    );

    let mut style = (*ctx.style()).clone();
    style.visuals = visuals;

    // Slightly roomier controls — interacts at 20 px (was 18) and
    // buttons get 8×4 padding (was 6×2) so rows don't feel cramped
    // against each other.
    style.spacing.item_spacing = egui::vec2(6.0, 3.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.indent = 14.0;
    style.spacing.window_margin = egui::Margin::ZERO;
    style.spacing.interact_size.y = 20.0;
    // Tight slider track. Combined with no inline `.text(...)` label
    // and no `.show_value()` suffix, this leaves enough right-cell
    // space for the slider PLUS the current value without pushing
    // the section card wider than its pinned inner width.
    style.spacing.slider_width = 90.0;
    style.spacing.icon_width = 14.0;
    style.spacing.icon_spacing = 6.0;

    // Scrollbar — always a thin line. The bar barely thickens on
    // hover (2 → 3 px); the visible cue is the handle's opacity
    // jumping from soft to full instead of the whole bar swelling.
    // Track has zero opacity in every state, so what the user sees
    // is just the handle line (no gutter painted around it).
    //
    // Handle corner radius flows from `widgets.X.corner_radius` =
    // `theme.radius_widget` — PRO 2 px (very small chamfer), GAME 0
    // (square). Both match the kit's overall corner language.
    //
    // `foreground_color = true` makes the handle pull from each
    // state's `fg_stroke.color` (accent variants we set below)
    // instead of `bg_fill`, so scrollbars tint per-accent without
    // dragging every other widget bg with them.
    style.spacing.scroll = egui::style::ScrollStyle {
        floating: true,
        bar_width: 3.0,
        floating_width: 2.0,
        floating_allocated_width: 3.0,
        handle_min_length: 16.0,
        bar_inner_margin: 2.0,
        bar_outer_margin: 0.0,
        foreground_color: true,
        dormant_background_opacity: 0.0,
        active_background_opacity: 0.0,
        interact_background_opacity: 0.0,
        dormant_handle_opacity: 0.55,
        active_handle_opacity: 0.85,
        interact_handle_opacity: 1.00,
    };
    // Rest: a dimmed-accent track handle that still belongs to the
    // accent family. Hover: full ACCENT_HOVER. Drag: ACCENT_PRESSED.
    // `fg_stroke` is also used for fine foreground elements
    // (checkmarks, focus rings) — re-tinting them to accent reads as
    // an improvement, not a regression.
    let accent_dim =
        egui::Color32::from_rgba_unmultiplied(accent_col.r(), accent_col.g(), accent_col.b(), 160);
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, accent_dim);
    style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, accent_hover());
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, accent_pressed());
    style.text_styles = [
        (
            egui::TextStyle::Heading,
            egui::FontId::new(16.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Body,
            egui::FontId::new(13.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Monospace,
            egui::FontId::new(13.0, egui::FontFamily::Monospace),
        ),
        (
            egui::TextStyle::Button,
            egui::FontId::new(13.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Small,
            egui::FontId::new(12.0, egui::FontFamily::Proportional),
        ),
    ]
    .into();

    // Animation timing now flows from the active theme. Drives
    // every `animate_bool` consumer (foldable chevron + banner,
    // hover lifts, accordion height, etc.). PRO ships a snappy
    // 0.15 s; GAME a deliberate 0.35 s for the cinematic feel.
    style.animation_time = th.container.animation_time;

    // Performance — parallel tessellation. egui's painter→mesh
    // pass runs on rayon when this is on, splitting large shape
    // batches across CPU cores. Defaults to true in egui 0.33
    // already; we set it explicitly so a host can't accidentally
    // disable it elsewhere and quietly halve our render speed.
    ctx.tessellation_options_mut(|opts| {
        opts.parallel_tessellation = true;
    });

    ctx.set_style(style);
}

/// Darker/muted version of an accent colour — used for "selected" row
/// fills where the full-strength accent would be too loud.
fn tinted_surface(c: egui::Color32) -> egui::Color32 {
    // 35 % of accent over the active theme's raised background.
    let bg = theme().bg_raised;
    let f = 0.35;
    let lerp = |a: u8, b: u8| ((a as f32) * (1.0 - f) + (b as f32) * f).round() as u8;
    egui::Color32::from_rgb(
        lerp(bg.r(), c.r()),
        lerp(bg.g(), c.g()),
        lerp(bg.b(), c.b()),
    )
}

/// Convert a linear-sRGB `[f32;3]` to an egui [`egui::Color32`].
/// Matches the visual tone a wgpu 3D view renders — handy when you
/// want the egui UI accent to match a material colour from the scene.
pub fn srgb_to_egui(rgb: [f32; 3]) -> egui::Color32 {
    let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    egui::Color32::from_rgb(to_u8(rgb[0]), to_u8(rgb[1]), to_u8(rgb[2]))
}

/// Uppercase accent section header. Used both by left panels
/// (`CollapsingHeader::new(section_caps(…))`) and by the right
/// inspector — keeps the visual language identical on both sides.
///
/// Pops against the Light body font via `.strong()` (darker render)
/// + caps + accent colour. Per-weight font selection isn't available:
///   see the comment on the embedded-font block above for why.
///
/// Size: 12 pt body baseline + 15 % bump so section titles read
/// clearly larger than body copy inside the same card.
pub fn section_caps(label: &str, accent: egui::Color32) -> egui::RichText {
    let th = theme();
    let mut t = egui::RichText::new(label.to_uppercase())
        .strong()
        .size(th.container.title_size)
        .color(accent);
    let spacing = th.container.title_letter_spacing;
    if spacing > 0.0 {
        t = t.extra_letter_spacing(spacing);
    }
    t
}

/// Accent colour applied to *body widget* fills — slider /
/// progress-bar fills, button hover-and-press tints, toggle
/// on-state, etc. The title banner / corner ticks keep the pure
/// accent so they remain the brightest surface in each card; body
/// widgets get a darkened variant (lerp toward black by
/// [`Theme.body_accent_darken`]) so they don't match the banner.
pub fn body_accent(accent: egui::Color32) -> egui::Color32 {
    let t = theme().text.body_accent_darken;
    if t <= 0.0 {
        accent
    } else {
        lerp_rgb(accent, egui::Color32::BLACK, t.clamp(0.0, 1.0))
    }
}

/// Hi-contrast accent for *visual anchors* — corner ticks, focus
/// reticles, status pips. The user-picked accent gets pulled hard
/// toward the opposite luma extreme of the active panel so the
/// anchor stays vivid even on themes where the accent already
/// matches the surface luma. Saturation is boosted ~15 % so the
/// result reads as a SATURATED-then-lifted/dropped colour, not a
/// desaturated grey.
///
/// Implementation goes through HSL via `pastel`: only lightness +
/// saturation are touched, so the user's hue is preserved exactly
/// (yellow stays yellow, red stays red, etc.).
pub fn high_contrast_accent(accent: egui::Color32) -> egui::Color32 {
    type HighContrastAccentCache =
        std::sync::OnceLock<std::sync::RwLock<Option<((u32, bool), u32)>>>;

    // Single-slot memo: if the (accent, is_light) input matches the
    // last call, skip the HSL roundtrip and return the cached
    // output. Maracore's section bracket paint calls this on
    // every section every frame; with N sections at 60 fps that's
    // N × 60 pastel conversions / second otherwise.
    static CACHE: HighContrastAccentCache = std::sync::OnceLock::new();
    fn pack(c: egui::Color32) -> u32 {
        ((c.r() as u32) << 24) | ((c.g() as u32) << 16) | ((c.b() as u32) << 8) | (c.a() as u32)
    }
    fn unpack(p: u32) -> egui::Color32 {
        egui::Color32::from_rgba_premultiplied(
            ((p >> 24) & 0xff) as u8,
            ((p >> 16) & 0xff) as u8,
            ((p >> 8) & 0xff) as u8,
            (p & 0xff) as u8,
        )
    }
    let is_light = theme().is_light;
    let key = (pack(accent), is_light);
    let lock = CACHE.get_or_init(|| std::sync::RwLock::new(None));
    if let Some((k, v)) = *read_unpoisoned(lock)
        && k == key
    {
        return unpack(v);
    }

    use pastel::Color as PastelColor;
    let c = PastelColor::from_rgb(accent.r(), accent.g(), accent.b());
    let hsl = c.to_hsla();
    let target_l = if is_light { 0.18 } else { 0.88 };
    let new_l = hsl.l + (target_l - hsl.l) * 0.70;
    let new_s = (hsl.s * 1.15).min(1.0);
    let adjusted = PastelColor::from_hsla(hsl.h, new_s, new_l, 1.0);
    let rgba = adjusted.to_rgba();
    let out = egui::Color32::from_rgb(rgba.r, rgba.g, rgba.b);
    *write_unpoisoned(lock) = Some((key, pack(out)));
    out
}

pub fn fg_dim() -> egui::Color32 {
    TEXT_SECONDARY
}

/// Tracks how many "appearance sessions" an `id` has had — the
/// counter increments every time the id is missing from a frame
/// and then re-appears (e.g. a pane closes and reopens). Use this
/// counter as a salt on per-id animation ids so each fresh
/// appearance gets a clean animation cycle instead of replaying
/// the previous session's locked-in state.
pub fn appearance_session(ctx: &egui::Context, id: egui::Id) -> u64 {
    let key_seen = id.with("mara_last_seen_pass");
    let key_sess = id.with("mara_session_count");
    let now = ctx.cumulative_pass_nr();
    let last: Option<u64> = ctx.data(|d| d.get_temp(key_seen));
    let mut sess: u64 = ctx.data(|d| d.get_temp(key_sess)).unwrap_or(0);
    let bumped = !matches!(last, Some(p) if p + 1 == now);
    if bumped {
        sess = sess.wrapping_add(1);
    }
    ctx.data_mut(|d| {
        d.insert_temp(key_seen, now);
        d.insert_temp(key_sess, sess);
    });
    sess
}

const SCRAMBLE_CHARS: &[char] = &[
    '!', '<', '>', '-', '_', '/', '[', ']', '{', '}', '=', '+', '*', '^', '?', '#',
];

/// GAME motion #17 — scramble-decode. Returns a display string in
/// which any character that *just appeared* (or changed) cycles
/// through `SCRAMBLE_CHARS` for a brief staggered duration before
/// locking on its final character. Locks LEFT-TO-RIGHT — character
/// `i` is fully settled by `t = base + i × STAGGER`, so a label
/// "decodes" from the left edge inward.
///
/// `active` gates the cycle: when **false** the function paints
/// fresh random glyphs every frame WITHOUT updating the
/// prev-text/start-time state. When `active` flips to **true**, the
/// stored prev is stale, the cycle starts from scratch on that
/// frame, and the scramble plays from the user's perspective. Use
/// this to wait until the host element has finished its fade-in
/// (e.g. `ui.opacity() >= 0.95`) so the scramble doesn't burn off
/// while the title is still invisible.
///
/// Calls `request_repaint` while any character is still scrambling
/// (or while gated, so the random glyphs keep cycling).
pub fn scramble_text(ctx: &egui::Context, id: egui::Id, current: &str, active: bool) -> String {
    /// Staggered delay between adjacent characters' lock times.
    /// `0.07` was the legacy default, but with `Pane`'s
    /// per-section staggered fade-in landing the last container at
    /// ~0.81 s, the first container's cipher finished too early —
    /// most letters had already locked by the time the user could
    /// see all the containers. Bumped to `0.10` so the per-letter
    /// stagger lingers and reads as ongoing decoding when the
    /// later containers arrive.
    const STAGGER: f64 = 0.10;
    /// Minimum scramble duration for the leftmost character. Bumped
    /// from `0.42` for the same "breathing room" reason — see
    /// `STAGGER`.
    const MIN_DUR: f64 = 0.65;

    let now = ctx.input(|i| i.time);
    let id_seed = id.value().wrapping_mul(0x9E37_79B9);
    let frame_phase = (now * 70.0) as u64;

    // Gated path — paint random glyphs continuously, never touch
    // the prev/start state. When `active` flips true the stored
    // prev (None or stale) doesn't match `current`, so the active
    // path will reset start_time and begin a fresh cycle.
    if !active {
        ctx.request_repaint();
        return current
            .chars()
            .enumerate()
            .map(|(i, c)| {
                if c.is_whitespace() {
                    return c;
                }
                let h = id_seed
                    .wrapping_add((i as u64).wrapping_mul(0xBF58_476D))
                    .wrapping_add(frame_phase.wrapping_mul(0x94D0_49BB));
                SCRAMBLE_CHARS[(h as usize) % SCRAMBLE_CHARS.len()]
            })
            .collect();
    }

    let key_start = id.with("mara_scramble_start");
    let key_prev = id.with("mara_scramble_prev");
    let prev: Option<String> = ctx.data(|d| d.get_temp(key_prev));
    let mut start: f64 = ctx.data(|d| d.get_temp(key_start)).unwrap_or(now);
    // Restart scramble whenever the text changes (or on first sight,
    // including the frame `active` first flips to true).
    if prev.as_deref() != Some(current) {
        start = now;
        ctx.data_mut(|d| {
            d.insert_temp(key_prev, current.to_string());
            d.insert_temp(key_start, start);
        });
    }

    let elapsed = now - start;
    let mut still_scrambling = false;
    let display: String = current
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if c.is_whitespace() {
                return c;
            }
            let lock_time = MIN_DUR + (i as f64) * STAGGER;
            if elapsed < lock_time {
                still_scrambling = true;
                let h = id_seed
                    .wrapping_add((i as u64).wrapping_mul(0xBF58_476D))
                    .wrapping_add(frame_phase.wrapping_mul(0x94D0_49BB));
                SCRAMBLE_CHARS[(h as usize) % SCRAMBLE_CHARS.len()]
            } else {
                c
            }
        })
        .collect();

    if still_scrambling {
        ctx.request_repaint();
    }
    display
}

/// Whether the scramble cycle for `scramble_id` is still in progress
/// for `current`. Mirrors the timing constants inside [`scramble_text`]
/// so secondary effects — chromatic aberration ghosts, focus glows —
/// can fire while the cipher is decoding without re-running the
/// scramble themselves.
///
/// Returns `true` when the stored prev text differs from `current`
/// (the scramble is about to start) or when elapsed is below the
/// last-character lock time.
pub fn scramble_active(ctx: &egui::Context, scramble_id: egui::Id, current: &str) -> bool {
    // Keep these in sync with `scramble_text`.
    const STAGGER: f64 = 0.10;
    const MIN_DUR: f64 = 0.65;
    let key_start = scramble_id.with("mara_scramble_start");
    let key_prev = scramble_id.with("mara_scramble_prev");
    let prev: Option<String> = ctx.data(|d| d.get_temp(key_prev));
    if prev.as_deref() != Some(current) {
        return true;
    }
    let now = ctx.input(|i| i.time);
    let start: f64 = ctx.data(|d| d.get_temp(key_start)).unwrap_or(now);
    let elapsed = now - start;
    let total = MIN_DUR + (current.chars().count() as f64) * STAGGER;
    elapsed < total
}

/// Periodic single-letter "glitch" overlay layered on top of the
/// stabilised text. Once per `BUCKET_PERIOD` (5 s), a deterministic-
/// random NON-whitespace character is replaced with a `SCRAMBLE_CHARS`
/// symbol for `GLITCH_DUR` (~180 ms) at a random phase within the
/// bucket. Outside that window the text is returned unchanged.
///
/// Intended to follow `scramble_text` so the title plays its decode
/// cycle on appear, then the occasional glitch flickers a single
/// letter every few seconds against the locked text.
pub fn glitch_text(ctx: &egui::Context, id: egui::Id, base: &str) -> String {
    const GLITCH_DUR: f64 = 0.18;

    // Collect non-whitespace character indices — those are the only
    // valid targets. If the picked target is whitespace we'd render
    // it unchanged (boring), so pre-filter and use the random hash
    // to pick from this list.
    let candidates: Vec<usize> = base
        .chars()
        .enumerate()
        .filter(|(_, c)| !c.is_whitespace())
        .map(|(i, _)| i)
        .collect();
    if candidates.is_empty() {
        return base.to_string();
    }

    // Per-id random bucket period in [3.0, 9.0] s — so multiple
    // titles glitch on independent cadences and never sync up.
    let id_seed = id.value().wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let period_h = id_seed.wrapping_mul(0xC229_6164_8C84_38AB);
    let bucket_period = 3.0 + ((period_h as f64) / (u64::MAX as f64)) * 6.0;

    let now = ctx.input(|i| i.time);
    let bucket = (now / bucket_period).floor() as u64;
    let bucket_start = (bucket as f64) * bucket_period;
    let phase = now - bucket_start;

    let h1 = id_seed.wrapping_add(bucket.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    let glitch_start = ((h1 as f64) / (u64::MAX as f64)) * (bucket_period - GLITCH_DUR);
    // Pick TWO independent (idx, scramble_char) pairs. Both are
    // hashed from the same bucket+id so they're stable for the
    // duration of the glitch. Independence between the two picks
    // is intentional — sometimes they land on the same character,
    // which is fine.
    let h2 = h1.wrapping_mul(0x94D0_49BB_1331_11EB);
    let target_idx_a = candidates[(h2 as usize) % candidates.len()];
    let h3 = h2.wrapping_mul(0xD680_5F76_18FB_F0FB);
    let scramble_idx_a = (h3 as usize) % SCRAMBLE_CHARS.len();
    let h4 = h3.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    let target_idx_b = candidates[(h4 as usize) % candidates.len()];
    let h5 = h4.wrapping_mul(0x165667B19E3779F9);
    let scramble_idx_b = (h5 as usize) % SCRAMBLE_CHARS.len();

    ctx.request_repaint_after(std::time::Duration::from_millis(33));

    let in_glitch = phase >= glitch_start && phase < glitch_start + GLITCH_DUR;
    if !in_glitch {
        return base.to_string();
    }

    base.chars()
        .enumerate()
        .map(|(i, c)| {
            if i == target_idx_a {
                SCRAMBLE_CHARS[scramble_idx_a]
            } else if i == target_idx_b {
                SCRAMBLE_CHARS[scramble_idx_b]
            } else {
                c
            }
        })
        .collect()
}

/// Periodic **chromatic-aberration** trigger (Volume-2 effect #47).
///
/// Returns the current ghost-offset in pixels (0.0 when inactive). Per-id
/// random firing every 5–13 seconds, ramping `0 → peak → 0` over 220 ms in
/// a triangular envelope. Callers paint a red ghost at `pos.x − offset`,
/// a cyan ghost at `pos.x + offset`, then the main text on top — the eye
/// reads the brief edge-fringes as a CRT-style chromatic split.
///
/// Same hash-driven timing pattern as [`glitch_text`] so different titles
/// fire on staggered, deterministic schedules.
pub fn chromatic_aberration_offset(ctx: &egui::Context, id: egui::Id) -> f32 {
    /// Total split duration, peak in the middle.
    const DUR: f64 = 0.28;
    /// Maximum pixel offset of each ghost from the centre, ALONG the
    /// text's reading direction. Caller is responsible for projecting
    /// this onto the appropriate axis (screen-X for horizontal text,
    /// screen-Y for vertical / rotated text).
    const PEAK: f32 = 8.0;

    let id_seed = id.value().wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let period_h = id_seed.wrapping_mul(0xC229_6164_8C84_38AB);
    // 5 + [0, 8) seconds → 5–13 s between firings, deterministic per id.
    let bucket_period = 5.0 + ((period_h as f64) / (u64::MAX as f64)) * 8.0;
    let now = ctx.input(|i| i.time);
    let bucket = (now / bucket_period).floor() as u64;
    let bucket_start = (bucket as f64) * bucket_period;
    let phase = now - bucket_start;
    let h1 = id_seed.wrapping_add(bucket.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    let trigger_start = ((h1 as f64) / (u64::MAX as f64)) * (bucket_period - DUR);

    if phase < trigger_start || phase >= trigger_start + DUR {
        return 0.0;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(33));
    let t = ((phase - trigger_start) / DUR).clamp(0.0, 1.0) as f32;
    // Triangular envelope: 0 → 1 at t=0.5, then back to 0.
    let strength = 1.0 - (2.0 * t - 1.0).abs();
    PEAK * strength
}

// ─── Design-system tokens ────────────────────────────────────────────
//
// Every panel should lay out against THESE instead of ad-hoc `add_space`
// calls. Keeps rhythm consistent and lets the whole UI be re-tuned
// from one place. Scale is a 4 px grid; sizes are named by use, not
// by pixel count, so the numbers can evolve without a find-and-replace.

pub mod space {
    /// Between tightly-related items inside one row (label↔chip, glyph↔text).
    pub const TIGHT: f32 = 2.0;
    /// Between adjacent rows inside one section (label rows, slider rows).
    pub const ROW: f32 = 2.0;
    /// Between a row and a sub-block inside one section.
    pub const BLOCK: f32 = 4.0;
    /// Between distinct section cards in a panel. Slight gap so the
    /// rounded frames don't kiss each other edge-to-edge.
    pub const SECTION: f32 = 1.0;
}

pub mod radius {
    /// Default radius for every in-panel control — sliders,
    /// progress bars, buttons, number inputs, colour pickers,
    /// combo boxes, key chips. Tuned against a *long* widget
    /// (progress bar / slider) where 2 px reads as subtly
    /// rounded.
    pub const WIDGET: u8 = 2;
    /// Radius for **compact** controls — toggles and anything
    /// else whose footprint is close to square or very short.
    /// At short widths a 2 px corner looks square-cut; a slightly
    /// larger radius compensates so the perceived roundness
    /// matches the wider widgets'.
    pub const COMPACT: u8 = 3;
    /// Progress bars, chips, bars-within-rows.
    /// *(Legacy — prefer `WIDGET` for new code.)*
    pub const SM: u8 = 3;
    /// Foldable container cards. Larger than `WIDGET` so the
    /// container reads as a surface and the widgets inside read
    /// as controls on top of it.
    pub const MD: u8 = 6;
    /// Panels, pop-overs, the biggest floating surfaces.
    pub const LG: u8 = 8;
}

// ─── Theme — pluggable visual profile ───────────────────────────────
//
// Every value that varies between visual profiles (PRO ↔ GAME ↔ user
// custom) lives in this struct. Widgets read from `theme()`; users
// switch with `set_theme(...)`. Composing a third profile is one
// struct-update expression — no widget edits needed.
//
// Fields are deliberately concrete (not `Option`s) so `Theme` is
// `Copy` and `theme()` can hand out a value rather than a borrow.
// That keeps every widget call a plain field access with no lock /
// reference threading.

/// Brightness mode of the kit — orthogonal to the theme variant.
/// Each theme variant (PRO, GAME) has a Dark and Light incarnation;
/// the user picks both axes independently.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Mode {
    Dark,
    Light,
}

impl Mode {
    pub const fn flipped(self) -> Self {
        match self {
            Mode::Dark => Mode::Light,
            Mode::Light => Mode::Dark,
        }
    }
}

/// Stable theme identity split from the display/cache name. Rendering
/// code should not branch on this either; it exists for diagnostics,
/// selectors, and future theme registries where a structured id is
/// safer than parsing `Theme::name`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ThemeId {
    pub family: &'static str,
    pub variant: &'static str,
}

/// How a surface fill is computed. Either pulled straight out of
/// the theme palette ([`ColorMode::FromBg`] — the PRO behaviour) or
/// derived from the runtime accent colour by lerping toward a
/// fixed colour ([`ColorMode::FromAccent`] — the GAME behaviour).
/// `lerp_target` lets the same enum cover Dark mode (lerp toward
/// black, dark accent surface) and Light mode (lerp toward white,
/// pale accent surface) without forking the panel-fill code.
#[derive(Copy, Clone, Debug)]
pub enum ColorMode {
    /// Use the corresponding `bg_*` field from the theme directly.
    FromBg,
    /// Compute as `lerp(lerp_target, accent, lerp_factor)`. A factor
    /// of `0.22` over `BLACK` produces the deep accent-tinted GAME
    /// dark panel; a factor of `0.18` over `WHITE` produces a pale
    /// accent-tinted GAME light panel.
    FromAccent {
        lerp_factor: f32,
        lerp_target: egui::Color32,
    },
}

/// How the title text colour is picked. Lets a theme flip the
/// "accent text on dark panel" PRO recipe to "dark text on accent
/// panel" without per-panel code.
#[derive(Copy, Clone, Debug)]
pub enum TextColorMode {
    Accent,
    Primary,
    Secondary,
    /// Pick the colour that best contrasts whatever the active theme
    /// produces for the panel fill (luma-based via
    /// [`contrast_text_for`]). Use when the panel itself is bright
    /// (GAME's accent panel) so the title text stays readable.
    ContrastWithPanel,
    /// Same idea but contrasts against the section fill.
    ContrastWithSection,
}

/// Which tab renderer a theme wants for `Normal::show_tabs`.
/// Rendering code must switch on this typed layout, not on
/// `Theme::name`, so a third theme can reuse either existing tab
/// dialect without another hardcoded family branch.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TabLayout {
    /// Folder-style tabs project from the title-facing edge of the
    /// container. This is the PRO default.
    FolderSideStrip,
    /// The container title row becomes a segmented tab strip. This
    /// is the GAME default.
    TitleRowSegmented,
}

/// How much outside padding the folder-tab strip reserves on its
/// outer edge.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TabOuterInset {
    /// No extra inset; the strip sits flush against the pane edge.
    None,
    /// Mirror the container body's cross-axis inset so tab content
    /// aligns visually with body content on the opposite edge.
    MirrorBodyInset,
}

/// Base colour rule for inactive folder-tab glyphs.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TabInactiveGlyphColor {
    /// Use `Theme::text_secondary`.
    TextSecondary,
    /// Pick black on light themes and white on dark themes.
    HighContrast,
}

/// Theme-owned tab geometry and paint dialect. The first migration
/// target is intentionally small: it captures the existing PRO and
/// GAME tab differences without forcing a wider theme-struct rewrite.
#[derive(Copy, Clone, Debug)]
pub struct TabTheme {
    pub layout: TabLayout,
    pub outer_inset: TabOuterInset,
    pub strip_thickness: f32,
    pub tab_len: f32,
    pub tab_gap: f32,
    pub tab_overlap: f32,
    pub title_row_height_multiplier: f32,
    pub folder_icon_size: f32,
    pub folder_active_radius: u8,
    pub inactive_glyph_color: TabInactiveGlyphColor,
}

/// Theme-owned pane geometry and pane chrome. This mirrors the
/// current flat pane-related fields and constants so call sites can
/// migrate incrementally.
#[derive(Copy, Clone, Debug)]
pub struct PaneTheme {
    pub inner_margin: f32,
    pub outer_span_default: f32,
    pub title_strip_thickness: f32,
    pub body_animation_time: f32,
    pub default_flow_open: f32,
    pub resize_handle_thickness: f32,
    pub min_user_flow: f32,
    pub max_user_flow: f32,
    pub min_user_span: f32,
    pub max_user_span: f32,
    pub rail_panel_gap: f32,
    pub fill_visible: bool,
    pub shadow_blur: u8,
    pub shadow_y: i8,
    pub show_title_divider: bool,
    pub title_stripes: bool,
    pub title_chromatic_aberration: bool,
    pub title_brackets: bool,
}

/// Theme-owned root view switcher layout.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ViewSwitcherLayout {
    /// Top/bottom horizontal tab or button strip.
    Horizontal,
    /// Left/right vertical icon rail.
    VerticalRail,
}

/// Theme-owned top-level view chrome and system-control semantics.
#[derive(Copy, Clone, Debug)]
pub struct ViewTheme {
    pub switcher_layout: ViewSwitcherLayout,
    pub switcher_button_min: f32,
    pub active_indicator_thickness: f32,
    pub active_indicator_inset: f32,
    pub close_icon: &'static str,
}

/// Theme-owned native-window chrome hit-test geometry.
///
/// Facades use these values when a Mara app opts into custom
/// borderless decorations. Keeping the metrics in the theme prevents
/// hard-coded demo constants from becoming the de facto resize feel.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct WindowChromeTheme {
    /// Length of each diagonal resize corner along the two window
    /// edges, in logical pixels.
    /// Straight window sides are deliberately not resize handles.
    pub resize_corner_extent: f32,
    /// Thickness of the two edge strips that form the L-shaped
    /// corner hit area. Keep this small: the side triggers should be
    /// a few pixels, not a big square.
    pub resize_corner_edge_width: f32,
}

/// Theme-owned module workspace semantics and restore affordance.
#[derive(Copy, Clone, Debug)]
pub struct ModuleTheme {
    pub allow_fullscreen_by_default: bool,
    pub workspace_restore_icon: &'static str,
    pub workspace_restore_label: &'static str,
    pub inline_workspace_button_label: &'static str,
}

/// Visual treatment for one class of slot-based ribbons.
#[derive(Copy, Clone, Debug)]
pub struct RibbonChromeTheme {
    pub button_size: f32,
    pub button_gap: f32,
    pub edge_gap: f32,
    pub panel_gap: f32,
    pub ghost_fill_alpha: u8,
    pub ghost_stroke_width: f32,
}

/// Theme-owned active-view indicator treatment.
#[derive(Copy, Clone, Debug)]
pub struct ActiveIndicatorTheme {
    pub thickness: f32,
    pub inset: f32,
    pub alpha: u8,
}

/// Theme-owned ribbon geometry and ribbon button chrome.
#[derive(Copy, Clone, Debug)]
pub struct RibbonTheme {
    pub side_button_size: f32,
    pub side_button_gap: f32,
    pub edge_gap: f32,
    pub panel_gap: f32,
    pub button_accent_fill: bool,
    pub ghost_fill_alpha: u8,
    pub ghost_stroke_width: f32,
    pub permanent: RibbonChromeTheme,
    pub view_local: RibbonChromeTheme,
    pub workspace: RibbonChromeTheme,
    pub slot_override_transition: f32,
    pub active_view_indicator: ActiveIndicatorTheme,
}

/// Theme-owned persistent Shelf geometry and chrome. Shelves are
/// docked structural regions (left/right/bottom) that reserve
/// viewport space, unlike floating ribbons/panes.
#[derive(Copy, Clone, Debug)]
pub struct ShelfTheme {
    pub side_default_size: f32,
    pub bottom_default_size: f32,
    pub min_size: f32,
    pub max_size: f32,
    pub padding: f32,
    pub resize_handle_thickness: f32,
    pub background_alpha: u8,
    pub border_width: f32,
}

/// Theme-owned container and section chrome. This mirrors the
/// current flat section fields during the migration.
#[derive(Copy, Clone, Debug)]
pub struct ContainerTheme {
    pub title_zone_thickness: f32,
    pub title_inset: f32,
    pub divider_inset: f32,
    pub title_body_gap_half: f32,
    pub default_width: f32,
    pub default_height: f32,
    pub default_min_width: f32,
    pub pod_pad_x: i8,
    pub pod_pad_y: i8,
    pub fill_mode: ColorMode,
    pub show_frame: bool,
    pub show_title_divider: bool,
    pub pad_x: i8,
    pub pad_y: i8,
    pub body_indent: f32,
    pub outer_margin_flow_title: i8,
    pub outer_margin_flow_body: i8,
    pub outer_margin_span: i8,
    pub body_inner_top_pad: f32,
    pub animation_time: f32,
    pub gap: f32,
    pub corner_ticks_inset: f32,
    pub title_brackets: bool,
    pub title_prefix: Option<&'static str>,
    pub title_letter_spacing: f32,
    pub bottom_rule: bool,
    pub show_chevron: bool,
    pub title_strip_filled: bool,
    pub title_size: f32,
    pub icon_at_end: bool,
    pub icon_size: f32,
    pub body_top_pad: f32,
    pub title_trailing_rule: bool,
    pub corner_ticks: f32,
    pub separator_strip_h: f32,
    pub separator_alpha: u8,
    pub body_inner_end_pad: f32,
}

/// Theme-owned pod rhythm. Pods remain the required container body
/// unit; this only moves their visual spacing and resize bounds out
/// of renderer-local constants.
#[derive(Copy, Clone, Debug)]
pub struct PodTheme {
    pub widget_spacing: f32,
    pub min_widget_h: f32,
    pub max_widget_h: f32,
    pub tag_row_pitch: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct ButtonTheme {
    pub row_h: f32,
    pub subtitle_row_h: f32,
    pub label_font: f32,
    pub subtitle_font: f32,
    pub glyph_font: f32,
    pub edge_pad: f32,
    pub glyph_w: f32,
    pub glyph_gap: f32,
    pub full_accent_on_press: bool,
    pub tint_rest: f32,
    pub tint_hover: f32,
    pub tint_press: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct ProgressTheme {
    pub row_h: f32,
    pub value_font: f32,
    pub segmented: bool,
    pub segments: usize,
    pub segment_gap: f32,
    pub segment_inset: f32,
    pub dim_alpha: u8,
}

#[derive(Copy, Clone, Debug)]
pub struct TreeTheme {
    pub row_h: f32,
    pub indent: f32,
    pub guide_width: f32,
    pub label_font: f32,
    pub chevron_w: f32,
    pub icon_w: f32,
    pub label_pad_l: f32,
    pub slot_w: f32,
    pub slot_gap: f32,
    pub right_pad_r: f32,
    pub row_pad_l: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct SelectTheme {
    pub row_h: f32,
    pub label_pad_l: f32,
    pub trailing_pad_r: f32,
    pub label_font: f32,
    pub trailing_font: f32,
    pub radio_outer_r: f32,
    pub radio_slot_w: f32,
    pub radio_pad_r: f32,
    pub radio_stroke_w: f32,
    pub radio_dot_inset: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct DropdownTheme {
    pub row_h: f32,
    pub item_h: f32,
    pub chevron_w: f32,
    pub pad_x: f32,
    pub text_font: f32,
    pub icon_size: f32,
    pub popup_gap: f32,
    pub popup_inner_margin: i8,
    pub item_spacing_y: f32,
    pub tint_rest: f32,
    pub tint_hover: f32,
    pub tint_press: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct SliderTheme {
    pub row_h: f32,
    pub value_font: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct ToggleTheme {
    pub row_h: f32,
    pub track_w: f32,
    pub label_track_gap: f32,
    pub knob_pad: f32,
    pub track_accent_hint: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct ReadoutTheme {
    pub row_h: f32,
    pub label_font: f32,
    pub value_font: f32,
    pub edge_pad: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct ColorTheme {
    pub row_h: f32,
    pub swatch_w: f32,
    pub label_font: f32,
    pub label_pad_l: f32,
    pub picker_gap: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct ChipTheme {
    pub height: f32,
    pub pad_x: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct KeybindingTheme {
    pub row_h: f32,
    pub key_font: f32,
    pub action_font: f32,
    pub key_pad_x: f32,
    pub key_pad_y: f32,
    pub key_action_gap: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct BadgeTheme {
    pub row_h: f32,
    pub label_col_w: f32,
    pub label_font: f32,
    pub label_pad_x: f32,
    pub label_chips_gap: f32,
    pub chip_gap_x: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct DragValueTheme {
    pub row_h: f32,
    pub input_w: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct WidgetTheme {
    pub button: ButtonTheme,
    pub progress: ProgressTheme,
    pub tree: TreeTheme,
    pub select: SelectTheme,
    pub dropdown: DropdownTheme,
    pub slider: SliderTheme,
    pub toggle: ToggleTheme,
    pub readout: ReadoutTheme,
    pub color: ColorTheme,
    pub chip: ChipTheme,
    pub keybinding: KeybindingTheme,
    pub badge: BadgeTheme,
    pub drag_value: DragValueTheme,
}

/// Theme-owned chrome icon treatment. Domain/content icon names
/// still come from callers; this group controls the recurring
/// structural icons mara itself paints.
#[derive(Copy, Clone, Debug)]
pub struct IconTheme {
    pub section_inline_scale: f32,
    pub section_icon_title_gap: f32,
    pub section_chevron_w: f32,
    pub section_chevron_gap: f32,
    pub overlay_icon_scale: f32,
    pub overlay_arrow_stroke_w: f32,
    pub overlay_arrow_shrink: f32,
    pub overlay_arrow_tip_t: f32,
    pub overlay_arrow_head_len: f32,
    pub overlay_arrow_head_half_w: f32,
    pub tree_type_icon_size: f32,
    pub tree_glyph_icon_size: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct PaletteTheme {
    pub bg_window: egui::Color32,
    pub bg_panel: egui::Color32,
    pub bg_raised: egui::Color32,
    pub bg_hover: egui::Color32,
    pub bg_input: egui::Color32,
    pub text_primary: egui::Color32,
    pub text_secondary: egui::Color32,
    pub text_disabled: egui::Color32,
    pub border_subtle: egui::Color32,
    pub border_inner: egui::Color32,
}

#[derive(Copy, Clone, Debug)]
pub struct StrokeTheme {
    pub border_alpha: u8,
    pub border_accent_tint: f32,
    pub border_width: f32,
    pub row_separator_alpha: u8,
    pub row_separator_dash: Option<(f32, f32)>,
}

#[derive(Copy, Clone, Debug)]
pub struct GlassTheme {
    pub card_factor: f32,
    pub group_factor: f32,
    pub accent_tint: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct ShapeTheme {
    pub radius_widget: u8,
    pub radius_compact: u8,
    pub radius_sm: u8,
    pub radius_md: u8,
    pub radius_lg: u8,
}

#[derive(Copy, Clone, Debug)]
pub struct TextTheme {
    pub title_color_mode: TextColorMode,
    pub title_softness: f32,
    pub body_accent_darken: f32,
    pub subcaption_prefix: Option<&'static str>,
}

#[derive(Copy, Clone, Debug)]
pub struct MotionTheme {
    pub animations_enabled: bool,
    pub button_anim_scale: f32,
    pub pane_fade_scale: f32,
    pub scramble_titles: bool,
}

/// Theme-owned graph canvas and node chrome. This keeps the
/// `extras::graph` adapter generic: it translates these tokens to
/// `mara_graph::GraphStyle` without knowing which theme family is
/// active.
#[derive(Copy, Clone, Debug)]
pub struct GraphTheme {
    pub node_pad_x: i8,
    pub node_pad_y: i8,
    pub bg_inner_margin: i8,
    pub canvas_pattern: GraphCanvasPattern,
    pub grid_alpha: u8,
    pub pin_stroke_width: f32,
    pub pin_stroke_alpha: u8,
    pub wire_width: f32,
    pub wire_glow: f32,
    pub pin_glow: f32,
    pub node_halo_gap: f32,
    pub node_halo_width: f32,
    pub node_halo_radius_outset: u8,
}

#[derive(Copy, Clone, Debug)]
pub enum GraphCanvasPattern {
    Dots { spacing: f32, radius: f32 },
    Hex { radius: f32 },
}

/// Theme-owned code editor adapter tokens.
#[derive(Copy, Clone, Debug)]
pub struct CodeTheme {
    pub font_size: f32,
    pub line_height_factor: f32,
    pub min_rows: usize,
    pub force_dark: bool,
    pub functions: egui::Color32,
    pub literals: egui::Color32,
    pub numerics: egui::Color32,
    pub strings: egui::Color32,
    pub types: egui::Color32,
}

/// Theme-owned fullscreen/maximize overlay chrome.
#[derive(Copy, Clone, Debug)]
pub struct OverlayTheme {
    pub inline_chip_size: f32,
    pub inline_chip_pad: f32,
    pub fullscreen_button_size: f32,
    pub fullscreen_edge_gap: f32,
    pub placeholder_text: &'static str,
    pub ghost_fill_alpha: u8,
    pub ghost_stroke_width: f32,
}

/// A complete visual profile for the mara UI kit. Built-in
/// profiles: [`theme_pro`] (the default — soft glass, rounded
/// corners, accent-tinted titles on a dark panel) and [`theme_game`]
/// (square corners, no borders, no padding, accent-coloured panel
/// with contrasting dark titles, full-accent click fills).
#[derive(Copy, Clone, Debug)]
pub struct Theme {
    /// Structured identity for selectors and diagnostics. Do not use
    /// this to special-case rendering; add a typed theme field for the
    /// visual decision instead.
    pub id: ThemeId,
    /// Identifier used by the de-dup cache in [`apply_theme`] — pick
    /// distinct names for distinct themes or the egui style won't
    /// re-apply on switch.
    pub name: &'static str,
    /// `true` for light-mode variants of the built-in themes —
    /// drives `apply_theme` to start from `egui::Visuals::light()`
    /// rather than `Visuals::dark()`, and lets widgets that need
    /// to know "am I in a light or dark context" branch cheaply
    /// without reading luma.
    pub is_light: bool,
    pub palette: PaletteTheme,
    pub stroke: StrokeTheme,
    pub glass: GlassTheme,
    pub shape: ShapeTheme,
    pub text: TextTheme,
    pub motion: MotionTheme,
    pub graph: GraphTheme,
    pub code: CodeTheme,
    pub overlay: OverlayTheme,
    /// Top-level L0 view chrome. New app-shell renderers should use
    /// this instead of hard-coding root view switcher placement.
    pub views: ViewTheme,
    /// Native-window chrome metrics for borderless host windows.
    pub window_chrome: WindowChromeTheme,
    /// Module inline/workspace chrome and restore semantics.
    pub modules: ModuleTheme,

    // ── Surfaces — palette ──
    pub bg_window: egui::Color32,
    pub bg_panel: egui::Color32,
    pub bg_raised: egui::Color32,
    pub bg_hover: egui::Color32,
    pub bg_input: egui::Color32,

    // ── Surfaces — fill mode ──
    /// How [`pane_fill`] resolves. PRO uses `FromBg` (dark panel);
    /// GAME uses `FromAccent` so the entire pane takes the user's
    /// accent colour.
    pub panel_fill_mode: ColorMode,
    /// How [`section_fill`] resolves. Only consulted when
    /// `section_show_frame` is true.
    pub section_fill_mode: ColorMode,
    /// `true` → sections paint a visible frame (fill + border + corner
    /// rounding) the way they always have. `false` → sections render
    /// no frame at all and the body content sits directly on the
    /// panel — the "no container" GAME look.
    pub section_show_frame: bool,
    /// PRO: 1 px hairline under the section header. GAME: false.
    pub section_show_title_divider: bool,
    /// Inner padding inside section / subsection frames.
    /// PRO: (4, 3); GAME: (0, 0).
    pub section_pad_x: i8,
    pub section_pad_y: i8,
    /// Horizontal indent applied to the section body content so it
    /// visually nests under the title rather than sitting flush at
    /// the same X. PRO ≈ 8 px, GAME ≈ 6 px — both themes get a
    /// distinct "body is inside" cue without the title needing to
    /// be cramped against the frame edge.
    pub section_body_indent: f32,
    /// Container Frame's `outer_margin` on the MAIN axis, on the
    /// side FACING the container's own title strip (and therefore
    /// also facing the parent pane's title strip for the FIRST
    /// container in the stack). Drives the gap between the pane
    /// title and the first container. PRO ≈ 3 px, GAME ≈ 9 px.
    pub section_outer_margin_flow_title: i8,
    /// Container Frame's `outer_margin` on the MAIN axis, on the
    /// side FACING the body (i.e. facing the next container in
    /// the stack). Two adjacent containers contribute
    /// `section_outer_margin_flow_body` (the upper container's
    /// title-facing side won't show here; what shows is THIS
    /// container's body-side + the NEXT container's title-side).
    /// Set this lower than `_title` to tighten inter-container
    /// gaps without shrinking the title-strip-to-first-container
    /// gap. PRO ≈ 3 px (uniform), GAME ≈ 0 px (so the inter-section
    /// gap is just the next container's title-side margin = 9 px,
    /// half the previous symmetric `9 + 9 = 18 px` gap).
    pub section_outer_margin_flow_body: i8,
    /// Container Frame's `outer_margin` on the CROSS axis (the axis
    /// the title strip spans). Drives breathing space between the
    /// container's painted edge and the pane's left/right (or
    /// top/bottom for vertical-strip panes) chrome. PRO ≈ 3 px;
    /// GAME ≈ 1 px so the containers feel like inset slabs almost
    /// flush with the pane edge.
    pub section_outer_margin_span: i8,
    /// Extra space inside the container body at its TITLE-facing
    /// edge — pushes the first body widget away from the title
    /// strip without changing the title strip's own thickness or
    /// the inter-section gap. PRO ≈ 0 (the existing
    /// `TITLE_BODY_GAP_HALF` already breathes well); GAME ≈ 8 px
    /// so body content sits clear of the heavy banner instead of
    /// crowding it.
    pub section_body_inner_top_pad: f32,
    /// `true` → fire a periodic chromatic-aberration split on the
    /// pane's title text (red ghost shifted along the reading
    /// direction one way, cyan ghost the other, ramping
    /// `0 → peak → 0` over a few hundred ms every 5–13 s). PRO
    /// keeps titles still; GAME enables the CRT-style colour split.
    pub pane_title_chromatic_aberration: bool,
    /// Section open / close animation duration in seconds. Drives
    /// egui's `style.animation_time`, so it also governs every
    /// `animate_bool` consumer in the kit (chevron rotation,
    /// banner height, hover-state lerps). PRO favours snap-quick
    /// fold / unfold; GAME favours a deliberate softer ease so the
    /// banner expansion reads as "scene transition".
    pub section_animation_time: f32,
    /// **NO ANIMATION** kill-switch. When `false`, every motion
    /// helper in the kit short-circuits to its end state — press
    /// depress, click pulse, bargraph catch-up, numeric tumble,
    /// section fade-in stagger, animated-button hover fills, all
    /// off. Defaults to `true` on both PRO and GAME presets; flip
    /// to `false` (e.g. `Theme { animations_enabled: false,
    /// ..theme_pro(Mode::Dark) }`) for an instant-feedback variant.
    /// Scramble-decode + corner-bracket pulse + telemetry pip +
    /// caution stripes have their own dedicated theme flags; this
    /// knob does NOT control them.
    pub animations_enabled: bool,
    /// Multiplier applied to button-feedback animation durations
    /// (press depress, click pulse, animated-button hover fill).
    /// `1.0` = base timings (60 ms press in, 90 ms out, 140 ms click
    /// pulse, 250 ms hover); `2.0` = twice as slow; `0.5` = twice
    /// as fast. PRO ships `1.0` for snappy clicks; GAME ships
    /// `2.0` for deliberate, lingering button feedback that fits
    /// the slower cinematic motion language.
    pub button_anim_scale: f32,
    /// Multiplier on the per-section fade-in stagger when a pane
    /// opens. Base values are `STAGGER = 0.18 s` between adjacent
    /// sections and `FADE = 0.45 s` per-section opacity ramp; both
    /// are scaled by this. PRO ships `0.5` (half — sections appear
    /// quickly so opening a pane doesn't feel slow); GAME ships
    /// `1.0` for the deliberate cinematic reveal.
    pub pane_fade_scale: f32,

    // ── Text ──
    pub text_primary: egui::Color32,
    pub text_secondary: egui::Color32,
    pub text_disabled: egui::Color32,
    /// How the section / pane title colour is resolved.
    pub title_color_mode: TextColorMode,
    /// Lerp fraction toward the title's surface applied AFTER
    /// [`title_color_mode`] resolves. PRO: `0.0` (titles render at
    /// full contrast / pure accent). GAME: a positive value softens
    /// titles so body row text reads as the *darker / heavier* tier
    /// — flipping the conventional "title darker than body" hierarchy
    /// so on a bright accent panel the row content punches and the
    /// header recedes. Body labels (`body_label` → `on_section_dim`)
    /// sit at a 40 % surface lerp, so a title softness of `0.55`
    /// orders three tiers: title (softest) → label (mid) → row text
    /// (full punch).
    pub title_softness: f32,
    /// `true` → ribbon buttons paint the three-tier accent ladder
    /// (idle = accent dimmed 30 % toward black, hover = pure accent,
    /// active = accent brightened 28 % toward white + outer accent
    /// halo). `false` → the original PRO recipe (panel fill idle,
    /// raised fill hover, 25 %-accent-blend + accent stroke active).
    /// PRO `false`, GAME `true`.
    pub ribbon_button_accent_fill: bool,
    /// Vertical gap painted between consecutive sections inside a
    /// pane. The gap is *transparent* — the scene below shows
    /// through, so each section reads as its own bracketed module
    /// instead of all sections being one undifferentiated stack.
    /// PRO `0.0` (back-to-back sections, original look). GAME
    /// `6.0` — pairs with `section_corner_ticks_inset` so the
    /// corner brackets sit just inside the gap.
    pub section_gap: f32,
    /// Pixels to inset L-bracket corner ticks from the section's
    /// outer rect. `0.0` paints them flush at the corners; a
    /// positive value pulls them in so they sit *inside* the
    /// section's inner edge, which reads as a "frame inside a
    /// frame" once `section_gap > 0` separates the sections. PRO
    /// `0.0`, GAME `2.0`.
    pub section_corner_ticks_inset: f32,
    /// `true` → wrap the section title text in literal `[ … ]`
    /// brackets — the iconic terminal / Helldivers / Pip-Boy cue.
    /// PRO false, GAME true.
    pub section_title_brackets: bool,
    /// Optional glyph prefix prepended to section titles (with a
    /// trailing space). `None` skips. PRO `None`, GAME `Some("▸")`
    /// — the "tactical menu" caret marker found in Cyberpunk 2077,
    /// Helldivers, Mass Effect Andromeda. Painted in the title
    /// colour, before any bracketing.
    pub section_title_prefix: Option<&'static str>,
    /// Extra letter-spacing (px) applied to section titles via
    /// `section_caps`. PRO `0.0`, GAME `1.5` — wide tracking is the
    /// near-universal "this is a system / game UI heading" cue.
    pub section_title_letter_spacing: f32,
    /// Optional glyph prefix prepended to `sub_caption` text. PRO
    /// `None`, GAME `Some("// ")` — `fsociety` / Helldivers / VS
    /// console comment marker; reads as code-style annotation.
    pub subcaption_prefix: Option<&'static str>,
    /// `true` → paint a dashed accent rule along the section's
    /// bottom edge after the body finishes rendering. Gives every
    /// section a "closes here" boundary even when there are no
    /// borders. PRO false, GAME true.
    pub section_bottom_rule: bool,
    /// `true` → the floating pane paints its main fill across the
    /// whole window (PRO behaviour). `false` → the pane's frame is
    /// **transparent**, the scene below shows through, and each
    /// section is responsible for painting its own opaque
    /// background. Combined with `section_gap`, this turns the pane
    /// into a stack of *floating cards* with see-through gaps
    /// between them — the modular HUD look the GAME theme is built
    /// around. The pane title strip still paints opaque (manually)
    /// even when this is false. PRO `true`, GAME `false`.
    pub pane_fill_visible: bool,
    /// `true` → paint the rotating chevron glyph at the start of
    /// every section header. `false` → no chevron at all (the title
    /// row still toggles on click; the visual cue is dropped). PRO
    /// `true`, GAME `false` — pure bracketed-title look without a
    /// fold indicator.
    pub show_section_chevron: bool,
    /// `true` → the section header strip paints a solid accent
    /// banner behind the title (bracketed title text in dark contrast
    /// colour on top). `false` → header strip is transparent over
    /// the section card. When this is enabled, the section's TOP
    /// corner ticks flip to the contrast colour so they don't
    /// disappear into the accent banner; bottom corners stay accent.
    /// PRO `false`, GAME `true`.
    pub title_strip_filled: bool,
    /// Section title font size in points. PRO `13.8` (12 × 1.15 — the
    /// kit's original size). GAME `11.5` — the title sits on a dense
    /// banner with brackets, so a smaller weight reads cleaner and
    /// keeps the strip from dominating the body.
    pub section_title_size: f32,
    /// Lerp fraction toward black applied to the accent before it's
    /// used by *body* widgets (sliders, progress bars, button hover
    /// / press tints, toggles). Title banner / corner ticks keep the
    /// pure accent so they stay the brightest "spotlight" surface in
    /// each card. PRO `0.0` (body widgets share the title accent —
    /// the original look). GAME `0.18` (body fills are darker, so
    /// they read as inside the title banner's brightness rather than
    /// matching it).
    pub body_accent_darken: f32,
    /// `true` → render the section's optional icon as a *large*
    /// floating glyph anchored at the RIGHT edge of the title strip
    /// (instead of inline next to the title text). Title galley
    /// drops the icon section in this mode. PRO `false` (icon stays
    /// next to title), GAME `true`.
    pub section_icon_at_end: bool,
    /// Pixel size of the section's right-edge floating icon when
    /// `section_icon_at_end` is set. PRO unused (`0.0`), GAME `24.0`
    /// — noticeably bigger than the title text so it reads as a
    /// floating accent ornament, not body text.
    pub section_icon_size: f32,
    /// Extra vertical space inserted between the section's title
    /// strip and the FIRST body row. PRO `0.0` — title and body sit
    /// flush. GAME `16.0` — clears the unfolded floating icon's
    /// downward overflow so the icon doesn't crash into the first
    /// row of the body.
    pub section_body_top_pad: f32,
    /// `Some((on, off))` → row separators paint dashed instead of
    /// solid, with `on` pixels of line and `off` pixels of gap. `None`
    /// keeps the original solid hairline. PRO `None`, GAME
    /// `Some((4.0, 3.0))` — the universal "machine-drawn, not
    /// designed" cue every cyberpunk / tactical / HUD UI uses.
    pub row_separator_dash: Option<(f32, f32)>,
    /// `true` → after the section title text, a dashed horizontal
    /// rule fills the remaining header width up to the actions tail
    /// (DOOM Eternal / Helldivers / EVE Online pattern). The line
    /// uses `row_separator_dash` if set, otherwise solid. PRO false,
    /// GAME true.
    pub section_title_trailing_rule: bool,
    /// Length of the L-bracket arms painted at the four corners of
    /// the section / pane's outer rect — the iconic HUD anchor seen
    /// in Destiny 2, Apex, Rainbow Six, Tron Legacy. `0.0` skips them
    /// entirely (PRO); a positive value paints two perpendicular
    /// strokes of that length flush at each corner. GAME `7.0`.
    pub section_corner_ticks: f32,

    // ── Tabs ──
    /// Tabbed container visual/layout dialect. This replaces
    /// renderer-side checks like `theme.name.starts_with("GAME")`.
    pub tabs: TabTheme,
    pub pod: PodTheme,
    pub widgets: WidgetTheme,
    pub icons: IconTheme,
    /// Pane geometry/chrome group. Flat fields remain during the
    /// migration; new code should prefer this nested contract.
    pub pane: PaneTheme,
    /// Ribbon geometry/chrome group. Flat fields remain during the
    /// migration; new code should prefer this nested contract.
    pub ribbon: RibbonTheme,
    /// Shelf geometry/chrome group. Shelves are structural chrome
    /// that reserve viewport space.
    pub shelf: ShelfTheme,
    /// Container/section chrome group. Flat fields remain during the
    /// migration; new code should prefer this nested contract.
    pub container: ContainerTheme,

    // ── Borders / strokes ──
    /// Base border colour (before the accent tint blend).
    pub border_subtle: egui::Color32,
    /// Inner-frame stroke colour for hover / active states.
    pub border_inner: egui::Color32,
    /// Alpha applied to [`widget_border`] strokes.
    pub border_alpha: u8,
    /// Fraction of the accent colour blended into [`widget_border`].
    pub border_accent_tint: f32,
    /// Stroke width used for every mara surface (sections,
    /// subsections, group frames, inputs, …). `0.0` paints no border
    /// at all — handy for the GAME profile.
    pub border_width: f32,
    /// Alpha applied to the hairline row-separator painted between
    /// row widgets in a section body. Decoupled from
    /// [`border_alpha`] so a theme can hide panel borders while
    /// keeping faint row dividers (the GAME profile does exactly
    /// that — `border_width = 0` so panes / sections paint no
    /// outline, while `row_separator_alpha = 32` keeps a faint
    /// hairline between widgets). PRO `96`, GAME `32`.
    pub row_separator_alpha: u8,

    // ── Glass ──
    /// Card alpha as a fraction of window alpha. PRO ≈ 0.76; GAME
    /// can flatten this to 1.0 + a flat fill to drop the glass effect.
    pub glass_card_factor: f32,
    /// Group alpha as a fraction of window alpha.
    pub glass_group_factor: f32,
    /// Fraction of the accent colour blended into glass surfaces.
    /// `0.0` produces a pure neutral fill — matches the flat,
    /// posterised look of game UIs.
    pub glass_accent_tint: f32,

    // ── Shape ──
    pub radius_widget: u8,
    pub radius_compact: u8,
    pub radius_sm: u8,
    pub radius_md: u8,
    pub radius_lg: u8,

    // ── Body row visuals ──
    /// PRO: false. GAME: true → row-level widgets paint an alternating
    /// fill behind every other row so a borderless widget stack still
    /// reads as a list. Implemented via a ctx-data row counter +
    /// deferred shape resolved from `widgets/shared.rs`'s
    /// `flush_pending_separator` — each `flush` call closes off the
    /// previous row, paints its zebra fill if the row index is odd,
    /// and arms the next row's placeholder.
    pub row_alternation: bool,
    /// Lightness lerp toward white applied to the panel base when
    /// painting an alternating row's fill. The five cross-app theming
    /// references (Houdini, Substance, Godot, AE, Logic) cluster
    /// around 4–6%; below 3% reads as noise, above 8% screams
    /// "stripes". PRO uses `0.0` (alternation off); GAME uses `0.05`.
    pub row_alt_lift: f32,

    // ── Click visuals ──
    /// `false` → press-state uses the subtle accent lerp the PRO
    /// theme has always done. `true` → pressed buttons fill solid
    /// with `accent`, no halftone, for the chunky GAME look.
    pub button_full_accent_on_press: bool,
    /// Accent-blend fraction for buttons at rest. PRO `0.08`, GAME
    /// `0.0` (flat panel under the button).
    pub button_tint_rest: f32,
    /// Accent-blend fraction for buttons on hover. PRO `0.16`, GAME
    /// `0.18` (a touch more pop on the bright accent panel).
    pub button_tint_hover: f32,
    /// Accent-blend fraction for buttons while pressed (when
    /// `button_full_accent_on_press` is `false`). PRO `0.30`.
    pub button_tint_press: f32,

    // ── Pane chrome ──
    /// Shadow blur radius for the floating pane window. PRO `24`,
    /// GAME `0` (hard-edge no-shadow look).
    pub pane_shadow_blur: u8,
    /// Shadow vertical offset. PRO `8`, GAME `0`.
    pub pane_shadow_y: i8,
    /// Whether the pane title strip paints a 1 px hairline divider
    /// under the title. PRO true, GAME false.
    pub pane_show_title_divider: bool,
    /// Whether the pane title strip paints a diagonal "caution-tape"
    /// stripe pattern (alternating accent + panel-neutral) behind the
    /// title text. PRO `false` (clean strip). GAME `true` (the bright
    /// accent + dark-neutral diagonals frame the title with the same
    /// "do-not-cross" cue police tape uses).
    pub pane_title_stripes: bool,
    /// Whether pane / section titles "decode" through the
    /// [`scramble_text`] symbol cycle on every appearance. PRO
    /// `false` (clean text). GAME `true` (tactical decode feel).
    pub scramble_titles: bool,

    // ── Tree / list visuals ──
    /// Width of the indent-guide line painted at each depth level
    /// of a tree. PRO `1.0`, GAME `0.0` (guides off — flat list).
    pub tree_guide_width: f32,
    /// Graph graph pin stroke width. PRO `1.0`, GAME `0.0`.
    pub graph_pin_width: f32,
    /// Bloom intensity for graph wires (`0.0` = no glow, `1.0+`
    /// = strong neon halo). Read by `mara_node_graph_style` and
    /// passed straight through to `GraphStyle::wire_glow`. PRO
    /// 0.6 (vibrant but tasteful), GAME 1.0 (full neon).
    pub graph_wire_glow: f32,
    /// Bloom intensity for graph pin glyphs — same scale and
    /// semantics as `graph_wire_glow`, applied to the pin
    /// shape's halo passes. PRO 0.5, GAME 0.85.
    pub graph_pin_glow: f32,
    /// Use a pointy-top hex tessellation as the graph canvas
    /// background instead of the Blender-style dot grid. PRO
    /// false (dots), GAME true (hex — sci-fi HUD motif).
    pub graph_canvas_hex: bool,
    /// Render progress bars as discrete cells with 1-px gaps
    /// (Mass Effect / Apex shield style) instead of a smooth
    /// fill. PRO false (smooth), GAME true (segmented).
    pub progressbar_segmented: bool,
    /// Wrap pane titles in literal `[ … ]` brackets — the
    /// terminal / Helldivers / Pip-Boy HUD cue. Section titles
    /// already have their own [`Self::section_title_brackets`]
    /// flag with collapse-aware layout. PRO false, GAME true.
    pub pane_title_brackets: bool,
    /// Cross-axis thickness (px) of the inter-pod separator
    /// strip. The rule itself paints at the strip's centre, so
    /// extra strip height converts directly into vertical pad
    /// above and below the rule. PRO 2 (hairline strip, no pad);
    /// GAME 14 — ≈6 px of pad on each side of the rule so pods
    /// breathe.
    pub section_separator_strip_h: f32,
    /// Alpha applied to the inter-pod separator rule. PRO 128
    /// (the kit's original half-strength rule — quiet but
    /// readable on a dark panel); GAME 64 — halved again so the
    /// rule barely whispers against the bright accent panel.
    pub section_separator_alpha: u8,
    /// Extra space (px) inserted INSIDE the container body at
    /// the body-end edge — the gap between the LAST pod's bottom
    /// edge and the section frame's body-facing inner edge.
    /// Mirror of [`Self::section_body_inner_top_pad`] on the
    /// opposite edge. Allocated AFTER the user body callback as
    /// a `ui.add_space()` so it grows the body's intrinsic
    /// content size and the container auto-fits to include it.
    /// PRO 0 (no extra pad — the symmetric `section_padding`
    /// already handles it), GAME ≈ 12 so the bracketed banner
    /// + last-pod combo doesn't slam into the section frame.
    pub section_body_inner_end_pad: f32,

    // ── Drag-reorder ghost ──
    /// Alpha applied to the accent fill of the section/ribbon-button
    /// drag-ghost rect. PRO `28` (faint), GAME `90` (visible against
    /// the accent panel).
    pub ghost_fill_alpha: u8,
    /// Stroke width on the drag-ghost rect's accent border. PRO
    /// `1.5`, GAME `0.0` (no stroke — fill alone reads).
    pub ghost_stroke_width: f32,

    // ── Accent adaptation ──
    /// `true` → run [`adapt_accent_to_mode`] on the user's raw accent
    /// before it propagates through every surface paint. The transform
    /// pulls accents into the readable lightness band for the active
    /// mode (whiter accents get *less* luminance, darker ones get
    /// *more*) — a "pastel" pull that keeps text readable on accent
    /// fills regardless of the user's pick. `false` → use the raw
    /// accent verbatim, no luminance / saturation tweaks. Default
    /// `true` for both built-in profiles; flip to `false` (e.g.
    /// `Theme { pastel_accent: false, ..theme_pro(Mode::Dark) }`) to
    /// honour saturated neon accents pixel-for-pixel.
    pub pastel_accent: bool,
}

impl Theme {
    pub fn palette(&self) -> &PaletteTheme {
        &self.palette
    }

    pub fn stroke(&self) -> &StrokeTheme {
        &self.stroke
    }

    pub fn glass(&self) -> &GlassTheme {
        &self.glass
    }

    pub fn shape(&self) -> &ShapeTheme {
        &self.shape
    }

    pub fn text(&self) -> &TextTheme {
        &self.text
    }

    pub fn motion(&self) -> &MotionTheme {
        &self.motion
    }

    pub fn graph(&self) -> &GraphTheme {
        &self.graph
    }

    pub fn code(&self) -> &CodeTheme {
        &self.code
    }

    pub fn overlay(&self) -> &OverlayTheme {
        &self.overlay
    }

    pub fn tabs(&self) -> &TabTheme {
        &self.tabs
    }

    pub fn pod(&self) -> &PodTheme {
        &self.pod
    }

    pub fn widgets(&self) -> &WidgetTheme {
        &self.widgets
    }

    pub fn icons(&self) -> &IconTheme {
        &self.icons
    }

    pub fn pane(&self) -> &PaneTheme {
        &self.pane
    }

    pub fn ribbon(&self) -> &RibbonTheme {
        &self.ribbon
    }

    pub fn shelf(&self) -> &ShelfTheme {
        &self.shelf
    }

    pub fn container(&self) -> &ContainerTheme {
        &self.container
    }
}

// ── Built-in themes ──
//
// `Theme` (the struct), the global `set_theme` / `theme()` state, and
// `apply_theme` (the de-dup cache + egui style transfer) live in this
// file — they're the *engine*. Theme PRESETS (PRO, GAME, …) live in
// `crate::themes`, one file per theme. To add a new theme, drop a file
// under `crates/core/src/themes/` and register it in
// `themes/mod.rs`; see that module's docs for the full template.
pub use crate::themes::{
    FLAT_DARK_BG_HOVER, FLAT_DARK_BG_INPUT, FLAT_DARK_BG_PANEL, FLAT_DARK_BG_RAISED,
    FLAT_DARK_BG_WINDOW, FLAT_LIGHT_BG_HOVER, FLAT_LIGHT_BG_INPUT, FLAT_LIGHT_BG_PANEL,
    FLAT_LIGHT_BG_RAISED, FLAT_LIGHT_BG_WINDOW, GAME_LIGHT_BG_HOVER, GAME_LIGHT_BG_INPUT,
    GAME_LIGHT_BG_PANEL, GAME_LIGHT_BG_RAISED, GAME_LIGHT_BG_WINDOW, PRO_LIGHT_BG_HOVER,
    PRO_LIGHT_BG_INPUT, PRO_LIGHT_BG_PANEL, PRO_LIGHT_BG_RAISED, PRO_LIGHT_BG_WINDOW,
    PRO_LIGHT_BORDER_INNER, PRO_LIGHT_BORDER_SUBTLE, theme_flat, theme_game, theme_pro,
};

/// Packed `(r, g, b, a)` snapshot of the active accent colour.
/// `apply_theme` writes this so widget paints can call
/// [`active_accent`] without threading the colour through every API.
static ACTIVE_ACCENT: core::sync::atomic::AtomicU32 =
    core::sync::atomic::AtomicU32::new(0xE6E6E8FF);

/// The user's raw, untransformed accent — what was passed to
/// `apply_theme` before any pastel pull. Read by text helpers
/// (e.g. `section_title_color`) so titles always paint the colour
/// the user actually picked, regardless of `Theme::pastel_accent`.
static RAW_ACCENT: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0xE6E6E8FF);

fn set_active_accent(c: egui::Color32) {
    let p =
        ((c.r() as u32) << 24) | ((c.g() as u32) << 16) | ((c.b() as u32) << 8) | (c.a() as u32);
    ACTIVE_ACCENT.store(p, Ordering::Relaxed);
}

fn set_raw_accent(c: egui::Color32) {
    let p =
        ((c.r() as u32) << 24) | ((c.g() as u32) << 16) | ((c.b() as u32) << 8) | (c.a() as u32);
    RAW_ACCENT.store(p, Ordering::Relaxed);
}

/// Read the current accent colour. Hosts call [`apply_theme`] each
/// frame, which keeps this in sync. Widget code that already has
/// `accent` in scope should keep using it; this exists for helpers
/// (text-contrast pickers, theme-aware fills) called from sites
/// that don't thread accent through their signatures.
pub fn active_accent() -> egui::Color32 {
    let p = ACTIVE_ACCENT.load(Ordering::Relaxed);
    egui::Color32::from_rgba_premultiplied(
        ((p >> 24) & 0xff) as u8,
        ((p >> 16) & 0xff) as u8,
        ((p >> 8) & 0xff) as u8,
        (p & 0xff) as u8,
    )
}

/// Read the user's raw, untransformed accent. Use this in text-paint
/// paths (titles, glyphs that should match the user's pick) instead
/// of [`active_accent`] when you don't want the pastel pull.
pub fn raw_accent() -> egui::Color32 {
    let p = RAW_ACCENT.load(Ordering::Relaxed);
    egui::Color32::from_rgba_premultiplied(
        ((p >> 24) & 0xff) as u8,
        ((p >> 16) & 0xff) as u8,
        ((p >> 8) & 0xff) as u8,
        (p & 0xff) as u8,
    )
}

/// Lazily-initialised storage for the active theme. Single-process
/// singleton, read on every widget paint.
static ACTIVE_THEME: std::sync::OnceLock<std::sync::RwLock<Theme>> = std::sync::OnceLock::new();

fn theme_lock() -> &'static std::sync::RwLock<Theme> {
    ACTIVE_THEME.get_or_init(|| std::sync::RwLock::new(theme_pro(Mode::Dark)))
}

fn read_unpoisoned<T>(lock: &std::sync::RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_unpoisoned<T>(lock: &std::sync::RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Replace the active theme. Takes effect on the next paint —
/// The de-dup cache in [`apply_theme`] uses `theme.name` to
/// detect the switch and re-push the egui style. Call this when the
/// user picks a profile from a settings UI.
pub fn set_theme(t: Theme) {
    *write_unpoisoned(theme_lock()) = t;
}

/// Return a copy of the active theme. `Theme` is `Copy`, so widgets
/// can call this freely — no lifetimes, no allocation. Reads are
/// `RwLock::read`; under typical UI contention (none) the cost is a
/// single relaxed atomic.
pub fn theme() -> Theme {
    *read_unpoisoned(theme_lock())
}

/// Paint a "do-not-cross" diagonal stripe pattern over `rect` —
/// alternating slabs of `accent` and the active theme's neutral
/// `bg_panel` colour. Used by the GAME pane title strip
/// ([`Theme::pane_title_stripes`]); enabled themes opt in by
/// setting that flag.
///
/// The painter's clip-rect is set to `rect` so the slanted
/// parallelograms can extend past the rect edges without overflowing
/// the strip — the GPU clips them to the strip's exact bounds.
pub fn paint_caution_stripes(painter: &egui::Painter, rect: egui::Rect, accent: egui::Color32) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    // Stripes paint as accent @ α 51 (≈ 80 % transparent). No base
    // fill is laid down — the underlying pane / panel surface shows
    // through both the painted slabs (lightly veiled in accent) and
    // the gaps (untouched). The alternation reads as
    // "accent-tinted band / pane band", same colour family on both
    // sides, no harsh second colour.
    let solid = accent;
    let translucent = egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 51);

    // Width of a single diagonal slab. Pattern period = STRIPE_W*2
    // (one solid + one translucent).
    const STRIPE_W: f32 = 12.0;
    // Logical points / second the pattern slides along the strip.
    // Slow enough to read as a deliberate "live banner" rather than
    // a frantic scroll.
    const SCROLL_SPEED: f32 = 18.0;

    let ctx = painter.ctx();
    let t = ctx.input(|i| i.time) as f32;
    let period = STRIPE_W * 2.0;
    // Modulo with manual rem_euclid so a hot reload that drops time
    // backwards doesn't produce a negative offset.
    let raw = (t * SCROLL_SPEED) % period;
    let x_offset = if raw < 0.0 { raw + period } else { raw };
    // Stripes are visible — keep animating. Without this egui only
    // repaints on input events and the strip would appear frozen.
    ctx.request_repaint();

    let clipped = painter.clone().with_clip_rect(rect);

    let h = rect.height();
    // Walk `x` from `-h - period` (one extra period to the left so
    // the wrapped offset still covers the left edge cleanly) up to
    // the rect's width.
    let mut x = -h - period - x_offset;
    let mut idx = 0;
    while x < rect.width() {
        let x0 = rect.min.x + x;
        let x1 = x0 + STRIPE_W;
        let p0 = egui::pos2(x0, rect.min.y);
        let p1 = egui::pos2(x1, rect.min.y);
        let p2 = egui::pos2(x1 + h, rect.max.y);
        let p3 = egui::pos2(x0 + h, rect.max.y);
        let fill = if idx % 2 == 0 { solid } else { translucent };
        clipped.add(egui::Shape::convex_polygon(
            vec![p0, p1, p2, p3],
            fill,
            egui::Stroke::NONE,
        ));
        x += STRIPE_W;
        idx += 1;
    }
}

/// Resolve the active theme's [`ColorMode`] for a fill against the
/// runtime accent colour. Used by [`pane_fill`] / [`section_fill`].
fn resolve_color(mode: ColorMode, fallback: egui::Color32, accent: egui::Color32) -> egui::Color32 {
    match mode {
        ColorMode::FromBg => fallback,
        ColorMode::FromAccent {
            lerp_factor,
            lerp_target,
        } => {
            let f = lerp_factor.clamp(0.0, 1.0);
            let lerp = |a: u8, b: u8| ((a as f32) * (1.0 - f) + (b as f32) * f).round() as u8;
            egui::Color32::from_rgb(
                lerp(lerp_target.r(), accent.r()),
                lerp(lerp_target.g(), accent.g()),
                lerp(lerp_target.b(), accent.b()),
            )
        }
    }
}

/// The opaque base fill colour for the floating pane window — what
/// `egui::Frame::fill` ultimately gets, modulo the glass alpha.
/// PRO returns `theme().bg_panel`; GAME returns an accent-derived
/// dark colour so the entire pane reads as "the user's accent".
pub fn pane_fill(accent: egui::Color32) -> egui::Color32 {
    let th = theme();
    resolve_color(th.panel_fill_mode, th.bg_panel, accent)
}

/// The opaque base fill colour for a section card. Only consulted
/// when `theme().section_show_frame` is `true`. PRO returns
/// `theme().bg_raised`; GAME falls through to its `bg_raised` when
/// frame paint is enabled at all.
pub fn section_fill(accent: egui::Color32) -> egui::Color32 {
    let th = theme();
    resolve_color(th.container.fill_mode, th.bg_raised, accent)
}

/// Resolve the active theme's title colour against the runtime
/// accent. PRO maps to `accent` (the title literally tints with the
/// user's chosen accent); GAME maps via [`contrast_text_for`] over
/// the resolved panel fill, so a bright accent panel shows
/// near-black titles and a dark panel shows near-white.
pub fn section_title_color(accent: egui::Color32) -> egui::Color32 {
    let th = theme();
    // Title text never pastelizes — it always paints the user's
    // raw pick. The pastel toggle is for chrome (fills, borders,
    // ribbon paint), not for what the user reads.
    let title_accent = raw_accent();
    let (resolved, surface) = match th.text.title_color_mode {
        // No luma guard, no contrast check: title literally tints
        // with the user's raw accent. Trust the user; if they pick
        // a low-contrast accent they accept the visual.
        TextColorMode::Accent => (title_accent, pane_fill(accent)),
        TextColorMode::Primary => (th.palette.text_primary, pane_fill(accent)),
        TextColorMode::Secondary => (th.palette.text_secondary, pane_fill(accent)),
        TextColorMode::ContrastWithPanel => {
            let surface = pane_fill(accent);
            (contrast_text_for(surface), surface)
        }
        TextColorMode::ContrastWithSection => {
            let surface = section_fill(accent);
            (contrast_text_for(surface), surface)
        }
    };
    if th.text.title_softness > 0.0 {
        lerp_rgb(resolved, surface, th.text.title_softness.clamp(0.0, 1.0))
    } else {
        resolved
    }
}

/// `egui::Margin` used by section / subsection / group inner frames,
/// driven by the theme's `section_pad_x/y`. GAME → `Margin::ZERO`.
pub fn section_padding() -> egui::Margin {
    let th = theme();
    egui::Margin::symmetric(th.container.pad_x, th.container.pad_y)
}

/// Whether the section header should paint a 1 px hairline divider
/// between its title and body. Mirrors `theme().section_show_title_divider`.
pub fn section_show_title_divider() -> bool {
    theme().container.show_title_divider
}

/// Whether sections should paint their own frame (fill + border +
/// corner rounding). When `false`, the section's `egui::Frame` paint
/// is skipped entirely and the body content renders directly on the
/// pane background — the GAME "no card" look.
pub fn section_show_frame() -> bool {
    theme().container.show_frame
}

/// Semantic fill roles for common mara surfaces. Use these in
/// modules before constructing a custom `Color32` recipe directly.
#[derive(Copy, Clone, Debug)]
pub enum FillRole {
    Pane,
    Section,
    Subsection,
    Track,
    Popup,
    DragGhost,
}

/// Semantic stroke roles for common mara outlines and separators.
#[derive(Copy, Clone, Debug)]
pub enum StrokeRole {
    WidgetBorder,
    SectionBorder,
    PopupBorder,
    Separator,
    DragGhost,
}

/// Semantic corner-radius roles.
#[derive(Copy, Clone, Debug)]
pub enum RadiusRole {
    Widget,
    Compact,
    Section,
    Pane,
    Popup,
}

/// Semantic frame roles used by helpers that can be expressed as an
/// `egui::Frame`.
#[derive(Copy, Clone, Debug)]
pub enum FrameRole {
    Section,
    Popup,
    KeyChip,
}

pub fn fill_for(role: FillRole, accent: egui::Color32) -> egui::Color32 {
    match role {
        FillRole::Pane => glass_fill(pane_fill(accent), accent, glass_alpha_window()),
        FillRole::Section => glass_fill(section_fill(accent), accent, glass_alpha_card()),
        FillRole::Subsection => glass_fill(subsection_fill(accent), accent, glass_alpha_group()),
        FillRole::Track => track_fill(accent),
        FillRole::Popup => glass_fill(popup_fill(accent), accent, glass_alpha_window()),
        FillRole::DragGhost => {
            let th = theme();
            egui::Color32::from_rgba_unmultiplied(
                accent.r(),
                accent.g(),
                accent.b(),
                th.ribbon.ghost_fill_alpha,
            )
        }
    }
}

pub fn stroke_for(role: StrokeRole, accent: egui::Color32) -> egui::Stroke {
    let th = theme();
    match role {
        StrokeRole::WidgetBorder | StrokeRole::SectionBorder | StrokeRole::PopupBorder => {
            egui::Stroke::new(th.stroke.border_width, widget_border(accent))
        }
        StrokeRole::Separator => {
            let base = th.palette.border_subtle;
            egui::Stroke::new(
                1.0,
                egui::Color32::from_rgba_unmultiplied(
                    base.r(),
                    base.g(),
                    base.b(),
                    th.container.separator_alpha,
                ),
            )
        }
        StrokeRole::DragGhost => egui::Stroke::new(th.ribbon.ghost_stroke_width, accent),
    }
}

pub fn radius_for(role: RadiusRole) -> egui::CornerRadius {
    let th = theme();
    match role {
        RadiusRole::Widget => egui::CornerRadius::same(th.shape.radius_widget),
        RadiusRole::Compact => egui::CornerRadius::same(th.shape.radius_compact),
        RadiusRole::Section | RadiusRole::Popup => egui::CornerRadius::same(th.shape.radius_md),
        RadiusRole::Pane => egui::CornerRadius::same(th.shape.radius_lg),
    }
}

pub fn frame_for(role: FrameRole, accent: egui::Color32) -> egui::Frame {
    match role {
        FrameRole::Section => egui::Frame::new()
            .fill(fill_for(FillRole::Section, accent))
            .stroke(stroke_for(StrokeRole::SectionBorder, accent))
            .corner_radius(radius_for(RadiusRole::Section))
            .inner_margin(section_padding()),
        FrameRole::Popup => egui::Frame::new()
            .fill(fill_for(FillRole::Popup, accent))
            .stroke(stroke_for(StrokeRole::PopupBorder, accent))
            .corner_radius(radius_for(RadiusRole::Popup))
            .inner_margin(egui::Margin::symmetric(4, 4))
            .shadow(egui::epaint::Shadow {
                offset: [0, 4],
                blur: 16,
                spread: 0,
                color: egui::Color32::from_black_alpha(120),
            }),
        FrameRole::KeyChip => egui::Frame::new()
            .fill(fill_for(FillRole::Track, accent))
            .inner_margin(egui::Margin::symmetric(5, 1))
            .corner_radius(radius_for(RadiusRole::Widget)),
    }
}

/// Pull an accent into the readable lightness band using a smooth
/// curve in **Lab L\*** space — perceptually uniform, so yellow
/// (Lab L\* ≈ 97) registers as bright the way the eye sees it,
/// not as 0.50 the way HSL claims.
///
/// Algorithm (per mode):
/// 1. Pick a "honoured" zone — accents whose L\* sits inside it
///    pass through unchanged. Mid-luma greens / oranges / mid-blues
///    that already contrast naturally never get touched.
/// 2. Outside the honoured zone, pull L\* toward a `target` with
///    **smoothstep-weighted strength** based on how far past the
///    zone the accent is. This is the "bezier-like" curve the user
///    asked for: identity in the middle, gentle at the edges,
///    strong at the extremes (white / black).
/// 3. Reduce chroma by 8 % so very neon accents lose their
///    fluorescent buzz without going grey.
///
/// Examples (Dark mode, honoured zone L\* ≤ 60, target 40):
///
///   white   L\* 100 → strong pull → ~ L\* 40 (mid grey)
///   yellow  L\* 97  → strong pull → ~ L\* 42 (mustard)
///   light   green L\* 90 → moderate pull → ~ L\* 48
///   mid-green L\* 65 → tiny pull → ~ L\* 62 (barely touched)
///   dark blue L\* 17 → unchanged (already dark enough)
///   black   L\* 0   → unchanged
///
/// Light mode mirrors: honoured zone L\* ≥ 40, target 60. Black
/// gets fully lifted to mid grey; mid-green stays put.
pub fn adapt_accent_to_mode(accent: egui::Color32, is_light: bool) -> egui::Color32 {
    use pastel::Color as PastelColor;
    let c = PastelColor::from_rgb(accent.r(), accent.g(), accent.b());
    // HSL space — preserves hue exactly. Yellow stays yellow when
    // darkened (no olive shift you get in Lab); pure red stays red.
    // The trade-off (HSL's L doesn't match perceived luminance for
    // saturated colours) doesn't matter here because we only adjust
    // the EXTREMES of HSL L (whites + blacks) and leave everything
    // mid-range untouched.
    let hsl = c.to_hsla();
    let l = hsl.l;
    let _smoothstep = |t: f64| -> f64 {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    // Hard band, clamped on BOTH ends per mode — so neither pure
    // black nor pure white can slip through. Without the second
    // bound a Dark-mode pure-black accent stayed at L = 0 (invisible
    // on the dark panel) and a Light-mode pure-white accent stayed
    // at L = 1 (invisible on white). The mid-range chromatic
    // accents (yellow ≈ 0.50, mid-blue ≈ 0.30) still get yanked
    // toward the same target the previous single-cap did, so the
    // existing "yellow → mustard on Dark" behaviour is preserved.
    //
    // Pure neutrals (S = 0) end up as a flat grey at the clamped L
    // — black on Dark becomes a dim grey ≈ #383838, white on Light
    // becomes a soft grey ≈ #C7C7C7 — no longer invisible.
    let new_l = if is_light {
        l.clamp(0.60, 0.78)
    } else {
        l.clamp(0.22, 0.40)
    };
    // BOOST saturation by 12 % so colours come out more vivid, not
    // washed. Neutrals (grey, white, black) have ~0 saturation so
    // the boost has no effect on them; chromatic accents pop more.
    let new_s = (hsl.s * 1.12).min(1.0);
    let adjusted = PastelColor::from_hsla(hsl.h, new_s, new_l, 1.0);
    let rgba = adjusted.to_rgba();
    egui::Color32::from_rgb(rgba.r, rgba.g, rgba.b)
}

/// Linear RGB blend of two colours by `t` in `[0, 1]`. Internal
/// helper for theme-aware fill resolvers.
pub(crate) fn lerp_rgb(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let f = t.clamp(0.0, 1.0);
    let lerp = |x: u8, y: u8| ((x as f32) * (1.0 - f) + (y as f32) * f).round() as u8;
    egui::Color32::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}

/// The neutral fill used for "track" surfaces — slider / progress
/// bar unfilled tracks, toggle pill OFF state, search field
/// background, dropdown trigger, popup fill, and any DragValue /
/// TextEdit input (via `Visuals.extreme_bg_color`).
///
/// PRO returns the dark `bg_input` palette colour (a sunken-input
/// look against the lighter card / panel surfaces). GAME picks a
/// shade slightly DARKER than the panel itself — `lerp(BLACK,
/// accent, lerp_factor - 0.20)` where `lerp_factor` is the panel's
/// own — so tracks read as a consistent "input on the accent
/// panel" tier rather than a near-black block sitting on a bright
/// accent.
/// Paint a dashed line between two points by walking the segment
/// from `p1` to `p2` in `dash_on + dash_off` increments. Cheap; one
/// `line_segment` shape per dash. Used by the row hairline and
/// section title trailing rule when a theme requests dashes
/// (`row_separator_dash`).
pub fn paint_dashed_line(
    painter: &egui::Painter,
    p1: egui::Pos2,
    p2: egui::Pos2,
    dash_on: f32,
    dash_off: f32,
    stroke: egui::Stroke,
) {
    let total = (p2 - p1).length();
    if total <= 0.0 || dash_on <= 0.0 {
        return;
    }
    let dir = (p2 - p1) / total;
    let step = dash_on + dash_off.max(0.0);
    let mut t = 0.0;
    while t < total {
        let start = p1 + dir * t;
        let end_t = (t + dash_on).min(total);
        let end = p1 + dir * end_t;
        painter.line_segment([start, end], stroke);
        t += step;
    }
}

/// Fill for a *nested* card (subsection / group frame) — picks a
/// surface that reads as one tier brighter than its parent section.
///
/// PRO returns `bg_hover` (the pre-existing recipe — a brighter
/// neutral that the original kit shipped with). GAME / any
/// accent-fill theme returns `pane_fill` lerped 6 % toward white,
/// so a subsection sits on a slightly raised dark-accent variant
/// instead of a hard-coded grey that doesn't match the parent
/// section's accent-tinted bg.
pub fn subsection_fill(accent: egui::Color32) -> egui::Color32 {
    let th = theme();
    match th.panel_fill_mode {
        ColorMode::FromAccent {
            lerp_factor,
            lerp_target,
        } => {
            let base = lerp_rgb(lerp_target, accent, lerp_factor);
            lerp_rgb(base, raise_target(lerp_target), 0.06)
        }
        ColorMode::FromBg => th.bg_hover,
    }
}

/// Direction to lerp TOWARD when raising a surface one tier above
/// the panel. Always white — both Dark and Light modes treat
/// "raised / elevated" as "lighter" so inputs / popups / subsections
/// look consistently raised instead of mirrored between modes.
/// (Earlier this returned the visual opposite of `lerp_target`,
/// which inverted the elevation direction in light mode and made
/// raised surfaces look sunken — fixed.)
fn raise_target(_lerp_target: egui::Color32) -> egui::Color32 {
    // Mode-aware: on Dark themes the panel is dark, so "raised"
    // surfaces lift TOWARD WHITE (visibly brighter). On Light themes
    // the panel is white-ish, so "raised" surfaces lift TOWARD BLACK
    // (visibly darker — a subtle shadow tier). Hardcoding WHITE
    // inverted the elevation direction in Light mode and made
    // dropdowns / inputs paint BRIGHTER than the panel they sit on,
    // i.e. invisible.
    if theme().is_light {
        egui::Color32::BLACK
    } else {
        egui::Color32::WHITE
    }
}

/// Direction a body widget (button, dropdown trigger, …) should
/// lerp from its surface fill to read as "raised" against the
/// panel. On Dark themes that's the user-picked accent (already
/// bright after `adapt_accent_to_mode`). On Light themes it's a
/// dimmed accent dragged 65 % toward black, so the lerp goes
/// visibly DARKER than the white-ish panel regardless of how
/// bright the user's raw accent was.
pub fn surface_lift_target(accent: egui::Color32) -> egui::Color32 {
    if theme().is_light {
        lerp_rgb(accent, egui::Color32::BLACK, 0.65)
    } else {
        accent
    }
}

/// Background fill for an alternating row. Returns `None` when the
/// active theme has `row_alternation = false` OR `row_index` is even
/// (zebra paints odd rows, even rows stay on the bare panel). When
/// the row IS to be tinted, the result is the panel base lifted
/// `row_alt_lift` toward white in straight RGB — avoids the hue
/// shift you'd get pulling toward a coloured highlight, which keeps
/// accent-tinted GAME panels reading as a single colour family.
pub fn row_alt_fill(accent: egui::Color32, row_index: u32) -> Option<egui::Color32> {
    let th = theme();
    if !th.row_alternation || row_index.is_multiple_of(2) {
        return None;
    }
    let base = pane_fill(accent);
    Some(lerp_rgb(base, egui::Color32::WHITE, th.row_alt_lift))
}

pub fn track_fill(accent: egui::Color32) -> egui::Color32 {
    let th = theme();
    match th.panel_fill_mode {
        ColorMode::FromAccent {
            lerp_factor,
            lerp_target,
        } => {
            // Track sits one tier ABOVE the panel — raise toward
            // the opposite of the panel's `lerp_target` so dark
            // panels raise toward white and light panels raise
            // toward black. Either way the input reads as one tier
            // up from the surrounding panel.
            let panel_color = lerp_rgb(lerp_target, accent, lerp_factor);
            lerp_rgb(panel_color, raise_target(lerp_target), 0.10)
        }
        ColorMode::FromBg => th.bg_input,
    }
}

/// Fill colour for floating popup surfaces — dropdown lists, the
/// command palette, context menus. Sits above the panel as a
/// "raised" tier:
/// - PRO returns `bg_raised` (existing behaviour).
/// - GAME returns a shade halfway between the panel and the track
///   (≈ panel - 0.10 lerp), so the popup is distinguishable from
///   both but stays in the same accent family.
pub fn popup_fill(accent: egui::Color32) -> egui::Color32 {
    let th = theme();
    match th.panel_fill_mode {
        ColorMode::FromAccent {
            lerp_factor,
            lerp_target,
        } => {
            // Popup sits one tier ABOVE the panel — raises toward
            // the opposite of the panel's `lerp_target` so it works
            // identically in dark and light modes.
            let panel_color = lerp_rgb(lerp_target, accent, lerp_factor);
            lerp_rgb(panel_color, raise_target(lerp_target), 0.18)
        }
        ColorMode::FromBg => th.bg_raised,
    }
}

// ─── Theme-aware text colours ───────────────────────────────────────
//
// Six no-arg helpers covering the three mara surfaces (panel,
// section, track) × two intensities (primary / dim). Each picks a
// luma-contrasted text colour against the surface fill the active
// theme + the active accent produce, so a yellow accent on PRO and
// a pastel accent on GAME both stay readable. The "dim" variant
// blends the primary text 40 % toward the surface — same role
// `TEXT_SECONDARY` plays on a dark panel, just generalised.
//
// These read from [`active_accent`] so callers don't have to thread
// the accent through every widget signature. `apply_theme` keeps
// the active accent in sync each frame.

fn dim_against(text: egui::Color32, surface: egui::Color32) -> egui::Color32 {
    // 40 % blend toward the surface — close enough to the surface to
    // read as "secondary" hierarchy, far enough off to stay
    // legible. Matches the visual weight `TEXT_SECONDARY` (#9A) had
    // against `BG_1_PANEL` (#24).
    lerp_rgb(text, surface, 0.4)
}

/// Primary-weight text colour for paint directly on the pane fill.
/// Reads `text_primary` from the active theme — predictable per
/// `Mode` regardless of how the user's accent shifts the panel's
/// luma. (The previous implementation derived this from
/// `contrast_text_for(pane_fill)` and would flip unexpectedly when
/// an accent landed near the panel-mode's mid-luma; switching to a
/// direct theme-field read makes Dark always return light text and
/// Light always return dark text, which is what callers actually
/// want.)
pub fn on_panel() -> egui::Color32 {
    theme().palette.text_primary
}
/// Secondary-weight (~`TEXT_SECONDARY` role) version of [`on_panel`].
pub fn on_panel_dim() -> egui::Color32 {
    theme().palette.text_secondary
}

/// Primary-weight text colour for paint inside a section frame.
/// Same direct-from-theme rule as [`on_panel`] — sections share the
/// brightness mode of their parent pane.
pub fn on_section() -> egui::Color32 {
    theme().palette.text_primary
}
/// Secondary-weight version of [`on_section`].
pub fn on_section_dim() -> egui::Color32 {
    theme().palette.text_secondary
}

/// Primary-weight text colour for paint on a track surface — search
/// field input, dropdown trigger label, slider/progress-bar readout
/// over the unfilled portion. Same direct-from-theme rule.
pub fn on_track() -> egui::Color32 {
    theme().palette.text_primary
}
/// Secondary-weight version of [`on_track`].
pub fn on_track_dim() -> egui::Color32 {
    theme().palette.text_secondary
}

/// Derived "hover" variant of the runtime accent — used by the
/// scrollbar's foreground colour for the handle's hover state, and
/// by any widget that wants a lighter accent for hover affordance.
/// Lerps the accent 25 % toward white. Replaces the legacy
/// hardcoded `ACCENT_HOVER` constant which never tracked the user's
/// chosen accent.
pub fn accent_hover() -> egui::Color32 {
    lerp_rgb(active_accent(), egui::Color32::WHITE, 0.25)
}

/// Derived "pressed" variant of the runtime accent — used by the
/// scrollbar's drag-state foreground and the code-editor selection
/// fill. Lerps the accent 25 % toward black. Replaces the legacy
/// `ACCENT_PRESSED`.
pub fn accent_pressed() -> egui::Color32 {
    lerp_rgb(active_accent(), egui::Color32::BLACK, 0.25)
}

/// Fill colour used by **multi-state row widgets** (tree row, hybrid
/// select row, dropdown popup row, command-palette row) when the
/// row is being hovered. Rather than the static `bg_hover` palette
/// colour these widgets used to hardcode, this helper accent-tints
/// the surface so hover pops on GAME's accent panel and stays
/// recognisable on PRO's dark panel.
pub fn row_hover_fill(accent: egui::Color32) -> egui::Color32 {
    let th = theme();
    let surface = if th.container.show_frame {
        section_fill(accent)
    } else {
        pane_fill(accent)
    };
    // 18 % accent blend — enough to tint hover, not enough to
    // collide with the 45 % accent blend used for "selected".
    lerp_rgb(surface, accent, 0.18)
}

/// Fill colour used by **multi-state row widgets** when the row is
/// the selected one. A 45 % accent blend over the row's natural
/// surface — clearly louder than `row_hover_fill` (18 %) so hover
/// and selected never visually collapse, even on flat themes
/// without strokes / glass.
pub fn row_selected_fill(accent: egui::Color32) -> egui::Color32 {
    let th = theme();
    let surface = if th.container.show_frame {
        section_fill(accent)
    } else {
        pane_fill(accent)
    };
    lerp_rgb(surface, accent, 0.45)
}

pub mod font {
    //! Typographic hierarchy — specific sizes so "small", "body",
    //! "strong" read as distinct tiers. Bodies 11 pt; captions 10;
    //! small-numeric (monospace, readouts) 11.
    pub const TITLE: f32 = 13.0;
    pub const BODY: f32 = 11.0;
    pub const CAPTION: f32 = 10.0;
    pub const NUMERIC: f32 = 11.0;
}

/// Draw a 1 px subtle divider line across the current row. Used to
/// separate the section header from its body and to split in-section
/// blocks (e.g. vehicle info vs controls).
pub fn divider(ui: &mut egui::Ui) {
    let bw = theme().stroke.border_width;
    if bw <= 0.0 {
        return;
    }
    let full_width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(full_width, 1.0), egui::Sense::empty());
    let th = theme();
    let base = outline_base();
    let color =
        egui::Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), th.stroke.border_alpha);
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        egui::Stroke::new(bw, color),
    );
}

/// Title divider — same shape as [`divider`]. Used under foldable
/// section headers so the title block stands apart from the body
/// content. Routes through `outline_base` + `border_alpha` so it
/// matches every other border / outline in the kit (was hardcoded
/// to α 220, far stronger than the kit's actual border weight).
pub fn thin_divider(ui: &mut egui::Ui) {
    let bw = theme().stroke.border_width;
    if bw <= 0.0 {
        return;
    }
    let full_width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(full_width, 1.0), egui::Sense::empty());
    let th = theme();
    let base = outline_base();
    let color =
        egui::Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), th.stroke.border_alpha);
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        egui::Stroke::new(bw, color),
    );
}

/// Uppercase title text for panel-level headings (above sections).
/// Pops against the Light body font via `.strong()` + an enlarged
/// point size (20 % above `font::TITLE`) + primary text colour.
pub fn title_text(label: &str) -> egui::RichText {
    egui::RichText::new(label)
        .strong()
        .size(font::TITLE * 1.20)
        .color(TEXT_PRIMARY)
}

/// Small dim label — the "what is this row" caption-sized text that
/// sits in the left cell of a labelled row. Resolves the colour via
/// `on_section_dim()` so it stays readable on whichever surface
/// the active theme + accent end up producing (PRO: dim grey on
/// dark card; GAME: contrast-tinted dim against the accent panel).
pub fn body_label(label: &str) -> egui::RichText {
    egui::RichText::new(label).small().color(on_section_dim())
}

/// Italic caption — for under-row hints ("drag to edit", etc.). Like
/// `body_label`, the colour comes from the active theme's
/// dim-against-section helper so it never decays into "grey on
/// bright accent" under GAME.
pub fn caption(label: &str) -> egui::RichText {
    egui::RichText::new(label)
        .small()
        .italics()
        .color(on_section_dim())
}

/// Text colour for paint on top of any fill. **Mode-driven** —
/// returns `theme().text_primary` always (light in Dark mode, dark
/// in Light mode). Consistent body-text tone across every accent
/// surface; the kit relies on `adapt_accent_to_mode` to pull bright
/// accents into a darker band in Dark mode so white-on-accent has
/// real contrast.
pub fn contrast_text_for(_fill: egui::Color32) -> egui::Color32 {
    theme().palette.text_primary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_access_recovers_from_poisoned_lock() {
        let poisoned = std::thread::spawn(|| {
            let _guard = theme_lock().write().expect("theme lock should be writable");
            panic!("intentionally poison the theme lock");
        })
        .join()
        .is_err();
        assert!(poisoned);

        let recovered = theme();
        assert_eq!(recovered.name, "PRO_DARK");

        set_theme(theme_flat(Mode::Light));
        assert_eq!(theme().name, "FLAT_LIGHT");
        set_theme(theme_pro(Mode::Dark));
    }
}
