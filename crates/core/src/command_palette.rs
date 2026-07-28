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

use crate::{
    layout::{
        AreaHost, FrameHostSpec, Layer, ScrollRegion, Sense as MaraSense, SpaceSpec,
        TextEditRegion, TextEditSpec, UiBackend,
    },
    mui::{MaraKey, MaraResponse},
    paint::PaintCmd,
    style::{
        font, glass_alpha_card, glass_alpha_window, glass_fill, pane_fill, popup_fill,
        widget_border,
    },
    vocab::{Align2, Color32 as MaraColor32, Id, Pos2, Rect as MaraRect, Stroke, Vec2},
};

const PALETTE_WIDTH: f32 = 560.0;
const PALETTE_INNER_MARGIN_X: i8 = 8;
const PALETTE_INNER_MARGIN_Y: i8 = 6;
const PALETTE_RESULTS_MAX_HEIGHT: f32 = 320.0;
const PALETTE_RESULTS_ROW_SPACING_Y: f32 = 1.0;
const PALETTE_NAV_KEYS: [MaraKey; 4] = [
    MaraKey::Escape,
    MaraKey::ArrowDown,
    MaraKey::ArrowUp,
    MaraKey::Enter,
];

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaletteKeyOutcome {
    None,
    Dismiss,
    PickSelected,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PaletteFrameLayout {
    pos: Pos2,
    outer_width: f32,
    content_width: f32,
    results_max_height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PaletteSeparatorSpec {
    dash_on: f32,
    dash_off: f32,
    color: MaraColor32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PaletteSearchColors {
    fill: MaraColor32,
    text: MaraColor32,
    hint: MaraColor32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PaletteFrameColors {
    fill: MaraColor32,
    stroke: MaraColor32,
}

/// Draw the palette overlay through the current egui backend.
///
/// Hidden first-party hook: app/host code should use `MaraHostCtx`
/// instead of passing raw backend context handles around.
#[doc(hidden)]
pub fn __internal_command_palette_egui(
    ctx: &dyn crate::context::MaraCtx,
    state: &mut CommandPaletteState,
    items: &[PaletteItem],
    accent: impl Into<MaraColor32>,
) -> Option<&'static str> {
    ctx.enforce_defaults();
    let accent = accent.into();
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

    clamp_palette_selection(state, filtered.len());

    let mut picked: Option<&'static str> = None;

    // Keyboard input — Up / Down / Enter / Escape. Consumed
    // before the palette body draws so the text field doesn't
    // swallow them.
    if apply_palette_consumed_keys(state, filtered.len(), ctx.consume_keys(&PALETTE_NAV_KEYS))
        == PaletteKeyOutcome::PickSelected
    {
        picked = Some(filtered[state.selected].id);
    }

    let screen = crate::context::MaraCtx::content_rect(ctx);
    // Full-screen scrim so clicks outside the palette dismiss it.
    // `Order::Foreground` places it above panes, below the
    // palette itself (which we paint at `Tooltip`).
    let scrim_host = palette_scrim_area_host(screen);
    let mut scrim_clicked = false;
    ctx.area(scrim_host, &mut |mara| {
        scrim_clicked = paint_palette_scrim(mara, screen.size()).clicked();
    });
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
    let layout = palette_frame_layout(screen);
    let window_host = palette_window_area_host(items_sig, layout);
    ctx.area(window_host, &mut |mara| {
        let frame_colors = palette_frame_colors(accent);
        let frame_stroke_width = crate::style::theme().border_width;
        let frame_spec = palette_frame_host_spec(layout);
        // The frame chrome paints UNDER the content laid out inside it,
        // so its slots are reserved first and filled once the frame's
        // rect is known.
        let chrome_slots: Vec<_> = (0..3).map(|_| mara.reserve_paint_slot()).collect();
        let frame_rect = {
            let frame_rect = mara.frame_host(frame_spec, &mut |mara| {
                // Search input — themes that filled the section
                // title strip with accent (GAME) get the same
                // treatment here: full accent background, contrast
                // text on top, accent-darkened hint text. PRO falls
                // back to the original raised glass fill.
                let search_colors = palette_search_colors(accent);
                let text_region = paint_palette_search_chrome(
                    mara,
                    search_colors.fill,
                    widget_border(accent),
                    24.0,
                );
                let text_spec = palette_search_text_edit_spec(
                    text_region,
                    search_colors.text,
                    search_colors.hint,
                );
                let edit_response = mara.text_edit_at(&mut state.query, text_spec, true);
                apply_palette_text_edit(state, edit_response.changed());

                mara.add_space(palette_input_results_gap_spec());

                // Dashed separator between the input and the result
                // list — matches the row-separator language used
                // inside section bodies in the GAME theme. PRO falls
                // back to the existing 4 px gap (the dash recipe is
                // None there).
                if let Some(separator) = palette_separator_spec() {
                    // Input-to-results divider — kit-shared
                    // `outline_base` + `row_separator_alpha`. Drops
                    // the previous `.max(60)` alpha floor and the
                    // raw `border_subtle` lookup; both were the
                    // pre-unification fallback path.
                    paint_palette_dash_separator(mara, separator);
                    mara.add_space(palette_input_results_gap_spec());
                }

                // Results list.
                let results_region = palette_results_region(items_sig, layout);
                mara.scroll_region(results_region, &mut |mara| {
                    if filtered.is_empty() {
                        paint_no_matches_row(mara);
                    }
                    // Use kit-shared `outline_base` so the
                    // inter-item rule matches every other row
                    // separator across the kit.
                    let row_separator = palette_separator_spec();
                    for (i, it) in filtered.iter().enumerate() {
                        if paint_row(mara, it, i == state.selected, accent).clicked() {
                            picked = Some(it.id);
                        }
                        // Dashed inter-item rule — only in themes
                        // that opted into dashed row separators
                        // (GAME). PRO continues with the implicit
                        // `item_spacing.y` gap.
                        if let Some(separator) =
                            palette_inter_row_separator_spec(row_separator, i, filtered.len())
                        {
                            paint_palette_dash_separator(mara, separator);
                        }
                    }
                });
            });
            frame_rect
        };
        for (slot, cmd) in chrome_slots
            .into_iter()
            .zip(palette_frame_chrome_paint_cmds(
                frame_rect,
                frame_spec.corner,
                frame_colors.fill,
                Stroke::new(frame_stroke_width, frame_colors.stroke),
            ))
        {
            mara.fill_paint_slot(slot, Some(cmd));
        }

        // L-bracket corner ticks at the palette's four corners,
        // matching the section-header language. Theme-gated via
        // `section_corner_ticks`; PRO sets it to `0.0` so this
        // is a no-op there.
        let tick_len = crate::style::theme().section_corner_ticks;
        let inset = crate::style::theme().section_corner_ticks_inset;
        let mut backend = mara.backend_mut();
        palette_corner_ticks_backend(
            &mut backend,
            frame_rect,
            tick_len,
            inset,
            MaraColor32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 220),
        );
    });

    if picked.is_some() {
        state.open = false;
        state.query.clear();
        state.selected = 0;
    }

    picked
}

fn clamp_palette_selection(state: &mut CommandPaletteState, filtered_len: usize) {
    if filtered_len == 0 {
        state.selected = 0;
    } else {
        state.selected = state.selected.min(filtered_len - 1);
    }
}

fn apply_palette_key(
    state: &mut CommandPaletteState,
    key: MaraKey,
    filtered_len: usize,
) -> PaletteKeyOutcome {
    match key {
        MaraKey::Escape => {
            state.open = false;
            PaletteKeyOutcome::Dismiss
        }
        MaraKey::ArrowDown => {
            if filtered_len > 0 {
                state.selected = (state.selected + 1).min(filtered_len - 1);
            } else {
                state.selected = 0;
            }
            PaletteKeyOutcome::None
        }
        MaraKey::ArrowUp => {
            state.selected = state.selected.saturating_sub(1);
            PaletteKeyOutcome::None
        }
        MaraKey::Enter => {
            if filtered_len > 0 {
                clamp_palette_selection(state, filtered_len);
                PaletteKeyOutcome::PickSelected
            } else {
                PaletteKeyOutcome::None
            }
        }
        // The palette only consumes its four navigation keys; every
        // other key belongs to the focused text field.
        _ => PaletteKeyOutcome::None,
    }
}

fn apply_palette_consumed_keys(
    state: &mut CommandPaletteState,
    filtered_len: usize,
    keys: impl IntoIterator<Item = MaraKey>,
) -> PaletteKeyOutcome {
    let mut outcome = PaletteKeyOutcome::None;
    for key in keys {
        if apply_palette_key(state, key, filtered_len) == PaletteKeyOutcome::PickSelected {
            outcome = PaletteKeyOutcome::PickSelected;
        }
    }
    outcome
}

fn palette_overlay_pos(screen: MaraRect, width: f32) -> Pos2 {
    Pos2::new(
        screen.center().x - width * 0.5,
        screen.min.y + screen.height() * 0.22,
    )
}

fn palette_frame_layout(screen: MaraRect) -> PaletteFrameLayout {
    PaletteFrameLayout {
        pos: palette_overlay_pos(screen, PALETTE_WIDTH),
        outer_width: PALETTE_WIDTH,
        content_width: PALETTE_WIDTH - f32::from(PALETTE_INNER_MARGIN_X) * 2.0,
        results_max_height: PALETTE_RESULTS_MAX_HEIGHT,
    }
}

fn palette_results_region(items_sig: u64, layout: PaletteFrameLayout) -> ScrollRegion {
    ScrollRegion::new(
        Id::new(("mara_palette_list", items_sig)),
        [false, true],
        layout.results_max_height,
        PALETTE_RESULTS_ROW_SPACING_Y,
    )
}

fn palette_frame_host_spec(layout: PaletteFrameLayout) -> FrameHostSpec {
    FrameHostSpec::new(
        layout.outer_width,
        layout.content_width,
        [PALETTE_INNER_MARGIN_X, PALETTE_INNER_MARGIN_Y],
        crate::style::theme().radius_lg.into(),
    )
}

fn palette_input_results_gap_spec() -> SpaceSpec {
    SpaceSpec::vertical(4.0)
}

fn palette_separator_spec() -> Option<PaletteSeparatorSpec> {
    let (dash_on, dash_off) = crate::style::theme().row_separator_dash?;
    let alpha = crate::style::theme().row_separator_alpha;
    if alpha == 0 {
        return None;
    }
    Some(PaletteSeparatorSpec {
        dash_on,
        dash_off,
        color: outline_color_with_alpha(alpha),
    })
}

fn palette_search_colors(accent: MaraColor32) -> PaletteSearchColors {
    let theme_now = crate::style::theme();
    if theme_now.title_strip_filled {
        let text = crate::style::contrast_text_for(accent);
        return PaletteSearchColors {
            fill: accent,
            text,
            hint: text_with_alpha(text, 160),
        };
    }

    PaletteSearchColors {
        fill: palette_glass_fill(
            theme_now.bg_raised.into(),
            accent,
            glass_alpha_card(),
            theme_now.glass.accent_tint,
        ),
        text: crate::style::on_section(),
        hint: crate::style::on_section_dim(),
    }
}

fn palette_frame_colors(accent: MaraColor32) -> PaletteFrameColors {
    PaletteFrameColors {
        fill: glass_fill(popup_fill(accent), accent, glass_alpha_window()),
        stroke: widget_border(accent),
    }
}

fn palette_glass_fill(
    base: MaraColor32,
    accent: MaraColor32,
    alpha: u8,
    accent_tint: f32,
) -> MaraColor32 {
    let f = accent_tint.clamp(0.0, 1.0);
    let blend = |a: u8, b: u8| ((a as f32) * (1.0 - f) + (b as f32) * f).round() as u8;
    MaraColor32::from_rgba_unmultiplied(
        blend(base.r(), accent.r()),
        blend(base.g(), accent.g()),
        blend(base.b(), accent.b()),
        alpha,
    )
}

fn text_with_alpha(color: MaraColor32, alpha: u8) -> MaraColor32 {
    MaraColor32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

fn palette_inter_row_separator_spec(
    separator: Option<PaletteSeparatorSpec>,
    row_index: usize,
    filtered_len: usize,
) -> Option<PaletteSeparatorSpec> {
    if row_index + 1 < filtered_len {
        separator
    } else {
        None
    }
}

fn outline_color_with_alpha(alpha: u8) -> MaraColor32 {
    let base = crate::style::outline_base();
    MaraColor32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), alpha)
}

fn palette_search_text_edit_spec(
    region: TextEditRegion,
    text_color: MaraColor32,
    hint_color: MaraColor32,
) -> TextEditSpec {
    TextEditSpec::singleline(region, "Type a command…", text_color, hint_color)
}

fn palette_scrim_area_host(screen: MaraRect) -> AreaHost {
    AreaHost::new(Id::new("mara_palette_scrim"), screen.min, Layer::Foreground)
}

fn palette_window_area_host(items_sig: u64, layout: PaletteFrameLayout) -> AreaHost {
    AreaHost::new(
        Id::new(("mara_palette", items_sig)),
        layout.pos,
        Layer::Overlay,
    )
}

fn apply_palette_text_edit(state: &mut CommandPaletteState, query_changed: bool) {
    if query_changed {
        // Query changed — reset selection to the top of the filtered
        // list so the highlight stays sensible.
        state.selected = 0;
    }
}

fn palette_frame_chrome_paint_cmds(
    rect: MaraRect,
    corner: crate::vocab::CornerRadius,
    fill: MaraColor32,
    stroke: Stroke,
) -> [PaintCmd; 3] {
    [
        PaintCmd::Shadow {
            rect,
            corner,
            offset: [0, 10],
            blur: 28,
            spread: 0,
            color: MaraColor32::from_black_alpha(150),
        },
        PaintCmd::RectFilled { rect, corner, fill },
        PaintCmd::RectStroke {
            rect,
            corner,
            stroke,
        },
    ]
}

/// Paint one row: label on the left, optional dim hint on the
/// right. Selected row gets an accent-tinted fill so keyboard
/// navigation is visible.
fn paint_row(
    mara: &mut crate::MaraUi<'_>,
    item: &PaletteItem,
    selected: bool,
    accent: MaraColor32,
) -> MaraResponse {
    let mut backend = mara.backend_mut();
    palette_row_backend(&mut backend, item, selected, accent)
}

fn paint_no_matches_row(mara: &mut crate::MaraUi<'_>) -> MaraResponse {
    let mut backend = mara.backend_mut();
    palette_no_matches_backend(&mut backend)
}

fn paint_palette_scrim(mara: &mut crate::MaraUi<'_>, size: Vec2) -> MaraResponse {
    let mut backend = mara.backend_mut();
    palette_scrim_backend(&mut backend, size)
}

/// Backend-neutral full-window palette dismissal hit target. The
/// floating overlay host remains egui-owned for now, but the click
/// target itself is Mara layout/input data.
fn palette_scrim_backend(backend: &mut impl UiBackend, size: Vec2) -> MaraResponse {
    backend.allocate(size, MaraSense::Click)
}

fn paint_palette_search_chrome(
    mara: &mut crate::MaraUi<'_>,
    fill: MaraColor32,
    border: MaraColor32,
    height: f32,
) -> TextEditRegion {
    let mut backend = mara.backend_mut();
    palette_search_chrome_backend(&mut backend, fill, border, height)
}

/// Backend-neutral command-palette query field chrome. Text editing
/// stays egui-owned for now; the field fill/border and text rect
/// are Mara-owned data/paint commands.
fn palette_search_chrome_backend(
    backend: &mut impl UiBackend,
    fill: MaraColor32,
    border: MaraColor32,
    height: f32,
) -> TextEditRegion {
    let available = backend.available_rect();
    let rect = backend
        .allocate(
            Vec2::new(available.width().max(0.0), height),
            MaraSense::Click,
        )
        .rect;
    let corner = crate::style::theme().radius_md.into();
    backend.paint(PaintCmd::RectFilled { rect, corner, fill });
    backend.paint(PaintCmd::RectStroke {
        rect,
        corner,
        stroke: Stroke::new(crate::style::theme().border_width, border),
    });
    TextEditRegion::new(
        rect,
        MaraRect::from_min_max(
            Pos2::new(rect.min.x + 8.0, rect.min.y),
            Pos2::new(rect.max.x - 8.0, rect.max.y),
        ),
        crate::style::BODY_FONT_SIZE,
    )
}

/// Backend-neutral empty-state row for the command palette.
fn palette_no_matches_backend(backend: &mut impl UiBackend) -> MaraResponse {
    const ROW_H: f32 = 24.0;
    let available = backend.available_rect();
    let response = backend.allocate(
        Vec2::new(available.width().max(0.0), ROW_H),
        MaraSense::Hover,
    );
    backend.paint(PaintCmd::Text {
        pos: Pos2::new(response.rect.min.x + 8.0, response.rect.center().y),
        anchor: Align2::LEFT_CENTER,
        text: "No matches".to_owned(),
        size: font::BODY,
        color: crate::style::on_section_dim(),
        mono: false,
    });
    response
}

/// Backend-neutral command-palette result row. The overlay, scroll
/// host, keyboard routing, and text input remain egui-owned for now,
/// but the result-list chrome itself is Mara layout/paint/input.
fn palette_row_backend(
    backend: &mut impl UiBackend,
    item: &PaletteItem,
    selected: bool,
    accent: MaraColor32,
) -> MaraResponse {
    const ROW_H: f32 = 24.0;
    let available = backend.available_rect();
    let response = backend.allocate(
        Vec2::new(available.width().max(0.0), ROW_H),
        MaraSense::Click,
    );
    let bg: Option<MaraColor32> = if selected {
        Some(crate::style::row_selected_fill(accent))
    } else if response.hovered() {
        Some(crate::style::row_hover_fill(accent))
    } else {
        None
    };
    if let Some(c) = bg {
        backend.paint(PaintCmd::RectFilled {
            rect: response.rect,
            corner: crate::style::theme().radius_md.into(),
            fill: c,
        });
    }
    let mid_y = response.rect.center().y;
    // Palette rows sit on the palette frame (panel-style fill).
    // Selected/hovered rows are accent-blended; pick contrast
    // against whatever the row ended up coloured.
    let row_bg = bg.unwrap_or(pane_fill(accent));
    let row_text = crate::style::contrast_text_for(row_bg);
    let row_text_dim = row_text;
    backend.paint(PaintCmd::Text {
        pos: Pos2::new(response.rect.min.x + 10.0, mid_y),
        anchor: Align2::LEFT_CENTER,
        text: item.label.to_owned(),
        size: font::BODY + 2.0,
        color: row_text,
        mono: false,
    });
    if let Some(hint) = item.hint {
        backend.paint(PaintCmd::Text {
            pos: Pos2::new(response.rect.max.x - 10.0, mid_y),
            anchor: Align2::RIGHT_CENTER,
            text: hint.to_owned(),
            size: font::CAPTION,
            color: row_text_dim,
            mono: false,
        });
    }
    response
}

/// Backend-neutral palette frame corner ticks. The egui overlay
/// still owns the floating Area and shadowed Frame; this helper
/// moves the decorative L-bracket chrome into Mara paint commands.
fn palette_corner_ticks_backend(
    backend: &mut impl UiBackend,
    rect: MaraRect,
    tick_len: f32,
    inset: f32,
    color: MaraColor32,
) {
    if tick_len <= 0.0 {
        return;
    }

    let rect = if inset > 0.0 {
        MaraRect::from_min_max(
            Pos2::new(rect.min.x + inset, rect.min.y + inset),
            Pos2::new(rect.max.x - inset, rect.max.y - inset),
        )
    } else {
        rect
    };
    let snap_low = |v: f32| v.round() + 0.5;
    let snap_high = |v: f32| v.round() - 0.5;
    let lx = snap_low(rect.min.x);
    let ty = snap_low(rect.min.y);
    let rx = snap_high(rect.max.x);
    let by = snap_high(rect.max.y);
    let stroke = Stroke::new(1.0, color);
    let mut line = |a: Pos2, b: Pos2| {
        backend.paint(PaintCmd::Line { a, b, stroke });
    };

    line(Pos2::new(lx, ty), Pos2::new(lx + tick_len, ty));
    line(Pos2::new(lx, ty), Pos2::new(lx, ty + tick_len));
    line(Pos2::new(rx - tick_len, ty), Pos2::new(rx, ty));
    line(Pos2::new(rx, ty), Pos2::new(rx, ty + tick_len));
    line(Pos2::new(lx, by - tick_len), Pos2::new(lx, by));
    line(Pos2::new(lx, by), Pos2::new(lx + tick_len, by));
    line(Pos2::new(rx - tick_len, by), Pos2::new(rx, by));
    line(Pos2::new(rx, by - tick_len), Pos2::new(rx, by));
}

fn paint_palette_dash_separator(
    mara: &mut crate::MaraUi<'_>,
    spec: PaletteSeparatorSpec,
) -> MaraResponse {
    let mut backend = mara.backend_mut();
    palette_dash_separator_backend(&mut backend, spec)
}

/// Backend-neutral 1px horizontal dashed palette divider.
fn palette_dash_separator_backend(
    backend: &mut impl UiBackend,
    spec: PaletteSeparatorSpec,
) -> MaraResponse {
    let available = backend.available_rect();
    let response = backend.allocate(Vec2::new(available.width().max(0.0), 1.0), MaraSense::Hover);
    palette_dashed_line_backend(
        backend,
        response.rect.left_center(),
        response.rect.right_center(),
        spec.dash_on,
        spec.dash_off,
        Stroke::new(1.0, spec.color),
    );
    response
}

fn palette_dashed_line_backend(
    backend: &mut impl UiBackend,
    p1: Pos2,
    p2: Pos2,
    dash_on: f32,
    dash_off: f32,
    stroke: Stroke,
) {
    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    let total = (dx * dx + dy * dy).sqrt();
    if total <= 0.0 || dash_on <= 0.0 {
        return;
    }
    let dir_x = dx / total;
    let dir_y = dy / total;
    let step = dash_on + dash_off.max(0.0);
    let mut t = 0.0;
    while t < total {
        let end_t = (t + dash_on).min(total);
        backend.paint(PaintCmd::Line {
            a: Pos2::new(p1.x + dir_x * t, p1.y + dir_y * t),
            b: Pos2::new(p1.x + dir_x * end_t, p1.y + dir_y * end_t),
            stroke,
        });
        t += step;
    }
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
    use crate::{
        layout::Layer,
        vocab::{Id, Rect},
    };

    use crate::backend::record::RecordingBackend;

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

    #[test]
    fn palette_keyboard_state_backend_moves_dismisses_and_picks() {
        let mut state = CommandPaletteState {
            open: true,
            query: String::new(),
            selected: 99,
        };

        clamp_palette_selection(&mut state, 3);
        assert_eq!(state.selected, 2);

        assert_eq!(
            apply_palette_key(&mut state, MaraKey::ArrowDown, 3),
            PaletteKeyOutcome::None
        );
        assert_eq!(state.selected, 2);

        assert_eq!(
            apply_palette_key(&mut state, MaraKey::ArrowUp, 3),
            PaletteKeyOutcome::None
        );
        assert_eq!(state.selected, 1);

        assert_eq!(
            apply_palette_key(&mut state, MaraKey::Enter, 3),
            PaletteKeyOutcome::PickSelected
        );
        assert!(state.open);

        assert_eq!(
            apply_palette_key(&mut state, MaraKey::Escape, 3),
            PaletteKeyOutcome::Dismiss
        );
        assert!(!state.open);

        let mut state = CommandPaletteState {
            open: true,
            query: String::new(),
            selected: 0,
        };
        assert_eq!(
            apply_palette_consumed_keys(
                &mut state,
                3,
                [MaraKey::ArrowDown, MaraKey::ArrowDown, MaraKey::Enter]
            ),
            PaletteKeyOutcome::PickSelected
        );
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn palette_overlay_position_backend_centers_width_and_uses_vertical_fraction() {
        let screen = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(1000.0, 500.0));

        let pos = palette_overlay_pos(screen, 560.0);

        assert_eq!(pos.x, 230.0);
        assert_eq!(pos.y, 130.0);
    }

    #[test]
    fn palette_frame_layout_backend_derives_host_geometry() {
        let screen = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(1000.0, 500.0));

        let layout = palette_frame_layout(screen);

        assert_eq!(layout.pos, Pos2::new(230.0, 130.0));
        assert_eq!(layout.outer_width, 560.0);
        assert_eq!(layout.content_width, 544.0);
        assert_eq!(layout.results_max_height, 320.0);
    }

    #[test]
    fn palette_results_region_backend_derives_scroll_host_contract() {
        let screen = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(1000.0, 500.0));
        let layout = palette_frame_layout(screen);

        let region = palette_results_region(42, layout);

        assert_eq!(region.id, Id::new(("mara_palette_list", 42)));
        assert_eq!(region.axis, crate::layout::ScrollAxis::Vertical);
        assert_eq!(region.auto_shrink, [false, true]);
        assert_eq!(region.max_extent, 320.0);
        assert_eq!(region.item_spacing, Vec2::new(0.0, 1.0));
    }

    #[test]
    fn palette_frame_host_spec_backend_derives_frame_contract() {
        let screen = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(1000.0, 500.0));
        let layout = palette_frame_layout(screen);

        let spec = palette_frame_host_spec(layout);

        assert_eq!(spec.outer_width, 560.0);
        assert_eq!(spec.content_width, 544.0);
        assert_eq!(
            spec.inner_margin,
            [PALETTE_INNER_MARGIN_X, PALETTE_INNER_MARGIN_Y]
        );
        let expected_corner: crate::vocab::CornerRadius = crate::style::theme().radius_lg.into();
        assert_eq!(spec.corner, expected_corner);
    }

    #[test]
    fn palette_gap_spec_backend_derives_fixed_vertical_spacing() {
        let spec = palette_input_results_gap_spec();

        assert_eq!(spec.size, Vec2::new(0.0, 4.0));
    }

    #[test]
    fn palette_search_color_backend_derives_mara_glass_and_hint_alpha() {
        let glass = palette_glass_fill(
            MaraColor32::from_rgb(100, 110, 120),
            MaraColor32::from_rgb(200, 10, 20),
            180,
            0.25,
        );

        assert_eq!(glass, MaraColor32::from_rgba_unmultiplied(125, 85, 95, 180));
        assert_eq!(
            text_with_alpha(MaraColor32::from_rgb(20, 30, 40), 160),
            MaraColor32::from_rgba_unmultiplied(20, 30, 40, 160)
        );
    }

    #[test]
    fn palette_frame_colors_stay_in_mara_vocabulary() {
        let accent = MaraColor32::from_rgb(80, 120, 200);

        let colors = palette_frame_colors(accent);

        assert_eq!(
            colors.fill,
            glass_fill(popup_fill(accent), accent, glass_alpha_window())
        );
        assert_eq!(colors.stroke, widget_border(accent));
    }

    #[test]
    fn palette_inter_row_separator_backend_omits_after_last_row() {
        let separator = Some(PaletteSeparatorSpec {
            dash_on: 4.0,
            dash_off: 2.0,
            color: MaraColor32::WHITE,
        });

        assert_eq!(palette_inter_row_separator_spec(separator, 0, 2), separator);
        assert_eq!(palette_inter_row_separator_spec(separator, 1, 2), None);
        assert_eq!(palette_inter_row_separator_spec(None, 0, 2), None);
    }

    #[test]
    fn palette_area_hosts_backend_derive_id_position_and_layer() {
        let screen = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(1000.0, 500.0));
        let layout = palette_frame_layout(screen);

        let scrim = palette_scrim_area_host(screen);
        let window = palette_window_area_host(42, layout);

        assert_eq!(scrim.id, Id::new("mara_palette_scrim"));
        assert_eq!(scrim.pos, screen.min);
        assert_eq!(scrim.layer, Layer::Foreground);

        assert_eq!(window.id, Id::new(("mara_palette", 42)));
        assert_eq!(window.pos, layout.pos);
        assert_eq!(window.layer, Layer::Overlay);
    }

    #[test]
    fn palette_text_edit_backend_resets_selection_on_query_change() {
        let mut state = CommandPaletteState {
            open: true,
            query: "open".to_owned(),
            selected: 7,
        };

        apply_palette_text_edit(&mut state, true);

        assert_eq!(state.selected, 0);

        state.selected = 3;
        apply_palette_text_edit(&mut state, false);

        assert_eq!(state.selected, 3);
    }

    #[test]
    fn palette_search_text_edit_spec_backend_carries_hint_and_colors() {
        let region = TextEditRegion::new(
            Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(240.0, 24.0)),
            Rect::from_min_size(Pos2::new(8.0, 0.0), Vec2::new(224.0, 24.0)),
            13.0,
        );

        let spec = palette_search_text_edit_spec(
            region,
            MaraColor32::WHITE,
            MaraColor32::from_black_alpha(160),
        );

        assert_eq!(spec.region, region);
        assert_eq!(spec.hint, "Type a command…");
        assert_eq!(spec.text_color, MaraColor32::WHITE);
        assert_eq!(spec.hint_color, MaraColor32::from_black_alpha(160));
        assert_eq!(spec.background_color, MaraColor32::TRANSPARENT);
        assert!(!spec.frame);
    }

    #[test]
    fn palette_row_backend_emits_selected_bg_label_and_hint() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(240.0, 24.0)),
            paints: Vec::new(),
            ..Default::default()
        };
        let item = PaletteItem::new("open", "Open Project").with_hint("Ctrl+O");

        let response = palette_row_backend(
            &mut backend,
            &item,
            true,
            MaraColor32::from_rgb(120, 80, 255),
        );

        assert_eq!(response.rect.width(), 240.0);
        let [
            PaintCmd::RectFilled { .. },
            PaintCmd::Text { text: label, .. },
            PaintCmd::Text { text: hint, .. },
        ] = backend.paints.as_slice()
        else {
            panic!("selected palette row should emit bg, label and hint commands");
        };
        assert_eq!(label, "Open Project");
        assert_eq!(hint, "Ctrl+O");
    }

    #[test]
    fn palette_row_backend_omits_bg_and_hint_when_plain() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(240.0, 24.0)),
            paints: Vec::new(),
            ..Default::default()
        };
        let item = PaletteItem::new("close", "Close Project");

        let _ = palette_row_backend(
            &mut backend,
            &item,
            false,
            MaraColor32::from_rgb(120, 80, 255),
        );

        let [PaintCmd::Text { text, .. }] = backend.paints.as_slice() else {
            panic!("plain palette row should emit only label text");
        };
        assert_eq!(text, "Close Project");
    }

    #[test]
    fn palette_no_matches_backend_emits_empty_state_text() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(240.0, 24.0)),
            paints: Vec::new(),
            ..Default::default()
        };

        let response = palette_no_matches_backend(&mut backend);

        assert_eq!(response.rect.width(), 240.0);
        let [PaintCmd::Text { text, .. }] = backend.paints.as_slice() else {
            panic!("no-matches row should emit one text command");
        };
        assert_eq!(text, "No matches");
    }

    #[test]
    fn palette_scrim_backend_allocates_click_target() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0)),
            paints: Vec::new(),
            ..Default::default()
        };

        let response = palette_scrim_backend(&mut backend, Vec2::new(800.0, 600.0));

        assert_eq!(response.rect.width(), 800.0);
        assert_eq!(response.rect.height(), 600.0);
        assert!(backend.paints.is_empty());
    }

    #[test]
    fn palette_frame_chrome_backend_emits_shadow_fill_and_stroke_commands() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(120.0, 80.0));

        let commands = palette_frame_chrome_paint_cmds(
            rect,
            crate::vocab::CornerRadius::same(8),
            MaraColor32::from_rgb(1, 2, 3),
            Stroke::new(1.0, MaraColor32::WHITE),
        );

        assert!(matches!(commands[0], PaintCmd::Shadow { .. }));
        assert!(matches!(commands[1], PaintCmd::RectFilled { .. }));
        assert!(matches!(commands[2], PaintCmd::RectStroke { .. }));
    }

    #[test]
    fn palette_search_chrome_backend_emits_field_and_text_rect() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(240.0, 32.0)),
            paints: Vec::new(),
            ..Default::default()
        };

        let chrome = palette_search_chrome_backend(
            &mut backend,
            MaraColor32::from_rgb(20, 30, 40),
            MaraColor32::WHITE,
            24.0,
        );

        assert_eq!(chrome.rect.width(), 240.0);
        assert_eq!(chrome.rect.height(), 24.0);
        assert!(chrome.text_rect.min.x > chrome.rect.min.x);
        assert!(chrome.text_rect.max.x < chrome.rect.max.x);
        let [PaintCmd::RectFilled { .. }, PaintCmd::RectStroke { .. }] = backend.paints.as_slice()
        else {
            panic!("search chrome should emit fill and stroke commands");
        };
    }

    #[test]
    fn palette_corner_ticks_backend_emits_eight_lines() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(240.0, 120.0)),
            paints: Vec::new(),
            ..Default::default()
        };

        palette_corner_ticks_backend(
            &mut backend,
            Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(100.0, 80.0)),
            8.0,
            2.0,
            MaraColor32::WHITE,
        );

        assert_eq!(backend.paints.len(), 8);
        assert!(
            backend
                .paints
                .iter()
                .all(|cmd| matches!(cmd, PaintCmd::Line { .. }))
        );
    }

    #[test]
    fn palette_corner_ticks_backend_omits_when_disabled() {
        let mut backend = RecordingBackend::default();

        palette_corner_ticks_backend(
            &mut backend,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 80.0)),
            0.0,
            0.0,
            MaraColor32::WHITE,
        );

        assert!(backend.paints.is_empty());
    }

    #[test]
    fn palette_dash_separator_backend_emits_line_segments() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(20.0, 4.0)),
            paints: Vec::new(),
            ..Default::default()
        };

        let response = palette_dash_separator_backend(
            &mut backend,
            PaletteSeparatorSpec {
                dash_on: 4.0,
                dash_off: 2.0,
                color: MaraColor32::WHITE,
            },
        );

        assert_eq!(response.rect.width(), 20.0);
        assert!(backend.paints.len() > 1);
        assert!(
            backend
                .paints
                .iter()
                .all(|cmd| matches!(cmd, PaintCmd::Line { .. }))
        );
    }

    #[test]
    fn palette_dashed_line_backend_omits_invalid_dash() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(20.0, 4.0)),
            paints: Vec::new(),
            ..Default::default()
        };

        palette_dashed_line_backend(
            &mut backend,
            Pos2::new(0.0, 0.0),
            Pos2::new(20.0, 0.0),
            0.0,
            2.0,
            Stroke::new(1.0, MaraColor32::WHITE),
        );

        assert!(backend.paints.is_empty());
    }
}
