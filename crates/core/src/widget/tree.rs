//! Tree row — one row of a hierarchical list, rendered with
//! per-depth indent guides, a chevron for expandable nodes, an
//! optional type-icon slot, a truncating label, and a right-aligned
//! gutter of **uniform icon toggles** (eye, lock, …) that are
//! identical in count and kind across every row in the tree.
//!
//! Uniformity is structural: the `slots` slice defines how many
//! icons each row has, and callers pass the same shape to every
//! `tree_row` call in one list. There is no per-row escape hatch —
//! the Blender 4 / UE5 / Maya outliner pattern is "every row has
//! the same gutter controls", so the widget enforces it.
//!
//! Stateless by design — the caller owns the tree data **and** the
//! expansion state. You walk your model, call [`tree_row`] per node,
//! and recurse into its children when the row reports expanded.
//!
//! Shape:
//! ```text
//!   │  │  ▸  ▲ Robot                                 👁 🔒
//!   │  │  │  └── label (body click target, selects)  └─ uniform icon gutter
//!   │  │  └── chevron (expand click target, independent of body)
//!   │  └── type-icon slot (optional glyph painted in the accent colour)
//!   └── indent guides (depth × TREE_INDENT)
//! ```

use crate::memory::{MaraAnim, MaraMemory};
use std::hash::Hash;

use crate::{
    paint::{PaintCmd, TextFamily},
    style,
    vocab::{
        Align2 as MaraAlign2, Color32 as MaraColor32, CornerRadius, Id as MaraId, Pos2 as MaraPos2,
        Rect as MaraRect, Stroke as MaraStroke, Vec2 as MaraVec2,
    },
};

/// Row height — matches [`crate::widget::SELECT_ROW_H`] so trees and
/// outliner-style lists stack at the same rhythm.
pub const TREE_ROW_H: f32 = 20.0;

/// Horizontal pixels per depth level — indent guides are painted at
/// `depth * TREE_INDENT` from the row's left edge.
pub const TREE_INDENT: f32 = 12.0;

pub const TREE_CHEVRON_W: f32 = 12.0;
pub const TREE_ICON_W: f32 = 14.0;
pub const TREE_LABEL_PAD_L: f32 = 4.0;
pub const TREE_SLOT_W: f32 = 16.0;
pub const TREE_SLOT_GAP: f32 = 2.0;
pub const TREE_RIGHT_PAD_R: f32 = 4.0;
pub const TREE_ROW_PAD_L: f32 = 4.0;
/// Height of a hierarchy row with title + metadata + embedded action.
pub const TREE_ACTION_ROW_H: f32 = 2.0 * crate::style::UNIT;
/// Size of the embedded action target in hierarchy rows.
pub const TREE_ACTION_W: f32 = 28.0;
/// Gap between hierarchy row text and the embedded action target.
pub const TREE_ACTION_GAP: f32 = 6.0;

/// Which built-in icon to paint in a [`TreeIconSlot`]. Built-ins are
/// drawn with painter shapes so they work identically regardless of
/// which font subset is installed. Use [`TreeIconKind::Glyph`] as an
/// escape hatch for anything not covered by the named variants.
#[derive(Debug, Clone, Copy)]
pub enum TreeIconKind {
    /// Eye icon — almond outline + pupil when active (visible),
    /// outline + diagonal slash when inactive (hidden).
    Eye,
    /// Padlock — body + closed shackle when active (locked), body +
    /// tilted open shackle when inactive (unlocked).
    Lock,
    /// Custom pair of font glyphs. Painted in the current text font
    /// at `12 px`. Use for icons you don't want to hand-paint.
    Glyph { on: &'static str, off: &'static str },
    /// Read-only colour swatch — a filled rounded square in the
    /// given colour, with the standard mara border stroke. The
    /// slot's `state: &mut bool` is ignored for this variant
    /// (still required by the slice shape — pass any `&mut bool`);
    /// the icon response is returned in [`TreeRowResponse::icons`]
    /// so callers can act on clicks (e.g. "select the material").
    Color(MaraColor32),
}

/// One slot in the right-gutter of a [`tree_row`].
pub struct TreeIconSlot<'a> {
    pub kind: TreeIconKind,
    pub state: &'a mut bool,
    pub tooltip: Option<&'static str>,
}

impl<'a> TreeIconSlot<'a> {
    pub fn new(kind: TreeIconKind, state: &'a mut bool) -> Self {
        Self {
            kind,
            state,
            tooltip: None,
        }
    }

    pub fn with_tooltip(mut self, text: &'static str) -> Self {
        self.tooltip = Some(text);
        self
    }
}

/// The click targets produced by [`tree_row`]. Inspect `body` for
/// select / double-click / drag, `chevron` (when `Some`) for
/// expand-toggle, and `icons[i]` for the `i`-th right-gutter slot.
/// Leaves get `chevron == None` and reserve the chevron column as
/// blank space so labels align with branches.
#[derive(Debug)]
pub struct TreeRowResponse {
    /// Click target covering the label + type-icon area.
    pub body: crate::mui::MaraResponse,
    /// Click target for the chevron glyph only. `None` for leaves.
    pub chevron: Option<crate::mui::MaraResponse>,
    /// One response per entry in the `slots` slice. The widget has
    /// already toggled each slot's `state` for you on click; this
    /// response is for additional hooks.
    pub icons: Vec<crate::mui::MaraResponse>,
    /// `true` when the chevron was clicked with the shift modifier
    /// held — caller's "recursively expand subtree" affordance.
    pub chevron_shift_clicked: bool,
}

/// The click targets produced by [`tree_action_row`]. This is the
/// hierarchy/list-row variant: body select/double-click, optional
/// chevron, and an independent embedded tail action (usually `+`).
#[derive(Debug)]
pub struct TreeActionRowResponse {
    pub body: crate::mui::MaraResponse,
    pub chevron: Option<crate::mui::MaraResponse>,
    pub action: crate::mui::MaraResponse,
    pub chevron_shift_clicked: bool,
}

/// Directory-tree connector state for [`tree_action_row_with_guide`].
///
/// This mirrors Coreviz's `<li class="tree-node">` CSS:
/// every non-root node gets a horizontal branch into the row, while
/// `:last-child` cuts the vertical at the row joint (`└`) and a
/// non-last child keeps the vertical running through (`├`).
///
/// `ancestor_continues` is one flag per ancestor column before this
/// node's parent. `true` paints a continuing `│` line for that
/// ancestor, so deep trees can render directory-style guides:
/// `│   │   ├── leaf`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeBranchGuide {
    pub ancestor_continues: Vec<bool>,
    pub is_last: bool,
}

impl TreeBranchGuide {
    pub fn new(ancestor_continues: impl Into<Vec<bool>>, is_last: bool) -> Self {
        Self {
            ancestor_continues: ancestor_continues.into(),
            is_last,
        }
    }

    pub fn tee(ancestor_continues: impl Into<Vec<bool>>) -> Self {
        Self::new(ancestor_continues, false)
    }

    pub fn last(ancestor_continues: impl Into<Vec<bool>>) -> Self {
        Self::new(ancestor_continues, true)
    }
}

/// Paint one row of a tree.
///
/// `depth` is the node's nesting level (0 = root); `expanded` is
/// `Some(&mut bool)` for branches and `None` for leaves. `icon` is
/// an optional type-indicator (Fluent icon NAME, falls back to
/// literal text rendering when not bundled). `slots` is the
/// fixed-width right gutter of action toggles — pass the same slice
/// shape for every row in the tree.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tree_row<B: crate::layout::UiBackend + ?Sized>(
    backend: &mut B,
    id_salt: impl Hash + Copy,
    depth: u32,
    expanded: Option<&mut bool>,
    icon: Option<&str>,
    label: &str,
    selected: bool,
    accent: impl Into<MaraColor32> + Copy,
    slots: &mut [TreeIconSlot<'_>],
) -> TreeRowResponse {
    let accent_mara = accent.into();
    let w = backend.available_width();
    // Reserve z-slots for the row background fill + indent guides
    // BEFORE inline widgets draw.
    let bg_slot = backend.reserve_paint_slot();

    let row = compute_row_rects(
        backend,
        w,
        depth,
        expanded.is_some(),
        icon.is_some(),
        slots.len(),
    );
    let rect = row.rect;
    let body_rect = row.body_rect;
    let chevron_rect_opt = row.chevron_rect;
    let icon_rect_opt = row.icon_rect;

    let ui_id = backend.id();
    let body_id = ui_id.with(("mara_tree_body", id_salt));
    let chevron_id = ui_id.with(("mara_tree_chevron", id_salt));
    let slot_ids: Vec<MaraId> = (0..slots.len())
        .map(|i| ui_id.with(("mara_tree_slot", id_salt, i)))
        .collect();
    let (body, chevron, icon_responses) = {
        let body = backend.interact(body_rect, body_id, crate::layout::Sense::Click);
        let chevron = chevron_rect_opt
            .map(|cr| backend.interact(cr, chevron_id, crate::layout::Sense::Click));
        let mut icon_responses = Vec::with_capacity(slots.len());
        for (i, slot_rect) in row.slot_rects.iter().enumerate() {
            icon_responses.push(backend.interact(
                *slot_rect,
                slot_ids[i],
                crate::layout::Sense::Click,
            ));
        }
        (body, chevron, icon_responses)
    };
    for (i, response) in icon_responses.iter().enumerate() {
        if let Some(tip) = slots[i].tooltip {
            backend.hover_text(response, tip);
        }
    }

    // Background fill — paints under the inline widgets via the
    // reserved slot.
    let any_slot_hovered = icon_responses.iter().any(|r| r.hovered());
    let hovered =
        body.hovered() || chevron.as_ref().is_some_and(|c| c.hovered()) || any_slot_hovered;
    let bg_cmd = if selected {
        Some(PaintCmd::RectFilled {
            rect,
            corner: style::radius_for(style::RadiusRole::Compact),
            fill: style::row_selected_fill(accent_mara),
        })
    } else if hovered {
        Some(PaintCmd::RectFilled {
            rect,
            corner: style::radius_for(style::RadiusRole::Compact),
            fill: style::row_hover_fill(accent_mara),
        })
    } else {
        None
    };
    backend.fill_paint_slot(bg_slot, bg_cmd);

    // Indent guides — faint vertical lines at each ancestor depth.
    let guide_base = style::theme().border_subtle;
    let guide_color =
        MaraColor32::from_rgba_unmultiplied(guide_base.r(), guide_base.g(), guide_base.b(), 90);
    paint_all(
        backend,
        tree_indent_guide_paint_cmds(rect, style::theme().widgets.tree, depth, guide_color),
    );

    // Chevron glyph — animated rotating triangle.
    let glyph_col = style::section_title_color(accent_mara);
    let mut chevron_shift_clicked = false;
    if let (Some(exp), Some(cr)) = (expanded, chevron_rect_opt) {
        let how_open = backend
            .memory()
            .animate_bool_responsive(ui_id.with(("mara_tree_chev_anim", id_salt)), *exp);
        paint_chevron(backend, cr, how_open, glyph_col);
        if let Some(ref cresp) = chevron
            && cresp.clicked()
        {
            let shift_held = backend.input().modifiers_shift;
            if shift_held {
                chevron_shift_clicked = true;
            } else {
                *exp = !*exp;
            }
        }
    }

    // Type-icon slot — Fluent name lookup with literal-text fallback.
    if let (Some(name), Some(ir)) = (icon, icon_rect_opt) {
        let size = if crate::icons::icon_glyph(name).is_some() {
            style::theme().icons.tree_type_icon_size
        } else {
            style::theme().icons.tree_glyph_icon_size
        };
        paint_icon_or_glyph(
            backend,
            ir.center(),
            MaraAlign2::CENTER_CENTER,
            name,
            size,
            glyph_col,
        );
    }

    // Label — truncated to the body rect minus its left padding.
    if backend.is_rect_visible(rect) {
        let label_left = body_rect.min.x
            + style::theme().widgets.tree.row_pad_l
            + depth as f32 * style::theme().widgets.tree.indent
            + style::theme().widgets.tree.chevron_w
            + if icon.is_some() {
                style::theme().widgets.tree.icon_w
            } else {
                0.0
            }
            + style::theme().widgets.tree.label_pad_l;
        let label_rect = MaraRect::from_min_max(
            MaraPos2::new(label_left, rect.min.y),
            MaraPos2::new(body_rect.max.x, rect.max.y),
        );
        backend.paint(clipped_text_paint_cmd(
            label_rect,
            label_rect.left_center(),
            MaraAlign2::LEFT_CENTER,
            label,
            style::theme().widgets.tree.label_font,
            style::on_section(),
        ));

        // Right-gutter slots — paint each icon in its reserved square.
        for (i, slot) in slots.iter_mut().enumerate() {
            let rect = row.slot_rects[i];
            let resp = &icon_responses[i];
            paint_slot_icon(
                backend,
                rect,
                &slot.kind,
                *slot.state,
                resp.hovered(),
                accent_mara,
            );
        }
    }

    // Flip slot states after painting (one-frame lag avoids tri-state
    // flicker). `Color` slots are read-only.
    for (i, slot) in slots.iter_mut().enumerate() {
        if icon_responses[i].clicked() && !matches!(slot.kind, TreeIconKind::Color(_)) {
            *slot.state = !*slot.state;
        }
    }

    TreeRowResponse {
        body,
        chevron,
        icons: icon_responses,
        chevron_shift_clicked,
    }
}

/// Paint one hierarchy row with two text layers and an embedded
/// independent action button at the far end. This mirrors the
/// Coreviz zone-list shape: clicking the row selects/focuses the
/// item; clicking the `+` arms child creation without also selecting
/// the row.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tree_action_row<B: crate::layout::UiBackend + ?Sized>(
    backend: &mut B,
    id_salt: impl Hash + Copy,
    depth: u32,
    expanded: Option<&mut bool>,
    icon: Option<&str>,
    title: &str,
    meta: &str,
    selected: bool,
    action_glyph: &str,
    action_tooltip: Option<&str>,
    action_armed: bool,
    accent: impl Into<MaraColor32> + Copy,
) -> TreeActionRowResponse {
    tree_action_row_with_guide(
        backend,
        id_salt,
        depth,
        expanded,
        icon,
        title,
        meta,
        selected,
        action_glyph,
        action_tooltip,
        action_armed,
        None,
        accent,
    )
}

/// Paint one hierarchy action row with explicit directory-tree
/// connector state. Use this when the caller knows sibling position
/// and ancestor continuation, so the row can render `├`, `└`, and
/// deep `│` columns exactly like Coreviz's tree CSS.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tree_action_row_with_guide<B: crate::layout::UiBackend + ?Sized>(
    backend: &mut B,
    id_salt: impl Hash + Copy,
    depth: u32,
    expanded: Option<&mut bool>,
    icon: Option<&str>,
    title: &str,
    meta: &str,
    selected: bool,
    action_glyph: &str,
    action_tooltip: Option<&str>,
    action_armed: bool,
    branch: Option<&TreeBranchGuide>,
    accent: impl Into<MaraColor32> + Copy,
) -> TreeActionRowResponse {
    let accent_mara = accent.into();
    let tree = style::theme().widgets.tree;
    let w = backend.available_width();
    let bg_slot = backend.reserve_paint_slot();

    let row = compute_action_row_rects(backend, w, depth, expanded.is_some(), icon.is_some());
    let rect = row.rect;
    let button_rect = row.button_rect;
    let body_rect = row.body_rect;
    let chevron_rect = row.chevron_rect;
    let icon_rect = row.icon_rect;
    let action_rect = row.action_rect;
    let action_size = row.action_size;
    let label_rect = row.label_rect;

    let ui_id = backend.id();
    let body_id = ui_id.with(("mara_tree_action_body", id_salt));
    let chevron_id = ui_id.with(("mara_tree_action_chevron", id_salt));
    let action_id = ui_id.with(("mara_tree_action_tail", id_salt));
    let (body, chevron, action) = {
        let body = backend.interact(body_rect, body_id, crate::layout::Sense::Click);
        let chevron =
            chevron_rect.map(|cr| backend.interact(cr, chevron_id, crate::layout::Sense::Click));
        let action = backend.interact(action_rect, action_id, crate::layout::Sense::Click);
        (body, chevron, action)
    };
    if let Some(tip) = action_tooltip {
        backend.hover_text(&action, tip);
    }

    let hovered =
        body.hovered() || action.hovered() || chevron.as_ref().is_some_and(|c| c.hovered());
    let pressed = body.pointer_button_down() || action.pointer_button_down();
    let active = hovered || pressed;
    let theme = style::theme();
    let button = theme.widgets.button;
    let hover_t = if theme.animations_enabled {
        backend.memory().animate_bool(
            ui_id.with(("mara_tree_action_button_hover", id_salt)),
            active,
            0.25 * theme.button_anim_scale.max(0.01),
        )
    } else if active {
        1.0
    } else {
        0.0
    };
    let radius = style::radius_for(style::RadiusRole::Widget);
    let body_acc = style::body_accent(accent_mara);
    let base = if style::section_show_frame() {
        style::section_fill(accent_mara)
    } else {
        style::pane_fill(accent_mara)
    };
    let target = style::surface_lift_target(body_acc);
    let rest_bg = with_alpha(
        lerp_color(base, target, button.tint_rest),
        style::glass_alpha_card(),
    );
    let target_bg = if button.full_accent_on_press {
        with_alpha(body_acc, 255)
    } else {
        with_alpha(
            lerp_color(base, target, button.tint_press),
            style::glass_alpha_card(),
        )
    };
    let selected_bg = with_alpha(lerp_color(base, body_acc, 0.34), style::glass_alpha_card());
    let bg = if selected {
        lerp_color(rest_bg, selected_bg, 0.90)
    } else {
        lerp_color(rest_bg, target_bg, hover_t)
    };
    backend.fill_paint_slot(
        bg_slot,
        Some(PaintCmd::RectFilled {
            rect: button_rect,
            corner: radius,
            fill: bg,
        }),
    );
    backend.paint(PaintCmd::RectStroke {
        rect: button_rect,
        corner: radius,
        stroke: MaraStroke::new(
            theme.border_width,
            lerp_color_opaque(style::widget_border(accent_mara), accent_mara, hover_t),
        ),
    });

    let guide_base = style::theme().border_subtle;
    let guide_color =
        MaraColor32::from_rgba_unmultiplied(guide_base.r(), guide_base.g(), guide_base.b(), 90);
    for cmd in
        tree_action_guide_paint_cmds(rect, tree, depth, button_rect.min.x, branch, guide_color)
    {
        backend.paint(cmd);
    }

    let glyph_col = style::section_title_color(accent_mara);
    let mut chevron_shift_clicked = false;
    if let (Some(exp), Some(cr)) = (expanded, chevron_rect) {
        let how_open = backend
            .memory()
            .animate_bool_responsive(ui_id.with(("mara_tree_action_chev_anim", id_salt)), *exp);
        paint_chevron(backend, cr, how_open, glyph_col);
        if let Some(ref cresp) = chevron
            && cresp.clicked()
        {
            let shift_held = backend.input().modifiers_shift;
            if shift_held {
                chevron_shift_clicked = true;
            } else {
                *exp = !*exp;
            }
        }
    }

    if let (Some(name), Some(ir)) = (icon, icon_rect) {
        paint_icon_or_glyph(
            backend,
            ir.center(),
            MaraAlign2::CENTER_CENTER,
            name,
            style::theme().icons.tree_type_icon_size,
            glyph_col,
        );
    }

    paint_two_line_label(backend, label_rect, title, meta);

    let action_hover_t = if style::theme().animations_enabled {
        backend.memory().animate_bool(
            ui_id.with(("mara_tree_action_tail_hover", id_salt)),
            action.hovered() || action.pointer_button_down() || action_armed,
            0.18 * style::theme().button_anim_scale.max(0.01),
        )
    } else if action.hovered() || action.pointer_button_down() || action_armed {
        1.0
    } else {
        0.0
    };
    let action_fill = lerp_color(
        style::surface_lift_target(style::body_accent(accent_mara)),
        accent_mara,
        if action_armed {
            0.30
        } else {
            0.18 * action_hover_t
        },
    );
    let action_fill =
        MaraColor32::from_rgba_unmultiplied(action_fill.r(), action_fill.g(), action_fill.b(), 80);
    let action_radius = CornerRadius::same((action_size * 0.5).round() as u8);
    backend.paint(PaintCmd::RectFilled {
        rect: action_rect,
        corner: action_radius,
        fill: action_fill,
    });
    backend.paint(PaintCmd::RectStroke {
        rect: action_rect,
        corner: action_radius,
        stroke: MaraStroke::new(
            style::theme().stroke.border_width,
            lerp_color_opaque(
                style::widget_border(accent_mara),
                accent_mara,
                action_hover_t.max(if action_armed { 0.75 } else { 0.0 }),
            ),
        ),
    });
    paint_icon_or_glyph(
        backend,
        action_rect.center(),
        MaraAlign2::CENTER_CENTER,
        action_glyph,
        16.0,
        accent_mara,
    );

    TreeActionRowResponse {
        body,
        chevron,
        action,
        chevron_shift_clicked,
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TreeRowGeometry {
    rect: MaraRect,
    body_rect: MaraRect,
    chevron_rect: Option<MaraRect>,
    icon_rect: Option<MaraRect>,
    slot_rects: Vec<MaraRect>,
}

#[derive(Clone, Debug, PartialEq)]
struct TreeActionRowGeometry {
    rect: MaraRect,
    button_rect: MaraRect,
    body_rect: MaraRect,
    chevron_rect: Option<MaraRect>,
    icon_rect: Option<MaraRect>,
    action_rect: MaraRect,
    label_rect: MaraRect,
    action_size: f32,
}

fn tree_row_size(width: f32, tree: style::TreeTheme) -> MaraVec2 {
    MaraVec2::new(width, tree.row_h)
}

fn tree_action_row_size(width: f32) -> MaraVec2 {
    MaraVec2::new(width, TREE_ACTION_ROW_H)
}

/// Paint every command in `cmds` in order through the backend.
fn paint_all<B: crate::layout::UiBackend + ?Sized>(
    backend: &mut B,
    cmds: impl IntoIterator<Item = PaintCmd>,
) {
    for cmd in cmds {
        backend.paint(cmd);
    }
}

fn compute_row_rects<B: crate::layout::UiBackend + ?Sized>(
    backend: &mut B,
    w: f32,
    depth: u32,
    has_chevron: bool,
    has_icon: bool,
    slot_count: usize,
) -> TreeRowGeometry {
    let tree = style::theme().widgets.tree;
    let rect = { backend.reserve_space(tree_row_size(w, tree)) };
    tree_row_geometry(rect, tree, depth, has_chevron, has_icon, slot_count)
}

fn compute_action_row_rects<B: crate::layout::UiBackend + ?Sized>(
    backend: &mut B,
    w: f32,
    depth: u32,
    has_chevron: bool,
    has_icon: bool,
) -> TreeActionRowGeometry {
    let tree = style::theme().widgets.tree;
    let rect = { backend.reserve_space(tree_action_row_size(w)) };
    tree_action_row_geometry(rect, tree, depth, has_chevron, has_icon)
}

fn tree_row_geometry(
    rect: MaraRect,
    tree: style::TreeTheme,
    depth: u32,
    has_chevron: bool,
    has_icon: bool,
    slot_count: usize,
) -> TreeRowGeometry {
    let left_start = rect.min.x + tree.row_pad_l + depth as f32 * tree.indent;
    let chevron_rect = if has_chevron {
        Some(MaraRect::from_min_size(
            MaraPos2::new(left_start, rect.min.y),
            MaraVec2::new(tree.chevron_w, rect.height()),
        ))
    } else {
        None
    };
    let icon_rect = if has_icon {
        Some(MaraRect::from_min_size(
            MaraPos2::new(left_start + tree.chevron_w, rect.min.y),
            MaraVec2::new(tree.icon_w, rect.height()),
        ))
    } else {
        None
    };

    let gutter_w = if slot_count == 0 {
        0.0
    } else {
        slot_count as f32 * tree.slot_w
            + (slot_count as f32 - 1.0) * tree.slot_gap
            + tree.right_pad_r
    };
    let mut slot_rects = Vec::with_capacity(slot_count);
    if slot_count > 0 {
        let mut x_max = rect.max.x - tree.right_pad_r;
        for _ in 0..slot_count {
            let x_min = x_max - tree.slot_w;
            slot_rects.push(MaraRect::from_min_max(
                MaraPos2::new(x_min, rect.min.y),
                MaraPos2::new(x_max, rect.max.y),
            ));
            x_max = x_min - tree.slot_gap;
        }
        slot_rects.reverse();
    }

    let body_rect =
        MaraRect::from_min_max(rect.min, MaraPos2::new(rect.max.x - gutter_w, rect.max.y));
    TreeRowGeometry {
        rect,
        body_rect,
        chevron_rect,
        icon_rect,
        slot_rects,
    }
}

fn tree_action_row_geometry(
    rect: MaraRect,
    tree: style::TreeTheme,
    depth: u32,
    has_chevron: bool,
    has_icon: bool,
) -> TreeActionRowGeometry {
    let button_rect = shrink_rect_xy(
        MaraRect::from_min_max(
            MaraPos2::new(rect.min.x + depth as f32 * tree.indent, rect.min.y),
            rect.max,
        ),
        0.0,
        1.0,
    );
    let left_start = button_rect.min.x + tree.row_pad_l;
    let chevron_rect = if has_chevron {
        Some(MaraRect::from_min_size(
            MaraPos2::new(left_start, rect.min.y),
            MaraVec2::new(tree.chevron_w, rect.height()),
        ))
    } else {
        None
    };
    let icon_rect = if has_icon {
        Some(MaraRect::from_min_size(
            MaraPos2::new(left_start + tree.chevron_w, rect.min.y),
            MaraVec2::new(tree.icon_w, rect.height()),
        ))
    } else {
        None
    };
    let action_size = TREE_ACTION_W.min((rect.height() - 10.0).max(18.0));
    let action_rect = MaraRect::from_center_size(
        MaraPos2::new(
            button_rect.max.x - tree.right_pad_r - action_size * 0.5,
            button_rect.center().y,
        ),
        MaraVec2::new(action_size, action_size),
    );
    let body_rect = MaraRect::from_min_max(
        button_rect.min,
        MaraPos2::new(
            (action_rect.min.x - TREE_ACTION_GAP).max(button_rect.min.x),
            button_rect.max.y,
        ),
    );
    let label_left =
        left_start + tree.chevron_w + if has_icon { tree.icon_w } else { 0.0 } + tree.label_pad_l;
    let label_rect = MaraRect::from_min_max(
        MaraPos2::new(label_left, rect.min.y + 6.0),
        MaraPos2::new(body_rect.max.x, rect.max.y - 6.0),
    );

    TreeActionRowGeometry {
        rect,
        button_rect,
        body_rect,
        chevron_rect,
        icon_rect,
        action_rect,
        label_rect,
        action_size,
    }
}

fn tree_indent_guide_paint_cmds(
    rect: MaraRect,
    tree: style::TreeTheme,
    depth: u32,
    color: MaraColor32,
) -> Vec<PaintCmd> {
    if tree.guide_width <= 0.0 {
        return Vec::new();
    }

    let stroke = MaraStroke::new(tree.guide_width, color);
    (0..depth)
        .map(|d| {
            let x = rect.min.x + tree.row_pad_l + d as f32 * tree.indent + tree.chevron_w * 0.5;
            PaintCmd::Line {
                a: MaraPos2::new(x, rect.min.y),
                b: MaraPos2::new(x, rect.max.y),
                stroke,
            }
        })
        .collect()
}

fn tree_action_guide_paint_cmds(
    rect: MaraRect,
    tree: style::TreeTheme,
    depth: u32,
    button_left: f32,
    branch: Option<&TreeBranchGuide>,
    color: MaraColor32,
) -> Vec<PaintCmd> {
    let stroke = MaraStroke::new(tree.guide_width, color);
    tree_action_guide_segments(rect, tree, depth, button_left, branch)
        .into_iter()
        .map(|[a, b]| PaintCmd::Line { a, b, stroke })
        .collect()
}

fn tree_action_guide_segments(
    rect: MaraRect,
    tree: style::TreeTheme,
    depth: u32,
    button_left: f32,
    branch: Option<&TreeBranchGuide>,
) -> Vec<[MaraPos2; 2]> {
    if depth == 0 || tree.guide_width <= 0.0 {
        return Vec::new();
    }

    let joint_y = rect.center().y;
    let column_x = |level: u32| rect.min.x + tree.row_pad_l + level as f32 * tree.indent;
    let mut segments = Vec::with_capacity(depth as usize + 1);

    if let Some(branch) = branch {
        for level in 0..depth.saturating_sub(1) {
            if branch
                .ancestor_continues
                .get(level as usize)
                .copied()
                .unwrap_or(false)
            {
                let x = column_x(level);
                segments.push([MaraPos2::new(x, rect.min.y), MaraPos2::new(x, rect.max.y)]);
            }
        }

        let x = column_x(depth - 1);
        let vertical_bottom = if branch.is_last { joint_y } else { rect.max.y };
        segments.push([
            MaraPos2::new(x, rect.min.y),
            MaraPos2::new(x, vertical_bottom),
        ]);
        segments.push([
            MaraPos2::new(x, joint_y),
            MaraPos2::new(button_left, joint_y),
        ]);
    } else {
        for level in 0..depth {
            let x = column_x(level);
            segments.push([MaraPos2::new(x, rect.min.y), MaraPos2::new(x, rect.max.y)]);
        }

        let x = column_x(depth - 1);
        segments.push([
            MaraPos2::new(x, joint_y),
            MaraPos2::new(button_left, joint_y),
        ]);
    }

    segments
}

/// Thin stroked chevron (`›` rotating to `⌄`) inside `rect`.
fn paint_chevron<B: crate::layout::UiBackend + ?Sized>(
    backend: &mut B,
    rect: MaraRect,
    how_open: f32,
    color: MaraColor32,
) {
    backend.paint(chevron_paint_cmd(rect, how_open, color));
}

fn chevron_paint_cmd(rect: MaraRect, how_open: f32, color: MaraColor32) -> PaintCmd {
    const GLYPH_W: f32 = 5.5;
    const GLYPH_H: f32 = 3.5;
    let cx = rect.center().x;
    let cy = rect.center().y;
    let hw = GLYPH_W * 0.5;
    let hh = GLYPH_H * 0.5;
    let raw = [(-hw, -hh), (0.0, hh), (hw, -hh)];
    use std::f32::consts::TAU;
    let angle = -TAU / 4.0 + (TAU / 4.0) * how_open.clamp(0.0, 1.0);
    let (sin, cos) = angle.sin_cos();
    let points: Vec<MaraPos2> = raw
        .iter()
        .map(|&(x, y)| {
            let rx = x * cos - y * sin;
            let ry = x * sin + y * cos;
            MaraPos2::new(cx + rx, cy + ry)
        })
        .collect();
    PaintCmd::Polyline {
        points,
        stroke: MaraStroke::new(1.2, color),
    }
}

fn paint_slot_icon<B: crate::layout::UiBackend + ?Sized>(
    backend: &mut B,
    rect: MaraRect,
    kind: &TreeIconKind,
    active: bool,
    hovered: bool,
    accent: MaraColor32,
) {
    paint_all(
        backend,
        slot_icon_paint_cmds(rect, kind, active, hovered, accent),
    );
}

fn paint_icon_or_glyph<B: crate::layout::UiBackend + ?Sized>(
    backend: &mut B,
    pos: MaraPos2,
    anchor: MaraAlign2,
    name: &str,
    size: f32,
    color: MaraColor32,
) {
    if crate::icons::icon_glyph(name).is_some() && !crate::icons::icon_fonts_ready() {
        return;
    }
    backend.paint(icon_or_glyph_paint_cmd(pos, anchor, name, size, color));
}

fn icon_or_glyph_paint_cmd(
    pos: MaraPos2,
    anchor: MaraAlign2,
    name: &str,
    size: f32,
    color: MaraColor32,
) -> PaintCmd {
    if let Some((glyph, family)) = crate::icons::icon_glyph(name) {
        PaintCmd::TextWithFamily {
            pos,
            anchor,
            text: glyph.to_string(),
            size,
            color,
            family: TextFamily::Named(family),
        }
    } else {
        PaintCmd::Text {
            pos,
            anchor,
            text: name.to_owned(),
            size,
            color,
            mono: false,
        }
    }
}

fn paint_two_line_label<B: crate::layout::UiBackend + ?Sized>(
    backend: &mut B,
    rect: MaraRect,
    title: &str,
    meta: &str,
) {
    backend.paint(two_line_label_paint_cmd(rect, title, meta));
}

fn clipped_text_paint_cmd(
    rect: MaraRect,
    pos: MaraPos2,
    anchor: MaraAlign2,
    text: &str,
    size: f32,
    color: MaraColor32,
) -> PaintCmd {
    PaintCmd::Clip {
        rect,
        children: vec![PaintCmd::Text {
            pos,
            anchor,
            text: text.to_owned(),
            size,
            color,
            mono: false,
        }],
    }
}

fn two_line_label_paint_cmd(rect: MaraRect, title: &str, meta: &str) -> PaintCmd {
    PaintCmd::Clip {
        rect,
        children: vec![
            PaintCmd::Text {
                pos: MaraPos2::new(rect.min.x, rect.center().y - 8.0),
                anchor: MaraAlign2::LEFT_CENTER,
                text: title.to_owned(),
                size: style::theme().widgets.tree.label_font + 1.0,
                color: style::on_section(),
                mono: false,
            },
            PaintCmd::Text {
                pos: MaraPos2::new(rect.min.x, rect.center().y + 8.0),
                anchor: MaraAlign2::LEFT_CENTER,
                text: meta.to_owned(),
                size: (style::theme().widgets.tree.label_font - 1.0).max(9.0),
                color: style::on_section_dim(),
                mono: false,
            },
        ],
    }
}

fn slot_icon_paint_cmds(
    rect: MaraRect,
    kind: &TreeIconKind,
    active: bool,
    hovered: bool,
    accent: MaraColor32,
) -> Vec<PaintCmd> {
    let color = slot_color(active, hovered, accent);
    match *kind {
        TreeIconKind::Eye => eye_paint_cmds(rect, active, color),
        TreeIconKind::Lock => lock_paint_cmds(rect, active, color),
        TreeIconKind::Glyph { on, off } => vec![PaintCmd::Text {
            pos: rect.center(),
            anchor: MaraAlign2::CENTER_CENTER,
            text: (if active { on } else { off }).to_owned(),
            size: style::theme().icons.tree_glyph_icon_size,
            color,
            mono: false,
        }],
        TreeIconKind::Color(fill) => color_chip_paint_cmds(rect, fill, accent, hovered),
    }
}

fn color_chip_paint_cmds(
    rect: MaraRect,
    fill: MaraColor32,
    accent: MaraColor32,
    hovered: bool,
) -> Vec<PaintCmd> {
    let inner = shrink_rect(rect, 3.0);
    let border = if hovered {
        MaraStroke::new(style::theme().stroke.border_width, accent)
    } else {
        style::stroke_for(style::StrokeRole::WidgetBorder, accent)
    };
    let corner: CornerRadius = style::radius_for(style::RadiusRole::Compact);
    vec![
        PaintCmd::RectFilled {
            rect: inner,
            corner,
            fill,
        },
        PaintCmd::RectStroke {
            rect: inner,
            corner,
            stroke: border,
        },
    ]
}

fn slot_color(active: bool, hovered: bool, accent: MaraColor32) -> MaraColor32 {
    match (active, hovered) {
        (true, true) => accent,
        (true, false) => style::on_section(),
        (false, true) => lerp_color(style::on_section_dim(), accent, 0.4),
        (false, false) => style::on_section_dim(),
    }
}

/// Eye — almond outline + pupil active, slash when off.
fn eye_paint_cmds(rect: MaraRect, active: bool, color: MaraColor32) -> Vec<PaintCmd> {
    let c = rect.center();
    let rx = 5.5_f32;
    let ry = 3.2_f32;
    let stroke = MaraStroke::new(1.1, color);
    let mut commands = Vec::with_capacity(3);

    let lid = |sign: f32| {
        let mut pts = Vec::with_capacity(11);
        for i in 0..=10 {
            let t = i as f32 / 10.0;
            let x = c.x + (t - 0.5) * 2.0 * rx;
            let y = c.y + sign * ry * (1.0 - ((x - c.x) / rx).powi(2));
            pts.push(MaraPos2::new(x, y));
        }
        PaintCmd::Polyline {
            points: pts,
            stroke,
        }
    };
    commands.push(lid(1.0));
    commands.push(lid(-1.0));

    if active {
        commands.push(PaintCmd::CircleFilled {
            center: c,
            radius: 1.6,
            fill: color,
        });
    } else {
        commands.push(PaintCmd::Line {
            a: MaraPos2::new(c.x - rx - 0.5, c.y + ry + 0.5),
            b: MaraPos2::new(c.x + rx + 0.5, c.y - ry - 0.5),
            stroke: MaraStroke::new(1.3, color),
        });
    }

    commands
}

/// Padlock — body + shackle, lifted leg on unlocked.
fn lock_paint_cmds(rect: MaraRect, active: bool, color: MaraColor32) -> Vec<PaintCmd> {
    let c = rect.center();
    let body_w = 7.0_f32;
    let body_h = 5.5_f32;
    let body_top_y = c.y + 0.2;
    let body_rect = MaraRect::from_min_size(
        MaraPos2::new(c.x - body_w * 0.5, body_top_y),
        crate::vocab::Vec2::new(body_w, body_h),
    );
    let mut commands = Vec::with_capacity(6);
    commands.push(PaintCmd::RectFilled {
        rect: body_rect,
        corner: style::radius_for(style::RadiusRole::Compact),
        fill: color,
    });

    let stroke = MaraStroke::new(1.1, color);
    let shackle_top_y = body_top_y - 4.0;
    let legs_x_l = c.x - 2.4;
    let legs_x_r = c.x + 2.4;

    let mut arc = Vec::with_capacity(7);
    for i in 0..=6 {
        let t = i as f32 / 6.0;
        let theta = std::f32::consts::PI - t * std::f32::consts::PI;
        let x = c.x + theta.cos() * 2.4;
        let y = shackle_top_y - theta.sin() * 1.6;
        arc.push(MaraPos2::new(x, y));
    }
    commands.push(PaintCmd::Polyline {
        points: arc,
        stroke,
    });

    commands.push(PaintCmd::Line {
        a: MaraPos2::new(legs_x_l, shackle_top_y),
        b: MaraPos2::new(legs_x_l, body_top_y + 0.3),
        stroke,
    });
    if active {
        commands.push(PaintCmd::Line {
            a: MaraPos2::new(legs_x_r, shackle_top_y),
            b: MaraPos2::new(legs_x_r, body_top_y + 0.3),
            stroke,
        });
    } else {
        commands.push(PaintCmd::Line {
            a: MaraPos2::new(legs_x_r, shackle_top_y),
            b: MaraPos2::new(legs_x_r, shackle_top_y + 2.0),
            stroke,
        });
    }

    if active {
        commands.push(PaintCmd::CircleFilled {
            center: MaraPos2::new(c.x, body_rect.center().y),
            radius: 0.7,
            fill: MaraColor32::from_rgba_unmultiplied(0, 0, 0, 140),
        });
    }

    commands
}

fn shrink_rect(rect: MaraRect, amount: f32) -> MaraRect {
    MaraRect::from_min_max(
        MaraPos2::new(rect.min.x + amount, rect.min.y + amount),
        MaraPos2::new(rect.max.x - amount, rect.max.y - amount),
    )
}

fn shrink_rect_xy(rect: MaraRect, x: f32, y: f32) -> MaraRect {
    MaraRect::from_min_max(
        MaraPos2::new(rect.min.x + x, rect.min.y + y),
        MaraPos2::new(rect.max.x - x, rect.max.y - y),
    )
}

fn lerp_color(a: MaraColor32, b: MaraColor32, t: f32) -> MaraColor32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| ((x as f32) * (1.0 - t) + (y as f32) * t).round() as u8;
    MaraColor32::from_rgba_premultiplied(
        mix(a.r(), b.r()),
        mix(a.g(), b.g()),
        mix(a.b(), b.b()),
        mix(a.a(), b.a()),
    )
}

fn lerp_color_opaque(a: MaraColor32, b: MaraColor32, t: f32) -> MaraColor32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| ((x as f32) * (1.0 - t) + (y as f32) * t).round() as u8;
    MaraColor32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

fn with_alpha(solid: MaraColor32, alpha: u8) -> MaraColor32 {
    MaraColor32::from_rgba_unmultiplied(solid.r(), solid.g(), solid.b(), alpha)
}

// ─── Typed Pod tree builder ─────────────────────────────────────────
//
// `TreeBody` is the typed wrapper passed to `Pod::with_tree`'s
// closure. It exposes ONE method, `row`, that forwards to
// [`tree_row`] — so a tree body can only compose other tree-rows
// (no raw egui access).

/// Typed wrapper around a pod's body Ui — only exposes
/// [`TreeBody::row`] (which forwards to [`tree_row`]) and a Mara
/// memory facade (for persisted-state lookups).
/// Used by `Pod::with_tree` to host a recursive tree without
/// leaking raw [`egui::Ui`] to the caller.
pub struct TreeBody<'a> {
    backend: &'a mut dyn crate::layout::UiBackend,
}

impl<'a> TreeBody<'a> {
    #[doc(hidden)]
    pub(crate) fn new(backend: &'a mut dyn crate::layout::UiBackend) -> Self {
        Self { backend }
    }

    /// Backend-neutral memory facade for persisted and frame-temp
    /// tree state.
    #[must_use]
    pub fn memory(&self) -> crate::memory::BackendMemory<'_> {
        self.backend.memory()
    }

    /// Read a persisted `bool` (e.g. an expanded/collapsed flag)
    /// keyed by `id`. Typed, sealed replacement for
    /// `ctx().data(...)`.
    #[must_use]
    pub fn persisted_bool(&self, id: impl Into<crate::vocab::Id>) -> Option<bool> {
        self.memory().get_persisted::<bool>(id.into())
    }

    /// Write a persisted `bool` keyed by `id`. Typed, sealed
    /// replacement for `ctx_mut().data_mut(...)`.
    pub fn set_persisted_bool(&mut self, id: impl Into<crate::vocab::Id>, value: bool) {
        self.memory().set_persisted(id.into(), value);
    }

    /// Read a persisted `String` (e.g. an "armed item" marker)
    /// keyed by `id`.
    #[must_use]
    pub fn persisted_string(&self, id: impl Into<crate::vocab::Id>) -> Option<String> {
        self.memory().get_persisted::<String>(id.into())
    }

    /// Write a persisted `String` keyed by `id`.
    pub fn set_persisted_string(&mut self, id: impl Into<crate::vocab::Id>, value: String) {
        self.memory().set_persisted(id.into(), value);
    }

    /// Read a frame-temporary `String` (e.g. a selection path
    /// shared with the hosting pane) keyed by `id`.
    #[must_use]
    pub fn temp_string(&self, id: impl Into<crate::vocab::Id>) -> Option<String> {
        self.memory().get_temp::<String>(id.into())
    }

    /// Write a frame-temporary `String` keyed by `id`.
    pub fn set_temp_string(&mut self, id: impl Into<crate::vocab::Id>, value: String) {
        self.memory().set_temp(id.into(), value);
    }

    /// Paint a single tree row. Mirrors [`tree_row`] verbatim.
    #[allow(clippy::too_many_arguments)]
    pub fn row<H: core::hash::Hash + Copy>(
        &mut self,
        id_salt: H,
        depth: u32,
        expanded: Option<&mut bool>,
        icon: Option<&str>,
        label: &str,
        selected: bool,
        accent: impl Into<crate::vocab::Color32> + Copy,
        slots: &mut [TreeIconSlot<'_>],
    ) -> TreeRowResponse {
        tree_row(
            &mut *self.backend,
            id_salt,
            depth,
            expanded,
            icon,
            label,
            selected,
            accent,
            slots,
        )
    }

    /// Paint a two-line hierarchy row with an independent embedded
    /// tail action. Use this for recursive zone/layer trees where
    /// row click selects the item and `+` starts "add child".
    #[allow(clippy::too_many_arguments)]
    pub fn action_row<H: core::hash::Hash + Copy>(
        &mut self,
        id_salt: H,
        depth: u32,
        expanded: Option<&mut bool>,
        icon: Option<&str>,
        title: &str,
        meta: &str,
        selected: bool,
        action_glyph: &str,
        action_tooltip: Option<&str>,
        action_armed: bool,
        accent: impl Into<crate::vocab::Color32> + Copy,
    ) -> TreeActionRowResponse {
        tree_action_row(
            &mut *self.backend,
            id_salt,
            depth,
            expanded,
            icon,
            title,
            meta,
            selected,
            action_glyph,
            action_tooltip,
            action_armed,
            accent,
        )
    }

    /// Paint a hierarchy row with Coreviz/directory-style branch
    /// guides (`├`, `└`, plus ancestor `│` columns).
    #[allow(clippy::too_many_arguments)]
    pub fn action_row_guided<H: core::hash::Hash + Copy>(
        &mut self,
        id_salt: H,
        depth: u32,
        expanded: Option<&mut bool>,
        icon: Option<&str>,
        title: &str,
        meta: &str,
        selected: bool,
        action_glyph: &str,
        action_tooltip: Option<&str>,
        action_armed: bool,
        branch: &TreeBranchGuide,
        accent: impl Into<crate::vocab::Color32> + Copy,
    ) -> TreeActionRowResponse {
        tree_action_row_with_guide(
            &mut *self.backend,
            id_salt,
            depth,
            expanded,
            icon,
            title,
            meta,
            selected,
            action_glyph,
            action_tooltip,
            action_armed,
            Some(branch),
            accent,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree_theme() -> style::TreeTheme {
        style::TreeTheme {
            row_h: TREE_ROW_H,
            indent: TREE_INDENT,
            guide_width: 1.0,
            label_font: 11.0,
            chevron_w: TREE_CHEVRON_W,
            icon_w: TREE_ICON_W,
            label_pad_l: TREE_LABEL_PAD_L,
            slot_w: TREE_SLOT_W,
            slot_gap: TREE_SLOT_GAP,
            right_pad_r: TREE_RIGHT_PAD_R,
            row_pad_l: TREE_ROW_PAD_L,
        }
    }

    fn assert_pos(actual: MaraPos2, expected: MaraPos2) {
        let eps = 0.001;
        assert!(
            (actual.x - expected.x).abs() <= eps && (actual.y - expected.y).abs() <= eps,
            "actual={actual:?} expected={expected:?}"
        );
    }

    #[test]
    fn tree_action_guides_last_child_make_l_joint() {
        let rect = MaraRect::from_min_size(MaraPos2::ZERO, crate::vocab::Vec2::new(200.0, 40.0));
        let segments = tree_action_guide_segments(
            rect,
            tree_theme(),
            1,
            TREE_INDENT,
            Some(&TreeBranchGuide::last([])),
        );

        assert_eq!(segments.len(), 2);
        assert_pos(segments[0][0], MaraPos2::new(TREE_ROW_PAD_L, 0.0));
        assert_pos(segments[0][1], MaraPos2::new(TREE_ROW_PAD_L, 20.0));
        assert_pos(segments[1][0], MaraPos2::new(TREE_ROW_PAD_L, 20.0));
        assert_pos(segments[1][1], MaraPos2::new(TREE_INDENT, 20.0));
    }

    #[test]
    fn tree_action_guides_non_last_child_make_tee_joint() {
        let rect = MaraRect::from_min_size(MaraPos2::ZERO, crate::vocab::Vec2::new(200.0, 40.0));
        let segments = tree_action_guide_segments(
            rect,
            tree_theme(),
            1,
            TREE_INDENT,
            Some(&TreeBranchGuide::tee([])),
        );

        assert_eq!(segments.len(), 2);
        assert_pos(segments[0][0], MaraPos2::new(TREE_ROW_PAD_L, 0.0));
        assert_pos(segments[0][1], MaraPos2::new(TREE_ROW_PAD_L, 40.0));
        assert_pos(segments[1][0], MaraPos2::new(TREE_ROW_PAD_L, 20.0));
        assert_pos(segments[1][1], MaraPos2::new(TREE_INDENT, 20.0));
    }

    #[test]
    fn tree_chevron_backend_lowers_to_polyline_command() {
        let rect = MaraRect::from_min_size(MaraPos2::ZERO, crate::vocab::Vec2::new(12.0, 20.0));

        let cmd = chevron_paint_cmd(rect, 1.0, MaraColor32::WHITE);

        let PaintCmd::Polyline { points, stroke } = cmd else {
            panic!("tree chevron should lower to a polyline command");
        };
        assert_eq!(points.len(), 3);
        assert_eq!(stroke.width, 1.2);
        assert!(points.iter().all(|p| rect.contains(*p)));
    }

    #[test]
    fn tree_action_guides_keep_multiple_ancestor_columns() {
        let rect = MaraRect::from_min_size(MaraPos2::ZERO, crate::vocab::Vec2::new(200.0, 40.0));
        let segments = tree_action_guide_segments(
            rect,
            tree_theme(),
            3,
            3.0 * TREE_INDENT,
            Some(&TreeBranchGuide::last([true, false])),
        );

        assert_eq!(segments.len(), 3);
        assert_pos(segments[0][0], MaraPos2::new(TREE_ROW_PAD_L, 0.0));
        assert_pos(segments[0][1], MaraPos2::new(TREE_ROW_PAD_L, 40.0));
        assert_pos(
            segments[1][0],
            MaraPos2::new(TREE_ROW_PAD_L + 2.0 * TREE_INDENT, 0.0),
        );
        assert_pos(
            segments[1][1],
            MaraPos2::new(TREE_ROW_PAD_L + 2.0 * TREE_INDENT, 20.0),
        );
        assert_pos(
            segments[2][0],
            MaraPos2::new(TREE_ROW_PAD_L + 2.0 * TREE_INDENT, 20.0),
        );
        assert_pos(segments[2][1], MaraPos2::new(3.0 * TREE_INDENT, 20.0));
    }

    #[test]
    fn tree_indent_guides_backend_emit_line_commands() {
        let rect = MaraRect::from_min_size(MaraPos2::ZERO, crate::vocab::Vec2::new(200.0, 20.0));

        let commands =
            tree_indent_guide_paint_cmds(rect, tree_theme(), 2, MaraColor32::from_gray(120));

        assert_eq!(commands.len(), 2);
        let PaintCmd::Line { a, b, stroke } = commands[1] else {
            panic!("tree indent guides should lower to line commands");
        };
        assert_pos(
            a,
            MaraPos2::new(TREE_ROW_PAD_L + TREE_INDENT + TREE_CHEVRON_W * 0.5, 0.0),
        );
        assert_pos(
            b,
            MaraPos2::new(TREE_ROW_PAD_L + TREE_INDENT + TREE_CHEVRON_W * 0.5, 20.0),
        );
        assert_eq!(stroke.width, 1.0);
    }

    #[test]
    fn tree_row_geometry_uses_mara_rects_for_body_chevron_icon_and_slots() {
        let rect =
            MaraRect::from_min_size(MaraPos2::new(10.0, 20.0), MaraVec2::new(240.0, TREE_ROW_H));
        let geom = tree_row_geometry(rect, tree_theme(), 2, true, true, 2);

        assert_eq!(geom.rect, rect);
        assert_eq!(
            geom.chevron_rect,
            Some(MaraRect::from_min_size(
                MaraPos2::new(10.0 + TREE_ROW_PAD_L + 2.0 * TREE_INDENT, 20.0),
                MaraVec2::new(TREE_CHEVRON_W, TREE_ROW_H),
            ))
        );
        assert_eq!(
            geom.icon_rect,
            Some(MaraRect::from_min_size(
                MaraPos2::new(
                    10.0 + TREE_ROW_PAD_L + 2.0 * TREE_INDENT + TREE_CHEVRON_W,
                    20.0,
                ),
                MaraVec2::new(TREE_ICON_W, TREE_ROW_H),
            ))
        );
        assert_eq!(
            geom.body_rect,
            MaraRect::from_min_max(
                MaraPos2::new(10.0, 20.0),
                MaraPos2::new(
                    250.0 - (2.0 * TREE_SLOT_W + TREE_SLOT_GAP + TREE_RIGHT_PAD_R),
                    40.0,
                ),
            )
        );
        assert_eq!(
            geom.slot_rects,
            vec![
                MaraRect::from_min_max(MaraPos2::new(212.0, 20.0), MaraPos2::new(228.0, 40.0)),
                MaraRect::from_min_max(MaraPos2::new(230.0, 20.0), MaraPos2::new(246.0, 40.0)),
            ]
        );
    }

    #[test]
    fn tree_action_row_geometry_uses_mara_rects_for_body_action_and_label() {
        let rect = MaraRect::from_min_size(
            MaraPos2::new(10.0, 20.0),
            MaraVec2::new(260.0, TREE_ACTION_ROW_H),
        );
        let geom = tree_action_row_geometry(rect, tree_theme(), 2, true, true);

        assert_eq!(geom.rect, rect);
        assert_eq!(
            geom.button_rect,
            MaraRect::from_min_max(MaraPos2::new(34.0, 21.0), MaraPos2::new(270.0, 58.0))
        );
        assert_eq!(
            geom.chevron_rect,
            Some(MaraRect::from_min_size(
                MaraPos2::new(38.0, 20.0),
                MaraVec2::new(TREE_CHEVRON_W, TREE_ACTION_ROW_H),
            ))
        );
        assert_eq!(
            geom.icon_rect,
            Some(MaraRect::from_min_size(
                MaraPos2::new(50.0, 20.0),
                MaraVec2::new(TREE_ICON_W, TREE_ACTION_ROW_H),
            ))
        );
        assert_eq!(geom.action_size, TREE_ACTION_W);
        assert_eq!(
            geom.action_rect,
            MaraRect::from_center_size(MaraPos2::new(252.0, 39.5), MaraVec2::new(28.0, 28.0))
        );
        assert_eq!(
            geom.body_rect,
            MaraRect::from_min_max(MaraPos2::new(34.0, 21.0), MaraPos2::new(232.0, 58.0))
        );
        assert_eq!(
            geom.label_rect,
            MaraRect::from_min_max(MaraPos2::new(68.0, 26.0), MaraPos2::new(232.0, 53.0))
        );
    }

    #[test]
    fn tree_action_guides_backend_emit_line_commands() {
        let rect = MaraRect::from_min_size(MaraPos2::ZERO, crate::vocab::Vec2::new(200.0, 40.0));

        let commands = tree_action_guide_paint_cmds(
            rect,
            tree_theme(),
            1,
            TREE_INDENT,
            Some(&TreeBranchGuide::last([])),
            MaraColor32::from_gray(120),
        );

        assert_eq!(commands.len(), 2);
        let PaintCmd::Line { a, b, stroke } = commands[1] else {
            panic!("tree action guides should lower to line commands");
        };
        assert_pos(a, MaraPos2::new(TREE_ROW_PAD_L, 20.0));
        assert_pos(b, MaraPos2::new(TREE_INDENT, 20.0));
        assert_eq!(stroke.width, 1.0);
    }

    #[test]
    fn tree_slot_icons_backend_emit_paint_commands() {
        let rect = MaraRect::from_min_size(
            MaraPos2::new(10.0, 20.0),
            crate::vocab::Vec2::new(16.0, 20.0),
        );

        let eye = slot_icon_paint_cmds(rect, &TreeIconKind::Eye, true, false, MaraColor32::WHITE);
        assert!(matches!(eye[0], PaintCmd::Polyline { .. }));
        assert!(matches!(eye[1], PaintCmd::Polyline { .. }));
        assert!(matches!(eye[2], PaintCmd::CircleFilled { .. }));

        let hidden_eye =
            slot_icon_paint_cmds(rect, &TreeIconKind::Eye, false, true, MaraColor32::WHITE);
        assert!(matches!(hidden_eye[2], PaintCmd::Line { .. }));

        let lock = slot_icon_paint_cmds(rect, &TreeIconKind::Lock, true, false, MaraColor32::WHITE);
        assert!(matches!(lock[0], PaintCmd::RectFilled { .. }));
        assert!(matches!(lock[1], PaintCmd::Polyline { .. }));
        assert!(
            lock.iter()
                .any(|cmd| matches!(cmd, PaintCmd::CircleFilled { .. }))
        );

        let glyph = slot_icon_paint_cmds(
            rect,
            &TreeIconKind::Glyph { on: "A", off: "B" },
            false,
            false,
            MaraColor32::WHITE,
        );
        let [PaintCmd::Text { text, .. }] = glyph.as_slice() else {
            panic!("glyph slot should lower to a text command");
        };
        assert_eq!(text, "B");

        let color = slot_icon_paint_cmds(
            rect,
            &TreeIconKind::Color(MaraColor32::from_rgb(1, 2, 3)),
            false,
            true,
            MaraColor32::WHITE,
        );
        assert!(matches!(color[0], PaintCmd::RectFilled { .. }));
        assert!(matches!(color[1], PaintCmd::RectStroke { .. }));
    }

    #[test]
    fn tree_labels_backend_emit_clipped_text_commands() {
        let rect = MaraRect::from_min_size(
            MaraPos2::new(10.0, 20.0),
            crate::vocab::Vec2::new(80.0, 24.0),
        );

        let cmd = clipped_text_paint_cmd(
            rect,
            rect.left_center(),
            MaraAlign2::LEFT_CENTER,
            "Robot Arm",
            11.0,
            MaraColor32::WHITE,
        );

        let PaintCmd::Clip {
            rect: clip,
            children,
        } = cmd
        else {
            panic!("single-line tree labels should lower to clipped paint commands");
        };
        assert_eq!(clip, rect);
        let [PaintCmd::Text { text, anchor, .. }] = children.as_slice() else {
            panic!("clipped tree label should contain one text command");
        };
        assert_eq!(text, "Robot Arm");
        assert_eq!(*anchor, MaraAlign2::LEFT_CENTER);

        let cmd = two_line_label_paint_cmd(rect, "Zone A", "12 children");
        let PaintCmd::Clip {
            rect: clip,
            children,
        } = cmd
        else {
            panic!("two-line tree labels should lower to clipped paint commands");
        };
        assert_eq!(clip, rect);
        let [
            PaintCmd::Text { text: title, .. },
            PaintCmd::Text { text: meta, .. },
        ] = children.as_slice()
        else {
            panic!("two-line tree label should contain title and metadata text");
        };
        assert_eq!(title, "Zone A");
        assert_eq!(meta, "12 children");
    }

    #[test]
    fn tree_type_icons_backend_emit_named_font_or_text_commands() {
        let icon = icon_or_glyph_paint_cmd(
            MaraPos2::new(10.0, 20.0),
            MaraAlign2::CENTER_CENTER,
            "search",
            14.0,
            MaraColor32::WHITE,
        );
        let PaintCmd::TextWithFamily { text, family, .. } = icon else {
            panic!("bundled type icons should lower to named-font text commands");
        };
        assert_eq!(text.chars().count(), 1);
        let TextFamily::Named(family) = family else {
            panic!("bundled type icons should keep a named text family");
        };
        assert!(!family.is_empty());

        let fallback = icon_or_glyph_paint_cmd(
            MaraPos2::new(10.0, 20.0),
            MaraAlign2::CENTER_CENTER,
            "usd",
            14.0,
            MaraColor32::WHITE,
        );
        let PaintCmd::Text { text, mono, .. } = fallback else {
            panic!("unknown type icons should lower to plain Mara text commands");
        };
        assert_eq!(text, "usd");
        assert!(!mono);
    }
}
