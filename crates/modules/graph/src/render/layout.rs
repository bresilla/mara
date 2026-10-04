//! Where everything goes, computed before anything is drawn.
//!
//! # Why this is a separate pass
//!
//! The renderer this replaces decided a node's size *from what got
//! painted into it*: the frame grew to fit whatever widgets the app's
//! viewer emitted, and the pin rows were laid out by the backend's own
//! layout engine as a side effect of drawing them. Three things
//! followed, and all three were reported as "it looks inconsistent":
//!
//! * two nodes with the same shape came out different widths, because
//!   one had an unwired input and therefore an inline editor;
//! * rows on neighbouring nodes did not line up, because each node's
//!   rhythm came from its own content;
//! * text escaped its box, because the truncation budget was computed
//!   from one padding and the box was painted with another.
//!
//! Deciding geometry up front from the node's *declared* shape — title,
//! pin counts, whether it has a body — makes all three impossible. Two
//! nodes with the same shape are the same size by construction, and the
//! painter cannot disagree with the layout because it is handed the
//! rects rather than computing its own.
//!
//! It is also the only reason any of this is testable without a window.
//!
//! This file is checked by `make check` to contain no backend types.

use mara_core::vocab::{Pos2, Rect, Vec2};

use super::spec::NodeSpec;

/// What a node declares about itself, before layout.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeShape {
    pub title: String,
    /// Second header line — what the node currently *is*, as opposed to
    /// what it is called. Empty centres the title instead.
    pub subtitle: String,
    /// Name of a bundled icon for the header, e.g. `"flowchart"`.
    pub icon: Option<String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    /// Extra height reserved under the pin rows for app-drawn content.
    pub body_h: f32,
}

impl Default for NodeShape {
    fn default() -> Self {
        Self {
            title: String::new(),
            subtitle: String::new(),
            icon: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
            body_h: 0.0,
        }
    }
}

impl NodeShape {
    /// How many rows of pins this node needs.
    ///
    /// Inputs and outputs share rows — the shape everyone expects from
    /// a node editor — so a node with two inputs and one output is two
    /// rows tall, not three.
    #[must_use]
    pub fn pin_rows(&self) -> usize {
        self.inputs.len().max(self.outputs.len())
    }
}

/// Where every part of one node ended up.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeLayout {
    /// The node's outer rect.
    pub rect: Rect,
    /// The header band, along the top.
    pub header: Rect,
    /// Where the header icon is centred. Zero-sized when there is none.
    pub icon: Rect,
    /// The title line inside the header.
    pub title: Rect,
    /// The subtitle line. Zero-height when the node has no subtitle.
    pub subtitle: Rect,
    /// The area under the header, holding pin rows and any body.
    pub content: Rect,
    /// Anchor point of each input pin, on the left edge.
    pub inputs: Vec<Pos2>,
    /// Anchor point of each output pin, on the right edge.
    pub outputs: Vec<Pos2>,
    /// Baseline rect for each input label, inside the body.
    pub input_labels: Vec<Rect>,
    /// Baseline rect for each output label.
    pub output_labels: Vec<Rect>,
    /// Where an unconnected input's value editor goes. Zero-width for
    /// rows that have no room for one.
    pub input_editors: Vec<Rect>,
    /// Rect reserved for app-drawn body content; empty when none.
    pub body: Rect,
    /// The metrics these rects were computed from.
    ///
    /// Carried along so the painter reads its paddings, corner radius
    /// and text sizes from the same numbers the layout used. Passing
    /// the two separately is how a zoomed layout ended up painted with
    /// unscaled text and pins.
    pub spec: NodeSpec,
}

/// Lay one node out at `pos`.
///
/// `pos` is the node's top-left in graph space. Height follows from the
/// shape; width never does.
#[must_use]
pub fn layout_node(pos: Pos2, shape: &NodeShape, spec: &NodeSpec) -> NodeLayout {
    let rows = shape.pin_rows();
    let height = spec.header_h
        + spec.pad_y
        + rows as f32 * spec.row_h
        + if shape.body_h > 0.0 { shape.body_h } else { 0.0 }
        + spec.pad_y;

    let rect = Rect::from_min_size(pos, Vec2::new(spec.width, height));
    let header = Rect::from_min_size(pos, Vec2::new(spec.width, spec.header_h));
    let content = Rect::from_min_max(
        Pos2::new(rect.min.x, header.max.y + spec.pad_y),
        Pos2::new(rect.max.x, rect.max.y - spec.pad_y),
    );

    // Header slots. The icon column is reserved whether or not there is
    // an icon, so titles start at the same x on every node — a column
    // of nodes where some titles are indented and some are not looks
    // accidental, however good each node is on its own.
    let gutter = spec.pad_x;
    let icon_col = spec.icon_size * 1.4;
    let icon = Rect::from_center_size(
        Pos2::new(header.min.x + gutter + icon_col * 0.5, header.center().y),
        Vec2::new(icon_col, icon_col),
    );
    let text_l = icon.max.x + gutter * 0.5;
    let text_r = header.max.x - gutter;
    let (title, subtitle) = if shape.subtitle.is_empty() {
        (
            Rect::from_min_max(
                Pos2::new(text_l, header.min.y),
                Pos2::new(text_r, header.max.y),
            ),
            Rect::from_min_max(
                Pos2::new(text_l, header.max.y),
                Pos2::new(text_r, header.max.y),
            ),
        )
    } else {
        let split = header.min.y + spec.header_h * 0.55;
        (
            Rect::from_min_max(Pos2::new(text_l, header.min.y + spec.pad_y * 0.4), Pos2::new(text_r, split)),
            Rect::from_min_max(Pos2::new(text_l, split), Pos2::new(text_r, header.max.y - spec.pad_y * 0.4)),
        )
    };

    let row_centre = |i: usize| content.min.y + (i as f32 + 0.5) * spec.row_h;

    // Pins sit ON the body edge — half inside, half out — so a wire
    // meets the node rather than stopping short of it or crossing into
    // it. Both columns use the same rows, which is what keeps a
    // node's two sides visually paired.
    let inputs: Vec<Pos2> = (0..shape.inputs.len())
        .map(|i| Pos2::new(rect.min.x, row_centre(i)))
        .collect();
    let outputs: Vec<Pos2> = (0..shape.outputs.len())
        .map(|i| Pos2::new(rect.max.x, row_centre(i)))
        .collect();

    // Each row is three zones: input label, editor band, output label.
    // The bands are at fixed offsets rather than sized to content, so a
    // node that grows an editor stays exactly as wide as one that does
    // not — the ragged-column problem, in miniature.
    let half = spec.row_h * 0.5;
    let inner_l = rect.min.x + spec.pad_x;
    let inner_r = rect.max.x - spec.pad_x;
    let editor_w = spec.width * spec.editor_frac;
    let editor_l = inner_r - editor_w;

    let band = |x0: f32, x1: f32, i: usize| {
        let y = row_centre(i);
        Rect::from_min_max(Pos2::new(x0, y - half), Pos2::new(x1.max(x0), y + half))
    };

    let (n_in, n_out) = (shape.inputs.len(), shape.outputs.len());
    let mut input_labels = Vec::with_capacity(n_in);
    let mut input_editors = Vec::with_capacity(n_in);
    let mut output_labels = Vec::with_capacity(n_out);

    for i in 0..rows {
        let has_in = i < n_in;
        let has_out = i < n_out;
        // An editor band is only reserved where an input could use it,
        // and never where an output label would have to share the row.
        let editor = has_in && !has_out;
        if has_in {
            let end = if editor {
                editor_l - spec.pad_x
            } else if has_out {
                rect.center().x
            } else {
                inner_r
            };
            input_labels.push(band(inner_l, end, i));
            input_editors.push(if editor {
                band(editor_l, inner_r, i)
            } else {
                band(inner_r, inner_r, i)
            });
        }
        if has_out {
            let start = if has_in { rect.center().x } else { inner_l };
            output_labels.push(band(start, inner_r, i));
        }
    }

    let body = if shape.body_h > 0.0 {
        Rect::from_min_max(
            Pos2::new(
                content.min.x + spec.pad_x,
                content.min.y + rows as f32 * spec.row_h,
            ),
            Pos2::new(content.max.x - spec.pad_x, content.max.y),
        )
    } else {
        Rect::from_min_size(content.min, Vec2::new(0.0, 0.0))
    };

    NodeLayout {
        rect,
        header,
        icon,
        title,
        subtitle,
        content,
        inputs,
        outputs,
        input_labels,
        output_labels,
        input_editors,
        body,
        spec: *spec,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> NodeSpec {
        NodeSpec::default()
    }

    fn shape(ins: usize, outs: usize) -> NodeShape {
        NodeShape {
            title: "node".into(),
            inputs: (0..ins).map(|i| format!("in{i}")).collect(),
            outputs: (0..outs).map(|i| format!("out{i}")).collect(),
            ..Default::default()
        }
    }

    /// The headline property. Width is a constant of the system, not a
    /// consequence of content — which is what makes a column of nodes
    /// line up instead of coming out ragged.
    #[test]
    fn every_node_is_the_same_width_whatever_its_shape() {
        let s = spec();
        let widths: Vec<f32> = [(0, 1), (1, 1), (3, 1), (2, 4), (0, 0)]
            .iter()
            .map(|(i, o)| layout_node(Pos2::new(0.0, 0.0), &shape(*i, *o), &s).rect.width())
            .collect();
        for w in &widths {
            assert!((w - s.width).abs() < 0.01, "got {w}, want {}", s.width);
        }
    }

    /// Two nodes with the same shape must be identical in every rect,
    /// wherever they sit. Anything else means geometry is leaking in
    /// from somewhere that is not the shape.
    #[test]
    fn identical_shapes_lay_out_identically() {
        let s = spec();
        let a = layout_node(Pos2::new(0.0, 0.0), &shape(2, 1), &s);
        let b = layout_node(Pos2::new(500.0, 300.0), &shape(2, 1), &s);
        let shift = Vec2::new(500.0, 300.0);

        assert_eq!(a.rect.size(), b.rect.size());
        for (pa, pb) in a.inputs.iter().zip(&b.inputs) {
            assert!((pa.x + shift.x - pb.x).abs() < 0.01);
            assert!((pa.y + shift.y - pb.y).abs() < 0.01);
        }
    }

    /// Rows share one rhythm, so a pin on one node aligns with a pin on
    /// its neighbour. If row spacing drifted with content, two nodes
    /// side by side would have their wires enter at different heights.
    #[test]
    fn pin_rows_share_one_pitch() {
        let s = spec();
        let l = layout_node(Pos2::new(0.0, 0.0), &shape(4, 0), &s);
        for pair in l.inputs.windows(2) {
            assert!((pair[1].y - pair[0].y - s.row_h).abs() < 0.01);
        }
    }

    /// Inputs and outputs share rows rather than stacking, so a 2-in
    /// 1-out node is two rows tall, not three.
    #[test]
    fn inputs_and_outputs_share_rows() {
        let s = spec();
        let stacked = layout_node(Pos2::new(0.0, 0.0), &shape(2, 1), &s);
        let two_rows = layout_node(Pos2::new(0.0, 0.0), &shape(2, 0), &s);
        assert!((stacked.rect.height() - two_rows.rect.height()).abs() < 0.01);
    }

    /// A label can never run into the pin column on the far side, and
    /// can never start outside the body — the two ways text escaped its
    /// box in the renderer this replaces.
    #[test]
    fn labels_stay_inside_the_body_and_never_overlap() {
        let s = spec();
        let l = layout_node(Pos2::new(0.0, 0.0), &shape(3, 3), &s);
        for (i, (li, lo)) in l.input_labels.iter().zip(&l.output_labels).enumerate() {
            assert!(li.min.x >= l.rect.min.x + s.pad_x - 0.01, "input {i} starts outside");
            assert!(lo.max.x <= l.rect.max.x - s.pad_x + 0.01, "output {i} ends outside");
            assert!(li.max.x <= lo.min.x + 0.01, "row {i} labels overlap");
        }
    }

    /// A subtitle must not make the header taller. Otherwise a row of
    /// nodes where only some carry a subtitle has a ragged top edge,
    /// which is the same complaint that started this rewrite.
    #[test]
    fn a_subtitle_does_not_change_the_header_height() {
        let s = spec();
        let plain = layout_node(Pos2::new(0.0, 0.0), &shape(1, 1), &s);
        let mut with_sub = shape(1, 1);
        with_sub.subtitle = "float".into();
        let subbed = layout_node(Pos2::new(0.0, 0.0), &with_sub, &s);

        assert!((plain.header.height() - subbed.header.height()).abs() < 0.01);
        assert!((plain.rect.height() - subbed.rect.height()).abs() < 0.01);
        assert!(subbed.subtitle.height() > 1.0, "subtitle has no room");
        assert!(subbed.subtitle.max.y <= subbed.header.max.y + 0.01);
        assert!(subbed.title.max.y <= subbed.subtitle.min.y + 0.01);
    }

    /// The icon column is reserved whether or not there is an icon, so
    /// titles line up down a column of mixed nodes.
    #[test]
    fn titles_start_at_the_same_x_with_or_without_an_icon() {
        let s = spec();
        let mut with_icon = shape(1, 1);
        with_icon.icon = Some("circle".into());
        let a = layout_node(Pos2::new(0.0, 0.0), &shape(1, 1), &s);
        let b = layout_node(Pos2::new(0.0, 0.0), &with_icon, &s);
        assert!((a.title.min.x - b.title.min.x).abs() < 0.01);
        assert!(a.title.min.x > a.rect.min.x + s.pad_x);
    }

    /// Header text stays inside the header, whatever it says.
    #[test]
    fn header_text_never_leaves_the_header() {
        let s = spec();
        let mut long = shape(2, 2);
        long.title = "An extremely long node title that will not fit".into();
        long.subtitle = "and a second line that is also far too long".into();
        long.icon = Some("circle".into());
        let l = layout_node(Pos2::new(0.0, 0.0), &long, &s);
        for r in [l.icon, l.title, l.subtitle] {
            assert!(r.min.y >= l.header.min.y - 0.01);
            assert!(r.max.y <= l.header.max.y + 0.01);
            assert!(r.max.x <= l.header.max.x - s.pad_x + 0.01);
        }
    }

    /// An editor band is reserved only where an input can actually use
    /// it, and never where it would sit under an output label.
    #[test]
    fn editor_bands_appear_only_on_rows_that_can_hold_one() {
        let s = spec();
        let l = layout_node(Pos2::new(0.0, 0.0), &shape(3, 1), &s);
        assert!(
            l.input_editors[0].width() < 0.01,
            "row 0 shares with an output and must not reserve an editor"
        );
        for i in 1..3 {
            assert!(l.input_editors[i].width() > 20.0, "row {i} has no editor");
            assert!(l.input_editors[i].max.x <= l.rect.max.x - s.pad_x + 0.01);
            assert!(l.input_labels[i].max.x <= l.input_editors[i].min.x + 0.01);
        }
    }

    /// An editor never changes the node's size. That is the whole
    /// reason the band is a fixed fraction rather than content-sized.
    #[test]
    fn an_editor_band_does_not_change_the_node_size() {
        let s = spec();
        let with_editor = layout_node(Pos2::new(0.0, 0.0), &shape(2, 0), &s);
        let without = layout_node(Pos2::new(0.0, 0.0), &shape(2, 2), &s);
        assert!((with_editor.rect.width() - without.rect.width()).abs() < 0.01);
        assert!((with_editor.rect.height() - without.rect.height()).abs() < 0.01);
    }

    /// Every pin sits exactly on the body edge, so a wire meets the
    /// node instead of stopping short or crossing into it.
    #[test]
    fn pins_sit_on_the_body_edge() {
        let s = spec();
        let l = layout_node(Pos2::new(10.0, 20.0), &shape(2, 2), &s);
        for p in &l.inputs {
            assert!((p.x - l.rect.min.x).abs() < 0.01);
        }
        for p in &l.outputs {
            assert!((p.x - l.rect.max.x).abs() < 0.01);
        }
    }

    /// Pins and labels stay within the node's own rect vertically —
    /// nothing may spill past the bottom edge.
    #[test]
    fn nothing_spills_past_the_bottom_edge() {
        let s = spec();
        for rows in 0..6 {
            let l = layout_node(Pos2::new(0.0, 0.0), &shape(rows, rows), &s);
            for p in l.inputs.iter().chain(&l.outputs) {
                assert!(p.y <= l.rect.max.y, "a pin sits below the node at {rows} rows");
            }
            for r in l.input_labels.iter().chain(&l.output_labels) {
                assert!(r.max.y <= l.rect.max.y + 0.01);
            }
        }
    }

    /// A layout carries the metrics it was built from, so the painter
    /// can never be handed a different set. The first version of this
    /// renderer passed the two separately, and a zoomed graph came out
    /// as full-size boxes holding unscaled text and pins.
    #[test]
    fn a_layout_remembers_the_metrics_it_used() {
        let zoomed = NodeSpec::default().scaled(2.5);
        let l = layout_node(Pos2::new(0.0, 0.0), &shape(1, 1), &zoomed);
        assert_eq!(l.spec, zoomed);
        assert!((l.rect.width() - NodeSpec::default().width * 2.5).abs() < 0.01);
    }

    /// Scaling multiplies every length by the same factor, so a node at
    /// zoom 2 is the same shape as a node at zoom 1, twice as big —
    /// rather than a big box with small text in it.
    #[test]
    fn zooming_scales_every_length_together() {
        let base = NodeSpec::default();
        let z = 2.0;
        let a = layout_node(Pos2::new(0.0, 0.0), &shape(2, 2), &base);
        let b = layout_node(Pos2::new(0.0, 0.0), &shape(2, 2), &base.scaled(z));
        assert!((b.rect.width() - a.rect.width() * z).abs() < 0.01);
        assert!((b.rect.height() - a.rect.height() * z).abs() < 0.01);
        assert!((b.spec.title_size - a.spec.title_size * z).abs() < 0.01);
        assert!((b.spec.pin_r - a.spec.pin_r * z).abs() < 0.01);
    }

    /// Reserving body space grows the node by exactly that much, so an
    /// app asking for a 150pt chart gets 150pt and the node does not
    /// silently absorb or overshoot it.
    #[test]
    fn body_height_is_added_exactly() {
        let s = spec();
        let bare = layout_node(Pos2::new(0.0, 0.0), &shape(1, 1), &s);
        let mut with_body = shape(1, 1);
        with_body.body_h = 150.0;
        let tall = layout_node(Pos2::new(0.0, 0.0), &with_body, &s);

        assert!((tall.rect.height() - bare.rect.height() - 150.0).abs() < 0.01);
        assert!((tall.body.height() - 150.0).abs() < 0.01);
    }
}
