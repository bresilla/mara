//! A multi-view: one view split into several child views.
//!
//! This is the generic "divide a view" primitive, independent of any
//! particular content. A [`MultiView`] holds a [`Layout`] and one child
//! [`MaraView`] per cell; each child is rendered scoped to its cell rect
//! (its own content area + workspace).
//!
//! A child is any view — a Board, a canvas, a map, even another
//! `MultiView`. So an instrument panel / VT is just a `MultiView` of
//! Boards: a big board in the middle, a board on each side.
//!
//! Children must render within their content area (via `ViewCtx::body`).
//! Views that take the whole window (`egui::CentralPanel`, or a GPU view
//! owning the swapchain) need adapting before they tile.

use crate::ribbon::RibbonAvoidance;
use crate::vocab::Id as MaraId;
use crate::workspace::WorkspaceStack;

use super::{CellId, Layout, MaraView, ViewCtx, ViewId};

struct Cell {
    id: CellId,
    view: Box<dyn MaraView>,
    workspace: WorkspaceStack,
}

/// A view whose content area is split into cells, each holding a child
/// view.
pub struct MultiView {
    id: ViewId,
    title: String,
    icon: &'static str,
    avoidance: RibbonAvoidance,
    margin: f32,
    layout: Layout,
    cells: Vec<Cell>,
}

impl MultiView {
    /// Create a multi-view with the given split layout. Attach a child
    /// view to each cell with [`MultiView::view`].
    #[must_use]
    pub fn new(id: impl Into<ViewId>, title: impl Into<String>, layout: Layout) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            icon: "grid",
            avoidance: RibbonAvoidance::none(),
            margin: 0.0,
            layout,
            cells: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_icon(mut self, icon: &'static str) -> Self {
        self.icon = icon;
        self
    }

    /// Inset the cells from the view edges by `margin` points — set it to
    /// the layout gap so the outer cells sit the same distance from the
    /// bars as they do from each other.
    #[must_use]
    pub fn with_margin(mut self, margin: f32) -> Self {
        self.margin = margin;
        self
    }

    /// Which ribbons the whole multi-view should avoid (see
    /// [`MaraView::content_avoidance`]).
    #[must_use]
    pub fn with_content_avoidance(mut self, avoidance: RibbonAvoidance) -> Self {
        self.avoidance = avoidance;
        self
    }

    /// Put a child view in the named cell. The id must match a cell in
    /// the layout; cells absent from the layout are not shown.
    #[must_use]
    pub fn view(mut self, cell: CellId, view: Box<dyn MaraView>) -> Self {
        let workspace = WorkspaceStack::new(MaraId::new(("mara.multiview.cell", self.id, cell)));
        self.cells.push(Cell {
            id: cell,
            view,
            workspace,
        });
        self
    }
}

impl MaraView for MultiView {
    fn id(&self) -> ViewId {
        self.id
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn icon(&self) -> &'static str {
        self.icon
    }

    fn content_avoidance(&self) -> RibbonAvoidance {
        self.avoidance
    }

    fn show(&mut self, ctx: &mut ViewCtx<'_>) {
        let rects = self.layout.resolve(ctx.content_rect().shrink(self.margin));
        let accent = ctx.accent;
        for cell in &mut self.cells {
            let Some((_, rect)) = rects.iter().find(|(id, _)| *id == cell.id).copied() else {
                continue;
            };
            let mut child = ctx.__internal_scoped(rect, &mut cell.workspace, accent);
            cell.view.show(&mut child);
        }
    }
}
