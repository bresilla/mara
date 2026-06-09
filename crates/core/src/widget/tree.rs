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

use std::hash::Hash;

use crate::style;

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
    Color(egui::Color32),
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
pub fn tree_row(
    ui: &mut egui::Ui,
    id_salt: impl Hash + Copy,
    depth: u32,
    expanded: Option<&mut bool>,
    icon: Option<&str>,
    label: &str,
    selected: bool,
    accent: egui::Color32,
    slots: &mut [TreeIconSlot<'_>],
) -> TreeRowResponse {
    let w = ui.available_width();
    // Reserve z-slots for the row background fill + indent guides
    // BEFORE inline widgets draw.
    let bg_anchor = ui.painter().add(egui::Shape::Noop);
    let guide_anchor = ui.painter().add(egui::Shape::Noop);

    let (rect, body_rect, chevron_rect_opt, icon_rect_opt, slot_rects) = compute_row_rects(
        ui,
        w,
        depth,
        expanded.is_some(),
        icon.is_some(),
        slots.len(),
    );

    let body = ui.interact(
        body_rect,
        ui.id().with(("mara_tree_body", id_salt)),
        egui::Sense::click(),
    );
    let chevron = chevron_rect_opt.map(|cr| {
        ui.interact(
            cr,
            ui.id().with(("mara_tree_chevron", id_salt)),
            egui::Sense::click(),
        )
    });
    let mut icon_responses: Vec<egui::Response> = Vec::with_capacity(slots.len());
    for (i, slot_rect) in slot_rects.iter().enumerate() {
        let mut r = ui.interact(
            *slot_rect,
            ui.id().with(("mara_tree_slot", id_salt, i)),
            egui::Sense::click(),
        );
        if let Some(tip) = slots[i].tooltip {
            r = r.on_hover_text(tip);
        }
        icon_responses.push(r);
    }

    // Background fill — paints under the inline widgets via the
    // reserved slot.
    let any_slot_hovered = icon_responses.iter().any(|r| r.hovered());
    let hovered =
        body.hovered() || chevron.as_ref().is_some_and(|c| c.hovered()) || any_slot_hovered;
    let bg_shape = if selected {
        egui::Shape::rect_filled(
            rect,
            style::radius_for(style::RadiusRole::Compact),
            style::row_selected_fill(accent),
        )
    } else if hovered {
        egui::Shape::rect_filled(
            rect,
            style::radius_for(style::RadiusRole::Compact),
            style::row_hover_fill(accent),
        )
    } else {
        egui::Shape::Noop
    };
    ui.painter().set(bg_anchor, bg_shape);

    // Indent guides — faint vertical lines at each ancestor depth.
    let guide_base = style::theme().border_subtle;
    let guide_color =
        egui::Color32::from_rgba_unmultiplied(guide_base.r(), guide_base.g(), guide_base.b(), 90);
    let mut guides = Vec::with_capacity(depth as usize);
    for d in 0..depth {
        let tree = style::theme().widgets.tree;
        let x = rect.min.x + tree.row_pad_l + d as f32 * tree.indent + tree.chevron_w * 0.5;
        guides.push(egui::Shape::line_segment(
            [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
            egui::Stroke::new(tree.guide_width, guide_color),
        ));
    }
    ui.painter().set(
        guide_anchor,
        if guides.is_empty() {
            egui::Shape::Noop
        } else {
            egui::Shape::Vec(guides)
        },
    );

    // Chevron glyph — animated rotating triangle.
    let glyph_col = style::section_title_color(accent);
    let mut chevron_shift_clicked = false;
    if let (Some(exp), Some(cr)) = (expanded, chevron_rect_opt) {
        let how_open = ui
            .ctx()
            .animate_bool_responsive(ui.id().with(("mara_tree_chev_anim", id_salt)), *exp);
        paint_chevron(ui, cr, how_open, glyph_col);
        if let Some(ref cresp) = chevron
            && cresp.clicked()
        {
            let shift_held = ui.ctx().input(|i| i.modifiers.shift);
            if shift_held {
                chevron_shift_clicked = true;
            } else {
                *exp = !*exp;
            }
        }
    }

    // Type-icon slot — Fluent name lookup with literal-text fallback.
    if let (Some(name), Some(ir)) = (icon, icon_rect_opt) {
        if crate::icons::icon(name).is_some() {
            crate::icons::paint_icon(
                ui.painter(),
                ir.center(),
                egui::Align2::CENTER_CENTER,
                name,
                style::theme().icons.tree_type_icon_size,
                glyph_col,
            );
        } else {
            ui.painter().text(
                ir.center(),
                egui::Align2::CENTER_CENTER,
                name,
                egui::FontId::proportional(style::theme().icons.tree_glyph_icon_size),
                glyph_col,
            );
        }
    }

    // Label — truncated to the body rect minus its left padding.
    let parent_clip = ui.clip_rect();
    if parent_clip.intersects(rect) {
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
        let label_rect = egui::Rect::from_min_max(
            egui::pos2(label_left, rect.min.y),
            egui::pos2(body_rect.max.x, rect.max.y),
        );
        let label_color = style::on_section();
        let font = egui::FontId::proportional(style::theme().widgets.tree.label_font);
        let galley = {
            let mut job = egui::text::LayoutJob::single_section(
                label.to_string(),
                egui::TextFormat::simple(font, label_color),
            );
            job.wrap.max_width = label_rect.width().max(0.0);
            job.wrap.max_rows = 1;
            job.wrap.break_anywhere = true;
            job.halign = egui::Align::LEFT;
            ui.painter().layout_job(job)
        };
        ui.painter().galley(
            egui::pos2(
                label_rect.min.x,
                label_rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            label_color,
        );

        // Right-gutter slots — paint each icon in its reserved square.
        for (i, slot) in slots.iter_mut().enumerate() {
            let rect = slot_rects[i];
            let resp = &icon_responses[i];
            paint_slot_icon(ui, rect, &slot.kind, *slot.state, resp.hovered(), accent);
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
        body: body.into(),
        chevron: chevron.map(Into::into),
        icons: icon_responses.into_iter().map(Into::into).collect(),
        chevron_shift_clicked,
    }
}

/// Paint one hierarchy row with two text layers and an embedded
/// independent action button at the far end. This mirrors the
/// Coreviz zone-list shape: clicking the row selects/focuses the
/// item; clicking the `+` arms child creation without also selecting
/// the row.
#[allow(clippy::too_many_arguments)]
pub fn tree_action_row(
    ui: &mut egui::Ui,
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
    accent: egui::Color32,
) -> TreeActionRowResponse {
    tree_action_row_with_guide(
        ui,
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
pub fn tree_action_row_with_guide(
    ui: &mut egui::Ui,
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
    accent: egui::Color32,
) -> TreeActionRowResponse {
    let tree = style::theme().widgets.tree;
    let w = ui.available_width();
    let bg_anchor = ui.painter().add(egui::Shape::Noop);
    let guide_anchor = ui.painter().add(egui::Shape::Noop);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, TREE_ACTION_ROW_H), egui::Sense::hover());

    let button_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x + depth as f32 * tree.indent, rect.min.y),
        rect.max,
    )
    .shrink2(egui::vec2(0.0, 1.0));
    let left_start = button_rect.min.x + tree.row_pad_l;
    let chevron_rect = expanded.as_ref().map(|_| {
        egui::Rect::from_min_size(
            egui::pos2(left_start, rect.min.y),
            egui::vec2(tree.chevron_w, rect.height()),
        )
    });
    let icon_rect = icon.map(|_| {
        egui::Rect::from_min_size(
            egui::pos2(left_start + tree.chevron_w, rect.min.y),
            egui::vec2(tree.icon_w, rect.height()),
        )
    });
    let action_size = TREE_ACTION_W.min((rect.height() - 10.0).max(18.0));
    let action_rect = egui::Rect::from_center_size(
        egui::pos2(
            button_rect.max.x - tree.right_pad_r - action_size * 0.5,
            button_rect.center().y,
        ),
        egui::vec2(action_size, action_size),
    );
    let body_rect = egui::Rect::from_min_max(
        button_rect.min,
        egui::pos2(
            (action_rect.min.x - TREE_ACTION_GAP).max(button_rect.min.x),
            button_rect.max.y,
        ),
    );

    let body = ui.interact(
        body_rect,
        ui.id().with(("mara_tree_action_body", id_salt)),
        egui::Sense::click(),
    );
    let chevron = chevron_rect.map(|cr| {
        ui.interact(
            cr,
            ui.id().with(("mara_tree_action_chevron", id_salt)),
            egui::Sense::click(),
        )
    });
    let mut action = ui.interact(
        action_rect,
        ui.id().with(("mara_tree_action_tail", id_salt)),
        egui::Sense::click(),
    );
    if let Some(tip) = action_tooltip {
        action = action.on_hover_text(tip);
    }

    let hovered =
        body.hovered() || action.hovered() || chevron.as_ref().is_some_and(|c| c.hovered());
    let pressed = body.is_pointer_button_down_on() || action.is_pointer_button_down_on();
    let active = hovered || pressed;
    let theme = style::theme();
    let button = theme.widgets.button;
    let hover_t = if theme.animations_enabled {
        ui.ctx().animate_bool_with_time(
            ui.id().with(("mara_tree_action_button_hover", id_salt)),
            active,
            0.25 * theme.button_anim_scale.max(0.01),
        )
    } else if active {
        1.0
    } else {
        0.0
    };
    let radius = style::radius_for(style::RadiusRole::Widget);
    let body_acc = style::body_accent(accent);
    let base = if style::section_show_frame() {
        style::section_fill(accent)
    } else {
        style::pane_fill(accent)
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
    let bg_shape = egui::Shape::rect_filled(button_rect, radius, bg);
    ui.painter().set(bg_anchor, bg_shape);
    ui.painter().rect_stroke(
        button_rect,
        radius,
        egui::Stroke::new(
            theme.border_width,
            lerp_color_opaque(style::widget_border(accent), accent, hover_t),
        ),
        egui::epaint::StrokeKind::Inside,
    );

    let guide_base = style::theme().border_subtle;
    let guide_color =
        egui::Color32::from_rgba_unmultiplied(guide_base.r(), guide_base.g(), guide_base.b(), 90);
    let guides =
        tree_action_guide_shapes(rect, tree, depth, button_rect.min.x, branch, guide_color);
    ui.painter().set(
        guide_anchor,
        if guides.is_empty() {
            egui::Shape::Noop
        } else {
            egui::Shape::Vec(guides)
        },
    );

    let glyph_col = style::section_title_color(accent);
    let mut chevron_shift_clicked = false;
    if let (Some(exp), Some(cr)) = (expanded, chevron_rect) {
        let how_open = ui
            .ctx()
            .animate_bool_responsive(ui.id().with(("mara_tree_action_chev_anim", id_salt)), *exp);
        paint_chevron(ui, cr, how_open, glyph_col);
        if let Some(ref cresp) = chevron
            && cresp.clicked()
        {
            let shift_held = ui.ctx().input(|i| i.modifiers.shift);
            if shift_held {
                chevron_shift_clicked = true;
            } else {
                *exp = !*exp;
            }
        }
    }

    if let (Some(name), Some(ir)) = (icon, icon_rect) {
        paint_icon_or_glyph(
            ui,
            ir.center(),
            egui::Align2::CENTER_CENTER,
            name,
            style::theme().icons.tree_type_icon_size,
            glyph_col,
        );
    }

    let label_left = left_start
        + tree.chevron_w
        + if icon.is_some() { tree.icon_w } else { 0.0 }
        + tree.label_pad_l;
    let label_rect = egui::Rect::from_min_max(
        egui::pos2(label_left, rect.min.y + 6.0),
        egui::pos2(body_rect.max.x, rect.max.y - 6.0),
    );
    paint_two_line_label(ui, label_rect, title, meta);

    let action_hover_t = if style::theme().animations_enabled {
        ui.ctx().animate_bool_with_time(
            action.id.with("mara_tree_action_tail_hover"),
            action.hovered() || action.is_pointer_button_down_on() || action_armed,
            0.18 * style::theme().button_anim_scale.max(0.01),
        )
    } else if action.hovered() || action.is_pointer_button_down_on() || action_armed {
        1.0
    } else {
        0.0
    };
    let action_fill = lerp_color(
        style::surface_lift_target(style::body_accent(accent)),
        accent,
        if action_armed {
            0.30
        } else {
            0.18 * action_hover_t
        },
    );
    let action_fill = egui::Color32::from_rgba_unmultiplied(
        action_fill.r(),
        action_fill.g(),
        action_fill.b(),
        80,
    );
    let action_radius = egui::CornerRadius::same((action_size * 0.5).round() as u8);
    ui.painter()
        .rect_filled(action_rect, action_radius, action_fill);
    ui.painter().rect_stroke(
        action_rect,
        action_radius,
        egui::Stroke::new(
            style::theme().stroke.border_width,
            lerp_color_opaque(
                style::widget_border(accent),
                accent,
                action_hover_t.max(if action_armed { 0.75 } else { 0.0 }),
            ),
        ),
        egui::epaint::StrokeKind::Inside,
    );
    paint_icon_or_glyph(
        ui,
        action_rect.center(),
        egui::Align2::CENTER_CENTER,
        action_glyph,
        16.0,
        accent,
    );

    TreeActionRowResponse {
        body: body.into(),
        chevron: chevron.map(Into::into),
        action: action.into(),
        chevron_shift_clicked,
    }
}

fn compute_row_rects(
    ui: &mut egui::Ui,
    w: f32,
    depth: u32,
    has_chevron: bool,
    has_icon: bool,
    slot_count: usize,
) -> (
    egui::Rect,
    egui::Rect,
    Option<egui::Rect>,
    Option<egui::Rect>,
    Vec<egui::Rect>,
) {
    let tree = style::theme().widgets.tree;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, tree.row_h), egui::Sense::hover());
    let left_start = rect.min.x + tree.row_pad_l + depth as f32 * tree.indent;
    let chevron_rect = if has_chevron {
        Some(egui::Rect::from_min_size(
            egui::pos2(left_start, rect.min.y),
            egui::vec2(tree.chevron_w, rect.height()),
        ))
    } else {
        None
    };
    let icon_rect = if has_icon {
        Some(egui::Rect::from_min_size(
            egui::pos2(left_start + tree.chevron_w, rect.min.y),
            egui::vec2(tree.icon_w, rect.height()),
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
            slot_rects.push(egui::Rect::from_min_max(
                egui::pos2(x_min, rect.min.y),
                egui::pos2(x_max, rect.max.y),
            ));
            x_max = x_min - tree.slot_gap;
        }
        slot_rects.reverse();
    }

    let body_rect =
        egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x - gutter_w, rect.max.y));
    (rect, body_rect, chevron_rect, icon_rect, slot_rects)
}

fn tree_action_guide_shapes(
    rect: egui::Rect,
    tree: style::TreeTheme,
    depth: u32,
    button_left: f32,
    branch: Option<&TreeBranchGuide>,
    color: egui::Color32,
) -> Vec<egui::Shape> {
    let stroke = egui::Stroke::new(tree.guide_width, color);
    tree_action_guide_segments(rect, tree, depth, button_left, branch)
        .into_iter()
        .map(|segment| egui::Shape::line_segment(segment, stroke))
        .collect()
}

fn tree_action_guide_segments(
    rect: egui::Rect,
    tree: style::TreeTheme,
    depth: u32,
    button_left: f32,
    branch: Option<&TreeBranchGuide>,
) -> Vec<[egui::Pos2; 2]> {
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
                segments.push([egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)]);
            }
        }

        let x = column_x(depth - 1);
        let vertical_bottom = if branch.is_last { joint_y } else { rect.max.y };
        segments.push([egui::pos2(x, rect.min.y), egui::pos2(x, vertical_bottom)]);
        segments.push([egui::pos2(x, joint_y), egui::pos2(button_left, joint_y)]);
    } else {
        for level in 0..depth {
            let x = column_x(level);
            segments.push([egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)]);
        }

        let x = column_x(depth - 1);
        segments.push([egui::pos2(x, joint_y), egui::pos2(button_left, joint_y)]);
    }

    segments
}

/// Thin stroked chevron (`›` rotating to `⌄`) inside `rect`.
fn paint_chevron(ui: &egui::Ui, rect: egui::Rect, how_open: f32, color: egui::Color32) {
    const GLYPH_W: f32 = 5.5;
    const GLYPH_H: f32 = 3.5;
    let cx = rect.center().x;
    let cy = rect.center().y;
    let hw = GLYPH_W * 0.5;
    let hh = GLYPH_H * 0.5;
    let raw = [
        egui::vec2(-hw, -hh),
        egui::vec2(0.0, hh),
        egui::vec2(hw, -hh),
    ];
    use std::f32::consts::TAU;
    let rot = egui::emath::Rot2::from_angle(egui::lerp(-TAU / 4.0..=0.0, how_open));
    let pts: Vec<egui::Pos2> = raw
        .iter()
        .map(|v| {
            let r = rot * *v;
            egui::pos2(cx + r.x, cy + r.y)
        })
        .collect();
    ui.painter()
        .add(egui::Shape::line(pts, egui::Stroke::new(1.2, color)));
}

fn paint_slot_icon(
    ui: &egui::Ui,
    rect: egui::Rect,
    kind: &TreeIconKind,
    active: bool,
    hovered: bool,
    accent: egui::Color32,
) {
    let color = slot_color(active, hovered, accent);
    match *kind {
        TreeIconKind::Eye => paint_eye(ui, rect, active, color),
        TreeIconKind::Lock => paint_lock(ui, rect, active, color),
        TreeIconKind::Glyph { on, off } => {
            let glyph = if active { on } else { off };
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                glyph,
                egui::FontId::proportional(style::theme().icons.tree_glyph_icon_size),
                color,
            );
        }
        TreeIconKind::Color(fill) => paint_color_chip(ui, rect, fill, accent, hovered),
    }
}

fn paint_icon_or_glyph(
    ui: &egui::Ui,
    pos: egui::Pos2,
    align: egui::Align2,
    name: &str,
    font_size: f32,
    color: egui::Color32,
) {
    if crate::icons::icon(name).is_some() {
        crate::icons::paint_icon(ui.painter(), pos, align, name, font_size, color);
    } else {
        ui.painter().text(
            pos,
            align,
            name,
            egui::FontId::proportional(font_size),
            color,
        );
    }
}

fn paint_two_line_label(ui: &egui::Ui, rect: egui::Rect, title: &str, meta: &str) {
    let title_galley = elided_galley(
        ui,
        title,
        egui::FontId::proportional(style::theme().widgets.tree.label_font + 1.0),
        style::on_section(),
        rect.width(),
    );
    let meta_galley = elided_galley(
        ui,
        meta,
        egui::FontId::proportional((style::theme().widgets.tree.label_font - 1.0).max(9.0)),
        style::on_section_dim(),
        rect.width(),
    );
    ui.painter().galley(
        egui::pos2(
            rect.min.x,
            rect.center().y - 8.0 - title_galley.size().y * 0.5,
        ),
        title_galley,
        style::on_section(),
    );
    ui.painter().galley(
        egui::pos2(
            rect.min.x,
            rect.center().y + 8.0 - meta_galley.size().y * 0.5,
        ),
        meta_galley,
        style::on_section_dim(),
    );
}

fn elided_galley(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    max_w: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_string(),
        egui::TextFormat::simple(font, color),
    );
    job.wrap.max_width = max_w.max(0.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.halign = egui::Align::LEFT;
    ui.painter().layout_job(job)
}

fn paint_color_chip(
    ui: &egui::Ui,
    rect: egui::Rect,
    fill: egui::Color32,
    accent: egui::Color32,
    hovered: bool,
) {
    let inner = rect.shrink(3.0);
    let border = if hovered {
        egui::Stroke::new(style::theme().stroke.border_width, accent)
    } else {
        style::stroke_for(style::StrokeRole::WidgetBorder, accent)
    };
    ui.painter().rect(
        inner,
        style::radius_for(style::RadiusRole::Compact),
        fill,
        border,
        egui::epaint::StrokeKind::Inside,
    );
}

fn slot_color(active: bool, hovered: bool, accent: egui::Color32) -> egui::Color32 {
    match (active, hovered) {
        (true, true) => accent,
        (true, false) => style::on_section(),
        (false, true) => lerp_color(style::on_section_dim(), accent, 0.4),
        (false, false) => style::on_section_dim(),
    }
}

/// Eye — almond outline + pupil active, slash when off.
fn paint_eye(ui: &egui::Ui, rect: egui::Rect, active: bool, color: egui::Color32) {
    let c = rect.center();
    let rx = 5.5_f32;
    let ry = 3.2_f32;
    let stroke = egui::Stroke::new(1.1, color);

    let lid = |sign: f32| {
        let mut pts = Vec::with_capacity(11);
        for i in 0..=10 {
            let t = i as f32 / 10.0;
            let x = c.x + (t - 0.5) * 2.0 * rx;
            let y = c.y + sign * ry * (1.0 - ((x - c.x) / rx).powi(2));
            pts.push(egui::pos2(x, y));
        }
        egui::Shape::line(pts, stroke)
    };
    ui.painter().add(lid(1.0));
    ui.painter().add(lid(-1.0));

    if active {
        ui.painter().circle_filled(c, 1.6, color);
    } else {
        ui.painter().line_segment(
            [
                egui::pos2(c.x - rx - 0.5, c.y + ry + 0.5),
                egui::pos2(c.x + rx + 0.5, c.y - ry - 0.5),
            ],
            egui::Stroke::new(1.3, color),
        );
    }
}

/// Padlock — body + shackle, lifted leg on unlocked.
fn paint_lock(ui: &egui::Ui, rect: egui::Rect, active: bool, color: egui::Color32) {
    let c = rect.center();
    let body_w = 7.0_f32;
    let body_h = 5.5_f32;
    let body_top_y = c.y + 0.2;
    let body_rect = egui::Rect::from_min_size(
        egui::pos2(c.x - body_w * 0.5, body_top_y),
        egui::vec2(body_w, body_h),
    );
    ui.painter().rect_filled(
        body_rect,
        style::radius_for(style::RadiusRole::Compact),
        color,
    );

    let stroke = egui::Stroke::new(1.1, color);
    let shackle_top_y = body_top_y - 4.0;
    let legs_x_l = c.x - 2.4;
    let legs_x_r = c.x + 2.4;

    let mut arc = Vec::with_capacity(7);
    for i in 0..=6 {
        let t = i as f32 / 6.0;
        let theta = std::f32::consts::PI - t * std::f32::consts::PI;
        let x = c.x + theta.cos() * 2.4;
        let y = shackle_top_y - theta.sin() * 1.6;
        arc.push(egui::pos2(x, y));
    }
    ui.painter().add(egui::Shape::line(arc, stroke));

    ui.painter().line_segment(
        [
            egui::pos2(legs_x_l, shackle_top_y),
            egui::pos2(legs_x_l, body_top_y + 0.3),
        ],
        stroke,
    );
    if active {
        ui.painter().line_segment(
            [
                egui::pos2(legs_x_r, shackle_top_y),
                egui::pos2(legs_x_r, body_top_y + 0.3),
            ],
            stroke,
        );
    } else {
        ui.painter().line_segment(
            [
                egui::pos2(legs_x_r, shackle_top_y),
                egui::pos2(legs_x_r, shackle_top_y + 2.0),
            ],
            stroke,
        );
    }

    if active {
        ui.painter().circle_filled(
            egui::pos2(c.x, body_rect.center().y),
            0.7,
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 140),
        );
    }
}

fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| ((x as f32) * (1.0 - t) + (y as f32) * t).round() as u8;
    egui::Color32::from_rgba_premultiplied(
        mix(a.r(), b.r()),
        mix(a.g(), b.g()),
        mix(a.b(), b.b()),
        mix(a.a(), b.a()),
    )
}

fn lerp_color_opaque(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| ((x as f32) * (1.0 - t) + (y as f32) * t).round() as u8;
    egui::Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

fn with_alpha(solid: egui::Color32, alpha: u8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(solid.r(), solid.g(), solid.b(), alpha)
}

// ─── Typed Pod tree builder ─────────────────────────────────────────
//
// `TreeBody` is the typed wrapper passed to `Pod::with_tree`'s
// closure. It exposes ONE method, `row`, that forwards to
// [`tree_row`] — so a tree body can only compose other tree-rows
// (no raw egui access).

/// Typed wrapper around a pod's body Ui — only exposes
/// [`TreeBody::row`] (which forwards to [`tree_row`]) and read-only
/// access to the egui [`Context`] (for persisted-state lookups).
/// Used by `Pod::with_tree` to host a recursive tree without
/// leaking raw [`egui::Ui`] to the caller.
pub struct TreeBody<'a> {
    ui: &'a mut egui::Ui,
}

impl<'a> TreeBody<'a> {
    #[doc(hidden)]
    pub fn new(ui: &'a mut egui::Ui) -> Self {
        Self { ui }
    }

    /// Read-only egui context, for persisted-state lookups
    /// (`ctx().data(...)`). Raw-egui escape hatch — sealed
    /// consumers use the typed persisted-state helpers instead.
    #[cfg(feature = "raw-egui")]
    #[must_use]
    pub fn ctx(&self) -> &egui::Context {
        self.ui.ctx()
    }

    /// Mutable egui context, for persisted-state writes
    /// (`ctx_mut().data_mut(...)`). Raw-egui escape hatch.
    #[cfg(feature = "raw-egui")]
    #[must_use]
    pub fn ctx_mut(&mut self) -> &egui::Context {
        // egui's `data_mut` only needs `&Context` even though it
        // mutates internal state, so this returns `&Context` not
        // `&mut Context`. The name `ctx_mut` signals intent.
        self.ui.ctx()
    }

    /// Read a persisted `bool` (e.g. an expanded/collapsed flag)
    /// keyed by `id`. Typed, sealed replacement for
    /// `ctx().data(...)`.
    #[must_use]
    pub fn persisted_bool(&self, id: egui::Id) -> Option<bool> {
        self.ui.ctx().data_mut(|d| d.get_persisted::<bool>(id))
    }

    /// Write a persisted `bool` keyed by `id`. Typed, sealed
    /// replacement for `ctx_mut().data_mut(...)`.
    pub fn set_persisted_bool(&mut self, id: egui::Id, value: bool) {
        self.ui.ctx().data_mut(|d| d.insert_persisted(id, value));
    }

    /// Read a persisted `String` (e.g. an "armed item" marker)
    /// keyed by `id`.
    #[must_use]
    pub fn persisted_string(&self, id: egui::Id) -> Option<String> {
        self.ui.ctx().data_mut(|d| d.get_persisted::<String>(id))
    }

    /// Write a persisted `String` keyed by `id`.
    pub fn set_persisted_string(&mut self, id: egui::Id, value: String) {
        self.ui.ctx().data_mut(|d| d.insert_persisted(id, value));
    }

    /// Read a frame-temporary `String` (e.g. a selection path
    /// shared with the hosting pane) keyed by `id`.
    #[must_use]
    pub fn temp_string(&self, id: egui::Id) -> Option<String> {
        self.ui.ctx().data(|d| d.get_temp::<String>(id))
    }

    /// Write a frame-temporary `String` keyed by `id`.
    pub fn set_temp_string(&mut self, id: egui::Id, value: String) {
        self.ui.ctx().data_mut(|d| d.insert_temp(id, value));
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
        accent: egui::Color32,
        slots: &mut [TreeIconSlot<'_>],
    ) -> TreeRowResponse {
        tree_row(
            self.ui, id_salt, depth, expanded, icon, label, selected, accent, slots,
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
        accent: egui::Color32,
    ) -> TreeActionRowResponse {
        tree_action_row(
            self.ui,
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
        accent: egui::Color32,
    ) -> TreeActionRowResponse {
        tree_action_row_with_guide(
            self.ui,
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

    fn assert_pos(actual: egui::Pos2, expected: egui::Pos2) {
        let eps = 0.001;
        assert!(
            (actual.x - expected.x).abs() <= eps && (actual.y - expected.y).abs() <= eps,
            "actual={actual:?} expected={expected:?}"
        );
    }

    #[test]
    fn tree_action_guides_last_child_make_l_joint() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 40.0));
        let segments = tree_action_guide_segments(
            rect,
            tree_theme(),
            1,
            TREE_INDENT,
            Some(&TreeBranchGuide::last([])),
        );

        assert_eq!(segments.len(), 2);
        assert_pos(segments[0][0], egui::pos2(TREE_ROW_PAD_L, 0.0));
        assert_pos(segments[0][1], egui::pos2(TREE_ROW_PAD_L, 20.0));
        assert_pos(segments[1][0], egui::pos2(TREE_ROW_PAD_L, 20.0));
        assert_pos(segments[1][1], egui::pos2(TREE_INDENT, 20.0));
    }

    #[test]
    fn tree_action_guides_non_last_child_make_tee_joint() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 40.0));
        let segments = tree_action_guide_segments(
            rect,
            tree_theme(),
            1,
            TREE_INDENT,
            Some(&TreeBranchGuide::tee([])),
        );

        assert_eq!(segments.len(), 2);
        assert_pos(segments[0][0], egui::pos2(TREE_ROW_PAD_L, 0.0));
        assert_pos(segments[0][1], egui::pos2(TREE_ROW_PAD_L, 40.0));
        assert_pos(segments[1][0], egui::pos2(TREE_ROW_PAD_L, 20.0));
        assert_pos(segments[1][1], egui::pos2(TREE_INDENT, 20.0));
    }

    #[test]
    fn tree_action_guides_keep_multiple_ancestor_columns() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 40.0));
        let segments = tree_action_guide_segments(
            rect,
            tree_theme(),
            3,
            3.0 * TREE_INDENT,
            Some(&TreeBranchGuide::last([true, false])),
        );

        assert_eq!(segments.len(), 3);
        assert_pos(segments[0][0], egui::pos2(TREE_ROW_PAD_L, 0.0));
        assert_pos(segments[0][1], egui::pos2(TREE_ROW_PAD_L, 40.0));
        assert_pos(
            segments[1][0],
            egui::pos2(TREE_ROW_PAD_L + 2.0 * TREE_INDENT, 0.0),
        );
        assert_pos(
            segments[1][1],
            egui::pos2(TREE_ROW_PAD_L + 2.0 * TREE_INDENT, 20.0),
        );
        assert_pos(
            segments[2][0],
            egui::pos2(TREE_ROW_PAD_L + 2.0 * TREE_INDENT, 20.0),
        );
        assert_pos(segments[2][1], egui::pos2(3.0 * TREE_INDENT, 20.0));
    }
}
