//! `bevy_mara` widget gallery + layout showcase, reimplemented on
//! top of `mara_core` (the new pane / ribbon / container / pod /
//! widget stack). Mirrors the layout of the original Mara demo
//! one panel at a time:
//!
//! * **Widgets** — Flags / Numbers / Bars / Buttons / Animated.
//! * **Containers** — Position + Rotation, axis-coloured drag values.
//! * **Elements** — scene tree (eye/lock/colour slots) + flat
//!   hybrid_select roster.
//! * **Theme** — Profile dropdowns + accent picker + glass slider.
//! * **Keys** — keybinding rows (readouts).
//! * **About** — version + dependency readouts.
//!
//! Ported from `bevy_mara --example demo` to run in the browser via
//! eframe (see `api_crates/web`). The Bevy 3D scene is dropped;
//! everything else — ribbons, panes, the widget gallery, theme
//! picker, canvas whiteboard, node graph and code editor — is the
//! same host-agnostic `mara_core` UI the Bevy demo renders.

#![allow(
    dead_code,
    clippy::collapsible_if,
    clippy::doc_lazy_continuation,
    clippy::explicit_auto_deref,
    clippy::too_many_arguments,
    clippy::upper_case_acronyms
)]

use eframe::egui;

use mara_core::container::SeparatorStyle;
use mara_core::pane::{Pane, PaneAnchor, PaneBody, RailZone};
use mara_core::pod::Pod;
use mara_core::ribbon::{
    ResolvedSlotRibbon, RibbonAction, RibbonCluster, RibbonDrag, RibbonEdge, RibbonGlyph,
    RibbonMode, RibbonOpen, RibbonPlacement, RibbonRole, RibbonSlotClick, RibbonSlotItem,
    draw_slot_ribbons_featureful,
};
use mara_core::shelf::{ShelfContainer, ShelfDef, ShelfEdge, ShelfState};
use mara_core::style::{AccentColor, GlassOpacity, Mode, srgb_to_egui};
use mara_core::widget::{FillStyle, TreeIconKind, TreeIconSlot};
// Vendored extras — node graph + code editor. In the `egui_mara`
// facade they live under `mara_core::extras::*`; the node-graph
// offscreen renderer is `egui_mara::EframeNodeViewBackend`.
use egui_mara::EframeNodeViewBackend;
use mara_core::extras::code::Syntax;
use mara_core::extras::graph::{
    Graph, InPin, InPinId, NodePin, NodeViewState, NodeViewer, OutPin, OutPinId, PinInfo,
};

// ─── Ribbon / pane ids ──────────────────────────────────────────────

const RIBBON_LEFT: &str = "demo_ribbon_left";
const RIBBON_RIGHT: &str = "demo_ribbon_right";
const RIBBON_TOP: &str = "demo_ribbon_top";
const RIBBON_BOTTOM: &str = "demo_ribbon_bottom";

// Fullscreen-only ribbons. Painted only while a maximizable widget
// (node graph / code editor) is in its fullscreen overlay — driven
// by `is_any_fullscreen(ctx)` in the per-frame top-level callback
// below. Uses the SAME ribbon API as the regular rails, so the
// fullscreen view looks like a fresh canvas built from the same
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
const CANVAS_SHELF_LEFT: &str = "demo_canvas_shelf_left";

const ACTION_PREV_CUBE: &str = "demo_action_prev_cube";
const ACTION_NEXT_CUBE: &str = "demo_action_next_cube";
const ACTION_CANVAS_CLEAR: &str = "demo_action_canvas_clear";
const ACTION_VIEW_BEVY: &str = "demo_action_view_bevy";
const ACTION_VIEW_CANVAS: &str = "demo_action_view_canvas";
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
        RIBBON_BOTTOM,
        PANE_EDITOR,
        PaneAnchor::BottomRail(RailZone::Start),
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
        RIBBON_BOTTOM,
        PANE_CANVAS_EXPORT,
        PaneAnchor::BottomRail(RailZone::Start),
        "Export",
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
        accepts: &[RIBBON_RIGHT, RIBBON_BOTTOM],
    },
    RibbonSpec {
        id: RIBBON_RIGHT,
        edge: RibbonEdge::Right,
        role: RibbonRole::Panel,
        mode: RibbonMode::ThreeSided,
        accepts: &[RIBBON_LEFT, RIBBON_BOTTOM],
    },
    RibbonSpec {
        id: RIBBON_BOTTOM,
        edge: RibbonEdge::Bottom,
        role: RibbonRole::Panel,
        mode: RibbonMode::ThreeSided,
        accepts: &[RIBBON_LEFT, RIBBON_RIGHT],
    },
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
    // TOP rail — meta.
    RibbonButtonSpec {
        id: PANE_ABOUT,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Start,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("info"),
        tooltip: "About this demo",
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
        id: ACTION_CLOSE_APP,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::End,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("dismiss"),
        tooltip: "Close application",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    // BOTTOM rail — Editor plus the
    // one-shot cube-cycle action buttons in the End cluster.
    RibbonButtonSpec {
        id: PANE_EDITOR,
        ribbon: RIBBON_BOTTOM,
        cluster: RibbonCluster::Start,
        slot: 0,
        draggable: true,
        glyph: RibbonGlyph::Icon("flowchart"),
        tooltip: "Editor",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: ACTION_PREV_CUBE,
        ribbon: RIBBON_BOTTOM,
        cluster: RibbonCluster::End,
        slot: 0,
        draggable: true,
        glyph: RibbonGlyph::Icon("arrow-left"),
        tooltip: "Previous cube",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
    RibbonButtonSpec {
        id: ACTION_NEXT_CUBE,
        ribbon: RIBBON_BOTTOM,
        cluster: RibbonCluster::End,
        slot: 1,
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
        id: PANE_ABOUT,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Start,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("info"),
        tooltip: "About this demo",
        child_ribbon: None,
        role: None,
    },
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
        id: ACTION_CLOSE_APP,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::End,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("dismiss"),
        tooltip: "Close application",
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
        ribbon: RIBBON_BOTTOM,
        cluster: RibbonCluster::Start,
        slot: 0,
        draggable: true,
        glyph: RibbonGlyph::Icon("arrow-download"),
        tooltip: "Canvas export",
        child_ribbon: None,
        role: None,
    },
    RibbonButtonSpec {
        id: ACTION_CANVAS_CLEAR,
        ribbon: RIBBON_BOTTOM,
        cluster: RibbonCluster::End,
        slot: 0,
        draggable: true,
        glyph: RibbonGlyph::Icon("delete"),
        tooltip: "Clear canvas strokes",
        child_ribbon: None,
        role: Some(RibbonRole::Icon),
    },
];

// ─── Fullscreen-only ribbon sets ───────────────────────────────────
//
// Painted by `ribbon renderer` only while the corresponding widget is
// in its fullscreen overlay — branched in the per-frame paint via
// `is_graph_fullscreen` / `is_code_fullscreen`. Each set uses the
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
        id: PANE_ABOUT,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Start,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("info"),
        tooltip: "About this demo",
        child_ribbon: None,
        role: None,
    },
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
        id: PANE_ABOUT,
        ribbon: RIBBON_TOP,
        cluster: RibbonCluster::Start,
        slot: 0,
        draggable: false,
        glyph: RibbonGlyph::Icon("info"),
        tooltip: "About this demo",
        child_ribbon: None,
        role: None,
    },
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

    #[test]
    fn demo_ribbon_icons_are_renderable() {
        for item in RIBBON_ITEMS
            .iter()
            .chain(RIBBON_ITEMS_ROOT_VIEW)
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
}

fn ribbon_action(id: &'static str) -> RibbonAction {
    match id {
        ACTION_CLOSE_APP => RibbonAction::CloseApp,
        ACTION_RESTORE_FULLSCREEN => RibbonAction::PopWorkspace,
        _ => RibbonAction::Command(egui::Id::new(id)),
    }
}

fn draw_unified_ribbons(
    ctx: &egui::Context,
    accent: egui::Color32,
    ribbons: &[RibbonSpec],
    items: &[RibbonButtonSpec],
    open: &mut RibbonOpen,
    placement: &mut RibbonPlacement,
    drag: &mut RibbonDrag,
    active: impl Fn(&'static str) -> bool,
) -> Vec<RibbonSlotClick> {
    let mut resolved = Vec::new();
    for ribbon in ribbons {
        for cluster in [
            RibbonCluster::Start,
            RibbonCluster::Middle,
            RibbonCluster::End,
        ] {
            let slot_items: Vec<RibbonSlotItem> = items
                .iter()
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
                id: egui::Id::new((ribbon.id, cluster)),
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
    draw_slot_ribbons_featureful(ctx, accent, &resolved, open, placement, drag)
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
}

#[derive(Default)]
struct CanvasViewState {
    strokes: Vec<Vec<egui::Pos2>>,
}

#[derive(Default)]
struct CanvasShelfState(ShelfState);

/// Per-graph sharp-zoom state (secondary `egui::Context`, pan, zoom,
/// wgpu render target). Persists across frames so the same instance
/// reaches `mara_node_graph` every frame for the editor pane —
/// recreating it would drop the cached wgpu texture and renderer.
struct EditorNodeView(NodeViewState);

impl Default for EditorNodeView {
    fn default() -> Self {
        Self(NodeViewState::new())
    }
}

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
    canvas_shelves: CanvasShelfState,
    editor_node_view: EditorNodeView,
    editor_graph: EditorGraph,
}

impl DemoApp {
    /// Built once by `eframe::WebRunner`. No persistence — every
    /// session starts from the default mara layout.
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }
}

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ui_system(self, ctx, frame);
    }
}

// ─── UI ─────────────────────────────────────────────────────────────

/// Per-frame UI — the body of the old Bevy `ui_system`, now driven by
/// eframe. `app` carries the state the Bevy build held as resources;
/// `frame` provides the wgpu render state the node graph paints into.
fn ui_system(app: &mut DemoApp, ctx: &egui::Context, frame: &mut eframe::Frame) {
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
        canvas_shelves,
        editor_node_view,
        editor_graph,
    } = app;
    // The editor pane's node graph renders into an offscreen wgpu
    // texture through `EframeNodeViewBackend`, which needs eframe's
    // wgpu render state. The web host always runs the wgpu backend.
    let render_state = frame
        .wgpu_render_state()
        .expect("eframe must run with the wgpu backend (see api_crates/web)");

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
    mara_core::style::apply_theme(ctx, *accent, *glass);

    let accent_col = mara_core::style::active_accent();
    mara_core::publish_shelf_layout(
        ctx,
        mara_core::ShelfLayout {
            viewport: ctx.content_rect(),
            left: None,
            right: None,
            bottom: None,
        },
    );

    // Actual root/L0 canvas switch:
    // - BevyScene is the normal demo: Bevy 3D scene plus Mara panes/ribbons.
    // - Canvas owns the whole egui canvas and replaces the Bevy scene visually.
    if *root_view == DemoRootView::Canvas {
        canvas_root_view(ctx, accent_col, canvas_view, &mut canvas_shelves.0);
    }

    // Fullscreen-view branch. The fullscreen overlay paints at
    // `Order::Background`, so the ribbon assembly below (drawn at
    // `Order::Middle`) layers over the maximised canvas. Which set
    // of items the rails carry depends on WHICH widget is fullscreen
    // — graph and code get their own toolsets, picked via the
    // module-supplied `is_graph_fullscreen` / `is_code_fullscreen`
    // helpers.
    let fs_active = mara_core::extras::maximize::is_any_fullscreen(ctx);
    let graph_fs = mara_core::extras::graph::is_graph_fullscreen(ctx);
    let code_fs = mara_core::extras::code::is_code_fullscreen(ctx, cid(PANE_EDITOR, "code_state"));
    if fs_active {
        // The persistent main bar owns module restore in L1/fullscreen.
        // Suppress the old floating restore chip so it does not stack
        // above the top-right system-control slot.
        mara_core::embed::set_fullscreen_minimize_chip_visible(ctx, false);
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
    let current_ribbon_items: &[RibbonButtonSpec] = if fs_active && graph_fs {
        RIBBON_ITEMS_FS_GRAPH
    } else if fs_active && code_fs {
        RIBBON_ITEMS_FS_CODE
    } else if fs_active {
        RIBBON_ITEMS_FS_GRAPH
    } else if *root_view == DemoRootView::Canvas {
        RIBBON_ITEMS_ROOT_VIEW
    } else {
        RIBBON_ITEMS
    };
    let current_ribbons: &[RibbonSpec] = if fs_active { RIBBONS_FS } else { RIBBONS };

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
        Some(match def.edge {
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
        let anchor = live_anchor(PANE_EDITOR).unwrap_or(PaneAnchor::BottomRail(RailZone::Start));
        let mut backend = EframeNodeViewBackend::new(render_state);
        let now = ctx.input(|i| i.time);
        let mut viewer = DemoViewer { time: now };
        Pane::new(PANE_EDITOR, "Editor", anchor, accent_col)
            .resize(mara_core::pane::PaneResize::SPAN)
            .show(ctx, |body| {
                editor_pane(
                    body,
                    &mut editor_node_view.0,
                    &mut editor_graph.0,
                    &mut viewer,
                    &mut backend,
                );
            });
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
        // `Pane::show` — the typed `PaneBody::add_node_graph` stores
        // them in the pending-spec list and the closure runs at
        // `body.finish()` time (after the user closure returns). Lift
        // `viewer` / `backend` to the iteration scope so they live
        // past that point.
        if button_id == PANE_EDITOR {
            let mut backend = EframeNodeViewBackend::new(render_state);
            let now = ctx.input(|i| i.time);
            let mut viewer = DemoViewer { time: now };
            Pane::new(button_id, label, anchor, accent_col)
                .resize(mara_core::pane::PaneResize::SPAN)
                .show(ctx, |body| {
                    editor_pane(
                        body,
                        &mut editor_node_view.0,
                        &mut editor_graph.0,
                        &mut viewer,
                        &mut backend,
                    );
                });
            continue;
        }
        Pane::new(button_id, label, anchor, accent_col)
            .resize(mara_core::pane::PaneResize::SPAN)
            .order(if fs_active {
                egui::Order::Foreground
            } else {
                egui::Order::Background
            })
            .show(ctx, |body| match button_id {
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
                _ => {}
            });
    }

    // Ribbon paint, AFTER the panes — registration order within
    // `Order::Foreground` lands the ribbon `Area`s on top of the
    // `embed` fullscreen overlay, so the host's fullscreen rails
    // remain visible.
    let clicks: Vec<RibbonSlotClick> = if fs_active {
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
            ctx,
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
            ctx,
            accent_col,
            RIBBONS,
            current_ribbon_items,
            open,
            placement,
            drag,
            |id| match *root_view {
                DemoRootView::BevyScene => id == ACTION_VIEW_BEVY,
                DemoRootView::Canvas => id == ACTION_VIEW_CANVAS,
            },
        )
    };

    // (Bevy's borderless main-bar window drag has no browser
    // equivalent — `main_bar_empty_drag_started` is simply ignored.)

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
        if click.item == egui::Id::new(ACTION_VIEW_BEVY) {
            if fs_active {
                mara_core::embed::restore_fullscreen(ctx);
            }
            *root_view = DemoRootView::BevyScene;
            continue;
        }
        if click.item == egui::Id::new(ACTION_VIEW_CANVAS) {
            if fs_active {
                mara_core::embed::restore_fullscreen(ctx);
            }
            *root_view = DemoRootView::Canvas;
            continue;
        }
        if click.item == egui::Id::new(ACTION_RESTORE_FULLSCREEN) {
            mara_core::embed::restore_fullscreen(ctx);
            continue;
        }
        // (No "close app" on web — closing the browser tab from
        // script isn't possible, so ACTION_CLOSE_APP is a no-op.)
        if click.item == egui::Id::new(ACTION_CANVAS_CLEAR) {
            canvas_view.strokes.clear();
            continue;
        }
        if click.item == egui::Id::new(ACTION_PREV_CUBE)
            || click.item == egui::Id::new(ACTION_NEXT_CUBE)
        {
            let cur = accent.0;
            let cur_idx = SWATCH_RGB
                .iter()
                .position(|&(r, g, b)| egui::Color32::from_rgb(r, g, b) == cur)
                .unwrap_or(0);
            let next_idx = if click.item == egui::Id::new(ACTION_PREV_CUBE) {
                (cur_idx + SWATCH_RGB.len() - 1) % SWATCH_RGB.len()
            } else {
                (cur_idx + 1) % SWATCH_RGB.len()
            };
            let (r, g, b) = SWATCH_RGB[next_idx];
            accent.0 = egui::Color32::from_rgb(r, g, b);
        }
    }
}

// ─── Canvas root view ──────────────────────────────────────────────

fn canvas_root_view(
    ctx: &egui::Context,
    accent: egui::Color32,
    canvas: &mut CanvasViewState,
    shelf_state: &mut ShelfState,
) {
    let shelves = canvas_shelves(accent);
    let shelf_theme = *mara_core::style::theme().shelf();
    let layout = mara_core::layout_shelves(ctx.content_rect(), &shelves, shelf_state, &shelf_theme);

    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(egui::Color32::TRANSPARENT))
        .show(ctx, |ui| {
            let screen_rect = ui.max_rect();
            let canvas_rect = layout.viewport;
            let response = ui.allocate_rect(canvas_rect, egui::Sense::drag());
            let painter = ui.painter_at(screen_rect);

            painter.rect_filled(screen_rect, 0, mara_core::style::theme().palette.bg_panel);
            painter.rect_filled(canvas_rect, 0, mara_core::style::theme().palette.bg_window);

            let grid = 32.0;
            let grid_col = egui::Color32::from_rgba_unmultiplied(
                mara_core::style::on_panel_dim().r(),
                mara_core::style::on_panel_dim().g(),
                mara_core::style::on_panel_dim().b(),
                34,
            );
            let mut x = canvas_rect.left() + grid;
            while x < canvas_rect.right() {
                painter.line_segment(
                    [
                        egui::pos2(x, canvas_rect.top()),
                        egui::pos2(x, canvas_rect.bottom()),
                    ],
                    egui::Stroke::new(1.0, grid_col),
                );
                x += grid;
            }
            let mut y = canvas_rect.top() + grid;
            while y < canvas_rect.bottom() {
                painter.line_segment(
                    [
                        egui::pos2(canvas_rect.left(), y),
                        egui::pos2(canvas_rect.right(), y),
                    ],
                    egui::Stroke::new(1.0, grid_col),
                );
                y += grid;
            }

            if response.drag_started() {
                canvas.strokes.push(Vec::new());
            }
            if response.dragged() || response.drag_started() {
                if let Some(pos) = response
                    .interact_pointer_pos()
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
                    painter.line_segment([points[0], points[1]], egui::Stroke::new(3.0, accent));
                }
            }

            if canvas.strokes.is_empty() {
                painter.text(
                    canvas_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Canvas root view\ndrag to draw",
                    egui::FontId::proportional(24.0),
                    mara_core::style::on_panel_dim(),
                );
            }
        });

    mara_core::show_shelves(ctx, layout, shelves, shelf_state);
}

fn canvas_shelves(accent: egui::Color32) -> Vec<ShelfDef<'static>> {
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

// ─── Per-pane content ──────────────────────────────────────────────

fn cid(pane: &str, suffix: &str) -> egui::Id {
    egui::Id::new((pane, suffix))
}
fn pid(pane: &str, container: &str, idx: usize) -> egui::Id {
    egui::Id::new((pane, container, "pod", idx))
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
    let tree_filter =
        mara_core::pod::Pod::search_query(body.ctx(), search_pod_id, 0).to_lowercase();
    let selected_path: String = body
        .ctx()
        .data(|d| d.get_temp::<String>(tree_root.with("mara_demo_tree_selected")))
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
                    accent_res.0 = srgb_to_egui([c.rgba[0], c.rgba[1], c.rgba[2]]);
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
        "bevy_mara",
        "info",
        vec![
            Pod::new(pid(PANE_ABOUT, "info", 0))
                .with_separator(SeparatorStyle::Line)
                .with_readout("version", env!("CARGO_PKG_VERSION")),
            Pod::new(pid(PANE_ABOUT, "info", 1))
                .with_separator(SeparatorStyle::Line)
                .with_readout("bevy", "0.18"),
            Pod::new(pid(PANE_ABOUT, "info", 2))
                .with_separator(SeparatorStyle::Line)
                .with_readout("bevy_egui", "0.39"),
            Pod::new(pid(PANE_ABOUT, "info", 3))
                .with_separator(SeparatorStyle::None)
                .with_readout("egui", "0.33"),
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

/// **Editor pane** — node graph (top) + code editor (bottom),
/// each in its own container with a fill pod so they soak up the
/// pane's available space. Mirrors the legacy demo's Editor pane,
/// now driven by the vendored `bevy_mara::extras` wrappers.
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
    node_view: &'spec mut NodeViewState,
    graph: &'spec mut Graph<GraphNode>,
    viewer: &'spec mut DemoViewer,
    backend: &'spec mut EframeNodeViewBackend<'_>,
) {
    let cid_graph = cid(PANE_EDITOR, "graph");
    let code_id = cid(PANE_EDITOR, "code_state");

    // Node graph uses the typed `add_node_graph` PaneBody method
    // (feature-gated under `graph`) — internally a `ContainerSpec`
    // with a raw closure, but the closure is owned by mara_core
    // and cannot smuggle arbitrary egui through.
    body.add_node_graph(
        cid_graph,
        "Node graph",
        "flowchart",
        node_view,
        graph,
        viewer,
        backend,
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
    /// Canonical fill colour for pins of this type. Picked from
    /// Unreal Engine's `K2GraphSchema` pin palette
    /// (`UGraphEditorSettings`), gamma-encoded from the engine's
    /// linear `FLinearColor` defaults so the colours match what
    /// you see in the Blueprint editor:
    ///   * Number  → Float        `#A4FF34` (bright lime).
    ///   * Vector  → Vector struct`#FFC247` (gold).
    ///   * Color   → LinearColor  `#FFA0FF` (pink-magenta — UE
    ///     uses the SoftClassRef tone for "pretty colour" type-
    ///     coding when the editor doesn't have a dedicated colour
    ///     pin).
    ///   * Bool    → Bool         `#960000` (deep maroon).
    ///   * Text    → String       `#FF38C9` (hot pink).
    /// Combined with `WireColorMode::FromSource` in
    /// `mara_node_graph_style`, every wire takes the colour of its
    /// source pin uniformly — the "Unreal Blueprint" look.
    fn color(self) -> egui::Color32 {
        match self {
            PinType::Number => egui::Color32::from_rgb(0xA4, 0xFF, 0x34),
            PinType::Vector => egui::Color32::from_rgb(0xFF, 0xC2, 0x47),
            PinType::Color => egui::Color32::from_rgb(0xFF, 0xA0, 0xFF),
            PinType::Bool => egui::Color32::from_rgb(0x96, 0x00, 0x00),
            PinType::Text => egui::Color32::from_rgb(0xFF, 0x38, 0xC9),
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
        let fill = self.color();
        if connected {
            PinInfo::circle()
                .with_fill(fill)
                .with_stroke(egui::Stroke::new(1.0, egui::Color32::from_black_alpha(180)))
        } else {
            PinInfo::circle()
                .with_fill(egui::Color32::TRANSPARENT)
                .with_stroke(egui::Stroke::new(1.5, fill))
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
    Color(egui::Color32),
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
    fn as_color(&self) -> egui::Color32 {
        let to_u8 = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        match self {
            Value::Number(v) => {
                let g = to_u8(*v);
                egui::Color32::from_rgb(g, g, g)
            }
            Value::Vector(v) => egui::Color32::from_rgb(to_u8(v[0]), to_u8(v[1]), to_u8(v[2])),
            Value::Color(c) => *c,
            Value::Bool(b) => {
                if *b {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::BLACK
                }
            }
            Value::Text(_) => egui::Color32::GRAY,
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
enum GraphNode {
    // ── Sources ──
    Number(f64),
    Integer(i64),
    Vector([f64; 3]),
    Color(egui::Color32),
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
    fn color(self) -> egui::Color32 {
        match self {
            Category::Source => egui::Color32::from_rgb(0x82, 0x35, 0x4C), // syntaxn
            Category::ScalarMath => egui::Color32::from_rgb(0x24, 0x62, 0x83), // syntaxv
            Category::Vector => egui::Color32::from_rgb(0x3C, 0x3C, 0x83), // nodeclass_vector
            Category::Color => egui::Color32::from_rgb(0x6E, 0x6E, 0x23),  // syntaxb
            Category::Logic => egui::Color32::from_rgb(0x41, 0x2B, 0x51),  // nodeclass_filter
            Category::Noise => egui::Color32::from_rgb(0x79, 0x46, 0x1D),  // nodeclass_texture
            Category::Sink => egui::Color32::from_rgb(0x3E, 0x23, 0x2A),   // nodeclass_output
        }
    }
}

impl GraphNode {
    /// Small Fluent-UI icon glyph painted to the left of the
    /// title in the header band. Picked from the set bundled in
    /// `mara_core::icons` so missing-glyph fallback never kicks in.
    fn icon_name(&self) -> &'static str {
        match self {
            // Sources
            GraphNode::Number(_) => "calculator",
            GraphNode::Integer(_) => "calculator",
            GraphNode::Vector(_) => "flowchart",
            GraphNode::Color(_) => "color",
            GraphNode::Bool(_) => "checkmark",
            GraphNode::Time => "clock",
            // Scalar math
            GraphNode::ScalarMath(_) => "calculator",
            GraphNode::Trig(_) => "wave",
            GraphNode::Compare(_) => "scales",
            GraphNode::Mix => "merge",
            GraphNode::Clamp => "border-all",
            GraphNode::MapRange => "ruler",
            GraphNode::Smoothstep => "wave",
            GraphNode::Step => "wave",
            // Vector
            GraphNode::VectorMath(_) => "flowchart",
            GraphNode::Compose => "merge",
            GraphNode::Decompose => "split",
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
            GraphNode::Perlin { .. } => "wave",
            GraphNode::WhiteNoise { .. } => "wave",
            GraphNode::Wave(_) => "wave",
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
    fn subtitle(&self) -> String {
        match self {
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

    fn category(&self) -> Category {
        match self {
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

    fn title(&self) -> &'static str {
        match self {
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
            Value::Color(egui::Color32::from_rgb(to_u8(r), to_u8(g), to_u8(b)))
        }
        GraphNode::ColorMix => {
            let a = eval_input_at(graph, time, pin.id.node, 0).as_color();
            let b = eval_input_at(graph, time, pin.id.node, 1).as_color();
            let t = eval_input_at(graph, time, pin.id.node, 2)
                .as_number()
                .clamp(0.0, 1.0) as f32;
            let lerp = |x: u8, y: u8| (x as f32 * (1.0 - t) + y as f32 * t).round() as u8;
            Value::Color(egui::Color32::from_rgba_unmultiplied(
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
            Value::Color(egui::Color32::from_rgb(to_u8(r), to_u8(g), to_u8(b)))
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
            Value::Color(egui::Color32::from_rgba_unmultiplied(
                to_u8(r),
                to_u8(g),
                to_u8(b),
                c.a(),
            ))
        }
        GraphNode::ColorInvert => {
            let c = eval_input_at(graph, time, pin.id.node, 0).as_color();
            Value::Color(egui::Color32::from_rgba_unmultiplied(
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
            Value::Color(egui::Color32::from_rgba_unmultiplied(
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
            Value::Color(egui::Color32::from_rgba_unmultiplied(
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
    node: mara_core::extras::graph::NodeId,
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

#[derive(Default)]
struct DemoViewer {
    /// Wall-clock seconds since startup, refreshed each frame by
    /// the editor pane. Threaded into `eval_output` so `Time` /
    /// `Perlin` nodes animate live.
    time: f64,
}

impl NodeViewer<GraphNode> for DemoViewer {
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
        default: egui::Frame,
        node: mara_core::extras::graph::NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        graph: &Graph<GraphNode>,
    ) -> egui::Frame {
        let Some(n) = graph.get_node(node) else {
            return default;
        };
        let tint = n.category().color();
        // Translucent tint — the dark body fill below shows
        // through, knocking the saturation down so the seven
        // category colours all sit at a consistent luminance the
        // way Blender's `node_class` palette does. Alpha 0xB0
        // (~69 %) lands roughly where Blender's headers sit
        // visually against the `#303030` body.
        default
            .fill(egui::Color32::from_rgba_unmultiplied(
                tint.r(),
                tint.g(),
                tint.b(),
                0xB0,
            ))
            .stroke(egui::Stroke::NONE)
    }

    /// Two-line header content: [icon] [title / subtitle], laid
    /// out like an Unreal Blueprint title bar — main title in
    /// near-white, smaller dim subtitle underneath, and a
    /// category-coloured Fluent-UI icon glyph on the left so the
    /// node is identifiable at a glance even when zoomed out.
    fn show_header(
        &mut self,
        node: mara_core::extras::graph::NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        graph: &mut Graph<GraphNode>,
    ) {
        let Some(n) = graph.get_node(node).cloned() else {
            return;
        };
        let title_color = egui::Color32::from_rgb(0xEE, 0xEE, 0xEE);
        let subtitle_color = egui::Color32::from_rgba_unmultiplied(0xEE, 0xEE, 0xEE, 0xB0);

        // No header width clamp — title bar sizes to its
        // natural content width (icon + title + subtitle).
        // The whole node ends up `max(title_w, every_pin_row_w,
        // body_w)`, exactly UE Slate's behaviour.

        // With graph's header layout fixed to `top_down(Min)`,
        // a normal horizontal block is left-anchored as expected.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let no_wrap = egui::TextWrapMode::Extend;

            // Icon — large enough to span both text rows so it
            // visually centres against the title+subtitle stack.
            if let Some(rt) = mara_core::icons::icon_text(n.icon_name(), 22.0, title_color) {
                ui.add(egui::Label::new(rt).wrap_mode(no_wrap).selectable(false));
            } else {
                ui.add(
                    egui::Label::new(egui::RichText::new("•").size(22.0).color(title_color))
                        .wrap_mode(no_wrap)
                        .selectable(false),
                );
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(n.title())
                            .strong()
                            .size(13.0)
                            .color(title_color),
                    )
                    .wrap_mode(no_wrap)
                    .selectable(false),
                );
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(n.subtitle())
                            .size(10.0)
                            .color(subtitle_color),
                    )
                    .wrap_mode(no_wrap)
                    .selectable(false),
                );
            });
        });
    }

    fn show_input(
        &mut self,
        pin: &InPin,
        ui: &mut egui::Ui,
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
        ui: &mut egui::Ui,
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
        node: mara_core::extras::graph::NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        graph: &mut Graph<GraphNode>,
    ) {
        // No body width clamp — body sizes to its content like
        // UE Slate's natural measurement. Inline widgets below
        // are wrapped in fixed-width slots so the body can't
        // grow per-frame when a value changes.

        let time = self.time;
        let accent = mara_core::style::active_accent();
        let Some(n) = graph.get_node_mut(node) else {
            return;
        };
        match n {
            // ── Source-node value editors (UE: Make-* nodes) ──
            GraphNode::Number(v) => {
                let h = mara_core::widget::drag_value::DRAG_VALUE_ROW_H;
                ui.allocate_ui_with_layout(
                    egui::vec2(125.0, h),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        mara_core::widget::drag_value::drag_value(
                            ui,
                            "",
                            v,
                            0.05,
                            f64::MIN..=f64::MAX,
                            2,
                            "",
                        );
                    },
                );
            }
            GraphNode::Integer(i) => {
                let h = mara_core::widget::drag_value::DRAG_VALUE_ROW_H;
                let mut tmp = *i as f64;
                ui.allocate_ui_with_layout(
                    egui::vec2(125.0, h),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        mara_core::widget::drag_value::drag_value(
                            ui,
                            "",
                            &mut tmp,
                            1.0,
                            f64::MIN..=f64::MAX,
                            0,
                            "",
                        );
                    },
                );
                *i = tmp as i64;
            }
            GraphNode::Vector(v) => {
                let h = mara_core::widget::drag_value::DRAG_VALUE_ROW_H;
                for (axis, comp) in ["x", "y", "z"].iter().zip(v.iter_mut()) {
                    ui.allocate_ui_with_layout(
                        egui::vec2(125.0, h),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            mara_core::widget::drag_value::drag_value(
                                ui,
                                axis,
                                comp,
                                0.05,
                                f64::MIN..=f64::MAX,
                                2,
                                "",
                            );
                        },
                    );
                }
            }
            GraphNode::Color(c) => {
                ui.color_edit_button_srgba(c);
            }
            GraphNode::Bool(b) => {
                let h = mara_core::widget::toggle::TOGGLE_ROW_H;
                ui.allocate_ui_with_layout(
                    egui::vec2(125.0, h),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        mara_core::widget::toggle::toggle(ui, "", b, accent);
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
                ui.allocate_ui_with_layout(
                    egui::vec2(SLOT_W, drag_h),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        mara_core::widget::drag_value::drag_value(
                            ui,
                            "seed",
                            &mut seed_f,
                            1.0,
                            0.0..=u32::MAX as f64,
                            0,
                            "",
                        );
                    },
                );
                *seed = seed_f as u32;
                ui.allocate_ui_with_layout(
                    egui::vec2(SLOT_W, drag_h * 2.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        mara_core::widget::slider::slider(
                            ui,
                            "freq",
                            frequency,
                            0.05..=8.0,
                            2,
                            "",
                            accent,
                        );
                    },
                );
            }
            GraphNode::WhiteNoise { seed } => {
                const SLOT_W: f32 = 140.0;
                let drag_h = mara_core::widget::drag_value::DRAG_VALUE_ROW_H;
                let mut seed_f = *seed as f64;
                ui.allocate_ui_with_layout(
                    egui::vec2(SLOT_W, drag_h),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        mara_core::widget::drag_value::drag_value(
                            ui,
                            "seed",
                            &mut seed_f,
                            1.0,
                            0.0..=u32::MAX as f64,
                            0,
                            "",
                        );
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
                use egui_plot::{Line, Plot, PlotPoints};
                const HISTORY: usize = 256;
                let v = eval_input_at(graph, time, node, 0).as_number();
                let key = egui::Id::new(("mara_demo_plotxy", node));
                let mut buf: Vec<f64> = ui
                    .ctx()
                    .data(|d| d.get_temp::<Vec<f64>>(key))
                    .unwrap_or_default();
                if buf.len() >= HISTORY {
                    buf.remove(0);
                }
                buf.push(v);
                ui.ctx().data_mut(|d| d.insert_temp(key, buf.clone()));
                let points: PlotPoints = buf
                    .iter()
                    .enumerate()
                    .map(|(i, y)| [i as f64, *y])
                    .collect();
                let line = Line::new(format!("plot_{:?}", node), points)
                    .color(egui::Color32::from_rgb(0xA4, 0xFF, 0x34))
                    .width(1.5);
                Plot::new(("mara_demo_plot", node))
                    .height(80.0)
                    .width(220.0)
                    .show_axes([false, true])
                    .show_grid([false, true])
                    .allow_drag(false)
                    .allow_zoom(false)
                    .allow_scroll(false)
                    .auto_bounds(egui::Vec2b::TRUE)
                    .show(ui, |plot_ui| {
                        plot_ui.line(line);
                    });
                ui.ctx().request_repaint();
            }
            GraphNode::Preview => {
                let c = eval_input_at(graph, time, node, 0).as_color();
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(96.0, 40.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 4.0, c);
                ui.painter().rect_stroke(
                    rect,
                    4.0,
                    egui::Stroke::new(1.0, egui::Color32::from_gray(80)),
                    egui::StrokeKind::Inside,
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
                let key = egui::Id::new(("mara_demo_noise_image", node));
                // Cache the previous frame's parameters so we
                // only regenerate the texture when something
                // actually changed (otherwise this would burn a
                // 96×64 hash + tessellator update every frame).
                let prev = ui.ctx().data(|d| d.get_temp::<(u32, u64, u64)>(key));
                let scale_bits = scale.to_bits();
                let offset_bits = offset.to_bits();
                let new_state = (seed, scale_bits, offset_bits);
                let needs_redraw = prev != Some(new_state);

                if needs_redraw {
                    let mut pixels = vec![egui::Color32::BLACK; W * H];
                    for j in 0..H {
                        for i in 0..W {
                            let x = (i as f64) * scale + offset;
                            let y = (j as f64) * scale;
                            let n = sample_2d_value_noise(seed, x, y);
                            // 0..1 → grey, then accent-tint it.
                            let g = (n * 255.0).clamp(0.0, 255.0) as u8;
                            pixels[j * W + i] = egui::Color32::from_rgb(
                                ((g as u16 * 0xA4) / 255) as u8,
                                ((g as u16 * 0xFF) / 255) as u8,
                                ((g as u16 * 0x34) / 255) as u8,
                            );
                        }
                    }
                    let img = egui::ColorImage {
                        size: [W, H],
                        pixels,
                        source_size: egui::vec2(W as f32, H as f32),
                    };
                    let tex = ui.ctx().load_texture(
                        format!("mara_demo_noise_{:?}", node),
                        img,
                        egui::TextureOptions::NEAREST,
                    );
                    let tex_key = key.with("tex");
                    ui.ctx().data_mut(|d| {
                        d.insert_temp::<egui::TextureHandle>(tex_key, tex);
                        d.insert_temp::<(u32, u64, u64)>(key, new_state);
                    });
                }
                let tex_key = key.with("tex");
                if let Some(tex) = ui
                    .ctx()
                    .data(|d| d.get_temp::<egui::TextureHandle>(tex_key))
                {
                    let resp = ui.add(
                        egui::Image::new((tex.id(), egui::vec2(W as f32 * 1.5, H as f32 * 1.5)))
                            .corner_radius(egui::CornerRadius::same(3))
                            .sense(egui::Sense::hover()),
                    );
                    let _ = resp;
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
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new("=")
                                .monospace()
                                .size(12.0)
                                .color(egui::Color32::from_gray(170)),
                        )
                        .selectable(false),
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
                let key = egui::Id::new(("mara_demo_noise_field", node));
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
                    .ctx()
                    .data(|d| d.get_temp::<(u32, u32, u64, u64, u64, u8, u64, u64, u64)>(key));
                if prev != Some(new_state) {
                    let mut pixels = vec![egui::Color32::BLACK; W * H];
                    let scale = 0.04 * freq;
                    let g_pow = gain;
                    for j in 0..H {
                        for i in 0..W {
                            let x = (i as f64) * scale + off_x;
                            let y = (j as f64) * scale + off_y;
                            let n = mode.sample(seed, x, y, octaves, pers, lac);
                            let n = n.clamp(0.0, 1.0).powf(g_pow);
                            let g = (n * 255.0).clamp(0.0, 255.0) as u8;
                            pixels[j * W + i] = egui::Color32::from_rgb(
                                ((g as u16 * 0xA4) / 255) as u8,
                                ((g as u16 * 0xFF) / 255) as u8,
                                ((g as u16 * 0x34) / 255) as u8,
                            );
                        }
                    }
                    let img = egui::ColorImage {
                        size: [W, H],
                        pixels,
                        source_size: egui::vec2(W as f32, H as f32),
                    };
                    let tex = ui.ctx().load_texture(
                        format!("mara_demo_noise_field_{:?}", node),
                        img,
                        egui::TextureOptions::NEAREST,
                    );
                    let tex_key = key.with("tex");
                    ui.ctx().data_mut(|d| {
                        d.insert_temp::<egui::TextureHandle>(tex_key, tex);
                        d.insert_temp::<(u32, u32, u64, u64, u64, u8, u64, u64, u64)>(
                            key, new_state,
                        );
                    });
                }
                let tex_key = key.with("tex");
                if let Some(tex) = ui
                    .ctx()
                    .data(|d| d.get_temp::<egui::TextureHandle>(tex_key))
                {
                    ui.add(
                        egui::Image::new((tex.id(), egui::vec2(W as f32 * 1.4, H as f32 * 1.4)))
                            .corner_radius(egui::CornerRadius::same(3))
                            .sense(egui::Sense::hover()),
                    );
                }
            }
            // ── 4-channel oscilloscope plot — each input
            //    rendered as its own coloured line. ──
            GraphNode::MultiPlot => {
                use egui_plot::{Line, Plot, PlotPoints};
                const HISTORY: usize = 256;
                const COLORS: [egui::Color32; 4] = [
                    egui::Color32::from_rgb(0xA4, 0xFF, 0x34), // lime (Float)
                    egui::Color32::from_rgb(0xFF, 0xC2, 0x47), // gold (Vector)
                    egui::Color32::from_rgb(0xFF, 0xA0, 0xFF), // pink
                    egui::Color32::from_rgb(0x6E, 0xC0, 0xFF), // cyan
                ];
                let key = egui::Id::new(("mara_demo_multiplot", node));
                let mut buf: Vec<[f64; 4]> = ui
                    .ctx()
                    .data(|d| d.get_temp::<Vec<[f64; 4]>>(key))
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
                ui.ctx().data_mut(|d| d.insert_temp(key, buf.clone()));

                Plot::new(("mara_demo_multiplot", node))
                    .height(110.0)
                    .width(260.0)
                    .show_axes([false, true])
                    .show_grid([true, true])
                    .allow_drag(false)
                    .allow_zoom(false)
                    .allow_scroll(false)
                    .auto_bounds(egui::Vec2b::TRUE)
                    .show(ui, |plot_ui| {
                        for ch in 0..4 {
                            let pts: PlotPoints = buf
                                .iter()
                                .enumerate()
                                .map(|(i, s)| [i as f64, s[ch]])
                                .collect();
                            plot_ui.line(
                                Line::new(format!("ch{ch}_{:?}", node), pts)
                                    .color(COLORS[ch])
                                    .width(1.5),
                            );
                        }
                    });
                ui.ctx().request_repaint();
            }
            GraphNode::VectorPreview => {
                let v = eval_input_at(graph, time, node, 0).as_vector();
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(140.0, 40.0), egui::Sense::hover());
                let painter = ui.painter();
                painter.rect_filled(rect, 3.0, egui::Color32::from_black_alpha(40));
                let bar_h = (rect.height() - 8.0) / 3.0;
                let max = v[0].abs().max(v[1].abs()).max(v[2].abs()).max(1.0) as f32;
                let colors = [
                    egui::Color32::from_rgb(0xFF, 0x33, 0x52), // x = red
                    egui::Color32::from_rgb(0x8B, 0xDC, 0x00), // y = green
                    egui::Color32::from_rgb(0x28, 0x90, 0xFF), // z = blue
                ];
                for (i, comp) in v.iter().enumerate() {
                    let y0 = rect.top() + 4.0 + (i as f32) * bar_h;
                    let centre_x = rect.center().x;
                    let len = (*comp as f32 / max) * (rect.width() * 0.5 - 8.0);
                    let bar_rect = egui::Rect::from_min_max(
                        egui::pos2(centre_x.min(centre_x + len), y0 + 2.0),
                        egui::pos2(centre_x.max(centre_x + len), y0 + bar_h - 2.0),
                    );
                    painter.rect_filled(bar_rect, 1.0, colors[i]);
                }
                // Centre rule
                painter.line_segment(
                    [
                        egui::pos2(rect.center().x, rect.top() + 2.0),
                        egui::pos2(rect.center().x, rect.bottom() - 2.0),
                    ],
                    egui::Stroke::new(1.0, egui::Color32::from_gray(120)),
                );
            }
            _ => {}
        }
    }

    fn has_graph_menu(&mut self, _: egui::Pos2, _: &mut Graph<GraphNode>) -> bool {
        true
    }
    fn show_graph_menu(
        &mut self,
        pos: egui::Pos2,
        ui: &mut egui::Ui,
        graph: &mut Graph<GraphNode>,
    ) {
        ui.set_min_width(180.0);
        ui.label(egui::RichText::new("Add node").strong());
        ui.separator();

        let mut spawn = |ui: &mut egui::Ui, label: &str, n: GraphNode| {
            if ui.button(label).clicked() {
                graph.insert_node(pos, n);
                ui.close();
            }
        };

        ui.menu_button("Sources", |ui| {
            spawn(ui, "Number", GraphNode::Number(0.0));
            spawn(ui, "Integer", GraphNode::Integer(0));
            spawn(ui, "Vector", GraphNode::Vector([0.0; 3]));
            spawn(
                ui,
                "Color",
                GraphNode::Color(egui::Color32::from_rgb(180, 200, 220)),
            );
            spawn(ui, "Bool", GraphNode::Bool(false));
            spawn(ui, "Time", GraphNode::Time);
        });
        ui.menu_button("Scalar math", |ui| {
            spawn(ui, "Scalar Math", GraphNode::ScalarMath(ScalarOp::Add));
            spawn(ui, "Math Func", GraphNode::Trig(TrigFn::Sin));
            spawn(ui, "Compare", GraphNode::Compare(CompareOp::Lt));
            spawn(ui, "Mix", GraphNode::Mix);
            spawn(ui, "Clamp", GraphNode::Clamp);
            spawn(ui, "Map Range", GraphNode::MapRange);
            spawn(ui, "Smoothstep", GraphNode::Smoothstep);
            spawn(ui, "Step", GraphNode::Step);
        });
        ui.menu_button("Vector", |ui| {
            spawn(ui, "Vector Math", GraphNode::VectorMath(VectorOp::Add));
            spawn(ui, "Compose", GraphNode::Compose);
            spawn(ui, "Decompose", GraphNode::Decompose);
            spawn(ui, "Length", GraphNode::Length);
            spawn(ui, "Dot Product", GraphNode::Dot);
            spawn(ui, "Distance", GraphNode::Distance);
            spawn(ui, "Normalize", GraphNode::Normalize);
            spawn(ui, "Vector Rotate", GraphNode::VectorRotate);
            spawn(ui, "Reflect", GraphNode::Reflect);
        });
        ui.menu_button("Color", |ui| {
            spawn(ui, "RGB → Color", GraphNode::RgbToColor);
            spawn(ui, "HSV → Color", GraphNode::HsvToColor);
            spawn(ui, "Color Mix", GraphNode::ColorMix);
            spawn(ui, "Hue Shift", GraphNode::HueShift);
            spawn(ui, "Invert", GraphNode::ColorInvert);
            spawn(ui, "Bright/Contrast", GraphNode::BrightContrast);
            spawn(ui, "Gamma", GraphNode::Gamma);
        });
        ui.menu_button("Logic", |ui| {
            spawn(ui, "If / Else", GraphNode::IfElse);
            spawn(ui, "Boolean Math", GraphNode::BooleanMath(BoolOp::And));
            spawn(ui, "Float → Bool", GraphNode::FloatToBool);
            spawn(ui, "Bool → Float", GraphNode::BoolToFloat);
        });
        ui.menu_button("Noise / Wave", |ui| {
            spawn(
                ui,
                "Perlin",
                GraphNode::Perlin {
                    seed: 12345,
                    frequency: 1.0,
                },
            );
            spawn(ui, "White Noise", GraphNode::WhiteNoise { seed: 12345 });
            spawn(ui, "Wave", GraphNode::Wave(WaveShape::Sine));
        });
        ui.menu_button("Sinks", |ui| {
            spawn(ui, "Display", GraphNode::Display);
            spawn(ui, "Plot", GraphNode::Plot);
            spawn(ui, "Plot XY", GraphNode::PlotXY);
            spawn(ui, "Preview", GraphNode::Preview);
            spawn(ui, "Vector Preview", GraphNode::VectorPreview);
            spawn(
                ui,
                "Noise Image",
                GraphNode::NoiseImage {
                    seed: 0xCAFE,
                    scale: 0.05,
                },
            );
            spawn(ui, "Noise Field", GraphNode::NoiseField);
            spawn(ui, "Multi Plot", GraphNode::MultiPlot);
            spawn(ui, "Output", GraphNode::Output);
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
    ui: &mut egui::Ui,
) {
    // Stable-width placeholders matching `inline_value_readout`'s
    // column counts, so swapping between connected (live readout)
    // and unconnected (placeholder) doesn't change the row width.
    let placeholder = match ty {
        PinType::Number => format!("{:>9}", "—"),
        PinType::Vector => format!("[{:>7}, {:>7}, {:>7}]", "—", "—", "—"),
        PinType::Color => format!("{:>9}", "—"),
        PinType::Bool => format!("{:>4}", "—"),
        PinType::Text => format!("{:<9}", "—"),
    };
    ui.add(
        egui::Label::new(
            egui::RichText::new(placeholder)
                .monospace()
                .size(11.0)
                .color(egui::Color32::from_gray(140)),
        )
        .wrap_mode(egui::TextWrapMode::Extend)
        .selectable(false),
    );
}

fn inline_value_readout(v: &Value, ty: PinType, ui: &mut egui::Ui) {
    // Monospace + right-aligned fixed-width formatting so the
    // node body width doesn't reflow when a value's digit count
    // changes (e.g. `0.123` → `12.345`). Each readout reserves
    // the same number of glyph columns regardless of magnitude.
    let mut mono = |text: String| {
        ui.add(
            egui::Label::new(
                egui::RichText::new(text)
                    .monospace()
                    .size(11.0)
                    .color(egui::Color32::from_gray(200)),
            )
            .wrap_mode(egui::TextWrapMode::Extend)
            .selectable(false),
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
            let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 14.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, *c);
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
fn op_dropdown<T>(ui: &mut egui::Ui, current: &mut T, options: &[(&str, T)])
where
    T: Copy + PartialEq,
{
    let mut idx = options
        .iter()
        .position(|(_, v)| *v == *current)
        .unwrap_or(0);
    let labels: Vec<&str> = options.iter().map(|(l, _)| *l).collect();
    let accent = mara_core::style::active_accent();
    const SLOT_W: f32 = 140.0;
    let h = mara_core::widget::dropdown::DROPDOWN_ROW_H;
    let mut changed = false;
    ui.allocate_ui_with_layout(
        egui::vec2(SLOT_W, h),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            let resp = mara_core::widget::dropdown::dropdown(
                ui,
                ("mara_demo_op_dropdown", current as *const T as usize),
                &mut idx,
                &labels,
                accent,
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
    node: mara_core::extras::graph::NodeId,
    ui: &mut egui::Ui,
    current: f64,
) {
    let _ = graph; // signature parity for future inline-editor use
    const HISTORY: usize = 96;
    let key = egui::Id::new(("mara_demo_sparkline", node));
    let mut buf: Vec<f32> = ui
        .ctx()
        .data(|d| d.get_temp::<Vec<f32>>(key))
        .unwrap_or_default();
    if buf.len() >= HISTORY {
        buf.remove(0);
    }
    buf.push(current as f32);
    ui.ctx().data_mut(|d| d.insert_temp(key, buf.clone()));

    let label = format!("{current:.3}");
    ui.label(egui::RichText::new(label).monospace());

    let (rect, _) = ui.allocate_exact_size(egui::vec2(140.0, 36.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, egui::Color32::from_black_alpha(40));

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
            points.push(egui::pos2(x, y));
        }
        painter.add(egui::epaint::PathShape::line(
            points,
            egui::Stroke::new(1.5, egui::Color32::from_rgb(0xFF, 0xB9, 0x38)),
        ));
    }
    ui.ctx().request_repaint();
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
fn default_graph() -> Graph<GraphNode> {
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
    let t = g.insert_node(egui::pos2(col(0), row(0.0)), GraphNode::Time);
    let freq = g.insert_node(egui::pos2(col(0), row(170.0)), GraphNode::Number(1.5));
    let mul = g.insert_node(
        egui::pos2(col(1), row(60.0)),
        GraphNode::ScalarMath(ScalarOp::Mul),
    );
    let sin = g.insert_node(egui::pos2(col(2), row(60.0)), GraphNode::Trig(TrigFn::Sin));
    let half = g.insert_node(egui::pos2(col(2), row(230.0)), GraphNode::Number(0.5));
    let bias = g.insert_node(
        egui::pos2(col(3), row(60.0)),
        GraphNode::ScalarMath(ScalarOp::Mul),
    );
    let half2 = g.insert_node(egui::pos2(col(3), row(230.0)), GraphNode::Number(0.5));
    let lift = g.insert_node(
        egui::pos2(col(4), row(60.0)),
        GraphNode::ScalarMath(ScalarOp::Add),
    );
    let display = g.insert_node(egui::pos2(col(5), row(60.0)), GraphNode::Display);

    // ── Colour mix branch (below pipeline 1, fed by `lift`) ──
    //
    //  col 3                       col 4         col 5
    //  Color(red)
    //  Color(blue)                 ColorMix      Preview
    //
    let red = g.insert_node(
        egui::pos2(col(3), row(480.0)),
        GraphNode::Color(egui::Color32::from_rgb(0xE0, 0x6C, 0x4F)),
    );
    let blue = g.insert_node(
        egui::pos2(col(3), row(620.0)),
        GraphNode::Color(egui::Color32::from_rgb(0x4D, 0xA8, 0xDA)),
    );
    let cmix = g.insert_node(egui::pos2(col(4), row(540.0)), GraphNode::ColorMix);
    let preview = g.insert_node(egui::pos2(col(5), row(540.0)), GraphNode::Preview);

    // ── Pipeline 2: vector → length → output ──
    //
    //  col 0          col 1     col 2
    //  Vector ────→   Length ──→ Output
    //
    let vec = g.insert_node(
        egui::pos2(col(0), row(920.0)),
        GraphNode::Vector([1.0, 2.0, 3.0]),
    );
    let len = g.insert_node(egui::pos2(col(1), row(920.0)), GraphNode::Length);
    let out = g.insert_node(egui::pos2(col(2), row(920.0)), GraphNode::Output);

    // ── Pipeline 3: noise → compare → ifelse → display ──
    //
    //  col 0     col 1                col 2                          col 3
    //  Perlin    Compare              IfElse                         Display
    //            Num(0)               Num(+1), Num(-1)
    //
    let perlin = g.insert_node(
        egui::pos2(col(0), row(1180.0)),
        GraphNode::Perlin {
            seed: 0xCAFE,
            frequency: 1.5,
        },
    );
    let zero = g.insert_node(egui::pos2(col(1), row(1320.0)), GraphNode::Number(0.0));
    let cmp = g.insert_node(
        egui::pos2(col(1), row(1180.0)),
        GraphNode::Compare(CompareOp::Gt),
    );
    let one = g.insert_node(egui::pos2(col(2), row(1320.0)), GraphNode::Number(1.0));
    let neg = g.insert_node(egui::pos2(col(2), row(1460.0)), GraphNode::Number(-1.0));
    let gate = g.insert_node(egui::pos2(col(2), row(1180.0)), GraphNode::IfElse);
    let display2 = g.insert_node(egui::pos2(col(3), row(1180.0)), GraphNode::Display);

    // ── Pipeline 4: sophisticated 4-channel scope ──
    //
    //  col 0     col 1       col 2          col 3
    //  Time ─→ ×freq[0] ─→  sin → ─┐
    //                              ├─→ MultiPlot  (4 lines)
    //  Time ─→ ×freq[1] ─→  cos →  │
    //  Time ─→ ×freq[2] ─→  sin² ─ │
    //  Time ─→ ×freq[3] ─→  saw ─  ┘
    //
    let t2 = g.insert_node(egui::pos2(col(0), row(1620.0)), GraphNode::Time);
    let f1 = g.insert_node(egui::pos2(col(0), row(1760.0)), GraphNode::Number(1.0));
    let f2 = g.insert_node(egui::pos2(col(0), row(1900.0)), GraphNode::Number(2.0));
    let f3 = g.insert_node(egui::pos2(col(0), row(2040.0)), GraphNode::Number(3.0));
    let f4 = g.insert_node(egui::pos2(col(0), row(2180.0)), GraphNode::Number(0.5));
    let m1 = g.insert_node(
        egui::pos2(col(1), row(1620.0)),
        GraphNode::ScalarMath(ScalarOp::Mul),
    );
    let m2 = g.insert_node(
        egui::pos2(col(1), row(1760.0)),
        GraphNode::ScalarMath(ScalarOp::Mul),
    );
    let m3 = g.insert_node(
        egui::pos2(col(1), row(1900.0)),
        GraphNode::ScalarMath(ScalarOp::Mul),
    );
    let m4 = g.insert_node(
        egui::pos2(col(1), row(2040.0)),
        GraphNode::ScalarMath(ScalarOp::Mul),
    );
    let s1 = g.insert_node(
        egui::pos2(col(2), row(1620.0)),
        GraphNode::Trig(TrigFn::Sin),
    );
    let s2 = g.insert_node(
        egui::pos2(col(2), row(1760.0)),
        GraphNode::Trig(TrigFn::Cos),
    );
    let s3 = g.insert_node(
        egui::pos2(col(2), row(1900.0)),
        GraphNode::Wave(WaveShape::Triangle),
    );
    let s4 = g.insert_node(
        egui::pos2(col(2), row(2040.0)),
        GraphNode::Wave(WaveShape::Saw),
    );
    let mplot = g.insert_node(egui::pos2(col(3), row(1620.0)), GraphNode::MultiPlot);

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
    let t3 = g.insert_node(egui::pos2(col(0), row(2400.0)), GraphNode::Time);
    let speed = g.insert_node(egui::pos2(col(0), row(2540.0)), GraphNode::Number(0.4));
    let drift_x = g.insert_node(
        egui::pos2(col(1), row(2400.0)),
        GraphNode::ScalarMath(ScalarOp::Mul),
    );
    let drift_y = g.insert_node(egui::pos2(col(1), row(2540.0)), GraphNode::Number(0.0));
    let freq_n = g.insert_node(egui::pos2(col(1), row(2680.0)), GraphNode::Number(1.5));
    let seed_n = g.insert_node(
        egui::pos2(col(1), row(2820.0)),
        GraphNode::Number(0xCAFE as f64),
    );
    let oct_n = g.insert_node(egui::pos2(col(1), row(2960.0)), GraphNode::Number(5.0));
    let pers_n = g.insert_node(egui::pos2(col(1), row(3100.0)), GraphNode::Number(0.55));
    let lac_n = g.insert_node(egui::pos2(col(1), row(3240.0)), GraphNode::Number(2.1));
    let gain_n = g.insert_node(egui::pos2(col(1), row(3380.0)), GraphNode::Number(1.0));
    let nfield = g.insert_node(egui::pos2(col(2), row(2400.0)), GraphNode::NoiseField);

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

    g
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
    egui::Color32,
);

const DEMO_TREE: &[DemoTreeRow] = &[
    (
        "/World",
        "World",
        "folder",
        &["/World/Robot", "/World/Lights"],
        egui::Color32::from_rgb(0x55, 0x6E, 0x9C),
    ),
    (
        "/World/Robot",
        "Robot",
        "person",
        &["/World/Robot/base", "/World/Robot/arm"],
        egui::Color32::from_rgb(0xE0, 0x6C, 0x4F),
    ),
    (
        "/World/Robot/base",
        "base",
        "code",
        &[],
        egui::Color32::from_rgb(0x4D, 0xA8, 0xDA),
    ),
    (
        "/World/Robot/arm",
        "arm",
        "code",
        &["/World/Robot/arm/grip"],
        egui::Color32::from_rgb(0xE6, 0xB7, 0x3D),
    ),
    (
        "/World/Robot/arm/grip",
        "grip",
        "code",
        &[],
        egui::Color32::from_rgb(0x9C, 0x55, 0xC0),
    ),
    (
        "/World/Lights",
        "Lights",
        "image",
        &["/World/Lights/sun"],
        egui::Color32::from_rgb(0xF5, 0xC2, 0x42),
    ),
    (
        "/World/Lights/sun",
        "sun",
        "image",
        &[],
        egui::Color32::from_rgb(0xFF, 0xE5, 0x6B),
    ),
];

fn demo_tree_node(path: &str) -> Option<&'static DemoTreeRow> {
    DEMO_TREE.iter().find(|(p, _, _, _, _)| *p == path)
}

fn demo_tree(
    tree: &mut mara_core::widget::TreeBody,
    root_id: egui::Id,
    accent: egui::Color32,
    filter: &str,
) {
    let sel_key = root_id.with("mara_demo_tree_selected");
    let mut selected: String = tree
        .ctx()
        .data(|d| d.get_temp::<String>(sel_key))
        .unwrap_or_default();
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
        tree.ctx().data_mut(|d| d.insert_temp(sel_key, selected));
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
    root_id: egui::Id,
    path: &'static str,
    depth: u32,
    selected: &str,
    accent: egui::Color32,
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
    let mut expanded: bool = tree
        .ctx()
        .data_mut(|d| d.get_persisted::<bool>(exp_key))
        .unwrap_or(true);
    let mut eye_on: bool = tree
        .ctx()
        .data_mut(|d| d.get_persisted::<bool>(eye_key))
        .unwrap_or(true);
    let mut lock_on: bool = tree
        .ctx()
        .data_mut(|d| d.get_persisted::<bool>(lock_key))
        .unwrap_or(false);
    let mut swatch_dummy = false;

    let mut slots = [
        TreeIconSlot::new(TreeIconKind::Eye, &mut eye_on).with_tooltip("Toggle visibility"),
        TreeIconSlot::new(TreeIconKind::Lock, &mut lock_on).with_tooltip("Toggle lock"),
        TreeIconSlot::new(TreeIconKind::Color(*material), &mut swatch_dummy)
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
    if resp.body.clicked() {
        *clicked = Some((*p).to_string());
    }

    tree.ctx().data_mut(|d| {
        d.insert_persisted(exp_key, expanded);
        d.insert_persisted(eye_key, eye_on);
        d.insert_persisted(lock_key, lock_on);
    });

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
