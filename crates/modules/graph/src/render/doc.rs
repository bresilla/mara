//! Navigating a [`GraphDoc`] with the rebuilt renderer.
//!
//! One level of a document is just a [`Graph`], so this is a thin shell
//! over [`super::view::show_graph`]: pick the level the path points at,
//! draw it, and turn a double-click on an instance node into a step
//! down.
//!
//! Keeping it thin is the point. In the renderer this replaces, level
//! navigation was tangled into the same function that laid out nodes,
//! so entering a subgraph and drawing a node shared state and could
//! disagree about which graph was being shown.
//!
//! This file is checked by `make check` to contain no backend types.

use mara_core::MaraUi;
use mara_core::vocab::Rect;

use super::spec::GraphSpec;
use super::view::{GraphResponse, GraphView, GraphViewState, show_graph};
use crate::{Crumb, GraphDoc, NodePath};

/// Where in a document the view is, plus the view state itself.
#[derive(Clone, Debug, Default)]
pub struct DocViewState {
    pub view: GraphViewState,
    pub path: NodePath,
}

impl DocViewState {
    /// Move to `path`, framing whatever is there.
    ///
    /// Refitting on every level change is deliberate: a subgraph's
    /// contents have no relationship to the parent's camera, so keeping
    /// the camera would usually drop the user onto empty canvas.
    pub fn go_to(&mut self, path: NodePath) {
        if self.path != path {
            self.path = path;
            self.view.request_fit();
        }
    }
}

/// What happened in the document this frame.
#[derive(Clone, Debug, Default)]
pub struct DocResponse {
    pub graph: GraphResponse,
    /// The trail from the root to the level being shown.
    pub breadcrumb: Vec<Crumb>,
    /// Set when the view stepped into or out of a subgraph.
    pub navigated: bool,
}

/// Draw the level `state.path` points at, and handle stepping into
/// subgraphs.
pub fn show_doc<T, V: GraphView<T>>(
    ui: &mut MaraUi<'_>,
    area: Rect,
    doc: &mut GraphDoc<T>,
    view: &mut V,
    state: &mut DocViewState,
    spec: &GraphSpec,
) -> DocResponse {
    // A path can go stale when a node it names is deleted, so it is
    // pruned to the deepest level that still exists before use.
    let pruned = doc.prune_path(&state.path);
    if pruned != state.path {
        state.go_to(pruned);
    }
    let breadcrumb = doc.breadcrumb(&state.path);

    let path = state.path.clone();
    let Some(graph) = doc.level_mut(&path) else {
        return DocResponse {
            breadcrumb,
            ..Default::default()
        };
    };

    let graph_out = show_graph(ui, area, graph, view, &mut state.view, spec);

    let mut navigated = false;
    if let Some(id) = graph_out.double_clicked {
        let entered = doc
            .level(&path)
            .and_then(|g| g.uid_of(id).filter(|uid| g.instance_def(*uid).is_some()));
        if let Some(uid) = entered {
            state.go_to(path.child(uid));
            navigated = true;
        }
    }

    DocResponse {
        graph: graph_out,
        breadcrumb,
        navigated,
    }
}

/// Step up one level, if there is one.
pub fn go_up(state: &mut DocViewState) -> bool {
    match state.path.parent() {
        Some(p) => {
            state.go_to(p);
            true
        }
        None => false,
    }
}
