//! The recursive view tree: one node type for a tab's content.
//!
//! A [`ViewNode`] is either a **Leaf** (any [`MaraView`] — a Board,
//! canvas, map, … — that draws content) or a **Split** (pure structure:
//! it divides its rect by a [`Layout`] and hosts one child node per
//! cell). This replaces the old leaf-vs-`MultiView` duality: a single
//! full-screen view is a Leaf, a tiled view is a Split of Leaves, and a
//! cell can itself be a Split, so the tree nests freely (PLAN.md Phase 2
//! / ADR 0001).
//!
//! `Split` draws nothing itself — only Leaves render. Each cell owns its
//! own [`WorkspaceStack`], so a Leaf can fullscreen a module inside its
//! own cell, and each child renders scoped to its cell rect via
//! [`ViewCtx::__internal_scoped`].

use std::hash::Hash;

use crate::vocab::Id as MaraId;
use crate::workspace::WorkspaceStack;

use super::{CellId, Layout, MaraView, ViewCtx};

/// A node in a tab's content tree — a content Leaf or a structural
/// Split. Opaque: construct with [`ViewNode::leaf`] / [`ViewNode::split`]
/// and compose with [`ViewNode::cell`].
pub struct ViewNode {
    kind: ViewNodeKind,
}

enum ViewNodeKind {
    /// A content view (any [`MaraView`]) that draws into its region.
    Leaf(Box<dyn MaraView>),
    /// A structural split: divides its rect by `layout` and renders one
    /// child node per cell. Draws nothing of its own.
    Split {
        salt: MaraId,
        layout: Layout,
        margin: f32,
        cells: Vec<ViewCell>,
    },
}

/// One structural cell of a split: a child node plus the workspace stack
/// that child (if a Leaf) renders against.
struct ViewCell {
    id: CellId,
    node: ViewNode,
    workspace: WorkspaceStack,
}

impl ViewNode {
    /// A leaf node wrapping any content view.
    #[must_use]
    pub fn leaf(view: impl MaraView + 'static) -> Self {
        Self {
            kind: ViewNodeKind::Leaf(Box::new(view)),
        }
    }

    /// A leaf node from an already-boxed content view.
    #[must_use]
    pub fn leaf_boxed(view: Box<dyn MaraView>) -> Self {
        Self {
            kind: ViewNodeKind::Leaf(view),
        }
    }

    /// Start a split node with the given cell layout. `salt` disambiguates
    /// this split's per-cell workspace ids from other splits. Attach
    /// children with [`ViewNode::cell`].
    #[must_use]
    pub fn split(salt: impl Hash, layout: Layout) -> Self {
        Self {
            kind: ViewNodeKind::Split {
                salt: MaraId::new(salt),
                layout,
                margin: 0.0,
                cells: Vec::new(),
            },
        }
    }

    /// Inset the cells from the split's edges by `margin` points. No-op on
    /// a leaf.
    #[must_use]
    pub fn margin(mut self, margin: f32) -> Self {
        if let ViewNodeKind::Split { margin: m, .. } = &mut self.kind {
            *m = margin;
        }
        self
    }

    /// Put a child node in the named cell of this split. The id must match
    /// a cell in the split's layout; cells absent from the layout are not
    /// shown. No-op on a leaf.
    #[must_use]
    pub fn cell(mut self, id: CellId, node: ViewNode) -> Self {
        if let ViewNodeKind::Split { salt, cells, .. } = &mut self.kind {
            let workspace = WorkspaceStack::new(MaraId::new((*salt, "cell", id)));
            cells.push(ViewCell {
                id,
                node,
                workspace,
            });
        }
        self
    }

    /// The ids of this split's direct cells (empty for a leaf), in order.
    /// Lets a host introspect the tree for runtime split/unsplit.
    #[must_use]
    pub fn cell_ids(&self) -> Vec<CellId> {
        match &self.kind {
            ViewNodeKind::Leaf(_) => Vec::new(),
            ViewNodeKind::Split { cells, .. } => cells.iter().map(|c| c.id).collect(),
        }
    }

    /// Runtime split: add a child in the named cell of this split (no-op /
    /// `false` on a leaf, or if the cell id is already present). Returns
    /// whether the cell was added.
    pub fn push_cell(&mut self, id: CellId, node: ViewNode) -> bool {
        if let ViewNodeKind::Split { salt, cells, .. } = &mut self.kind {
            if cells.iter().any(|c| c.id == id) {
                return false;
            }
            let workspace = WorkspaceStack::new(MaraId::new((*salt, "cell", id)));
            cells.push(ViewCell {
                id,
                node,
                workspace,
            });
            return true;
        }
        false
    }

    /// Runtime unsplit: remove the named cell (and its child subtree) from
    /// this split. Returns whether a cell was removed.
    pub fn remove_cell(&mut self, id: CellId) -> bool {
        if let ViewNodeKind::Split { cells, .. } = &mut self.kind {
            let before = cells.len();
            cells.retain(|c| c.id != id);
            return cells.len() != before;
        }
        false
    }

    /// Replace the child node in the named cell, keeping the cell's
    /// workspace stack. Returns whether the cell existed.
    pub fn replace_cell(&mut self, id: CellId, node: ViewNode) -> bool {
        if let ViewNodeKind::Split { cells, .. } = &mut self.kind
            && let Some(cell) = cells.iter_mut().find(|c| c.id == id)
        {
            cell.node = node;
            return true;
        }
        false
    }

    /// Render this node into `ctx`'s region. A Leaf draws its content; a
    /// Split resolves its layout over the region and renders each child
    /// scoped to its cell rect.
    pub fn render(&mut self, ctx: &mut ViewCtx<'_>) {
        match &mut self.kind {
            ViewNodeKind::Leaf(view) => {
                // A leaf owns its ribbons: render them at its region edges,
                // deliver their clicks to the view, then draw its content
                // (which `content_rect` insets away from those ribbons).
                let ribbons = view.ribbons();
                for click in ctx.show_ribbons(&ribbons) {
                    view.on_ribbon_click(&click.action);
                }
                view.show(ctx);
            }
            ViewNodeKind::Split {
                layout,
                margin,
                cells,
                ..
            } => {
                let rects = layout.resolve(ctx.content_rect().shrink(*margin));
                let accent = ctx.accent;
                for cell in cells.iter_mut() {
                    let Some((_, rect)) = rects.iter().find(|(id, _)| *id == cell.id).copied()
                    else {
                        continue;
                    };
                    let mut child = ctx.__internal_scoped(rect, &mut cell.workspace, accent);
                    cell.node.render(&mut child);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::ViewId;

    struct DummyView(ViewId);
    impl MaraView for DummyView {
        fn id(&self) -> ViewId {
            self.0
        }
        fn title(&self) -> &str {
            "dummy"
        }
        fn icon(&self) -> &'static str {
            "square"
        }
        fn show(&mut self, _ctx: &mut ViewCtx<'_>) {}
    }

    fn leaf(id: &'static str) -> ViewNode {
        ViewNode::leaf(DummyView(ViewId::new(id)))
    }

    #[test]
    fn split_runtime_split_and_unsplit() {
        let layout = Layout::row(
            4.0,
            vec![(1.0, Layout::cell("a")), (1.0, Layout::cell("b"))],
        );
        let mut root = ViewNode::split("root", layout).cell("a", leaf("a"));
        assert_eq!(root.cell_ids(), vec!["a"]);

        assert!(root.push_cell("b", leaf("b")));
        assert!(
            !root.push_cell("b", leaf("b2")),
            "duplicate cell id is rejected"
        );
        assert_eq!(root.cell_ids(), vec!["a", "b"]);

        assert!(root.replace_cell("a", leaf("a2")));
        assert!(
            !root.replace_cell("z", leaf("z")),
            "missing cell not replaced"
        );

        assert!(root.remove_cell("a"));
        assert_eq!(root.cell_ids(), vec!["b"]);
        assert!(!root.remove_cell("a"), "already removed");
    }

    #[test]
    fn leaf_tree_mutations_are_noops() {
        let mut node = leaf("x");
        assert!(node.cell_ids().is_empty());
        assert!(!node.push_cell("a", leaf("a")));
        assert!(!node.remove_cell("a"));
        assert!(!node.replace_cell("a", leaf("a")));
    }
}
