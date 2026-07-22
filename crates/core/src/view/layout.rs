//! A reusable tiling layout: a split-tree that resolves to named cell
//! rects.
//!
//! This is shared, deliberately, by two independent features:
//!
//! * [`ViewNode`](crate::ViewNode) — split one *view* into several
//!   child views, one per cell.
//! * a Board's *internal* layout — split one drawing surface into cells
//!   the board draws into.
//!
//! It is pure geometry: given a rect, it hands back `(cell, rect)` pairs.
//! It knows nothing about views or drawing.

use crate::vocab::{Pos2, Rect};

/// Identifier for a cell in a [`Layout`].
pub type CellId = &'static str;

/// Direction of a [`Layout::Split`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitAxis {
    Horizontal,
    Vertical,
}

/// A tiling layout tree. Either a single named cell, or a split whose
/// children are sized by relative weight with `gap` points between them.
#[derive(Clone, Debug)]
pub enum Layout {
    Cell(CellId),
    Split {
        axis: SplitAxis,
        gap: f32,
        /// `(weight, child)` pairs.
        children: Vec<(f32, Layout)>,
    },
}

impl Layout {
    /// A single named cell.
    #[must_use]
    pub fn cell(id: CellId) -> Self {
        Layout::Cell(id)
    }

    /// A left-to-right split, children sized by weight.
    #[must_use]
    pub fn row(gap: f32, children: Vec<(f32, Layout)>) -> Self {
        Layout::Split {
            axis: SplitAxis::Horizontal,
            gap,
            children,
        }
    }

    /// A top-to-bottom split, children sized by weight.
    #[must_use]
    pub fn col(gap: f32, children: Vec<(f32, Layout)>) -> Self {
        Layout::Split {
            axis: SplitAxis::Vertical,
            gap,
            children,
        }
    }

    /// Set the weight of this split's `index`-th direct child (clamped to
    /// `>= 0`). Returns `false` if this is a cell or `index` is out of
    /// range. This is the data model behind a draggable splitter: a host
    /// adjusts child weights and the next `resolve` re-tiles.
    pub fn set_child_weight(&mut self, index: usize, weight: f32) -> bool {
        match self {
            Layout::Cell(_) => false,
            Layout::Split { children, .. } => match children.get_mut(index) {
                Some((w, _)) => {
                    *w = weight.max(0.0);
                    true
                }
                None => false,
            },
        }
    }

    /// Resolve this layout against `rect` into `(cell, rect)` pairs.
    #[must_use]
    pub fn resolve(&self, rect: Rect) -> Vec<(CellId, Rect)> {
        let mut out = Vec::new();
        resolve_into(self, rect, &mut out);
        out
    }
}

/// One draggable boundary between two adjacent children of a `Split`,
/// resolved against a concrete rect: everything needed to hit-test the
/// divider and convert a dragged pointer position back into the two
/// children's weights (the interactive face of the splitter model).
#[derive(Clone, Debug)]
pub(crate) struct SplitDivider {
    /// Child indices from the layout root down to the owning `Split`.
    pub path: Vec<usize>,
    /// Boundary between child `boundary` and `boundary + 1`.
    pub boundary: usize,
    /// `true` when the divider line runs vertically (side-by-side cells).
    pub vertical_line: bool,
    /// Screen-space pointer hit strip centered on the shared edge.
    pub strip: Rect,
    /// Axis start of the first child of the pair.
    pub span_min: f32,
    /// Axis end of the second child of the pair.
    pub span_max: f32,
    /// Px available to ALL children on the axis (rect minus gaps), and
    /// the split's total weight — the px→weight conversion factors as of
    /// resolve time.
    pub avail: f32,
    pub total_weight: f32,
}

/// Half-width of a divider's pointer hit strip, in points.
pub(crate) const DIVIDER_HIT_HALF_WIDTH: f32 = 4.0;

impl Layout {
    /// Every draggable divider in this layout resolved against `rect`,
    /// including nested splits.
    #[must_use]
    pub(crate) fn dividers(&self, rect: Rect) -> Vec<SplitDivider> {
        let mut out = Vec::new();
        dividers_into(self, rect, &mut Vec::new(), &mut out);
        out
    }

    /// Move `divider` so its line sits at axis position `pos`,
    /// re-weighting the two adjacent children. Cells never shrink below
    /// a minimum. Returns whether the weights changed.
    pub(crate) fn set_divider_pos(&mut self, divider: &SplitDivider, pos: f32) -> bool {
        const MIN_CELL: f32 = 48.0;
        let mut node = self;
        for &idx in &divider.path {
            let Layout::Split { children, .. } = node else {
                return false;
            };
            let Some((_, child)) = children.get_mut(idx) else {
                return false;
            };
            node = child;
        }
        let Layout::Split { gap, children, .. } = node else {
            return false;
        };
        let i = divider.boundary;
        if i + 1 >= children.len() || divider.avail <= 0.0 {
            return false;
        }
        let pair = (divider.span_max - divider.span_min - *gap).max(0.0);
        if pair <= MIN_CELL * 2.0 {
            return false;
        }
        let left = (pos - *gap * 0.5 - divider.span_min).clamp(MIN_CELL, pair - MIN_CELL);
        let right = pair - left;
        children[i].0 = left / divider.avail * divider.total_weight;
        children[i + 1].0 = right / divider.avail * divider.total_weight;
        true
    }
}

fn dividers_into(node: &Layout, rect: Rect, path: &mut Vec<usize>, out: &mut Vec<SplitDivider>) {
    let Layout::Split {
        axis,
        gap,
        children,
    } = node
    else {
        return;
    };
    if children.is_empty() {
        return;
    }
    let total_weight = children
        .iter()
        .map(|(w, _)| w.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    let total_gap = gap * (children.len() - 1) as f32;
    let avail = match axis {
        SplitAxis::Horizontal => (rect.width() - total_gap).max(0.0),
        SplitAxis::Vertical => (rect.height() - total_gap).max(0.0),
    };
    let sizes: Vec<f32> = children
        .iter()
        .map(|(weight, _)| avail * (weight.max(0.0) / total_weight))
        .collect();
    let mut cursor = match axis {
        SplitAxis::Horizontal => rect.left(),
        SplitAxis::Vertical => rect.top(),
    };
    for (i, (_, child)) in children.iter().enumerate() {
        let size = sizes[i];
        let child_rect = match axis {
            SplitAxis::Horizontal => Rect::from_min_max(
                Pos2::new(cursor, rect.top()),
                Pos2::new(cursor + size, rect.bottom()),
            ),
            SplitAxis::Vertical => Rect::from_min_max(
                Pos2::new(rect.left(), cursor),
                Pos2::new(rect.right(), cursor + size),
            ),
        };
        if i + 1 < children.len() {
            let line = cursor + size + gap * 0.5;
            let strip = match axis {
                SplitAxis::Horizontal => Rect::from_min_max(
                    Pos2::new(line - DIVIDER_HIT_HALF_WIDTH, rect.top()),
                    Pos2::new(line + DIVIDER_HIT_HALF_WIDTH, rect.bottom()),
                ),
                SplitAxis::Vertical => Rect::from_min_max(
                    Pos2::new(rect.left(), line - DIVIDER_HIT_HALF_WIDTH),
                    Pos2::new(rect.right(), line + DIVIDER_HIT_HALF_WIDTH),
                ),
            };
            out.push(SplitDivider {
                path: path.clone(),
                boundary: i,
                vertical_line: matches!(axis, SplitAxis::Horizontal),
                strip,
                span_min: cursor,
                span_max: cursor + size + gap + sizes[i + 1],
                avail,
                total_weight,
            });
        }
        path.push(i);
        dividers_into(child, child_rect, path, out);
        path.pop();
        cursor += size + gap;
    }
}

fn resolve_into(node: &Layout, rect: Rect, out: &mut Vec<(CellId, Rect)>) {
    match node {
        Layout::Cell(id) => out.push((*id, rect)),
        Layout::Split {
            axis,
            gap,
            children,
        } => {
            if children.is_empty() {
                return;
            }
            let total_weight = children
                .iter()
                .map(|(w, _)| w.max(0.0))
                .sum::<f32>()
                .max(f32::EPSILON);
            let total_gap = gap * (children.len() - 1) as f32;
            let avail = match axis {
                SplitAxis::Horizontal => (rect.width() - total_gap).max(0.0),
                SplitAxis::Vertical => (rect.height() - total_gap).max(0.0),
            };
            let mut cursor = match axis {
                SplitAxis::Horizontal => rect.left(),
                SplitAxis::Vertical => rect.top(),
            };
            for (weight, child) in children {
                let size = avail * (weight.max(0.0) / total_weight);
                let child_rect = match axis {
                    SplitAxis::Horizontal => Rect::from_min_max(
                        Pos2::new(cursor, rect.top()),
                        Pos2::new(cursor + size, rect.bottom()),
                    ),
                    SplitAxis::Vertical => Rect::from_min_max(
                        Pos2::new(rect.left(), cursor),
                        Pos2::new(rect.right(), cursor + size),
                    ),
                };
                resolve_into(child, child_rect, out);
                cursor += size + gap;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_cell_fills_the_rect() {
        let rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(100.0, 60.0));
        let cells = Layout::cell("only").resolve(rect);
        assert_eq!(cells, vec![("only", rect)]);
    }

    #[test]
    fn row_split_divides_width_by_weight_with_gaps() {
        // weights 1:3, 10pt gap across 210pt → usable 200 → 50 / 150.
        let rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(210.0, 40.0));
        let cells = Layout::row(
            10.0,
            vec![(1.0, Layout::cell("a")), (3.0, Layout::cell("b"))],
        )
        .resolve(rect);
        assert_eq!(cells.len(), 2);
        assert!((cells[0].1.width() - 50.0).abs() < 0.01);
        assert!((cells[1].1.width() - 150.0).abs() < 0.01);
        assert!((cells[1].1.left() - 60.0).abs() < 0.01);
    }

    #[test]
    fn set_child_weight_retiles_the_split() {
        // Start 1:1 across 200pt (no gap) → 100 / 100.
        let rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(200.0, 40.0));
        let mut layout = Layout::row(
            0.0,
            vec![(1.0, Layout::cell("a")), (1.0, Layout::cell("b"))],
        );
        assert!((layout.resolve(rect)[0].1.width() - 100.0).abs() < 0.01);

        // Drag the splitter → weights 3:1 → 150 / 50.
        assert!(layout.set_child_weight(0, 3.0));
        let cells = layout.resolve(rect);
        assert!((cells[0].1.width() - 150.0).abs() < 0.01);
        assert!((cells[1].1.width() - 50.0).abs() < 0.01);

        // Out-of-range and cell targets are rejected.
        assert!(!layout.set_child_weight(9, 1.0));
        assert!(!Layout::cell("x").set_child_weight(0, 1.0));
    }

    #[test]
    fn nested_split_resolves_recursively() {
        let rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(100.0, 100.0));
        let cells = Layout::row(
            0.0,
            vec![
                (
                    1.0,
                    Layout::col(
                        0.0,
                        vec![(1.0, Layout::cell("tl")), (1.0, Layout::cell("bl"))],
                    ),
                ),
                (1.0, Layout::cell("right")),
            ],
        )
        .resolve(rect);
        assert_eq!(cells.len(), 3);
        assert!((cells[0].1.height() - 50.0).abs() < 0.01);
        assert!((cells[1].1.top() - 50.0).abs() < 0.01);
        assert!((cells[2].1.left() - 50.0).abs() < 0.01);
        assert!((cells[2].1.height() - 100.0).abs() < 0.01);
    }
}
