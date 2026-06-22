//! `mara_board` — a pure pixel-drawing **Board** surface for Mara.
//!
//! A Board is an id'd canvas region you draw raw paint primitives into
//! (rect, text, ellipse, arc, image, …) and read pointer input back from
//! — no widgets, no freehand sketching. It is the leaf surface of the
//! (planned) multiview view layer: an external driver (e.g. an ISOBUS
//! virtual terminal that decodes its own object pool) composes a screen
//! by drawing into one or more Boards. Mara stays GUI-only — it owns the
//! surface and the draw calls, not the data model.

use mara_core::{
    MaraPainter, MaraResponse, MaraUi, MaraView, RibbonAvoidance, ViewCtx, ViewId,
    vocab::{Align2, Color32, Pos2, Rect, Stroke, Vec2},
};

/// A pixel-drawing surface keyed by id.
///
/// Wraps [`MaraUi::canvas_at`] and hands back its painter + response so a
/// caller draws raw [`mara_core::PaintCmd`]-level primitives and hit-tests
/// pointer input itself. This is the leaf the multiview layout places.
#[derive(Clone, Copy, Debug)]
pub struct Board {
    id: ViewId,
}

impl Board {
    #[must_use]
    pub fn new(id: impl Into<ViewId>) -> Self {
        Self { id: id.into() }
    }

    #[must_use]
    pub fn id(&self) -> ViewId {
        self.id
    }

    /// Acquire the board's drawing surface at `rect`: a painter to draw
    /// into and a response carrying the pointer/click for hit-testing.
    pub fn surface(
        &self,
        mui: &mut MaraUi<'_>,
        rect: impl Into<Rect>,
    ) -> (MaraPainter, MaraResponse) {
        mui.canvas_at(rect.into())
    }
}

/// A multiview demo: one central Board flanked by columns of small
/// VT-style "soft-key" Boards on each side. Every region is an
/// independent [`Board`] surface drawn into separately — proving a view
/// can be split into many addressable pixel surfaces (the shape an
/// ISOBUS virtual terminal needs: a data-mask board + soft-key boards).
pub struct BoardView {
    id: &'static str,
    title: String,
}

impl BoardView {
    #[must_use]
    pub fn new(id: &'static str, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
        }
    }
}

impl MaraView for BoardView {
    fn id(&self) -> ViewId {
        ViewId::new(self.id)
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn icon(&self) -> &'static str {
        "square-multiple"
    }

    /// Unlike the canvas/map views (which paint full-bleed *behind* the
    /// ribbons), a Board view shrinks to sit *inside* all the ribbons —
    /// the top bar and the left/right/bottom rails are never drawn over
    /// it. Panes still float over it (they're toggleable). The content
    /// area comes from [`ViewCtx::content_rect`], which already insets by
    /// this avoidance.
    fn content_avoidance(&self) -> RibbonAvoidance {
        RibbonAvoidance::all()
    }

    fn show(&mut self, ctx: &mut ViewCtx<'_>) {
        let area = ctx.content_rect();
        ctx.body(|mui| {
            let accent = mui.accent();
            // Backdrop for the content area (the gaps between boards).
            mui.painter()
                .rect_filled(area, 0, mara_core::style::theme().palette.bg_window);

            let pad = 10.0;
            let col_w = 120.0;
            let keys = 5;
            let inner = Rect::from_min_max(
                Pos2::new(area.left() + pad, area.top() + pad),
                Pos2::new(area.right() - pad, area.bottom() - pad),
            );
            let left_col =
                Rect::from_min_max(inner.min, Pos2::new(inner.left() + col_w, inner.bottom()));
            let right_col =
                Rect::from_min_max(Pos2::new(inner.right() - col_w, inner.top()), inner.max);
            let center = Rect::from_min_max(
                Pos2::new(left_col.right() + pad, inner.top()),
                Pos2::new(right_col.left() - pad, inner.bottom()),
            );

            // Soft-key Boards down each side — each its own surface.
            for (col, side) in [(left_col, "L"), (right_col, "R")] {
                let kh = (col.height() - pad * (keys as f32 - 1.0)) / keys as f32;
                for i in 0..keys {
                    let top = col.top() + i as f32 * (kh + pad);
                    let kr = Rect::from_min_max(
                        Pos2::new(col.left(), top),
                        Pos2::new(col.right(), top + kh),
                    );
                    let key = Board::new(ViewId::new(("board.softkey", side, i)));
                    let (painter, response) = key.surface(mui, kr);
                    draw_soft_key(
                        &painter,
                        kr,
                        &format!("{side}{}", i + 1),
                        accent,
                        response.hovered,
                    );
                }
            }

            // The big central Board.
            let data_mask = Board::new(ViewId::new("board.data_mask"));
            let (painter, _response) = data_mask.surface(mui, center);
            draw_center_board(&painter, center, accent);
        });
    }
}

/// One VT-style soft key: rounded fill + border + label, highlighted on
/// hover so it reads as independently targetable.
fn draw_soft_key(painter: &MaraPainter, rect: Rect, label: &str, accent: Color32, hovered: bool) {
    let bg = if hovered {
        accent
    } else {
        Color32::from_rgb(40, 46, 58)
    };
    painter.rect_filled(rect, 8, bg);
    painter.rect_stroke(rect, 8, Stroke::new(1.5, Color32::from_gray(90)));
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        16.0,
        Color32::WHITE,
    );
}

/// The central Board's content — a framed surface with a big gauge built
/// from the new arc primitive, plus a sample ellipse.
fn draw_center_board(painter: &MaraPainter, area: Rect, accent: Color32) {
    painter.rect_filled(area, 10, Color32::from_rgb(24, 28, 36));
    painter.rect_stroke(area, 10, Stroke::new(1.5, Color32::from_gray(80)));
    painter.text(
        Pos2::new(area.center().x, area.top() + 18.0),
        Align2::CENTER_TOP,
        "center board",
        14.0,
        Color32::from_gray(160),
    );

    let c = area.center();
    let r = (area.width().min(area.height()) * 0.30).max(24.0);
    let deg = |d: f32| d.to_radians();
    let (a0, sweep, value) = (deg(135.0), deg(270.0), 0.62_f32);
    painter.arc(
        c,
        Vec2::new(r, r),
        a0,
        a0 + sweep,
        Stroke::new(12.0, Color32::from_gray(70)),
    );
    painter.arc(
        c,
        Vec2::new(r, r),
        a0,
        a0 + sweep * value,
        Stroke::new(12.0, accent),
    );
    let na = a0 + sweep * value;
    painter.line_segment(
        c,
        Pos2::new(c.x + r * 0.82 * na.cos(), c.y + r * 0.82 * na.sin()),
        Stroke::new(3.5, Color32::WHITE),
    );

    // A sample ellipse below the gauge.
    let er = Rect::from_min_size(
        Pos2::new(c.x - 60.0, area.bottom() - 70.0),
        Vec2::new(120.0, 44.0),
    );
    painter.ellipse_filled(er, Color32::from_rgb(70, 110, 200));
    painter.ellipse_stroke(er, Stroke::new(2.0, Color32::WHITE));
}
