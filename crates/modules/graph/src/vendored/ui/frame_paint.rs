//! Painting and dragging frame groups — PLAN_NODE.md P5.
//!
//! Written entirely against `MaraUi` / `MaraPainter`. This file is
//! checked by `make check` to contain no backend types, so it needs no
//! re-porting when the surrounding renderer finishes its WS-D1
//! migration.
//!
//! # Two decisions worth knowing before editing
//!
//! **Frames paint inline, not through a reserved paint slot.** The
//! backend's slot filler maps `Text`, `Image`, `Svg` and `Clip` to a
//! no-op shape (`shape_from_paint_cmd` in `mara_backend_egui`), so a
//! frame title routed through `fill_paint_slot` would render as
//! nothing — silently, with no error and no panic. Painting inline is
//! possible with no one-frame lag because member rects can be computed
//! up front from cached node state.
//!
//! **Only the title band is draggable.** The canvas's own pan/zoom
//! response is registered before the node loop; a drag hotspot spanning
//! the whole frame rect would be registered later, win on hit priority,
//! and steal panning everywhere a frame exists. It would also break
//! rubber-band selection over a frame. The body stays click-through.

use mara_core::MaraUi;
use mara_core::layout::{CursorIcon, Sense};
use mara_core::style::{FrameRole, FrameSpec, frame_for};
use mara_core::vocab::{Align2, Color32, CornerRadius, Id, Pos2, Rect, Stroke, Vec2};

use crate::vendored::frames::{FrameId, title_band_rect};
use crate::vendored::{Graph, NodeId};

/// Fixed geometry for the group box. Not theme tokens: these are the
/// proportions of the affordance itself, and a theme that changed them
/// would change what the box *is* rather than how it looks.
const PADDING: f32 = 12.0;
const CORNER: u8 = 8;
const FILL_ALPHA: f32 = 0.10;
const STROKE_ALPHA: f32 = 0.45;
const BAND_FILL_ALPHA: f32 = 0.22;
/// Highlight applied to the frame a dragged node would land in.
const CANDIDATE_FILL_ALPHA: f32 = 0.16;
const CANDIDATE_STROKE_ALPHA: f32 = 0.90;
/// Lightness step per nesting level, so dark-on-dark nesting stays
/// legible — Blender's 4.5 fix for the same problem. Deliberately large:
/// the previous 0.06 was reported as indistinguishable in practice, and
/// it was applied through `gamma_multiply`, which scales the alpha byte
/// as well and so partly cancelled itself out.
const DEPTH_LIGHTNESS_STEP: f32 = 0.14;
/// Opacity added per nesting level, so "deeper" reads as "denser" even
/// where every frame shares one colour. Lightness alone has to alternate
/// direction to keep headroom, which makes levels 1 and 3 look alike;
/// opacity only ever grows, so it breaks that tie.
const DEPTH_FILL_STEP: f32 = 0.05;
const DEPTH_STROKE_STEP: f32 = 0.14;
const DEPTH_BAND_STEP: f32 = 0.09;
/// Depth past which the per-level steps stop accumulating. Without a cap
/// a five-deep tree paints an opaque slab and the nodes inside it stop
/// being readable at all — the steps exist to tell levels apart, not to
/// fill the canvas.
const DEPTH_STEP_CAP: u32 = 3;
/// Inset of the second hairline drawn on nested frames, in graph points.
/// Held as a `u8` because the inner corner radius is `CORNER` minus this
/// and corner radii are bytes.
const NEST_INSET_PX: u8 = 3;
const NEST_INSET: f32 = NEST_INSET_PX as f32;
/// Gap between the title band's edge and its text. Matched to `CORNER`
/// so the text clears the rounded corner instead of sitting in it.
const TITLE_PAD: f32 = 8.0;
/// Resize handle size in **screen** points. Divided by the viewport
/// scale at hit-test time so the grab area stays the same physical size
/// at every zoom — Blender filed PR #108359 for getting this wrong, and
/// it costs one division to avoid.
const HANDLE_PX: f32 = 8.0;

/// What a frame pass decided, applied by the caller at the renderer's
/// single deferred-mutation site.
#[derive(Debug, Default)]
pub struct FramePassOutcome {
    /// A frame's title band was dragged by this delta, in graph space.
    pub frame_moved: Option<(FrameId, Vec2)>,
    /// A resize handle was dragged; the frame's new bounds.
    pub frame_resized: Option<(FrameId, Rect)>,
    /// The innermost frame under the pointer, for adoption highlighting
    /// and for deciding membership when a node drag stops.
    pub hovered_frame: Option<FrameId>,
    /// A frame whose title band was double-clicked, asking to fold or
    /// unfold it.
    ///
    /// Without this a collapsed frame is a one-way door: nothing else
    /// in the crate ever clears `Frame::collapsed`, so a group folded
    /// to a pill could only be reopened through the API.
    pub toggle_collapsed: Option<FrameId>,
}

/// Which corner or edge a resize handle drives.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Handle {
    N,
    S,
    E,
    W,
    Nw,
    Ne,
    Sw,
    Se,
}

impl Handle {
    const ALL: [Self; 8] = [
        Self::Nw,
        Self::Ne,
        Self::Sw,
        Self::Se,
        Self::N,
        Self::S,
        Self::E,
        Self::W,
    ];

    fn cursor(self) -> CursorIcon {
        match self {
            Self::N | Self::S => CursorIcon::ResizeVertical,
            Self::E | Self::W => CursorIcon::ResizeHorizontal,
            Self::Nw | Self::Se => CursorIcon::ResizeNwSe,
            Self::Ne | Self::Sw => CursorIcon::ResizeNeSw,
        }
    }

    /// The handle's grab rect, `size` points across in graph space.
    fn rect(self, b: Rect, size: f32) -> Rect {
        let h = size * 0.5;
        let (x, y) = match self {
            Self::Nw => (b.min.x, b.min.y),
            Self::Ne => (b.max.x, b.min.y),
            Self::Sw => (b.min.x, b.max.y),
            Self::Se => (b.max.x, b.max.y),
            Self::N => (b.center().x, b.min.y),
            Self::S => (b.center().x, b.max.y),
            Self::E => (b.max.x, b.center().y),
            Self::W => (b.min.x, b.center().y),
        };
        Rect::from_min_max(Pos2::new(x - h, y - h), Pos2::new(x + h, y + h))
    }

    /// Apply a drag delta to `b`, moving only the edges this handle owns.
    fn resize(self, b: Rect, d: Vec2) -> Rect {
        let (mut min, mut max) = (b.min, b.max);
        match self {
            Self::N => min.y += d.y,
            Self::S => max.y += d.y,
            Self::E => max.x += d.x,
            Self::W => min.x += d.x,
            Self::Nw => {
                min.x += d.x;
                min.y += d.y;
            }
            Self::Ne => {
                max.x += d.x;
                min.y += d.y;
            }
            Self::Sw => {
                min.x += d.x;
                max.y += d.y;
            }
            Self::Se => {
                max.x += d.x;
                max.y += d.y;
            }
        }
        // Keep the rect non-degenerate: dragging an edge past its
        // opposite would otherwise invert it and the box would vanish.
        const MIN: f32 = 48.0;
        if max.x - min.x < MIN {
            max.x = min.x + MIN;
        }
        if max.y - min.y < MIN {
            max.y = min.y + MIN;
        }
        Rect::from_min_max(min, max)
    }
}

/// Paint every frame and register their interactions.
///
/// Call between the background pass and the wire slot reservation, so
/// frames land under wires and under nodes. `rects` supplies node
/// geometry in graph space; `scale` is the viewport scale, used only to
/// keep handle grab areas constant in screen points.
///
/// `pointer_graph` must already be in **graph** space, the space
/// `rects` and `frame_bounds` speak. It is a parameter rather than an
/// `ui.input()` read because `MaraInput::pointer` is in screen space:
/// reading it here made every hit test miss once the canvas was panned
/// or zoomed, so `hovered_frame` came back `None` and the caller
/// ejected each dragged node from its group on drag-stop. Only the
/// renderer holds the inverse transform, so only the renderer can
/// supply this.
pub fn frame_pass<T>(
    ui: &mut MaraUi<'_>,
    graph: &Graph<T>,
    graph_id: Id,
    accent: Color32,
    rects: &dyn Fn(NodeId) -> Rect,
    scale: f32,
    pointer_graph: Option<Pos2>,
    dragging_node: bool,
) -> FramePassOutcome {
    let mut out = FramePassOutcome::default();

    // Outermost first, so a nested frame paints over its parent. Depth
    // is cheap here and stable, unlike relying on slab order.
    let mut ordered: Vec<(FrameId, u32)> = graph
        .frames()
        .map(|(id, _)| (id, graph.frame_depth(id)))
        .collect();
    ordered.sort_by_key(|(_, d)| *d);

    if dragging_node && let Some(p) = pointer_graph {
        out.hovered_frame = graph.frame_at(p, rects, PADDING);
    }

    for (id, depth) in ordered {
        let Some(bounds) = graph.frame_bounds(id, rects, PADDING) else {
            // An empty auto-fitting frame has no meaningful extent.
            // Painting one anyway puts a stray box on the canvas.
            continue;
        };
        let Some(frame) = graph.frame(id) else {
            continue;
        };

        let is_candidate = out.hovered_frame == Some(id);
        let spec = frame_spec_for(depth, accent);
        let base = tint_for_depth(frame.color, depth);

        paint_box(ui, bounds, base, &spec, is_candidate, depth);
        paint_title(ui, frame, bounds, base, graph, id, depth);

        // ── Title-band drag ──
        let band = title_band_rect(frame, bounds);
        let r = ui.interact(
            band,
            graph_id.with(("frame-title", id.0)),
            Sense::ClickAndDrag,
        );
        if r.dragged {
            out.frame_moved = Some((id, r.drag_delta));
        }
        if r.double_clicked {
            out.toggle_collapsed = Some(id);
        }
        if r.hovered {
            ui.set_cursor_icon(CursorIcon::Grabbing);
        }

        // ── Resize handles, manual-size frames only ──
        if !frame.shrink && (r.hovered || pointer_graph.is_some_and(|p| bounds.contains(p))) {
            let size = HANDLE_PX / scale.max(0.01);
            for h in Handle::ALL {
                let hr = h.rect(bounds, size);
                let hid = graph_id.with(("frame-handle", id.0, h as u8));
                let hres = ui.interact(hr, hid, Sense::Drag);
                if hres.hovered {
                    ui.set_cursor_icon(h.cursor());
                }
                if hres.dragged {
                    out.frame_resized = Some((id, h.resize(bounds, hres.drag_delta)));
                }
            }
        }
    }

    out
}

/// Group boxes theme off `FrameRole::Group`, so a host that restyles
/// Mara restyles these without knowing the graph exists.
fn frame_spec_for(depth: u32, accent: Color32) -> FrameSpec {
    let _ = depth;
    frame_for(FrameRole::Group, accent)
}

/// Mix `color` toward white (`amount > 0`) or black (`amount < 0`),
/// leaving its opacity alone.
///
/// [`Color32::gamma_multiply`] scales all four *premultiplied* bytes,
/// alpha included, so using it as a lightness knob also changed how
/// transparent the result was — and since the caller then multiplies by
/// a fill alpha of its own, the two compounded. Mixing toward a target
/// also keeps working at the ends of the range, where a multiply cannot:
/// black times any factor is still black.
fn shade(color: Color32, amount: f32) -> Color32 {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    let target = if amount >= 0.0 { 255.0 } else { 0.0 };
    let t = amount.abs().clamp(0.0, 1.0);
    let mix = |c: u8| (f32::from(c) + (target - f32::from(c)) * t).clamp(0.0, 255.0) as u8;
    Color32::from_rgba_unmultiplied(mix(r), mix(g), mix(b), a)
}

/// Alternate lightness per nesting level so a group inside a group is
/// still distinguishable from its parent.
///
/// Alternating rather than ramping because a ramp runs out of headroom:
/// four levels of "a bit lighter" all land on white. Alternating with a
/// magnitude that grows with depth keeps *adjacent* levels — the ones
/// actually seen touching — the furthest apart.
fn tint_for_depth(color: Color32, depth: u32) -> Color32 {
    if depth == 0 {
        return color;
    }
    let amount = DEPTH_LIGHTNESS_STEP * depth.min(DEPTH_STEP_CAP) as f32;
    if depth % 2 == 1 {
        shade(color, amount)
    } else {
        shade(color, -amount)
    }
}

/// Fill and stroke opacity for a frame at `depth`.
///
/// A candidate frame ignores depth entirely: the point of the highlight
/// is that it is the one obviously different box on screen while a node
/// is in flight.
fn depth_alphas(depth: u32, candidate: bool) -> (f32, f32) {
    if candidate {
        return (CANDIDATE_FILL_ALPHA, CANDIDATE_STROKE_ALPHA);
    }
    let steps = depth.min(DEPTH_STEP_CAP) as f32;
    (
        (FILL_ALPHA + steps * DEPTH_FILL_STEP).min(1.0),
        (STROKE_ALPHA + steps * DEPTH_STROKE_STEP).min(1.0),
    )
}

fn paint_box(
    ui: &mut MaraUi<'_>,
    bounds: Rect,
    base: Color32,
    spec: &FrameSpec,
    candidate: bool,
    depth: u32,
) {
    let corner = CornerRadius::same(CORNER);
    let p = ui.painter();

    if let Some(sh) = spec.shadow {
        p.shadow(bounds, corner, sh.offset, sh.blur, sh.spread, sh.color);
    }

    let (fill_a, stroke_a) = depth_alphas(depth, candidate);

    p.rect_filled(bounds, corner, base.gamma_multiply(fill_a));
    p.rect_stroke(
        bounds,
        corner,
        Stroke::new(1.0, base.gamma_multiply(stroke_a)),
    );

    // A nested frame's border sits directly on its parent's fill, where
    // a single line reads as "one box", not "a box inside a box". The
    // second hairline is the same cue a double rule gives on paper, and
    // unlike a colour shift it survives every theme.
    if depth > 0 && bounds.width() > NEST_INSET * 4.0 && bounds.height() > NEST_INSET * 4.0 {
        p.rect_stroke(
            bounds.shrink(NEST_INSET),
            CornerRadius::same(CORNER.saturating_sub(NEST_INSET_PX)),
            Stroke::new(1.0, base.gamma_multiply(stroke_a * 0.45)),
        );
    }
}

fn paint_title<T>(
    ui: &mut MaraUi<'_>,
    frame: &crate::vendored::frames::Frame,
    bounds: Rect,
    base: Color32,
    graph: &Graph<T>,
    id: FrameId,
    depth: u32,
) {
    let band = title_band_rect(frame, bounds);
    let p = ui.painter();
    // Square bottom corners: the band's lower edge meets the body, and
    // rounding it there left two notches of body fill showing through.
    let band_corner = CornerRadius {
        nw: CORNER,
        ne: CORNER,
        sw: 0,
        se: 0,
    };
    let band_alpha =
        (BAND_FILL_ALPHA + depth.min(DEPTH_STEP_CAP) as f32 * DEPTH_BAND_STEP).min(1.0);
    p.rect_filled(band, band_corner, base.gamma_multiply(band_alpha));

    // Collapsed frames read as a pill carrying their member count, so a
    // folded group still says how much it is hiding.
    let label = if frame.collapsed {
        let n = graph.frame_members_deep(id).len();
        format!("{} ({n})", frame.title)
    } else {
        frame.title.clone()
    };
    if label.is_empty() {
        return;
    }

    // `title_band_rect` clamps the band to the box height, so a frame
    // resized down to the 48pt minimum can end up shorter than its own
    // label. Clamping the painted size by the same ratio the band is
    // derived from keeps the text inside the box vertically, the way
    // truncation keeps it inside horizontally.
    let size = frame
        .label_size
        .min(band.height() / crate::vendored::frames::TITLE_BAND_RATIO)
        .max(1.0);
    let budget = band.width() - TITLE_PAD * 2.0;
    if budget <= 0.0 {
        return;
    }
    let shown = truncate_to_width(&p, &label, size, budget);
    if shown.is_empty() {
        return;
    }

    // `LEFT_CENTER` against the band's centre line, so the text is
    // centred on the band whatever the label size ends up being.
    p.text(
        Pos2::new(band.min.x + TITLE_PAD, band.center().y),
        Align2::LEFT_CENTER,
        shown,
        size,
        base,
    );
}

/// `label` shortened with an ellipsis until it measures no wider than
/// `budget` at `size` points.
///
/// [`MaraPainter::measure_text`] lays out *without* wrapping, so an
/// over-long title runs out past the box instead of folding — which is
/// why truncating is the caller's job and why it has to be done against
/// a real measurement. The estimate this replaces assumed every glyph
/// was `0.55 * size` wide; real text is wider than that often enough
/// that titles escaped their band, which is the reported symptom.
///
/// Binary search over the character count, so a long title costs a
/// handful of measurements rather than one per character removed.
fn truncate_to_width(p: &mara_core::MaraPainter, label: &str, size: f32, budget: f32) -> String {
    if p.measure_text(label, size, false).x <= budget {
        return label.to_string();
    }

    let with_ellipsis = |n: usize| -> String {
        let mut s: String = label.chars().take(n).collect();
        s.push('…');
        s
    };

    // An ellipsis on its own already over-running means there is no
    // honest way to show the title; painting nothing beats painting
    // something that leaves the box.
    if p.measure_text("…", size, false).x > budget {
        return String::new();
    }

    let (mut lo, mut hi) = (0usize, label.chars().count());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if p.measure_text(&with_ellipsis(mid), size, false).x <= budget {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    with_ellipsis(lo)
}
