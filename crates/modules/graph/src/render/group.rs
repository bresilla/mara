//! Frames — the named boxes that hold a group of nodes.
//!
//! Drawn behind everything, hit-tested in front of the canvas but
//! behind nodes, so clicking inside a frame still picks the node under
//! the pointer and only the title band grabs the frame itself.
//!
//! This file is checked by `make check` to contain no backend types.

use mara_core::mui::MaraPainter;
use mara_core::vocab::{Align2, Color32, CornerRadius, Pos2, Rect, Stroke, Vec2};

use super::spec::GraphSpec;
use super::view::Camera;
use crate::{Frame, FrameId, Graph, NodeId, fit_frame_bounds};

/// Breathing room an auto-fitting frame leaves around its members.
const FRAME_PAD: f32 = 18.0;

/// A frame's on-screen geometry for this frame of animation.
#[derive(Clone, Copy, Debug)]
pub struct PlacedFrame {
    pub id: FrameId,
    /// The whole box, in screen space.
    pub rect: Rect,
    /// The title band along the top — the only part that drags the box.
    pub band: Rect,
}

/// Lay out every frame in `graph`, outermost first.
///
/// Auto-fitting frames are refitted to their members here rather than
/// when a node moves, because a node can move for reasons the frame
/// never sees — an undo, a paste, a layout pass.
pub fn place_frames<T>(
    graph: &mut Graph<T>,
    camera: Camera,
    _spec: &GraphSpec,
    node_rect: &dyn Fn(NodeId) -> Rect,
) -> Vec<PlacedFrame> {
    let ids: Vec<FrameId> = graph.frames().map(|(id, _)| id).collect();
    for id in &ids {
        let shrink = graph.frame(*id).is_some_and(|f| f.shrink);
        if !shrink {
            continue;
        }
        if let Some(fitted) = fit_frame_bounds(graph, *id, node_rect, FRAME_PAD) {
            if let Some(f) = graph.frame_mut(*id) {
                f.bounds = fitted;
            }
        }
    }

    let mut placed: Vec<PlacedFrame> = ids
        .iter()
        .filter_map(|id| {
            let f = graph.frame(*id)?;
            let rect = Rect::from_two_pos(
                camera.to_screen(f.bounds.min),
                camera.to_screen(f.bounds.max),
            );
            let band_h = (f.label_size * crate::vendored::frames::TITLE_BAND_RATIO * camera.zoom)
                .min(rect.height() * 0.5);
            let band = Rect::from_min_size(rect.min, Vec2::new(rect.width(), band_h));
            Some(PlacedFrame { id: *id, rect, band })
        })
        .collect();

    // Outer frames paint first so a nested frame reads as sitting
    // inside its parent rather than being hidden by it.
    placed.sort_by(|a, b| {
        b.rect
            .area()
            .partial_cmp(&a.rect.area())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    placed
}

/// Paint one frame.
pub fn paint_frame(
    p: &MaraPainter,
    pf: &PlacedFrame,
    f: &Frame,
    hovered: bool,
    zoom: f32,
    spec: &GraphSpec,
) {
    let corner = CornerRadius::same(spec.node.corner);
    // A wash rather than a fill: the box has to read as an enclosure
    // without darkening the nodes it holds.
    p.rect_filled(pf.rect, corner, with_alpha(f.color, 20));
    p.rect_filled(
        pf.band,
        CornerRadius::from_corners(spec.node.corner, spec.node.corner, 0, 0),
        with_alpha(f.color, 120),
    );
    p.rect_stroke(
        pf.rect,
        corner,
        Stroke::new(
            if hovered { 2.0 } else { 1.2 },
            with_alpha(f.color, if hovered { 235 } else { 175 }),
        ),
    );

    let size = f.label_size * zoom;
    if size >= 7.0 && pf.band.height() > size {
        p.text(
            Pos2::new(pf.band.min.x + 8.0 * zoom, pf.band.center().y),
            Align2::LEFT_CENTER,
            f.title.clone(),
            size,
            spec.palette.title,
        );
    }
}

/// The frame whose title band is under `p`, innermost first.
///
/// Only the band, deliberately: a frame that grabbed its whole area
/// would make every node inside it undraggable.
#[must_use]
pub fn band_at(placed: &[PlacedFrame], p: Pos2) -> Option<FrameId> {
    placed.iter().rev().find(|f| f.band.contains(p)).map(|f| f.id)
}

/// The innermost frame whose box contains `p`.
#[must_use]
pub fn frame_at(placed: &[PlacedFrame], p: Pos2) -> Option<FrameId> {
    placed.iter().rev().find(|f| f.rect.contains(p)).map(|f| f.id)
}

/// Move a frame and everything inside it by `delta` graph points.
pub fn move_frame<T>(graph: &mut Graph<T>, frame: FrameId, delta: Vec2) {
    let members: Vec<NodeId> = graph.frame_members_deep(frame);
    for id in members {
        if let Some(info) = graph.get_node_info_mut(id) {
            info.pos += delta;
        }
    }
    let children: Vec<FrameId> = graph.child_frames(frame).map(|(id, _)| id).collect();
    for child in children {
        if let Some(f) = graph.frame_mut(child) {
            f.bounds = f.bounds.translate(delta);
        }
    }
    if let Some(f) = graph.frame_mut(frame) {
        f.bounds = f.bounds.translate(delta);
    }
}

fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Pos2 as GPos;

    fn graph_with_frame() -> (Graph<u8>, FrameId, NodeId) {
        let mut g: Graph<u8> = Graph::new();
        let n = g.insert_node(GPos::new(100.0, 100.0), 1);
        let f = g.insert_frame(
            "group",
            Color32::from_rgb(90, 140, 200),
            Rect::from_min_size(GPos::new(80.0, 80.0), Vec2::new(300.0, 200.0)),
        );
        g.frame_mut(f).unwrap().shrink = false;
        g.set_node_frame(n, Some(f));
        (g, f, n)
    }

    fn no_fit(_: NodeId) -> Rect {
        Rect::from_min_size(GPos::new(0.0, 0.0), Vec2::new(0.0, 0.0))
    }

    /// Moving a frame takes its members with it. A box that slides off
    /// its own contents is the single most confusing thing a group can
    /// do.
    #[test]
    fn moving_a_frame_moves_what_is_inside_it() {
        let (mut g, f, n) = graph_with_frame();
        let before = g.get_node_info(n).unwrap().pos;
        move_frame(&mut g, f, Vec2::new(50.0, -20.0));
        let after = g.get_node_info(n).unwrap().pos;
        assert!((after.x - before.x - 50.0).abs() < 0.01);
        assert!((after.y - before.y + 20.0).abs() < 0.01);
        let b = g.frame(f).unwrap().bounds;
        assert!((b.min.x - 130.0).abs() < 0.01);
    }

    /// Only the title band grabs the frame. If the whole box did, every
    /// node inside would become undraggable.
    #[test]
    fn only_the_title_band_grabs_the_frame() {
        let (mut g, f, _) = graph_with_frame();
        let spec = GraphSpec::from_surface(Color32::from_gray(30), Color32::WHITE, true);
        let placed = place_frames(&mut g, Camera::default(), &spec, &no_fit);
        let pf = placed.iter().find(|p| p.id == f).unwrap();
        assert_eq!(band_at(&placed, pf.band.center()), Some(f));
        let deep = Pos2::new(pf.rect.center().x, pf.rect.max.y - 10.0);
        assert_eq!(band_at(&placed, deep), None);
        assert_eq!(frame_at(&placed, deep), Some(f));
    }
}
