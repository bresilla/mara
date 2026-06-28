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
//! Boards instead is a separate concern — that is `mara_core::MultiView`.
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
        let rect = ctx.content_rect();
        ctx.body(|mui| {
            let accent = mui.accent();
            let (painter, response) = mui.canvas_at(rect);
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
