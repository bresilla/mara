//! A reusable tiling layout: a split-tree that resolves to named cell
//! rects.
//!
//! This is shared, deliberately, by two independent features:
//!
//! * [`MultiView`](crate::MultiView) — split one *view* into several
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

    /// Resolve this layout against `rect` into `(cell, rect)` pairs.
    #[must_use]
    pub fn resolve(&self, rect: Rect) -> Vec<(CellId, Rect)> {
        let mut out = Vec::new();
        resolve_into(self, rect, &mut out);
        out
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
