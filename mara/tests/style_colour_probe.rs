//! Diagnostic: what colours does `mara_node_graph_style` actually emit?
//!
//! Written after a report that the graph rendered "all black". A
//! headless pass distinguishes a styling regression — the style itself
//! resolving to black — from a compositing one in the sharp-zoom path,
//! which no headless render exercises.

use mara::extras::graph::{Graph, InPin, NodePin, NodeViewer, OutPin, PinInfo, pos2};
use mara_core::MaraUi;

struct N;
struct V;

impl NodeViewer<N> for V {
    fn title(&mut self, _n: &N) -> String {
        "node".into()
    }
    fn inputs(&mut self, _n: &N) -> usize {
        1
    }
    fn show_input(
        &mut self,
        _p: &InPin,
        ui: &mut MaraUi<'_>,
        _g: &mut Graph<N>,
    ) -> impl NodePin + 'static {
        ui.label("in");
        PinInfo::circle()
    }
    fn outputs(&mut self, _n: &N) -> usize {
        1
    }
    fn show_output(
        &mut self,
        _p: &OutPin,
        ui: &mut MaraUi<'_>,
        _g: &mut Graph<N>,
    ) -> impl NodePin + 'static {
        ui.label("out");
        PinInfo::circle()
    }
}

/// The style must not resolve to an all-black canvas.
#[test]
fn the_graph_style_emits_more_than_black() {
    let mut graph = Graph::<N>::new();
    graph.insert_node(pos2(100.0, 100.0), N);
    graph.insert_node(pos2(400.0, 200.0), N);

    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0));
    let accent = mara_core::style::active_accent();
    let mut counts: std::collections::HashMap<[u8; 4], usize> = std::collections::HashMap::new();

    for _ in 0..2 {
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        });
        #[allow(deprecated)]
        egui::CentralPanel::default().show(&ctx, |ui| {
            let mut b = mara_backend_egui::EguiUiBackend::new(ui);
            MaraUi::__internal_over_backend_ret(&mut b, accent, |mara| {
                let _ = mara::extras::graph::GraphWidget::new()
                    .id(mara_core::vocab::Id::new("probe"))
                    .style(mara::extras::graph::mara_node_graph_style(accent))
                    .show(&mut graph, &mut V, mara);
            });
        });
        let out = ctx.end_pass();
        let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
        counts.clear();
        for p in &prims {
            if let egui::epaint::Primitive::Mesh(m) = &p.primitive {
                for v in &m.vertices {
                    *counts.entry(v.color.to_array()).or_default() += 1;
                }
            }
        }
    }

    let mut top: Vec<_> = counts.into_iter().collect();
    top.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    eprintln!("distinct colours: {}", top.len());
    for (c, n) in top.iter().take(12) {
        eprintln!("  rgba{c:?} x{n}");
    }

    let non_black: usize = top
        .iter()
        .filter(|(c, _)| c[0] > 12 || c[1] > 12 || c[2] > 12)
        .map(|(_, n)| *n)
        .sum();
    let total: usize = top.iter().map(|(_, n)| *n).sum();
    assert!(total > 0, "nothing painted at all");
    assert!(
        non_black * 4 > total,
        "over three quarters of painted vertices are black: {non_black}/{total}"
    );
}
