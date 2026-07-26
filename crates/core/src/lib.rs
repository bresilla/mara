//! # mara_core — modular UI core for `bevy_mara`
//!
//! Framework-agnostic UI core for Mara. Self-contained: ships its
//! own bundled Iosevka fonts, theme runtime, ribbon strip, shelf
//! system, pane system, and reusable widget contracts.
//!
//! Naming note: directory is `crates/core/` but the crate identifier
//! is `mara_core`. Naming the package `core` would shadow Rust's std
//! `core`, breaking derive macros that expand to `::core::clone::Clone`.
//!
//! ## Modules
//!
//! * [`pane`] — floating pane with anchored positioning, theme-
//!   aware title strip, and GAME / PRO visuals. Layout is plain
//!   egui (no flex).
//! * [`container`] — in-pane content blocks (`Normal`, `Tabbed`).
//!   A container's body accepts a [`pod::Pod`] — never raw
//!   widgets / closures.
//! * [`pod`] — composable content units; the only thing a
//!   container's body accepts. Built-ins so far: `SearchPod`.
//! * [`widget`] — mara-styled widgets (`text_input`, …).
//! * [`style`] — theme + colour + font runtime. Mara host/facade code
//!   wires the active `Theme` into the current backend; `set_theme`
//!   swaps the global theme.
//! * [`ribbon`] — edge button strips.
//! * [`shelf`] — persistent docked tabbed container regions that
//!   reserve viewport space.
//! * [`icons`] — Fluent UI System Icon glyph painter.

pub mod app_shell;
#[doc(hidden)]
pub mod backend;
pub mod command_palette;
pub mod container;
pub(crate) mod debug;
pub mod embed;
pub mod enforce;
pub mod focus;
pub mod icons;
pub mod layer;
pub mod layout;
pub mod memory;
pub mod module;
pub mod mui;
pub mod paint;
pub mod pane;
pub mod pod;
pub mod popup;
pub mod probe;
pub mod ribbon;
pub(crate) mod scroll;
pub mod scroll_state;
pub mod shelf;
pub mod shell;
pub mod style;
pub mod text_edit;
pub mod themes;
pub mod view;
pub mod vocab;
pub mod widget;
pub mod window_chrome;
pub mod workspace;

pub use layout::{Layer, Sense as MaraSense, UiBackend};
pub use memory::{MaraMemory, MaraMemoryCtx};
pub use mui::{MaraInput, MaraKey, MaraKeySet, MaraPainter, MaraResponse, MaraUi};
pub use paint::{PaintCmd, PaintList};
// `vocab` is deliberately NOT glob-re-exported at the root: hosts
// like Bevy glob-import both their own prelude and `mara_core::*`,
// and egui's `Vec2`/`Rect` would silently shadow the host's math
// types. Consumers reach the data vocabulary via
// `use mara_core::vocab::{Color32, Rect, ...}` (or `mara::ui::vocab`).

// Foundational row-height unit — re-exported at crate root so the
// canonical name is `mara_core::UNIT`. Every widget is sized in
// multiples of this. See [`style::UNIT`] for the definition.
pub use style::{BODY_FONT_SIZE, UNIT};

// ─── Top-level convenience re-exports ─────────────────────────────
//
// `bevy_mara::prelude::*` glob-imports `mara_core::*`, so anything
// re-exported here surfaces directly under the consumer's prelude
// (`use bevy_mara::prelude::*;` → `RibbonOpen`, `AccentColor`, …
// in scope). Keeping these callable at the crate root means apps
// can use the concise `mara_core::*` / facade prelude surface
// without needing deeply nested imports.

pub use app_shell::{
    AppMenuPolicy, AppShellChrome, AppShellError, AppShellResolution, ResolvedRibbon,
    WindowControlsPolicy, dispatch_app_shell_action, resolve_app_shell_chrome,
    resolve_app_shell_chrome_with_workspace, resolve_app_shell_ribbons,
    resolve_app_shell_ribbons_with_workspace_chrome,
    resolve_app_shell_ribbons_with_workspace_layers,
};
pub use command_palette::{CommandPaletteState, PaletteItem};
pub use module::{MaraModule, ModuleInlineCtx, ModuleInlineOptions, ModuleResponse, WorkspaceCtx};
pub use ribbon::{
    ResolvedSlotRibbon, RibbonAction, RibbonActionError, RibbonActionResult, RibbonAvoidance,
    RibbonCluster, RibbonDrag, RibbonEdge, RibbonGlyph, RibbonMode, RibbonOpen,
    RibbonOverrideLayer, RibbonOverridePolicy, RibbonPlacement, RibbonRole, RibbonScope,
    RibbonSlot, RibbonSlotClick, RibbonSlotDef, RibbonSlotId, RibbonSlotItem, RibbonSlotOverride,
    RibbonWidth, app_menu_command_id, app_menu_slot_id, bottom_shelf_command_id,
    bottom_shelf_slot_id, dispatch_ribbon_action, left_shelf_command_id, left_shelf_slot_id,
    permanent_app_menu_slot, permanent_bottom_shelf_slot, permanent_left_shelf_slot,
    permanent_right_shelf_slot, permanent_system_control_slot, permanent_view_switcher_ribbon,
    phone_remapped_ribbon_edge, resolve_slot_item, resolve_slot_items,
    restore_workspace_slot_override, ribbon_clearance, right_shelf_command_id, right_shelf_slot_id,
    system_close_or_restore_slot_id,
};
pub use shelf::{
    ShelfContainer, ShelfDef, ShelfEdge, ShelfEdgeError, ShelfLayout, ShelfState, layout_shelves,
    responsive_shelves, shelf_insets,
};
pub use shell::{ShellBar, ShellEvent, ShellView};
pub use style::{
    AccentColor, Breakpoint, GlassOpacity, PHONE_MAX_WIDTH, ScreenMetrics, TABLET_MAX_WIDTH,
    screen_class, screen_metrics, set_glass_opacity, set_touch_density_override, touch_density,
};
pub use view::{
    CellId, Layout, MaraView, SharedSurfaceId, SplitAxis, Tab, Tabs, ViewCtx, ViewEntry, ViewId,
    ViewNode, ViewRouter, ViewRouterError,
};
pub use window_chrome::{
    WindowChromeHit, WindowChromeHostCapabilities, WindowChromeInput, WindowChromePolicy,
    WindowChromeRegions, WindowChromeState, WindowChromeUpdate, WindowResizeDirection,
    hit_test_window_chrome_regions, resize_direction,
};
pub use workspace::{
    WorkspaceBar, WorkspaceBarCluster, WorkspaceBarEdge, WorkspaceBarItem, WorkspaceBarItemKind,
    WorkspaceLevelState, WorkspaceOwner, WorkspacePolicy, WorkspaceStack, WorkspaceStackError,
};

// Surface the remaining transitional widget functions at the crate
// root so `use bevy_mara::prelude::*;` still brings the widgets that
// have not been fully sealed yet into scope. Completed simple
// widgets are exposed through `MaraUi` methods instead of loose
// `egui::Ui` helpers. The TYPE-style names (`Button`,
// `TreeIconSlot`, …) sit here too so trait-shaped widgets compose
// without a longer path.
pub use widget::text_area::{MaraTextArea, MaraTextAreaResponse};
pub use widget::{
    BADGE_LABEL_COL_W, BADGE_ROW_H, BUTTON_LABEL_FONT, BUTTON_ROW_H, BUTTON_ROW_H_SUBTITLE, Button,
    CARD_BUTTON_ROW_H, CHIP_H, COLOR_SWATCH_H, DROPDOWN_ROW_H, FillStyle, HYBRID_SELECT_ROW_H,
    HybridSelectResponse, KEYBINDING_ROW_H, PROGRESSBAR_ROW_H, READOUT_ROW_H, SELECT_ROW_H,
    SLIDER_ROW_H, TOGGLE_ROW_H, TREE_INDENT, TREE_ROW_H, TreeBranchGuide, TreeIconKind,
    TreeIconSlot, TreeRowResponse,
};

// Re-export of the bundled `iconflow` crate so consumers can reach
// `iconflow::list(Pack::Fluentui)`, `Pack`, etc. without their own
// dependency on the same version we ship.
pub use iconflow;
