//! Layout introspection ("pose") probe.
//!
//! When enabled, the backend records an [`ElementPose`] — id, label,
//! rect (position + size), and interaction state — for every element it
//! lays out, plus key global rects (screen / content / chrome bounds /
//! pane placement). A host can then dump the whole frame's layout to
//! the terminal as text via [`format`].
//!
//! This is a debugging/inspection facility, not a render path. Because
//! the UI now routes allocation/interaction through
//! [`crate::layout::UiBackend`], recording at that seam captures the
//! real layout of every widget without each widget opting in.
//!
//! The ctx-backed storage + recording live in the egui backend
//! (`backend::egui::probe_*`); this module owns the backend-neutral
//! data type and the text formatter so they can be reused by any
//! backend and inspected in tests without egui.

use crate::vocab::{Id, Rect};

/// One recorded element: where it is, how big, and its live state.
#[derive(Clone, Debug, PartialEq)]
pub struct ElementPose {
    /// What kind of element this is (e.g. "area", "alloc", "interact",
    /// "global", "pane"). A stable, human-readable tag.
    pub kind: &'static str,
    /// Optional human label (widget name, area id text, etc.).
    pub label: String,
    /// Stable id, when the element had one.
    pub id: Option<Id>,
    /// Position + size in screen-space points.
    pub rect: Rect,
    /// Nesting depth for indentation in the dump (0 = top level).
    pub depth: usize,
    /// Whether the element is interactive (allocated with a sense).
    pub interactive: bool,
    /// Live interaction state this frame.
    pub hovered: bool,
    pub clicked: bool,
}

impl ElementPose {
    #[must_use]
    pub fn new(kind: &'static str, rect: Rect) -> Self {
        Self {
            kind,
            label: String::new(),
            id: None,
            rect,
            depth: 0,
            interactive: false,
            hovered: false,
            clicked: false,
        }
    }

    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    #[must_use]
    pub fn with_id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    #[must_use]
    pub fn with_depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }

    #[must_use]
    pub fn interactive(mut self, hovered: bool, clicked: bool) -> Self {
        self.interactive = true;
        self.hovered = hovered;
        self.clicked = clicked;
        self
    }
}

/// Render a recorded frame's poses as an aligned, indented text block.
#[must_use]
pub fn format(poses: &[ElementPose]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "=== MARA POSE DUMP ({} elements) ===\n",
        poses.len()
    ));
    if poses.is_empty() {
        out.push_str("(nothing recorded — probe enabled but no elements laid out)\n");
        return out;
    }
    for p in poses {
        let indent = "  ".repeat(p.depth);
        let r = p.rect;
        let geom = format!(
            "x={:>7.1} y={:>7.1}  {:>7.1}x{:<7.1}",
            r.min.x,
            r.min.y,
            r.max.x - r.min.x,
            r.max.y - r.min.y,
        );
        let mut state = String::new();
        if p.interactive {
            state.push_str(" [interactive");
            if p.hovered {
                state.push_str(" hovered");
            }
            if p.clicked {
                state.push_str(" clicked");
            }
            state.push(']');
        }
        let label = if p.label.is_empty() {
            String::new()
        } else {
            format!(" {}", p.label)
        };
        out.push_str(&format!(
            "{indent}[{kind}]{label}  {geom}{state}\n",
            kind = p.kind,
        ));
    }
    out
}

// ─── Host hooks ─────────────────────────────────────────────────────
//
// First-party host adapters (the window runner, Bevy plugin) drive the
// probe through these. They take a raw `egui::Context` because the host
// owns the frame; ordinary app code does not need them.

/// Enable (fresh log) or disable pose recording for this frame. Call
/// before running the UI.
#[doc(hidden)]
pub fn __internal_set_enabled(ctx: &dyn crate::context::MaraCtx, on: bool) {
    ctx.probe_set_enabled(on);
}

/// Drain and return the poses recorded this frame.
#[doc(hidden)]
#[must_use]
pub fn __internal_drain(ctx: &dyn crate::context::MaraCtx) -> Vec<ElementPose> {
    ctx.probe_drain()
}

/// Record a labeled global/structural pose (used by first-party
/// layout code, e.g. the pane placer, to surface key rects).
#[doc(hidden)]
pub fn __internal_record(ctx: &dyn crate::context::MaraCtx, pose: ElementPose) {
    ctx.probe_record(pose);
}

/// Whether the probe is currently capturing this frame.
#[doc(hidden)]
#[must_use]
pub fn __internal_enabled(ctx: &dyn crate::context::MaraCtx) -> bool {
    ctx.probe_enabled()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::{Pos2, Vec2};

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h))
    }

    #[test]
    fn format_lists_elements_with_geometry() {
        let poses = vec![
            ElementPose::new("global", rect(0.0, 0.0, 1280.0, 800.0)).with_label("screen"),
            ElementPose::new("area", rect(8.0, 8.0, 320.0, 600.0))
                .with_label("pane:left")
                .with_depth(1),
            ElementPose::new("interact", rect(20.0, 20.0, 100.0, 24.0))
                .with_depth(2)
                .interactive(true, false),
        ];
        let text = format(&poses);
        assert!(text.contains("3 elements"));
        assert!(text.contains("screen"));
        assert!(text.contains("pane:left"));
        assert!(text.contains("1280.0x800.0"));
        assert!(text.contains("[interactive hovered]"));
    }

    #[test]
    fn format_handles_empty() {
        assert!(format(&[]).contains("nothing recorded"));
    }
}
