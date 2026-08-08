//! Root Mara example app, owned by eframe/egui and consuming the
//! Mara API crates like an external application. It mirrors the
//! old demo layout one panel at a time:
//!
//! * **Widgets** — Flags / Numbers / Bars / Buttons / Animated.
//! * **Containers** — Position + Rotation, axis-coloured drag values.
//! * **Elements** — scene tree (eye/lock/colour slots) + flat
//!   hybrid_select roster.
//! * **Theme** — Profile dropdowns + accent picker + glass slider.
//! * **Keys** — keybinding rows (readouts).
//! * **About** — version + dependency readouts.
//!
//! The default/root view is an egui-owned Bevy viewport bridge; the
//! rest — ribbons, panes, the widget gallery, theme picker, canvas
//! whiteboard, node graph, map, and code editor — is host-agnostic
//! `mara_core` UI.
//!
//! ## Host egui vs sealed Mara surface
//!
//! This crate plays two roles, and the line between them matters:
//!
//! * **Host glue** (frame drivers like `ui_system`, the ribbon
//!   assembly, the Bevy/eframe/winit bridges, and the node-graph
//!   design) legitimately uses host-owned egui from eframe. It does
//!   **not** enable Mara's `raw-egui` feature. The node-graph
//!   `NodeViewer` impl is no longer among them — it speaks `MaraUi`
//!   since WS-D1.4.
//! * **App content** (pane bodies, pods, trees, the canvas
//!   whiteboard body) goes through the sealed Mara surface —
//!   `PaneBody`, `Pod`, `TreeBody`, `MaraUi`, `MaraPainter`,
//!   `vocab` data types — exactly like an external sealed app
//!   would. See `example/sealed` for the enforced, egui-free proof
//!   crate.

#![allow(
    dead_code,
    clippy::collapsible_if,
    clippy::doc_lazy_continuation,
    clippy::explicit_auto_deref,
    clippy::too_many_arguments,
    clippy::upper_case_acronyms
)]

use std::io::Cursor;

// `egui` directly (not via `eframe`) so this file compiles on Android,
// where the app does not depend on `eframe`. `eframe::egui` is the same
// crate re-exported, so non-Android behavior is unchanged.

use mara::ui::{mara_core, modules::map as mara_map};
use mara_core::LabelSpec;
use mara_core::MaraMemory as _;
use mara_core::container::SeparatorStyle;
use mara_core::pane::{Pane, PaneAnchor, PaneBody, RailZone};
use mara_core::pod::Pod;
use mara_core::ribbon::{
    ResolvedSlotRibbon, RibbonAction, RibbonCluster, RibbonDrag, RibbonEdge, RibbonGlyph,
    RibbonMode, RibbonOpen, RibbonPlacement, RibbonRole, RibbonSlotClick, RibbonSlotItem,
};
use mara_core::shelf::{ShelfContainer, ShelfDef, ShelfEdge, ShelfState};
use mara_core::style::{AccentColor, GlassOpacity, Mode, srgb_to_color};
use mara_core::vocab::Color32 as MaraColor32;
use mara_core::vocab::Pos2 as MaraPos2;
use mara_core::vocab::Stroke as MaraStroke;
use mara_core::vocab::Vec2 as MaraVec2;
use mara_core::widget::{FillStyle, TreeBranchGuide, TreeIconKind, TreeIconSlot};
use mara_map::{
    DEFAULT_SVG_MARKER, MapAnnotation, MapDocument, MapFeatureGeometry, MapFeatureInfo, MapIcon,
    MapInteraction, MapLine, MapPoint, MapPolygon, MapSurface, MapTool, MapViewport, MaraMap,
    lon_lat,
};
// Vendored extras — node graph + code editor. In the unified `mara`
// facade they live under `mara::extras::*`; the node-graph
// offscreen renderer is created from `mara::host::MaraHostCtx`.
use mara::extras::code::{PodCodeEditorExt, Syntax};
use mara::extras::graph::PaneBodyNodeGraphExt;
use mara::extras::graph::{
    Graph, InPin, InPinId, NodeId, NodePin, NodeViewer, OutPin, OutPinId, PinInfo,
};
use mara::host::MaraHostCtx;
use mara::ui::modules::bevy::MaraBevyViewport;
use mara::ui::modules::board::{Board, BoardPaint};
use mara::ui::modules::canvas::{CanvasDocument, CanvasSurface};
use mara::ui::modules::image::{ImageDocument, ImageSurface};
use mara::ui::modules::three_d::{Scene3d, TriangleMesh3d, View3d};
use mara_core::vocab::Id as MaraId;
use mara_core::{Layout, MaraView, RibbonAvoidance, Tab, Tabs, ViewNode, WorkspaceStack};

// ─── Ribbon / pane ids ──────────────────────────────────────────────

const RIBBON_LEFT: &str = "demo_ribbon_left";
const RIBBON_RIGHT: &str = "demo_ribbon_right";
const RIBBON_TOP: &str = "demo_ribbon_top";

// Fullscreen-only ribbons. Painted only while a maximizable widget
// (node graph / code editor) is in its fullscreen overlay — driven
// by the host fullscreen snapshot in the per-frame top-level
// callback below. Uses the SAME ribbon API as the regular rails, so
// the fullscreen view looks like a fresh canvas built from the same
// mara UI primitives.
const RIBBON_FS_LEFT: &str = "demo_ribbon_fs_left";

const PANE_WIDGETS: &str = "demo_pane_widgets";
const PANE_CONTAINERS: &str = "demo_pane_containers";
const PANE_SCENE: &str = "demo_pane_scene";
const PANE_EDITOR: &str = "demo_pane_editor";
const PANE_THEME: &str = "demo_pane_theme";
const PANE_KEYS: &str = "demo_pane_keys";
const PANE_ABOUT: &str = "demo_pane_about";
const PANE_CANVAS_BRUSH: &str = "demo_canvas_pane_brush";
const PANE_CANVAS_LAYERS: &str = "demo_canvas_pane_layers";
const PANE_CANVAS_ASSETS: &str = "demo_canvas_pane_assets";
const PANE_CANVAS_INSPECTOR: &str = "demo_canvas_pane_inspector";
const PANE_CANVAS_HISTORY: &str = "demo_canvas_pane_history";
const PANE_CANVAS_EXPORT: &str = "demo_canvas_pane_export";
const PANE_3D_SCENE: &str = "demo_3d_pane_scene";
const PANE_3D_INSPECTOR: &str = "demo_3d_pane_inspector";
const PANE_MAP_INFO: &str = "demo_map_pane_info";
const PANE_MAP_OBJECTS: &str = "demo_map_pane_objects";
const PANE_COREVIZ_ZONES: &str = "demo_coreviz_pane_zones";
const PANE_COREVIZ_REFERENCE: &str = "demo_coreviz_pane_reference";
const PANE_COREVIZ_NODES: &str = "demo_coreviz_pane_nodes";
const PANE_COREVIZ_EDGES: &str = "demo_coreviz_pane_edges";
const PANE_COREVIZ_ZENOH: &str = "demo_coreviz_pane_zenoh";
const PANE_COREVIZ_ROBOTS: &str = "demo_coreviz_pane_robots";
const PANE_COREVIZ_DETAILS: &str = "demo_coreviz_pane_details";
const PANE_COREVIZ_JSON: &str = "demo_coreviz_pane_json";
const PANE_COREVIZ_SCHEDULER: &str = "demo_coreviz_pane_scheduler";
const PANE_COREVIZ_TASKS: &str = "demo_coreviz_pane_tasks";
const CANVAS_SHELF_LEFT: &str = "demo_canvas_shelf_left";
const GRAPH_LAB_SHELF: &str = "demo_graph_lab_shelf";
/// Every icon the Graph Lab names, in one place.
///
/// Listed as constants rather than spelled inline so the test below has
/// something to check. An icon name that does not resolve panics inside
/// the ribbon assert the first time the bar or shelf renders — after
/// startup, so no compile-time check and no headless test would catch a
/// typo here.
const GRAPH_LAB_ICONS: &[&str] = &[
    GRAPH_LAB_ICON_VIEW,
    GRAPH_LAB_ICON_WHERE,
    GRAPH_LAB_ICON_GROUPS,
    GRAPH_LAB_ICON_SUBGRAPHS,
    GRAPH_LAB_ICON_LIBRARY,
    GRAPH_LAB_ICON_VISUALS,
    GRAPH_LAB_ICON_LEGEND,
];
const GRAPH_LAB_ICON_VIEW: &str = "flowchart";
const GRAPH_LAB_ICON_WHERE: &str = "location";
const GRAPH_LAB_ICON_GROUPS: &str = "square-multiple";
const GRAPH_LAB_ICON_SUBGRAPHS: &str = "branch";
const GRAPH_LAB_ICON_LIBRARY: &str = "list";
const GRAPH_LAB_ICON_VISUALS: &str = "eye";
const GRAPH_LAB_ICON_LEGEND: &str = "color";

const ACTION_PREV_CUBE: &str = "demo_action_prev_cube";
const ACTION_NEXT_CUBE: &str = "demo_action_next_cube";
const ACTION_CANVAS_CLEAR: &str = "demo_action_canvas_clear";
const ACTION_VIEW_BEVY: &str = "demo_action_view_bevy";
const ACTION_VIEW_CANVAS: &str = "demo_action_view_canvas";
const ACTION_VIEW_3D: &str = "demo_action_view_3d";
const ACTION_VIEW_BOARD: &str = "demo_action_view_board";
const ACTION_VIEW_MULTI: &str = "demo_action_view_multi";
const ACTION_VIEW_GRAPHLAB: &str = "demo_action_view_graphlab";
const ACTION_COREVIZ_ZONES: &str = "demo_action_coreviz_zones";
const ACTION_COREVIZ_MANAGEMENT: &str = "demo_action_coreviz_management";
const ACTION_MAP_SELECT: &str = "demo_action_map_select";
const ACTION_MAP_POINT: &str = "demo_action_map_point";
const ACTION_MAP_LINE: &str = "demo_action_map_line";
const ACTION_MAP_POLYGON: &str = "demo_action_map_polygon";
const ACTION_MAP_CLEAR: &str = "demo_action_map_clear";
const ACTION_CLOSE_APP: &str = "demo_action_close_app";
const ACTION_RESTORE_FULLSCREEN: &str = "demo_action_restore_fullscreen";

// Fullscreen-only ribbon actions. Click targets are no-ops in this
// demo — purpose is to show that "the same ribbon API works as
// fullscreen chrome too" with different toolsets per widget kind.
// Graph fullscreen:
const FS_GRAPH_ADD: &str = "demo_fs_graph_add";
const FS_GRAPH_FRAME: &str = "demo_fs_graph_frame";
const FS_GRAPH_CLEAR: &str = "demo_fs_graph_clear";
const FS_GRAPH_SAVE: &str = "demo_fs_graph_save";
const FS_CAT_SOURCES: &str = "demo_fs_cat_sources";
const FS_CAT_MATH: &str = "demo_fs_cat_math";
const FS_CAT_NOISE: &str = "demo_fs_cat_noise";
const FS_CAT_LOGIC: &str = "demo_fs_cat_logic";
// Code-editor fullscreen:
const FS_CODE_SAVE: &str = "demo_fs_code_save";
const FS_CODE_RUN: &str = "demo_fs_code_run";
const FS_CODE_FORMAT: &str = "demo_fs_code_format";
const FS_CODE_FIND: &str = "demo_fs_code_find";
const FS_FILE_MAIN: &str = "demo_fs_file_main";
const FS_FILE_LIB: &str = "demo_fs_file_lib";
const FS_FILE_CARGO: &str = "demo_fs_file_cargo";

const PANE_DEFS: &[(&str, &str, PaneAnchor, &str)] = &[
    (
        RIBBON_LEFT,
        PANE_WIDGETS,
        PaneAnchor::LeftRail(RailZone::Start),
        "Widgets",
    ),
    (
        RIBBON_LEFT,
        PANE_CONTAINERS,
        PaneAnchor::LeftRail(RailZone::Middle),
        "Containers",
    ),
    (
        RIBBON_LEFT,
        PANE_SCENE,
        PaneAnchor::LeftRail(RailZone::End),
        "Elements",
    ),
    (
        RIBBON_RIGHT,
        PANE_THEME,
        PaneAnchor::RightRail(RailZone::Start),
        "Theme",
    ),
    (
        RIBBON_RIGHT,
        PANE_KEYS,
        PaneAnchor::RightRail(RailZone::Middle),
        "Keys",
    ),
    (
        RIBBON_TOP,
        PANE_ABOUT,
        PaneAnchor::TopRail(RailZone::Start),
        "About",
    ),
    (
        RIBBON_RIGHT,
        PANE_EDITOR,
        PaneAnchor::RightRail(RailZone::End),
        "Editor",
    ),
    (
        RIBBON_LEFT,
        PANE_CANVAS_BRUSH,
        PaneAnchor::LeftRail(RailZone::Start),
        "Brush",
    ),
    (
        RIBBON_LEFT,
        PANE_CANVAS_LAYERS,
        PaneAnchor::LeftRail(RailZone::Middle),
        "Layers",
    ),
    (
        RIBBON_LEFT,
        PANE_CANVAS_ASSETS,
        PaneAnchor::LeftRail(RailZone::End),
        "Assets",
    ),
    (
        RIBBON_RIGHT,
        PANE_CANVAS_INSPECTOR,
        PaneAnchor::RightRail(RailZone::Start),
        "Inspector",
    ),
    (
        RIBBON_RIGHT,
        PANE_CANVAS_HISTORY,
        PaneAnchor::RightRail(RailZone::Middle),
        "History",
    ),
    (
        RIBBON_RIGHT,
        PANE_CANVAS_EXPORT,
        PaneAnchor::RightRail(RailZone::End),
        "Export",
    ),
    (
        RIBBON_LEFT,
        PANE_3D_SCENE,
        PaneAnchor::LeftRail(RailZone::Start),
        "3D Scene",
    ),
    (
        RIBBON_RIGHT,
        PANE_3D_INSPECTOR,
        PaneAnchor::RightRail(RailZone::Start),
        "3D Inspector",
    ),
    (
        RIBBON_LEFT,
        PANE_COREVIZ_ZONES,
        PaneAnchor::LeftRail(RailZone::Start),
        "Zones",
    ),
    (
        RIBBON_LEFT,
        PANE_COREVIZ_REFERENCE,
        PaneAnchor::LeftRail(RailZone::Middle),
        "Reference",
    ),
    (
        RIBBON_LEFT,
        PANE_COREVIZ_NODES,
        PaneAnchor::LeftRail(RailZone::Start),
        "Nodes",
    ),
    (
        RIBBON_LEFT,
        PANE_COREVIZ_EDGES,
        PaneAnchor::LeftRail(RailZone::Middle),
        "Edges",
    ),
    (
        RIBBON_LEFT,
        PANE_COREVIZ_ZENOH,
        PaneAnchor::LeftRail(RailZone::Start),
        "Zenoh",
    ),
    (
        RIBBON_LEFT,
        PANE_COREVIZ_ROBOTS,
        PaneAnchor::LeftRail(RailZone::Middle),
        "Robots",
    ),
    (
        RIBBON_RIGHT,
        PANE_COREVIZ_DETAILS,
        PaneAnchor::RightRail(RailZone::Start),
        "Details",
    ),
    (
        RIBBON_RIGHT,
        PANE_MAP_OBJECTS,
        PaneAnchor::RightRail(RailZone::Middle),
        "Map Objects",
    ),
    (
        RIBBON_RIGHT,
        PANE_MAP_INFO,
        PaneAnchor::RightRail(RailZone::Start),
        "Map Selection",
    ),
    (
        RIBBON_RIGHT,
        PANE_COREVIZ_JSON,
        PaneAnchor::RightRail(RailZone::Middle),
        "JSON",
    ),
    (
        RIBBON_RIGHT,
        PANE_COREVIZ_SCHEDULER,
        PaneAnchor::RightRail(RailZone::Start),
        "Scheduler",
    ),
    (
        RIBBON_RIGHT,
        PANE_COREVIZ_TASKS,
        PaneAnchor::RightRail(RailZone::Middle),
        "Tasks",
    ),
];

#[derive(Clone, Copy, Debug)]
struct RibbonSpec {
    id: &'static str,
    edge: RibbonEdge,
    role: RibbonRole,
    mode: RibbonMode,
    accepts: &'static [&'static str],
}

#[derive(Clone, Copy, Debug)]
struct RibbonButtonSpec {
    id: &'static str,
    ribbon: &'static str,
    cluster: RibbonCluster,
    slot: u32,
    draggable: bool,
    glyph: RibbonGlyph,
    tooltip: &'static str,
    child_ribbon: Option<&'static str>,
    role: Option<RibbonRole>,
}

const RIBBONS: &[RibbonSpec] = &[
    // First declared ribbon is the persistent/main app bar. Keep it
    // first so it owns the full left-to-right top edge.
    RibbonSpec {
        id: RIBBON_TOP,
        edge: RibbonEdge::Top,
        role: RibbonRole::Panel,
        mode: RibbonMode::ThreeSided,
        accepts: &[],
    },
    RibbonSpec {
        id: RIBBON_LEFT,
        edge: RibbonEdge::Left,
        role: RibbonRole::Panel,
        mode: RibbonMode::ThreeSided,
        accepts: &[RIBBON_RIGHT],
    },
    RibbonSpec {
        id: RIBBON_RIGHT,
        edge: RibbonEdge::Right,
        role: RibbonRole::Panel,
        mode: RibbonMode::ThreeSided,
        accepts: &[RIBBON_LEFT],
    },
];

const RIBBON_ITEMS_PERSISTENT_TOP: &[RibbonButtonSpec] = &[
    // The root/L0 view switcher now lives in the enforced shell bar
    // (`mara_core::ShellBar`, rendered by the host adapter), so the
    // demo no longer hand-rolls it here. See `demo_shell_views`.
];

const RIBBON_ITEMS: &[RibbonButtonSpec] = &[
    // LEFT rail — primary navigation cluster.
    RibbonButtonSpec {
        id: PANE_WIDGETS,
        ribbon: RIBBON_LEFT,
        cluster: RibbonCluster::Start,
        slot: 0,
        draggable: true,
        glyph: RibbonGlyph::Icon("apps"),
        tooltip: "Widgets gallery",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: PANE_CONTAINERS,
        ribbon: RIBBON_LEFT,
        cluster: RibbonCluster::Start,
        slot: 1,
        draggable: true,
        glyph: RibbonGlyph::Icon("box"),
        tooltip: "Containers showcase",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: PANE_SCENE,
        ribbon: RIBBON_LEFT,
        cluster: RibbonCluster::Start,
        slot: 2,
        draggable: true,
        glyph: RibbonGlyph::Icon("folder"),
        tooltip: "Scene outliner",
        child_ribbon: None,
        role: None,
    },
    // RIGHT rail — theme + input.
    RibbonButtonSpec {
        id: PANE_THEME,
        ribbon: RIBBON_RIGHT,
        cluster: RibbonCluster::Start,
        slot: 0,
        draggable: true,
        glyph: RibbonGlyph::Icon("color"),
        tooltip: "Theme & colour",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: PANE_KEYS,
        ribbon: RIBBON_RIGHT,
        cluster: RibbonCluster::Start,
        slot: 1,
        draggable: true,
        glyph: RibbonGlyph::Icon("keyboard"),
        tooltip: "Keys & gestures",
        child_ribbon: None,
        role: None,
    },
    // TOP middle — root/L0 view switcher. These are normal ribbon
    // buttons, same style as every other demo ribbon button.
    RibbonButtonSpec {
        id: ACTION_VIEW_BEVY,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("cube"),
        tooltip: "Bevy scene view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_VIEW_CANVAS,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 1,
        draggable: false,
        glyph: RibbonGlyph::Icon("pen"),
        tooltip: "Canvas / whiteboard view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_COREVIZ_ZONES,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 2,
        draggable: false,
        glyph: RibbonGlyph::Icon("draw-shape"),
        tooltip: "Map annotation view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_COREVIZ_MANAGEMENT,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 3,
        draggable: false,
        glyph: RibbonGlyph::Icon("location"),
        tooltip: "Map object selection view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    // BOTTOM rail — Editor plus the
    // one-shot cube-cycle action buttons in the End cluster.
    RibbonButtonSpec {
        id: PANE_EDITOR,
        ribbon: RIBBON_RIGHT,
        cluster: RibbonCluster::End,
        slot: 0,
        draggable: true,
        glyph: RibbonGlyph::Icon("flowchart"),
        tooltip: "Editor",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: ACTION_PREV_CUBE,
        ribbon: RIBBON_RIGHT,
        cluster: RibbonCluster::End,
        slot: 1,
        draggable: true,
        glyph: RibbonGlyph::Icon("arrow-left"),
        tooltip: "Previous cube",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_NEXT_CUBE,
        ribbon: RIBBON_RIGHT,
        cluster: RibbonCluster::End,
        slot: 2,
        draggable: true,
        glyph: RibbonGlyph::Icon("arrow-right"),
        tooltip: "Next cube",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
];

const RIBBON_ITEMS_ROOT_VIEW: &[RibbonButtonSpec] = &[
    // TOP rail — the only persistent/shared bar.
    RibbonButtonSpec {
        id: ACTION_VIEW_BEVY,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("cube"),
        tooltip: "Bevy scene view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_VIEW_CANVAS,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 1,
        draggable: false,
        glyph: RibbonGlyph::Icon("pen"),
        tooltip: "Canvas / whiteboard view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_COREVIZ_ZONES,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 2,
        draggable: false,
        glyph: RibbonGlyph::Icon("draw-shape"),
        tooltip: "Map annotation view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_COREVIZ_MANAGEMENT,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 3,
        draggable: false,
        glyph: RibbonGlyph::Icon("location"),
        tooltip: "Map object selection view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    // Canvas LEFT rail — canvas-specific tools.
    RibbonButtonSpec {
        id: PANE_CANVAS_BRUSH,
        ribbon: RIBBON_LEFT,
        cluster: RibbonCluster::Start,
        slot: 0,
        draggable: true,
        glyph: RibbonGlyph::Icon("paint-brush"),
        tooltip: "Canvas brush settings",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: PANE_CANVAS_LAYERS,
        ribbon: RIBBON_LEFT,
        cluster: RibbonCluster::Start,
        slot: 1,
        draggable: true,
        glyph: RibbonGlyph::Icon("square-multiple"),
        tooltip: "Canvas layers",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: PANE_CANVAS_ASSETS,
        ribbon: RIBBON_LEFT,
        cluster: RibbonCluster::Start,
        slot: 2,
        draggable: true,
        glyph: RibbonGlyph::Icon("image"),
        tooltip: "Canvas assets",
        child_ribbon: None,
        role: None,
    },
    // Canvas RIGHT rail — canvas-specific state.
    RibbonButtonSpec {
        id: PANE_CANVAS_INSPECTOR,
        ribbon: RIBBON_RIGHT,
        cluster: RibbonCluster::Start,
        slot: 0,
        draggable: true,
        glyph: RibbonGlyph::Icon("options"),
        tooltip: "Canvas inspector",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: PANE_CANVAS_HISTORY,
        ribbon: RIBBON_RIGHT,
        cluster: RibbonCluster::Start,
        slot: 1,
        draggable: true,
        glyph: RibbonGlyph::Icon("history"),
        tooltip: "Canvas history",
        child_ribbon: None,
        role: None,
    },
    // Canvas BOTTOM rail — canvas actions.
    RibbonButtonSpec {
        id: PANE_CANVAS_EXPORT,
        ribbon: RIBBON_RIGHT,
        cluster: RibbonCluster::End,
        slot: 0,
        draggable: true,
        glyph: RibbonGlyph::Icon("arrow-download"),
        tooltip: "Canvas export",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: ACTION_CANVAS_CLEAR,
        ribbon: RIBBON_RIGHT,
        cluster: RibbonCluster::End,
        slot: 1,
        draggable: true,
        glyph: RibbonGlyph::Icon("delete"),
        tooltip: "Clear canvas strokes",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
];

const RIBBON_ITEMS_3D_VIEW: &[RibbonButtonSpec] = &[
    RibbonButtonSpec {
        id: PANE_3D_SCENE,
        ribbon: RIBBON_LEFT,
        cluster: RibbonCluster::Start,
        slot: 0,
        draggable: true,
        glyph: RibbonGlyph::Icon("folder"),
        tooltip: "3D scene objects",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: PANE_3D_INSPECTOR,
        ribbon: RIBBON_RIGHT,
        cluster: RibbonCluster::Start,
        slot: 0,
        draggable: true,
        glyph: RibbonGlyph::Icon("options"),
        tooltip: "3D selection inspector",
        child_ribbon: None,
        role: None,
    },
];

const RIBBON_ITEMS_MAP_VIEW: &[RibbonButtonSpec] = &[
    RibbonButtonSpec {
        id: ACTION_VIEW_BEVY,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("cube"),
        tooltip: "Bevy scene view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_VIEW_CANVAS,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 1,
        draggable: false,
        glyph: RibbonGlyph::Icon("pen"),
        tooltip: "Canvas / whiteboard view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_COREVIZ_ZONES,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 2,
        draggable: false,
        glyph: RibbonGlyph::Icon("draw-shape"),
        tooltip: "Map annotation view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_COREVIZ_MANAGEMENT,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 3,
        draggable: false,
        glyph: RibbonGlyph::Icon("location"),
        tooltip: "Map object selection view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: PANE_COREVIZ_ZONES,
        ribbon: RIBBON_LEFT,
        cluster: RibbonCluster::Start,
        slot: 0,
        draggable: true,
        glyph: RibbonGlyph::Icon("shape-union"),
        tooltip: "Zones hierarchy",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: PANE_COREVIZ_REFERENCE,
        ribbon: RIBBON_LEFT,
        cluster: RibbonCluster::Start,
        slot: 1,
        draggable: true,
        glyph: RibbonGlyph::Icon("map"),
        tooltip: "Reference datum",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: ACTION_MAP_SELECT,
        ribbon: RIBBON_LEFT,
        cluster: RibbonCluster::Middle,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("cursor"),
        tooltip: "Select annotations",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_MAP_POINT,
        ribbon: RIBBON_LEFT,
        cluster: RibbonCluster::Middle,
        slot: 1,
        draggable: false,
        glyph: RibbonGlyph::Icon("pin"),
        tooltip: "Add point",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_MAP_LINE,
        ribbon: RIBBON_LEFT,
        cluster: RibbonCluster::Middle,
        slot: 2,
        draggable: false,
        glyph: RibbonGlyph::Icon("line"),
        tooltip: "Draw line",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_MAP_POLYGON,
        ribbon: RIBBON_LEFT,
        cluster: RibbonCluster::Middle,
        slot: 3,
        draggable: false,
        glyph: RibbonGlyph::Icon("shape-union"),
        tooltip: "Draw polygon",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_MAP_CLEAR,
        ribbon: RIBBON_LEFT,
        cluster: RibbonCluster::End,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("delete"),
        tooltip: "Clear map",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: PANE_MAP_OBJECTS,
        ribbon: RIBBON_RIGHT,
        cluster: RibbonCluster::Start,
        slot: 0,
        draggable: true,
        glyph: RibbonGlyph::Icon("color"),
        tooltip: "Map object colors",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: PANE_COREVIZ_DETAILS,
        ribbon: RIBBON_RIGHT,
        cluster: RibbonCluster::Start,
        slot: 1,
        draggable: true,
        glyph: RibbonGlyph::Icon("options"),
        tooltip: "Selection inspector",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: PANE_COREVIZ_JSON,
        ribbon: RIBBON_RIGHT,
        cluster: RibbonCluster::Start,
        slot: 2,
        draggable: true,
        glyph: RibbonGlyph::Icon("document"),
        tooltip: "Raw workspace JSON",
        child_ribbon: None,
        role: None,
    },
];

const RIBBON_ITEMS_MAP_MANAGEMENT_VIEW: &[RibbonButtonSpec] = &[
    RibbonButtonSpec {
        id: ACTION_VIEW_BEVY,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("cube"),
        tooltip: "Bevy scene view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_VIEW_CANVAS,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 1,
        draggable: false,
        glyph: RibbonGlyph::Icon("pen"),
        tooltip: "Canvas / whiteboard view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_COREVIZ_ZONES,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 2,
        draggable: false,
        glyph: RibbonGlyph::Icon("draw-shape"),
        tooltip: "Map annotation view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_COREVIZ_MANAGEMENT,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 3,
        draggable: false,
        glyph: RibbonGlyph::Icon("location"),
        tooltip: "Map object selection view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: PANE_COREVIZ_ZENOH,
        ribbon: RIBBON_LEFT,
        cluster: RibbonCluster::Start,
        slot: 0,
        draggable: true,
        glyph: RibbonGlyph::Icon("box"),
        tooltip: "Zenoh connection",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: PANE_COREVIZ_ROBOTS,
        ribbon: RIBBON_LEFT,
        cluster: RibbonCluster::Start,
        slot: 1,
        draggable: true,
        glyph: RibbonGlyph::Icon("location"),
        tooltip: "Robot fleet",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: ACTION_MAP_SELECT,
        ribbon: RIBBON_LEFT,
        cluster: RibbonCluster::Middle,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("cursor"),
        tooltip: "Select map objects",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: PANE_MAP_INFO,
        ribbon: RIBBON_RIGHT,
        cluster: RibbonCluster::Start,
        slot: 0,
        draggable: true,
        glyph: RibbonGlyph::Icon("map"),
        tooltip: "Selected map object",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: PANE_MAP_OBJECTS,
        ribbon: RIBBON_RIGHT,
        cluster: RibbonCluster::Start,
        slot: 1,
        draggable: true,
        glyph: RibbonGlyph::Icon("color"),
        tooltip: "Map object colors",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: PANE_COREVIZ_DETAILS,
        ribbon: RIBBON_RIGHT,
        cluster: RibbonCluster::Start,
        slot: 2,
        draggable: true,
        glyph: RibbonGlyph::Icon("options"),
        tooltip: "Robot details",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: PANE_COREVIZ_SCHEDULER,
        ribbon: RIBBON_RIGHT,
        cluster: RibbonCluster::Start,
        slot: 3,
        draggable: true,
        glyph: RibbonGlyph::Icon("clock"),
        tooltip: "Scheduler",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: PANE_COREVIZ_TASKS,
        ribbon: RIBBON_RIGHT,
        cluster: RibbonCluster::Start,
        slot: 4,
        draggable: true,
        glyph: RibbonGlyph::Icon("list"),
        tooltip: "Robot tasks",
        child_ribbon: None,
        role: None,
    },
];

// ─── Fullscreen-only ribbon sets ───────────────────────────────────
//
// Painted by `ribbon renderer` only while the corresponding widget is
// in its fullscreen overlay — branched in the per-frame paint via
// module fullscreen keys compared against `fullscreen_owner`. Each set uses the
// SAME ribbon API as the regular rails, so the fullscreen view is
// a fresh canvas built from the same mara UI primitives.

// Shared rail-definitions reused by both fullscreen flavours. Items
// reference these by id; the per-widget `RIBBON_ITEMS_FS_*` slices
// below decide which icons populate each rail.
const RIBBONS_FS: &[RibbonSpec] = &[
    RibbonSpec {
        id: RIBBON_TOP,
        edge: RibbonEdge::Top,
        role: RibbonRole::Panel,
        mode: RibbonMode::ThreeSided,
        accepts: &[],
    },
    RibbonSpec {
        id: RIBBON_FS_LEFT,
        edge: RibbonEdge::Left,
        role: RibbonRole::Panel,
        mode: RibbonMode::ThreeSided,
        accepts: &[],
    },
];

// Node-graph fullscreen: a graph-builder toolbar across the top
// (Add / Frame / Clear / Save) plus a category sidebar on the left
// (Sources / Math / Noise / Logic).
const RIBBON_ITEMS_FS_GRAPH: &[RibbonButtonSpec] = &[
    // Persistent main bar stays present in module/fullscreen views.
    // The system-control slot changes meaning here: close becomes
    // restore-to-parent/fullscreen-exit, not app close.
    RibbonButtonSpec {
        id: ACTION_VIEW_BEVY,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("cube"),
        tooltip: "Bevy scene view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_VIEW_CANVAS,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 1,
        draggable: false,
        glyph: RibbonGlyph::Icon("pen"),
        tooltip: "Canvas / whiteboard view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_RESTORE_FULLSCREEN,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::End,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("arrow-minimize"),
        tooltip: "Restore module",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: FS_GRAPH_ADD,
        ribbon: RIBBON_FS_LEFT,
        cluster: RibbonCluster::Start,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("add"),
        tooltip: "Add node",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: FS_GRAPH_FRAME,
        ribbon: RIBBON_FS_LEFT,
        cluster: RibbonCluster::Start,
        slot: 1,
        draggable: false,
        glyph: RibbonGlyph::Icon("arrow-expand"),
        tooltip: "Frame all",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: FS_GRAPH_CLEAR,
        ribbon: RIBBON_FS_LEFT,
        cluster: RibbonCluster::Start,
        slot: 2,
        draggable: false,
        glyph: RibbonGlyph::Icon("delete"),
        tooltip: "Clear graph",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: FS_GRAPH_SAVE,
        ribbon: RIBBON_FS_LEFT,
        cluster: RibbonCluster::Start,
        slot: 3,
        draggable: false,
        glyph: RibbonGlyph::Icon("save"),
        tooltip: "Save graph",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: FS_CAT_SOURCES,
        ribbon: RIBBON_FS_LEFT,
        cluster: RibbonCluster::Middle,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("circle"),
        tooltip: "Sources",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: FS_CAT_MATH,
        ribbon: RIBBON_FS_LEFT,
        cluster: RibbonCluster::Middle,
        slot: 1,
        draggable: false,
        glyph: RibbonGlyph::Icon("calculator"),
        tooltip: "Math",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: FS_CAT_NOISE,
        ribbon: RIBBON_FS_LEFT,
        cluster: RibbonCluster::Middle,
        slot: 2,
        draggable: false,
        glyph: RibbonGlyph::Icon("sine-wave-dots"),
        tooltip: "Noise",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: FS_CAT_LOGIC,
        ribbon: RIBBON_FS_LEFT,
        cluster: RibbonCluster::Middle,
        slot: 3,
        draggable: false,
        glyph: RibbonGlyph::Icon("flowchart"),
        tooltip: "Logic",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
];

// Code-editor fullscreen: an editor toolbar across the top
// (Save / Run / Format / Find) plus a file-switcher sidebar on the
// left (main.rs / lib.rs / Cargo.toml).
const RIBBON_ITEMS_FS_CODE: &[RibbonButtonSpec] = &[
    RibbonButtonSpec {
        id: ACTION_VIEW_BEVY,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("cube"),
        tooltip: "Bevy scene view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_VIEW_CANVAS,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Middle,
        slot: 1,
        draggable: false,
        glyph: RibbonGlyph::Icon("pen"),
        tooltip: "Canvas / whiteboard view",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_RESTORE_FULLSCREEN,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::End,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("arrow-minimize"),
        tooltip: "Restore module",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: FS_CODE_SAVE,
        ribbon: RIBBON_FS_LEFT,
        cluster: RibbonCluster::Start,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("save"),
        tooltip: "Save",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: FS_CODE_RUN,
        ribbon: RIBBON_FS_LEFT,
        cluster: RibbonCluster::Start,
        slot: 1,
        draggable: false,
        glyph: RibbonGlyph::Icon("play"),
        tooltip: "Run",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: FS_CODE_FORMAT,
        ribbon: RIBBON_FS_LEFT,
        cluster: RibbonCluster::Start,
        slot: 2,
        draggable: false,
        glyph: RibbonGlyph::Icon("wand"),
        tooltip: "Format",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: FS_CODE_FIND,
        ribbon: RIBBON_FS_LEFT,
        cluster: RibbonCluster::Start,
        slot: 3,
        draggable: false,
        glyph: RibbonGlyph::Icon("search"),
        tooltip: "Find",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: FS_FILE_MAIN,
        ribbon: RIBBON_FS_LEFT,
        cluster: RibbonCluster::Middle,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("code"),
        tooltip: "main.rs",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: FS_FILE_LIB,
        ribbon: RIBBON_FS_LEFT,
        cluster: RibbonCluster::Middle,
        slot: 1,
        draggable: false,
        glyph: RibbonGlyph::Icon("book"),
        tooltip: "lib.rs",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: FS_FILE_CARGO,
        ribbon: RIBBON_FS_LEFT,
        cluster: RibbonCluster::Middle,
        slot: 2,
        draggable: false,
        glyph: RibbonGlyph::Icon("box"),
        tooltip: "Cargo.toml",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
];

fn find_item<'a>(items: &'a [RibbonButtonSpec], id: &'static str) -> Option<&'a RibbonButtonSpec> {
    items.iter().find(|item| item.id == id)
}

fn find_ribbon<'a>(ribbons: &'a [RibbonSpec], id: &'static str) -> Option<&'a RibbonSpec> {
    ribbons.iter().find(|ribbon| ribbon.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One payload of every shape the demo can put on screen.
    ///
    /// Spelled out rather than derived, because the point is to fail
    /// when a variant is ADDED without a matching icon or title — and
    /// anything derived from the enum would grow silently with it.
    fn graph_node_samples() -> Vec<GraphNode> {
        vec![
            GraphNode::Subgraph,
            GraphNode::Port {
                out: false,
                name: "signal".into(),
            },
            GraphNode::Port {
                out: true,
                name: "value".into(),
            },
            GraphNode::Number(0.0),
            GraphNode::Integer(0),
            GraphNode::Vector([0.0; 3]),
            GraphNode::Color(MaraColor32::WHITE),
            GraphNode::Bool(false),
            GraphNode::Time,
            GraphNode::ScalarMath(ScalarOp::Add),
            GraphNode::Trig(TrigFn::Sin),
            GraphNode::Compare(CompareOp::Lt),
            GraphNode::Mix,
            GraphNode::Clamp,
            GraphNode::MapRange,
            GraphNode::Smoothstep,
            GraphNode::Step,
            GraphNode::VectorMath(VectorOp::Add),
            GraphNode::Compose,
            GraphNode::Decompose,
            GraphNode::Length,
            GraphNode::Dot,
            GraphNode::Distance,
            GraphNode::Normalize,
            GraphNode::VectorRotate,
            GraphNode::Reflect,
            GraphNode::RgbToColor,
            GraphNode::HsvToColor,
            GraphNode::ColorMix,
            GraphNode::HueShift,
            GraphNode::ColorInvert,
            GraphNode::BrightContrast,
            GraphNode::Gamma,
            GraphNode::IfElse,
            GraphNode::BooleanMath(BoolOp::And),
            GraphNode::FloatToBool,
            GraphNode::BoolToFloat,
            GraphNode::Perlin {
                seed: 0,
                frequency: 1.0,
            },
            GraphNode::WhiteNoise { seed: 0 },
            GraphNode::Wave(WaveShape::Sine),
            GraphNode::Display,
            GraphNode::Plot,
            GraphNode::PlotXY,
            GraphNode::Preview,
            GraphNode::VectorPreview,
            GraphNode::NoiseImage {
                seed: 0,
                scale: 1.0,
            },
            GraphNode::NoiseField,
            GraphNode::MultiPlot,
            GraphNode::Output,
        ]
    }

    /// Both graphs in the demo must size their nodes by one rule.
    ///
    /// The editor graph had no width rule at all — every node came out
    /// as wide as its own content, and this viewer draws an inline
    /// editor for each unwired input, so peers in a column were visibly
    /// ragged while the Graph Lab's were not. Two graphs in one app
    /// sizing their nodes differently is the most visible way they stop
    /// looking like the same product.
    #[test]
    fn every_node_in_both_graphs_shares_one_width_rule() {
        let editor = default_graph();
        let missing: Vec<_> = editor
            .node_ids()
            .filter(|(id, _)| editor.size_override_of(*id).is_none())
            .map(|(id, _)| id)
            .collect();
        assert!(
            missing.is_empty(),
            "{} editor-graph nodes have no width rule",
            missing.len()
        );

        for (id, _) in editor.node_ids() {
            let w = editor.size_override_of(id).unwrap().x;
            assert!(
                (w - NODE_W).abs() < 0.01,
                "editor node {id:?} is {w} wide, not the shared {NODE_W}"
            );
        }

        let lab = build_graph_lab_doc();
        for (id, _) in lab.root.node_ids() {
            let Some(size) = lab.root.size_override_of(id) else {
                panic!("graph lab node {id:?} has no width rule");
            };
            assert!(
                size.x >= NODE_W - 0.01,
                "graph lab node {id:?} is narrower ({}) than the shared {NODE_W}",
                size.x
            );
        }
    }

    #[test]
    fn demo_ribbon_icons_are_renderable() {
        for item in RIBBON_ITEMS_PERSISTENT_TOP
            .iter()
            .chain(RIBBON_ITEMS)
            .chain(RIBBON_ITEMS_ROOT_VIEW)
            .chain(RIBBON_ITEMS_3D_VIEW)
            .chain(RIBBON_ITEMS_MAP_VIEW)
            .chain(RIBBON_ITEMS_MAP_MANAGEMENT_VIEW)
            .chain(RIBBON_ITEMS_FS_GRAPH)
            .chain(RIBBON_ITEMS_FS_CODE)
        {
            let icon = match item.glyph {
                RibbonGlyph::Icon(icon) | RibbonGlyph::Text(icon) | RibbonGlyph::Svg(icon) => icon,
            };
            assert!(
                mara_core::icons::is_icon_payload(icon),
                "demo ribbon item {} uses a non-renderable icon payload {:?}",
                item.id,
                icon
            );
        }
    }

    /// The top-bar view switcher's icons.
    ///
    /// Not covered by `demo_ribbon_icons_are_renderable` above, which
    /// walks the ribbon tables — and that gap is not theoretical: a
    /// `ShellView` added with an icon name that does not resolve panics
    /// at `assert_ribbon_icon` the moment the bar renders, which is
    /// after startup and therefore invisible to every compile-time
    /// check and every headless test.
    #[test]
    fn shell_view_icons_are_renderable() {
        for view in demo_shell_views() {
            assert!(
                mara_core::icons::is_icon_payload(view.icon),
                "shell view {:?} uses a non-renderable icon payload {:?}",
                view.id,
                view.icon
            );
        }
    }

    /// The same hazard one level down: a shelf container or one of its
    /// tabs naming an icon that does not resolve.
    #[test]
    fn graph_lab_icons_are_renderable() {
        for icon in GRAPH_LAB_ICONS {
            assert!(
                mara_core::icons::is_icon_payload(icon),
                "graph lab uses a non-renderable icon payload {icon:?}"
            );
        }
    }

    /// And once more inside the canvas. A node header falls back to a
    /// bullet rather than panicking, so a name that does not resolve is
    /// invisible in CI and merely wrong on screen — which is exactly
    /// the class of drift this whole pass is about.
    #[test]
    fn graph_node_icons_are_renderable() {
        let bad: Vec<String> = graph_node_samples()
            .iter()
            .filter(|n| !mara_core::icons::is_icon_payload(n.icon_name()))
            .map(|n| format!("{} → {:?}", n.title(), n.icon_name()))
            .collect();
        assert!(bad.is_empty(), "non-renderable node icons: {bad:#?}");
    }

    /// Every node in the Graph Lab carries a width the app chose.
    ///
    /// Width is otherwise measured from content, and two peers in one
    /// group whose content differs by an inline editor end up two
    /// widths. Asserting it here is cheaper than noticing it on screen.
    #[test]
    fn graph_lab_nodes_share_a_width() {
        let doc = build_graph_lab_doc();
        for (id, node) in doc.root.node_ids() {
            let want = match node {
                GraphNode::Display => LAB_READOUT_SIZE.x,
                _ => LAB_NODE_W,
            };
            let got = doc.root.size_override_of(id).map(|s| s.x);
            assert_eq!(
                got,
                Some(want),
                "{} is sized by its content, not by the layout",
                node.title()
            );
        }
    }

    /// A chip presents as its definition, not as its payload.
    ///
    /// The payload the crate mints carries no name, no colour and no
    /// port labels, so without the library snapshot every placement of
    /// every definition renders as "Subgraph" over unnamed pins — which
    /// is what "an afterthought next to every other node" looks like.
    #[test]
    fn chip_presents_as_its_definition() {
        let doc = build_graph_lab_doc();
        let viewer = DemoViewer {
            defs: def_looks(&doc),
            ..DemoViewer::default()
        };
        let chips: Vec<NodeId> = doc
            .root
            .node_ids()
            .filter(|(_, n)| matches!(n, GraphNode::Subgraph))
            .map(|(id, _)| id)
            .collect();
        assert_eq!(chips.len(), 2, "the chip band places one definition twice");
        for chip in chips {
            let look = viewer.look(chip, &doc.root);
            assert_eq!(look.title, "Gain stage");
            assert_eq!(look.tint, mara::extras::graph::palette(5));
            assert_eq!(look.subtitle, "shared");
            assert_eq!(
                viewer.instance_input(chip, 0, &doc.root),
                Some("signal"),
                "instance pins take their labels from the definition's ports"
            );
            assert_eq!(viewer.instance_output(chip, 0, &doc.root), Some("value"));
        }
    }

    /// Diving into a chip must land on a laid-out graph.
    ///
    /// `collapse` stacks boundary nodes 70 px apart, which is closer
    /// than a node is tall, so a definition with two ports on one side
    /// opens with them overlapping. "Go inside" is a headline feature
    /// of this view; arriving at a pile is what makes it look
    /// unfinished.
    #[test]
    fn chip_interior_is_laid_out() {
        let doc = build_graph_lab_doc();
        let (_, def) = doc.defs().next().expect("the chip band makes one");
        let mut seen: Vec<mara::ui::vocab::Pos2> = Vec::new();
        for (id, node) in def.body.node_ids() {
            let pos = def.body.get_node_info(id).expect("live").pos;
            assert!(
                !seen.iter().any(|p| *p == pos),
                "two nodes share a position inside the definition"
            );
            seen.push(pos);
            if let GraphNode::Port { name, .. } = node {
                assert!(
                    def.ports
                        .inputs()
                        .iter()
                        .chain(def.ports.outputs())
                        .any(|p| p.name == *name),
                    "boundary node {name:?} kept a name the interface no longer has"
                );
            }
        }
        assert_eq!(seen.len(), 5, "two maths nodes plus three boundaries");
    }

    /// Bands must not reach into one another.
    ///
    /// A group box grows upward by its title band, so the pitch has to
    /// clear the tallest node in the band above plus that band — and
    /// "looks fine on my screen" is not a check anything can run.
    #[test]
    fn graph_lab_bands_are_ordered_and_spaced() {
        let mut last = f32::NEG_INFINITY;
        for y in LAB_BAND_Y {
            assert!(y > last, "graph lab bands must run top to bottom");
            last = y;
        }
        assert!(
            LAB_BAND_Y[1] - LAB_BAND_Y[0] >= 400.0,
            "the chip band is two rows of nodes plus two readouts tall"
        );
    }

    #[test]
    fn view_switcher_lives_in_the_enforced_shell_bar() {
        // The permanent top bar is now the enforced `mara_core::ShellBar`,
        // not a hand-rolled persistent-top ribbon — the demo dogfoods it.
        // So the persistent-top item list is empty and the views come
        // from `demo_shell_views()` instead.
        assert!(
            RIBBON_ITEMS_PERSISTENT_TOP.is_empty(),
            "the demo must not hand-roll a persistent-top view switcher anymore"
        );
        let views = demo_shell_views();
        let ids: Vec<&'static str> = views.iter().map(|v| v.id).collect();
        for id in [
            ACTION_VIEW_BEVY,
            ACTION_VIEW_CANVAS,
            ACTION_COREVIZ_ZONES,
            ACTION_COREVIZ_MANAGEMENT,
            ACTION_VIEW_3D,
        ] {
            assert!(ids.contains(&id), "shell view switcher missing {id}");
        }
        assert_eq!(shell_active_view_id(DemoRootView::ThreeD), ACTION_VIEW_3D);
    }

    #[test]
    fn coreviz_views_stay_on_persistent_bar() {
        for items in [
            RIBBON_ITEMS,
            RIBBON_ITEMS_ROOT_VIEW,
            RIBBON_ITEMS_MAP_VIEW,
            RIBBON_ITEMS_MAP_MANAGEMENT_VIEW,
        ] {
            for (slot, id) in [(2, ACTION_COREVIZ_ZONES), (3, ACTION_COREVIZ_MANAGEMENT)] {
                let item = find_item(items, id).expect("missing Coreviz context button");
                assert_eq!(item.ribbon, RIBBON_TOP);
                assert_eq!(item.cluster, RibbonCluster::Middle);
                assert_eq!(item.slot, slot);
                assert!(!item.draggable);
                assert_eq!(item.role, Some(RibbonRole::Icon));
            }
        }
    }

    #[test]
    fn hidden_map_surface_has_no_top_level_button() {
        for items in [
            RIBBON_ITEMS,
            RIBBON_ITEMS_ROOT_VIEW,
            RIBBON_ITEMS_3D_VIEW,
            RIBBON_ITEMS_MAP_VIEW,
            RIBBON_ITEMS_MAP_MANAGEMENT_VIEW,
        ] {
            assert!(find_item(items, "demo_action_view_map").is_none());
        }
    }
}

fn ribbon_action(id: &'static str) -> RibbonAction {
    match id {
        ACTION_CLOSE_APP => RibbonAction::CloseApp,
        ACTION_RESTORE_FULLSCREEN => RibbonAction::PopWorkspace,
        _ => RibbonAction::Command(MaraId::new(id)),
    }
}

fn is_persistent_top_item(id: &'static str) -> bool {
    matches!(
        id,
        ACTION_VIEW_BEVY
            | ACTION_VIEW_CANVAS
            | ACTION_VIEW_3D
            | ACTION_COREVIZ_ZONES
            | ACTION_COREVIZ_MANAGEMENT
    )
}

fn draw_unified_ribbons(
    host: &MaraHostCtx<'_>,
    accent: MaraColor32,
    ribbons: &[RibbonSpec],
    items: &[RibbonButtonSpec],
    open: &mut RibbonOpen,
    placement: &mut RibbonPlacement,
    drag: &mut RibbonDrag,
    active: impl Fn(&'static str) -> bool,
) -> Vec<RibbonSlotClick> {
    let mut stable_items: Vec<&RibbonButtonSpec> = Vec::with_capacity(
        RIBBON_ITEMS_PERSISTENT_TOP
            .len()
            .saturating_add(items.len()),
    );
    stable_items.extend(RIBBON_ITEMS_PERSISTENT_TOP.iter());
    stable_items.extend(items.iter().filter(|item| !is_persistent_top_item(item.id)));

    let mut resolved = Vec::new();
    for ribbon in ribbons {
        for cluster in [
            RibbonCluster::Start,
            RibbonCluster::Middle,
            RibbonCluster::End,
        ] {
            let slot_items: Vec<RibbonSlotItem> = stable_items
                .iter()
                .copied()
                .filter(|item| item.ribbon == ribbon.id && item.cluster == cluster)
                .map(|item| {
                    let icon = match item.glyph {
                        RibbonGlyph::Icon(icon)
                        | RibbonGlyph::Text(icon)
                        | RibbonGlyph::Svg(icon) => icon,
                    };
                    let mut slot_item = RibbonSlotItem::featureful(
                        item.id,
                        icon,
                        item.id,
                        item.tooltip,
                        ribbon_action(item.id),
                    )
                    .with_role(item.role.unwrap_or(ribbon.role));
                    if let Some(child) = item.child_ribbon {
                        slot_item = slot_item.with_child_ribbon(child);
                    }
                    slot_item.draggable = item.draggable;
                    slot_item.active = active(item.id);
                    slot_item
                })
                .collect();
            if slot_items.is_empty() {
                continue;
            }
            resolved.push(ResolvedSlotRibbon {
                id: MaraId::new((ribbon.id, cluster)),
                chrome_id: Some(ribbon.id),
                scope: demo_ribbon_scope(ribbon.id),
                edge: ribbon.edge,
                role: ribbon.role,
                mode: ribbon.mode,
                cluster,
                accepts: ribbon.accepts,
                items: slot_items,
            });
        }
    }
    host.draw_slot_ribbons_featureful(accent, &resolved, open, placement, drag)
}

fn publish_current_pane_ribbon_buttons(
    host: &MaraHostCtx<'_>,
    items: &[RibbonButtonSpec],
    fullscreen_active: bool,
) {
    let mut pane_ids = Vec::new();
    for item in RIBBON_ITEMS_PERSISTENT_TOP.iter().chain(items).chain(
        fullscreen_active
            .then_some(RIBBON_ITEMS)
            .into_iter()
            .flatten(),
    ) {
        if item.role.is_none() {
            pane_ids.push(MaraId::new(item.id));
        }
    }
    // The multiview cells' demo panel buttons (RibbonDemoView) open
    // per-view panes; register their pane ids so the pane guard accepts
    // them. Ids must mirror `RibbonDemoView::show`'s Pane ids exactly.
    for salt in ["mv.canvas", "mv.image", "mv.board"] {
        for (_, _, edge_name, cluster_name, _) in demo_ribbon_combos() {
            pane_ids.push(MaraId::new((
                "demo.view.pane",
                salt,
                edge_name,
                cluster_name,
            )));
        }
    }
    host.publish_ribbon_pane_ids(pane_ids);
}

fn demo_ribbon_scope(ribbon_id: &'static str) -> mara_core::RibbonScope {
    if ribbon_id == RIBBON_TOP {
        mara_core::RibbonScope::Permanent
    } else {
        mara_core::RibbonScope::View(mara_core::ViewId::new("demo.local_ribbons"))
    }
}

// ─── Theme + UI state ──────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default)]
struct ThemeFamily(u8);

#[derive(Clone, Copy, Debug, Default)]
struct ThemeModeRes(u8);

#[derive(Clone, Copy, Debug)]
struct PastelToggle(bool);
impl Default for PastelToggle {
    fn default() -> Self {
        Self(true)
    }
}

#[derive(Clone, Copy, Debug)]
struct TintRgba(pub [f32; 4]);
impl Default for TintRgba {
    fn default() -> Self {
        Self([0.5, 0.7, 0.9, 0.6])
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DemoRootView {
    #[default]
    BevyScene,
    Canvas,
    ThreeD,
    Board,
    Multi,
    CorevizZones,
    CorevizManagement,
    /// Full-screen node graph showcasing frames, subgraphs and the
    /// visual work — see `PLAN_NODE.md`.
    GraphLab,
}

impl DemoRootView {
    fn is_coreviz(self) -> bool {
        matches!(self, Self::CorevizZones | Self::CorevizManagement)
    }
}

#[derive(Default)]
struct CanvasViewState {
    strokes: Vec<Vec<mara::ui::vocab::Pos2>>,
}

struct ThreeDViewState {
    view: View3d,
    workspace: WorkspaceStack,
}

// The VT soft keys — drawn into the single Board's internal-layout cells.
const LEFT_KEYS: [&str; 5] = ["L1", "L2", "L3", "L4", "L5"];
const RIGHT_KEYS: [&str; 5] = ["R1", "R2", "R3", "R4", "R5"];

// "Board" view: ONE full Board with its own internal layout — a whole VT
// (data-mask cell + soft-key cells) drawn inside a single board.
fn board_view_node() -> ViewNode {
    // No gaps: the board's cells tile flush and each cell's border
    // is the dividing line — same look as the multiview tiles.
    let gap = 0.0;
    let column = |keys: &[&'static str]| {
        Layout::col(gap, keys.iter().map(|k| (1.0, Layout::cell(*k))).collect())
    };
    let layout = Layout::row(
        gap,
        vec![
            (1.0, column(&LEFT_KEYS)),
            (4.0, Layout::cell("data_mask")),
            (1.0, column(&RIGHT_KEYS)),
        ],
    );
    let view = Board::new("demo-board", "Board")
        .with_icon("square-multiple")
        .with_layout(layout)
        .on_draw(|b: BoardPaint| {
            if let Some(rect) = b.cell("data_mask") {
                draw_gauge_at(b.painter, rect, b.accent);
            }
            for key in LEFT_KEYS.iter().chain(RIGHT_KEYS.iter()) {
                if let Some(rect) = b.cell(key) {
                    draw_key_at(b.painter, rect, key);
                }
            }
        });
    // A single-view tab is the degenerate tree: one Leaf as the
    // root ViewNode (PLAN.md Phase 5).
    ViewNode::leaf(view)
}

// Demo decorator for multiview leaves: fills every FREE per-view ribbon
// spot — Left/Right/Bottom edge × Start/Middle/End cluster — with a
// PANEL button: clicking toggles a pane anchored to that rail zone,
// inside the view's own region. Spots the wrapped view already uses
// keep the real buttons.
struct RibbonDemoView<V: MaraView> {
    salt: &'static str,
    inner: V,
    /// Open demo panels, keyed by (edge, cluster).
    open: std::collections::HashSet<(mara_core::RibbonEdge, mara_core::RibbonCluster)>,
}

/// The nine per-view rail spots: edge × cluster, with stable name parts
/// (for ids) and a distinct icon per cluster.
fn demo_ribbon_combos() -> [(
    mara_core::RibbonEdge,
    mara_core::RibbonCluster,
    &'static str,
    &'static str,
    &'static str,
); 9] {
    use mara_core::{RibbonCluster, RibbonEdge};
    let clusters = [
        (RibbonCluster::Start, "start", "star"),
        (RibbonCluster::Middle, "middle", "circle"),
        (RibbonCluster::End, "end", "flag"),
    ];
    let edges = [
        (RibbonEdge::Left, "left"),
        (RibbonEdge::Right, "right"),
        (RibbonEdge::Bottom, "bottom"),
    ];
    let mut out = [(RibbonEdge::Left, RibbonCluster::Start, "", "", ""); 9];
    let mut i = 0;
    for (edge, edge_name) in edges {
        for (cluster, cluster_name, icon) in clusters {
            out[i] = (edge, cluster, edge_name, cluster_name, icon);
            i += 1;
        }
    }
    out
}

impl<V: MaraView> RibbonDemoView<V> {
    fn new(salt: &'static str, inner: V) -> Self {
        Self {
            salt,
            inner,
            open: std::collections::HashSet::new(),
        }
    }

    /// The toggle-command id for one rail spot's panel button.
    fn panel_command_id(&self, edge_name: &'static str, cluster_name: &'static str) -> MaraId {
        MaraId::new(("demo.view.panel", self.salt, edge_name, cluster_name))
    }
}

impl<V: MaraView> MaraView for RibbonDemoView<V> {
    fn id(&self) -> mara_core::ViewId {
        self.inner.id()
    }

    fn title(&self) -> &str {
        self.inner.title()
    }

    fn icon(&self) -> &'static str {
        self.inner.icon()
    }

    fn shared_surface(&self) -> Option<mara_core::SharedSurfaceId> {
        self.inner.shared_surface()
    }

    fn ribbons(&mut self) -> Vec<mara_core::RibbonSlotDef> {
        use mara_core::{
            RibbonAction, RibbonCluster, RibbonEdge, RibbonOverridePolicy, RibbonScope, RibbonSlot,
            RibbonSlotDef, RibbonSlotId, RibbonSlotItem,
        };
        let mut defs = self.inner.ribbons();
        let taken: Vec<(RibbonEdge, RibbonCluster)> =
            defs.iter().map(|def| (def.edge, def.cluster)).collect();
        let view = self.inner.id();
        for (edge, cluster, edge_name, cluster_name, icon) in demo_ribbon_combos() {
            if taken.contains(&(edge, cluster)) {
                continue;
            }
            let mut item = RibbonSlotItem::new(
                MaraId::new(("demo.view.ribbon.item", self.salt, edge_name, cluster_name)),
                icon,
                "Demo",
                "Toggle demo panel",
                RibbonAction::Command(self.panel_command_id(edge_name, cluster_name)),
            );
            item.active = self.open.contains(&(edge, cluster));
            defs.push(RibbonSlotDef::new(
                MaraId::new(("demo.view.ribbon", self.salt, edge_name, cluster_name)),
                RibbonScope::View(view),
                edge,
                cluster,
                vec![RibbonSlot::new(
                    RibbonSlotId::new((
                        "demo.view.ribbon.slot",
                        self.salt,
                        edge_name,
                        cluster_name,
                    )),
                    Some(item),
                    RibbonOverridePolicy::Fixed,
                )],
            ));
        }
        defs
    }

    fn on_ribbon_click(&mut self, action: &mara_core::RibbonAction) {
        if let mara_core::RibbonAction::Command(id) = action {
            for (edge, cluster, edge_name, cluster_name, _) in demo_ribbon_combos() {
                if *id == self.panel_command_id(edge_name, cluster_name) {
                    if !self.open.remove(&(edge, cluster)) {
                        self.open.insert((edge, cluster));
                    }
                    return;
                }
            }
        }
        self.inner.on_ribbon_click(action);
    }

    fn ribbon_overrides(&mut self) -> mara_core::RibbonOverrideLayer {
        self.inner.ribbon_overrides()
    }

    fn content_avoidance(&self) -> RibbonAvoidance {
        self.inner.content_avoidance()
    }

    fn show(&mut self, ctx: &mut mara_core::ViewCtx<'_>) {
        use mara_core::{RibbonCluster, RibbonEdge};
        self.inner.show(ctx);
        // Open demo panels: one pane per toggled rail spot, anchored to
        // that spot INSIDE this view's region (show_pane scopes panes to
        // the node), so each cell owns its own panels.
        for (edge, cluster, edge_name, cluster_name, _) in demo_ribbon_combos() {
            if !self.open.contains(&(edge, cluster)) {
                continue;
            }
            let zone = match cluster {
                RibbonCluster::Start => RailZone::Start,
                RibbonCluster::Middle => RailZone::Middle,
                RibbonCluster::End => RailZone::End,
            };
            let anchor = match edge {
                RibbonEdge::Left => PaneAnchor::LeftRail(zone),
                RibbonEdge::Right => PaneAnchor::RightRail(zone),
                RibbonEdge::Top | RibbonEdge::Bottom => PaneAnchor::BottomRail(zone),
            };
            ctx.show_pane(
                Pane::new(
                    MaraId::new(("demo.view.pane", self.salt, edge_name, cluster_name)),
                    format!("{edge_name} {cluster_name}"),
                    anchor,
                    mara_core::style::active_accent(),
                ),
                |body| {
                    body.add_normal(
                        MaraId::new(("demo.view.pane.c", self.salt, edge_name, cluster_name)),
                        "Demo panel",
                        "info",
                        vec![
                            Pod::new(MaraId::new((
                                "demo.view.pane.pod",
                                self.salt,
                                edge_name,
                                cluster_name,
                            )))
                            .with_separator(SeparatorStyle::Line)
                            .with_readout("view", self.salt),
                            Pod::new(MaraId::new((
                                "demo.view.pane.pod2",
                                self.salt,
                                edge_name,
                                cluster_name,
                            )))
                            .with_separator(SeparatorStyle::None)
                            .with_readout("zone", format!("{edge_name} / {cluster_name}")),
                        ],
                    );
                },
            );
        }
    }
}

// "Multiview" view: split in half; the right half split again → three
// different child views (a Canvas, an Image, a Board).
fn multi_view_node() -> ViewNode {
    // No gaps, no margin: cells tile edge-to-edge and each cell's
    // square border provides the dividing line between views.
    let gap = 0.0;
    let layout = Layout::row(
        gap,
        vec![
            (1.0, Layout::cell("left")),
            (
                1.0,
                Layout::col(
                    gap,
                    vec![(1.0, Layout::cell("rt")), (1.0, Layout::cell("rb"))],
                ),
            ),
        ],
    );
    ViewNode::split("demo-multi", layout)
        .cell(
            "left",
            ViewNode::leaf(RibbonDemoView::new(
                "mv.canvas",
                CanvasSurface::new("mv.canvas", CanvasDocument::new("Sketch")),
            )),
        )
        .cell(
            "rt",
            ViewNode::leaf(RibbonDemoView::new(
                "mv.image",
                ImageSurface::new("mv.image", ImageDocument::empty("Image")),
            )),
        )
        .cell(
            "rb",
            ViewNode::leaf(RibbonDemoView::new(
                "mv.board",
                Board::new("mv.board", "Gauge")
                    .on_draw(|b: BoardPaint| draw_gauge_at(b.painter, b.rect, b.accent)),
            )),
        )
}

/// The tab-migrated root views (PLAN.md WS8, increment one): Board and
/// Multi live in the sealed [`Tabs`] collection — one type owning each
/// tab's switcher entry, tree, and workspace. The remaining root views
/// migrate tab-by-tab; until then `DemoRootView` stays the dispatcher
/// and is kept in sync with the active tab.
struct DemoTabs(Tabs);

impl Default for DemoTabs {
    fn default() -> Self {
        // Switcher entries reuse the ids/icons/tooltips from
        // `demo_shell_views()` so the enforced bar behaves identically.
        Self(Tabs::new(vec![
            Tab::new(
                mara_core::ShellView::new(
                    ACTION_VIEW_BOARD,
                    "square-multiple",
                    "Board view (single board)",
                ),
                board_view_node(),
            ),
            Tab::new(
                mara_core::ShellView::new(
                    ACTION_VIEW_MULTI,
                    "grid",
                    "Multiview (split into views)",
                ),
                multi_view_node(),
            ),
        ]))
    }
}

/// Draw a data-mask gauge (frame + arc gauge + sample ellipse) into `rect`.
fn draw_gauge_at(p: &mara_core::MaraPainter, rect: mara_core::vocab::Rect, accent: MaraColor32) {
    use mara_core::vocab::{Align2, Color32, Pos2, Rect, Stroke, Vec2};
    p.rect_filled(rect, 0, Color32::from_rgb(24, 28, 36));
    // Same border as every other view surface (canvas/image use the
    // theme WidgetBorder stroke), so tiled cells read as one family.
    p.rect_stroke(
        rect,
        0,
        mara_core::style::stroke_for(mara_core::style::StrokeRole::WidgetBorder, accent),
    );
    p.text(
        Pos2::new(rect.center().x, rect.top() + 18.0),
        Align2::CENTER_TOP,
        "data mask",
        14.0,
        Color32::from_gray(160),
    );

    let c = rect.center();
    let r = (rect.width().min(rect.height()) * 0.30).max(24.0);
    let deg = |d: f32| d.to_radians();
    let (a0, sweep, value) = (deg(135.0), deg(270.0), 0.62_f32);
    p.arc(
        c,
        Vec2::new(r, r),
        a0,
        a0 + sweep,
        Stroke::new(12.0, Color32::from_gray(70)),
    );
    p.arc(
        c,
        Vec2::new(r, r),
        a0,
        a0 + sweep * value,
        Stroke::new(12.0, accent),
    );
    let na = a0 + sweep * value;
    p.line_segment(
        c,
        Pos2::new(c.x + r * 0.82 * na.cos(), c.y + r * 0.82 * na.sin()),
        Stroke::new(3.5, Color32::WHITE),
    );

    let er = Rect::from_min_size(
        Pos2::new(c.x - 60.0, rect.bottom() - 70.0),
        Vec2::new(120.0, 44.0),
    );
    p.ellipse_filled(er, Color32::from_rgb(70, 110, 200));
    p.ellipse_stroke(er, Stroke::new(2.0, Color32::WHITE));
}

/// Draw one VT soft key (rounded fill + border + label) into `rect`.
fn draw_key_at(p: &mara_core::MaraPainter, rect: mara_core::vocab::Rect, label: &str) {
    use mara_core::vocab::{Align2, Color32};
    // Square, flush key caps — the shared border stroke is the divider,
    // matching the view-surface look everywhere else.
    p.rect_filled(rect, 0, Color32::from_rgb(40, 46, 58));
    p.rect_stroke(
        rect,
        0,
        mara_core::style::stroke_for(
            mara_core::style::StrokeRole::WidgetBorder,
            mara_core::style::active_accent(),
        ),
    );
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        16.0,
        Color32::WHITE,
    );
}

impl Default for ThreeDViewState {
    fn default() -> Self {
        let mut scene = Scene3d::demo("3D");
        add_demo_obj_model(&mut scene);
        Self {
            view: View3d::new("demo-three-d", scene),
            workspace: WorkspaceStack::new("demo-three-d-workspace"),
        }
    }
}

const STANFORD_BUNNY_OBJ: &str = include_str!("../assets/stanford-bunny.obj");
const STANFORD_DRAGON_OBJ: &str = include_str!("../assets/stanford-dragon.obj");

fn add_demo_obj_model(scene: &mut Scene3d) {
    add_demo_obj_asset(
        scene,
        "Stanford Bunny",
        STANFORD_BUNNY_OBJ,
        [-1.1, 0.0, -3.65],
        MaraColor32::from_rgb(218, 186, 142),
    );
    add_demo_obj_asset(
        scene,
        "Stanford Dragon",
        STANFORD_DRAGON_OBJ,
        [1.15, 0.0, -3.65],
        MaraColor32::from_rgb(155, 205, 220),
    );
}

fn add_demo_obj_asset(
    scene: &mut Scene3d,
    label: &str,
    obj: &str,
    translation: [f32; 3],
    color: MaraColor32,
) {
    let mut reader = Cursor::new(obj.as_bytes());
    let options = tobj::LoadOptions {
        triangulate: true,
        single_index: true,
        ..Default::default()
    };
    let Ok((models, _materials)) = tobj::load_obj_buf(&mut reader, &options, |_| {
        Err(tobj::LoadError::OpenFileFailed)
    }) else {
        return;
    };
    if models.is_empty() {
        return;
    }

    let Some(bounds) = obj_bounds(&models) else {
        return;
    };
    let center = [
        (bounds.min[0] + bounds.max[0]) * 0.5,
        (bounds.min[1] + bounds.max[1]) * 0.5,
        (bounds.min[2] + bounds.max[2]) * 0.5,
    ];
    let extent = (bounds.max[0] - bounds.min[0])
        .max(bounds.max[1] - bounds.min[1])
        .max(bounds.max[2] - bounds.min[2])
        .max(1.0e-6);
    let scale = 1.45 / extent;
    let height = (bounds.max[1] - bounds.min[1]) * scale;
    let figurine_material = scene.add_material(format!("Downloaded {label} OBJ"), color);

    for (index, model) in models.into_iter().enumerate() {
        let mesh = &model.mesh;
        if mesh.positions.len() < 9 || mesh.indices.len() < 3 {
            continue;
        }

        let vertices = mesh
            .positions
            .chunks_exact(3)
            .map(|position| {
                [
                    (position[0] - center[0]) * scale,
                    (position[1] - center[1]) * scale,
                    (position[2] - center[2]) * scale,
                ]
            })
            .collect::<Vec<_>>();
        let indices = mesh
            .indices
            .chunks_exact(3)
            .map(|triangle| [triangle[0], triangle[1], triangle[2]])
            .collect::<Vec<_>>();
        let normals = if mesh.normals.len() / 3 == vertices.len() {
            mesh.normals
                .chunks_exact(3)
                .map(|normal| [normal[0], normal[1], normal[2]])
                .collect()
        } else {
            Vec::new()
        };
        let uvs = if mesh.texcoords.len() / 2 == vertices.len() {
            mesh.texcoords
                .chunks_exact(2)
                .map(|uv| [uv[0], uv[1]])
                .collect()
        } else {
            Vec::new()
        };

        let mesh = if normals.len() == vertices.len() {
            TriangleMesh3d {
                vertices,
                indices,
                normals,
                uvs,
                vertex_colors: Vec::new(),
            }
        } else if uvs.len() == vertices.len() {
            TriangleMesh3d::with_generated_normals_and_uvs(vertices, indices, uvs)
        } else {
            TriangleMesh3d::with_generated_normals(vertices, indices)
        };

        let object = scene.add_mesh_object(
            if model.name.is_empty() {
                format!("Downloaded {label} OBJ")
            } else {
                format!("{label} {}", model.name)
            },
            mesh,
            figurine_material,
        );
        if let Some(object) = scene.object_mut(object) {
            object.transform.translation = [
                translation[0],
                translation[1] + height * 0.5 + 0.02,
                translation[2] + index as f32 * 0.03,
            ];
        }
    }
}

#[derive(Clone, Copy)]
struct ObjBounds {
    min: [f32; 3],
    max: [f32; 3],
}

fn obj_bounds(models: &[tobj::Model]) -> Option<ObjBounds> {
    let mut bounds = ObjBounds {
        min: [f32::INFINITY; 3],
        max: [f32::NEG_INFINITY; 3],
    };
    let mut any = false;
    for model in models {
        for position in model.mesh.positions.chunks_exact(3) {
            any = true;
            for (axis, &coord) in position.iter().enumerate() {
                bounds.min[axis] = bounds.min[axis].min(coord);
                bounds.max[axis] = bounds.max[axis].max(coord);
            }
        }
    }
    any.then_some(bounds)
}

#[derive(Default)]
struct CanvasShelfState(ShelfState);

/// Shelf state for the Graph Lab view, kept separate so toggling its
/// guide does not disturb the canvas view's shelves.
#[derive(Default)]
struct GraphLabShelfState(ShelfState);

struct MapViewState {
    surface: MapSurface,
    interaction: MapInteraction,
    workspace: WorkspaceStack,
}

impl Default for MapViewState {
    fn default() -> Self {
        let center = lon_lat(4.904_138_9, 52.367_573_4);
        let mut document = MapDocument::new("Map");
        document.add(MapPoint::new("origin", center).label("origin"));
        document.add(MapLine::new(
            "line",
            vec![
                lon_lat(4.902_8, 52.367_1),
                lon_lat(4.904_2, 52.368_0),
                lon_lat(4.906_0, 52.367_4),
            ],
        ));
        document.add(MapPolygon::new(
            "polygon",
            vec![
                lon_lat(4.903_2, 52.366_9),
                lon_lat(4.905_2, 52.366_9),
                lon_lat(4.905_0, 52.368_2),
                lon_lat(4.903_0, 52.368_0),
            ],
        ));
        document.add(MapIcon::fluent(
            "robot",
            lon_lat(4.904_9, 52.367_8),
            "location",
        ));
        document.add(MapIcon::svg(
            "svg-marker",
            lon_lat(4.903_6, 52.367_8),
            DEFAULT_SVG_MARKER,
        ));
        Self {
            surface: MapSurface::new("demo-map", document, MapViewport::new(center, 15.0)),
            interaction: MapInteraction::default(),
            workspace: WorkspaceStack::new("demo-map-workspace"),
        }
    }
}

fn map_ribbon_items(root_view: DemoRootView) -> &'static [RibbonButtonSpec] {
    match root_view {
        DemoRootView::CorevizManagement => RIBBON_ITEMS_MAP_MANAGEMENT_VIEW,
        _ => RIBBON_ITEMS_MAP_VIEW,
    }
}

/// The editor pane's camera and selection.
///
/// A plain value the app owns, so it is independent of the Graph Lab's
/// by construction. The renderer keeps nothing of its own between
/// frames.
#[derive(Default)]
struct EditorNodeView(mara::extras::graph::render::GraphViewState);

/// The persistent node graph for the editor pane — same cross-frame
/// lifetime story as `EditorNodeView` so node edits + connections
/// survive between frames.
struct EditorGraph(Graph<GraphNode>);

impl Default for EditorGraph {
    fn default() -> Self {
        Self(default_graph())
    }
}

// ─── App ───────────────────────────────────────────────────────────

/// Root eframe app. Holds what the Bevy demo kept as `Resource`s:
/// theme / accent state, ribbon state, the canvas whiteboard, and the
/// editor pane's node graph + sharp-zoom render state.
#[derive(Default)]
pub struct DemoApp {
    accent: AccentColor,
    glass: GlassOpacity,
    open: RibbonOpen,
    placement: RibbonPlacement,
    drag: RibbonDrag,
    family: ThemeFamily,
    mode: ThemeModeRes,
    pastel: PastelToggle,
    tint: TintRgba,
    root_view: DemoRootView,
    canvas_view: CanvasViewState,
    three_d_view: ThreeDViewState,
    tabs: DemoTabs,
    canvas_shelves: CanvasShelfState,
    graph_lab: GraphLabState,
    graph_lab_shelf: GraphLabShelfState,
    map_view: MapViewState,
    bevy_view: MaraBevyViewport,
    bevy_workspace: WorkspaceStack,
    bevy_hosted_scene: bool,
    editor_node_view: EditorNodeView,
    editor_graph: EditorGraph,
    // Enforced shell bar plumbing. The permanent top bar (view
    // switcher + window controls) is now owned by the host adapter's
    // shell, not hand-rolled here — this demo dogfoods it. Top-bar
    // interactions arrive as `ShellEvent`s and are replayed into the
    // existing ribbon click-dispatch as synthetic clicks, so the
    // intricate per-view side effects stay in one place.
    pending_shell_events: Vec<mara_core::ShellEvent>,
    last_fs_active: bool,
    // App-side shell state, used only on hosts with no mara adapter to
    // enforce the bar for us (the eframe/web `eframe::App` path). On
    // the native `mara::window` runner and the Bevy plugin the host
    // owns this and renders the bar itself.
    shell: mara_core::ShellBar,
    shell_open: RibbonOpen,
    shell_placement: RibbonPlacement,
    shell_drag: RibbonDrag,
}

impl DemoApp {
    /// Fill a shell bar from current demo state — shared by all hosts.
    pub fn configure_shell_bar(&self, bar: &mut mara_core::ShellBar) {
        configure_demo_shell(bar, self.root_view, self.last_fs_active);
    }

    /// Queue a top-bar event for `ui_system` to replay into the ribbon
    /// dispatch (used by host adapters that deliver events out-of-band).
    pub fn queue_shell_event(&mut self, event: mara_core::ShellEvent) {
        self.pending_shell_events.push(event);
    }
}

/// The 5 root views, as the enforced shell bar's switcher entries.
/// Order mirrors the old hand-rolled persistent-top bar.
fn demo_shell_views() -> Vec<mara_core::ShellView> {
    vec![
        mara_core::ShellView::new(ACTION_VIEW_BEVY, "cube", "Bevy scene view"),
        mara_core::ShellView::new(ACTION_VIEW_CANVAS, "pen", "Canvas / whiteboard view"),
        mara_core::ShellView::new(ACTION_COREVIZ_ZONES, "draw-shape", "Map annotation view"),
        mara_core::ShellView::new(
            ACTION_COREVIZ_MANAGEMENT,
            "location",
            "Map object selection view",
        ),
        mara_core::ShellView::new(ACTION_VIEW_3D, "cube", "Three-d scene view"),
        mara_core::ShellView::new(
            ACTION_VIEW_BOARD,
            "square-multiple",
            "Board view (single board)",
        ),
        mara_core::ShellView::new(ACTION_VIEW_MULTI, "grid", "Multiview (split into views)"),
        mara_core::ShellView::new(
            ACTION_VIEW_GRAPHLAB,
            GRAPH_LAB_ICON_VIEW,
            "Graph Lab — groups, subgraphs, visuals",
        ),
    ]
}

/// The shell view id for the active root view (drives the highlight).
fn shell_active_view_id(root_view: DemoRootView) -> &'static str {
    match root_view {
        DemoRootView::BevyScene => ACTION_VIEW_BEVY,
        DemoRootView::Canvas => ACTION_VIEW_CANVAS,
        DemoRootView::ThreeD => ACTION_VIEW_3D,
        DemoRootView::Board => ACTION_VIEW_BOARD,
        DemoRootView::Multi => ACTION_VIEW_MULTI,
        DemoRootView::CorevizZones => ACTION_COREVIZ_ZONES,
        DemoRootView::CorevizManagement => ACTION_COREVIZ_MANAGEMENT,
        DemoRootView::GraphLab => ACTION_VIEW_GRAPHLAB,
    }
}

/// Populate a [`ShellBar`](mara_core::ShellBar) from demo state. Shared
/// by both hosts so the bar is identical everywhere. The bar is
/// enforced and always renders — including over fullscreen widgets,
/// which paint full-bleed behind the glass bar.
fn configure_demo_shell(bar: &mut mara_core::ShellBar, root_view: DemoRootView, _fs_active: bool) {
    bar.app_menu = true;
    bar.views = demo_shell_views();
    bar.active = Some(shell_active_view_id(root_view));
}

/// Translate a top-bar [`ShellEvent`](mara_core::ShellEvent) into the
/// synthetic ribbon click the existing dispatch already understands, so
/// view switches / shelf toggles reuse the same side-effect code.
fn shell_event_to_click(event: mara_core::ShellEvent) -> Option<RibbonSlotClick> {
    let (item, action) = match event {
        mara_core::ShellEvent::ViewSelected(id) => (MaraId::new(id), ribbon_action(id)),
        mara_core::ShellEvent::LeftShelfToggled => (
            MaraId::new("shell.left_shelf"),
            RibbonAction::Command(mara_core::left_shelf_command_id()),
        ),
        mara_core::ShellEvent::RightShelfToggled => (
            MaraId::new("shell.right_shelf"),
            RibbonAction::Command(mara_core::right_shelf_command_id()),
        ),
        mara_core::ShellEvent::BottomShelfToggled => (
            MaraId::new("shell.bottom_shelf"),
            RibbonAction::Command(mara_core::bottom_shelf_command_id()),
        ),
        // Menu has no demo behavior yet; close/maximize are handled by
        // the host adapter and never reach the app.
        mara_core::ShellEvent::MenuOpened
        | mara_core::ShellEvent::CloseRequested
        | mara_core::ShellEvent::MaximizeToggleRequested => return None,
    };
    Some(RibbonSlotClick {
        ribbon: MaraId::new(RIBBON_TOP),
        item,
        action,
    })
}

impl DemoApp {
    /// Built once by `eframe::WebRunner`. No persistence — every
    /// session starts from the default mara layout. eframe-only, so it
    /// is excluded on Android (which uses the Mara Android runner).
    #[cfg(not(target_os = "android"))]
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            bevy_view: MaraBevyViewport::with_content(crate::bevy_content::configure_app),
            bevy_workspace: WorkspaceStack::new("demo-bevy-workspace"),
            ..Self::default()
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_winit(gpu: Option<mara::ui::mara_gpu::MaraRenderState<'_>>) -> Self {
        Self {
            bevy_view: MaraBevyViewport::with_render_state_and_content(
                gpu,
                crate::bevy_content::configure_app,
            ),
            bevy_workspace: WorkspaceStack::new("demo-bevy-workspace"),
            // TEMP probe: start on a shelf-bearing view to inspect window-control persistence.
            root_view: std::env::var("MARA_START_VIEW")
                .ok()
                .and_then(|v| match v.as_str() {
                    "canvas" => Some(DemoRootView::Canvas),
                    "threed" => Some(DemoRootView::ThreeD),
                    "board" => Some(DemoRootView::Board),
                    "multi" => Some(DemoRootView::Multi),
                    "zones" => Some(DemoRootView::CorevizZones),
                    "mgmt" => Some(DemoRootView::CorevizManagement),
                    _ => None,
                })
                .unwrap_or_default(),
            ..Self::default()
        }
    }

    /// Build the same Mara demo state for a Bevy-owned window.
    ///
    /// This is retained for local experiments, but the canonical app
    /// path is Mara-owned: Mara owns egui and embeds Bevy as a
    /// viewport instead of routing UI through a Bevy egui bridge.
    pub fn new_bevy_hosted() -> Self {
        Self {
            bevy_hosted_scene: true,
            bevy_workspace: WorkspaceStack::new("demo-bevy-workspace"),
            ..Self::default()
        }
    }

    pub fn set_accent_color(&mut self, color: MaraColor32) {
        self.accent.0 = color;
    }

    /// One frame of the demo, against a Mara host context.
    ///
    /// Backend-free (PLAN.md WS-F6): the eframe pass that produces the
    /// `MaraHostCtx` lives in `crate::host`, and this is everything
    /// after it.
    pub(crate) fn update_frame(&mut self, host: &mut MaraHostCtx<'_>) {
        ui_system(self, host);

        // Web/eframe has no mara host adapter to enforce the shell bar,
        // so the demo renders it app-side here. (On native/bevy the
        // runner/plugin does this for us.) The same `mara_core::ShellBar`
        // is used everywhere — this is dogfooding, not a fork. `mem::take`
        // lets the bar render against the sibling `shell_*` fields
        // without a borrow conflict.
        let mut bar = std::mem::take(&mut self.shell);
        configure_demo_shell(&mut bar, self.root_view, self.last_fs_active);
        let events = host.show_shell_bar(
            &mut bar,
            &mut self.shell_open,
            &mut self.shell_placement,
            &mut self.shell_drag,
        );
        self.shell = bar;
        for event in events {
            if !matches!(
                event,
                mara_core::ShellEvent::CloseRequested
                    | mara_core::ShellEvent::MaximizeToggleRequested
            ) {
                self.pending_shell_events.push(event);
            }
        }
    }

    #[must_use]
    pub fn bevy_host_scene_visible(&self) -> bool {
        self.bevy_hosted_scene && self.root_view == DemoRootView::BevyScene
    }
}

// The window-owning runner differs by platform but exposes the same
// `WindowApp` contract: `mara::window` on desktop, `mara::android` on
// Android. Alias whichever applies so this single impl serves both.
#[cfg(target_os = "android")]
use mara::android::{CreationContext as RunnerCreationContext, WindowApp as RunnerWindowApp};
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
use mara::window::{CreationContext as RunnerCreationContext, WindowApp as RunnerWindowApp};

#[cfg(not(target_arch = "wasm32"))]
impl RunnerWindowApp for DemoApp {
    fn new(ctx: RunnerCreationContext<'_>) -> Self {
        Self::new_winit(ctx.gpu())
    }

    fn update(&mut self, host: &mut MaraHostCtx<'_>) {
        ui_system(self, host);
    }

    fn configure_shell(&mut self, bar: &mut mara::ui::ShellBar) {
        configure_demo_shell(bar, self.root_view, self.last_fs_active);
    }

    fn on_shell_event(&mut self, event: mara::ui::ShellEvent, _host: &mut MaraHostCtx<'_>) {
        // Queue for ui_system to replay into the ribbon dispatch.
        self.pending_shell_events.push(event);
    }
}

// ─── UI ─────────────────────────────────────────────────────────────

/// Per-frame UI — the body of the old Bevy `ui_system`, now driven by
/// eframe/winit. `app` carries the state the Bevy build held as
/// resources; `host` provides app-level actions and render helpers.
pub fn ui_system(app: &mut DemoApp, host: &mut MaraHostCtx<'_>) {
    let DemoApp {
        accent,
        glass,
        open,
        placement,
        drag,
        family,
        mode,
        pastel,
        tint,
        root_view,
        canvas_view,
        three_d_view,
        tabs,
        canvas_shelves,
        graph_lab,
        graph_lab_shelf,
        map_view,
        bevy_view,
        bevy_workspace,
        bevy_hosted_scene,
        editor_node_view,
        editor_graph,
        pending_shell_events,
        last_fs_active,
        // App-side shell state (`shell`, `shell_open`, …) is only used
        // by the eframe/web path in `update_with_render_state`.
        ..
    } = app;
    let mut active_theme = match (family.0, mode.0) {
        (0, 0) => mara_core::style::theme_pro(Mode::Dark),
        (0, 1) => mara_core::style::theme_pro(Mode::Light),
        (1, 0) => mara_core::style::theme_game(Mode::Dark),
        (1, 1) => mara_core::style::theme_game(Mode::Light),
        (2, 0) => mara_core::style::theme_flat(Mode::Dark),
        (2, 1) => mara_core::style::theme_flat(Mode::Light),
        _ => mara_core::style::theme_pro(Mode::Dark),
    };
    active_theme.pastel_accent = pastel.0;
    mara_core::style::set_theme(active_theme);
    host.apply_theme(*accent, *glass);

    let mut accent_col = mara_core::style::active_accent();
    host.publish_full_shelf_layout();

    let bevy_view_active = *root_view == DemoRootView::BevyScene && !*bevy_hosted_scene;
    bevy_view.set_active(bevy_view_active);
    if !root_view.is_coreviz() {
        let size = host.content_rect().size();
        let MapViewState {
            surface, workspace, ..
        } = map_view;
        let map_ctx = host.view_ctx(workspace, accent_col, RibbonAvoidance::all());
        surface.prewarm_tiles(&map_ctx, size);
    }

    // Actual root/L0 canvas switch. The app shell is always eframe;
    // Bevy is represented as an embedded viewport surface, not the
    // top-level window owner.
    if bevy_view_active {
        let mut bevy_ctx = host.view_ctx(bevy_workspace, accent_col, RibbonAvoidance::all());
        if let Some(color) = bevy_view.show(&mut bevy_ctx, host.gpu(), accent_col) {
            accent.0 = color;
            host.apply_theme(*accent, *glass);
            accent_col = mara_core::style::active_accent();
        }
    } else if *root_view == DemoRootView::Canvas {
        canvas_root_view(host, accent_col, canvas_view, &mut canvas_shelves.0);
    } else if *root_view == DemoRootView::ThreeD {
        three_d_root_view(host, accent_col, three_d_view);
    } else if *root_view == DemoRootView::Board {
        tabs.0.select(ACTION_VIEW_BOARD);
        tab_root_view(host, accent_col, &mut tabs.0);
    } else if *root_view == DemoRootView::Multi {
        tabs.0.select(ACTION_VIEW_MULTI);
        tab_root_view(host, accent_col, &mut tabs.0);
    } else if *root_view == DemoRootView::GraphLab {
        graph_lab_root_view(
            host,
            accent_col,
            graph_lab,
            &mut graph_lab_shelf.0,
            host.input_time(),
        );
    } else if root_view.is_coreviz() {
        map_root_view(
            host,
            map_view,
            accent_col,
            *root_view == DemoRootView::CorevizManagement,
        );
    }

    // Fullscreen-view branch. The fullscreen overlay paints at
    // `Order::Background`, so the ribbon assembly below (drawn at
    // `Order::Middle`) layers over the maximised canvas. Which set
    // of items the rails carry depends on WHICH widget is fullscreen
    // — graph and code get their own toolsets, picked via the
    // module-supplied fullscreen keys.
    let fs_active = host.is_any_fullscreen();
    // Stash for the host adapter's shell config (hide the enforced bar
    // while a widget is fullscreen — the demo shows its own restore rail).
    *last_fs_active = fs_active;
    let fullscreen_owner = host.fullscreen_owner();
    let graph_fs = fullscreen_owner == Some(mara::extras::graph::graph_fullscreen_key());
    let code_fs = fullscreen_owner
        == Some(mara::extras::code::code_fullscreen_key(cid(
            PANE_EDITOR,
            "code_state",
        )));
    if fs_active {
        // The persistent main bar owns module restore in L1/fullscreen.
        // Suppress the old floating restore chip so it does not stack
        // above the top-right system-control slot.
        host.set_fullscreen_minimize_chip_visible(false);
    }
    let allow_persistent_panes_over_fullscreen = fs_active
        && open.get(RIBBON_TOP).is_some_and(|id| {
            matches!(
                id,
                PANE_ABOUT | PANE_WIDGETS | PANE_CONTAINERS | PANE_SCENE | PANE_THEME | PANE_KEYS
            )
        });
    // Ribbon assembly is rendered AFTER the pane loop below — see
    // the trailing `ribbon renderer` call. The ribbon `Area`s share
    // `Order::Foreground` with the `embed` fullscreen overlay, so
    // they must register later to land on top of it. Click handling
    // happens during that paint; pane `open` state is read one
    // frame later (~16 ms — imperceptible).
    //
    // IMPORTANT: pane buttons must resolve against the CURRENT root
    // view's item set. Canvas now carries the same four-edge demo
    // ribbon layout as the Bevy view, with only the top ribbon being
    // persistent.
    let current_ribbon_items: &[RibbonButtonSpec] = if *root_view == DemoRootView::Multi {
        // The multiview's cells own their own per-view ribbons
        // (ViewNode leaves), so the app draws no window-level rails here.
        &[]
    } else if fs_active && graph_fs {
        RIBBON_ITEMS_FS_GRAPH
    } else if fs_active && code_fs {
        RIBBON_ITEMS_FS_CODE
    } else if fs_active {
        RIBBON_ITEMS_FS_GRAPH
    } else if *root_view == DemoRootView::Canvas {
        RIBBON_ITEMS_ROOT_VIEW
    } else if *root_view == DemoRootView::ThreeD {
        RIBBON_ITEMS_3D_VIEW
    } else if root_view.is_coreviz() {
        map_ribbon_items(*root_view)
    } else {
        RIBBON_ITEMS
    };
    let current_ribbons: &[RibbonSpec] = if *root_view == DemoRootView::Multi {
        &[]
    } else if fs_active {
        RIBBONS_FS
    } else {
        RIBBONS
    };
    publish_current_pane_ribbon_buttons(host, current_ribbon_items, fs_active);

    let is_open_in = |items: &[RibbonButtonSpec], id: &'static str| -> bool {
        let Some(item) = find_item(items, id) else {
            return false;
        };
        let (rid, _, _) = placement.resolve_parts(item.id, item.ribbon, item.cluster, item.slot);
        open.is_open(rid, id)
    };
    let is_open = |id: &'static str| -> bool { is_open_in(current_ribbon_items, id) };
    let live_anchor = |id: &'static str| -> Option<PaneAnchor> {
        let item = find_item(current_ribbon_items, id)?;
        let (rid, cluster, _) =
            placement.resolve_parts(item.id, item.ribbon, item.cluster, item.slot);
        let def = find_ribbon(current_ribbons, rid)?;
        let zone = match cluster {
            RibbonCluster::Start => RailZone::Start,
            RibbonCluster::Middle => RailZone::Middle,
            RibbonCluster::End => RailZone::End,
        };
        // The ribbon button may be relocated by the phone reflow (top →
        // bottom, top/bottom → side). Anchor the pane to the button's
        // CURRENT edge so it opens where the icon actually is.
        let edge = mara_core::phone_remapped_ribbon_edge(def.edge, cluster, demo_ribbon_scope(rid));
        Some(match edge {
            RibbonEdge::Left => PaneAnchor::LeftRail(zone),
            RibbonEdge::Right => PaneAnchor::RightRail(zone),
            RibbonEdge::Top => PaneAnchor::TopRail(zone),
            RibbonEdge::Bottom => PaneAnchor::BottomRail(zone),
        })
    };

    // In fullscreen/module mode the maximizable owner MUST render
    // first, because it registers the full-window overlay at
    // `Order::Foreground`. Persistent panes are then rendered after
    // it and also lifted to `Foreground`, so they appear on top of
    // the module canvas instead of being hidden behind it.
    if fs_active && is_open_in(RIBBON_ITEMS, PANE_EDITOR) {
        let anchor = live_anchor(PANE_EDITOR).unwrap_or(PaneAnchor::RightRail(RailZone::End));
        let now = host.input_time();
        let mut viewer = DemoViewer {
            time: now,
            ..DemoViewer::default()
        };
        host.show_pane(
            Pane::new(PANE_EDITOR, "Editor", anchor, accent_col)
                .resize(mara_core::pane::PaneResize::SPAN),
            |body| {
                editor_pane(
                    body,
                    &mut editor_node_view.0,
                    &mut editor_graph.0,
                    &mut viewer,
                    accent_col,
                );
            },
        );
    }

    for &(_, button_id, default_anchor, label) in PANE_DEFS {
        let is_fullscreen_owner_pane = fs_active && button_id == PANE_EDITOR;
        if is_fullscreen_owner_pane {
            continue;
        }
        if find_item(current_ribbon_items, button_id).is_none() && !is_fullscreen_owner_pane {
            continue;
        }
        let pane_is_open = if fs_active {
            is_open_in(RIBBON_ITEMS, button_id)
        } else {
            is_open(button_id)
        };
        if !pane_is_open {
            continue;
        }
        if fs_active && !is_fullscreen_owner_pane && !allow_persistent_panes_over_fullscreen {
            continue;
        }
        let anchor = live_anchor(button_id).unwrap_or(default_anchor);
        // Editor pane uses non-`'static` borrows that have to outlive
        // host pane rendering — the typed `PaneBody::add_graph_view`
        // stores them in the pending-spec list and the closure runs at
        // `body.finish()` time (after the user closure returns). Lift
        // `viewer` to the iteration scope so it lives past that point.
        if button_id == PANE_EDITOR {
            let now = host.input_time();
            let mut viewer = DemoViewer {
                time: now,
                ..DemoViewer::default()
            };
            host.show_pane(
                Pane::new(button_id, label, anchor, accent_col)
                    .resize(mara_core::pane::PaneResize::SPAN),
                |body| {
                    editor_pane(
                        body,
                        &mut editor_node_view.0,
                        &mut editor_graph.0,
                        &mut viewer,
                        accent_col,
                    );
                },
            );
            continue;
        }
        host.show_pane(
            Pane::new(button_id, label, anchor, accent_col)
                .resize(mara_core::pane::PaneResize::SPAN)
                .order(if fs_active {
                    mara_core::layout::Layer::Foreground
                } else {
                    mara_core::layout::Layer::Middle
                }),
            |body| match button_id {
                PANE_WIDGETS => widgets_pane(body),
                PANE_CONTAINERS => containers_pane(body),
                PANE_SCENE => scene_pane(body),
                PANE_THEME => theme_pane(body, accent, glass, family, mode, pastel, tint),
                PANE_KEYS => keys_pane(body),
                PANE_ABOUT => about_pane(body),
                PANE_CANVAS_BRUSH => canvas_brush_pane(body),
                PANE_CANVAS_LAYERS => canvas_layers_pane(body),
                PANE_CANVAS_ASSETS => canvas_assets_pane(body),
                PANE_CANVAS_INSPECTOR => canvas_inspector_pane(body),
                PANE_CANVAS_HISTORY => canvas_history_pane(body),
                PANE_CANVAS_EXPORT => canvas_export_pane(body),
                PANE_3D_SCENE => three_d_scene_pane(body, three_d_view),
                PANE_3D_INSPECTOR => three_d_inspector_pane(body, three_d_view),
                PANE_COREVIZ_ZONES => coreviz_zones_pane(body),
                PANE_COREVIZ_REFERENCE => coreviz_reference_pane(body),
                PANE_COREVIZ_NODES => coreviz_nodes_pane(body),
                PANE_COREVIZ_EDGES => coreviz_edges_pane(body),
                PANE_COREVIZ_ZENOH => coreviz_zenoh_pane(body),
                PANE_COREVIZ_ROBOTS => coreviz_robots_pane(body),
                PANE_COREVIZ_DETAILS => coreviz_details_pane(body),
                PANE_MAP_INFO => map_info_pane(body, map_view),
                PANE_MAP_OBJECTS => map_objects_pane(body, map_view),
                PANE_COREVIZ_JSON => coreviz_json_pane(body),
                PANE_COREVIZ_SCHEDULER => coreviz_scheduler_pane(body),
                PANE_COREVIZ_TASKS => coreviz_tasks_pane(body),
                _ => {}
            },
        );
    }

    // Ribbon paint, AFTER the panes — registration order within
    // `Order::Foreground` lands the ribbon `Area`s on top of the
    // `embed` fullscreen overlay, so the host's fullscreen rails
    // remain visible.
    let mut clicks: Vec<RibbonSlotClick> = if fs_active {
        let fs_items: &[RibbonButtonSpec] = if graph_fs {
            RIBBON_ITEMS_FS_GRAPH
        } else if code_fs {
            RIBBON_ITEMS_FS_CODE
        } else {
            RIBBON_ITEMS_FS_GRAPH
        };
        let mut fs_placement = mara_core::ribbon::RibbonPlacement::default();
        let mut fs_drag = mara_core::ribbon::RibbonDrag::default();
        draw_unified_ribbons(
            host,
            accent_col,
            RIBBONS_FS,
            fs_items,
            open,
            &mut fs_placement,
            &mut fs_drag,
            |id| matches!(id, ACTION_RESTORE_FULLSCREEN),
        )
    } else {
        draw_unified_ribbons(
            host,
            accent_col,
            RIBBONS,
            current_ribbon_items,
            open,
            placement,
            drag,
            |id| match *root_view {
                DemoRootView::BevyScene => id == ACTION_VIEW_BEVY,
                DemoRootView::Canvas => id == ACTION_VIEW_CANVAS,
                DemoRootView::ThreeD => id == ACTION_VIEW_3D,
                DemoRootView::Board => id == ACTION_VIEW_BOARD,
                DemoRootView::GraphLab => id == ACTION_VIEW_GRAPHLAB,
                DemoRootView::Multi => id == ACTION_VIEW_MULTI,
                DemoRootView::CorevizZones => {
                    id == ACTION_COREVIZ_ZONES
                        || matches!(
                            (id, map_view.interaction.tool),
                            (ACTION_MAP_SELECT, MapTool::Select)
                                | (ACTION_MAP_POINT, MapTool::Point)
                                | (ACTION_MAP_LINE, MapTool::Line)
                                | (ACTION_MAP_POLYGON, MapTool::Polygon)
                        )
                }
                DemoRootView::CorevizManagement => {
                    id == ACTION_COREVIZ_MANAGEMENT
                        || matches!(
                            (id, map_view.interaction.tool),
                            (ACTION_MAP_SELECT, MapTool::Select)
                        )
                }
            },
        )
    };
    // Replay the enforced shell bar's interactions (view switch / shelf
    // toggles, collected from the host adapter) as synthetic clicks, so
    // the existing dispatch below handles them with the same per-view
    // side effects as before. Tab-migrated views (Board/Multi) route
    // through `Tabs` first — it consumes their `ViewSelected` — while
    // the click replay still runs to keep `DemoRootView` and the
    // fullscreen-restore side effect identical for every view.
    clicks.extend(pending_shell_events.drain(..).filter_map(|event| {
        tabs.0.on_shell_event(&event);
        shell_event_to_click(event)
    }));
    // PREV / NEXT cube — one-shot icon buttons in the BOTTOM rail's
    // End cluster. Each click rotates the AccentColor through the
    // hardcoded swatch row.
    const SWATCH_RGB: &[(u8, u8, u8)] = &[
        (230, 76, 76),
        (242, 166, 51),
        (242, 230, 76),
        (89, 217, 115),
        (76, 153, 242),
        (191, 115, 242),
    ];
    for click in clicks {
        let item_is = |id: &'static str| click.item == MaraId::new(id);
        if click.action == RibbonAction::Command(mara_core::app_menu_command_id()) {
            open.set(RIBBON_TOP, PANE_ABOUT);
            host.request_repaint();
            continue;
        }
        if click.action == RibbonAction::Command(mara_core::left_shelf_command_id()) {
            canvas_shelves.0.toggle_edge_visible(ShelfEdge::Left);
            // Phone: a small screen shows only one side shelf at a time.
            if mara_core::screen_class() == mara_core::Breakpoint::Phone
                && canvas_shelves.0.edge_visible(ShelfEdge::Left)
            {
                canvas_shelves.0.set_edge_visible(ShelfEdge::Right, false);
            }
            continue;
        }
        if click.action == RibbonAction::Command(mara_core::right_shelf_command_id()) {
            canvas_shelves.0.toggle_edge_visible(ShelfEdge::Right);
            if mara_core::screen_class() == mara_core::Breakpoint::Phone
                && canvas_shelves.0.edge_visible(ShelfEdge::Right)
            {
                canvas_shelves.0.set_edge_visible(ShelfEdge::Left, false);
            }
            continue;
        }
        if click.action == RibbonAction::Command(mara_core::bottom_shelf_command_id()) {
            canvas_shelves.0.toggle_edge_visible(ShelfEdge::Bottom);
            continue;
        }
        if item_is(ACTION_VIEW_BEVY) {
            if fs_active {
                host.restore_fullscreen();
            }
            *root_view = DemoRootView::BevyScene;
            host.request_repaint();
            continue;
        }
        if item_is(ACTION_VIEW_CANVAS) {
            if fs_active {
                host.restore_fullscreen();
            }
            *root_view = DemoRootView::Canvas;
            host.request_repaint();
            continue;
        }
        if item_is(ACTION_VIEW_3D) {
            if fs_active {
                host.restore_fullscreen();
            }
            *root_view = DemoRootView::ThreeD;
            open.set(RIBBON_LEFT, PANE_3D_SCENE);
            open.set(RIBBON_RIGHT, PANE_3D_INSPECTOR);
            host.request_repaint();
            continue;
        }
        if item_is(ACTION_VIEW_BOARD) {
            if fs_active {
                host.restore_fullscreen();
            }
            *root_view = DemoRootView::Board;
            tabs.0.select(ACTION_VIEW_BOARD);
            host.request_repaint();
            continue;
        }
        if item_is(ACTION_VIEW_MULTI) {
            if fs_active {
                host.restore_fullscreen();
            }
            *root_view = DemoRootView::Multi;
            tabs.0.select(ACTION_VIEW_MULTI);
            host.request_repaint();
            continue;
        }
        if item_is(ACTION_VIEW_GRAPHLAB) {
            if fs_active {
                host.restore_fullscreen();
            }
            *root_view = DemoRootView::GraphLab;
            host.request_repaint();
            continue;
        }
        if item_is(ACTION_RESTORE_FULLSCREEN) {
            host.restore_fullscreen();
            continue;
        }
        if matches!(click.action, RibbonAction::CloseApp) {
            #[cfg(not(target_arch = "wasm32"))]
            host.request_close();
            continue;
        }
        if matches!(click.action, RibbonAction::ToggleMaximize) {
            host.request_maximize_toggle();
            continue;
        }
        if item_is(ACTION_CANVAS_CLEAR) {
            canvas_view.strokes.clear();
            continue;
        }
        if item_is(ACTION_COREVIZ_ZONES) {
            if fs_active {
                host.restore_fullscreen();
            }
            *root_view = DemoRootView::CorevizZones;
            map_view.surface.defer_full_detail();
            map_view.interaction.set_tool(MapTool::Select);
            map_view.interaction.clear_selection();
            open.set(RIBBON_LEFT, PANE_COREVIZ_ZONES);
            open.set(RIBBON_RIGHT, PANE_MAP_OBJECTS);
            host.request_repaint();
            continue;
        }
        if item_is(ACTION_COREVIZ_MANAGEMENT) {
            if fs_active {
                host.restore_fullscreen();
            }
            *root_view = DemoRootView::CorevizManagement;
            map_view.surface.defer_full_detail();
            map_view.interaction.set_tool(MapTool::Select);
            map_view.interaction.clear_selection();
            open.set(RIBBON_LEFT, PANE_COREVIZ_ROBOTS);
            open.set(RIBBON_RIGHT, PANE_MAP_INFO);
            host.request_repaint();
            continue;
        }
        if item_is(ACTION_MAP_SELECT) {
            map_view.interaction.set_tool(MapTool::Select);
            continue;
        }
        if item_is(ACTION_MAP_POINT) {
            map_view.interaction.set_tool(MapTool::Point);
            continue;
        }
        if item_is(ACTION_MAP_LINE) {
            map_view.interaction.set_tool(MapTool::Line);
            continue;
        }
        if item_is(ACTION_MAP_POLYGON) {
            map_view.interaction.set_tool(MapTool::Polygon);
            continue;
        }
        if item_is(ACTION_MAP_CLEAR) {
            map_view.surface.document.annotations.clear();
            map_view.interaction.clear_selection();
            map_view.interaction.clear_draft();
            continue;
        }
        if item_is(ACTION_PREV_CUBE) || item_is(ACTION_NEXT_CUBE) {
            let cur = accent.0;
            let cur_idx = SWATCH_RGB
                .iter()
                .position(|&(r, g, b)| MaraColor32::from_rgb(r, g, b) == cur)
                .unwrap_or(0);
            let next_idx = if item_is(ACTION_PREV_CUBE) {
                (cur_idx + SWATCH_RGB.len() - 1) % SWATCH_RGB.len()
            } else {
                (cur_idx + 1) % SWATCH_RGB.len()
            };
            let (r, g, b) = SWATCH_RGB[next_idx];
            accent.0 = MaraColor32::from_rgb(r, g, b);
        }
    }
}

// ─── Map root view ─────────────────────────────────────────────────

fn map_root_view(
    host: &MaraHostCtx<'_>,
    map: &mut MapViewState,
    accent: MaraColor32,
    basemap_selection_enabled: bool,
) {
    map.interaction.basemap_selection_enabled = basemap_selection_enabled;
    let MapViewState {
        surface,
        interaction,
        workspace,
    } = map;
    let mut map_ctx = host.view_ctx(workspace, accent, RibbonAvoidance::all());
    let _ = MaraMap::new(surface, interaction).show(&mut map_ctx);
}

fn map_info_pane(body: &mut PaneBody, map: &MapViewState) {
    if let Some(feature) = map.interaction.selected_feature.as_ref() {
        show_map_feature_info(body, feature);
    } else if let Some(id) = map.interaction.selected {
        if let Some(annotation) = map.surface.document.get(id) {
            show_map_annotation_info(body, annotation);
        } else {
            body.add_normal(
                cid(PANE_MAP_INFO, "selection"),
                "Selection",
                "info",
                vec![
                    Pod::new(pid(PANE_MAP_INFO, "selection", 0))
                        .with_separator(SeparatorStyle::Line)
                        .with_readout("selection", "missing"),
                    Pod::new(pid(PANE_MAP_INFO, "selection", 1))
                        .with_separator(SeparatorStyle::None)
                        .with_readout("hint", "click map object"),
                ],
            );
        }
    } else {
        body.add_normal(
            cid(PANE_MAP_INFO, "selection"),
            "Selection",
            "info",
            vec![
                Pod::new(pid(PANE_MAP_INFO, "selection", 0))
                    .with_separator(SeparatorStyle::Line)
                    .with_readout("selection", "none"),
                Pod::new(pid(PANE_MAP_INFO, "selection", 1))
                    .with_separator(SeparatorStyle::None)
                    .with_readout("hint", "click map object"),
            ],
        );
    }
}

fn map_objects_pane(body: &mut PaneBody, map: &mut MapViewState) {
    let accent = body.accent();
    let container_id = cid(PANE_MAP_OBJECTS, "objects");
    let selected = map.interaction.selected;
    let selected_annotation = selected.and_then(|id| map.surface.document.get(id));
    let pods = if let Some(annotation) = selected_annotation {
        vec![
            Pod::new(pid(PANE_MAP_OBJECTS, "selected", 0))
                .with_separator(SeparatorStyle::Line)
                .with_readout(map_annotation_label(annotation), id_short(annotation.id())),
            Pod::new(MaraId::new((
                PANE_MAP_OBJECTS,
                "selected-color",
                annotation.id().uuid,
            )))
            .with_separator(SeparatorStyle::None)
            .with_color_rgb("color", map_annotation_rgb(annotation), accent),
        ]
    } else if map.interaction.selected_feature.is_some() {
        vec![
            Pod::new(pid(PANE_MAP_OBJECTS, "feature", 0))
                .with_separator(SeparatorStyle::Line)
                .with_readout("selection", "map feature"),
            Pod::new(pid(PANE_MAP_OBJECTS, "feature", 1))
                .with_separator(SeparatorStyle::None)
                .with_readout("hint", "annotation color only"),
        ]
    } else {
        vec![
            Pod::new(pid(PANE_MAP_OBJECTS, "empty", 0))
                .with_separator(SeparatorStyle::Line)
                .with_readout("selection", "none"),
            Pod::new(pid(PANE_MAP_OBJECTS, "empty", 1))
                .with_separator(SeparatorStyle::None)
                .with_readout("hint", "click point / line / polygon"),
        ]
    };

    body.add_normal(container_id, "Selected Object", "color", pods);

    let responses = body.render();
    let Some(pod_responses) = responses.get(&container_id) else {
        return;
    };
    let Some(color) = pod_responses
        .iter()
        .find_map(|pod_response| pod_response.colors.first())
        .filter(|color| color.changed)
    else {
        return;
    };
    let Some(id) = selected else {
        return;
    };
    let picked = srgb_to_color([color.rgba[0], color.rgba[1], color.rgba[2]]);
    if let Some(annotation) = map
        .surface
        .document
        .annotations
        .iter_mut()
        .find(|annotation| annotation.id() == id)
    {
        set_map_annotation_color(annotation, picked);
    }
}

fn show_map_feature_info(body: &mut PaneBody, feature: &MapFeatureInfo) {
    let mut summary = vec![
        Pod::new(pid(PANE_MAP_INFO, "feature", 0))
            .with_separator(SeparatorStyle::Line)
            .with_readout("type", feature.type_label()),
        Pod::new(pid(PANE_MAP_INFO, "feature", 1))
            .with_separator(if feature.name.is_some() {
                SeparatorStyle::Line
            } else {
                SeparatorStyle::None
            })
            .with_readout("geometry", map_feature_geometry_label(feature.geometry)),
    ];
    if let Some(name) = feature.name.as_deref() {
        summary.push(
            Pod::new(pid(PANE_MAP_INFO, "feature", 2))
                .with_separator(SeparatorStyle::None)
                .with_readout("name", sanitize_readout(name)),
        );
    }
    body.add_normal(cid(PANE_MAP_INFO, "feature"), "Feature", "map", summary);

    let properties = feature
        .properties
        .iter()
        .take(18)
        .enumerate()
        .map(|(idx, (key, value))| {
            Pod::new(pid(PANE_MAP_INFO, "property", idx))
                .with_separator(if idx >= feature.properties.len().min(18) - 1 {
                    SeparatorStyle::None
                } else {
                    SeparatorStyle::Line
                })
                .with_readout(sanitize_readout(key), sanitize_readout(value))
        })
        .collect::<Vec<_>>();
    if !properties.is_empty() {
        body.add_normal(
            cid(PANE_MAP_INFO, "properties"),
            "Properties",
            "document",
            properties,
        );
    }
}

fn show_map_annotation_info(body: &mut PaneBody, annotation: &MapAnnotation) {
    let mut pods = vec![
        Pod::new(pid(PANE_MAP_INFO, "annotation", 0))
            .with_separator(SeparatorStyle::Line)
            .with_readout("type", map_annotation_label(annotation)),
        Pod::new(pid(PANE_MAP_INFO, "annotation", 1))
            .with_separator(SeparatorStyle::Line)
            .with_readout("uuid", annotation.id().hyphenated()),
    ];
    match annotation {
        MapAnnotation::Point(point) => {
            push_geo_position_pods(&mut pods, "annotation", 2, point.position);
            if let Some(label) = point.label.as_deref() {
                pods.push(
                    Pod::new(pid(PANE_MAP_INFO, "annotation", pods.len()))
                        .with_separator(SeparatorStyle::Line)
                        .with_readout("label", sanitize_readout(label)),
                );
            }
        }
        MapAnnotation::Line(line) => {
            pods.push(
                Pod::new(pid(PANE_MAP_INFO, "annotation", pods.len()))
                    .with_separator(SeparatorStyle::Line)
                    .with_readout("points", line.points.len().to_string()),
            );
            if let Some(label) = line.label.as_deref() {
                pods.push(
                    Pod::new(pid(PANE_MAP_INFO, "annotation", pods.len()))
                        .with_separator(SeparatorStyle::Line)
                        .with_readout("label", sanitize_readout(label)),
                );
            }
        }
        MapAnnotation::Polygon(poly) => {
            pods.push(
                Pod::new(pid(PANE_MAP_INFO, "annotation", pods.len()))
                    .with_separator(SeparatorStyle::Line)
                    .with_readout("points", poly.points.len().to_string()),
            );
            if let Some(label) = poly.label.as_deref() {
                pods.push(
                    Pod::new(pid(PANE_MAP_INFO, "annotation", pods.len()))
                        .with_separator(SeparatorStyle::Line)
                        .with_readout("label", sanitize_readout(label)),
                );
            }
        }
        MapAnnotation::Icon(icon) => {
            push_geo_position_pods(&mut pods, "annotation", 2, icon.position);
            if let Some(label) = icon.label.as_deref() {
                pods.push(
                    Pod::new(pid(PANE_MAP_INFO, "annotation", pods.len()))
                        .with_separator(SeparatorStyle::Line)
                        .with_readout("label", sanitize_readout(label)),
                );
            }
        }
    }
    body.add_normal(
        cid(PANE_MAP_INFO, "annotation"),
        "Annotation",
        "location",
        pods,
    );
}

fn push_geo_position_pods(
    pods: &mut Vec<Pod>,
    container: &'static str,
    start: usize,
    position: mara_map::GeoPosition,
) {
    pods.push(
        Pod::new(pid(PANE_MAP_INFO, container, start))
            .with_separator(SeparatorStyle::Line)
            .with_readout("lon", format!("{:.9}", position.lon)),
    );
    pods.push(
        Pod::new(pid(PANE_MAP_INFO, container, start + 1))
            .with_separator(SeparatorStyle::Line)
            .with_readout("lat", format!("{:.9}", position.lat)),
    );
}

fn sanitize_readout(value: impl AsRef<str>) -> String {
    let value = value.as_ref().trim();
    if value.is_empty() {
        "—".to_owned()
    } else {
        value.to_owned()
    }
}

fn map_feature_geometry_label(geometry: MapFeatureGeometry) -> &'static str {
    match geometry {
        MapFeatureGeometry::Point => "point",
        MapFeatureGeometry::Line => "line",
        MapFeatureGeometry::Polygon => "polygon",
    }
}

fn map_annotation_label(annotation: &MapAnnotation) -> &'static str {
    match annotation {
        MapAnnotation::Point(_) => "point",
        MapAnnotation::Line(_) => "line",
        MapAnnotation::Polygon(_) => "polygon",
        MapAnnotation::Icon(_) => "icon",
    }
}

fn id_short(id: mara_map::MapAnnotationId) -> String {
    format!("{:08x}", (id.uuid & 0xffff_ffff) as u32)
}

fn map_annotation_rgb(annotation: &MapAnnotation) -> [f32; 3] {
    color32_to_rgb(match annotation {
        MapAnnotation::Point(point) => point.color,
        MapAnnotation::Line(line) => line.color,
        MapAnnotation::Polygon(poly) => poly.stroke.color,
        MapAnnotation::Icon(icon) => icon.color,
    })
}

fn set_map_annotation_color(annotation: &mut MapAnnotation, color: MaraColor32) {
    match annotation {
        MapAnnotation::Point(point) => point.color = color,
        MapAnnotation::Line(line) => line.color = color,
        MapAnnotation::Polygon(poly) => {
            poly.stroke.color = color;
            poly.fill = MaraColor32::from_rgba_unmultiplied(
                color.r(),
                color.g(),
                color.b(),
                poly.fill.a().max(36),
            );
        }
        MapAnnotation::Icon(icon) => icon.color = color,
    }
}

fn color32_to_rgb(color: MaraColor32) -> [f32; 3] {
    [
        color.r() as f32 / 255.0,
        color.g() as f32 / 255.0,
        color.b() as f32 / 255.0,
    ]
}

// ─── 3D root view ──────────────────────────────────────────────────

fn three_d_root_view(
    host: &mut MaraHostCtx<'_>,
    accent: MaraColor32,
    three_d: &mut ThreeDViewState,
) {
    // Honor the view's own decision (default: under the ribbons). The
    // render target format is published by `view_ctx` and pulled inside
    // `View3d::show`, so no per-frame GPU setter is needed here.
    let avoidance = three_d.view.content_avoidance();
    let mut view_ctx = host.view_ctx(&mut three_d.workspace, accent, avoidance);
    three_d.view.show(&mut view_ctx);
}

fn tab_root_view(host: &MaraHostCtx<'_>, accent: MaraColor32, tabs: &mut Tabs) {
    // No avoidance at all: the view tree fills the window from the very
    // top — the glass top bar draws over the cells, not above them.
    let (node, workspace, _shelf_state) = tabs.active_mut();
    let mut view_ctx = host.view_ctx(workspace, accent, RibbonAvoidance::none());
    node.render(&mut view_ctx);
}

// ─── Graph Lab root view ───────────────────────────────────────────
//
// A full-screen node graph built to show the grouping and subgraph
// work in one place. Everything here is ordinary consumer code — the
// same public API any app would use — so it doubles as the worked
// example for `PLAN_NODE.md`.

/// Mints payloads for nodes the crate creates on the app's behalf.
///
/// Collapsing a selection has to make an instance node and one boundary
/// node per derived port, and `Graph<T>` has no way to construct a `T`.
/// Returning `None` from either would abort the collapse and leave the
/// document untouched.
struct GraphLabFactory;

impl mara::extras::graph::NodeFactory<GraphNode> for GraphLabFactory {
    fn instance_node(
        &mut self,
        _def: mara::extras::graph::DefId,
        _name: &str,
        _ports: &mara::extras::graph::Ports,
    ) -> Option<GraphNode> {
        Some(GraphNode::Subgraph)
    }

    fn port_node(&mut self, spec: &mara::extras::graph::PortSpec<'_>) -> Option<GraphNode> {
        Some(port_payload(spec))
    }
}

/// The payload for one boundary node.
///
/// Shared by the two factories the demo has to keep in step — the
/// standalone `GraphLabFactory` and `DemoViewer`'s own `NodeFactory`
/// widening — because two copies of this decision is how a chip built
/// with `Ctrl+G` ends up looking unlike a chip built at startup.
fn port_payload(spec: &mara::extras::graph::PortSpec<'_>) -> GraphNode {
    GraphNode::Port {
        out: matches!(spec.dir, mara::extras::graph::PortDir::Out),
        name: spec.name.to_string(),
    }
}

/// State for the Graph Lab view.
struct GraphLabState {
    doc: mara::extras::graph::GraphDoc<GraphNode>,
    viewer: DemoViewer,
    /// Camera, selection and the level being shown.
    ///
    /// Held here rather than in the backend's keyed memory, so two
    /// graph surfaces in this app are independent because they are two
    /// values — not because two string keys happened not to collide.
    nav: mara::extras::graph::render::DocViewState,
    /// Breadcrumb from the last frame, so the shelf can show where the
    /// canvas currently is. Returned as data rather than painted by the
    /// crate, because the host owns chrome.
    crumbs: Vec<String>,
    /// Depth of the level being shown, for the exit hint.
    depth: usize,
}

impl Default for GraphLabState {
    fn default() -> Self {
        Self {
            doc: build_graph_lab_doc(),
            viewer: DemoViewer::default(),
            nav: mara::extras::graph::render::DocViewState::default(),
            crumbs: vec!["Root".to_string()],
            depth: 0,
        }
    }
}

/// The showcase document.
///
/// Three stacked bands, one feature each, rather than a scatter of
/// one-off nodes: the view has five things to demonstrate at once, and
/// a canvas where every node is a different type, colour and size shows
/// none of them — the differences that carry meaning drown in the
/// differences that carry none.
///
///   * top    — one definition placed twice, each feeding a readout
///   * middle — a group folded to a pill
///   * bottom — a live signal through nested groups
///
/// Colour is a variable here, not decoration. A node's header is its
/// category (rose values, brown wave generators, blue scalar maths,
/// maroon readouts), a chip's header is its definition's colour, and a
/// group's box takes the colour of what it holds — so orange means
/// "signal generation" and blue means "maths" in whichever band they
/// appear, on a node or on a box. Peers therefore look like peers:
/// within any one group every node is the same variant family, which
/// fixes the pin count and the header colour, and `LAB_NODE_W` fixes
/// the last free variable, the width.
pub fn build_graph_lab_doc() -> mara::extras::graph::GraphDoc<GraphNode> {
    let mut doc = mara::extras::graph::GraphDoc::<GraphNode>::new();
    lab_band_chip(&mut doc);
    lab_band_folded(&mut doc);
    lab_band_frames(&mut doc);
    doc
}

/// Every readout in the lab is this size, whatever it is wired to.
///
/// Sizing a chart from its content is how two readouts end up two
/// widths; how big a readout should be is a property of the layout, and
/// `set_size_override` is the only channel that can say so.
const LAB_READOUT_SIZE: mara::ui::vocab::Vec2 = mara::ui::vocab::Vec2::new(260.0, 170.0);

/// Every ordinary node in the lab is at least this wide.
///
/// Width is otherwise measured from content, and content varies for
/// reasons the reader is not meant to notice: an unwired input grows an
/// inline editor, and a longer subtitle grows the header. Two peers in
/// one group then differ in width for no reason the design intends,
/// which is precisely the noise the whole layout exists to remove. The
/// override is a floor, so a node with more to say still grows.
const LAB_NODE_W: f32 = NODE_W;

/// The one node width every graph in this demo uses.
///
/// Shared by the editor pane's graph and the Graph Lab so the two read
/// as the same product. Wide enough for this viewer's inline editors
/// (a drag field plus its label) without forcing every node to be as
/// wide as its widest sibling.
const NODE_W: f32 = 210.0;

/// Usable width inside a node body.
///
/// The renderer fixes node width, so a body widget can fill it rather
/// than guessing. Widgets that guessed low are most of why nodes used
/// to look like mostly-empty boxes.
const BODY_W: f32 = 220.0;

/// Column origins. One node width plus a fixed gutter apart, so a wire
/// always has the same run and the bands stack into a grid.
const LAB_COL: [f32; 4] = [40.0, 320.0, 600.0, 880.0];

/// Where each band starts. Pitch is generous enough that a group box —
/// which adds a title band and padding ABOVE its topmost member — never
/// reaches into the band above it.
const LAB_BAND_Y: [f32; 3] = [120.0, 620.0, 800.0];

/// Bounds passed to `insert_frame` for a group that auto-fits.
///
/// A `shrink` frame recomputes its box from its members every frame, so
/// the rect handed over at construction is never read.
fn lab_auto_bounds() -> mara::ui::vocab::Rect {
    mara::ui::vocab::Rect::from_min_size(
        mara::ui::vocab::Pos2::new(0.0, 0.0),
        mara::ui::vocab::Vec2::new(0.0, 0.0),
    )
}

/// Add an ordinary lab node at the shared width.
fn lab_node(level: &mut Graph<GraphNode>, x: f32, y: f32, payload: GraphNode) -> NodeId {
    let id = level.insert_node(mara_graph_pos(x, y), payload);
    level.set_size_override(id, Some(mara::ui::vocab::Vec2::new(LAB_NODE_W, 0.0)));
    id
}

/// Add a readout node, sized by the app rather than by its content.
fn lab_readout(level: &mut Graph<GraphNode>, x: f32, y: f32) -> NodeId {
    let id = level.insert_node(mara_graph_pos(x, y), GraphNode::Display);
    level.set_size_override(id, Some(LAB_READOUT_SIZE));
    id
}

/// Adopt a node the crate placed on the app's behalf.
///
/// `collapse` drops the instance at the centroid of what it replaced,
/// which is the only sensible default and the wrong spot in a laid-out
/// showcase, where the chip has to line up with its twin. It also
/// leaves the node content-sized, so a chip would be the one node in
/// the view whose width nobody chose — and a chip is exactly the node
/// that must read as first-class.
fn lab_place(level: &mut Graph<GraphNode>, uid: mara::extras::graph::NodeUid, x: f32, y: f32) {
    let Some(id) = level.by_uid(uid) else {
        return;
    };
    if let Some(info) = level.get_node_info_mut(id) {
        info.pos = mara_graph_pos(x, y);
    }
    level.set_size_override(id, Some(mara::ui::vocab::Vec2::new(LAB_NODE_W, 0.0)));
}

/// Top band: one definition, placed twice.
///
/// The chip is built as an ordinary two-node chain and then collapsed,
/// because collapse derives the interface from the wires that cross the
/// selection boundary — the chip's two inputs and one output are a
/// consequence of how it was wired, not something declared up front.
///
/// Both placements sit in the same column at the same width, fed by the
/// same two sources and each feeding its own readout, so the `×2` badge
/// is confirming something the eye has already spotted rather than
/// announcing it — and neither placement reads as the leftover of the
/// other.
fn lab_band_chip(doc: &mut mara::extras::graph::GraphDoc<GraphNode>) {
    use mara::extras::graph::{DefScope, NodePath};

    let y = LAB_BAND_Y[0];
    let signal = lab_node(&mut doc.root, LAB_COL[0], y + 40.0, GraphNode::Number(1.5));
    let amount = lab_node(&mut doc.root, LAB_COL[0], y + 250.0, GraphNode::Number(0.5));
    let scale = lab_node(
        &mut doc.root,
        LAB_COL[1],
        y + 20.0,
        GraphNode::ScalarMath(ScalarOp::Mul),
    );
    let offset = lab_node(
        &mut doc.root,
        LAB_COL[1],
        y + 180.0,
        GraphNode::ScalarMath(ScalarOp::Add),
    );
    let readout = lab_readout(&mut doc.root, LAB_COL[2], y + 20.0);

    doc.root.connect(out_pin(signal, 0), in_pin(scale, 0));
    doc.root.connect(out_pin(amount, 0), in_pin(scale, 1));
    doc.root.connect(out_pin(scale, 0), in_pin(offset, 0));
    doc.root.connect(out_pin(offset, 0), in_pin(readout, 0));

    let members: Vec<_> = [scale, offset]
        .iter()
        .filter_map(|n| doc.root.uid_of(*n))
        .collect();
    let Ok((def, first)) = doc.collapse(
        &NodePath::root(),
        &members,
        DefScope::Shared,
        "Gain stage",
        &mut GraphLabFactory,
    ) else {
        return;
    };
    if let Some(d) = doc.def_mut(def) {
        d.color = Some(mara::extras::graph::palette(5));
    }
    lab_name_ports(doc, def, &["signal", "amount"], &["value"]);
    lab_tidy_def(doc, def);
    lab_place(&mut doc.root, first, LAB_COL[1], y + 30.0);

    let Ok(second) = doc.instantiate(
        &NodePath::root(),
        def,
        mara_graph_pos(LAB_COL[1], y + 240.0),
        &mut GraphLabFactory,
    ) else {
        return;
    };
    lab_place(&mut doc.root, second, LAB_COL[1], y + 240.0);
    let twin_readout = lab_readout(&mut doc.root, LAB_COL[2], y + 230.0);
    if let Some(twin) = doc.root.by_uid(second) {
        doc.root.connect(out_pin(signal, 0), in_pin(twin, 0));
        doc.root.connect(out_pin(amount, 0), in_pin(twin, 1));
        doc.root.connect(out_pin(twin, 0), in_pin(twin_readout, 0));
    }
}

/// Give a definition's derived ports names a reader can use.
///
/// `collapse` names them `in0` / `out0`, which is the only thing it can
/// know and is exactly as informative as no label at all on the
/// instance pins those ports become.
///
/// The second pass re-titles the boundary nodes. `rename_port` renames
/// the port and nothing else, and the boundary nodes were minted with
/// the old name baked into their payload — a node titled `in0` inside a
/// definition whose instance pin reads `signal` is the same
/// inconsistency one level down.
fn lab_name_ports(
    doc: &mut mara::extras::graph::GraphDoc<GraphNode>,
    def: mara::extras::graph::DefId,
    inputs: &[&str],
    outputs: &[&str],
) {
    use mara::extras::graph::PortDir;

    let Some(d) = doc.def(def) else {
        return;
    };
    let renames: Vec<(mara::extras::graph::PortId, String)> = d
        .ports
        .inputs()
        .iter()
        .zip(inputs)
        .chain(d.ports.outputs().iter().zip(outputs))
        .map(|(port, name)| (port.id, (*name).to_string()))
        .collect();
    for (port, name) in renames {
        doc.rename_port(def, port, name);
    }
    let Some(d) = doc.def_mut(def) else {
        return;
    };
    let named: Vec<(mara::extras::graph::PortId, String)> = d
        .ports
        .side(PortDir::In)
        .iter()
        .chain(d.ports.side(PortDir::Out).iter())
        .map(|p| (p.id, p.name.clone()))
        .collect();
    let ids: Vec<NodeId> = d.body.node_ids().map(|(id, _)| id).collect();
    for id in ids {
        let Some(uid) = d.body.uid_of(id) else {
            continue;
        };
        let Some(port) = d.body.port_node(uid) else {
            continue;
        };
        let Some((_, name)) = named.iter().find(|(p, _)| *p == port) else {
            continue;
        };
        if let Some(GraphNode::Port { name: slot, .. }) = d.body.get_node_mut(id) {
            *slot = name.clone();
        }
    }
}

/// Lay a definition's interior out the way the root level is laid out.
///
/// `collapse` stacks boundary nodes 70 px apart — closer than a node is
/// tall, so two ports on one side overlap — and parks the output column
/// 600 px out regardless of how wide the body actually is. Diving into
/// a chip is a headline feature of this view; arriving at a pile is the
/// thing that makes the feature look unfinished.
fn lab_tidy_def(
    doc: &mut mara::extras::graph::GraphDoc<GraphNode>,
    def: mara::extras::graph::DefId,
) {
    let Some(d) = doc.def_mut(def) else {
        return;
    };
    let y = LAB_BAND_Y[0];
    let ids: Vec<NodeId> = d.body.node_ids().map(|(id, _)| id).collect();
    let (mut ins, mut outs, mut interior) = (0.0_f32, 0.0_f32, 0.0_f32);
    for id in ids {
        let boundary = d
            .body
            .uid_of(id)
            .and_then(|uid| d.body.port_node(uid))
            .is_some();
        let out = matches!(d.body.get_node(id), Some(GraphNode::Port { out: true, .. }));
        let (col, slot) = match (boundary, out) {
            (true, false) => (LAB_COL[0], &mut ins),
            (true, true) => (LAB_COL[2], &mut outs),
            _ => (LAB_COL[1], &mut interior),
        };
        if let Some(info) = d.body.get_node_info_mut(id) {
            info.pos = mara_graph_pos(col, y + *slot);
        }
        d.body
            .set_size_override(id, Some(mara::ui::vocab::Vec2::new(LAB_NODE_W, 0.0)));
        *slot += 180.0;
    }
}

/// Middle band: a group folded to a pill.
///
/// Its members are skipped by the node loop entirely, so folding a big
/// group costs nothing to draw — and never having been drawn, they have
/// no measured size for the box to fit around. The pair is therefore
/// laid out side by side rather than stacked: the fit falls back to the
/// two positions alone, and two positions in a column would fold to a
/// tall sliver with the title truncated away.
fn lab_band_folded(doc: &mut mara::extras::graph::GraphDoc<GraphNode>) {
    let y = LAB_BAND_Y[1];
    let coarse = lab_node(
        &mut doc.root,
        LAB_COL[0],
        y,
        GraphNode::Wave(WaveShape::Saw),
    );
    let fine = lab_node(
        &mut doc.root,
        LAB_COL[1],
        y,
        GraphNode::Wave(WaveShape::Square),
    );
    doc.root.connect(out_pin(coarse, 0), in_pin(fine, 0));

    let folded =
        doc.root
            .insert_frame("Detune", mara::extras::graph::palette(1), lab_auto_bounds());
    doc.root.set_node_frame(coarse, Some(folded));
    doc.root.set_node_frame(fine, Some(folded));
    if let Some(f) = doc.root.frame_mut(folded) {
        f.collapsed = true;
    }
}

/// Bottom band: a live signal through nested groups.
///
/// The clock and the bias sit outside both boxes so each group holds
/// one kind of node and one header colour — the generators in the outer
/// box, the maths in the inner one. Nesting is then legible as nesting:
/// the two columns are the same shape at the same pitch, and the only
/// thing that differs between them is the depth tint of the box behind.
///
/// The bias exists to leave no input dangling. An unwired input grows
/// an inline editor, which would make one of the two peers inside
/// `Shaping` wider than the other for a reason the design does not
/// intend and the reader cannot decode.
fn lab_band_frames(doc: &mut mara::extras::graph::GraphDoc<GraphNode>) {
    let y = LAB_BAND_Y[2];
    let clock = lab_node(&mut doc.root, LAB_COL[0], y + 90.0, GraphNode::Time);
    let sine = lab_node(
        &mut doc.root,
        LAB_COL[1],
        y,
        GraphNode::Wave(WaveShape::Sine),
    );
    let triangle = lab_node(
        &mut doc.root,
        LAB_COL[1],
        y + 180.0,
        GraphNode::Wave(WaveShape::Triangle),
    );
    let blend = lab_node(
        &mut doc.root,
        LAB_COL[2],
        y,
        GraphNode::ScalarMath(ScalarOp::Mul),
    );
    let lift = lab_node(
        &mut doc.root,
        LAB_COL[2],
        y + 180.0,
        GraphNode::ScalarMath(ScalarOp::Add),
    );
    let bias = lab_node(
        &mut doc.root,
        LAB_COL[0],
        y + 270.0,
        GraphNode::Number(0.25),
    );
    let readout = lab_readout(&mut doc.root, LAB_COL[3], y + 50.0);

    doc.root.connect(out_pin(clock, 0), in_pin(sine, 0));
    doc.root.connect(out_pin(clock, 0), in_pin(triangle, 0));
    doc.root.connect(out_pin(sine, 0), in_pin(blend, 0));
    doc.root.connect(out_pin(triangle, 0), in_pin(blend, 1));
    doc.root.connect(out_pin(blend, 0), in_pin(lift, 0));
    doc.root.connect(out_pin(bias, 0), in_pin(lift, 1));
    doc.root.connect(out_pin(lift, 0), in_pin(readout, 0));

    let outer = doc.root.insert_frame(
        "Oscillator",
        mara::extras::graph::palette(1),
        lab_auto_bounds(),
    );
    let inner = doc.root.insert_frame(
        "Shaping",
        mara::extras::graph::palette(5),
        lab_auto_bounds(),
    );
    doc.root.set_frame_parent(inner, Some(outer));
    doc.root.set_node_frame(sine, Some(outer));
    doc.root.set_node_frame(triangle, Some(outer));
    doc.root.set_node_frame(blend, Some(inner));
    doc.root.set_node_frame(lift, Some(inner));
}

fn mara_graph_pos(x: f32, y: f32) -> mara::ui::vocab::Pos2 {
    mara::ui::vocab::Pos2::new(x, y)
}

fn out_pin(node: NodeId, output: usize) -> OutPinId {
    OutPinId { node, output }
}

fn in_pin(node: NodeId, input: usize) -> InPinId {
    InPinId { node, input }
}

/// Full-screen node graph plus a shelf of instructions.
///
/// The library snapshot is handed to the viewer before the widget
/// borrows the document, because from inside the render the viewer only
/// ever sees ONE level and an instance node's payload knows nothing
/// about the definition it stands for.
fn graph_lab_root_view(
    host: &MaraHostCtx<'_>,
    accent: MaraColor32,
    lab: &mut GraphLabState,
    shelf_state: &mut ShelfState,
    time: f64,
) {
    lab.viewer.time = time;
    lab.viewer.defs = def_looks(&lab.doc);

    let shelves = mara_core::responsive_shelves(graph_lab_shelves(accent, lab));
    let layout = host.layout_shelves(&shelves, shelf_state);

    host.show_root_body(accent, |mui, _screen| {
        let area = mui.available_rect();
        let spec = mara::extras::graph::mara_graph_spec(accent);
        let out = mara::extras::graph::render::show_doc(
            mui,
            area,
            &mut lab.doc,
            &mut lab.viewer,
            &mut lab.nav,
            &spec,
        );

        lab.crumbs = out.breadcrumb.iter().map(|c| c.name.clone()).collect();
        lab.depth = lab.nav.path.depth();
    });

    host.show_shelves(layout, shelves, shelf_state);
}

/// The instruction shelf.
///
/// Built from ordinary pods, because the point of the view is that
/// every graph feature below is reachable from plain consumer code.
fn graph_lab_shelves(accent: MaraColor32, lab: &GraphLabState) -> Vec<ShelfDef<'static>> {
    let here = lab.crumbs.join("  ›  ");
    let level = if lab.depth == 0 {
        "root".to_string()
    } else {
        format!("{} deep — Esc to go up", lab.depth)
    };

    let mut defs: Vec<Pod> = lab
        .doc
        .defs()
        .enumerate()
        .map(|(i, (id, d))| {
            Pod::new(pid(GRAPH_LAB_SHELF, "defs", i))
                .with_separator(SeparatorStyle::None)
                .with_readout(d.name.clone(), format!("×{}", lab.doc.instance_count(id)))
        })
        .collect();
    if defs.is_empty() {
        defs.push(
            Pod::new(pid(GRAPH_LAB_SHELF, "defs", 0))
                .with_separator(SeparatorStyle::None)
                .with_readout("none yet", "made by GraphDoc::collapse"),
        );
    }

    let readout = |slot: usize, k: &str, v: &str| {
        Pod::new(pid(GRAPH_LAB_SHELF, "help", slot))
            .with_separator(SeparatorStyle::None)
            .with_readout(k.to_string(), v.to_string())
    };

    vec![
        ShelfDef::new(GRAPH_LAB_SHELF, ShelfEdge::Left, accent)
            .default_size(320.0)
            .movable()
            .container(ShelfContainer::tabbed(
                cid(GRAPH_LAB_SHELF, "guide"),
                "Graph Lab",
                GRAPH_LAB_ICON_VIEW,
                vec![
                    mara_core::container::Tab::new("lab.where", "Here", GRAPH_LAB_ICON_WHERE).pods(
                        vec![
                            Pod::new(pid(GRAPH_LAB_SHELF, "where", 0))
                                .with_separator(SeparatorStyle::Line)
                                .with_readout("path", here),
                            Pod::new(pid(GRAPH_LAB_SHELF, "where", 1))
                                .with_separator(SeparatorStyle::Line)
                                .with_readout("level", level),
                        ],
                    ),
                    mara_core::container::Tab::new("lab.groups", "Groups", GRAPH_LAB_ICON_GROUPS)
                        .pods(vec![
                            readout(0, "below", "Shaping nests inside Oscillator"),
                            readout(1, "depth", "the inner box tints one step up"),
                            readout(2, "folded", "Detune is a pill — 2 nodes hidden"),
                            readout(3, "move a group", "drag its TITLE BAR"),
                            readout(4, "pan instead", "drag the group BODY"),
                            readout(5, "make a group", "shift-click nodes, then F"),
                            readout(6, "join / leave", "drag a node in or out"),
                            readout(7, "resize", "corners, when auto-fit is off"),
                        ]),
                    mara_core::container::Tab::new(
                        "lab.subgraphs",
                        "Subgraphs",
                        GRAPH_LAB_ICON_SUBGRAPHS,
                    )
                    .pods(vec![
                        readout(10, "above", "Gain stage is placed twice"),
                        readout(11, "sharing", "×N counts placements"),
                        readout(12, "go inside", "double-click a stacked node"),
                        readout(13, "come back", "Esc or Backspace"),
                        readout(14, "careful", "editing one changes all N"),
                        readout(15, "make one", "select nodes, then Ctrl+G"),
                        readout(16, "undo it", "select it, Ctrl+Shift+G"),
                    ]),
                    mara_core::container::Tab::new("lab.defs", "Library", GRAPH_LAB_ICON_LIBRARY)
                        .pods(defs),
                    mara_core::container::Tab::new("lab.legend", "Legend", GRAPH_LAB_ICON_LEGEND)
                        .pods(vec![
                            readout(30, "rose", "values entering the graph"),
                            readout(31, "brown", "wave generators"),
                            readout(32, "blue", "scalar maths"),
                            readout(33, "maroon", "readouts"),
                            readout(34, "a box", "takes the colour of its contents"),
                            readout(35, "a chip", "takes its definition's colour"),
                            readout(36, "one node", "one colour — never two"),
                        ]),
                    mara_core::container::Tab::new(
                        "lab.visuals",
                        "Visuals",
                        GRAPH_LAB_ICON_VISUALS,
                    )
                    .pods(vec![
                        readout(20, "node width", "app-set, so peers match"),
                        readout(21, "readouts", "app-set size, not content size"),
                        readout(22, "shadows", "deepen while dragging"),
                        readout(23, "selection", "replaces the ring, never adds one"),
                        readout(24, "wires", "coloured by the pin they leave"),
                        readout(25, "camera", "double-click empty space to recentre"),
                        readout(26, "zoom", "the editor pane's graph, not this one"),
                    ]),
                ],
            )),
    ]
}

// ─── Canvas root view ──────────────────────────────────────────────

fn canvas_root_view(
    host: &MaraHostCtx<'_>,
    accent: MaraColor32,
    canvas: &mut CanvasViewState,
    shelf_state: &mut ShelfState,
) {
    let shelves = mara_core::responsive_shelves(canvas_shelves(accent));
    let layout = host.layout_shelves(&shelves, shelf_state);
    host.show_root_body(accent, |mui, screen_rect| {
        canvas_root_body(mui, screen_rect, layout.viewport, canvas);
    });
    host.show_shelves(layout, shelves, shelf_state);
}

/// Sealed whiteboard body: backdrop, grid, drag-to-draw strokes,
/// empty-state hint — all through `MaraUi`/`MaraPainter`/vocab types.
fn canvas_root_body(
    mui: &mut mara_core::MaraUi<'_>,
    screen_rect: mara::ui::vocab::Rect,
    canvas_rect: mara::ui::vocab::Rect,
    canvas: &mut CanvasViewState,
) {
    use mara::ui::vocab::{Align2, Color32, Stroke, pos2};

    let accent = mui.accent();
    let (painter, response) = mui.canvas_at(canvas_rect);
    let backdrop = mui.painter();

    // The whiteboard fills the WHOLE window (full-bleed) so it shows
    // through the glass top bar and behind the shelves — drawing still
    // happens only in `canvas_rect` (the open area below the bar, between
    // shelves), but the surface itself is edge-to-edge.
    backdrop.rect_filled(screen_rect, 0, mara_core::style::theme().palette.bg_window);

    let grid = 32.0;
    let grid_col = Color32::from_rgba_unmultiplied(
        mara_core::style::on_panel_dim().r(),
        mara_core::style::on_panel_dim().g(),
        mara_core::style::on_panel_dim().b(),
        34,
    );
    let mut x = screen_rect.left() + grid;
    while x < screen_rect.right() {
        backdrop.line_segment(
            pos2(x, screen_rect.top()),
            pos2(x, screen_rect.bottom()),
            Stroke::new(1.0, grid_col),
        );
        x += grid;
    }
    let mut y = screen_rect.top() + grid;
    while y < screen_rect.bottom() {
        backdrop.line_segment(
            pos2(screen_rect.left(), y),
            pos2(screen_rect.right(), y),
            Stroke::new(1.0, grid_col),
        );
        y += grid;
    }

    if response.drag_started {
        canvas.strokes.push(Vec::new());
    }
    if response.dragged || response.drag_started {
        if let Some(pos) = response
            .interact_pointer
            .filter(|pos| canvas_rect.contains(*pos))
        {
            if let Some(stroke) = canvas.strokes.last_mut() {
                if stroke.last().is_none_or(|last| last.distance(pos) > 1.5) {
                    stroke.push(pos);
                }
            }
        }
    }

    for stroke in &canvas.strokes {
        for points in stroke.windows(2) {
            painter.line_segment(points[0], points[1], Stroke::new(3.0, accent));
        }
    }

    if canvas.strokes.is_empty() {
        painter.text(
            canvas_rect.center(),
            Align2::CENTER_CENTER,
            "Canvas root view\ndrag to draw",
            24.0,
            mara_core::style::on_panel_dim(),
        );
    }
}

fn canvas_shelves(accent: MaraColor32) -> Vec<ShelfDef<'static>> {
    vec![
        ShelfDef::new(CANVAS_SHELF_LEFT, ShelfEdge::Left, accent)
            .default_size(300.0)
            .movable()
            .container(ShelfContainer::tabbed(
                cid(CANVAS_SHELF_LEFT, "tools"),
                "Canvas Tools",
                "draw-shape",
                vec![
                    mara_core::container::Tab::new("paint.brush", "Brush", "paint-brush").pods(
                        vec![
                            Pod::new(pid(CANVAS_SHELF_LEFT, "brush", 0))
                                .with_separator(SeparatorStyle::Line)
                                .with_slider("size", 3.0, 1.0..=24.0, 1, " px", accent),
                            Pod::new(pid(CANVAS_SHELF_LEFT, "brush", 1))
                                .with_separator(SeparatorStyle::Line)
                                .with_slider("opacity", 1.0, 0.05..=1.0, 2, "", accent),
                            Pod::new(pid(CANVAS_SHELF_LEFT, "brush", 2))
                                .with_separator(SeparatorStyle::None)
                                .with_button("Clear strokes", accent),
                        ],
                    ),
                    mara_core::container::Tab::new("paint.layers", "Layers", "square-multiple")
                        .pods(vec![
                            Pod::new(pid(CANVAS_SHELF_LEFT, "layers", 0))
                                .with_separator(SeparatorStyle::Line)
                                .with_select_list(
                                    vec![
                                        "Sketch layer".to_owned(),
                                        "Ink layer".to_owned(),
                                        "Notes layer".to_owned(),
                                    ],
                                    None::<Vec<String>>,
                                    accent,
                                ),
                            Pod::new(pid(CANVAS_SHELF_LEFT, "layers", 1))
                                .with_separator(SeparatorStyle::None)
                                .with_toggle_initial("show grid", accent, true),
                        ]),
                    mara_core::container::Tab::new("paint.assets", "Assets", "image").pods(vec![
                        Pod::new(pid(CANVAS_SHELF_LEFT, "assets", 0))
                            .with_separator(SeparatorStyle::Line)
                            .with_search("search images…", accent),
                        Pod::new(pid(CANVAS_SHELF_LEFT, "assets", 1))
                            .with_separator(SeparatorStyle::None)
                            .with_button("Import image", accent),
                    ]),
                ],
            ))
            .container(ShelfContainer::tabbed(
                cid(CANVAS_SHELF_LEFT, "document"),
                "Document",
                "document",
                vec![
                    mara_core::container::Tab::new("paint.info", "Info", "info").pods(vec![
                        Pod::new(pid(CANVAS_SHELF_LEFT, "info", 0))
                            .with_separator(SeparatorStyle::Line)
                            .with_readout("view", "Canvas"),
                        Pod::new(pid(CANVAS_SHELF_LEFT, "info", 1))
                            .with_separator(SeparatorStyle::Line)
                            .with_readout("shelf", "Left dock"),
                        Pod::new(pid(CANVAS_SHELF_LEFT, "info", 2))
                            .with_separator(SeparatorStyle::None)
                            .with_readout("content", "multiple tabbed containers"),
                    ]),
                    mara_core::container::Tab::new("paint.export", "Export", "save").pods(vec![
                        Pod::new(pid(CANVAS_SHELF_LEFT, "export", 0))
                            .with_separator(SeparatorStyle::Line)
                            .with_dropdown(
                                vec!["PNG".to_owned(), "SVG".to_owned(), "Mara Scene".to_owned()],
                                0,
                                accent,
                            ),
                        Pod::new(pid(CANVAS_SHELF_LEFT, "export", 1))
                            .with_separator(SeparatorStyle::None)
                            .with_button("Export canvas", accent),
                    ]),
                ],
            ))
            .container(ShelfContainer::tabbed(
                cid(CANVAS_SHELF_LEFT, "history"),
                "History",
                "history",
                vec![
                    mara_core::container::Tab::new("paint.undo", "Undo", "arrow-undo").pods(vec![
                        Pod::new(pid(CANVAS_SHELF_LEFT, "undo", 0))
                            .with_separator(SeparatorStyle::Line)
                            .with_readout("last action", "Brush stroke"),
                        Pod::new(pid(CANVAS_SHELF_LEFT, "undo", 1))
                            .with_separator(SeparatorStyle::None)
                            .with_button("Revert action", accent),
                    ]),
                    mara_core::container::Tab::new("paint.timeline", "Timeline", "clock").pods(
                        vec![
                            Pod::new(pid(CANVAS_SHELF_LEFT, "timeline", 0))
                                .with_separator(SeparatorStyle::Line)
                                .with_slider("scrub", 0.0, 0.0..=100.0, 0, " %", accent),
                        ],
                    ),
                ],
            ))
            .container(ShelfContainer::tabbed(
                cid(CANVAS_SHELF_LEFT, "properties"),
                "Properties",
                "settings",
                vec![
                    mara_core::container::Tab::new("paint.stroke", "Stroke", "pen").pods(vec![
                        Pod::new(pid(CANVAS_SHELF_LEFT, "stroke", 0))
                            .with_separator(SeparatorStyle::Line)
                            .with_dropdown(
                                vec![
                                    "Round".to_owned(),
                                    "Square".to_owned(),
                                    "Calligraphy".to_owned(),
                                ],
                                0,
                                accent,
                            ),
                        Pod::new(pid(CANVAS_SHELF_LEFT, "stroke", 1))
                            .with_separator(SeparatorStyle::None)
                            .with_toggle_initial("pressure", accent, true),
                    ]),
                ],
            )),
    ]
}

// ─── Coreviz-style context panes (UI only) ─────────────────────────

fn coreviz_zones_pane(body: &mut PaneBody) {
    let accent = body.accent();
    body.add_normal(
        cid(PANE_COREVIZ_ZONES, "summary"),
        "Hierarchy",
        "shape-union",
        vec![
            Pod::new(pid(PANE_COREVIZ_ZONES, "summary", 0))
                .with_separator(SeparatorStyle::Line)
                .with_readout("root", "Root Zone"),
            Pod::new(pid(PANE_COREVIZ_ZONES, "summary", 1))
                .with_separator(SeparatorStyle::Line)
                .with_card_action_button(
                    "shape-union",
                    "Root Zone",
                    "root · 4 pts",
                    "add",
                    "Add child zone",
                    false,
                    accent,
                ),
            Pod::new(pid(PANE_COREVIZ_ZONES, "tree", 0))
                .with_separator(SeparatorStyle::None)
                .with_tree(7, move |tree| {
                    let armed_key = MaraId::new(("coreviz_demo", "armed_zone"));
                    let mut armed = tree.persisted_string(armed_key).filter(|s| !s.is_empty());
                    let armed_is = |armed: &Option<String>, v: &str| armed.as_deref() == Some(v);
                    let root = tree.action_row(
                        "root",
                        0,
                        None,
                        Some("map"),
                        "Root Zone",
                        "root · 4 pts",
                        armed_is(&armed, "root"),
                        "add",
                        Some("Add child zone"),
                        armed_is(&armed, "root"),
                        accent,
                    );
                    if root.action.clicked {
                        armed = Some("root".to_string());
                    }
                    let floor = tree.action_row_guided(
                        "floor",
                        1,
                        None,
                        Some("shape-union"),
                        "Floor 1",
                        "zone · 7 pts",
                        armed_is(&armed, "floor"),
                        "add",
                        Some("Add child zone"),
                        armed_is(&armed, "floor"),
                        &TreeBranchGuide::tee([]),
                        accent,
                    );
                    if floor.action.clicked {
                        armed = Some("floor".to_string());
                    }
                    let dock = tree.action_row_guided(
                        "dock",
                        2,
                        None,
                        Some("location"),
                        "Dock A",
                        "zone · 5 pts",
                        armed_is(&armed, "dock"),
                        "add",
                        Some("Add child zone"),
                        armed_is(&armed, "dock"),
                        &TreeBranchGuide::last([true]),
                        accent,
                    );
                    if dock.action.clicked {
                        armed = Some("dock".to_string());
                    }
                    let yard = tree.action_row_guided(
                        "yard",
                        1,
                        None,
                        Some("shape-union"),
                        "Yard",
                        "zone · 6 pts",
                        armed_is(&armed, "yard"),
                        "add",
                        Some("Add child zone"),
                        armed_is(&armed, "yard"),
                        &TreeBranchGuide::last([]),
                        accent,
                    );
                    if yard.action.clicked {
                        armed = Some("yard".to_string());
                    }
                    tree.set_persisted_string(armed_key, armed.unwrap_or_default());
                }),
        ],
    );
}

fn coreviz_reference_pane(body: &mut PaneBody) {
    body.add_normal(
        cid(PANE_COREVIZ_REFERENCE, "datum"),
        "Datum",
        "map",
        vec![
            Pod::new(pid(PANE_COREVIZ_REFERENCE, "datum", 0))
                .with_separator(SeparatorStyle::Line)
                .with_readout("origin", "Amsterdam"),
            Pod::new(pid(PANE_COREVIZ_REFERENCE, "datum", 1))
                .with_separator(SeparatorStyle::Line)
                .with_readout("lat/lon", "52.3675734 / 4.9041389"),
            Pod::new(pid(PANE_COREVIZ_REFERENCE, "datum", 2))
                .with_separator(SeparatorStyle::None)
                .with_button("Place datum marker", body.accent()),
        ],
    );
}

fn coreviz_nodes_pane(body: &mut PaneBody) {
    let accent = body.accent();
    body.add_normal(
        cid(PANE_COREVIZ_NODES, "nodes"),
        "Nodes",
        "pin",
        vec![
            Pod::new(pid(PANE_COREVIZ_NODES, "nodes", 0))
                .with_separator(SeparatorStyle::Line)
                .with_search("filter nodes…", accent),
            Pod::new(pid(PANE_COREVIZ_NODES, "nodes", 1))
                .with_separator(SeparatorStyle::None)
                .with_select_list(
                    ["Node A", "Dock B", "Charging C"],
                    Some(vec!["2 zones".into(), "1 zone".into(), "3 zones".into()]),
                    accent,
                ),
        ],
    );
}

fn coreviz_edges_pane(body: &mut PaneBody) {
    let accent = body.accent();
    body.add_normal(
        cid(PANE_COREVIZ_EDGES, "edges"),
        "Edges",
        "line",
        vec![
            Pod::new(pid(PANE_COREVIZ_EDGES, "edges", 0))
                .with_separator(SeparatorStyle::Line)
                .with_toggle_initial("directed", accent, true),
            Pod::new(pid(PANE_COREVIZ_EDGES, "edges", 1))
                .with_separator(SeparatorStyle::Line)
                .with_drag_value("weight", 1.0, 0.1, 0.0..=100.0, 2, ""),
            Pod::new(pid(PANE_COREVIZ_EDGES, "edges", 2))
                .with_separator(SeparatorStyle::None)
                .with_button("Connect selected nodes", accent),
        ],
    );
}

fn coreviz_zenoh_pane(body: &mut PaneBody) {
    let accent = body.accent();
    body.add_normal(
        cid(PANE_COREVIZ_ZENOH, "connection"),
        "Zenoh",
        "box",
        vec![
            Pod::new(pid(PANE_COREVIZ_ZENOH, "connection", 0))
                .with_separator(SeparatorStyle::Line)
                .with_readout("status", "disconnected"),
            Pod::new(pid(PANE_COREVIZ_ZENOH, "connection", 1))
                .with_separator(SeparatorStyle::Line)
                .with_dropdown(["peer", "client"], 0, accent),
            Pod::new(pid(PANE_COREVIZ_ZENOH, "connection", 2))
                .with_separator(SeparatorStyle::None)
                .with_button("Connect", accent),
        ],
    );
}

fn coreviz_robots_pane(body: &mut PaneBody) {
    let accent = body.accent();
    body.add_normal(
        cid(PANE_COREVIZ_ROBOTS, "fleet"),
        "Fleet",
        "location",
        vec![
            Pod::new(pid(PANE_COREVIZ_ROBOTS, "fleet", 0))
                .with_separator(SeparatorStyle::Line)
                .with_search("filter robots…", accent),
            Pod::new(pid(PANE_COREVIZ_ROBOTS, "fleet", 1))
                .with_separator(SeparatorStyle::None)
                .with_select_list(
                    ["amr-01", "forklift-02", "cart-03"],
                    Some(vec!["idle".into(), "task".into(), "charging".into()]),
                    accent,
                ),
        ],
    );
}

fn coreviz_details_pane(body: &mut PaneBody) {
    let accent = body.accent();
    body.add_normal(
        cid(PANE_COREVIZ_DETAILS, "selection"),
        "Selection",
        "options",
        vec![
            Pod::new(pid(PANE_COREVIZ_DETAILS, "selection", 0))
                .with_separator(SeparatorStyle::Line)
                .with_readout("selected", "mock item"),
            Pod::new(pid(PANE_COREVIZ_DETAILS, "selection", 1))
                .with_separator(SeparatorStyle::Line)
                .with_button_subtitle("Edit properties", "UI placeholder only", accent),
            Pod::new(pid(PANE_COREVIZ_DETAILS, "selection", 2))
                .with_separator(SeparatorStyle::None)
                .with_tags(["zone", "node", "robot"], accent),
        ],
    );
}

fn coreviz_json_pane(body: &mut PaneBody) {
    let accent = body.accent();
    body.add_normal(
        cid(PANE_COREVIZ_JSON, "json"),
        "Workspace JSON",
        "document",
        vec![
            Pod::new(pid(PANE_COREVIZ_JSON, "json", 0))
                .with_separator(SeparatorStyle::Line)
                .with_readout("format", "Coreviz workspace"),
            Pod::new(pid(PANE_COREVIZ_JSON, "json", 1))
                .with_separator(SeparatorStyle::Line)
                .with_button("Format JSON", accent),
            Pod::new(pid(PANE_COREVIZ_JSON, "json", 2))
                .with_separator(SeparatorStyle::None)
                .with_button("Apply JSON", accent),
        ],
    );
}

fn coreviz_scheduler_pane(body: &mut PaneBody) {
    let accent = body.accent();
    body.add_normal(
        cid(PANE_COREVIZ_SCHEDULER, "order"),
        "Order",
        "clock",
        vec![
            Pod::new(pid(PANE_COREVIZ_SCHEDULER, "order", 0))
                .with_separator(SeparatorStyle::Line)
                .with_dropdown(["Move", "Inspect", "Charge"], 0, accent),
            Pod::new(pid(PANE_COREVIZ_SCHEDULER, "order", 1))
                .with_separator(SeparatorStyle::Line)
                .with_select_list(["Target A", "Target B", "Target C"], None, accent),
            Pod::new(pid(PANE_COREVIZ_SCHEDULER, "order", 2))
                .with_separator(SeparatorStyle::None)
                .with_button("Dispatch order", accent),
        ],
    );
}

fn coreviz_tasks_pane(body: &mut PaneBody) {
    body.add_normal(
        cid(PANE_COREVIZ_TASKS, "tasks"),
        "Tasks",
        "list",
        vec![
            Pod::new(pid(PANE_COREVIZ_TASKS, "tasks", 0))
                .with_separator(SeparatorStyle::Line)
                .with_readout("active", "Pick/Drop"),
            Pod::new(pid(PANE_COREVIZ_TASKS, "tasks", 1))
                .with_separator(SeparatorStyle::None)
                .with_badge_row("queue", ["2 pending", "1 blocked"], body.accent()),
        ],
    );
}

// ─── Per-pane content ──────────────────────────────────────────────

fn cid(pane: &str, suffix: &str) -> MaraId {
    MaraId::new((pane, suffix))
}
fn pid(pane: &str, container: &str, idx: usize) -> MaraId {
    MaraId::new((pane, container, "pod", idx))
}

/// **Widgets pane** — one container per widget category.
fn widgets_pane(body: &mut PaneBody) {
    let accent = body.accent();
    let anim = |name: &str, style: FillStyle, sep: SeparatorStyle, idx: usize| -> Pod {
        Pod::new(pid(PANE_WIDGETS, "anim", idx))
            .with_separator(sep)
            .with_button_animated(name, accent, style)
    };
    body.add_normal(
        cid(PANE_WIDGETS, "flags"),
        "Flags",
        "flag",
        vec![
            Pod::new(pid(PANE_WIDGETS, "flags", 0))
                .with_separator(SeparatorStyle::Line)
                .with_toggle_initial("power", accent, true),
            Pod::new(pid(PANE_WIDGETS, "flags", 1))
                .with_separator(SeparatorStyle::None)
                .with_toggle_initial("headlights", accent, false),
        ],
    );
    body.add_normal(
        cid(PANE_WIDGETS, "numbers"),
        "Numbers",
        "calculator",
        vec![
            Pod::new(pid(PANE_WIDGETS, "numbers", 0))
                .with_separator(SeparatorStyle::Line)
                .with_drag_value("gravity", 9.81, 0.05, 0.0..=30.0, 2, " m/s²"),
            Pod::new(pid(PANE_WIDGETS, "numbers", 1))
                .with_separator(SeparatorStyle::Line)
                .with_drag_value("speed limit", 60.0, 0.1, 0.0..=200.0, 1, " m/s"),
            Pod::new(pid(PANE_WIDGETS, "numbers", 2))
                .with_separator(SeparatorStyle::None)
                .with_drag_value("engine power", 750.0, 1.0, 0.0..=2000.0, 0, " kW"),
        ],
    );
    body.add_normal(
        cid(PANE_WIDGETS, "bars"),
        "Bars",
        "gauge",
        vec![
            Pod::new(pid(PANE_WIDGETS, "bars", 0))
                .with_separator(SeparatorStyle::Line)
                .with_slider("throttle", 0.4, 0.0..=1.0, 2, "", accent),
            Pod::new(pid(PANE_WIDGETS, "bars", 1))
                .with_separator(SeparatorStyle::Line)
                .with_slider("brake", 0.0, 0.0..=1.0, 2, "", accent),
            Pod::new(pid(PANE_WIDGETS, "bars", 2))
                .with_separator(SeparatorStyle::None)
                .with_progress("fuel", 0.62, "62%", accent),
        ],
    );
    body.add_normal(
        cid(PANE_WIDGETS, "buttons"),
        "Buttons",
        "button",
        vec![
            Pod::new(pid(PANE_WIDGETS, "buttons", 0))
                .with_separator(SeparatorStyle::Line)
                .with_button("Refuel", accent),
            Pod::new(pid(PANE_WIDGETS, "buttons", 1))
                .with_separator(SeparatorStyle::None)
                .with_card_button(
                    "star",
                    "Primary action",
                    "Two-line card button with glyph + subtitle",
                    accent,
                ),
        ],
    );
    body.add_normal(
        cid(PANE_WIDGETS, "hierarchy"),
        "Hierarchy",
        "branch",
        vec![
            Pod::new(pid(PANE_WIDGETS, "hierarchy", 0))
                .with_separator(SeparatorStyle::Line)
                .with_card_action_button(
                    "shape-union",
                    "Zone row",
                    "Body click + embedded add action",
                    "add",
                    "Add child zone",
                    false,
                    accent,
                ),
            Pod::new(pid(PANE_WIDGETS, "hierarchy", 1))
                .with_separator(SeparatorStyle::None)
                .with_tree(6, move |tree| {
                    let root_key = MaraId::new(("demo_hierarchy", "root_open"));
                    let floor_key = MaraId::new(("demo_hierarchy", "floor_open"));
                    let armed_key = MaraId::new(("demo_hierarchy", "armed_child"));
                    let mut root_open = tree.persisted_bool(root_key).unwrap_or(true);
                    let mut floor_open = tree.persisted_bool(floor_key).unwrap_or(true);
                    let mut armed = tree.persisted_string(armed_key).filter(|s| !s.is_empty());
                    let armed_is = |armed: &Option<String>, v: &str| armed.as_deref() == Some(v);

                    let root = tree.action_row(
                        "root-zone",
                        0,
                        Some(&mut root_open),
                        Some("map"),
                        "Root Zone",
                        "root · 4 pts",
                        armed_is(&armed, "root-zone"),
                        "add",
                        Some("Add child zone"),
                        armed_is(&armed, "root-zone"),
                        accent,
                    );
                    if root.action.clicked {
                        armed = Some("root-zone".to_string());
                    }
                    if root_open {
                        let floor = tree.action_row_guided(
                            "floor-zone",
                            1,
                            Some(&mut floor_open),
                            Some("shape-union"),
                            "Floor 1",
                            "zone · 7 pts",
                            armed_is(&armed, "floor-zone"),
                            "add",
                            Some("Add child zone"),
                            armed_is(&armed, "floor-zone"),
                            &TreeBranchGuide::tee([]),
                            accent,
                        );
                        if floor.action.clicked {
                            armed = Some("floor-zone".to_string());
                        }
                        if floor_open {
                            let dock = tree.action_row_guided(
                                "dock-zone",
                                2,
                                None,
                                Some("location"),
                                "Dock A",
                                "zone · 5 pts",
                                armed_is(&armed, "dock-zone"),
                                "add",
                                Some("Add child zone"),
                                armed_is(&armed, "dock-zone"),
                                &TreeBranchGuide::tee([true]),
                                accent,
                            );
                            if dock.action.clicked {
                                armed = Some("dock-zone".to_string());
                            }
                            let storage = tree.action_row_guided(
                                "storage-zone",
                                2,
                                None,
                                Some("shape-union"),
                                "Storage",
                                "zone · 6 pts",
                                armed_is(&armed, "storage-zone"),
                                "add",
                                Some("Add child zone"),
                                armed_is(&armed, "storage-zone"),
                                &TreeBranchGuide::last([true]),
                                accent,
                            );
                            if storage.action.clicked {
                                armed = Some("storage-zone".to_string());
                            }
                        }
                        let yard = tree.action_row_guided(
                            "yard-zone",
                            1,
                            None,
                            Some("shape-union"),
                            "Yard",
                            "zone · 6 pts",
                            armed_is(&armed, "yard-zone"),
                            "add",
                            Some("Add child zone"),
                            armed_is(&armed, "yard-zone"),
                            &TreeBranchGuide::last([]),
                            accent,
                        );
                        if yard.action.clicked {
                            armed = Some("yard-zone".to_string());
                        }
                    }

                    tree.set_persisted_bool(root_key, root_open);
                    tree.set_persisted_bool(floor_key, floor_open);
                    tree.set_persisted_string(armed_key, armed.unwrap_or_default());
                }),
        ],
    );
    body.add_normal(
        cid(PANE_WIDGETS, "anim"),
        "Animated",
        "animation",
        vec![
            anim("Slide left", FillStyle::SlideLeft, SeparatorStyle::Line, 0),
            anim(
                "Parallelogram",
                FillStyle::Parallelogram,
                SeparatorStyle::Line,
                1,
            ),
            anim(
                "Parallelogram meet",
                FillStyle::ParallelogramMeet,
                SeparatorStyle::Line,
                2,
            ),
            anim("Bowtie", FillStyle::Bowtie, SeparatorStyle::Line, 3),
            anim("Bands meet", FillStyle::BandsMeet, SeparatorStyle::Line, 4),
            anim(
                "Corner squares",
                FillStyle::CornerSquares,
                SeparatorStyle::Line,
                5,
            ),
            anim(
                "Diagonal triangles",
                FillStyle::DiagonalTriangles,
                SeparatorStyle::Line,
                6,
            ),
            anim(
                "Circle grow",
                FillStyle::CircleGrow,
                SeparatorStyle::Line,
                7,
            ),
            anim("Equalizer", FillStyle::Equalizer, SeparatorStyle::Line, 8),
            anim(
                "Horizontal slide",
                FillStyle::HorizontalSlide,
                SeparatorStyle::Line,
                9,
            ),
            anim(
                "Horizontal delayed",
                FillStyle::HorizontalSlideDelayed,
                SeparatorStyle::Line,
                10,
            ),
            anim(
                "Vertical delayed",
                FillStyle::VerticalSlideDelayed,
                SeparatorStyle::Line,
                11,
            ),
            anim(
                "Criss cross",
                FillStyle::CrissCross,
                SeparatorStyle::None,
                12,
            ),
        ],
    );
}

/// **Containers pane** — two tabbed containers stacked: `Transform`
/// (Position / Rotation / Scale) and `Velocity` (Linear / Angular).
/// Both go through `render_containers` so they share the three-dot
/// drag handle, drag-reorder, and persisted-flow plumbing every
/// other container gets.
fn containers_pane(body: &mut PaneBody) {
    body.add_tabbed(
        cid(PANE_CONTAINERS, "xform"),
        "Transform",
        "cube",
        vec![
            mara_core::container::Tab::new("xform.position", "Position", "arrow-move").pods(vec![
                Pod::new(pid(PANE_CONTAINERS, "pos", 0))
                    .with_separator(SeparatorStyle::Line)
                    .with_drag_value("X", 0.0, 0.05, -1000.0..=1000.0, 3, " m"),
                Pod::new(pid(PANE_CONTAINERS, "pos", 1))
                    .with_separator(SeparatorStyle::Line)
                    .with_drag_value("Y", 0.0, 0.05, -1000.0..=1000.0, 3, " m"),
                Pod::new(pid(PANE_CONTAINERS, "pos", 2))
                    .with_separator(SeparatorStyle::None)
                    .with_drag_value("Z", 0.0, 0.05, -1000.0..=1000.0, 3, " m"),
            ]),
            mara_core::container::Tab::new("xform.rotation", "Rotation", "arrow-rotate-clockwise")
                .pods(vec![
                    Pod::new(pid(PANE_CONTAINERS, "rot", 0))
                        .with_separator(SeparatorStyle::Line)
                        .with_drag_value("X", 0.0, 1.0, -360.0..=360.0, 2, "°"),
                    Pod::new(pid(PANE_CONTAINERS, "rot", 1))
                        .with_separator(SeparatorStyle::Line)
                        .with_drag_value("Y", 0.0, 1.0, -360.0..=360.0, 2, "°"),
                    Pod::new(pid(PANE_CONTAINERS, "rot", 2))
                        .with_separator(SeparatorStyle::None)
                        .with_drag_value("Z", 0.0, 1.0, -360.0..=360.0, 2, "°"),
                ]),
            mara_core::container::Tab::new("xform.scale", "Scale", "maximize").pods(vec![
                Pod::new(pid(PANE_CONTAINERS, "scl", 0))
                    .with_separator(SeparatorStyle::Line)
                    .with_drag_value("X", 1.0, 0.01, 0.01..=100.0, 3, "×"),
                Pod::new(pid(PANE_CONTAINERS, "scl", 1))
                    .with_separator(SeparatorStyle::Line)
                    .with_drag_value("Y", 1.0, 0.01, 0.01..=100.0, 3, "×"),
                Pod::new(pid(PANE_CONTAINERS, "scl", 2))
                    .with_separator(SeparatorStyle::None)
                    .with_drag_value("Z", 1.0, 0.01, 0.01..=100.0, 3, "×"),
            ]),
        ],
    );
    body.add_tabbed(
        cid(PANE_CONTAINERS, "vel"),
        "Velocity",
        "flash",
        vec![
            mara_core::container::Tab::new("vel.linear", "Linear", "arrow-trending").pods(vec![
                Pod::new(pid(PANE_CONTAINERS, "vlin", 0))
                    .with_separator(SeparatorStyle::Line)
                    .with_drag_value("X", 0.0, 0.05, -100.0..=100.0, 2, " m/s"),
                Pod::new(pid(PANE_CONTAINERS, "vlin", 1))
                    .with_separator(SeparatorStyle::Line)
                    .with_drag_value("Y", 0.0, 0.05, -100.0..=100.0, 2, " m/s"),
                Pod::new(pid(PANE_CONTAINERS, "vlin", 2))
                    .with_separator(SeparatorStyle::None)
                    .with_drag_value("Z", 0.0, 0.05, -100.0..=100.0, 2, " m/s"),
            ]),
            mara_core::container::Tab::new(
                "vel.angular",
                "Angular",
                "arrow-rotate-counterclockwise",
            )
            .pods(vec![
                Pod::new(pid(PANE_CONTAINERS, "vang", 0))
                    .with_separator(SeparatorStyle::Line)
                    .with_drag_value("X", 0.0, 0.1, -720.0..=720.0, 2, " °/s"),
                Pod::new(pid(PANE_CONTAINERS, "vang", 1))
                    .with_separator(SeparatorStyle::Line)
                    .with_drag_value("Y", 0.0, 0.1, -720.0..=720.0, 2, " °/s"),
                Pod::new(pid(PANE_CONTAINERS, "vang", 2))
                    .with_separator(SeparatorStyle::None)
                    .with_drag_value("Z", 0.0, 0.1, -720.0..=720.0, 2, " °/s"),
            ]),
        ],
    );
}

/// **Scene pane** — outliner tree + flat hybrid_select roster.
fn scene_pane(body: &mut PaneBody) {
    let accent = body.accent();
    let tree_root = cid(PANE_SCENE, "tree_root");
    let search_pod_id = pid(PANE_SCENE, "scene", 0);
    let tree_filter = body.search_query(search_pod_id, 0).to_lowercase();
    let selected_path: String = body
        .temp_string(tree_root.with("mara_demo_tree_selected"))
        .unwrap_or_default();
    let selected_display = if selected_path.is_empty() {
        "—".to_string()
    } else {
        selected_path
    };

    let entities: Vec<String> = [
        "Planet",
        "Robot",
        "Sun",
        "Cloud Shell",
        "Camera",
        "Swatch[0]",
        "Swatch[1]",
        "Swatch[2]",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let trailing: Vec<String> = (0..entities.len()).map(|i| format!("#{i}")).collect();

    body.add_normal(
        cid(PANE_SCENE, "scene"),
        "Scene",
        "folder",
        vec![
            Pod::new(pid(PANE_SCENE, "scene", 0))
                .with_separator(SeparatorStyle::Line)
                .with_search("filter by name / path…", accent),
            Pod::new(pid(PANE_SCENE, "scene", 1))
                .with_separator(SeparatorStyle::Line)
                .with_dropdown(["all", "transforms", "lights", "meshes"], 0, accent),
            Pod::new(pid(PANE_SCENE, "scene", 2))
                .with_separator(SeparatorStyle::Line)
                .fill()
                .with_tree(7, move |tree| {
                    demo_tree(tree, tree_root, accent, &tree_filter)
                }),
            Pod::new(pid(PANE_SCENE, "scene", 3))
                .with_separator(SeparatorStyle::None)
                .with_readout("selected", selected_display),
        ],
    );
    body.add_normal(
        cid(PANE_SCENE, "flat"),
        "Flat list",
        "list",
        vec![
            Pod::new(pid(PANE_SCENE, "flat", 0))
                .with_separator(SeparatorStyle::LineDots)
                .resizable()
                .with_hybrid_select_list(entities, Some(trailing), accent),
        ],
    );
}

/// **Theme pane** — Profile / Accent / Glass.
#[allow(clippy::too_many_arguments)]
fn theme_pane(
    body: &mut PaneBody,
    accent_res: &mut AccentColor,
    glass: &mut GlassOpacity,
    family: &mut ThemeFamily,
    mode: &mut ThemeModeRes,
    pastel: &mut PastelToggle,
    tint: &mut TintRgba,
) {
    let accent = body.accent();
    let profile_id = cid(PANE_THEME, "profile");
    let accent_id = cid(PANE_THEME, "accent");
    let glass_id = cid(PANE_THEME, "glass");
    body.add_normal(
        profile_id,
        "Profile",
        "person",
        vec![
            Pod::new(pid(PANE_THEME, "profile", 0))
                .with_separator(SeparatorStyle::Line)
                .with_dropdown(["PRO", "GAME", "FLAT"], family.0 as usize, accent),
            Pod::new(pid(PANE_THEME, "profile", 1))
                .with_separator(SeparatorStyle::Line)
                .with_dropdown(["Dark", "Light"], mode.0 as usize, accent),
            Pod::new(pid(PANE_THEME, "profile", 2))
                .with_separator(SeparatorStyle::None)
                .with_toggle_initial("pastel accent", accent, pastel.0),
        ],
    );
    body.add_normal(
        accent_id,
        "Accent",
        "color",
        vec![
            Pod::new(pid(PANE_THEME, "accent", 0))
                .with_separator(SeparatorStyle::Line)
                .with_color_rgb(
                    "accent",
                    [
                        accent_res.0.r() as f32 / 255.0,
                        accent_res.0.g() as f32 / 255.0,
                        accent_res.0.b() as f32 / 255.0,
                    ],
                    accent,
                ),
            Pod::new(pid(PANE_THEME, "accent", 1))
                .with_separator(SeparatorStyle::None)
                .with_color_rgba("tint", tint.0, accent),
        ],
    );
    body.add_normal(
        glass_id,
        "Glass",
        "glasses",
        vec![
            Pod::new(pid(PANE_THEME, "glass", 0))
                .with_separator(SeparatorStyle::None)
                .with_slider("opacity", glass.0 as f64, 1.0..=100.0, 0, "%", accent),
        ],
    );
    // Paint now so we can read pod responses and wire them back to
    // the mutable state below in the same closure.
    let responses = body.render();
    // Wire response → mutable state.
    if let Some(pr) = responses.get(&profile_id) {
        if let Some(p0) = pr.first() {
            if let Some(d) = p0.dropdowns.first() {
                if d.changed {
                    family.0 = d.selected as u8;
                }
            }
        }
        if let Some(p1) = pr.get(1) {
            if let Some(d) = p1.dropdowns.first() {
                if d.changed {
                    mode.0 = d.selected as u8;
                }
            }
        }
        if let Some(p2) = pr.get(2) {
            if let Some(t) = p2.toggles.first() {
                if t.changed {
                    pastel.0 = t.on;
                }
            }
        }
    }
    if let Some(pr) = responses.get(&accent_id) {
        if let Some(p0) = pr.first() {
            if let Some(c) = p0.colors.first() {
                if c.changed {
                    accent_res.0 = srgb_to_color([c.rgba[0], c.rgba[1], c.rgba[2]]);
                }
            }
        }
        if let Some(p1) = pr.get(1) {
            if let Some(c) = p1.colors.first() {
                if c.changed {
                    tint.0 = c.rgba;
                }
            }
        }
    }
    if let Some(pr) = responses.get(&glass_id) {
        if let Some(p0) = pr.first() {
            if let Some(s) = p0.sliders.first() {
                if s.changed {
                    glass.0 = s.value.round().clamp(1.0, 100.0) as u8;
                }
            }
        }
    }
}

/// **Keys pane** — keybinding readouts.
fn keys_pane(body: &mut PaneBody) {
    body.add_normal(
        cid(PANE_KEYS, "mouse"),
        "Mouse",
        "cursor",
        vec![
            Pod::new(pid(PANE_KEYS, "mouse", 0))
                .with_separator(SeparatorStyle::None)
                .with_keybindings(vec![
                    ("MMB drag", "pan camera focus"),
                    ("LMB+RMB", "orbit camera"),
                    ("Scroll", "log-smooth zoom"),
                    ("LMB cube", "re-tint UI accent"),
                ]),
        ],
    );
    body.add_normal(
        cid(PANE_KEYS, "layout"),
        "Layout",
        "grid",
        vec![
            Pod::new(pid(PANE_KEYS, "layout", 0))
                .with_separator(SeparatorStyle::None)
                .with_keybindings(vec![
                    ("Drag edge", "resize the pane"),
                    ("Click btn", "open / close pane"),
                    ("Drag btn", "reorder ribbon"),
                    ("F12", "egui debug overlay"),
                ]),
        ],
    );
    body.add_normal(
        cid(PANE_KEYS, "global"),
        "Global",
        "keyboard",
        vec![
            Pod::new(pid(PANE_KEYS, "global", 0))
                .with_separator(SeparatorStyle::None)
                .with_keybindings(vec![
                    ("Ctrl+K", "command palette"),
                    ("Ctrl+P", "command palette"),
                    ("Esc", "close palette"),
                ]),
        ],
    );
}

/// **About pane** — version + dependency readouts plus a feature
/// chip cluster that demonstrates the auto-growing tags pod.
fn about_pane(body: &mut PaneBody) {
    let accent = body.accent();
    body.add_normal(
        cid(PANE_ABOUT, "info"),
        "mara",
        "info",
        vec![
            Pod::new(pid(PANE_ABOUT, "info", 0))
                .with_separator(SeparatorStyle::Line)
                .with_readout("version", env!("CARGO_PKG_VERSION")),
            Pod::new(pid(PANE_ABOUT, "info", 1))
                .with_separator(SeparatorStyle::Line)
                .with_readout("bevy", "0.19"),
            Pod::new(pid(PANE_ABOUT, "info", 2))
                .with_separator(SeparatorStyle::None)
                .with_readout("egui", "0.34"),
        ],
    );
    body.add_normal(
        cid(PANE_ABOUT, "features"),
        "Features",
        "tag",
        vec![
            Pod::new(pid(PANE_ABOUT, "features", 0))
                .with_separator(SeparatorStyle::None)
                .with_tag_items(
                    vec![
                        mara_core::pod::TagItem::new("widgets"),
                        mara_core::pod::TagItem::new("ribbons"),
                        mara_core::pod::TagItem::new("panes"),
                        mara_core::pod::TagItem::new("pods"),
                        mara_core::pod::TagItem::new("graph-graph"),
                        mara_core::pod::TagItem::new("code-editor"),
                        mara_core::pod::TagItem::new("theme/PRO"),
                        mara_core::pod::TagItem::new("theme/GAME"),
                        mara_core::pod::TagItem::new("theme/FLAT"),
                        mara_core::pod::TagItem::colored("experimental", mara_core::style::WARNING),
                        mara_core::pod::TagItem::colored("stable-api", mara_core::style::SUCCESS),
                    ],
                    accent,
                ),
        ],
    );
    body.add_normal(
        cid(PANE_ABOUT, "stats"),
        "Stage stats",
        "info",
        vec![
            Pod::new(pid(PANE_ABOUT, "stats", 0))
                .with_separator(SeparatorStyle::Line)
                .with_badge_row("lights", vec!["12 dir", "4 pt", "2 spot", "1 dome"], accent),
            Pod::new(pid(PANE_ABOUT, "stats", 1))
                .with_separator(SeparatorStyle::Line)
                .with_badge_row("instances", vec!["3 proto", "128 inst", "anim"], accent),
            Pod::new(pid(PANE_ABOUT, "stats", 2))
                .with_separator(SeparatorStyle::Line)
                .with_badge_row("skel", vec!["6 skel", "1 root", "84 bind"], accent),
            Pod::new(pid(PANE_ABOUT, "stats", 3))
                .with_separator(SeparatorStyle::Line)
                .with_badge_row("render", vec!["1 settings", "2 product", "3 var"], accent),
            Pod::new(pid(PANE_ABOUT, "stats", 4))
                .with_separator(SeparatorStyle::None)
                .with_badge_row_items(
                    "physics",
                    vec![
                        mara_core::pod::TagItem::new("1 scene"),
                        mara_core::pod::TagItem::new("12 rb"),
                        mara_core::pod::TagItem::colored("broken", mara_core::style::WARNING),
                    ],
                    accent,
                ),
        ],
    );
}

fn canvas_brush_pane(body: &mut PaneBody) {
    let accent = body.accent();
    body.add_normal(
        cid(PANE_CANVAS_BRUSH, "brush"),
        "Brush",
        "paint-brush",
        vec![
            Pod::new(pid(PANE_CANVAS_BRUSH, "brush", 0))
                .with_separator(SeparatorStyle::Line)
                .with_slider("size", 6.0, 1.0..=32.0, 1, " px", accent),
            Pod::new(pid(PANE_CANVAS_BRUSH, "brush", 1))
                .with_separator(SeparatorStyle::Line)
                .with_slider("opacity", 1.0, 0.05..=1.0, 2, "", accent),
            Pod::new(pid(PANE_CANVAS_BRUSH, "brush", 2))
                .with_separator(SeparatorStyle::None)
                .with_toggle_initial("pressure", accent, true),
        ],
    );
}

fn canvas_layers_pane(body: &mut PaneBody) {
    let accent = body.accent();
    body.add_normal(
        cid(PANE_CANVAS_LAYERS, "layers"),
        "Layers",
        "square-multiple",
        vec![
            Pod::new(pid(PANE_CANVAS_LAYERS, "layers", 0))
                .with_separator(SeparatorStyle::Line)
                .with_select_list(
                    vec![
                        "Sketch".to_owned(),
                        "Ink".to_owned(),
                        "Annotations".to_owned(),
                    ],
                    None::<Vec<String>>,
                    accent,
                ),
            Pod::new(pid(PANE_CANVAS_LAYERS, "layers", 1))
                .with_separator(SeparatorStyle::None)
                .with_toggle_initial("show grid", accent, true),
        ],
    );
}

fn canvas_assets_pane(body: &mut PaneBody) {
    let accent = body.accent();
    body.add_normal(
        cid(PANE_CANVAS_ASSETS, "assets"),
        "Assets",
        "image",
        vec![
            Pod::new(pid(PANE_CANVAS_ASSETS, "assets", 0))
                .with_separator(SeparatorStyle::Line)
                .with_search("search images…", accent),
            Pod::new(pid(PANE_CANVAS_ASSETS, "assets", 1))
                .with_separator(SeparatorStyle::None)
                .with_button("Import image", accent),
        ],
    );
}

fn canvas_inspector_pane(body: &mut PaneBody) {
    let accent = body.accent();
    body.add_normal(
        cid(PANE_CANVAS_INSPECTOR, "selection"),
        "Selection",
        "sliders",
        vec![
            Pod::new(pid(PANE_CANVAS_INSPECTOR, "selection", 0))
                .with_separator(SeparatorStyle::Line)
                .with_readout("selection", "none"),
            Pod::new(pid(PANE_CANVAS_INSPECTOR, "selection", 1))
                .with_separator(SeparatorStyle::None)
                .with_slider("scale", 1.0, 0.25..=4.0, 2, "x", accent),
        ],
    );
}

fn canvas_history_pane(body: &mut PaneBody) {
    let accent = body.accent();
    body.add_normal(
        cid(PANE_CANVAS_HISTORY, "history"),
        "History",
        "history",
        vec![
            Pod::new(pid(PANE_CANVAS_HISTORY, "history", 0))
                .with_separator(SeparatorStyle::None)
                .with_select_list(
                    vec![
                        "New stroke".to_owned(),
                        "Brush changed".to_owned(),
                        "Layer toggled".to_owned(),
                    ],
                    None::<Vec<String>>,
                    accent,
                ),
        ],
    );
}

fn canvas_export_pane(body: &mut PaneBody) {
    let accent = body.accent();
    body.add_normal(
        cid(PANE_CANVAS_EXPORT, "export"),
        "Export",
        "download",
        vec![
            Pod::new(pid(PANE_CANVAS_EXPORT, "export", 0))
                .with_separator(SeparatorStyle::Line)
                .with_readout("format", "PNG"),
            Pod::new(pid(PANE_CANVAS_EXPORT, "export", 1))
                .with_separator(SeparatorStyle::None)
                .with_button("Export canvas", accent),
        ],
    );
}

fn three_d_scene_pane(body: &mut PaneBody, three_d: &ThreeDViewState) {
    let scene = three_d.view.scene();
    let mut pods = Vec::with_capacity(scene.objects.len().saturating_add(1));
    pods.push(
        Pod::new(pid(PANE_3D_SCENE, "summary", 0))
            .with_separator(SeparatorStyle::Line)
            .with_readout("objects", scene.objects.len().to_string()),
    );
    for (index, object) in scene.objects.iter().enumerate() {
        let state = if object.selected {
            format!("{} · selected", object.kind_name())
        } else if object.visible {
            object.kind_name().to_owned()
        } else {
            format!("{} · hidden", object.kind_name())
        };
        pods.push(
            Pod::new(pid(PANE_3D_SCENE, "object", index))
                .with_separator(if index + 1 == scene.objects.len() {
                    SeparatorStyle::None
                } else {
                    SeparatorStyle::Line
                })
                .with_readout(object.name.clone(), state),
        );
    }
    body.add_normal(cid(PANE_3D_SCENE, "objects"), "Objects", "folder", pods);
}

fn three_d_inspector_pane(body: &mut PaneBody, three_d: &ThreeDViewState) {
    let scene = three_d.view.scene();
    let orbit = three_d.view.orbit();
    let selected = scene.selected_object();
    let mut pods = vec![
        Pod::new(pid(PANE_3D_INSPECTOR, "selection", 0))
            .with_separator(SeparatorStyle::Line)
            .with_readout(
                "selection",
                selected.map_or_else(|| "none".to_owned(), |object| object.name.clone()),
            ),
        Pod::new(pid(PANE_3D_INSPECTOR, "selection", 1))
            .with_separator(SeparatorStyle::Line)
            .with_readout(
                "orbit",
                format!(
                    "yaw {:.2} · pitch {:.2} · dist {:.2}",
                    orbit.yaw, orbit.pitch, orbit.distance
                ),
            ),
    ];

    if let Some(object) = selected {
        pods.push(
            Pod::new(pid(PANE_3D_INSPECTOR, "selection", 2))
                .with_separator(SeparatorStyle::Line)
                .with_readout("id", format!("#{}", object.id.0)),
        );
        pods.push(
            Pod::new(pid(PANE_3D_INSPECTOR, "selection", 3))
                .with_separator(SeparatorStyle::Line)
                .with_readout(
                    "position",
                    format!(
                        "{:.2}, {:.2}, {:.2}",
                        object.transform.translation[0],
                        object.transform.translation[1],
                        object.transform.translation[2]
                    ),
                ),
        );
        pods.push(
            Pod::new(pid(PANE_3D_INSPECTOR, "selection", 4))
                .with_separator(SeparatorStyle::None)
                .with_readout("primitive", object.kind_name()),
        );
    } else {
        pods.push(
            Pod::new(pid(PANE_3D_INSPECTOR, "selection", 2))
                .with_separator(SeparatorStyle::None)
                .with_readout("hint", "click an object"),
        );
    }

    body.add_normal(
        cid(PANE_3D_INSPECTOR, "selection"),
        "Selection",
        "options",
        pods,
    );
}

/// **Editor pane** — node graph (top) + code editor (bottom),
/// each in its own container with a fill pod so they soak up the
/// pane's available space. Driven by the vendored `mara_core::extras`
/// wrappers.
///
/// The graph container is rendered via `Normal::show_raw` rather
/// than the standard `with_custom_units` pod path so we can pass
/// `&mut NodeViewState`, `&mut Graph`, `&mut Viewer`, and the
/// Bevy-side `&mut dyn NodeViewBackend` straight through to
/// `mara_node_graph`. The pod-path closure has a `'static` bound
/// that those refs can't satisfy.
#[allow(clippy::too_many_arguments)]
fn editor_pane<'spec>(
    body: &mut PaneBody<'_, 'spec>,
    nav: &'spec mut mara::extras::graph::render::GraphViewState,
    graph: &'spec mut Graph<GraphNode>,
    viewer: &'spec mut DemoViewer,
    accent: MaraColor32,
) {
    let cid_graph = cid(PANE_EDITOR, "graph");
    let code_id = cid(PANE_EDITOR, "code_state");

    // The same renderer and the same spec the Graph Lab uses. Two
    // surfaces that assembled their own styling is why the app used to
    // show two node editors that plainly were not the same widget.
    body.add_graph_view(
        cid_graph,
        "Node graph",
        "flowchart",
        graph,
        viewer,
        nav,
        mara::extras::graph::mara_graph_spec(accent),
    );
    // Code editor goes through `Pod::with_code_editor` (typed
    // pod constructor, feature-gated under `code`). Text buffer
    // lives in ctx data under `code_id`; seeded on first render.
    body.add_normal(
        cid(PANE_EDITOR, "code"),
        "Source",
        "code",
        vec![
            Pod::new(pid(PANE_EDITOR, "code", 0))
                .with_separator(SeparatorStyle::None)
                .fill()
                .with_code_editor(code_id, Syntax::rust(), DEFAULT_CODE),
        ],
    );
}

// ─── Node-graph types (used by Editor pane) ────────────────────────
//
// A multi-typed node graph styled after Blackjack + noise_gui. Pins
// carry typed values (`Value::Number / Vector / Color / Bool /
// Text`) — colour-coded and shape-coded so the user can read the
// graph at a glance, with implicit conversion between compatible
// types when the evaluator pulls a value from the wrong pin shape.
//
// Categories of nodes implemented:
//   * **Sources** — Number / Vector / Color / Bool / Time
//   * **Scalar math** — ScalarMath / Trig / Compare / Mix / Clamp
//   * **Vector** — VectorMath / Compose / Decompose / Length
//   * **Colour** — RgbToColor / ColorMix
//   * **Logic** — IfElse
//   * **Noise** — Perlin (1-D value noise from a `t` input)
//   * **Sinks** — Display (sparkline) / Preview (swatch) / Output
//
// Each variant shows off a different egui widget in its body / pin
// rows so the demo doubles as a widget gallery: drag values, color
// pickers, toggles, dropdowns, sliders, mini sparklines, etc.

#[derive(Clone, Copy, PartialEq)]
enum PinType {
    Number,
    Vector,
    Color,
    Bool,
    Text,
}

impl PinType {
    /// Canonical fill colour for pins of this type, and therefore for
    /// every wire leaving one.
    ///
    /// Five hues at roughly one lightness and one saturation, so no
    /// type shouts over the others and none disappears. The previous
    /// set was Unreal's literal Blueprint palette, which pairs a neon
    /// lime with a near-black maroon: on a canvas where most of the ink
    /// is wire, that reads as an electrical fault rather than as type
    /// information.
    fn color(self) -> MaraColor32 {
        match self {
            PinType::Number => MaraColor32::from_rgb(0x6F, 0xCF, 0x97),
            PinType::Vector => MaraColor32::from_rgb(0xF2, 0xC9, 0x4C),
            PinType::Color => MaraColor32::from_rgb(0xBB, 0x6B, 0xD9),
            PinType::Bool => MaraColor32::from_rgb(0xEB, 0x5F, 0x5F),
            PinType::Text => MaraColor32::from_rgb(0x56, 0xCC, 0xF2),
        }
    }

    /// `PinInfo` for this pin's type, sized as a uniform circle
    /// across all types (matches Unreal Blueprints' single-shape
    /// pin convention; type info is carried by colour alone).
    ///
    /// `connected` toggles the fill: a connected pin is solid in
    /// the type colour with a thin dark outline, an unconnected
    /// pin is a hollow ring (transparent fill + a thicker stroke
    /// in the type colour) — visually telling the user "this slot
    /// expects a wire".
    fn pin(self, connected: bool) -> PinInfo {
        // `PinInfo` speaks vocab since WS-D1.3 ported `pin.rs`, so this
        // no longer converts at a boundary — it just passes data through.
        let fill = self.color();
        if connected {
            PinInfo::circle()
                .with_fill(fill)
                .with_stroke(MaraStroke::new(1.0, MaraColor32::from_black_alpha(180)))
        } else {
            PinInfo::circle()
                .with_fill(MaraColor32::TRANSPARENT)
                .with_stroke(MaraStroke::new(1.5, fill))
        }
    }
}

/// A value flowing along a wire. The graph is dynamically typed —
/// pins advertise an "expected" `PinType` for clarity, but the
/// evaluator coerces on read so e.g. plugging a `Vector` into a
/// scalar slot yields `length(v)` rather than an error.
#[derive(Clone)]
#[allow(dead_code)] // `Text` is part of the type-spectrum the graph
// models even though no current node emits it.
enum Value {
    Number(f64),
    Vector([f64; 3]),
    Color(MaraColor32),
    Bool(bool),
    Text(String),
}

impl Value {
    fn as_number(&self) -> f64 {
        match self {
            Value::Number(v) => *v,
            Value::Vector(v) => (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt(),
            Value::Color(c) => c.r() as f64 / 255.0,
            Value::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            Value::Text(s) => s.parse().unwrap_or(0.0),
        }
    }
    fn as_vector(&self) -> [f64; 3] {
        match self {
            Value::Number(v) => [*v, *v, *v],
            Value::Vector(v) => *v,
            Value::Color(c) => [
                c.r() as f64 / 255.0,
                c.g() as f64 / 255.0,
                c.b() as f64 / 255.0,
            ],
            Value::Bool(b) => {
                let v = if *b { 1.0 } else { 0.0 };
                [v; 3]
            }
            Value::Text(_) => [0.0; 3],
        }
    }
    fn as_color(&self) -> MaraColor32 {
        let to_u8 = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        match self {
            Value::Number(v) => {
                let g = to_u8(*v);
                MaraColor32::from_rgb(g, g, g)
            }
            Value::Vector(v) => MaraColor32::from_rgb(to_u8(v[0]), to_u8(v[1]), to_u8(v[2])),
            Value::Color(c) => *c,
            Value::Bool(b) => {
                if *b {
                    MaraColor32::WHITE
                } else {
                    MaraColor32::BLACK
                }
            }
            Value::Text(_) => MaraColor32::GRAY,
        }
    }
    fn as_bool(&self) -> bool {
        match self {
            Value::Number(v) => *v >= 0.5,
            Value::Vector(v) => v[0] * v[0] + v[1] * v[1] + v[2] * v[2] > 0.0,
            Value::Color(c) => c.r() as u16 + c.g() as u16 + c.b() as u16 > 384,
            Value::Bool(b) => *b,
            Value::Text(s) => !s.is_empty(),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ScalarOp {
    Add,
    Sub,
    Mul,
    Div,
    Min,
    Max,
    Pow,
    Mod,
    SmoothMin,
    SmoothMax,
}
impl ScalarOp {
    fn label(self) -> &'static str {
        match self {
            Self::Add => "a + b",
            Self::Sub => "a − b",
            Self::Mul => "a × b",
            Self::Div => "a ÷ b",
            Self::Min => "min(a,b)",
            Self::Max => "max(a,b)",
            Self::Pow => "a ^ b",
            Self::Mod => "a mod b",
            Self::SmoothMin => "smin(a,b)",
            Self::SmoothMax => "smax(a,b)",
        }
    }
    fn apply(self, a: f64, b: f64) -> f64 {
        match self {
            Self::Add => a + b,
            Self::Sub => a - b,
            Self::Mul => a * b,
            Self::Div => {
                if b.abs() < 1e-9 {
                    0.0
                } else {
                    a / b
                }
            }
            Self::Min => a.min(b),
            Self::Max => a.max(b),
            Self::Pow => a.powf(b),
            Self::Mod => {
                if b.abs() < 1e-9 {
                    0.0
                } else {
                    a.rem_euclid(b)
                }
            }
            // Smooth min/max (h=0.5 default) — exact Blender formula.
            Self::SmoothMin => {
                let k = 0.5;
                let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
                b * (1.0 - h) + a * h - k * h * (1.0 - h)
            }
            Self::SmoothMax => {
                let k = 0.5;
                let h = (0.5 - 0.5 * (b - a) / k).clamp(0.0, 1.0);
                b * (1.0 - h) + a * h + k * h * (1.0 - h)
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum TrigFn {
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Sinh,
    Cosh,
    Tanh,
    Sqrt,
    Abs,
    Floor,
    Ceil,
    Round,
    Trunc,
    Frac,
    Sign,
    Exp,
    Log,
}
impl TrigFn {
    fn label(self) -> &'static str {
        match self {
            Self::Sin => "sin",
            Self::Cos => "cos",
            Self::Tan => "tan",
            Self::Asin => "asin",
            Self::Acos => "acos",
            Self::Atan => "atan",
            Self::Sinh => "sinh",
            Self::Cosh => "cosh",
            Self::Tanh => "tanh",
            Self::Sqrt => "sqrt",
            Self::Abs => "abs",
            Self::Floor => "floor",
            Self::Ceil => "ceil",
            Self::Round => "round",
            Self::Trunc => "trunc",
            Self::Frac => "frac",
            Self::Sign => "sign",
            Self::Exp => "exp",
            Self::Log => "ln",
        }
    }
    fn apply(self, x: f64) -> f64 {
        match self {
            Self::Sin => x.sin(),
            Self::Cos => x.cos(),
            Self::Tan => x.tan(),
            Self::Asin => x.clamp(-1.0, 1.0).asin(),
            Self::Acos => x.clamp(-1.0, 1.0).acos(),
            Self::Atan => x.atan(),
            Self::Sinh => x.sinh(),
            Self::Cosh => x.cosh(),
            Self::Tanh => x.tanh(),
            Self::Sqrt => x.max(0.0).sqrt(),
            Self::Abs => x.abs(),
            Self::Floor => x.floor(),
            Self::Ceil => x.ceil(),
            Self::Round => x.round(),
            Self::Trunc => x.trunc(),
            Self::Frac => x - x.floor(),
            Self::Sign => x.signum(),
            Self::Exp => x.exp(),
            Self::Log => {
                if x > 0.0 {
                    x.ln()
                } else {
                    0.0
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum BoolOp {
    And,
    Or,
    Not,
    Xor,
    Nand,
    Nor,
    Xnor,
}
impl BoolOp {
    fn label(self) -> &'static str {
        match self {
            Self::And => "a ∧ b",
            Self::Or => "a ∨ b",
            Self::Not => "¬a",
            Self::Xor => "a ⊕ b",
            Self::Nand => "a ⊼ b",
            Self::Nor => "a ⊽ b",
            Self::Xnor => "a = b",
        }
    }
    fn apply(self, a: bool, b: bool) -> bool {
        match self {
            Self::And => a && b,
            Self::Or => a || b,
            Self::Not => !a,
            Self::Xor => a ^ b,
            Self::Nand => !(a && b),
            Self::Nor => !(a || b),
            Self::Xnor => a == b,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
#[allow(dead_code)] // Turbulence + Ridged are reserved variants —
// the current demo only selects FBM, but the
// noise-node UI lists all three modes.
enum NoiseMode {
    FBM,
    Turbulence,
    Ridged,
}
impl NoiseMode {
    #[allow(dead_code)]
    fn label(self) -> &'static str {
        match self {
            Self::FBM => "FBM",
            Self::Turbulence => "Turbulence",
            Self::Ridged => "Ridged",
        }
    }
    /// Combine octaves of value-noise into the chosen pattern.
    /// `octaves`, `persistence` (amplitude decay), `lacunarity`
    /// (frequency growth) follow the standard FBM convention.
    fn sample(
        self,
        seed: u32,
        x: f64,
        y: f64,
        octaves: u32,
        persistence: f64,
        lacunarity: f64,
    ) -> f64 {
        let mut amp = 1.0;
        let mut freq = 1.0;
        let mut acc = 0.0;
        let mut norm = 0.0;
        for o in 0..octaves.max(1) {
            let n = sample_2d_value_noise(seed.wrapping_add(o), x * freq, y * freq);
            // sample_2d_value_noise is 0..1 — re-centre to -1..1.
            let s = n * 2.0 - 1.0;
            let v = match self {
                Self::FBM => s,
                Self::Turbulence => s.abs(),
                Self::Ridged => 1.0 - s.abs(),
            };
            acc += v * amp;
            norm += amp;
            amp *= persistence;
            freq *= lacunarity;
        }
        let v = acc / norm.max(1e-9);
        // Re-map FBM/Ridged from -1..1 to 0..1 for display.
        match self {
            Self::FBM => v * 0.5 + 0.5,
            Self::Ridged => v * 0.5 + 0.5,
            Self::Turbulence => v.clamp(0.0, 1.0),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum WaveShape {
    Sine,
    Saw,
    Triangle,
    Square,
}
impl WaveShape {
    fn label(self) -> &'static str {
        match self {
            Self::Sine => "sine",
            Self::Saw => "saw",
            Self::Triangle => "triangle",
            Self::Square => "square",
        }
    }
    fn apply(self, t: f64) -> f64 {
        let p = t - t.floor(); // 0..1
        match self {
            Self::Sine => (t * std::f64::consts::TAU).sin(),
            Self::Saw => p * 2.0 - 1.0,
            Self::Triangle => 1.0 - 4.0 * (p - 0.5).abs(),
            Self::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum CompareOp {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
    Ne,
}
impl CompareOp {
    fn label(self) -> &'static str {
        match self {
            Self::Lt => "a < b",
            Self::Le => "a ≤ b",
            Self::Eq => "a = b",
            Self::Ge => "a ≥ b",
            Self::Gt => "a > b",
            Self::Ne => "a ≠ b",
        }
    }
    fn apply(self, a: f64, b: f64) -> bool {
        match self {
            Self::Lt => a < b,
            Self::Le => a <= b,
            Self::Eq => (a - b).abs() < 1e-9,
            Self::Ge => a >= b,
            Self::Gt => a > b,
            Self::Ne => (a - b).abs() >= 1e-9,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum VectorOp {
    Add,
    Sub,
    Mul,
    Cross,
}
impl VectorOp {
    fn label(self) -> &'static str {
        match self {
            Self::Add => "a + b",
            Self::Sub => "a − b",
            Self::Mul => "a ⊙ b",
            Self::Cross => "a × b",
        }
    }
    fn apply(self, a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
        match self {
            Self::Add => [a[0] + b[0], a[1] + b[1], a[2] + b[2]],
            Self::Sub => [a[0] - b[0], a[1] - b[1], a[2] - b[2]],
            Self::Mul => [a[0] * b[0], a[1] * b[1], a[2] * b[2]],
            Self::Cross => [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ],
        }
    }
}

#[derive(Clone)]
pub enum GraphNode {
    // ── Subgraph plumbing (PLAN_NODE.md P7/P8) ──
    //
    // Minted by `GraphLabFactory` when a selection is collapsed. The
    // crate answers an instance's pin COUNT from its definition's
    // interface, so these never have to know how many pins they have —
    // but `show_input`/`show_output` still route here, so they need to
    // exist as payloads.
    /// A placed instance of a definition.
    Subgraph,
    /// A boundary node standing for one port of the enclosing
    /// definition.
    ///
    /// Carries the port's name because nothing else can: the names live
    /// in the definition's `Ports`, and a viewer is only ever handed
    /// one level of the document. Without it every boundary node inside
    /// a chip is titled "Port", which is three identical nodes on
    /// screen and no way to tell which pin is which.
    Port {
        out: bool,
        name: String,
    },

    // ── Sources ──
    Number(f64),
    Integer(i64),
    Vector([f64; 3]),
    Color(MaraColor32),
    Bool(bool),
    Time,

    // ── Scalar math ──
    ScalarMath(ScalarOp),
    Trig(TrigFn),
    Compare(CompareOp),
    Mix,        // lerp(a, b, t)
    Clamp,      // clamp(x, lo, hi)
    MapRange,   // remap (x, in_lo, in_hi, out_lo, out_hi)
    Smoothstep, // smoothstep(edge0, edge1, x)
    Step,       // step(edge, x) — 0 if x<edge else 1

    // ── Vector ──
    VectorMath(VectorOp),
    Compose,      // x, y, z → vec3
    Decompose,    // vec3 → x, y, z
    Length,       // ||v||
    Dot,          // a · b → scalar
    Distance,     // |a − b| → scalar
    Normalize,    // v / |v| → vec3
    VectorRotate, // rotate v around axis by angle → vec3
    Reflect,      // reflect v across plane(normal) → vec3

    // ── Colour ──
    RgbToColor,     // r, g, b → color
    HsvToColor,     // h, s, v → color
    ColorMix,       // lerp(c1, c2, t)
    HueShift,       // rotate hue → color
    ColorInvert,    // (1 - r, 1 - g, 1 - b) → color
    BrightContrast, // brightness × contrast → color
    Gamma,          // c^gamma → color

    // ── Logic ──
    IfElse,              // bool ? a : b
    BooleanMath(BoolOp), // and/or/not/xor/...
    FloatToBool,         // x > threshold → bool
    BoolToFloat,         // bool ? 1 : 0 → f64

    // ── Noise / wave ──
    Perlin {
        // 1-D value noise
        seed: u32,
        frequency: f64,
    },
    WhiteNoise {
        // unsmoothed hash-based pseudo-random
        seed: u32,
    },
    Wave(WaveShape), // sine / saw / triangle / square

    // ── Sinks / display ──
    Display, // scalar with sparkline
    Plot,    // scalar with auto-scale chart + min/max readout
    PlotXY,  // sophisticated egui_plot line chart of a value
    // over time, with auto-fit axes and grid
    Preview,       // color swatch
    VectorPreview, // x/y/z bars
    NoiseImage {
        // 2-D noise rendered as a 96×64 image inside
        // the body — mirrors `noise_gui` previews
        seed: u32,
        scale: f64,
    },
    NoiseField, // Sophisticated multi-octave FBM noise.
    // PURE OUTPUT — every parameter (seed,
    // offsets, freq, octaves, persistence,
    // lacunarity, gain) is a graph-wired input.
    // Body is ONLY the 160 × 96 px image. No
    // sliders, no dropdowns, no buttons.
    MultiPlot, // 4-channel oscilloscope-style egui_plot
    // chart — `a / b / c / d` rendered as
    // separate coloured lines on shared axes.
    Output, // terminal sink
}

/// Node category — drives the per-node header gradient (à la
/// Unreal Blueprints' "color spill") with the actual hex values
/// taken from Blender 4.x's `nodeclass_*` palette so the muscle
/// memory of seasoned users carries over.
#[derive(Clone, Copy, PartialEq)]
enum Category {
    Source,     // Blender "Input"      — dusty rose
    ScalarMath, // Blender "Converter"  — steel blue
    Vector,     // Blender "Vector"     — indigo
    Color,      // Blender "Color"      — olive
    Logic,      // Blender "Filter"     — deep purple
    Noise,      // Blender "Texture"    — brown
    Sink,       // Blender "Output"     — dark maroon
}

impl Category {
    /// Header tint, fully opaque. The header band is painted as a
    /// horizontal gradient `(tint @ alpha 0.85) → (transparent)`
    /// so the dark body fill bleeds through past the title (UE's
    /// "color spill" pattern but with Blender's palette).
    fn color(self) -> MaraColor32 {
        match self {
            Category::Source => MaraColor32::from_rgb(0x82, 0x35, 0x4C), // syntaxn
            Category::ScalarMath => MaraColor32::from_rgb(0x24, 0x62, 0x83), // syntaxv
            Category::Vector => MaraColor32::from_rgb(0x3C, 0x3C, 0x83), // nodeclass_vector
            Category::Color => MaraColor32::from_rgb(0x6E, 0x6E, 0x23),  // syntaxb
            Category::Logic => MaraColor32::from_rgb(0x41, 0x2B, 0x51),  // nodeclass_filter
            Category::Noise => MaraColor32::from_rgb(0x79, 0x46, 0x1D),  // nodeclass_texture
            Category::Sink => MaraColor32::from_rgb(0x3E, 0x23, 0x2A),   // nodeclass_output
        }
    }
}

impl GraphNode {
    /// Small Fluent-UI icon glyph painted to the left of the
    /// title in the header band. Picked from the set bundled in
    /// `mara_core::icons` so missing-glyph fallback never kicks in.
    ///
    /// A boundary node's arrow points the way the value travels, which
    /// is the opposite of the side the node sits on: an `in` port is
    /// where the interior gets its value FROM, so it exports.
    fn icon_name(&self) -> &'static str {
        match self {
            GraphNode::Subgraph => "square-multiple",
            GraphNode::Port { out: false, .. } => "arrow-export",
            GraphNode::Port { out: true, .. } => "arrow-import",
            // Sources
            GraphNode::Number(_) => "calculator",
            GraphNode::Integer(_) => "calculator",
            GraphNode::Vector(_) => "flowchart",
            GraphNode::Color(_) => "color",
            GraphNode::Bool(_) => "checkmark",
            GraphNode::Time => "clock",
            // Scalar math
            GraphNode::ScalarMath(_) => "calculator",
            GraphNode::Trig(_) => "math-formula",
            GraphNode::Compare(_) => "scales",
            GraphNode::Mix => "merge",
            GraphNode::Clamp => "border-all",
            GraphNode::MapRange => "ruler",
            GraphNode::Smoothstep => "pulse",
            GraphNode::Step => "pulse-square",
            // Vector
            GraphNode::VectorMath(_) => "flowchart",
            GraphNode::Compose => "merge",
            GraphNode::Decompose => "arrow-split",
            GraphNode::Length => "ruler",
            GraphNode::Dot => "calculator",
            GraphNode::Distance => "ruler",
            GraphNode::Normalize => "flowchart",
            GraphNode::VectorRotate => "flowchart",
            GraphNode::Reflect => "branch",
            // Colour
            GraphNode::RgbToColor => "color",
            GraphNode::HsvToColor => "color",
            GraphNode::ColorMix => "color",
            GraphNode::HueShift => "color",
            GraphNode::ColorInvert => "color",
            GraphNode::BrightContrast => "color",
            GraphNode::Gamma => "color",
            // Logic
            GraphNode::IfElse => "branch",
            GraphNode::BooleanMath(_) => "code",
            GraphNode::FloatToBool => "code",
            GraphNode::BoolToFloat => "code",
            // Noise
            GraphNode::Perlin { .. } => "pulse",
            GraphNode::WhiteNoise { .. } => "pulse",
            GraphNode::Wave(_) => "pulse",
            // Sinks
            GraphNode::Display => "chart-multiple",
            GraphNode::Plot => "chart-multiple",
            GraphNode::PlotXY => "chart-multiple",
            GraphNode::Preview => "image",
            GraphNode::VectorPreview => "image",
            GraphNode::NoiseImage { .. } => "image",
            GraphNode::NoiseField => "image",
            GraphNode::MultiPlot => "chart-multiple",
            GraphNode::Output => "save",
        }
    }

    /// Smaller subtitle line under the main title — describes the
    /// node's *current* state (selected operator, value type, etc.)
    /// the way Unreal Blueprint title bars show e.g. "Float" under
    /// "Add". Stays in sync with the dropdown in the body.
    ///
    /// A `Subgraph` is overridden per placement by `DemoViewer::look`,
    /// which knows the definition's scope; the string here is what an
    /// instance rendered outside a document falls back to.
    fn subtitle(&self) -> String {
        match self {
            GraphNode::Subgraph => "definition".into(),
            GraphNode::Port { out: false, .. } => "input".into(),
            GraphNode::Port { out: true, .. } => "output".into(),
            // Sources
            GraphNode::Number(_) => "Float".into(),
            GraphNode::Integer(_) => "Int".into(),
            GraphNode::Vector(_) => "Vec3".into(),
            GraphNode::Color(_) => "RGBA".into(),
            GraphNode::Bool(_) => "Bool".into(),
            GraphNode::Time => "seconds".into(),
            // Scalar math
            GraphNode::ScalarMath(op) => op.label().into(),
            GraphNode::Trig(f) => f.label().into(),
            GraphNode::Compare(op) => op.label().into(),
            GraphNode::Mix => "lerp(a, b, t)".into(),
            GraphNode::Clamp => "clamp(x, lo, hi)".into(),
            GraphNode::MapRange => "remap range".into(),
            GraphNode::Smoothstep => "smoothstep(e0, e1, x)".into(),
            GraphNode::Step => "step(edge, x)".into(),
            // Vector
            GraphNode::VectorMath(op) => op.label().into(),
            GraphNode::Compose => "x, y, z → vec".into(),
            GraphNode::Decompose => "vec → x, y, z".into(),
            GraphNode::Length => "‖v‖".into(),
            GraphNode::Dot => "a · b".into(),
            GraphNode::Distance => "‖a − b‖".into(),
            GraphNode::Normalize => "v / ‖v‖".into(),
            GraphNode::VectorRotate => "rotate axis-angle".into(),
            GraphNode::Reflect => "reflect across n".into(),
            // Colour
            GraphNode::RgbToColor => "RGB → Color".into(),
            GraphNode::HsvToColor => "HSV → Color".into(),
            GraphNode::ColorMix => "lerp(c₁, c₂, t)".into(),
            GraphNode::HueShift => "rotate hue".into(),
            GraphNode::ColorInvert => "1 − rgb".into(),
            GraphNode::BrightContrast => "bright × contrast".into(),
            GraphNode::Gamma => "c ^ γ".into(),
            // Logic
            GraphNode::IfElse => "cond ? a : b".into(),
            GraphNode::BooleanMath(op) => op.label().into(),
            GraphNode::FloatToBool => "x > threshold".into(),
            GraphNode::BoolToFloat => "true → 1, false → 0".into(),
            // Noise
            GraphNode::Perlin { seed, .. } => format!("seed {seed}"),
            GraphNode::WhiteNoise { seed } => format!("hash, seed {seed}"),
            GraphNode::Wave(s) => format!("{} wave", s.label()),
            // Sinks
            GraphNode::Display => "scalar + sparkline".into(),
            GraphNode::Plot => "auto-scale chart".into(),
            GraphNode::PlotXY => "egui_plot line chart".into(),
            GraphNode::Preview => "color swatch".into(),
            GraphNode::VectorPreview => "x/y/z bars".into(),
            GraphNode::NoiseImage { seed, .. } => format!("2-D noise · seed {seed}"),
            GraphNode::NoiseField => "FBM · multi-octave".into(),
            GraphNode::MultiPlot => "4-channel scope".into(),
            GraphNode::Output => "sink".into(),
        }
    }

    /// Height this node's body needs, in graph points.
    ///
    /// The rebuilt renderer decides geometry before anything is drawn,
    /// so a node declares what its body needs instead of growing to fit
    /// whatever came out. That is what stops two `Add` nodes ending up
    /// different sizes because one of them had a value typed into it.
    fn body_height(&self) -> f32 {
        const ROW: f32 = 22.0;
        match self {
            GraphNode::Number(_) | GraphNode::Integer(_) | GraphNode::Bool(_) => ROW,
            GraphNode::Vector(_) => 3.0 * ROW,
            GraphNode::Color(_) => ROW + 4.0,
            GraphNode::ScalarMath(_)
            | GraphNode::Trig(_)
            | GraphNode::Compare(_)
            | GraphNode::VectorMath(_)
            | GraphNode::BooleanMath(_)
            | GraphNode::Wave(_) => ROW + 8.0,
            GraphNode::Perlin { .. } => 2.0 * ROW + 8.0,
            GraphNode::WhiteNoise { .. } => ROW + 4.0,
            GraphNode::Display => 44.0,
            GraphNode::Plot => 60.0,
            GraphNode::PlotXY => 88.0,
            GraphNode::Preview | GraphNode::VectorPreview => 46.0,
            GraphNode::NoiseImage { .. } | GraphNode::NoiseField => 100.0,
            GraphNode::MultiPlot => 96.0,
            GraphNode::Output => ROW + 4.0,
            _ => 0.0,
        }
    }

    /// Which colour family the node belongs to.
    ///
    /// A chip's own colour is its definition's, resolved in
    /// `DemoViewer::look`; `Subgraph` here is the tint it falls back to
    /// when there is no library to ask.
    ///
    /// A boundary node is not "structure" to the eye, it is the
    /// interior's source and its sink. Colouring it as such makes a
    /// definition's guts read exactly like the graph they were cut out
    /// of: values enter rose on the left and land in maroon on the
    /// right.
    fn category(&self) -> Category {
        match self {
            GraphNode::Subgraph => Category::Logic,
            GraphNode::Port { out: false, .. } => Category::Source,
            GraphNode::Port { out: true, .. } => Category::Sink,
            GraphNode::Number(_)
            | GraphNode::Integer(_)
            | GraphNode::Vector(_)
            | GraphNode::Color(_)
            | GraphNode::Bool(_)
            | GraphNode::Time => Category::Source,
            GraphNode::ScalarMath(_)
            | GraphNode::Trig(_)
            | GraphNode::Compare(_)
            | GraphNode::Mix
            | GraphNode::Clamp
            | GraphNode::MapRange
            | GraphNode::Smoothstep
            | GraphNode::Step => Category::ScalarMath,
            GraphNode::VectorMath(_)
            | GraphNode::Compose
            | GraphNode::Decompose
            | GraphNode::Length
            | GraphNode::Dot
            | GraphNode::Distance
            | GraphNode::Normalize
            | GraphNode::VectorRotate
            | GraphNode::Reflect => Category::Vector,
            GraphNode::RgbToColor
            | GraphNode::HsvToColor
            | GraphNode::ColorMix
            | GraphNode::HueShift
            | GraphNode::ColorInvert
            | GraphNode::BrightContrast
            | GraphNode::Gamma => Category::Color,
            GraphNode::IfElse
            | GraphNode::BooleanMath(_)
            | GraphNode::FloatToBool
            | GraphNode::BoolToFloat => Category::Logic,
            GraphNode::Perlin { .. } | GraphNode::WhiteNoise { .. } | GraphNode::Wave(_) => {
                Category::Noise
            }
            GraphNode::Display
            | GraphNode::Plot
            | GraphNode::PlotXY
            | GraphNode::Preview
            | GraphNode::VectorPreview
            | GraphNode::NoiseImage { .. }
            | GraphNode::NoiseField
            | GraphNode::MultiPlot
            | GraphNode::Output => Category::Sink,
        }
    }

    /// The node's name.
    ///
    /// `Subgraph` and `Port` are both replaced by `DemoViewer::look`
    /// wherever the real name is knowable — the definition's for an
    /// instance, the payload's own for a boundary node. The strings
    /// here are the last resort for a payload with neither.
    fn title(&self) -> &'static str {
        match self {
            GraphNode::Subgraph => "Subgraph",
            GraphNode::Port { .. } => "Port",
            // Sources
            GraphNode::Number(_) => "Number",
            GraphNode::Integer(_) => "Integer",
            GraphNode::Vector(_) => "Vector",
            GraphNode::Color(_) => "Color",
            GraphNode::Bool(_) => "Bool",
            GraphNode::Time => "Time",
            // Scalar math
            GraphNode::ScalarMath(_) => "Scalar Math",
            GraphNode::Trig(_) => "Math Func",
            GraphNode::Compare(_) => "Compare",
            GraphNode::Mix => "Mix",
            GraphNode::Clamp => "Clamp",
            GraphNode::MapRange => "Map Range",
            GraphNode::Smoothstep => "Smoothstep",
            GraphNode::Step => "Step",
            // Vector
            GraphNode::VectorMath(_) => "Vector Math",
            GraphNode::Compose => "Compose",
            GraphNode::Decompose => "Decompose",
            GraphNode::Length => "Length",
            GraphNode::Dot => "Dot Product",
            GraphNode::Distance => "Distance",
            GraphNode::Normalize => "Normalize",
            GraphNode::VectorRotate => "Vector Rotate",
            GraphNode::Reflect => "Reflect",
            // Colour
            GraphNode::RgbToColor => "RGB → Color",
            GraphNode::HsvToColor => "HSV → Color",
            GraphNode::ColorMix => "Color Mix",
            GraphNode::HueShift => "Hue Shift",
            GraphNode::ColorInvert => "Invert",
            GraphNode::BrightContrast => "Bright/Contrast",
            GraphNode::Gamma => "Gamma",
            // Logic
            GraphNode::IfElse => "If / Else",
            GraphNode::BooleanMath(_) => "Boolean Math",
            GraphNode::FloatToBool => "Float → Bool",
            GraphNode::BoolToFloat => "Bool → Float",
            // Noise
            GraphNode::Perlin { .. } => "Perlin",
            GraphNode::WhiteNoise { .. } => "White Noise",
            GraphNode::Wave(_) => "Wave",
            // Sinks
            GraphNode::Display => "Display",
            GraphNode::Plot => "Plot",
            GraphNode::PlotXY => "Plot XY",
            GraphNode::Preview => "Preview",
            GraphNode::VectorPreview => "Vector Preview",
            GraphNode::NoiseImage { .. } => "Noise Image",
            GraphNode::NoiseField => "Noise Field",
            GraphNode::MultiPlot => "Multi Plot",
            GraphNode::Output => "Output",
        }
    }

    /// Per-input typed-pin labels. `Vec<(label, type)>`.
    fn inputs(&self) -> Vec<(&'static str, PinType)> {
        match self {
            // An instance's pin COUNT comes from its definition, so this
            // is only a fallback for a `Subgraph` payload rendered
            // outside `show_doc`. A boundary node has exactly one pin,
            // on the side that faces the interior.
            GraphNode::Subgraph => vec![],
            GraphNode::Port { out: true, .. } => vec![("", PinType::Number)],
            GraphNode::Port { out: false, .. } => vec![],
            // Sources — no inputs
            GraphNode::Number(_)
            | GraphNode::Integer(_)
            | GraphNode::Vector(_)
            | GraphNode::Color(_)
            | GraphNode::Bool(_)
            | GraphNode::Time
            | GraphNode::Perlin { .. }
            | GraphNode::WhiteNoise { .. } => vec![],
            // Scalar math
            GraphNode::ScalarMath(_) => vec![("a", PinType::Number), ("b", PinType::Number)],
            GraphNode::Trig(_) => vec![("x", PinType::Number)],
            GraphNode::Compare(_) => vec![("a", PinType::Number), ("b", PinType::Number)],
            GraphNode::Mix => vec![
                ("a", PinType::Number),
                ("b", PinType::Number),
                ("t", PinType::Number),
            ],
            GraphNode::Clamp => vec![
                ("x", PinType::Number),
                ("min", PinType::Number),
                ("max", PinType::Number),
            ],
            GraphNode::MapRange => vec![
                ("x", PinType::Number),
                ("from min", PinType::Number),
                ("from max", PinType::Number),
                ("to min", PinType::Number),
                ("to max", PinType::Number),
            ],
            GraphNode::Smoothstep => vec![
                ("edge0", PinType::Number),
                ("edge1", PinType::Number),
                ("x", PinType::Number),
            ],
            GraphNode::Step => vec![("edge", PinType::Number), ("x", PinType::Number)],
            // Vector
            GraphNode::VectorMath(_) => vec![("a", PinType::Vector), ("b", PinType::Vector)],
            GraphNode::Compose => vec![
                ("x", PinType::Number),
                ("y", PinType::Number),
                ("z", PinType::Number),
            ],
            GraphNode::Decompose => vec![("v", PinType::Vector)],
            GraphNode::Length => vec![("v", PinType::Vector)],
            GraphNode::Dot => vec![("a", PinType::Vector), ("b", PinType::Vector)],
            GraphNode::Distance => vec![("a", PinType::Vector), ("b", PinType::Vector)],
            GraphNode::Normalize => vec![("v", PinType::Vector)],
            GraphNode::VectorRotate => vec![
                ("v", PinType::Vector),
                ("axis", PinType::Vector),
                ("angle", PinType::Number),
            ],
            GraphNode::Reflect => vec![("v", PinType::Vector), ("n", PinType::Vector)],
            // Colour
            GraphNode::RgbToColor => vec![
                ("r", PinType::Number),
                ("g", PinType::Number),
                ("b", PinType::Number),
            ],
            GraphNode::HsvToColor => vec![
                ("h", PinType::Number),
                ("s", PinType::Number),
                ("v", PinType::Number),
            ],
            GraphNode::ColorMix => vec![
                ("a", PinType::Color),
                ("b", PinType::Color),
                ("t", PinType::Number),
            ],
            GraphNode::HueShift => vec![("c", PinType::Color), ("shift", PinType::Number)],
            GraphNode::ColorInvert => vec![("c", PinType::Color)],
            GraphNode::BrightContrast => vec![
                ("c", PinType::Color),
                ("bright", PinType::Number),
                ("contrast", PinType::Number),
            ],
            GraphNode::Gamma => vec![("c", PinType::Color), ("γ", PinType::Number)],
            // Logic
            GraphNode::IfElse => vec![
                ("cond", PinType::Bool),
                ("then", PinType::Number),
                ("else", PinType::Number),
            ],
            GraphNode::BooleanMath(_) => vec![("a", PinType::Bool), ("b", PinType::Bool)],
            GraphNode::FloatToBool => vec![("x", PinType::Number), ("threshold", PinType::Number)],
            GraphNode::BoolToFloat => vec![("b", PinType::Bool)],
            // Noise / wave
            GraphNode::Wave(_) => vec![("t", PinType::Number)],
            // Sinks
            GraphNode::Display | GraphNode::Plot | GraphNode::PlotXY => {
                vec![("x", PinType::Number)]
            }
            GraphNode::Preview => vec![("c", PinType::Color)],
            GraphNode::VectorPreview => vec![("v", PinType::Vector)],
            GraphNode::NoiseImage { .. } => vec![("uv offset", PinType::Number)],
            // NoiseField — every parameter exposed as a wireable
            // input pin (UE-style), no body sliders. Compose
            // your noise with Number / math nodes.
            GraphNode::NoiseField => vec![
                ("seed", PinType::Number),
                ("offset x", PinType::Number),
                ("offset y", PinType::Number),
                ("freq", PinType::Number),
                ("octaves", PinType::Number),
                ("persistence", PinType::Number),
                ("lacunarity", PinType::Number),
                ("gain", PinType::Number),
            ],
            GraphNode::MultiPlot => vec![
                ("a", PinType::Number),
                ("b", PinType::Number),
                ("c", PinType::Number),
                ("d", PinType::Number),
            ],
            // Output accepts ANY type — its body auto-detects.
            GraphNode::Output => vec![("any", PinType::Number)],
        }
    }

    fn outputs(&self) -> Vec<(&'static str, PinType)> {
        match self {
            GraphNode::Subgraph => vec![],
            GraphNode::Port { out: true, .. } => vec![],
            GraphNode::Port { out: false, .. } => vec![("", PinType::Number)],
            // Sources
            GraphNode::Number(_) | GraphNode::Integer(_) => vec![("", PinType::Number)],
            GraphNode::Vector(_) => vec![("", PinType::Vector)],
            GraphNode::Color(_) => vec![("", PinType::Color)],
            GraphNode::Bool(_) => vec![("", PinType::Bool)],
            GraphNode::Time => vec![("t", PinType::Number)],
            // Scalar-output families
            GraphNode::ScalarMath(_)
            | GraphNode::Trig(_)
            | GraphNode::Mix
            | GraphNode::Clamp
            | GraphNode::MapRange
            | GraphNode::Smoothstep
            | GraphNode::Step
            | GraphNode::Length
            | GraphNode::Dot
            | GraphNode::Distance
            | GraphNode::IfElse
            | GraphNode::BoolToFloat
            | GraphNode::Perlin { .. }
            | GraphNode::WhiteNoise { .. }
            | GraphNode::Wave(_) => vec![("", PinType::Number)],
            // Bool-output families
            GraphNode::Compare(_) | GraphNode::BooleanMath(_) | GraphNode::FloatToBool => {
                vec![("", PinType::Bool)]
            }
            // Vec-output families
            GraphNode::VectorMath(_)
            | GraphNode::Compose
            | GraphNode::Normalize
            | GraphNode::VectorRotate
            | GraphNode::Reflect => vec![("", PinType::Vector)],
            // Colour-output families
            GraphNode::RgbToColor
            | GraphNode::HsvToColor
            | GraphNode::ColorMix
            | GraphNode::HueShift
            | GraphNode::ColorInvert
            | GraphNode::BrightContrast
            | GraphNode::Gamma => vec![("", PinType::Color)],
            // Decompose
            GraphNode::Decompose => vec![
                ("x", PinType::Number),
                ("y", PinType::Number),
                ("z", PinType::Number),
            ],
            // Sinks
            GraphNode::Display
            | GraphNode::Plot
            | GraphNode::PlotXY
            | GraphNode::Preview
            | GraphNode::VectorPreview
            | GraphNode::NoiseImage { .. }
            | GraphNode::NoiseField
            | GraphNode::MultiPlot
            | GraphNode::Output => vec![],
        }
    }
}

/// Recursively evaluate the value flowing OUT of `pin`. `time` is
/// the seconds-since-startup value the `Time` node emits. Each
/// pull walks the upstream subtree once — fine for graphs of
/// hundreds of nodes; if you chain thousands you'd add a memo.
fn eval_output(graph: &Graph<GraphNode>, time: f64, pin: &OutPin) -> Value {
    let Some(node) = graph.get_node(pin.id.node) else {
        return Value::Number(0.0);
    };
    match node {
        // Structure, not computation: a subgraph is not evaluated by
        // the demo, and a boundary node forwards nothing.
        GraphNode::Subgraph | GraphNode::Port { .. } => Value::Number(0.0),
        GraphNode::Number(v) => Value::Number(*v),
        GraphNode::Integer(i) => Value::Number(*i as f64),
        GraphNode::Vector(v) => Value::Vector(*v),
        GraphNode::Color(c) => Value::Color(*c),
        GraphNode::Bool(b) => Value::Bool(*b),
        GraphNode::Time => Value::Number(time),
        GraphNode::ScalarMath(op) => {
            let a = eval_input_at(graph, time, pin.id.node, 0).as_number();
            let b = eval_input_at(graph, time, pin.id.node, 1).as_number();
            Value::Number(op.apply(a, b))
        }
        GraphNode::Trig(f) => {
            let x = eval_input_at(graph, time, pin.id.node, 0).as_number();
            Value::Number(f.apply(x))
        }
        GraphNode::Compare(op) => {
            let a = eval_input_at(graph, time, pin.id.node, 0).as_number();
            let b = eval_input_at(graph, time, pin.id.node, 1).as_number();
            Value::Bool(op.apply(a, b))
        }
        GraphNode::Mix => {
            let a = eval_input_at(graph, time, pin.id.node, 0).as_number();
            let b = eval_input_at(graph, time, pin.id.node, 1).as_number();
            let t = eval_input_at(graph, time, pin.id.node, 2)
                .as_number()
                .clamp(0.0, 1.0);
            Value::Number(a + (b - a) * t)
        }
        GraphNode::Clamp => {
            let x = eval_input_at(graph, time, pin.id.node, 0).as_number();
            let lo = eval_input_at(graph, time, pin.id.node, 1).as_number();
            let hi = eval_input_at(graph, time, pin.id.node, 2).as_number();
            Value::Number(x.clamp(lo.min(hi), lo.max(hi)))
        }
        GraphNode::VectorMath(op) => {
            let a = eval_input_at(graph, time, pin.id.node, 0).as_vector();
            let b = eval_input_at(graph, time, pin.id.node, 1).as_vector();
            Value::Vector(op.apply(a, b))
        }
        GraphNode::Compose => {
            let x = eval_input_at(graph, time, pin.id.node, 0).as_number();
            let y = eval_input_at(graph, time, pin.id.node, 1).as_number();
            let z = eval_input_at(graph, time, pin.id.node, 2).as_number();
            Value::Vector([x, y, z])
        }
        GraphNode::Decompose => {
            let v = eval_input_at(graph, time, pin.id.node, 0).as_vector();
            Value::Number(v[pin.id.output.min(2)])
        }
        GraphNode::Length => {
            let v = eval_input_at(graph, time, pin.id.node, 0).as_vector();
            Value::Number((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt())
        }
        GraphNode::RgbToColor => {
            let r = eval_input_at(graph, time, pin.id.node, 0).as_number();
            let g = eval_input_at(graph, time, pin.id.node, 1).as_number();
            let b = eval_input_at(graph, time, pin.id.node, 2).as_number();
            let to_u8 = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            Value::Color(MaraColor32::from_rgb(to_u8(r), to_u8(g), to_u8(b)))
        }
        GraphNode::ColorMix => {
            let a = eval_input_at(graph, time, pin.id.node, 0).as_color();
            let b = eval_input_at(graph, time, pin.id.node, 1).as_color();
            let t = eval_input_at(graph, time, pin.id.node, 2)
                .as_number()
                .clamp(0.0, 1.0) as f32;
            let lerp = |x: u8, y: u8| (x as f32 * (1.0 - t) + y as f32 * t).round() as u8;
            Value::Color(MaraColor32::from_rgba_unmultiplied(
                lerp(a.r(), b.r()),
                lerp(a.g(), b.g()),
                lerp(a.b(), b.b()),
                lerp(a.a(), b.a()),
            ))
        }
        GraphNode::IfElse => {
            let cond = eval_input_at(graph, time, pin.id.node, 0).as_bool();
            let then = eval_input_at(graph, time, pin.id.node, 1).as_number();
            let elze = eval_input_at(graph, time, pin.id.node, 2).as_number();
            Value::Number(if cond { then } else { elze })
        }
        // ── New scalar nodes ──
        GraphNode::MapRange => {
            let x = eval_input_at(graph, time, pin.id.node, 0).as_number();
            let lo = eval_input_at(graph, time, pin.id.node, 1).as_number();
            let hi = eval_input_at(graph, time, pin.id.node, 2).as_number();
            let olo = eval_input_at(graph, time, pin.id.node, 3).as_number();
            let ohi = eval_input_at(graph, time, pin.id.node, 4).as_number();
            let span = hi - lo;
            if span.abs() < 1e-9 {
                Value::Number(olo)
            } else {
                let t = ((x - lo) / span).clamp(0.0, 1.0);
                Value::Number(olo + (ohi - olo) * t)
            }
        }
        GraphNode::Smoothstep => {
            let e0 = eval_input_at(graph, time, pin.id.node, 0).as_number();
            let e1 = eval_input_at(graph, time, pin.id.node, 1).as_number();
            let x = eval_input_at(graph, time, pin.id.node, 2).as_number();
            let span = e1 - e0;
            let t = if span.abs() < 1e-9 {
                0.0
            } else {
                ((x - e0) / span).clamp(0.0, 1.0)
            };
            Value::Number(t * t * (3.0 - 2.0 * t))
        }
        GraphNode::Step => {
            let edge = eval_input_at(graph, time, pin.id.node, 0).as_number();
            let x = eval_input_at(graph, time, pin.id.node, 1).as_number();
            Value::Number(if x < edge { 0.0 } else { 1.0 })
        }
        // ── New vector nodes ──
        GraphNode::Dot => {
            let a = eval_input_at(graph, time, pin.id.node, 0).as_vector();
            let b = eval_input_at(graph, time, pin.id.node, 1).as_vector();
            Value::Number(a[0] * b[0] + a[1] * b[1] + a[2] * b[2])
        }
        GraphNode::Distance => {
            let a = eval_input_at(graph, time, pin.id.node, 0).as_vector();
            let b = eval_input_at(graph, time, pin.id.node, 1).as_vector();
            let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
            Value::Number((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt())
        }
        GraphNode::Normalize => {
            let v = eval_input_at(graph, time, pin.id.node, 0).as_vector();
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            if len < 1e-9 {
                Value::Vector([0.0; 3])
            } else {
                Value::Vector([v[0] / len, v[1] / len, v[2] / len])
            }
        }
        GraphNode::VectorRotate => {
            let v = eval_input_at(graph, time, pin.id.node, 0).as_vector();
            let mut axis = eval_input_at(graph, time, pin.id.node, 1).as_vector();
            let angle = eval_input_at(graph, time, pin.id.node, 2).as_number();
            let alen = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
            if alen < 1e-9 {
                return Value::Vector(v);
            }
            axis = [axis[0] / alen, axis[1] / alen, axis[2] / alen];
            let (s, c) = (angle.sin(), angle.cos());
            let dot = axis[0] * v[0] + axis[1] * v[1] + axis[2] * v[2];
            let cross = [
                axis[1] * v[2] - axis[2] * v[1],
                axis[2] * v[0] - axis[0] * v[2],
                axis[0] * v[1] - axis[1] * v[0],
            ];
            Value::Vector([
                v[0] * c + cross[0] * s + axis[0] * dot * (1.0 - c),
                v[1] * c + cross[1] * s + axis[1] * dot * (1.0 - c),
                v[2] * c + cross[2] * s + axis[2] * dot * (1.0 - c),
            ])
        }
        GraphNode::Reflect => {
            let v = eval_input_at(graph, time, pin.id.node, 0).as_vector();
            let mut n = eval_input_at(graph, time, pin.id.node, 1).as_vector();
            let nlen = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if nlen > 1e-9 {
                n = [n[0] / nlen, n[1] / nlen, n[2] / nlen];
            }
            let d = 2.0 * (v[0] * n[0] + v[1] * n[1] + v[2] * n[2]);
            Value::Vector([v[0] - d * n[0], v[1] - d * n[1], v[2] - d * n[2]])
        }
        // ── New colour nodes ──
        GraphNode::HsvToColor => {
            let h = eval_input_at(graph, time, pin.id.node, 0)
                .as_number()
                .rem_euclid(1.0);
            let s = eval_input_at(graph, time, pin.id.node, 1)
                .as_number()
                .clamp(0.0, 1.0);
            let v = eval_input_at(graph, time, pin.id.node, 2)
                .as_number()
                .clamp(0.0, 1.0);
            let (r, g, b) = hsv_to_rgb(h, s, v);
            let to_u8 = |x: f64| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
            Value::Color(MaraColor32::from_rgb(to_u8(r), to_u8(g), to_u8(b)))
        }
        GraphNode::HueShift => {
            let c = eval_input_at(graph, time, pin.id.node, 0).as_color();
            let shift = eval_input_at(graph, time, pin.id.node, 1).as_number();
            let (mut h, s, v) = rgb_to_hsv(
                c.r() as f64 / 255.0,
                c.g() as f64 / 255.0,
                c.b() as f64 / 255.0,
            );
            h = (h + shift).rem_euclid(1.0);
            let (r, g, b) = hsv_to_rgb(h, s, v);
            let to_u8 = |x: f64| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
            Value::Color(MaraColor32::from_rgba_unmultiplied(
                to_u8(r),
                to_u8(g),
                to_u8(b),
                c.a(),
            ))
        }
        GraphNode::ColorInvert => {
            let c = eval_input_at(graph, time, pin.id.node, 0).as_color();
            Value::Color(MaraColor32::from_rgba_unmultiplied(
                255 - c.r(),
                255 - c.g(),
                255 - c.b(),
                c.a(),
            ))
        }
        GraphNode::BrightContrast => {
            let c = eval_input_at(graph, time, pin.id.node, 0).as_color();
            let bright = eval_input_at(graph, time, pin.id.node, 1).as_number();
            let contrast = eval_input_at(graph, time, pin.id.node, 2).as_number();
            let adjust = |x: f64| ((x - 0.5) * (1.0 + contrast) + 0.5 + bright).clamp(0.0, 1.0);
            let to_u8 = |x: f64| (x * 255.0).round() as u8;
            Value::Color(MaraColor32::from_rgba_unmultiplied(
                to_u8(adjust(c.r() as f64 / 255.0)),
                to_u8(adjust(c.g() as f64 / 255.0)),
                to_u8(adjust(c.b() as f64 / 255.0)),
                c.a(),
            ))
        }
        GraphNode::Gamma => {
            let c = eval_input_at(graph, time, pin.id.node, 0).as_color();
            let g = eval_input_at(graph, time, pin.id.node, 1)
                .as_number()
                .max(0.01);
            let to_u8 = |x: u8| ((x as f64 / 255.0).powf(g).clamp(0.0, 1.0) * 255.0).round() as u8;
            Value::Color(MaraColor32::from_rgba_unmultiplied(
                to_u8(c.r()),
                to_u8(c.g()),
                to_u8(c.b()),
                c.a(),
            ))
        }
        // ── New logic nodes ──
        GraphNode::BooleanMath(op) => {
            let a = eval_input_at(graph, time, pin.id.node, 0).as_bool();
            let b = eval_input_at(graph, time, pin.id.node, 1).as_bool();
            Value::Bool(op.apply(a, b))
        }
        GraphNode::FloatToBool => {
            let x = eval_input_at(graph, time, pin.id.node, 0).as_number();
            let t = eval_input_at(graph, time, pin.id.node, 1).as_number();
            Value::Bool(x > t)
        }
        GraphNode::BoolToFloat => {
            let b = eval_input_at(graph, time, pin.id.node, 0).as_bool();
            Value::Number(if b { 1.0 } else { 0.0 })
        }
        // ── New noise / wave ──
        GraphNode::WhiteNoise { seed } => {
            let i = (time * 1000.0).floor() as i64 as u32;
            let mut x = i.wrapping_mul(0x9E3779B1).wrapping_add(*seed);
            x = (x ^ (x >> 16)).wrapping_mul(0x85EBCA6B);
            x = (x ^ (x >> 13)).wrapping_mul(0xC2B2AE35);
            Value::Number(((x ^ (x >> 16)) as f64 / u32::MAX as f64) * 2.0 - 1.0)
        }
        GraphNode::Wave(shape) => {
            let t = eval_input_at(graph, time, pin.id.node, 0).as_number();
            Value::Number(shape.apply(t))
        }
        GraphNode::Perlin { seed, frequency } => {
            // Very compact 1-D value noise — enough to look organic
            // when fed `Time`. Smoothed via a cubic
            // (`smoothstep` of the fractional offset) so output is
            // C¹-continuous at integer boundaries.
            let t = time * *frequency;
            let i = t.floor();
            let f = t - i;
            let h = |k: f64| {
                let k = (k as i64) as u32;
                let mut x = k.wrapping_mul(0x27d4eb2d).wrapping_add(*seed);
                x = (x ^ (x >> 15)).wrapping_mul(0x85ebca6b);
                x = (x ^ (x >> 13)).wrapping_mul(0xc2b2ae35);
                ((x ^ (x >> 16)) as f64 / u32::MAX as f64) * 2.0 - 1.0
            };
            let s = f * f * (3.0 - 2.0 * f);
            Value::Number(h(i) * (1.0 - s) + h(i + 1.0) * s)
        }
        GraphNode::Display
        | GraphNode::Plot
        | GraphNode::PlotXY
        | GraphNode::Preview
        | GraphNode::VectorPreview
        | GraphNode::NoiseImage { .. }
        | GraphNode::NoiseField
        | GraphNode::MultiPlot
        | GraphNode::Output => {
            Value::Number(0.0) // sinks have no outputs but be safe
        }
    }
}

/// 2-D value noise sampled at `(x, y)` with `seed`, returns
/// `0..1`. Smoothed via a cubic `smoothstep` so the image is
/// C¹-continuous at integer cell boundaries — the same kernel
/// the existing 1-D `Perlin` node uses, lifted to two axes.
/// Used by the `NoiseImage` body widget to fill a 96 × 64 px
/// image preview à la `noise_gui`.
fn sample_2d_value_noise(seed: u32, x: f64, y: f64) -> f64 {
    let hash = |i: i64, j: i64| -> f64 {
        let mut k = (i as u32).wrapping_mul(0x27d4eb2d).wrapping_add(seed);
        k = (k ^ (k >> 15)).wrapping_mul(0x85ebca6b);
        k = (k ^ (k >> 13)).wrapping_mul(0xc2b2ae35);
        k = k.wrapping_add((j as u32).wrapping_mul(0x9E3779B1));
        k = (k ^ (k >> 16)).wrapping_mul(0x85ebca6b);
        k = (k ^ (k >> 13)).wrapping_mul(0xc2b2ae35);
        (k ^ (k >> 16)) as f64 / u32::MAX as f64
    };
    let xi = x.floor();
    let yi = y.floor();
    let xf = x - xi;
    let yf = y - yi;
    let i = xi as i64;
    let j = yi as i64;
    let s00 = hash(i, j);
    let s10 = hash(i + 1, j);
    let s01 = hash(i, j + 1);
    let s11 = hash(i + 1, j + 1);
    let smoothstep = |t: f64| t * t * (3.0 - 2.0 * t);
    let u = smoothstep(xf);
    let v = smoothstep(yf);
    let a = s00 * (1.0 - u) + s10 * u;
    let b = s01 * (1.0 - u) + s11 * u;
    a * (1.0 - v) + b * v
}

/// Standard HSV → RGB. h, s, v all in 0..1. Returns components in 0..1.
fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    let i = (h * 6.0).floor() as i32;
    let f = h * 6.0 - i as f64;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    match i.rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// Standard RGB → HSV. Components in 0..1.
fn rgb_to_hsv(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let v = max;
    let s = if max < 1e-9 { 0.0 } else { d / max };
    let h = if d < 1e-9 {
        0.0
    } else if (max - r).abs() < 1e-9 {
        ((g - b) / d).rem_euclid(6.0)
    } else if (max - g).abs() < 1e-9 {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    ((h / 6.0).rem_euclid(1.0), s, v)
}

/// First connected upstream value at `(node, input)`, defaulted to
/// `Number(0.0)` when nothing is wired.
fn eval_input_at(
    graph: &Graph<GraphNode>,
    time: f64,
    node: mara::extras::graph::NodeId,
    input: usize,
) -> Value {
    let in_pin = graph.in_pin(InPinId { node, input });
    in_pin
        .remotes
        .first()
        .map(|r| eval_output(graph, time, &graph.out_pin(*r)))
        .unwrap_or(Value::Number(0.0))
}

#[allow(dead_code)] // Sibling of `eval_input_idx` — same shape but
// takes a borrowed `InPin` instead of an
// `(NodeId, usize)`. Kept for callers that
// already hold a pin reference.
fn eval_input(graph: &Graph<GraphNode>, time: f64, pin: &InPin) -> Value {
    pin.remotes
        .first()
        .map(|r| eval_output(graph, time, &graph.out_pin(*r)))
        .unwrap_or(Value::Number(0.0))
}

pub struct DemoViewer {
    /// Wall-clock seconds since startup, refreshed each frame by
    /// the editor pane. Threaded into `eval_output` so `Time` /
    /// `Perlin` nodes animate live.
    time: f64,
    /// What the document's definitions look like, refreshed each frame
    /// by whoever owns the document.
    ///
    /// Empty for the editor pane, which shows a bare `Graph` with no
    /// library — and an empty table is exactly the right answer there,
    /// because a graph with no definitions has no instances either.
    defs: std::collections::HashMap<mara::extras::graph::DefId, DefLook>,
    /// Width of the body area the renderer is currently offering, in
    /// screen points.
    ///
    /// Set per node, immediately before the body is drawn. Body widgets
    /// sized from a constant instead were either narrower than the node
    /// — leaving it looking mostly empty — or wider, and clipped to a
    /// stub at the edge, depending on the zoom.
    body_w: f32,
}

impl Default for DemoViewer {
    fn default() -> Self {
        Self {
            time: 0.0,
            defs: std::collections::HashMap::new(),
            body_w: BODY_W,
        }
    }
}

/// How one definition dresses its instances.
///
/// A viewer is handed one level of the document at a time, so an
/// instance node can otherwise only report what its payload knows —
/// which is nothing, since the payload was minted by the crate. The
/// name, the colour and the port labels all live in the library, and
/// this is the only channel that carries them to the node.
#[derive(Clone, Default)]
struct DefLook {
    name: String,
    /// The definition's own colour, which becomes the whole node's
    /// colour. Falls back to the `Subgraph` category tint for a
    /// definition the app never coloured.
    tint: MaraColor32,
    /// `shared` or `local`, shown as the instance's subtitle. Editing a
    /// shared definition changes every placement of it, and a user who
    /// cannot see which kind they are about to edit is about to be
    /// surprised.
    scope: &'static str,
    inputs: Vec<String>,
    outputs: Vec<String>,
}

/// How one node presents itself.
///
/// Resolved once and used for the header text, the header fill AND the
/// chrome accent, because three call sites that each looked the node up
/// and decided for themselves is how a chip ended up wearing its
/// definition's colour behind its payload's title.
struct NodeLook {
    title: String,
    subtitle: String,
    icon: &'static str,
    /// The node's single colour. Everything the crate tints per node —
    /// header fill, the deck of cards behind a chip, any accent bar a
    /// theme turns on — is handed this one value, so a node can never
    /// show two unrelated colours at rest.
    tint: MaraColor32,
}

/// Snapshot every definition's name, colour and port labels.
///
/// Rebuilt per frame rather than cached: renaming a port has to show on
/// every placement immediately, and invalidating a cache across the six
/// mutation paths that can change a definition is the bug this avoids.
/// The document is small enough that the clone is noise.
fn def_looks(
    doc: &mara::extras::graph::GraphDoc<GraphNode>,
) -> std::collections::HashMap<mara::extras::graph::DefId, DefLook> {
    let names = |ports: &[mara::extras::graph::PortDef]| -> Vec<String> {
        ports.iter().map(|p| p.name.clone()).collect()
    };
    doc.defs()
        .map(|(id, d)| {
            (
                id,
                DefLook {
                    name: d.name.clone(),
                    tint: d
                        .color
                        .unwrap_or_else(|| GraphNode::Subgraph.category().color()),
                    scope: match d.scope {
                        mara::extras::graph::DefScope::Shared => "shared",
                        _ => "local",
                    },
                    inputs: names(d.ports.inputs()),
                    outputs: names(d.ports.outputs()),
                },
            )
        })
        .collect()
}

impl DemoViewer {
    /// The definition a node instantiates, if it instantiates one.
    fn def_of(&self, node: NodeId, graph: &Graph<GraphNode>) -> Option<&DefLook> {
        let def = graph.instance_def(graph.uid_of(node)?)?;
        self.defs.get(&def)
    }

    /// Resolve a node's whole presentation in one place.
    ///
    /// Two payloads cannot answer for themselves and are patched here.
    /// A boundary node was minted by the crate and carries the port
    /// name it was handed. An instance IS its definition — same name,
    /// same colour on every placement — which is what makes two
    /// placements of one chip read as the same thing rather than as two
    /// unrelated nodes that happen to be next to each other.
    fn look(&self, node: NodeId, graph: &Graph<GraphNode>) -> NodeLook {
        let Some(payload) = graph.get_node(node) else {
            return NodeLook {
                title: String::new(),
                subtitle: String::new(),
                icon: "circle",
                tint: Category::Logic.color(),
            };
        };
        let mut look = NodeLook {
            title: payload.title().to_string(),
            subtitle: payload.subtitle(),
            icon: payload.icon_name(),
            tint: payload.category().color(),
        };
        if let GraphNode::Port { name, .. } = payload
            && !name.is_empty()
        {
            look.title = name.clone();
        }
        if let Some(def) = self.def_of(node, graph) {
            look.title = def.name.clone();
            look.subtitle = def.scope.to_string();
            look.tint = def.tint;
        }
        look
    }

    /// The label for one input pin of an instance node.
    ///
    /// An instance's pins ARE its definition's ports, so its payload
    /// cannot name them — unnamed pins on a node whose entire point is
    /// a named interface is the afterthought look this removes.
    fn instance_input(&self, node: NodeId, index: usize, graph: &Graph<GraphNode>) -> Option<&str> {
        Some(self.def_of(node, graph)?.inputs.get(index)?.as_str())
    }

    /// The label for one output pin of an instance node.
    fn instance_output(
        &self,
        node: NodeId,
        index: usize,
        graph: &Graph<GraphNode>,
    ) -> Option<&str> {
        Some(self.def_of(node, graph)?.outputs.get(index)?.as_str())
    }
}

impl DemoViewer {
    /// A viewer with no definition library, for a bare `Graph`.
    ///
    /// Public so the render snapshot can drive the *real* viewer rather
    /// than a stub — a stub gets the node frame right and the header
    /// band, pin colours and icons wrong, which is most of what a
    /// reviewer is trying to look at.
    #[must_use]
    pub fn for_graph(time: f64) -> Self {
        Self {
            time,
            defs: std::collections::HashMap::new(),
            body_w: BODY_W,
        }
    }
}

/// The rebuilt renderer's view of a demo node.
///
/// Every graph surface in this app goes through this one impl, so the
/// Editor pane and the Graph Lab cannot look like different widgets —
/// which is what they used to be, each assembling its own style.
impl mara::extras::graph::render::GraphView<GraphNode> for DemoViewer {
    fn shape(
        &mut self,
        id: NodeId,
        g: &Graph<GraphNode>,
    ) -> mara::extras::graph::render::NodeShape {
        let look = self.look(id, g);
        let def = self.def_of(id, g);
        let payload = g.get_node(id);
        let labels = |own: Vec<(&'static str, PinType)>, from_def: Option<&Vec<String>>| {
            match from_def {
                Some(names) => names.clone(),
                None => own.iter().map(|(l, _)| (*l).to_string()).collect(),
            }
        };
        mara::extras::graph::render::NodeShape {
            title: look.title,
            subtitle: look.subtitle,
            icon: Some(look.icon.to_string()),
            inputs: labels(
                payload.map(GraphNode::inputs).unwrap_or_default(),
                def.map(|d| &d.inputs),
            ),
            outputs: labels(
                payload.map(GraphNode::outputs).unwrap_or_default(),
                def.map(|d| &d.outputs),
            ),
            body_h: payload.map_or(0.0, GraphNode::body_height),
        }
    }

    fn tint(&mut self, id: NodeId, g: &Graph<GraphNode>) -> Option<MaraColor32> {
        Some(self.look(id, g).tint)
    }

    fn input_color(&mut self, pin: InPinId, g: &Graph<GraphNode>) -> Option<MaraColor32> {
        let n = g.get_node(pin.node)?;
        n.inputs().get(pin.input).map(|(_, t)| t.color())
    }

    fn output_color(&mut self, pin: OutPinId, g: &Graph<GraphNode>) -> Option<MaraColor32> {
        let n = g.get_node(pin.node)?;
        n.outputs().get(pin.output).map(|(_, t)| t.color())
    }

    /// The value editors, plots, swatches and noise previews that make
    /// this demo a widget gallery rather than a diagram. Delegates to
    /// the same `show_body` the previous renderer called, so nothing
    /// had to be rewritten to move renderers.
    fn body(
        &mut self,
        id: NodeId,
        rect: mara_core::vocab::Rect,
        ui: &mut mara_core::MaraUi<'_>,
        g: &mut Graph<GraphNode>,
    ) {
        self.body_w = rect.width();
        NodeViewer::show_body(self, id, &[], &[], ui, g);
    }

    /// The default-value editor on a disconnected input, the way the
    /// previous renderer put one in the pin row.
    fn input_editor(
        &mut self,
        pin: InPinId,
        _rect: mara_core::vocab::Rect,
        ui: &mut mara_core::MaraUi<'_>,
        g: &mut Graph<GraphNode>,
    ) {
        let Some((_, ty)) = g
            .get_node(pin.node)
            .and_then(|n| n.inputs().get(pin.input).copied())
        else {
            return;
        };
        let in_pin = g.in_pin(pin);
        inline_input_editor(g, &in_pin, ty, ui);
    }
}

impl NodeViewer<GraphNode> for DemoViewer {
    /// Mint the node `GraphDoc::collapse` places where the selection
    /// was. Without this the crate declines the edit, so `Ctrl+G` would
    /// silently do nothing.
    fn make_instance_node(
        &mut self,
        _def: mara::extras::graph::DefId,
        _name: &str,
        _ports: &mara::extras::graph::Ports,
    ) -> Option<GraphNode> {
        Some(GraphNode::Subgraph)
    }

    /// Mint one boundary node per derived port, inside the definition.
    fn make_port_node(&mut self, spec: &mara::extras::graph::PortSpec<'_>) -> Option<GraphNode> {
        Some(port_payload(spec))
    }

    fn title(&mut self, n: &GraphNode) -> String {
        n.title().into()
    }
    fn inputs(&mut self, n: &GraphNode) -> usize {
        n.inputs().len()
    }
    fn outputs(&mut self, n: &GraphNode) -> usize {
        n.outputs().len()
    }

    /// Per-node `header_frame` override — paints the Blender
    /// category tint as a SOLID full-width fill on the title bar,
    /// with the top-only rounded corners that match the node's
    /// outline. This is how Blender draws node headers (a flat
    /// colour strip the full width of the node, NOT a UE-style
    /// "spill" gradient). The frame's `fill` is the category
    /// colour at full alpha; the body underneath stays neutral
    /// dark.
    fn header_frame(
        &mut self,
        default: mara_core::style::FrameSpec,
        node: mara::extras::graph::NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        graph: &Graph<GraphNode>,
    ) -> mara_core::style::FrameSpec {
        if graph.get_node(node).is_none() {
            return default;
        }
        let tint = self.look(node, graph).tint;
        // Translucent tint — the dark body fill below shows
        // through, knocking the saturation down so the seven
        // category colours all sit at a consistent luminance the
        // way Blender's `node_class` palette does. Alpha 0xB0
        // (~69 %) lands roughly where Blender's headers sit
        // visually against the `#303030` body.
        let mut frame = default;
        frame.fill =
            mara_core::vocab::Color32::from_rgba_unmultiplied(tint.r(), tint.g(), tint.b(), 0xB0);
        frame.stroke = mara_core::vocab::Stroke::NONE;
        frame
    }

    /// The node's chrome — one explicit colour, and nothing else.
    ///
    /// `accent` is deliberately `Some` rather than left to the default.
    /// A `None` here means "whatever the host accent happens to be",
    /// and for a subgraph instance the crate helpfully substitutes the
    /// definition's colour — either way the node's rings and any accent
    /// bar a theme turns on would be painted in a colour that has
    /// nothing to do with the header band this viewer just painted.
    /// Handing back the header's own tint collapses the two colour
    /// systems into one, which is the whole reason the header carries
    /// category colour in this demo.
    fn node_chrome(
        &mut self,
        node: mara::extras::graph::NodeId,
        _tier: mara::extras::graph::DetailTier,
        graph: &Graph<GraphNode>,
    ) -> mara::extras::graph::NodeChrome {
        mara::extras::graph::NodeChrome {
            accent: Some(self.look(node, graph).tint),
            ..mara::extras::graph::NodeChrome::lit()
        }
    }

    /// Two-line header content: [icon] [title / subtitle], laid
    /// out like an Unreal Blueprint title bar — main title in
    /// near-white, smaller dim subtitle underneath, and a
    /// category-coloured Fluent-UI icon glyph on the left so the
    /// node is identifiable at a glance even when zoomed out.
    fn show_header(
        &mut self,
        node: mara::extras::graph::NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut mara_core::MaraUi<'_>,
        graph: &mut Graph<GraphNode>,
    ) {
        if graph.get_node(node).is_none() {
            return;
        }
        let look = self.look(node, graph);
        let title_color = MaraColor32::from_rgb(0xEE, 0xEE, 0xEE);
        let subtitle_color = MaraColor32::from_rgba_unmultiplied(0xEE, 0xEE, 0xEE, 0xB0);

        // No header width clamp — the title bar sizes to its natural
        // content width, so the node ends up
        // `max(title_w, every_pin_row_w, body_w)`.
        ui.horizontal(|ui| {
            // Icon, sized to span both text rows so it centres against
            // the title+subtitle stack.
            let glyph = mara_core::icons::icon_glyph(look.icon)
                .map_or_else(|| "\u{2022}".to_owned(), |(glyph, _)| glyph.to_string());
            ui.label_spec(&glyph, &LabelSpec::new(22.0, title_color).truncate(false));
            ui.vertical(|ui| {
                ui.label_spec(
                    &look.title,
                    &LabelSpec::new(13.0, title_color).truncate(false),
                );
                ui.label_spec(
                    &look.subtitle,
                    &LabelSpec::new(10.0, subtitle_color).truncate(false),
                );
            });
        });
    }

    fn show_input(
        &mut self,
        pin: &InPin,
        ui: &mut mara_core::MaraUi<'_>,
        graph: &mut Graph<GraphNode>,
    ) -> impl NodePin + 'static {
        // UE Blueprint pin row:
        //   * Disconnected → `[pin glyph] [label] [default value]`.
        //   * Connected    → `[pin glyph] [label]` (no live value
        //     readout — UE never shows in-flight values on a
        //     connected pin; debugging is via watch / tooltip).
        let _ = self.time;
        let (label, ty) = graph
            .get_node(pin.id.node)
            .and_then(|n| n.inputs().get(pin.id.input).copied())
            .unwrap_or(("", PinType::Number));
        let label = self
            .instance_input(pin.id.node, pin.id.input, graph)
            .unwrap_or(label);
        ui.label(label);
        let connected = !pin.remotes.is_empty();
        if !connected {
            inline_input_editor(graph, pin, ty, ui);
        }
        ty.pin(connected)
    }

    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut mara_core::MaraUi<'_>,
        graph: &mut Graph<GraphNode>,
    ) -> impl NodePin + 'static {
        // UE Blueprint output row: just `[label] [pin glyph]`.
        // **No live value readout**, **no value editor** — UE
        // never shows in-flight values on output pins. Source
        // nodes (Number / Vector / Color / Bool) host their
        // editor in the body instead (see `show_body` below),
        // matching `Make Vector` / `Make LinearColor`.
        let connected = !pin.remotes.is_empty();
        let (label, ty) = graph
            .get_node(pin.id.node)
            .map(|n| n.outputs())
            .and_then(|os| os.get(pin.id.output).copied())
            .unwrap_or(("", PinType::Number));
        let label = self
            .instance_output(pin.id.node, pin.id.output, graph)
            .unwrap_or(label);
        if !label.is_empty() {
            ui.label(label);
        }
        ty.pin(connected)
    }

    fn has_body(&mut self, node: &GraphNode) -> bool {
        matches!(
            node,
            // Source nodes host their value editor in the body
            // (UE-style `Make Vector` / `Make LinearColor`).
            GraphNode::Number(_) | GraphNode::Integer(_)
                | GraphNode::Vector(_) | GraphNode::Color(_)
                | GraphNode::Bool(_)
                // Op-dropdown nodes
                | GraphNode::ScalarMath(_) | GraphNode::Trig(_) | GraphNode::Compare(_)
                | GraphNode::VectorMath(_) | GraphNode::BooleanMath(_)
                | GraphNode::Wave(_)
                | GraphNode::Perlin { .. } | GraphNode::WhiteNoise { .. }
                // Sinks
                | GraphNode::Display | GraphNode::Plot
                | GraphNode::PlotXY
                | GraphNode::Preview | GraphNode::VectorPreview
                | GraphNode::NoiseImage { .. }
                | GraphNode::NoiseField
                | GraphNode::MultiPlot
                | GraphNode::Output
        )
    }

    fn show_body(
        &mut self,
        node: mara::extras::graph::NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut mara_core::MaraUi<'_>,
        graph: &mut Graph<GraphNode>,
    ) {
        // No body width clamp — body sizes to its content like
        // UE Slate's natural measurement. Inline widgets below
        // are wrapped in fixed-width slots so the body can't
        // grow per-frame when a value changes.

        let time = self.time;
        let body_w = self.body_w;
        let Some(n) = graph.get_node_mut(node) else {
            return;
        };
        match n {
            // ── Source-node value editors (UE: Make-* nodes) ──
            GraphNode::Number(v) => {
                let h = mara_core::widget::drag_value::DRAG_VALUE_ROW_H;
                ui.row(
                    MaraVec2::new(body_w, h),
                    mara_core::CrossAlign::Center,
                    |ui| {
                        ui.drag_value("", v, 0.05, f64::MIN..=f64::MAX, 2, "");
                    },
                );
            }
            GraphNode::Integer(i) => {
                let h = mara_core::widget::drag_value::DRAG_VALUE_ROW_H;
                let mut tmp = *i as f64;
                ui.row(
                    MaraVec2::new(body_w, h),
                    mara_core::CrossAlign::Center,
                    |ui| {
                        ui.drag_value("", &mut tmp, 1.0, f64::MIN..=f64::MAX, 0, "");
                    },
                );
                *i = tmp as i64;
            }
            GraphNode::Vector(v) => {
                let h = mara_core::widget::drag_value::DRAG_VALUE_ROW_H;
                for (axis, comp) in ["x", "y", "z"].iter().zip(v.iter_mut()) {
                    ui.row(
                        MaraVec2::new(body_w, h),
                        mara_core::CrossAlign::Center,
                        |ui| {
                            ui.drag_value(axis, comp, 0.05, f64::MIN..=f64::MAX, 2, "");
                        },
                    );
                }
            }
            GraphNode::Color(c) => {
                let mut rgba = [
                    f32::from(c.r()) / 255.0,
                    f32::from(c.g()) / 255.0,
                    f32::from(c.b()) / 255.0,
                    f32::from(c.a()) / 255.0,
                ];
                if ui.color_rgba("", &mut rgba).changed() {
                    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                    *c = MaraColor32::from_rgba_unmultiplied(
                        byte(rgba[0]),
                        byte(rgba[1]),
                        byte(rgba[2]),
                        byte(rgba[3]),
                    );
                }
            }
            GraphNode::Bool(b) => {
                let h = mara_core::widget::toggle::TOGGLE_ROW_H;
                ui.row(
                    MaraVec2::new(body_w, h),
                    mara_core::CrossAlign::Center,
                    |ui| {
                        ui.toggle("", b);
                    },
                );
            }
            GraphNode::ScalarMath(op) => {
                op_dropdown(
                    ui,
                    op,
                    &[
                        ("a + b", ScalarOp::Add),
                        ("a − b", ScalarOp::Sub),
                        ("a × b", ScalarOp::Mul),
                        ("a ÷ b", ScalarOp::Div),
                        ("min(a,b)", ScalarOp::Min),
                        ("max(a,b)", ScalarOp::Max),
                        ("a ^ b", ScalarOp::Pow),
                        ("a mod b", ScalarOp::Mod),
                        ("smin(a,b)", ScalarOp::SmoothMin),
                        ("smax(a,b)", ScalarOp::SmoothMax),
                    ],
                );
            }
            GraphNode::Trig(f) => {
                op_dropdown(
                    ui,
                    f,
                    &[
                        ("sin", TrigFn::Sin),
                        ("cos", TrigFn::Cos),
                        ("tan", TrigFn::Tan),
                        ("asin", TrigFn::Asin),
                        ("acos", TrigFn::Acos),
                        ("atan", TrigFn::Atan),
                        ("sinh", TrigFn::Sinh),
                        ("cosh", TrigFn::Cosh),
                        ("tanh", TrigFn::Tanh),
                        ("sqrt", TrigFn::Sqrt),
                        ("abs", TrigFn::Abs),
                        ("floor", TrigFn::Floor),
                        ("ceil", TrigFn::Ceil),
                        ("round", TrigFn::Round),
                        ("trunc", TrigFn::Trunc),
                        ("frac", TrigFn::Frac),
                        ("sign", TrigFn::Sign),
                        ("exp", TrigFn::Exp),
                        ("ln", TrigFn::Log),
                    ],
                );
            }
            GraphNode::Compare(op) => {
                op_dropdown(
                    ui,
                    op,
                    &[
                        ("a < b", CompareOp::Lt),
                        ("a ≤ b", CompareOp::Le),
                        ("a = b", CompareOp::Eq),
                        ("a ≠ b", CompareOp::Ne),
                        ("a ≥ b", CompareOp::Ge),
                        ("a > b", CompareOp::Gt),
                    ],
                );
            }
            GraphNode::VectorMath(op) => {
                op_dropdown(
                    ui,
                    op,
                    &[
                        ("a + b", VectorOp::Add),
                        ("a − b", VectorOp::Sub),
                        ("a ⊙ b (component)", VectorOp::Mul),
                        ("a × b (cross)", VectorOp::Cross),
                    ],
                );
            }
            GraphNode::BooleanMath(op) => {
                op_dropdown(
                    ui,
                    op,
                    &[
                        ("a ∧ b (and)", BoolOp::And),
                        ("a ∨ b (or)", BoolOp::Or),
                        ("¬a (not)", BoolOp::Not),
                        ("a ⊕ b (xor)", BoolOp::Xor),
                        ("a ⊼ b (nand)", BoolOp::Nand),
                        ("a ⊽ b (nor)", BoolOp::Nor),
                        ("a = b (xnor)", BoolOp::Xnor),
                    ],
                );
            }
            GraphNode::Wave(shape) => {
                op_dropdown(
                    ui,
                    shape,
                    &[
                        ("sine", WaveShape::Sine),
                        ("saw", WaveShape::Saw),
                        ("triangle", WaveShape::Triangle),
                        ("square", WaveShape::Square),
                    ],
                );
            }
            GraphNode::Perlin { seed, frequency } => {
                // Both widgets in a fixed 140-px slot so the
                // Perlin body can't grow horizontally with the
                // node — mara drag_value / slider both consume
                // `available_width`. Slider is 2 rows tall.
                const SLOT_W: f32 = 140.0;
                let drag_h = mara_core::widget::drag_value::DRAG_VALUE_ROW_H;
                let mut seed_f = *seed as f64;
                ui.row(
                    MaraVec2::new(SLOT_W, drag_h),
                    mara_core::CrossAlign::Center,
                    |ui| {
                        ui.drag_value("seed", &mut seed_f, 1.0, 0.0..=u32::MAX as f64, 0, "");
                    },
                );
                *seed = seed_f as u32;
                ui.row(
                    MaraVec2::new(SLOT_W, drag_h * 2.0),
                    mara_core::CrossAlign::Center,
                    |ui| {
                        ui.slider("freq", frequency, 0.05..=8.0, 2, "");
                    },
                );
            }
            GraphNode::WhiteNoise { seed } => {
                const SLOT_W: f32 = 140.0;
                let drag_h = mara_core::widget::drag_value::DRAG_VALUE_ROW_H;
                let mut seed_f = *seed as f64;
                ui.row(
                    MaraVec2::new(SLOT_W, drag_h),
                    mara_core::CrossAlign::Center,
                    |ui| {
                        ui.drag_value("seed", &mut seed_f, 1.0, 0.0..=u32::MAX as f64, 0, "");
                    },
                );
                *seed = seed_f as u32;
            }
            GraphNode::Display => {
                let v = eval_input_at(graph, time, node, 0).as_number();
                draw_sparkline(graph, node, ui, v);
            }
            GraphNode::Plot => {
                let v = eval_input_at(graph, time, node, 0).as_number();
                // Plot uses the same sparkline drawer but at a
                // larger size — separate visual identity vs Display.
                draw_sparkline(graph, node, ui, v);
            }
            // ── Sophisticated egui_plot line chart (HISTORY
            //    samples on the X axis, value on the Y axis with
            //    auto-fit, gridlines and axis labels). ──
            GraphNode::PlotXY => {
                // Line chart drawn from paint primitives rather than a
                // plotting crate: two demo nodes did not justify keeping
                // an egui-only widget in the sealed path (PLAN.md WS-D1.4).
                const HISTORY: usize = 256;
                let v = eval_input_at(graph, time, node, 0).as_number();
                let key = MaraId::new(("mara_demo_plotxy", node));
                let mut buf: Vec<f64> = ui.memory().get_temp::<Vec<f64>>(key).unwrap_or_default();
                if buf.len() >= HISTORY {
                    buf.remove(0);
                }
                buf.push(v);
                ui.memory().set_temp(key, buf.clone());

                let (painter, _) = ui.canvas(MaraVec2::new(body_w, 80.0));
                let rect = painter.clip_rect();
                paint_line_chart(
                    &painter,
                    rect,
                    &[(&buf, MaraColor32::from_rgb(0xA4, 0xFF, 0x34))],
                );
                ui.request_repaint();
            }
            GraphNode::Preview => {
                let c = eval_input_at(graph, time, node, 0).as_color();
                let (painter, __resp) = ui.canvas(MaraVec2::new(body_w, 40.0));
                let rect = __resp.rect;
                painter.rect_filled(rect, mara_core::vocab::CornerRadius::same(4), c);
                painter.rect_stroke(
                    rect,
                    mara_core::vocab::CornerRadius::same(4),
                    MaraStroke::new(1.0, MaraColor32::from_gray(80)),
                );
            }
            // ── Sophisticated 2-D noise image preview (à la
            //    `noise_gui`). 96 × 64 pixels of Perlin noise
            //    sampled with the node's seed + scale fields,
            //    optionally offset by an upstream `uv offset`
            //    scalar so the texture animates when fed Time.
            GraphNode::NoiseImage { seed, scale } => {
                const W: usize = 96;
                const H: usize = 64;
                let seed = *seed;
                let scale = *scale;
                let offset = eval_input_at(graph, time, node, 0).as_number();
                let key = MaraId::new(("mara_demo_noise_image", node));
                // Cache the previous frame's parameters so we
                // only regenerate the texture when something
                // actually changed (otherwise this would burn a
                // 96×64 hash + tessellator update every frame).
                let prev = ui.memory().get_temp::<(u32, u64, u64)>(key);
                let scale_bits = scale.to_bits();
                let offset_bits = offset.to_bits();
                let new_state = (seed, scale_bits, offset_bits);
                let needs_redraw = prev != Some(new_state);

                if needs_redraw {
                    let mut pixels = vec![MaraColor32::BLACK; W * H];
                    for j in 0..H {
                        for i in 0..W {
                            let x = (i as f64) * scale + offset;
                            let y = (j as f64) * scale;
                            let n = sample_2d_value_noise(seed, x, y);
                            // 0..1 → grey, then accent-tint it.
                            let g = (n * 255.0).clamp(0.0, 255.0) as u8;
                            pixels[j * W + i] = MaraColor32::from_rgb(
                                ((g as u16 * 0xA4) / 255) as u8,
                                ((g as u16 * 0xFF) / 255) as u8,
                                ((g as u16 * 0x34) / 255) as u8,
                            );
                        }
                    }
                    let img = mara_core::vocab::ColorImage::from_rgba_pixels([W, H], &pixels);
                    let tex = ui.load_texture(
                        &format!("mara_demo_noise_{:?}", node),
                        img,
                        mara_core::vocab::TextureOptions::NEAREST,
                    );
                    let tex_key = key.with("tex");
                    if let Some(tex) = tex {
                        ui.memory().set_temp(tex_key, tex);
                        ui.memory().set_temp(key, new_state);
                    }
                }
                let tex_key = key.with("tex");
                if let Some(tex) = ui
                    .memory()
                    .get_temp::<mara_core::vocab::TextureHandle>(tex_key)
                {
                    let size = MaraVec2::new(W as f32 * 1.5, H as f32 * 1.5);
                    let (painter, resp) = ui.canvas(size);
                    painter.image(
                        tex.id(),
                        resp.rect,
                        mara_core::MaraPainter::full_uv(),
                        MaraColor32::WHITE,
                    );
                }
            }
            // ── Output sink — displays the connected input's
            //    live value. Auto-detects the upstream pin's
            //    type so a Vector / Color / Bool feeding the
            //    sink shows the appropriate readout (numeric,
            //    swatch, etc.) — `Output` is the demo's
            //    universal "watch this value" node.
            GraphNode::Output => {
                let in_pin = graph.in_pin(InPinId { node, input: 0 });
                let v = if in_pin.remotes.is_empty() {
                    Value::Number(0.0)
                } else {
                    let r = in_pin.remotes[0];
                    eval_output(graph, time, &graph.out_pin(r))
                };
                let inferred_ty = match &v {
                    Value::Number(_) => PinType::Number,
                    Value::Vector(_) => PinType::Vector,
                    Value::Color(_) => PinType::Color,
                    Value::Bool(_) => PinType::Bool,
                    Value::Text(_) => PinType::Text,
                };
                ui.horizontal(|ui| {
                    ui.label_spec(
                        "=",
                        &LabelSpec::new(12.0, MaraColor32::from_gray(170))
                            .mono(true)
                            .truncate(false),
                    );
                    inline_value_readout(&v, inferred_ty, ui);
                });
            }
            // ── Sophisticated noise field — full FBM rig with
            //    sliders + larger image preview. Mirrors what
            //    `noise_gui` exposes: octaves / persistence /
            //    lacunarity / gain plus a mode selector for
            //    FBM, Turbulence, or Ridged. ──
            GraphNode::NoiseField => {
                const W: usize = 160;
                const H: usize = 96;
                // Read inputs FIRST (immutable borrow of graph).
                // Sensible defaults when a pin is disconnected:
                // we treat `Number(0)` (the eval default) as
                // "use this fallback" so the field renders
                // something even on a freshly-spawned node with
                // no wires. Connected non-zero inputs override.
                let seed_in = eval_input_at(graph, time, node, 0).as_number();
                let off_x = eval_input_at(graph, time, node, 1).as_number();
                let off_y = eval_input_at(graph, time, node, 2).as_number();
                let freq_in = eval_input_at(graph, time, node, 3).as_number();
                let oct_in = eval_input_at(graph, time, node, 4).as_number();
                let pers_in = eval_input_at(graph, time, node, 5).as_number();
                let lac_in = eval_input_at(graph, time, node, 6).as_number();
                let gain_in = eval_input_at(graph, time, node, 7).as_number();
                let seed = if seed_in.abs() < 1e-9 {
                    0xCAFE
                } else {
                    seed_in as u32
                };
                let freq = if freq_in.abs() < 1e-9 { 1.0 } else { freq_in };
                let octaves = if oct_in < 0.5 {
                    4u32
                } else {
                    oct_in.clamp(1.0, 8.0) as u32
                };
                let pers = if pers_in.abs() < 1e-9 {
                    0.5
                } else {
                    pers_in.clamp(0.0, 1.0)
                };
                let lac = if lac_in < 1.0 {
                    2.0
                } else {
                    lac_in.clamp(1.0, 4.0)
                };
                let gain = if gain_in.abs() < 1e-9 {
                    1.0
                } else {
                    gain_in.max(0.01)
                };

                // No body widgets — pure output. FBM is the
                // only mode; if you want Turbulence/Ridged in
                // future, add separate node types.
                let mode = NoiseMode::FBM;

                // Cache + render the image. Re-roll only when any
                // param changed — otherwise blit the texture.
                let key = MaraId::new(("mara_demo_noise_field", node));
                let new_state = (
                    seed,
                    octaves,
                    pers.to_bits(),
                    lac.to_bits(),
                    gain.to_bits(),
                    mode as u8,
                    off_x.to_bits(),
                    off_y.to_bits(),
                    freq.to_bits(),
                );
                let prev = ui
                    .memory()
                    .get_temp::<(u32, u32, u64, u64, u64, u8, u64, u64, u64)>(key);
                if prev != Some(new_state) {
                    let mut pixels = vec![MaraColor32::BLACK; W * H];
                    let scale = 0.04 * freq;
                    let g_pow = gain;
                    for j in 0..H {
                        for i in 0..W {
                            let x = (i as f64) * scale + off_x;
                            let y = (j as f64) * scale + off_y;
                            let n = mode.sample(seed, x, y, octaves, pers, lac);
                            let n = n.clamp(0.0, 1.0).powf(g_pow);
                            let g = (n * 255.0).clamp(0.0, 255.0) as u8;
                            pixels[j * W + i] = MaraColor32::from_rgb(
                                ((g as u16 * 0xA4) / 255) as u8,
                                ((g as u16 * 0xFF) / 255) as u8,
                                ((g as u16 * 0x34) / 255) as u8,
                            );
                        }
                    }
                    let img = mara_core::vocab::ColorImage::from_rgba_pixels([W, H], &pixels);
                    let tex = ui.load_texture(
                        &format!("mara_demo_noise_field_{:?}", node),
                        img,
                        mara_core::vocab::TextureOptions::NEAREST,
                    );
                    let tex_key = key.with("tex");
                    if let Some(tex) = tex {
                        ui.memory().set_temp(tex_key, tex);
                        ui.memory().set_temp(key, new_state);
                    }
                }
                let tex_key = key.with("tex");
                if let Some(tex) = ui
                    .memory()
                    .get_temp::<mara_core::vocab::TextureHandle>(tex_key)
                {
                    let size = MaraVec2::new(W as f32 * 1.4, H as f32 * 1.4);
                    let (painter, resp) = ui.canvas(size);
                    painter.image(
                        tex.id(),
                        resp.rect,
                        mara_core::MaraPainter::full_uv(),
                        MaraColor32::WHITE,
                    );
                }
            }
            // ── 4-channel oscilloscope plot — each input
            //    rendered as its own coloured line. ──
            GraphNode::MultiPlot => {
                const HISTORY: usize = 256;
                const COLORS: [MaraColor32; 4] = [
                    MaraColor32::from_rgb(0xA4, 0xFF, 0x34), // lime (Float)
                    MaraColor32::from_rgb(0xFF, 0xC2, 0x47), // gold (Vector)
                    MaraColor32::from_rgb(0xFF, 0xA0, 0xFF), // pink
                    MaraColor32::from_rgb(0x6E, 0xC0, 0xFF), // cyan
                ];
                let key = MaraId::new(("mara_demo_multiplot", node));
                let mut buf: Vec<[f64; 4]> = ui
                    .memory()
                    .get_temp::<Vec<[f64; 4]>>(key)
                    .unwrap_or_default();
                let sample = [
                    eval_input_at(graph, time, node, 0).as_number(),
                    eval_input_at(graph, time, node, 1).as_number(),
                    eval_input_at(graph, time, node, 2).as_number(),
                    eval_input_at(graph, time, node, 3).as_number(),
                ];
                if buf.len() >= HISTORY {
                    buf.remove(0);
                }
                buf.push(sample);
                ui.memory().set_temp(key, buf.clone());

                // Split the interleaved samples into one series per
                // channel, then share a range so the four stay comparable.
                let channels: Vec<Vec<f64>> = (0..4)
                    .map(|ch| buf.iter().map(|s| s[ch]).collect())
                    .collect();
                let series: Vec<(&[f64], MaraColor32)> = channels
                    .iter()
                    .zip(COLORS)
                    .map(|(values, color)| (values.as_slice(), color))
                    .collect();

                let (painter, _) = ui.canvas(MaraVec2::new(body_w, 92.0));
                let rect = painter.clip_rect();
                paint_line_chart(&painter, rect, &series);
                ui.request_repaint();
            }
            GraphNode::VectorPreview => {
                let v = eval_input_at(graph, time, node, 0).as_vector();
                let (painter, resp) = ui.canvas(MaraVec2::new(body_w, 40.0));
                let rect = resp.rect;
                painter.rect_filled(
                    rect,
                    mara_core::vocab::CornerRadius::same(3),
                    MaraColor32::from_black_alpha(40),
                );
                let bar_h = (rect.height() - 8.0) / 3.0;
                let max = v[0].abs().max(v[1].abs()).max(v[2].abs()).max(1.0) as f32;
                let colors = [
                    MaraColor32::from_rgb(0xFF, 0x33, 0x52), // x = red
                    MaraColor32::from_rgb(0x8B, 0xDC, 0x00), // y = green
                    MaraColor32::from_rgb(0x28, 0x90, 0xFF), // z = blue
                ];
                for (i, comp) in v.iter().enumerate() {
                    let y0 = rect.top() + 4.0 + (i as f32) * bar_h;
                    let centre_x = rect.center().x;
                    let len = (*comp as f32 / max) * (rect.width() * 0.5 - 8.0);
                    let bar_rect = mara_core::vocab::Rect::from_min_max(
                        MaraPos2::new(centre_x.min(centre_x + len), y0 + 2.0),
                        MaraPos2::new(centre_x.max(centre_x + len), y0 + bar_h - 2.0),
                    );
                    painter.rect_filled(
                        bar_rect,
                        mara_core::vocab::CornerRadius::same(1),
                        colors[i],
                    );
                }
                // Centre rule
                painter.line_segment(
                    MaraPos2::new(rect.center().x, rect.top() + 2.0),
                    MaraPos2::new(rect.center().x, rect.bottom() - 2.0),
                    MaraStroke::new(1.0, MaraColor32::from_gray(120)),
                );
            }
            _ => {}
        }
    }

    fn has_graph_menu(&mut self, _: MaraPos2, _: &mut Graph<GraphNode>) -> bool {
        true
    }
    fn show_graph_menu(
        &mut self,
        pos: MaraPos2,
        ui: &mut mara_core::MaraUi<'_>,
        graph: &mut Graph<GraphNode>,
    ) {
        ui.label_spec(
            "Add node",
            &LabelSpec::new(13.0, mara_core::style::on_panel()).truncate(false),
        );
        ui.separator();

        // Each submenu closes itself once an item spawns, so the menu
        // dismisses rather than staying open behind the new node.
        let mut spawn = |ui: &mut mara_core::MaraUi<'_>, menu: &str, label: &str, n: GraphNode| {
            if ui.button(label).clicked() {
                graph.insert_node(pos, n);
                ui.close_menu(MaraId::new(("demo_graph_menu", menu)));
            }
        };

        ui.menu_button(
            MaraId::new(("demo_graph_menu", "Sources")),
            "Sources",
            |ui| {
                spawn(ui, "Sources", "Number", GraphNode::Number(0.0));
                spawn(ui, "Sources", "Integer", GraphNode::Integer(0));
                spawn(ui, "Sources", "Vector", GraphNode::Vector([0.0; 3]));
                spawn(
                    ui,
                    "Sources",
                    "Color",
                    GraphNode::Color(MaraColor32::from_rgb(180, 200, 220)),
                );
                spawn(ui, "Sources", "Bool", GraphNode::Bool(false));
                spawn(ui, "Sources", "Time", GraphNode::Time);
            },
        );
        ui.menu_button(
            MaraId::new(("demo_graph_menu", "Scalar math")),
            "Scalar math",
            |ui| {
                spawn(
                    ui,
                    "Scalar math",
                    "Scalar Math",
                    GraphNode::ScalarMath(ScalarOp::Add),
                );
                spawn(ui, "Scalar math", "Math Func", GraphNode::Trig(TrigFn::Sin));
                spawn(
                    ui,
                    "Scalar math",
                    "Compare",
                    GraphNode::Compare(CompareOp::Lt),
                );
                spawn(ui, "Scalar math", "Mix", GraphNode::Mix);
                spawn(ui, "Scalar math", "Clamp", GraphNode::Clamp);
                spawn(ui, "Scalar math", "Map Range", GraphNode::MapRange);
                spawn(ui, "Scalar math", "Smoothstep", GraphNode::Smoothstep);
                spawn(ui, "Scalar math", "Step", GraphNode::Step);
            },
        );
        ui.menu_button(MaraId::new(("demo_graph_menu", "Vector")), "Vector", |ui| {
            spawn(
                ui,
                "Vector",
                "Vector Math",
                GraphNode::VectorMath(VectorOp::Add),
            );
            spawn(ui, "Vector", "Compose", GraphNode::Compose);
            spawn(ui, "Vector", "Decompose", GraphNode::Decompose);
            spawn(ui, "Vector", "Length", GraphNode::Length);
            spawn(ui, "Vector", "Dot Product", GraphNode::Dot);
            spawn(ui, "Vector", "Distance", GraphNode::Distance);
            spawn(ui, "Vector", "Normalize", GraphNode::Normalize);
            spawn(ui, "Vector", "Vector Rotate", GraphNode::VectorRotate);
            spawn(ui, "Vector", "Reflect", GraphNode::Reflect);
        });
        ui.menu_button(MaraId::new(("demo_graph_menu", "Color")), "Color", |ui| {
            spawn(ui, "Color", "RGB → Color", GraphNode::RgbToColor);
            spawn(ui, "Color", "HSV → Color", GraphNode::HsvToColor);
            spawn(ui, "Color", "Color Mix", GraphNode::ColorMix);
            spawn(ui, "Color", "Hue Shift", GraphNode::HueShift);
            spawn(ui, "Color", "Invert", GraphNode::ColorInvert);
            spawn(ui, "Color", "Bright/Contrast", GraphNode::BrightContrast);
            spawn(ui, "Color", "Gamma", GraphNode::Gamma);
        });
        ui.menu_button(MaraId::new(("demo_graph_menu", "Logic")), "Logic", |ui| {
            spawn(ui, "Logic", "If / Else", GraphNode::IfElse);
            spawn(
                ui,
                "Logic",
                "Boolean Math",
                GraphNode::BooleanMath(BoolOp::And),
            );
            spawn(ui, "Logic", "Float → Bool", GraphNode::FloatToBool);
            spawn(ui, "Logic", "Bool → Float", GraphNode::BoolToFloat);
        });
        ui.menu_button(
            MaraId::new(("demo_graph_menu", "Noise / Wave")),
            "Noise / Wave",
            |ui| {
                spawn(
                    ui,
                    "Noise / Wave",
                    "Perlin",
                    GraphNode::Perlin {
                        seed: 12345,
                        frequency: 1.0,
                    },
                );
                spawn(
                    ui,
                    "Noise / Wave",
                    "White Noise",
                    GraphNode::WhiteNoise { seed: 12345 },
                );
                spawn(ui, "Noise / Wave", "Wave", GraphNode::Wave(WaveShape::Sine));
            },
        );
        ui.menu_button(MaraId::new(("demo_graph_menu", "Sinks")), "Sinks", |ui| {
            spawn(ui, "Sinks", "Display", GraphNode::Display);
            spawn(ui, "Sinks", "Plot", GraphNode::Plot);
            spawn(ui, "Sinks", "Plot XY", GraphNode::PlotXY);
            spawn(ui, "Sinks", "Preview", GraphNode::Preview);
            spawn(ui, "Sinks", "Vector Preview", GraphNode::VectorPreview);
            spawn(
                ui,
                "Sinks",
                "Noise Image",
                GraphNode::NoiseImage {
                    seed: 0xCAFE,
                    scale: 0.05,
                },
            );
            spawn(ui, "Sinks", "Noise Field", GraphNode::NoiseField);
            spawn(ui, "Sinks", "Multi Plot", GraphNode::MultiPlot);
            spawn(ui, "Sinks", "Output", GraphNode::Output);
        });
    }
}

/// Inline editor used for unconnected pin rows — surfaces the
/// expected pin type as a tiny widget so the user can type a
/// constant without having to wire a Number/Color/Bool source
/// node. Currently only edits the most useful slots: numeric and
/// boolean. Extending this to vector/color/text per-input is
/// straightforward but the constants live in the graph as fields,
/// not inputs, so the bigger value editors still go on the source
/// node's body / output row.
fn inline_input_editor(
    _graph: &mut Graph<GraphNode>,
    _pin: &InPin,
    ty: PinType,
    ui: &mut mara_core::MaraUi<'_>,
) {
    // Stable-width placeholders matching `inline_value_readout`'s
    // column counts, so swapping between connected (live readout)
    // and unconnected (placeholder) doesn't change the row width.
    let placeholder = match ty {
        PinType::Number => format!("{:>9}", "\u{2014}"),
        PinType::Vector => format!("[{:>7}, {:>7}, {:>7}]", "\u{2014}", "\u{2014}", "\u{2014}"),
        PinType::Color => format!("{:>9}", "\u{2014}"),
        PinType::Bool => format!("{:>4}", "\u{2014}"),
        PinType::Text => format!("{:<9}", "\u{2014}"),
    };
    ui.label_spec(
        &placeholder,
        &LabelSpec::new(11.0, MaraColor32::from_gray(140))
            .mono(true)
            .truncate(false),
    );
}

fn inline_value_readout(v: &Value, ty: PinType, ui: &mut mara_core::MaraUi<'_>) {
    // Monospace + right-aligned fixed-width formatting so the
    // node body width doesn't reflow when a value's digit count
    // changes (e.g. `0.123` → `12.345`). Each readout reserves
    // the same number of glyph columns regardless of magnitude.
    let mut mono = |text: String| {
        ui.label_spec(
            &text,
            &LabelSpec::new(11.0, MaraColor32::from_gray(200))
                .mono(true)
                .truncate(false),
        )
    };
    match (ty, v) {
        (_, Value::Number(n)) => {
            // 9 char-wide column — fits `-NNNN.NNN` (sign +
            // 4-digit integer + decimal + 3 fractional). One
            // char wider than the previous 8-wide so a negative
            // 4-digit value doesn't overflow.
            mono(format!("{n:>9.3}"));
        }
        (_, Value::Vector(v)) => {
            // Each component in 7 chars: `-NN.NN`.
            mono(format!("[{:>7.2}, {:>7.2}, {:>7.2}]", v[0], v[1], v[2]));
        }
        (_, Value::Color(c)) => {
            // Fixed-width swatch — never reflows.
            let (painter, resp) = ui.canvas(MaraVec2::new(28.0, 14.0));
            painter.rect_filled(resp.rect, mara_core::vocab::CornerRadius::same(2), *c);
        }
        (_, Value::Bool(b)) => {
            mono(format!("{:>4}", if *b { "true" } else { "false" }));
        }
        (_, Value::Text(s)) => {
            mono(format!("{s:<8}"));
        }
    }
}

/// Mara-styled operator dropdown for body op pickers — wrapped
/// in a fixed 140-px slot mirroring UE Slate's
/// `SBox.MinDesiredWidth(125)` default-value column. Mara's
/// `dropdown` consumes `ui.available_width()`, so without the
/// slot it'd grow the node arbitrarily wide; the slot caps it
/// at a stable column.
fn op_dropdown<T>(ui: &mut mara_core::MaraUi<'_>, current: &mut T, options: &[(&str, T)])
where
    T: Copy + PartialEq,
{
    let mut idx = options
        .iter()
        .position(|(_, v)| *v == *current)
        .unwrap_or(0);
    let labels: Vec<&str> = options.iter().map(|(l, _)| *l).collect();
    const SLOT_W: f32 = 140.0;
    let h = mara_core::widget::DROPDOWN_ROW_H;
    let mut changed = false;
    ui.row(
        MaraVec2::new(SLOT_W, h),
        mara_core::CrossAlign::Center,
        |ui| {
            let resp = ui.dropdown(
                ("mara_demo_op_dropdown", current as *const T as usize),
                &mut idx,
                &labels,
            );
            changed = resp.changed();
        },
    );
    if changed {
        if let Some((_, v)) = options.get(idx) {
            *current = *v;
        }
    }
}

/// Per-Display-node ring buffer of recent values painted as a
/// sparkline in the node body. Stored in egui ctx data keyed by
/// the graph node id so it survives across frames without leaking.
fn draw_sparkline(
    graph: &Graph<GraphNode>,
    node: mara::extras::graph::NodeId,
    ui: &mut mara_core::MaraUi<'_>,
    current: f64,
) {
    let _ = graph; // signature parity for future inline-editor use
    const HISTORY: usize = 96;
    let key = MaraId::new(("mara_demo_sparkline", node));
    let mut buf: Vec<f32> = ui.memory().get_temp::<Vec<f32>>(key).unwrap_or_default();
    if buf.len() >= HISTORY {
        buf.remove(0);
    }
    buf.push(current as f32);
    ui.memory().set_temp(key, buf.clone());

    let label = format!("{current:.3}");
    ui.label_spec(
        &label,
        &LabelSpec::new(11.0, mara_core::style::on_panel())
            .mono(true)
            .truncate(false),
    );

    let (painter, resp) = ui.canvas(MaraVec2::new(140.0, 36.0));
    let rect = resp.rect;
    painter.rect_filled(
        rect,
        mara_core::vocab::CornerRadius::same(3),
        MaraColor32::from_black_alpha(40),
    );

    if buf.len() >= 2 {
        let (lo, hi) = buf
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), v| {
                (a.min(*v), b.max(*v))
            });
        let span = (hi - lo).max(1e-3);
        let pad = 4.0;
        let n = buf.len();
        let mut points = Vec::with_capacity(n);
        for (i, v) in buf.iter().enumerate() {
            let x = rect.left()
                + pad
                + (i as f32 / (n.saturating_sub(1).max(1) as f32)) * (rect.width() - 2.0 * pad);
            let t = 1.0 - (v - lo) / span;
            let y = rect.top() + pad + t * (rect.height() - 2.0 * pad);
            points.push(MaraPos2::new(x, y));
        }
        painter.polyline(
            points,
            MaraStroke::new(1.5, MaraColor32::from_rgb(0xFF, 0xB9, 0x38)),
        );
    }
    ui.request_repaint();
}

/// A demo graph that wires several capabilities together so the
/// user can SEE the full feature set on first open. Laid out on
/// a strict grid (`COL_W = 260`, `ROW_H = 110`) so wires flow
/// straight left-to-right and node columns line up cleanly across
/// the three sub-pipelines.
///
/// ```text
/// Pipeline 1 — sine wave + colour mix:
///   Time ─┐
///         ├→ × ─→ sin ─→ × ─→ + ─→ Display
///   Num1.5┘     Num0.5┘    Num0.5┘    │
///                                     └→ ColorMix ─→ Preview
///   Color(red)  ─┐
///   Color(blue) ─┘
///
/// Pipeline 2 — vector → scalar:
///   Vec(1,2,3) ─→ Length ─→ Output
///
/// Pipeline 3 — noise gate:
///   Perlin ─→ Compare ─→ IfElse ─→ Display
///   Num0  ─┘   Num1, Num-1 ─┘
/// ```
pub fn default_graph() -> Graph<GraphNode> {
    let mut g = Graph::new();

    const COL_W: f32 = 340.0; // horizontal spacing between columns
    let col = |i: i32| (i as f32) * COL_W;
    let row = |y: f32| y;

    // ── Pipeline 1: time-driven sine wave (top) ──
    //
    //  col 0      col 1     col 2     col 3      col 4     col 5
    //  Time       Mul       Sin       Mul        Add       Display
    //  Num(1.5)             Num(0.5)  Num(0.5)
    //
    let t = g.insert_node(MaraPos2::new(col(0), row(0.0)), GraphNode::Time);
    let freq = g.insert_node(MaraPos2::new(col(0), row(170.0)), GraphNode::Number(1.5));
    let mul = g.insert_node(
        MaraPos2::new(col(1), row(60.0)),
        GraphNode::ScalarMath(ScalarOp::Mul),
    );
    let sin = g.insert_node(
        MaraPos2::new(col(2), row(60.0)),
        GraphNode::Trig(TrigFn::Sin),
    );
    let half = g.insert_node(MaraPos2::new(col(2), row(230.0)), GraphNode::Number(0.5));
    let bias = g.insert_node(
        MaraPos2::new(col(3), row(60.0)),
        GraphNode::ScalarMath(ScalarOp::Mul),
    );
    let half2 = g.insert_node(MaraPos2::new(col(3), row(230.0)), GraphNode::Number(0.5));
    let lift = g.insert_node(
        MaraPos2::new(col(4), row(60.0)),
        GraphNode::ScalarMath(ScalarOp::Add),
    );
    let display = g.insert_node(MaraPos2::new(col(5), row(60.0)), GraphNode::Display);

    // ── Colour mix branch (below pipeline 1, fed by `lift`) ──
    //
    //  col 3                       col 4         col 5
    //  Color(red)
    //  Color(blue)                 ColorMix      Preview
    //
    let red = g.insert_node(
        MaraPos2::new(col(3), row(480.0)),
        GraphNode::Color(MaraColor32::from_rgb(0xE0, 0x6C, 0x4F)),
    );
    let blue = g.insert_node(
        MaraPos2::new(col(3), row(620.0)),
        GraphNode::Color(MaraColor32::from_rgb(0x4D, 0xA8, 0xDA)),
    );
    let cmix = g.insert_node(MaraPos2::new(col(4), row(540.0)), GraphNode::ColorMix);
    let preview = g.insert_node(MaraPos2::new(col(5), row(540.0)), GraphNode::Preview);

    // ── Pipeline 2: vector → length → output ──
    //
    //  col 0          col 1     col 2
    //  Vector ────→   Length ──→ Output
    //
    let vec = g.insert_node(
        MaraPos2::new(col(0), row(920.0)),
        GraphNode::Vector([1.0, 2.0, 3.0]),
    );
    let len = g.insert_node(MaraPos2::new(col(1), row(920.0)), GraphNode::Length);
    let out = g.insert_node(MaraPos2::new(col(2), row(920.0)), GraphNode::Output);

    // ── Pipeline 3: noise → compare → ifelse → display ──
    //
    //  col 0     col 1                col 2                          col 3
    //  Perlin    Compare              IfElse                         Display
    //            Num(0)               Num(+1), Num(-1)
    //
    let perlin = g.insert_node(
        MaraPos2::new(col(0), row(1180.0)),
        GraphNode::Perlin {
            seed: 0xCAFE,
            frequency: 1.5,
        },
    );
    let zero = g.insert_node(MaraPos2::new(col(1), row(1320.0)), GraphNode::Number(0.0));
    let cmp = g.insert_node(
        MaraPos2::new(col(1), row(1180.0)),
        GraphNode::Compare(CompareOp::Gt),
    );
    let one = g.insert_node(MaraPos2::new(col(2), row(1320.0)), GraphNode::Number(1.0));
    let neg = g.insert_node(MaraPos2::new(col(2), row(1460.0)), GraphNode::Number(-1.0));
    let gate = g.insert_node(MaraPos2::new(col(2), row(1180.0)), GraphNode::IfElse);
    let display2 = g.insert_node(MaraPos2::new(col(3), row(1180.0)), GraphNode::Display);

    // ── Pipeline 4: sophisticated 4-channel scope ──
    //
    //  col 0     col 1       col 2          col 3
    //  Time ─→ ×freq[0] ─→  sin → ─┐
    //                              ├─→ MultiPlot  (4 lines)
    //  Time ─→ ×freq[1] ─→  cos →  │
    //  Time ─→ ×freq[2] ─→  sin² ─ │
    //  Time ─→ ×freq[3] ─→  saw ─  ┘
    //
    let t2 = g.insert_node(MaraPos2::new(col(0), row(1620.0)), GraphNode::Time);
    let f1 = g.insert_node(MaraPos2::new(col(0), row(1760.0)), GraphNode::Number(1.0));
    let f2 = g.insert_node(MaraPos2::new(col(0), row(1900.0)), GraphNode::Number(2.0));
    let f3 = g.insert_node(MaraPos2::new(col(0), row(2040.0)), GraphNode::Number(3.0));
    let f4 = g.insert_node(MaraPos2::new(col(0), row(2180.0)), GraphNode::Number(0.5));
    let m1 = g.insert_node(
        MaraPos2::new(col(1), row(1620.0)),
        GraphNode::ScalarMath(ScalarOp::Mul),
    );
    let m2 = g.insert_node(
        MaraPos2::new(col(1), row(1760.0)),
        GraphNode::ScalarMath(ScalarOp::Mul),
    );
    let m3 = g.insert_node(
        MaraPos2::new(col(1), row(1900.0)),
        GraphNode::ScalarMath(ScalarOp::Mul),
    );
    let m4 = g.insert_node(
        MaraPos2::new(col(1), row(2040.0)),
        GraphNode::ScalarMath(ScalarOp::Mul),
    );
    let s1 = g.insert_node(
        MaraPos2::new(col(2), row(1620.0)),
        GraphNode::Trig(TrigFn::Sin),
    );
    let s2 = g.insert_node(
        MaraPos2::new(col(2), row(1760.0)),
        GraphNode::Trig(TrigFn::Cos),
    );
    let s3 = g.insert_node(
        MaraPos2::new(col(2), row(1900.0)),
        GraphNode::Wave(WaveShape::Triangle),
    );
    let s4 = g.insert_node(
        MaraPos2::new(col(2), row(2040.0)),
        GraphNode::Wave(WaveShape::Saw),
    );
    let mplot = g.insert_node(MaraPos2::new(col(3), row(1620.0)), GraphNode::MultiPlot);

    // ── Pipeline 5: sophisticated noise field ──
    //
    // Every NoiseField parameter is driven by a Number source —
    // the user can swap, scale, or replace any one of them with
    // arbitrary upstream graph logic. The body of NoiseField
    // shows ONLY the mode dropdown + the 160 × 96 px preview.
    //
    //  col 0          col 1          col 2          col 3
    //  Time ─→──→──→  × ─→──────→  drift_x ─┐
    //  Num(0.4) ─────╱                       ├──→ NoiseField
    //  Num(0)  drift_y ──────────→──────────┤   (mode dropdown
    //  Num(1.5) freq ────────────→──────────┤    + image)
    //  Num(0xCAFE) seed ────────→───────────┤
    //  Num(5) octaves ─────────→────────────┤
    //  Num(0.55) persistence ─→─────────────┤
    //  Num(2.1) lacunarity ─→───────────────┤
    //  Num(1.0) gain ──────→────────────────┘
    //
    let t3 = g.insert_node(MaraPos2::new(col(0), row(2400.0)), GraphNode::Time);
    let speed = g.insert_node(MaraPos2::new(col(0), row(2540.0)), GraphNode::Number(0.4));
    let drift_x = g.insert_node(
        MaraPos2::new(col(1), row(2400.0)),
        GraphNode::ScalarMath(ScalarOp::Mul),
    );
    let drift_y = g.insert_node(MaraPos2::new(col(1), row(2540.0)), GraphNode::Number(0.0));
    let freq_n = g.insert_node(MaraPos2::new(col(1), row(2680.0)), GraphNode::Number(1.5));
    let seed_n = g.insert_node(
        MaraPos2::new(col(1), row(2820.0)),
        GraphNode::Number(0xCAFE as f64),
    );
    let oct_n = g.insert_node(MaraPos2::new(col(1), row(2960.0)), GraphNode::Number(5.0));
    let pers_n = g.insert_node(MaraPos2::new(col(1), row(3100.0)), GraphNode::Number(0.55));
    let lac_n = g.insert_node(MaraPos2::new(col(1), row(3240.0)), GraphNode::Number(2.1));
    let gain_n = g.insert_node(MaraPos2::new(col(1), row(3380.0)), GraphNode::Number(1.0));
    let nfield = g.insert_node(MaraPos2::new(col(2), row(2400.0)), GraphNode::NoiseField);

    // ── Wire it up ──
    let connect = |g: &mut Graph<GraphNode>, src, sout, dst, dinp| {
        g.connect(
            OutPinId {
                node: src,
                output: sout,
            },
            InPinId {
                node: dst,
                input: dinp,
            },
        );
    };
    connect(&mut g, t, 0, mul, 0);
    connect(&mut g, freq, 0, mul, 1);
    connect(&mut g, mul, 0, sin, 0);
    connect(&mut g, sin, 0, bias, 0);
    connect(&mut g, half2, 0, bias, 1);
    connect(&mut g, bias, 0, lift, 0);
    connect(&mut g, half, 0, lift, 1);
    connect(&mut g, lift, 0, display, 0);
    connect(&mut g, lift, 0, cmix, 2);
    connect(&mut g, red, 0, cmix, 0);
    connect(&mut g, blue, 0, cmix, 1);
    connect(&mut g, cmix, 0, preview, 0);

    connect(&mut g, vec, 0, len, 0);
    connect(&mut g, len, 0, out, 0);

    connect(&mut g, perlin, 0, cmp, 0);
    connect(&mut g, zero, 0, cmp, 1);
    connect(&mut g, cmp, 0, gate, 0);
    connect(&mut g, one, 0, gate, 1);
    connect(&mut g, neg, 0, gate, 2);
    connect(&mut g, gate, 0, display2, 0);

    // Pipeline 4 wires (4-channel scope)
    connect(&mut g, t2, 0, m1, 0);
    connect(&mut g, f1, 0, m1, 1);
    connect(&mut g, t2, 0, m2, 0);
    connect(&mut g, f2, 0, m2, 1);
    connect(&mut g, t2, 0, m3, 0);
    connect(&mut g, f3, 0, m3, 1);
    connect(&mut g, t2, 0, m4, 0);
    connect(&mut g, f4, 0, m4, 1);
    connect(&mut g, m1, 0, s1, 0);
    connect(&mut g, m2, 0, s2, 0);
    connect(&mut g, m3, 0, s3, 0);
    connect(&mut g, m4, 0, s4, 0);
    connect(&mut g, s1, 0, mplot, 0);
    connect(&mut g, s2, 0, mplot, 1);
    connect(&mut g, s3, 0, mplot, 2);
    connect(&mut g, s4, 0, mplot, 3);

    // Pipeline 5 wires (sophisticated noise field)
    // Pin order: seed, offset_x, offset_y, freq, octaves,
    //            persistence, lacunarity, gain.
    connect(&mut g, t3, 0, drift_x, 0);
    connect(&mut g, speed, 0, drift_x, 1);
    connect(&mut g, seed_n, 0, nfield, 0);
    connect(&mut g, drift_x, 0, nfield, 1);
    connect(&mut g, drift_y, 0, nfield, 2);
    connect(&mut g, freq_n, 0, nfield, 3);
    connect(&mut g, oct_n, 0, nfield, 4);
    connect(&mut g, pers_n, 0, nfield, 5);
    connect(&mut g, lac_n, 0, nfield, 6);
    connect(&mut g, gain_n, 0, nfield, 7);

    uniform_node_widths(&mut g);
    g
}

/// Give every node in `graph` the same minimum width.
///
/// Node width is otherwise whatever the drawn content happens to need,
/// and this viewer draws an inline editor for any unwired input — so a
/// `Number` with a drag field comes out visibly wider than the `Add`
/// beside it, and a column of peers ends up ragged. Applying one floor
/// is the only rule that makes a graph read as a set of peers rather
/// than as assorted boxes.
///
/// A floor rather than a fixed size: a node whose content genuinely
/// needs more room still gets it, because clipping the app's own
/// widgets to enforce tidiness would be the worse trade.
fn uniform_node_widths(graph: &mut Graph<GraphNode>) {
    let ids: Vec<NodeId> = graph.node_ids().map(|(id, _)| id).collect();
    for id in ids {
        if graph.size_override_of(id).is_some() {
            continue;
        }
        graph.set_size_override(id, Some(mara::ui::vocab::Vec2::new(NODE_W, 0.0)));
    }
}

const DEFAULT_CODE: &str = "// Mara code editor demo — Rust syntax highlighting.
fn fibonacci(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let mut a: u64 = 0;
    let mut b: u64 = 1;
    for _ in 2..=n {
        let next = a + b;
        a = b;
        b = next;
    }
    b
}

fn main() {
    let label = \"fib(20)\";
    println!(\"{label} = {}\", fibonacci(20));
}
";

// ─── Demo scene tree ───────────────────────────────────────────────

type DemoTreeRow = (
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
    MaraColor32,
);

const DEMO_TREE: &[DemoTreeRow] = &[
    (
        "/World",
        "World",
        "folder",
        &["/World/Robot", "/World/Lights"],
        MaraColor32::from_rgb(0x55, 0x6E, 0x9C),
    ),
    (
        "/World/Robot",
        "Robot",
        "person",
        &["/World/Robot/base", "/World/Robot/arm"],
        MaraColor32::from_rgb(0xE0, 0x6C, 0x4F),
    ),
    (
        "/World/Robot/base",
        "base",
        "code",
        &[],
        MaraColor32::from_rgb(0x4D, 0xA8, 0xDA),
    ),
    (
        "/World/Robot/arm",
        "arm",
        "code",
        &["/World/Robot/arm/grip"],
        MaraColor32::from_rgb(0xE6, 0xB7, 0x3D),
    ),
    (
        "/World/Robot/arm/grip",
        "grip",
        "code",
        &[],
        MaraColor32::from_rgb(0x9C, 0x55, 0xC0),
    ),
    (
        "/World/Lights",
        "Lights",
        "image",
        &["/World/Lights/sun"],
        MaraColor32::from_rgb(0xF5, 0xC2, 0x42),
    ),
    (
        "/World/Lights/sun",
        "sun",
        "image",
        &[],
        MaraColor32::from_rgb(0xFF, 0xE5, 0x6B),
    ),
];

fn demo_tree_node(path: &str) -> Option<&'static DemoTreeRow> {
    DEMO_TREE.iter().find(|(p, _, _, _, _)| *p == path)
}

fn demo_tree(
    tree: &mut mara_core::widget::TreeBody,
    root_id: MaraId,
    accent: MaraColor32,
    filter: &str,
) {
    let sel_key = root_id.with("mara_demo_tree_selected");
    let mut selected: String = tree.temp_string(sel_key).unwrap_or_default();
    let initial_selected = selected.clone();
    let mut frame_clicked: Option<String> = None;
    walk_demo_tree(
        tree,
        root_id,
        "/World",
        0,
        &selected,
        accent,
        filter,
        &mut frame_clicked,
    );
    if let Some(p) = frame_clicked {
        selected = p;
    }
    if selected != initial_selected {
        tree.set_temp_string(sel_key, selected);
    }
}

/// Does this node — or any descendant — match the (lowercase)
/// substring `filter`? Branches stay visible when any child passes
/// so the path to a matching leaf never gets hidden by the parent
/// chain. Empty filter passes everything.
fn demo_tree_passes(path: &'static str, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let Some((p, name, _, children, _)) = demo_tree_node(path) else {
        return false;
    };
    if name.to_lowercase().contains(filter) || p.to_lowercase().contains(filter) {
        return true;
    }
    children.iter().any(|c| demo_tree_passes(c, filter))
}

fn walk_demo_tree(
    tree: &mut mara_core::widget::TreeBody,
    root_id: MaraId,
    path: &'static str,
    depth: u32,
    selected: &str,
    accent: MaraColor32,
    filter: &str,
    clicked: &mut Option<String>,
) {
    let Some((p, name, icon, children, material)) = demo_tree_node(path) else {
        return;
    };
    if !demo_tree_passes(path, filter) {
        return;
    }
    let is_branch = !children.is_empty();
    let exp_key = root_id.with(("mara_demo_tree_expanded", *p));
    let eye_key = root_id.with(("mara_demo_tree_eye", *p));
    let lock_key = root_id.with(("mara_demo_tree_lock", *p));
    let mut expanded: bool = tree.persisted_bool(exp_key).unwrap_or(true);
    let mut eye_on: bool = tree.persisted_bool(eye_key).unwrap_or(true);
    let mut lock_on: bool = tree.persisted_bool(lock_key).unwrap_or(false);
    let mut swatch_dummy = false;

    let mut slots = [
        TreeIconSlot::new(TreeIconKind::Eye, &mut eye_on).with_tooltip("Toggle visibility"),
        TreeIconSlot::new(TreeIconKind::Lock, &mut lock_on).with_tooltip("Toggle lock"),
        TreeIconSlot::new(TreeIconKind::Color((*material).into()), &mut swatch_dummy)
            .with_tooltip("Material colour"),
    ];
    let resp = tree.row(
        *p,
        depth,
        if is_branch { Some(&mut expanded) } else { None },
        Some(*icon),
        *name,
        selected == *p,
        accent,
        &mut slots,
    );
    if resp.body.clicked {
        *clicked = Some((*p).to_string());
    }

    tree.set_persisted_bool(exp_key, expanded);
    tree.set_persisted_bool(eye_key, eye_on);
    tree.set_persisted_bool(lock_key, lock_on);

    if is_branch && expanded {
        for child in *children {
            walk_demo_tree(
                tree,
                root_id,
                child,
                depth + 1,
                selected,
                accent,
                filter,
                clicked,
            );
        }
    }
}

/// Draw one or more series as an auto-fitted line chart.
///
/// Replaces the `egui_plot` widget the demo's Plot nodes used: axes are
/// a baseline plus a mid-line, and each series is a single polyline
/// scaled to the combined value range. Enough for a scope readout, and
/// it keeps the node body on the sealed surface (PLAN.md WS-D1.4).
fn paint_line_chart(
    painter: &mara_core::MaraPainter,
    rect: mara_core::vocab::Rect,
    series: &[(&[f64], MaraColor32)],
) {
    let grid = MaraColor32::from_rgba_unmultiplied(0xEE, 0xEE, 0xEE, 40);
    painter.line_segment(
        MaraPos2::new(rect.min.x, rect.center().y),
        MaraPos2::new(rect.max.x, rect.center().y),
        MaraStroke::new(1.0, grid),
    );

    // One shared range so multiple series stay comparable.
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for (values, _) in series {
        for v in *values {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
    }
    if !lo.is_finite() || !hi.is_finite() {
        return;
    }
    // A flat series would divide by zero; give it a unit window so it
    // draws along the middle instead of vanishing.
    let span = if (hi - lo).abs() < f64::EPSILON {
        1.0
    } else {
        hi - lo
    };

    for (values, color) in series {
        if values.len() < 2 {
            continue;
        }
        let last = (values.len() - 1) as f32;
        let points: Vec<MaraPos2> = values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let t = i as f32 / last;
                let norm = ((*v - lo) / span) as f32;
                MaraPos2::new(
                    rect.min.x + rect.width() * t,
                    rect.max.y - rect.height() * norm,
                )
            })
            .collect();
        painter.polyline(points, MaraStroke::new(1.5, *color));
    }
}
