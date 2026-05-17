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
    pub body: egui::Response,
    /// Click target for the chevron glyph only. `None` for leaves.
    pub chevron: Option<egui::Response>,
    /// One `Response` per entry in the `slots` slice. The widget has
    /// already toggled each slot's `state` for you on click; this
    /// response is for additional hooks.
    pub icons: Vec<egui::Response>,
    /// `true` when the chevron was clicked with the shift modifier
    /// held — caller's "recursively expand subtree" affordance.
    pub chevron_shift_clicked: bool,
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
        body,
        chevron,
        icons: icon_responses,
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
    /// (`ctx().data(...)`).
    #[must_use]
    pub fn ctx(&self) -> &egui::Context {
        self.ui.ctx()
    }

    /// Mutable egui context, for persisted-state writes
    /// (`ctx_mut().data_mut(...)`).
    #[must_use]
    pub fn ctx_mut(&mut self) -> &egui::Context {
        // egui's `data_mut` only needs `&Context` even though it
        // mutates internal state, so this returns `&Context` not
        // `&mut Context`. The name `ctx_mut` signals intent.
        self.ui.ctx()
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
}
