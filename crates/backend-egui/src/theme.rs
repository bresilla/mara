//! Theme + font application and the enforced per-pass defaults.
//!
//! These build the backend's own visuals out of Mara's theme data, so
//! they belong with the backend rather than with the data.

use mara_core::context::MaraCtx;
use mara_core::enforce::{
    FallbackShell, app_theme_pass_key, enforced_shell_pass_key, enforcing, fallback_bar_due,
    fallback_state_key, fresh, set_enforcing,
};
use mara_core::layout::TextMeasureSpec;
use mara_core::paint::PaintCmd;
use mara_core::style::*;
use mara_core::vocab::{Color32 as MaraColor32, Vec2};
use std::sync::atomic::{AtomicU8, Ordering};

/// Push a `FontDefinitions` that binds:
///
/// * The selected body weight as **face 0** of `Proportional` and
///   `Monospace` — every native egui widget (Label, Button, …)
///   picks it up automatically.
/// * The selected title weight under [`TITLE_FAMILY_NAME`] as
///   `FontFamily::Name(...)` so the pane / section title sites can
///   paint with a heavier face independently of the body.
/// * Every iconflow Fluent UI variant under its own named family
///   (`install_iconflow_fonts`).
///
/// Called from the internal theme hook whenever either weight changes;
/// the dedup atomics keep the cost to a single `ctx.set_fonts` per change.
#[doc(hidden)]
pub fn __internal_install_fonts(ctx: &egui::Context, body: FontWeight, title: FontWeight) {
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

    install_iconflow_fonts(&mut fonts);
    ctx.set_fonts(fonts);
    // Do NOT flip the ready flags here — `ctx.set_fonts` queues the
    // new `FontDefinitions` for the NEXT pass, so any paint that
    // happens in the rest of THIS pass would still find the
    // FontFamily::Name unbound and panic ("FontFamily::Name(...) is
    // not bound to any fonts"). The flags are flipped one frame
    // later by `__internal_apply_theme` on its `else` branch (no install
    // needed → fonts have been alive on the ctx for at least one
    // pass), and by then egui has actually accepted the binding.
    ctx.request_repaint();
}

/// Apply the mara theme to the given egui context.
///
/// Internal first-party backend hook. Hosts expose this through Mara
/// facade APIs instead of handing app code raw backend contexts. The
/// function de-dupes internally via a static cache so re-calling with
/// the same `(accent, opacity)` skips the `ctx.set_global_style` /
/// `ctx.set_fonts` work.
#[doc(hidden)]
pub fn __internal_apply_theme(ctx: &egui::Context, accent: AccentColor, opacity: GlassOpacity) {
    use core::sync::atomic::{AtomicU32, AtomicUsize};

    // Record that a theme was applied this pass (no-op while the
    // enforcement fallback itself applies it) so `mara_core::enforce`
    // doesn't override an app/host-applied theme.
    let seam = crate::EguiCtx::new(ctx);
    mara_core::enforce::mark_app_theme_applied(&seam);

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
    // internal theme-application call always installs fonts, and any later
    // `set_font_weight` / `set_title_weight` change is detected by
    // comparing these against the live atomics.
    static LAST_BODY_WEIGHT: AtomicU8 = AtomicU8::new(u8::MAX);
    static LAST_TITLE_WEIGHT: AtomicU8 = AtomicU8::new(u8::MAX);
    // Touch density gates the spacing bump in `__internal_apply_theme_to`. Track
    // it in the dedup set so crossing the handheld/desktop threshold
    // re-pushes the egui style even when accent/theme are unchanged.
    // `u8::MAX` is the never-applied sentinel.
    static LAST_TOUCH_DENSITY: AtomicU8 = AtomicU8::new(u8::MAX);

    // Publish the per-frame responsive metrics BEFORE the dedup gate,
    // so `screen_class()` / `touch_density()` stay current every frame
    // even when the theme itself hasn't changed.
    set_screen_metrics(&seam);
    let touch_u8 = touch_density() as u8;

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
    let accent_raw: egui::Color32 = accent.0.into();
    let accent_col: egui::Color32 = if th.pastel_accent {
        adapt_accent_to_mode(accent_raw, th.is_light).into()
    } else {
        accent_raw
    };
    set_raw_accent(accent_raw.into());
    let body_w = font_weight();
    let title_w = title_weight();
    let body_u8 = body_w.as_u8();
    let title_u8 = title_w.as_u8();
    if LAST_BODY_WEIGHT.load(Ordering::Relaxed) != body_u8
        || LAST_TITLE_WEIGHT.load(Ordering::Relaxed) != title_u8
    {
        __internal_install_fonts(ctx, body_w, title_w);
        LAST_BODY_WEIGHT.store(body_u8, Ordering::Relaxed);
        LAST_TITLE_WEIGHT.store(title_u8, Ordering::Relaxed);
    } else {
        // No install needed → set_fonts (if any) ran on a PREVIOUS
        // frame, so by now egui has bound the FontFamily::Name(...)
        // entries we registered. Flip the ready flags now so paint
        // sites stop falling back to `Proportional` and start using
        // the iconflow + title families.
        if !mara_core::icons::ICONFLOW_FONTS_READY.load(Ordering::Relaxed) {
            mara_core::icons::ICONFLOW_FONTS_READY.store(true, Ordering::Release);
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
        && LAST_TOUCH_DENSITY.load(Ordering::Relaxed) == touch_u8
    {
        return;
    }
    LAST_ACCENT.store(packed, Ordering::Relaxed);
    LAST_OPACITY.store(opacity.0, Ordering::Relaxed);
    LAST_THEME_NAME_PTR.store(theme_ptr, Ordering::Relaxed);
    LAST_PASTEL.store(pastel_u8, Ordering::Relaxed);
    LAST_TOUCH_DENSITY.store(touch_u8, Ordering::Relaxed);
    // Publish the accent / opacity globals BEFORE applying the
    // visuals — every paint site downstream (titles, borders,
    // glass-alpha helpers) reads from these atomics rather than
    // re-deriving from `theme()`.
    set_raw_accent(accent_raw.into());
    set_glass_opacity(opacity.0);
    set_active_accent(accent_col.into());
    __internal_apply_theme_to(ctx, accent, opacity);
}

/// Apply the mara theme's *visuals* to `ctx` unconditionally,
/// bypassing [`__internal_apply_theme`]'s global de-dup cache. Useful for
/// *secondary* `egui::Context`s — the primary theme cache is keyed
/// on the theme state, not on the context, so once the parent
/// ctx has been styled the cache early-returns and any sibling
/// sub-context (e.g. the one `node_view::show` runs graph in)
/// never receives the visuals. Calling this directly skips that
/// gate.
///
/// **Does NOT publish globals** (`set_raw_accent` /
/// `set_glass_opacity` / `set_active_accent`). The caller is
/// expected to have already written those — `__internal_apply_theme` does
/// it for the primary ctx, and a sub-ctx caller should be
/// passing values *already published* (typically via
/// `active_accent()` / `glass_opacity()`), so re-writing here
/// would just double-apply pastel adaptation and corrupt the
/// downstream paint sites that read the same globals.
#[doc(hidden)]
pub fn __internal_apply_theme_to(
    ctx: &egui::Context,
    accent: AccentColor,
    // Argument retained for API symmetry with `__internal_apply_theme`; the
    // alpha levels in the visuals below read directly from the
    // `glass_opacity()` global, so the caller's value is informational
    // only. Renamed `_opacity` to silence the unused-arg lint.
    _opacity: GlassOpacity,
) {
    let th = theme();
    let accent_raw: egui::Color32 = accent.0.into();
    let accent_col: egui::Color32 = if th.pastel_accent {
        adapt_accent_to_mode(accent_raw, th.is_light).into()
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

    let unified_border: egui::Color32 = widget_border(accent_col).into();
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
    visuals.panel_fill = glass_panel.into();
    visuals.window_fill = glass_panel.into();
    visuals.window_stroke = egui::Stroke::new(stroke_w, unified_border);
    // `extreme_bg_color` is the egui visual every native input
    // (DragValue, TextEdit, ScrollArea track, …) pulls from. Route
    // it through `track_fill` so PRO keeps the dark sunken look and
    // GAME blends into the accent panel.
    visuals.extreme_bg_color = track_fill(accent_col).into();
    visuals.faint_bg_color = glass_card.into();
    visuals.code_bg_color = glass_card.into();
    visuals.override_text_color = Some(th.palette.text_primary.into());
    // Force the gamma-correct (linear) coverage→alpha curve for text in
    // both modes. egui's dark-mode default is `TwoCoverageMinusCoverageSq`,
    // which deliberately fattens glyph edges to make light text on dark
    // backgrounds look bolder. On a saturated accent fill (yellow / cyan
    // / lime) that fattened edge reads as a visible coloured halo around
    // every glyph — the "border around the text" the user sees only when
    // the accent is applied. `Linear` blends the coverage straight, so
    // the AA edge is a single 1-px transition between text and bg.
    visuals.text_options.alpha_from_coverage = egui::epaint::AlphaFromCoverage::Linear;
    visuals.selection.bg_fill = tinted_surface(accent_col.into()).into();
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
    visuals.widgets.noninteractive = widget(
        glass_panel.into(),
        th.palette.text_secondary.into(),
        unified_border,
    );
    visuals.widgets.inactive = widget(
        glass_input.into(),
        th.palette.text_primary.into(),
        unified_border,
    );
    visuals.widgets.hovered = widget(
        glass_hover.into(),
        th.palette.text_primary.into(),
        th.palette.border_inner.into(),
    );
    visuals.widgets.active = widget(accent_col, th.palette.text_primary.into(), accent_col);
    visuals.widgets.open = widget(
        glass_hover.into(),
        th.palette.text_primary.into(),
        th.palette.border_inner.into(),
    );

    let mut style = (*ctx.global_style()).clone();
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

    // Touch density: on handheld/touch surfaces, grow hit targets and
    // breathing room so controls clear the ~44 px finger-target
    // guideline without the desktop layout having to know about it.
    // Reads the per-frame `touch_density()` global published by
    // `set_screen_metrics`; the internal theme hook folds it into its dedup key
    // so the bump is re-pushed when the threshold is crossed.
    if touch_density() {
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 10.0);
        style.spacing.interact_size.y = 30.0;
        style.spacing.icon_width = 18.0;
        style.spacing.icon_spacing = 8.0;
        style.spacing.scroll.bar_width = 6.0;
        style.spacing.scroll.floating_width = 4.0;
        style.spacing.scroll.floating_allocated_width = 6.0;
    }

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
        content_margin: egui::Margin::ZERO,
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
        fade: Default::default(),
    };
    // Rest: a dimmed-accent track handle that still belongs to the
    // accent family. Hover: full ACCENT_HOVER. Drag: ACCENT_PRESSED.
    // `fg_stroke` is also used for fine foreground elements
    // (checkmarks, focus rings) — re-tinting them to accent reads as
    // an improvement, not a regression.
    let accent_dim =
        egui::Color32::from_rgba_unmultiplied(accent_col.r(), accent_col.g(), accent_col.b(), 160);
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, accent_dim);
    let accent_hover: egui::Color32 = accent_hover().into();
    let accent_pressed: egui::Color32 = accent_pressed().into();
    style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, accent_hover);
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, accent_pressed);
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

    ctx.set_global_style(style);
}

/// Enforce Mara's defaults for this pass. Called by every Mara surface
/// entry point; cheap after the first call of a pass (stamp reads).
#[doc(hidden)]
pub fn __internal_enforce_defaults(ctx: &egui::Context) {
    // The seam view of the same context; the wrapper owns an `Arc`
    // handle, so this is a refcount bump rather than a copy.
    let seam = crate::EguiCtx::new(ctx);
    let seam: &dyn MaraCtx = &seam;
    if enforcing(seam) {
        return;
    }

    // Image loaders: `PaintCmd::Svg` needs a rasteriser registered on
    // the context. Enforced here rather than left to the app so a
    // sealed module can emit `Svg` and have it render, on any host,
    // without the app knowing an image-loader chain exists.
    // `install_image_loaders` guards each loader on
    // `Context::is_loader_installed`, so calling it per pass is cheap.
    #[cfg(feature = "svg")]
    egui_extras::install_image_loaders(ctx);

    // Theme application builds backend visuals, so it stays on this
    // side of the split. It de-dupes internally, so re-applying every
    // pass for theme-less apps is cheap and tracks resizes.
    if !fresh(seam, app_theme_pass_key()) {
        set_enforcing(seam, true);
        __internal_apply_theme(
            ctx,
            mara_core::style::AccentColor(mara_core::style::raw_accent()),
            mara_core::style::glass_opacity(),
        );
        set_enforcing(seam, false);
    }

    let Some(pass) = fallback_bar_due(seam) else {
        return;
    };

    // The app has had its chance — render the Mara-owned fallback bar.
    // The default `ShellBar` (app-menu + injected host controls, no
    // views) — the same bar a bare runner app gets. An items-less bar
    // would paint nothing, which is no enforcement at all. Events are
    // dropped — there is no app wired to receive them; apps that want
    // the functional bar render `ShellBar` themselves, which
    // suppresses this fallback.
    set_enforcing(seam, true);
    let mut state = MaraCtx::memory(seam)
        .get_temp::<FallbackShell>(fallback_state_key())
        .unwrap_or_default();
    let _ = state.bar.__internal_show_egui(
        seam,
        &mut state.open,
        &mut state.placement,
        &mut state.drag,
    );
    {
        let mut memory = MaraCtx::memory(seam);
        memory.set_temp(fallback_state_key(), state);
        memory.set_temp(enforced_shell_pass_key(), pass);
    };
    set_enforcing(seam, false);
}

/// Hidden egui measurement adapter for first-party crates that have
/// already expressed text measurement as Mara-owned data but still run
/// on the current egui backend.
#[doc(hidden)]
pub fn __internal_measure_text_egui(painter: &egui::Painter, spec: &TextMeasureSpec) -> Vec2 {
    crate::measure_text_for_spec(painter, spec)
}
#[doc(hidden)]
pub fn __internal_render_paint_cmd_egui_ui(ui: &mut egui::Ui, cmd: PaintCmd) {
    crate::render_paint_cmd_ui(ui, cmd);
}

/// Build a view context over the backend's frame context.
#[must_use]
#[doc(hidden)]
pub fn __internal_view_ctx<'a>(
    egui_ctx: &'a egui::Context,
    workspace: &'a mut mara_core::WorkspaceStack,
    accent: impl Into<MaraColor32>,
    content_avoidance: mara_core::RibbonAvoidance,
) -> mara_core::ViewCtx<'a> {
    let seam: Box<dyn MaraCtx + 'a> = Box::new(crate::EguiCtx::new(egui_ctx));
    seam.enforce_defaults();
    mara_core::ViewCtx {
        region: seam.content_rect(),
        seam,
        egui_ctx,
        workspace,
        accent: accent.into(),
        content_avoidance,
    }
}

use iconflow::fonts;
use std::sync::Arc;

/// Pull every iconflow font into `FontDefinitions` and register
/// each as a named family so `FontFamily::Name(family)` resolves to
/// the right glyph table. Called from [].
///
/// Lives here rather than in  because
///  is an egui type — core owns the glyph lookup
/// (), the backend owns binding it to a font family.
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
