//! `mara_board` — a **Board**: a pixel-drawing surface module for Mara.
//!
//! A Board is the drawing counterpart to `mara_canvas`: where the canvas
//! captures freehand strokes, a Board lets *you* draw raw paint
//! primitives (`rect`, `text`, `ellipse`, `arc`, `sector`, `image`, …)
//! and read pointer input back. No widgets. It is a peer module to
//! canvas / code / image / graph — a top-level [`MaraView`] and an
//! embeddable [`MaraModule`].
//!
//! A Board can carry its own **internal layout** ([`mara_core::Layout`])
//! — splitting itself into named cells the draw callback fills. That is
//! enough to build an entire ISOBUS virtual terminal *inside one Board*
//! (a data-mask cell + soft-key cells). Splitting across *several*
//! Boards instead is a separate concern — that is `mara_core::ViewNode`.
//!
//! Mara stays GUI-only: a Board owns the surface and the draw calls, not
//! the data model. The consumer (e.g. a VT driver that decoded an object
//! pool elsewhere) supplies the drawing via [`Board::on_draw`].

use mara_core::{
    CellId, Layout, MaraModule, MaraPainter, MaraResponse, MaraUi, MaraView, ModuleInlineCtx,
    ModuleResponse, RibbonAvoidance, ViewCtx, ViewId,
    vocab::{Color32, Rect, Vec2},
};

/// The drawing context handed to a Board's [`Board::on_draw`] callback.
///
/// `painter` draws into the board, `rect` is its bounds, `response`
/// carries this frame's pointer/click state, `accent` is the theme
/// accent, and `cells` are the board's internal layout regions (empty if
/// the board has no internal layout).
pub struct BoardPaint<'a> {
    pub painter: &'a MaraPainter,
    pub response: &'a MaraResponse,
    pub rect: Rect,
    pub accent: Color32,
    pub cells: &'a [(CellId, Rect)],
}

impl BoardPaint<'_> {
    /// The rect of an internal-layout cell by id.
    #[must_use]
    pub fn cell(&self, id: CellId) -> Option<Rect> {
        self.cells.iter().find(|(c, _)| *c == id).map(|(_, r)| *r)
    }
}

/// A pixel-drawing surface. Build it, optionally give it an internal
/// [`Layout`], and supply the drawing with [`Board::on_draw`].
pub struct Board {
    id: ViewId,
    title: String,
    icon: &'static str,
    avoidance: RibbonAvoidance,
    layout: Option<Layout>,
    draw: Box<dyn FnMut(BoardPaint)>,
}

impl Board {
    #[must_use]
    pub fn new(id: impl std::hash::Hash, title: impl Into<String>) -> Self {
        Self {
            id: ViewId::new(id),
            title: title.into(),
            icon: "square-multiple",
            avoidance: RibbonAvoidance::none(),
            layout: None,
            draw: Box::new(|_| {}),
        }
    }

    #[must_use]
    pub fn with_icon(mut self, icon: &'static str) -> Self {
        self.icon = icon;
        self
    }

    /// Make the board shrink inside the ribbons (vs. painting behind
    /// them). Pass [`RibbonAvoidance::all`] for an instrument-panel look.
    #[must_use]
    pub fn with_content_avoidance(mut self, avoidance: RibbonAvoidance) -> Self {
        self.avoidance = avoidance;
        self
    }

    /// Give the board an internal layout — its draw callback then sees
    /// the resolved cells via [`BoardPaint::cell`] / `BoardPaint::cells`.
    #[must_use]
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = Some(layout);
        self
    }

    /// Set the per-frame drawing callback.
    #[must_use]
    pub fn on_draw(mut self, draw: impl FnMut(BoardPaint) + 'static) -> Self {
        self.draw = Box::new(draw);
        self
    }

    fn paint(
        &mut self,
        painter: &MaraPainter,
        response: &MaraResponse,
        rect: Rect,
        accent: Color32,
    ) {
        let cells = self
            .layout
            .as_ref()
            .map(|layout| layout.resolve(rect))
            .unwrap_or_default();
        (self.draw)(BoardPaint {
            painter,
            response,
            rect,
            accent,
            cells: &cells,
        });
    }
}

impl MaraView for Board {
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
        // The board surface fills the WHOLE region, edge to edge — its
        // drawing is the view's backdrop and the per-view ribbons sit on
        // top of it, so the surface is never gapped from its own
        // buttons. The region painter is used because the body Ui is
        // clipped to the ribbon-avoiding content rect.
        let rect = ctx.screen_rect();
        let painter = ctx.painter();
        ctx.body(|mui| {
            let accent = mui.accent();
            let (_, response) = mui.canvas_at(rect);
            self.paint(&painter, &response, rect, accent);
        });
    }
}

impl MaraModule for Board {
    fn id(&self) -> mara_core::vocab::Id {
        self.id.0
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn icon(&self) -> &'static str {
        self.icon
    }

    fn inline(&mut self, mui: &mut MaraUi<'_>, _ctx: ModuleInlineCtx<'_>) -> ModuleResponse {
        let accent = mui.accent();
        let (painter, response) = mui.canvas(Vec2::new(220.0, 140.0));
        let rect = response.rect;
        self.paint(&painter, &response, rect, accent);
        ModuleResponse::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mara_core::vocab::{Pos2, Stroke};
    use std::cell::Cell;
    use std::rc::Rc;

    fn rect() -> Rect {
        Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(200.0, 120.0))
    }

    /// Portability proof: a Board draws only through [`MaraPainter`], so
    /// running its `on_draw` against a command-recording painter — with
    /// no egui `Ui`/`Context` anywhere — still yields `PaintCmd`s. That
    /// is exactly what a non-egui backend would rasterise.
    #[test]
    fn board_draw_emits_paint_commands_on_recording_painter() {
        let r = rect();
        let accent = Color32::from_rgb(120, 180, 255);
        let mut board = Board::new("test-board", "Test").on_draw(|b: BoardPaint| {
            b.painter
                .line_segment(b.rect.min, b.rect.max, Stroke::new(1.0, b.accent));
        });

        let painter = MaraPainter::__internal_recording(r);
        let response = MaraResponse::__internal_synthetic(r);
        board.paint(&painter, &response, r, accent);

        let cmds = painter.__internal_recorded_commands();
        assert!(
            !cmds.is_empty(),
            "board on_draw must emit PaintCmds — the backend-portability contract"
        );
    }

    /// Board's *use* of the shared [`Layout`]: a single named cell
    /// resolves to the full board rect and reaches `on_draw` via
    /// [`BoardPaint::cell`]. (Core's `Layout::resolve` geometry is tested
    /// in `mara_core`; this pins the board wiring around it.)
    #[test]
    fn board_layout_cell_reaches_on_draw() {
        let r = rect();
        let seen: Rc<Cell<Option<Rect>>> = Rc::new(Cell::new(None));
        let sink = Rc::clone(&seen);
        let mut board = Board::new("test-board", "Test")
            .with_layout(Layout::cell("only"))
            .on_draw(move |b: BoardPaint| {
                sink.set(b.cell("only"));
            });

        let painter = MaraPainter::__internal_recording(r);
        let response = MaraResponse::__internal_synthetic(r);
        board.paint(&painter, &response, r, Color32::from_rgb(0, 0, 0));

        assert_eq!(
            seen.get(),
            Some(r),
            "a single-cell layout should resolve to the whole board rect"
        );
    }
}
