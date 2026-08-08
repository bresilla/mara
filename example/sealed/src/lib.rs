//! Sealed-consumer proof crate.
//!
//! This crate compiles against `mara` with default-ish features and
//! **no** `egui` dependency and **no** `raw-egui` feature. It
//! exercises the full app-content surface — views, modules, pods,
//! panes, shelves, view-tree splits, every widget family, and custom
//! canvas drawing — purely through Mara's typed API. If a change to
//! Mara makes raw `egui` reachable (or makes this surface
//! insufficient), this crate is where it should show up as a compile
//! error.
//!
//! It asserts *reachability and types*, not behaviour: nothing here
//! runs, so a widget that compiles but paints nothing still passes.
//! Behavioural coverage lives in the backend crate's frame tests.
//! What this catches is the failure the unit tests structurally
//! cannot — a surface that only a caller holding a raw `egui::Ui` can
//! use.

use mara::ui::container::Tab;
use mara::ui::pane::{Pane, PaneAnchor, RailZone};
use mara::ui::pod::Pod;
use mara::ui::shelf::{ShelfContainer, ShelfDef, ShelfEdge, ShelfLayout, ShelfState};
use mara::ui::vocab::{Align2, Color32, Id, Pos2, Rect, Stroke, Vec2};
use mara::ui::widget::{TreeIconKind, TreeIconSlot};
use mara::ui::{
    CellId, Layout, MaraModule, MaraUi, MaraView, ModuleInlineCtx, ModuleResponse, RibbonAvoidance,
    ViewCtx, ViewId, ViewNode, WorkspaceCtx,
};

// ─── A sealed module: widgets + custom canvas drawing ─────────────

pub struct SealedGauge {
    pub value: f64,
    pub enabled: bool,
    pub query: String,
}

impl MaraModule for SealedGauge {
    fn id(&self) -> Id {
        Id::new("sealed.gauge")
    }

    fn title(&self) -> &str {
        "Sealed Gauge"
    }

    fn icon(&self) -> &'static str {
        "gauge"
    }

    fn inline(&mut self, mui: &mut MaraUi<'_>, ctx: ModuleInlineCtx<'_>) -> ModuleResponse {
        // Plain widgets through the sealed surface.
        mui.label("sealed module body");
        let _ = mui.toggle("enabled", &mut self.enabled);
        let _ = mui.slider("value", &mut self.value, 0.0..=100.0, 1, "%");
        let _ = mui.text_input(&mut self.query, "filter…");
        let resp = mui.button("apply");
        mui.context_menu(&resp, |m| {
            let _ = m.button("reset");
        });

        // Custom drawing through the sealed painter.
        let (painter, canvas_resp) = mui.canvas(Vec2::new(120.0, 60.0));
        let rect: Rect = canvas_resp.rect;
        painter.rect_filled(rect, 4.0, Color32::from_gray(30));
        painter.circle_stroke(rect.center(), 20.0, Stroke::new(2.0, mui.accent()));
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            format!("{:.0}", self.value),
            12.0,
            Color32::WHITE,
        );
        painter.line_segment(
            Pos2::new(rect.left(), rect.bottom()),
            Pos2::new(rect.right(), rect.bottom()),
            Stroke::new(1.0, Color32::GRAY),
        );

        // Sealed input snapshot.
        let input = mui.input();
        if input.primary_pressed && canvas_resp.hovered {
            self.value = (self.value + 1.0).min(100.0);
        }

        if ctx.can_enter_workspace() && resp.clicked {
            ModuleResponse::enter_workspace()
        } else {
            ModuleResponse::none()
        }
    }

    fn workspace(&mut self, _ws: &mut WorkspaceCtx<'_>) {}
}

// ─── Every widget family, in one place ────────────────────────────

/// State for [`every_widget_family`], so the call site stays a list of
/// widgets rather than a list of `let mut`s.
#[derive(Default)]
pub struct WidgetBench {
    pub flag: bool,
    pub radio_on: bool,
    pub amount: f64,
    pub gain: f64,
    pub choice: usize,
    pub text: String,
    pub rgb: [f32; 3],
    pub rgba: [f32; 4],
    pub expanded: bool,
    pub slot_on: bool,
}

/// Touch every widget family the sealed surface exposes.
///
/// A family missing here is a family an app could not reach without a
/// raw `egui::Ui` — which is the whole thing this crate exists to
/// prevent. Add to this when `MaraUi` grows a widget.
pub fn every_widget_family(mui: &mut MaraUi<'_>, state: &mut WidgetBench) {
    let accent = mui.accent();

    // Text and readouts.
    mui.label("label");
    mui.label_colored("label_colored", accent);
    let _ = mui.readout("readout", "value");
    let _ = mui.readout_h("readout_h", "value", 20.0);
    let _ = mui.keybinding_row("Ctrl+K", "Command palette");

    // Chips and badges.
    let _ = mui.chip("chip");
    let _ = mui.chip_colored("chip_colored", accent);
    let _ = mui.badge_row("badge_row", &["a", "b"]);
    let _ = mui.badge_row_colored("badge_row_colored", &[("a", Some(accent)), ("b", None)]);

    // Buttons.
    let _ = mui.button("button");
    let _ = mui.button_h("button_h", 22.0);

    // Pointer predicates on the input snapshot (not widgets).
    let input = mui.input();
    let _ = input.button_down(mara::ui::vocab::PointerButton::Primary);
    let _ = input.button_pressed(mara::ui::vocab::PointerButton::Secondary);

    // Numeric and boolean input.
    let _ = mui.toggle("toggle", &mut state.flag);
    let _ = mui.toggle_track_only(&mut state.flag);
    let _ = mui.slider("slider", &mut state.amount, 0.0..=100.0, 1, "%");
    let _ = mui.drag_value("drag_value", &mut state.gain, 0.1, 0.0..=10.0, 2, "x");
    let _ = mui.progressbar("progressbar", 0.4, "40%");

    // Selection.
    let _ = mui.select_row("select_row", "select_row", Some("trailing"), state.flag);
    let _ = mui.hybrid_select_row(
        "hybrid_select_row",
        "hybrid_select_row",
        None,
        state.flag,
        state.radio_on,
    );
    let _ = mui.dropdown("dropdown", &mut state.choice, &["one", "two", "three"]);

    // Text and colour.
    let _ = mui.text_input(&mut state.text, "text_input…");
    let _ = mui.color_rgb("color_rgb", &mut state.rgb);
    let _ = mui.color_rgba("color_rgba", &mut state.rgba);

    // Grouping.
    mui.section("sealed.section", "section", true, |m| {
        let _ = m.chip("inside a section");
    });

    // Hierarchy.
    let expanded = &mut state.expanded;
    let slot_on = &mut state.slot_on;
    mui.tree(|tree| {
        let mut slots = [TreeIconSlot::new(TreeIconKind::Eye, slot_on)];
        let _ = tree.row(
            "sealed.tree.row",
            0,
            Some(expanded),
            Some("folder"),
            "tree row",
            false,
            accent,
            &mut slots,
        );
    });

    // Typed pod content — the only thing containers accept.
    let _ = mui.pod(
        Pod::new(Id::new("sealed.widgets.pod"))
            .with_search("search…", accent)
            .with_toggle("visible", accent)
            .with_button("refresh", accent),
    );
}

// ─── A sealed view: backdrop, panes, shelves, widgets ─────────────

pub struct SealedView {
    pub gauge_value: f64,
    pub widgets: WidgetBench,
    pub shelves: ShelfState,
}

impl SealedView {
    #[must_use]
    pub fn new() -> Self {
        Self {
            gauge_value: 1.0,
            widgets: WidgetBench::default(),
            shelves: ShelfState::default(),
        }
    }
}

impl Default for SealedView {
    fn default() -> Self {
        Self::new()
    }
}

impl MaraView for SealedView {
    fn id(&self) -> ViewId {
        ViewId::new("sealed.view")
    }

    fn title(&self) -> &str {
        "Sealed"
    }

    fn icon(&self) -> &'static str {
        "grid"
    }

    fn content_avoidance(&self) -> RibbonAvoidance {
        RibbonAvoidance::all()
    }

    fn show(&mut self, ctx: &mut ViewCtx<'_>) {
        // Edge-to-edge backdrop through the sealed view painter.
        let painter = ctx.painter();
        let screen = ctx.screen_rect();
        painter.rect_filled(screen, 0.0, Color32::from_gray(12));

        let accent = ctx.accent;

        // Structural shelves with typed tabbed containers.
        ctx.show_shelves(
            ShelfLayout::full(screen),
            vec![
                ShelfDef::new(Id::new("sealed.shelf.left"), ShelfEdge::Left, accent)
                    .default_size(200.0)
                    .container(ShelfContainer::tabbed(
                        Id::new("sealed.shelf.left.container"),
                        "Left",
                        "box",
                        vec![Tab::new("sealed.tab.a", "A", "info")],
                    )),
                ShelfDef::new(Id::new("sealed.shelf.bottom"), ShelfEdge::Bottom, accent)
                    .default_size(120.0)
                    .movable(),
            ],
            &mut self.shelves,
        );

        // A pane with a typed container holding a pod.
        ctx.show_pane(
            Pane::new(
                Id::new("sealed.pane"),
                "Sealed Pane",
                PaneAnchor::LeftRail(RailZone::Middle),
                accent,
            ),
            |body| {
                body.add_normal(
                    Id::new("sealed.pane.container"),
                    "Panel",
                    "info",
                    vec![Pod::new(Id::new("sealed.pane.pod")).with_button("go", accent)],
                );
            },
        );

        // Widget body laid over the ribbon-avoiding content rect.
        let value = &mut self.gauge_value;
        let widgets = &mut self.widgets;
        ctx.body(|mui| {
            mui.label("sealed view body");
            let _ = mui.drag_value("gauge", value, 0.1, 0.0..=10.0, 2, "x");
            every_widget_family(mui, widgets);
        });
    }
}

// ─── The view tree: a sealed app composes views, not surfaces ─────

/// Build a split view tree from sealed leaves.
///
/// The tree is how a sealed app lays out more than one view at once,
/// so it has to be reachable without naming a backend — a split whose
/// cells could only be filled by raw-surface code would put every
/// multi-view app outside the seal.
#[must_use]
pub fn sealed_view_tree() -> ViewNode {
    const LEFT: CellId = "left";
    const RIGHT: CellId = "right";
    let mut root = ViewNode::split(
        "sealed.split",
        Layout::row(
            4.0,
            vec![(1.0, Layout::cell(LEFT)), (2.0, Layout::cell(RIGHT))],
        ),
    );
    root.push_cell(LEFT, ViewNode::leaf(SealedView::new()));
    root.push_cell(RIGHT, ViewNode::leaf(SealedView::new()));
    root.margin(4.0)
}
