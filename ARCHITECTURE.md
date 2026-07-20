# Mara — Architecture

This document describes Mara at a high level: the design boundaries, how the
pieces fit together, and why. For how to *use* the toolkit, see
[`README.md`](README.md); for the build toolchain, see [`TOOLCHAIN.md`](TOOLCHAIN.md).

---

## 1. What Mara is

Mara is a reusable **egui-based editor UI kit** built around one rule:

> The UI logic lives in a single **framework-agnostic core**; every concrete
> environment (a native window, a Bevy app, the browser) is a **thin host
> adapter** that drives that core.

App code is written once against a **sealed UI surface** and runs unchanged on
all hosts. egui is an implementation detail of the core, not part of Mara's
public API — the long-term goal is that the rendering backend could be replaced
without touching app code.

Two consequences shape everything below:

1. **Backend-agnostic seam.** Widgets never call egui directly. They go through
   an abstract `UiBackend` and emit a paint IR (`PaintCmd`). Today there is one
   backend (egui); the seam is what keeps that swappable.
2. **Sealed public API.** App code sees Mara vocabulary types (`vocab::Rect`,
   `Color32`, `Id`, …) and the `MaraUi` widget surface — never `egui::Ui` or
   `egui::Rect`. A deliberate escape hatch (`raw-egui` feature) exists for
   advanced consumers, behind `__internal_*` entry points.

---

## 2. Repository layout

```text
crates/
  core/                  mara_core   — the framework-agnostic UI core
  modules/graph/         mara_graph  — node graph widget (vendored fork)
  modules/code/          mara_code   — code editor widget (vendored fork)
  modules/image/         mara_image  — proof View+Module surface
  modules/canvas/        mara_canvas — freehand canvas View+Module
  modules/board/         mara_board  — pixel-drawing Board surface module
  modules/map/           mara_map    — vector-tile map View+Module
  modules/three_d/       mara_three_d— retained 3D scene View+Module
  modules/bevy/          mara_bevy   — embedded Bevy viewport (offscreen)
mara/
  src/                   mara        — unified facade (ui + native window runner)
  plugin/bevy/           bevy_mara   — Bevy host adapter (plugins)
example/                 root demo (native.rs / web.rs / app.rs / bevy_content.rs)
example/sealed/          compile-test that only the sealed API is reachable
```

Naming note: the directory is `crates/core/` but the crate is `mara_core`
(naming a crate `core` would shadow Rust's `::core`).

---

## 3. The backend-agnostic seam (the heart of Mara)

Everything visual flows through five abstractions in `mara_core`. This is the
layer that makes the toolkit backend-swappable.

| Abstraction | File | Role |
|---|---|---|
| `vocab` | `vocab.rs` | Mara's own `Pos2` / `Vec2` / `Rect` / `Color32` / `Id`, wrapping the egui equivalents. The public API speaks *only* these — egui types never leak. |
| `UiBackend` | `layout.rs` | Trait the UI calls instead of `egui::Ui`: `allocate`, `interact`, `reserve_rect`, `show_area*`, `id`, `available_width/height`, `input`, `add_space`. |
| `EguiUiBackend` | `backend/egui.rs` | The one concrete `UiBackend` today — wraps `&mut egui::Ui` and translates calls to egui. |
| `PaintCmd` / `PaintList` | `paint.rs` | A paint **intermediate representation**: `Rect`, `Polygon`, `Polyline`, `Text`, `Svg`, `Circle*`, `Ellipse`, `Arc`, `Sector`, `Image`, `Clip`, … Widgets build `PaintCmd`s; `render_paint_cmd*` lowers them to egui shapes. Exposed as `MaraPainter` methods (`rect_filled`, `text`, `ellipse_filled`/`ellipse_stroke`, `arc`, `sector`, `image`, …). |
| `MaraMemory` / `MaraMemoryCtx` | `memory.rs` | Backend-neutral persisted per-id state, backed by egui's data store. |

On top of these sits the **sealed widget surface**:

- **`MaraUi`** (`mui/mod.rs`) — what every widget and pane body receives. It
  holds an `EguiUiBackend`, *not* an `egui::Ui`, and exposes Mara-shaped
  primitives (`MaraResponse`, `MaraInput`, `MaraPainter`, `MaraKey`).

**How far the seam actually goes today** (measured 2026-07-20; tracked by the
coupling ratchet in `make check` and scheduled for reduction by `PLAN.md`):

- The seam is **real at the paint/measure/interact layer**: widget logic
  expresses painting as `PaintCmd` and drives `*_backend` functions over the
  `UiBackend` trait; `backend/egui.rs` is the only *lowering* code. Seven
  widget families (label, toggle, readout, chip, badge, keybinding,
  progressbar) are fully backend-routed.
- The seam is **not yet real at the signature/runtime layer**: widget entry
  points in all 10 `widget/` files are still typed on `&mut egui::Ui` and
  construct `EguiUiBackend` locally; `egui::` is referenced in 57 core
  files; `MaraUi` stores the concrete `EguiUiBackend` (with 24 `ui_mut()`
  escape sites), and per-id state/animation mostly hits egui's data store
  directly (~139 sites) rather than `MaraMemory`. A second backend cannot
  run Mara today. `docs/adr/0001-backend-seam-scope.md` records the decided
  direction; `PLAN.md` Phases 0–3 are the closure work.

A handful of **backend-neutral state machines** live alongside, each a pure
`MaraMemory`-backed core that any backend could reuse:

- `popup.rs` (popup open/dismiss), `text_edit.rs` (text field state),
  `focus.rs` (focus registry), `scroll_state.rs` (scroll offsets).

```
app / widget code
        │  (only sees MaraUi + vocab types)
        ▼
     MaraUi  ──────────────►  PaintCmd / PaintList   (paint IR)
        │                              │
        ▼                              ▼
   UiBackend trait  ──────►  EguiUiBackend  ──►  egui::Ui / egui shapes
                              (the only egui-aware code)
```

---

## 4. Hosts and the host handshake

The core never owns an event loop, a window, or GPU resources. A **host** owns
those and, once per frame, packages its frame-local state into a single bridge
object the app and views consume:

- **`MaraHostCtx`** (`mara/src/host.rs`) — the per-frame handshake. Wraps the
  live `egui::Context`, an optional `egui_wgpu::RenderState` (present on
  native/eframe, absent on web), and a `MaraWindowHost` discriminant. It exposes
  everything app code needs: `apply_theme`, `show_pane`, `show_shelves`,
  `draw_slot_ribbons_featureful`, `view_ctx`, `content_rect`, `request_close`,
  `request_maximize_toggle`, `request_repaint`, `node_view_backend`, …

- **`MaraWindowHost`** advertises capabilities so the same UI adapts per host:
  - `None` — web/embedded (no native window ops)
  - `ExternalEgui` — eframe or another egui host owns the window (move/resize, but no system maximize/close)
  - `MaraNative` — Mara owns a borderless native window (all chrome + window ops)

### 4.1 Native runner — `mara/src/window.rs` (feature `window`)

A winit + wgpu runner (eframe-style, but Mara-owned). Per redraw:

```
take_egui_input(window)                         // winit → egui RawInput
egui_ctx.run(raw_input, |ctx| {
    app.update(&mut MaraHostCtx::mara_window(…)) // app draws its UI
    app.configure_shell(shell)
    shell.show(ctx, …)                           // enforced top bar (runner-owned)
})
ctx.tessellate(shapes)                           // egui shapes → GPU primitives
painter.paint_and_update_textures(…)             // render + present
```

Key responsibilities: borderless **window chrome** (top-bar drag, edge/corner
resize, close) via `mara_core::window_chrome` hit-testing; the **enforced
`ShellBar`** (the runner renders it — the app only configures views/active and
reacts to app-level `ShellEvent`s; window actions are handled in the runner).

> **Present mode is `AutoNoVsync`.** `paint_and_update_textures` presents the
> swapchain on the UI thread; the egui default `AutoVsync` (Fifo) *blocks* that
> thread on every present, which froze input/animation behind the GPU (measured
> at hundreds of ms to >1 s per frame). `AutoNoVsync` makes present non-blocking.
> See `crates/.../memory` notes and diagnose with `MARA_FRAME_TIME` (§10).

### 4.2 Bevy content — `crates/modules/bevy` (`mara_bevy`)

Mara owns egui and the shell. Bevy is embedded as content through
`MaraBevyViewport`, which keeps the top-level window, theme, ribbons, and pane
system in Mara while letting Bevy render a scene into an offscreen viewport.

- **`MaraBevyViewport`** — egui/Mara widget that reserves a region, drives a
  windowless Bevy app, uploads the latest frame/texture, and forwards pointer
  input into the Bevy camera.
- **`BevyViewportBridge` / `BevyViewportRenderTarget`** — the Bevy-side
  offscreen render target and frame-copy bridge.
- **`MaraBevySceneHelpersPlugin`** — small Bevy content helpers such as the demo
  ground grid and orbit camera support. It does not render Mara UI and does not
  create an egui context.
- **`mara/plugin/bevy` (`bevy_mara`)** — compatibility/helper crate that
  re-exports the embedded viewport helpers and Bevy-only material utilities. It
  deliberately has no Bevy-owned egui bridge.

### 4.3 Web

The browser owns the outer window, so the web host runs through eframe/`trunk`
(`MaraWindowHost::None`/`ExternalEgui`): native resize corners and top-bar
dragging are disabled and window chrome is left to the browser.

### The shared seam, in one picture

```
                     ┌───────────────────────────┐
                     │         mara_core          │
                     │ widgets · panes · shelves  │
                     │ ribbons · ShellBar · theme │
                     │ window-chrome hit-testing  │
                     └─────────────┬──────────────┘
                                   │  MaraHostCtx (per-frame bridge)
              ┌────────────────────┼────────────────────┐
              ▼                    ▼                     ▼
     native runner          eframe / web        embedded Bevy viewport
   (winit + wgpu,         (browser owns          (Bevy renders content;
    borderless chrome)     the window)            Mara owns the shell)
              └──────────── all render through Mara-owned egui ──────┘
```

App code only ever touches `MaraUi` + `MaraHostCtx`; the host supplies the egui
context and GPU plumbing behind them.

---

## 5. The UI composition model

Content nests through a fixed, type-checked hierarchy. A container accepts only
**Pods**, never raw widgets or closures — this keeps response collection and
layout bookkeeping uniform.

```
Pane (floating, anchored to an edge rail / zone)   ─┐
  └─ Container  (Normal = title+body, or Tabbed)     │  both reach the
       └─ Pod   (one composable unit, 1+ widgets)    │  same Pod/Widget
            └─ Widget (button, slider, toggle, …)    │  hierarchy
Shelf (docked left/right/bottom, reserves space)   ─┘
  └─ ShelfContainer (tabbed) → Container → Pod → Widget
```

- **Pane** (`pane/`) — floating/anchored surface pinned to an edge rail with
  start/middle/end zones; owns the themed title strip, fold/unfold animation,
  drag/resize, and auto-folding so it never exceeds the viewport. Shown via
  `host.show_pane(Pane::new(…), |body| { … })`; the body is a `PaneBody`.
- **Container** (`container/`) — a title zone + body zone block, the only direct
  child of a pane/shelf. Kinds: `Normal` (single body) and `Tabbed`.
- **Pod** (`pod/`) — the composable unit. Built fluently
  (`Pod::new(id).with_button(…).with_slider(…)…`); `with_*` covers the widget
  families. Flags like `fill()` (expand to remaining height) and `resizable()`
  (inter-pod drag) handle layout; returns a `PodResponse` with one result list
  per widget in declaration order.
- **Widget** (`widget/`) — themed leaf primitives: button, toggle, slider,
  drag_value, dropdown/select, color, badge, chip, progressbar, readout,
  keybinding, foldable, tree, context_menu, … Every widget is sized in multiples
  of **`UNIT`** (derived from the body font size), so rows align across the UI.

### Scopes

Surfaces carry a **scope** that controls lifetime/placement:
`Permanent` (always present, e.g. the top bar), `View` (tied to the active
view), and `Workspace` (tied to a pushed workspace level).

---

## 6. Views, Modules, Workspaces (the nesting levels)

This is how Mara goes from a single screen to nested editing surfaces.

- **`MaraView`** (`view/`) — a top-level (**L0**) surface selected from the
  shell's view switcher (e.g. a scene, a map, an editor). `ViewRouter` maps view
  ids to builders; `ViewCtx` is the sealed facade a view uses to draw its panes,
  shelves, and modules. Distinct views may share a `SharedSurfaceId` to back one
  document from several views.
- **`MaraModule`** (`module/`) — a heavier work surface that renders inline in a
  pod and can **escalate to fullscreen**, pushing a new workspace level.
  `ModuleInlineCtx`/`ModuleResponse` drive the inline render; `WorkspaceCtx`
  drives the pushed level.
- **`WorkspaceStack`** (`workspace/`) — one stack per view managing
  **L0 → L1 → L2 …** nesting as modules are fullscreened and restored;
  `WorkspaceBar` provides per-level action bars.

```
L0  MaraView  ── owns one WorkspaceStack
     ├─ bar + panes + shelves + pods (a pod may host a MaraModule inline)
     └─ fullscreen a module ─► push L1
            └─ bar + panes + shelves + pods (nested module inline)
                   └─ fullscreen ─► push L2 …
```

### Board & multiview (two independent things)

The pane/shelf hierarchy above is for *widget* UIs. Two separate
primitives cover free drawing and view composition; they are unrelated and
compose freely.

**1. Board** (`mara_board`) — a pixel-drawing surface *module*, a peer to
canvas/code/image/graph. It is a top-level `MaraView` **and** an embeddable
`MaraModule`. Where the canvas captures freehand strokes, a Board lets the
consumer draw raw `PaintCmd` primitives (`rect`, `text`, `ellipse_*`,
`arc`, `sector`, `image`) and read pointer input back — no widgets. The
consumer supplies drawing via a callback:

```rust
Board::new(id, "VT")
    .with_layout(Layout::row(…))         // OPTIONAL internal cells
    .on_draw(|b: BoardPaint| {           // b.painter, b.rect, b.response,
        // draw primitives; b.cell("data_mask") → cell rect   b.accent, b.cells
    })
```

The consumer owns the data model *and* hit-testing; Mara owns the surface
and the draw calls. A Board's *own* internal `Layout` is enough to build a
whole ISOBUS virtual terminal **inside one Board** (a data-mask cell +
soft-key cells). **Mara stays GUI-only** — Boards + primitives, never the
IOP/CAN/data model.

**2. MultiView** (`mara_core`, `view/multi.rs`) — the generic "divide a
view" primitive, independent of Board. It splits one view into cells and
hosts a *child view per cell*, each rendered scoped to its cell rect (own
content area + workspace, via `ViewCtx::__internal_scoped`). A child is any
`MaraView` — a Board, a canvas, a map, even another `MultiView`.

```rust
MultiView::new(id, "VT", Layout::row(gap, vec![
        (1.0, Layout::col(gap, soft_keys_left)),
        (4.0, Layout::cell("data_mask")),
        (1.0, Layout::col(gap, soft_keys_right))]))
    .view("data_mask", Box::new(Board::new(…).on_draw(…)))
    .view("L1",        Box::new(Board::new(…).on_draw(…)))   // each key its own Board
```

So a VT is buildable **either** as one Board with an internal layout, **or**
as a MultiView of Boards (a big board in the middle, a board per physical
key) — both are just compositions.

**Shared `Layout`** (`mara_core`, `view/layout.rs`) — the split-tree both
use: `Layout::{cell, row, col}` (children weighted, with a `gap`) →
`layout.resolve(rect) -> Vec<(CellId, Rect)>`. Pure geometry; knows nothing
about views or drawing.

*Not yet:* absolute (fixed-coordinate) cells; host-owned drag-to-rearrange;
and `CentralPanel`/GPU views (`three_d`, `bevy`) as MultiView children —
they own the whole window and must render into an Area at `content_rect`
before they can tile.

---

## 7. Ribbons and the Shell bar

- **Ribbons** (`ribbon/`) — edge-anchored slot button strips. The model is
  `RibbonSlotDef`/`RibbonSlot`/`RibbonSlotItem` resolved into
  `ResolvedSlotRibbon`s laid out by cluster (`Start`/`Middle`/`End`) and mode.
  "Featureful" ribbons add panel buttons, drag/reorder, cross-ribbon drops, and
  pane anchoring. Slot layout is computed in core (`SlotRibbonLayoutSpec`); each
  button is hosted in its own area at a screen-space rect
  (`item_screen_rect`, which offsets the local item rect by the ribbon's
  position).
- **Shell bar** (`shell.rs`) — the **enforced, host-neutral top bar**:
  app-menu (Start) + view switcher (Middle) + window controls / shelf toggles
  (End, injected from published host capabilities). It is **UI, not window
  chrome**, so there is one implementation rendered by the host each frame
  (`ShellBar::show` → `Vec<ShellEvent>`). Web/android advertise no window
  capabilities, so those buttons simply drop out.

### Enforced defaults (`enforce.rs`)

Mara's contract is that *using the toolkit at all* yields a correct Mara
app. Every surface entry point (panes, shelves, ribbons, views, command
palette, root body) first calls `enforce::__internal_enforce_defaults`,
whose rule per default is: **if the app did it this pass or the previous
pass, Mara stays out of the way; otherwise Mara does it.**

- **Theme** — the active Mara theme is applied unless the app applied one
  (opt-out = apply your own).
- **Shelf-layout baseline** — a full-viewport no-shelf layout is published
  unless the app published one (opt-out = publish a real layout).
- **Top bar** — if no `ShellBar` rendered, Mara renders the default
  fallback bar. There is **no passive disable flag** (`ShellBar::enabled`
  was removed; old code fails to compile). Apps wanting the functional bar
  render it via `ShellBar::show`, which suppresses the fallback. The one
  deliberate escape hatch is `MaraHostCtx::opt_out_shell_bar()` — an
  explicit *per-frame* call (honored by the runners too); stop calling it
  and the bar comes back.

The one-pass hysteresis exists because hosts render the bar *after* app
content each frame; the first pass a context is seen is a grace pass for
the same reason. This is what guarantees the bar on consumers that drive
`mara::ui` from their own egui host and never opted into a runner.

---

## 8. Theme & style runtime

`style.rs` + `themes/` hold a framework-agnostic theme runtime (no egui/bevy
imports):

- A global active `Theme` (`set_theme`) with built-ins **`flat`**, **`pro`**,
  and **`game`**, each in Dark/Light.
- **`UNIT`** / `BODY_FONT_SIZE` — the canonical row-height unit every widget
  sizes against.
- **Glass** — accent-tinted translucent panels driven by a global
  `GlassOpacity`; `AccentColor` is the user-selected accent.
- **Responsive** — `screen_class()` returns `Phone` / `Tablet` / `Desktop`
  breakpoints; ribbons/shelves reflow per class (e.g. bottom shelf merges into
  the side on phones). `touch_density` adjusts spacing.
- **Fonts** — bundled Iosevka weights + the `iconflow` Fluent icon set
  (`icons.rs`). Font installation rebuilds the egui atlas, so it is **deduped**
  by weight to avoid per-frame rebuilds.

---

## 9. Feature modules

The `crates/modules/*` crates are optional surfaces. They integrate in one of
three ways, which is worth knowing because it explains their performance and
portability characteristics:

| Module | Implements | How it renders |
|---|---|---|
| `mara_image` | `MaraView` + `MaraModule` | Through `MaraUi`/`PaintCmd` (fully abstracted) |
| `mara_canvas` | `MaraView` + `MaraModule` | Through `MaraUi`/`PaintCmd` (fully abstracted) |
| `mara_board` | `MaraView` + `MaraModule` | Through `MaraUi`/`PaintCmd` — a pixel-drawing surface with `on_draw` + optional internal `Layout` (see §6) |
| `mara_map` | `MaraView` + `MaraModule` | Hybrid: basemap via raw egui tessellation (`mvt.rs`); annotations lower to `PaintCmd` |
| `mara_graph` | View/Module (Mara styling optional) | A **secondary `egui::Context`** rendered to a wgpu texture (sharp-zoom), composited back as an image |
| `mara_code` | View/Module (Mara styling optional) | Raw egui in the parent context (vendored editor) |
| `mara_three_d` | `MaraView` + `MaraModule` | A standalone `three_d` (GL/WebGL) renderer; host owns the target |
| `mara_bevy` | egui widget (`MaraBevyViewport`) | A windowless **Bevy app** rendered to an offscreen wgpu texture, uploaded as an egui texture |

Modules that own a secondary context / offscreen render target (`graph`,
`bevy`, `three_d`) carry a one-time setup cost on first display (render-target
allocation + pipeline compile); the inline-`PaintCmd` modules do not.

---

## 10. Window chrome & diagnostics

- **Window chrome** (`window_chrome.rs`) — host-neutral hit-testing for a
  borderless window: it computes drag/resize/maximize/close regions; the host
  (native runner or `MaraWindowChromePlugin`) maps the hits onto real window
  operations and publishes its capabilities back so the shell shows the right
  controls.
- **Layout probe** (`probe.rs`, env `MARA_SHOW_POSE`) — every backend
  `allocate`/`interact`/area records an `ElementPose` (kind, label, rect, state);
  the host periodically dumps the whole GUI's positions/state to the terminal.
  Zero-overhead when disabled (a single atomic gate).
- **Frame timing** (env `MARA_FRAME_TIME`, native runner) — prints slow frames
  broken down into `ui_run` (CPU) / `tessellate` / `gpu_paint` (present), which
  is how the present-mode stall in §4.1 was found. CPU-dominant vs
  present-dominant tells you immediately where a slowdown lives.

---

## 11. The sealed-API pattern

- The public surface deliberately hides egui. App code uses `MaraUi`, `vocab`
  types, and `MaraHostCtx`; `example/sealed/` is a compile test that nothing
  else is reachable.
- Functions named `__internal_*` are the controlled seams hosts/adapters use
  (e.g. `__internal_draw_slot_ribbons_featureful_egui`, `__internal_apply_theme`).
  They are public-by-necessity but not part of the stable app API.
- A **`raw-egui`** feature exposes a raw escape hatch for advanced consumers.
  ⚠️ Cargo feature unification means enabling `raw-egui` anywhere unseals it for
  *everyone* in the build — never enable it in a library crate.

---

## 12. Design rules (for contributors)

- Put reusable behavior in **`mara_core`**; keep host/plugin crates as thin
  adapters.
- Don't add Bevy-only logic for behavior that should also work on egui/eframe or
  web. One core, many hosts.
- Widgets emit `PaintCmd` and go through `MaraUi`/`UiBackend` — **new code
  must not add `egui::Ui`-typed entry points or direct `ctx.data*` access**
  (the coupling ratchet in `make check` enforces no-increase). Existing
  egui-typed entries are being migrated per `PLAN.md`; each migrated surface
  deletes its old entry in the same change (greenfield rule — see
  `docs/adr/0001-backend-seam-scope.md`).
- egui's data store is **type-keyed**; `MaraMemory` is the sanctioned store
  for widget state. Today chrome-level state (pane/shelf/container/ribbon/
  enforce, ~139 sites) still uses `ctx.data*` directly — scheduled migration,
  not a pattern to copy. Atlas-affecting work (fonts/theme) must be deduped.
- Run the gates under the repo Nix shell:
  `nix develop --impure -c make check` and `… make test-all`.
