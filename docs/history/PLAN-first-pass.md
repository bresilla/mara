<!-- Recovered 2026-07-20 from a prior Claude Code session transcript.
     This is the first-pass 'Backend-Agnostic Mara UI' plan (~1804-line
     revision, ~mid-2026) that previously lived at repo-root PLAN.md,
     which was gitignored and therefore never committed. Superseded by
     the current PLAN.md (True Backend Independence — Second Pass). -->

# Plan: Backend-Agnostic Mara UI

Mara should become a UI toolkit whose public app surface is not tied to egui.
Egui can remain the reference backend because it is mature and productive, but
Mara should be structured so a future custom immediate-mode backend can replace
egui without rewriting application code.

This is a long-term architecture plan. It does **not** say we should replace
egui now. It says the internal boundaries should stop assuming egui is the only
possible engine.

## Current status

Mara is currently **consumer-sealed** and has the first backend-agnostic seams,
but it is not yet a fully backend-agnostic UI engine.

What is already good:

- `mara::ui` is the intended public facade for app/UI consumers.
- `example/sealed` proves a consumer can build Mara UI with no direct `egui`
  dependency.
- The public `raw-egui` feature/re-export/accessor surface has been removed;
  first-party egui integration now uses hidden `__internal_*` adapter hooks.
- `make check` verifies the sealed proof crate, rejects any `raw-egui`
  feature reintroduction in public manifests, rejects direct `mara_core` /
  leaf-module dependencies in the full example manifest, and rejects
  app-content raw backend escape hatches.
- Legacy compatibility aliases are being removed rather than kept as public
  alternate names; the Bevy viewport and Mara-owned window runner now expose
  their canonical names only.
- `crates/core/src/vocab.rs` now owns the common app-facing vocabulary:
  `Vec2`, `Pos2`, `Rect`, `Id`, `Color32`, `Stroke`, `TextureId`,
  `TextureHandle`, `ColorImage`, `Align2`, and `CornerRadius`.
- `ViewId`, `SharedSurfaceId`, `MaraModule::id`, `MaraResponse`, `MaraInput`,
  and the sealed canvas state now use Mara vocabulary instead of public egui
  identifiers/geometry.
- `MaraResponse` no longer stores `egui::Response`; follow-up behavior such as
  context menus uses an egui-backend side table.
- `PaintCmd` exists as the first Mara-owned paint IR. `MaraPainter` still
  renders immediately through egui today, but its drawing methods now lower
  through that command vocabulary.
- App-facing surfaces exist:
  - `MaraUi`
  - `MaraPainter`
  - `MaraResponse`
  - `MaraInput`
  - `MaraView`
  - `MaraModule`
  - `ViewCtx`
  - `PaneBody`
  - `Pod`

What is still egui-bound internally:

- `MaraUi` wraps `egui::Ui`.
- `MaraPainter` wraps `egui::Painter`.
- `ViewCtx` stores `egui::Context`.
- less-common vocabulary wrappers may still convert to egui internally while
  they wait for fully custom non-egui implementations.
- Widgets, panes, shelves, ribbons, command palette, theme application, and
  window chrome call egui layout/paint/input APIs directly.
- Module crates such as `mara_graph`, `mara_code`, `mara_map`, `mara_3d`,
  `mara_image`, and `mara_canvas` are still egui implementations.

So the current architecture is:

```text
App code
  -> sealed Mara API wrappers
  -> egui Ui / Context / Painter / Style / Memory
```

The target architecture is:

```text
App code
  -> Mara API
  -> Mara UI engine
  -> backend adapter
  -> egui today, custom backend later
```

## Goals

1. Keep application code on the Mara API.
2. Preserve egui as the first and best-supported backend.
3. Introduce Mara-owned data, layout, input, response, paint, and memory
   contracts.
4. Make egui an adapter behind those contracts, not the semantic model.
5. Allow advanced modules to remain backend-specific until they are ready to be
   ported.

## Non-goals

- Do not rewrite egui now.
- Do not make every public type generic over a backend.
- Do not expose raw backend handles to ordinary app code.
- Do not weaken the sealed consumer boundary.
- Do not force graph/code/3D/map to be portable in the first pass.

## Boundary rules

### Public app code

Public app code should only use:

- `mara::ui`
- `mara::ui::vocab`
- `MaraUi`
- `MaraPainter`
- `MaraView`
- `MaraModule`
- `ViewCtx`
- `PaneBody`
- `Pod`
- module surfaces exposed through `mara::ui::modules`

Public app code should not depend directly on:

- `egui`
- `mara_core`
- leaf module crates such as `mara_map`
- backend crates
- raw backend escape hatches

### Host glue

Host glue may use egui today because eframe, bevy_egui, and egui-winit are host
mechanisms. Host glue must keep that usage out of app content.

Examples of valid host glue:

- eframe `App::update`
- native/web runner setup
- Bevy/eframe/winit bridges
- render-state plumbing
- backend-specific texture upload

### Mara internals

Mara internals may use egui while the egui backend is the only backend, but new
code should prefer Mara-owned contracts when they exist.

## Target architecture

### `mara_runtime`

Long term, introduce a backend-neutral runtime layer. It may live inside
`mara_core` first and move to a crate later.

Core concepts:

```rust
MaraFrame
MaraInput
MaraPointer
MaraKeyboard
MaraResponse
MaraMemory
MaraLayout
MaraPaintCmd
MaraTextureId
MaraFontId
```

### `mara_backend_egui`

The egui backend translates between Mara runtime concepts and egui:

```text
MaraInput      <- egui::InputState
MaraMemory     <-> egui::Context data
MaraLayout     -> egui allocation/layout for the first backend
MaraPaintCmd   -> egui::Painter
MaraTextureId  <-> egui::TextureId / TextureHandle
```

This can start inside `mara_core` as an internal `backend::egui` module. It does
not need to be a separate crate on day one.

### Future custom backend

A future custom immediate-mode backend should implement the same contracts:

```text
MaraInput
MaraMemory
MaraLayout
MaraPainter / paint command renderer
Texture upload
Text shaping
Focus and keyboard routing
```

## Migration phases

### Phase 0: Keep the sealed boundary strict

Status: done for the current boundary.

Keep:

- `example/sealed` as the compile-only proof crate.
- `make check` verification for the sealed proof crate.
- guards that reject reintroducing a public `raw-egui` feature or egui
  re-export/accessor surface.

Added:

- a check that `example/Cargo.toml` does not directly depend on `mara_core` or
  leaf module crates.
- a grep/script check that app content does not call raw backend escape hatches.

Acceptance:

```sh
nix develop --impure -c make check
```

### Phase 1: Own the vocabulary

Status: first pass done for the common app-facing vocabulary.

Replace egui re-exports in `crates/core/src/vocab.rs` with Mara-owned types.

Current:

```rust
pub use egui::{Color32, Id, Pos2, Rect, Stroke, Vec2, ...};
```

Target:

```rust
pub struct Color { ... }
pub struct Id { ... }
pub struct Pos2 { ... }
pub struct Vec2 { ... }
pub struct Rect { ... }
pub struct Stroke { ... }
pub struct TextureId { ... }
```

The egui backend owns conversions:

```rust
impl From<mara::ui::vocab::Color> for egui::Color32 { ... }
impl From<egui::Color32> for mara::ui::vocab::Color { ... }
```

Rules:

- Keep constructors ergonomic.
- Keep names stable for app code where possible.
- Do not glob-export vocab at the root; keep it under `mara::ui::vocab`.
- Avoid pulling egui types into public signatures outside backend modules.

Done:

- `AccentColor` now stores Mara `Color32`, not raw egui `Color32`.
- `active_accent()` and `raw_accent()` now return Mara `Color32`; the current
  egui implementation converts at module/rendering call sites that still need
  concrete backend colours.
- Simple style text/accent getters (`on_panel`, `on_section`, `on_track`,
  dim variants, `fg_dim`, `accent_hover`, and `accent_pressed`) now return
  Mara `Color32`; egui conversion stays at rendering call sites.
- `contrast_text_for` now accepts/returns Mara `Color32`, leaving raw egui
  conversion at concrete paint/text call sites.
- Surface/fill style helpers (`body_accent`, `pane_fill`, `section_fill`,
  `subsection_fill`, `surface_lift_target`, `track_fill`, `popup_fill`,
  `row_alt_fill`, `row_hover_fill`, and `row_selected_fill`) now
  accept/return Mara `Color32`; egui conversion stays at concrete frame,
  fill-role, and rendering call sites.
- `section_title_color` now accepts/returns Mara `Color32`; container, pane,
  and tree renderers convert only when they paint through egui today.
- Shared outline and widget-border helpers now accept/return Mara `Color32`;
  concrete egui strokes and visual setup perform the backend conversion.
- `glass_fill` and semantic `fill_for` now accept/return Mara `Color32`;
  egui frames, visuals, and legacy module renderers convert at their paint
  boundary.
- `stroke_for` now accepts Mara color input and returns Mara `Stroke`; egui
  frame/chrome paths convert explicitly at the current backend boundary.
- `radius_for` now returns Mara `CornerRadius`; egui frame construction
  converts at the current backend boundary.
- Accent adaptation helpers (`high_contrast_accent` and
  `adapt_accent_to_mode`) now accept/return Mara `Color32`; theme application
  converts at the egui visuals boundary.
- `frame_for` now returns a Mara `FrameSpec` made from Mara fill, stroke,
  corner, margin, and shadow vocabulary; the egui backend owns conversion to
  concrete `egui::Frame`.
- `section_padding` now returns Mara `MarginSpec`, and `FrameSpec` stores
  backend-neutral margins instead of concrete `egui::Margin`.
- Text style helpers (`section_caps`, `title_text`, `body_label`, and
  `caption`) now return Mara `TextSpec`; RichText construction lives behind
  the egui backend adapter.
- `title_font_family` now returns Mara `TextFamily`; the egui backend maps it
  to the concrete host `FontFamily` only at text-rendering call sites.
- `srgb_to_color` returns Mara `Color32`; egui conversion stays at current
  implementation call sites that still need concrete egui colours.
- The Bevy-hosted demo accent setter now accepts Mara `Color32`, keeping
  cross-host accent state in Mara vocabulary.
- Built-in theme raw egui palette constants are crate-internal implementation
  details now; public theme exports expose constructors, not backend color
  tokens.
- The public shared palette/status constants in `style.rs` now use Mara
  `Color32`; theme constructors convert them only when populating the current
  egui-backed theme storage.
- Public icon lookup now exposes backend-neutral glyph/family data through
  `icon_glyph`; egui `FontFamily`/`RichText` convenience helpers stay internal
  or demo-local instead of being public Mara API.
- Bundled icon lookup/validation now reads the static icon registry even before
  egui runtime font installation, so backend-neutral icon payload checks do not
  depend on an egui frame having installed fonts.
- The remaining public `vocab` aliases to egui types were removed; texture
  upload options now use a Mara-owned `TextureOptions` wrapper with `NEAREST`
  and `LINEAR` constants and conversion only at the egui backend boundary.

Acceptance:

- `example/sealed` compiles without egui.
- `grep -R "pub use egui" crates/core/src/vocab.rs` returns nothing.
- `grep -nE '^pub type .*=[[:space:]]*egui::' crates/core/src/vocab.rs`
  returns nothing.
- `nix develop --impure -c make check` passes.

### Phase 2: Introduce a paint command IR

Status: started.

Done:

- `crates/core/src/paint.rs` defines `PaintCmd`.
- `crates/core/src/paint.rs` defines `PaintList`, a retained command buffer
  with tests for backend-independent command inspection.
- `MaraPainter` public drawing methods speak Mara vocabulary instead of
  `egui::...` signatures.
- `MaraPainter` drawing methods lower through `PaintCmd`.
- `crates/core/src/backend/egui.rs` owns the current egui paint translation.
- Pane title caution-stripe chrome now builds a clipped Mara `PaintCmd` tree
  instead of exposing a public helper that takes raw egui painter/rect/color
  handles.
- Dead public raw-egui paint helpers (`paint_dashed_line`, `divider`, and
  `thin_divider`) were removed instead of kept as compatibility clutter.
- Z-layer helpers that return raw egui layer/painter types are crate-internal;
  public code can still use the named Mara z-tier constants without receiving
  a raw `egui::Painter`.
- `PaintList` tests now cover retained clip/text/image command data and clear
  semantics, so command buffers can be inspected without rendering through egui.
- `MaraPainter` now stores an internal sink instead of a direct `egui::Painter`
  field: the egui sink renders immediately through `backend::egui`, while the
  command sink retains clipped `PaintCmd` data for tests and future backends.
- `MaraPainter` command-sink tests cover line capture, nested clip
  intersection, and text command capture without rendering through egui.
- Raw icon paint helpers are no longer public app-facing API and the loose
  `__internal_paint_section_icon_egui` / `__internal_paint_icon_egui` hooks
  were removed. Public icon support is backend-neutral payload validation plus
  `icon_glyph`, while first-party icon/SVG calls now lower through Mara paint
  commands before the egui backend flushes them.
- Map Fluent/text icon annotations now lower to Mara `PaintCmd` text commands
  before the current egui backend renders them.
- Map retained point/line/polygon annotations now lower to Mara `PaintCmd`
  vector commands before rendering, including selection halos. The map view
  still uses the egui backend to flush those commands today.
- Map selected-feature chrome and in-progress draft geometry now lower to Mara
  `PaintCmd` vector commands before rendering.
- MVT basemap area fills and ordinary feature-line passes now lower to Mara
  `PaintCmd` polygon/polyline commands before the egui backend renders them.
- MVT label painting now lowers to Mara `PaintCmd::Text` commands after the
  current egui backend performs measurement/collision checks. Extruded
  building meshes, backend-neutral text measurement, and SVG icon textures
  remain backend-owned migration targets.
- `PaintCmd::Mesh` now carries backend-neutral solid triangle meshes, and MVT
  extruded building side/roof geometry lowers to mesh/line paint commands
  before the egui backend renders it. Backend-neutral text measurement and SVG
  icon textures remain backend-owned migration targets.
- `TextMeasureSpec` now records backend-neutral text measurement requests, and
  MVT labels use that Mara contract plus Mara `Rect`/`Vec2` collision state
  before the current egui backend measures and renders text.
- `PaintCmd::Svg` now carries raw SVG paint payloads with Mara rect/tint data,
  and map SVG icon annotations lower through that command instead of calling a
  loose egui icon/image-loader hook from the map view.
- Mara `Align2` now owns anchored-rect placement, so SVG/icon placement can be
  computed with Mara geometry before an egui backend adapter flushes it.
- `icon_paint_cmd` now lowers named Fluent icons and SVG payloads to Mara
  paint commands, giving first-party chrome a shared icon path without calling
  raw egui icon hooks directly.

Still needed:

- choose the host flush path for a fully retained frame once a second renderer
  exists; the current egui backend still renders its sink immediately.

Change `MaraPainter` from a thin egui wrapper into a command emitter.

Target:

```rust
pub enum PaintCmd {
    Line { a: Pos2, b: Pos2, stroke: Stroke },
    Polyline { points: Vec<Pos2>, stroke: Stroke },
    Polygon { points: Vec<Pos2>, fill: Color, stroke: Stroke },
    Rect { rect: Rect, radius: CornerRadius, fill: Color, stroke: Option<Stroke> },
    RectStrokeOutside { rect: Rect, radius: CornerRadius, stroke: Stroke },
    Circle { center: Pos2, radius: f32, fill: Option<Color>, stroke: Option<Stroke> },
    Text { pos: Pos2, anchor: Align2, text: String, size: f32, color: Color, mono: bool },
    TextWithFamily { pos: Pos2, anchor: Align2, text: String, size: f32, color: Color, family: TextFamily },
    Image { texture: TextureId, rect: Rect, uv: Rect, tint: Color },
    Shadow { rect: Rect, offset: Vec2, blur: u8, spread: u8, color: Color },
    Clip { rect: Rect, children: Vec<PaintCmd> },
}
```

Egui backend translates these into `egui::Painter` calls.

Acceptance:

- `MaraPainter` public methods no longer expose egui types.
- egui-specific painting is isolated to backend code.
- sealed canvas examples still work.

### Phase 3: Backend-neutral input, response, and memory

Status: started.

Done:

- `MaraResponse` stores only Mara-owned response data plus a backend token.
- Raw `egui::Response` retention moved into `backend::egui` temp storage.
- `MaraUi::context_menu` resolves the backend token internally, so sealed app
  code still uses Mara APIs.
- `MaraUi::context_menu` now resolves its response token through an egui backend
  helper for the current `Ui`, so the sealed facade no longer spells the raw
  context lookup itself.
- The raw `context_menu_mara` helper is no longer re-exported as public
  app-facing API.
- `MaraInput` translation moved into `backend::egui`.
- `MaraInput` now carries both latest pointer and interaction pointer
  positions plus any-button release state, so drag/resize code can consume a
  Mara snapshot instead of querying backend pointer helpers piecemeal.
- `MaraKey` exists as the first backend-neutral keyboard key vocabulary needed
  by Mara-owned command-palette navigation.
- `MaraMemory` and `MaraMemoryCtx` exist as the first backend-neutral memory
  facade, keyed by Mara `Id`.
- `MaraUi::memory`, `ViewCtx::memory`, and `TreeBody::memory` expose memory
  without handing out a raw backend context.
- `TreeBody` typed state helpers now accept Mara IDs instead of raw egui IDs.
- `TreeBody` no longer exposes raw `ctx()` / `ctx_mut()` handles; tree state
  access stays on the Mara memory helpers.
- Color-picker open/closed state now uses Mara `Id` plus `MaraMemory`, not
  direct widget-local `egui::Context` data access.
- Animated-button click-pulse timestamps now use Mara `Id` plus `MaraMemory`,
  not direct widget-local `egui::Context` data access.
- Foldable-section open/closed state now uses Mara `Id` plus `MaraMemory`,
  not egui `CollapsingState`; body indentation policy now lowers through Mara
  `IndentedBodySpec` before the current egui backend hosts the body `Ui`.
- Style animation helpers for appearance sessions, title scramble/glitch
  effects, chromatic-aberration offsets, and screen-metric publication are
  crate-internal egui-backend details; they no longer expose public
  `egui::Context` helper signatures to app code.
- The raw-egui debug inspector module is no longer public app API; it remains
  crate-internal while a Mara-owned debug/inspection surface is designed.
- The public `raw-egui` feature, public egui re-exports, and public raw
  `MaraUi`/`MaraPainter`/`ViewCtx`/`PaneBody`/host/window context accessors
  were removed; hidden `__internal_*` hooks remain only for first-party host
  and backend adapter code.
- Dead window-chrome input-claim helpers were removed, and the remaining
  region-clear helper is no longer re-exported as public app API; host-facing
  chrome region/capability publication remains explicit host glue.
- Window-chrome region/capability storage helpers that take raw
  `egui::Context` were demoted to hidden first-party host hooks; the public
  window-chrome region/input contracts now use Mara `Rect`/`Pos2`/`Vec2`
  vocabulary, and root exports expose the backend-neutral state machine /
  explicit-region hit-test rather than raw-context helpers.
- Pane-rect publication for first-party host input firewalls now stores and
  returns Mara `Rect` vocabulary through hidden internal hooks instead of
  exposing public raw-context helper names or retaining `Vec<egui::Rect>` in
  the cross-host contract.
- Shelf-layout publication/readback helpers that take raw `egui::Context` are
  now hidden first-party hooks; app/demo code publishes a full live content
  layout through `MaraHostCtx` instead of calling loose `mara_core` raw-context
  helpers directly, while shelf renderers still auto-publish real reservations.
- Theme/font application helpers that take raw `egui::Context` are hidden
  first-party backend hooks now; app/demo code applies themes through
  `MaraHostCtx`, and the Bevy plugin calls the internal hook from host glue
  instead of exposing `mara_core::style::apply_theme` as public app API.
- `Pane::show(ctx, ...)` was removed as a loose public raw-context render path;
  panes now render through sealed `ViewCtx::show_pane` / `MaraHostCtx::show_pane`
  facades that invoke the hidden first-party pane renderer.
- `ViewCtx::load_texture` now returns Mara vocabulary texture handles.
- Fullscreen/maximizable state access is now exposed through sealed
  `ViewCtx`/facade host-context methods returning Mara `Id` vocabulary;
  the raw `egui::Context` fullscreen helpers in `embed` were demoted to
  hidden first-party adapter hooks, and the demo uses the host context instead
  of calling `mara_core::embed::*` raw-context helpers directly.
- `make check` guards against reintroducing an `egui::Response` field in
  `MaraResponse`, against moving input snapshotting back into `mui`, and
  against reintroducing direct `ctx().data` / `ctx.data` state storage in the
  color and button widgets, egui collapsing-state ownership in the foldable
  section widget, or public raw-context fullscreen helper signatures.

Still needed:

- port more internal widget/layout state users from direct egui context data to
  `MaraMemory`.

Move interaction snapshots and widget state away from egui.

Target:

```rust
pub struct MaraInput { ... }
pub struct MaraResponse { ... }
pub trait MaraMemory {
    fn get<T>(&self, id: Id) -> Option<T>;
    fn set<T>(&mut self, id: Id, value: T);
}
```

Current `MaraResponse` should stop retaining `egui::Response` directly in its
core representation. If the egui backend needs the raw response for context
menus or tooltips, keep it in an egui-specific side table.

Acceptance:

- `MaraResponse` has no egui fields.
- context menus and follow-up interactions still work through Mara APIs.
- sealed app code still cannot get backend handles.

### Phase 4: Define a small Mara layout engine

Status: started.

Done:

- `crates/core/src/layout.rs` defines `Layer`, Mara `Sense`, and the
  `UiBackend` trait.
- `Layer` covers background, middle, foreground, and overlay host tiers, with
  the egui backend owning the concrete `Order` mapping.
- `crates/core/src/layout.rs` defines `AreaHost`, a backend-neutral identity,
  position, and layer record for absolute/floating areas.
- `AreaHost` now also carries whether the host is interactive, so passive drag
  previews can stay backend-neutral instead of spelling concrete egui area
  interactivity flags inline.
- `crates/core/src/layout.rs` defines `AreaSlotSpec`, a backend-neutral
  fixed-size area host record for chrome surfaces whose identity/layer/position
  and local size should be decided before a concrete backend opens the area.
- `crates/core/src/layout.rs` defines `CanvasSlotSpec` and `CanvasRectSpec`,
  backend-neutral canvas allocation/absolute-hit-region records that carry
  size/rect, identity, and interaction sense before the egui backend creates
  concrete painters.
- `crates/core/src/layout.rs` defines `PaintSurfaceSpec`, a backend-neutral
  painter-surface policy record for non-allocating draw surfaces such as the
  remaining available UI region.
- `crates/core/src/layout.rs` defines `ScrollRegion`, a backend-neutral
  identity and policy record for scroll hosts.
- `ScrollRegion` now carries horizontal/vertical axis, max extent, and full
  item-spacing policy, so shelf and palette scroll hosts describe scroll
  behavior as Mara data before the egui backend creates a concrete
  `ScrollArea`.
- `crates/core/src/layout.rs` defines `ChildRegion`, `StackDirection`, and
  `StackAlign`, so fixed child UI regions can describe their rect, stacking
  direction, and cross-axis alignment as Mara data before a backend constructs
  a concrete child layout.
- `crates/core/src/layout.rs` defines `PopupAlign` and `PopupSpec`, a
  backend-neutral popup alignment/gap/width/inner-margin policy record.
- `crates/core/src/layout.rs` defines `PopupTrigger`, a backend-neutral record
  connecting a widget response token to a stable popup identity.
- `crates/core/src/layout.rs` defines `PopupListSpec`, a backend-neutral popup
  body/list spacing policy record.
- `crates/core/src/layout.rs` defines `TextEditRegion`, a backend-neutral
  field/text-rect/font-size geometry record for text-edit hosts.
- `crates/core/src/layout.rs` defines `TextEditSpec`, a backend-neutral
  single-line text-edit policy record for region, hint, colors, background, and
  frame behavior.
- `crates/core/src/layout.rs` defines `InlinePickerSpec`, a backend-neutral
  policy record for inline picker host slider width and clip expansion.
- `crates/core/src/layout.rs` defines `IndentedBodySpec`, a backend-neutral
  policy record for simple indented body hosts such as foldable sections.
- `crates/core/src/layout.rs` defines `FrameHostSpec`, a backend-neutral
  policy record for frame outer width, content width, inner margin, and corner
  radius.
- `crates/core/src/layout.rs` defines `SpaceSpec`, a backend-neutral spacing
  policy record for fixed gaps.
- `crates/core/src/layout.rs` defines `ItemSpacingSpec`, a backend-neutral
  item-spacing policy record for stack/list hosts.
- `crates/core/src/layout.rs` defines `StackScopeSpec`, a backend-neutral
  child-scope direction policy for simple horizontal/vertical Mara UI scopes.
- `crates/core/src/layout.rs` defines `CursorIcon`, a backend-neutral cursor
  affordance vocabulary for hover/drag handles.
- `backend::egui::EguiUiBackend` implements the first egui-backed adapter for
  allocation, available-rect reads, clipping, and paint submission.
- `backend::egui` owns the current Mara `Layer` to egui `Order` mapping.
- `Pane::order` now accepts Mara `Layer`, not raw egui `Order`; the egui order
  conversion is kept at the backend adapter boundary.
- `backend::egui` owns adapter helpers that turn Mara `AreaHost` and
  `ScrollRegion` contracts into egui `Area` and vertical `ScrollArea` hosts.
- `backend::egui` owns the helper that shows an `AreaHost` through the current
  concrete egui `Area`.
- `backend::egui` owns the helper that constrains a concrete egui `Ui` to a
  Mara `Rect`, so sealed view code can describe its body host in Mara geometry
  instead of constructing an egui rect/area directly.
- `backend::egui` owns the helper that builds a context-level painter from a
  Mara `Layer`, `Id`, and `Rect`, so view background/overlay painters no
  longer construct concrete egui `Painter` / `LayerId` values inline.
- `backend::egui` owns the helper that shows a Mara `ScrollRegion` and applies
  its row-spacing policy inside the concrete egui scroll body.
- `backend::egui` owns the helper that turns a Mara `PopupSpec` plus a backend
  response into the current concrete egui popup host.
- `backend::egui` owns the helper that converts a Mara `PopupTrigger` into the
  egui toggle response used by the current popup open-state implementation.
- `backend::egui` owns the helper that applies a Mara `PopupListSpec` to the
  current egui popup body.
- `backend::egui` owns the helper that turns a Mara `TextEditRegion` into the
  egui child `Ui` used by the current text editor host.
- `backend::egui` owns the helper that turns a Mara `TextEditSpec` into the
  current concrete egui single-line `TextEdit`.
- `backend::egui` owns the helper that shows the current concrete single-line
  text editor from a Mara `TextEditSpec`, including child-UI creation and
  response rect normalization, backend-neutral response conversion, and focus
  state capture.
- `backend::egui` owns the helper that combines a Mara `TextEditSpec` with the
  current focus-if-unfocused policy, so command-palette code no longer branches
  on concrete `has_focus` output or calls the concrete focus-request helper
  directly.
- `backend::egui` owns the helper that applies focus requests to the concrete
  backend response remembered behind a Mara response token, including the
  current egui-`Ui` hosted focus request path.
- `backend::egui` owns the helper that consumes Mara `MaraKey` values from the
  current concrete egui input state.
- `backend::egui` owns the helper that snapshots a fixed list of consumed Mara
  keys, letting command-palette navigation apply pure Mara state transitions
  without calling the per-key concrete input consumer inline.
- `backend::egui` owns the helper that converts Mara `Color32` into the current
  concrete egui color where legacy style helpers still require host colors.
- `backend::egui` owns the helper that converts Mara `CursorIcon` into the
  current concrete egui cursor icon and applies hover-cursor requests through
  Mara response tokens.
- `backend::egui` owns the helper that applies a Mara `CursorIcon` directly to
  the current concrete host context for migration sites that still hold an
  egui `Ui`.
- `backend::egui` owns the helper that applies an `InlinePickerSpec` while
  hosting the current concrete egui picker body.
- `backend::egui` owns the helper that turns an `IndentedBodySpec` into the
  current concrete egui indented body host.
- `backend::egui` owns the helper that turns a `FrameHostSpec` into the
  current concrete transparent egui frame host.
- `backend::egui` owns the helper that applies a `SpaceSpec` to the current
  concrete egui layout.
- `backend::egui` owns the helper that applies an `ItemSpacingSpec` to the
  current concrete egui layout.
- `backend::egui` owns the helper that turns a Mara `PaintSurfaceSpec` into
  the current concrete painter for non-allocating custom drawing.
- `backend::egui` owns the helper that reads the current concrete layout
  direction as Mara `StackDirection`, keeping direction branching out of
  migrated chrome logic.
- `backend::egui` owns the helper that reads the current host content rect and
  returns Mara `Rect` vocabulary, so overlay geometry does not call egui
  context geometry APIs inline.
- `backend::egui` owns the helper that reserves and fills deferred paint
  command slots for chrome that must render behind later egui-hosted content.
- `UiBackend::interact` covers explicit sub-rect interaction, which lets
  composite widgets expose independent responses without reaching directly into
  egui.
- `UiBackend::reserve_space` covers fixed-size layout reservation, so code can
  advance a parent layout and receive a Mara `Rect` without directly calling
  concrete `allocate_space`.
- `UiBackend::reserve_rect` covers explicit absolute-rect layout reservation,
  so composite chrome can advance parent layout through the backend contract
  instead of calling concrete `allocate_rect` directly.
- `UiBackend::begin_area` now consumes `AreaHost`, so the backend contract
  receives Mara-owned area identity/position/layer data instead of separate
  raw parameters.
- `MaraUi::id`, available-size reads, input snapshots, memory facade creation,
  fixed spacing, horizontal/vertical child scopes, separator paint, canvas
  allocation, absolute canvas hit testing, and remaining-rect painter creation
  now use backend
  contract/helper paths instead of directly owning those egui calls.
- `MaraUi::separator` now allocates through `UiBackend` and submits a Mara
  `PaintCmd::Line` through `UiBackend::paint` instead of drawing through the
  immediate remaining-rect `MaraPainter` path.
- `MaraUi::canvas` and `MaraUi::canvas_at` now lower their allocation /
  absolute-interaction policy through `CanvasSlotSpec` / `CanvasRectSpec` and
  egui backend helpers; the sealed facade no longer spells the concrete
  allocate/interact/painter plumbing inline.
- `MaraUi::painter` now lowers its non-allocating remaining-rect surface
  through `PaintSurfaceSpec` and one egui backend helper instead of naming a
  concrete available-rect painter helper directly.
- `MaraUi::horizontal` and `MaraUi::vertical` now lower through
  `StackScopeSpec` plus one backend stack-scope helper instead of calling
  separate concrete egui scope helpers.
- `MaraUi` now stores its ambient accent as Mara `Color32`; raw egui color
  conversion happens only when calling legacy egui-backed widget adapters.
- A backend-neutral recording test proves allocation and paint submission can be
  exercised without egui.

Still needed:

- move `MaraUi` fully onto `UiBackend` rather than storing/borrowing egui `Ui`
  directly.
- introduce real area/popup/scroll abstractions through the backend contract.
- port simple widgets to consume `UiBackend` allocation/paint instead of raw
  egui `Ui`.

Do not copy all of egui. Mara has a controlled editor UI language, so the
layout engine can be smaller.

Minimum layout primitives:

- vertical stack
- horizontal row
- fixed-size allocation
- available rect
- clip rect
- absolute area
- scroll region
- overlay layer
- popup anchor

Target:

```rust
pub trait UiBackend {
    fn begin_area(&mut self, host: AreaHost, rect: Rect);
    fn allocate(&mut self, size: Vec2, sense: Sense) -> MaraResponse;
    fn interact(&mut self, rect: Rect, id: Id, sense: Sense) -> MaraResponse;
    fn available_rect(&self) -> Rect;
    fn push_clip(&mut self, rect: Rect);
    fn pop_clip(&mut self);
    fn measure_text(&self, text: &str, size: f32, mono: bool) -> Vec2;
    fn paint(&mut self, cmd: PaintCmd);
}
```

Acceptance:

- simple widgets can render through `UiBackend`.
- egui remains the only concrete implementation initially.

### Phase 5: Port simple widgets to Mara runtime

Status: started.

Done:

- `label`/`label_colored` now return `MaraResponse`.
- `label_backend` renders plain text through `UiBackend` and `PaintCmd::Text`;
  `MaraUi::label` and `MaraUi::label_colored` no longer call egui label
  directly.
- The raw egui adapter wrappers for labels have been removed; `MaraUi` now
  creates the current backend adapter and calls the backend-neutral label
  renderer directly.
- A backend-neutral label test proves the widget allocates measured text and
  emits a proportional text command without egui.
- `readout`/`readout_h` now return `MaraResponse`.
- `readout_backend` renders through `UiBackend` and `PaintCmd` instead of
  directly allocating/painting through egui.
- `MaraUi::readout`, `MaraUi::readout_h`, and pod readout rows create the
  current backend adapter and call the backend-neutral readout renderer
  directly.
- The raw egui adapter wrappers for readouts have been removed; public app
  code reaches readouts through `MaraUi`, not a loose `egui::Ui` helper.
- A backend-neutral readout test proves the widget emits label/value text
  commands without egui.
- `chip`/`chip_colored` now return `MaraResponse`.
- `chip_colored_backend` renders through `UiBackend` and `PaintCmd`; the egui
  facade and pod paths now create the current backend adapter and call the
  backend-neutral renderer directly.
- Chip default glass fill now stays in Mara `Color32` vocabulary instead of
  converting the ambient accent to raw egui color before paint-command
  lowering.
- The raw egui adapter wrappers for chips have been removed; public app code
  reaches chips through `MaraUi`, not a loose `egui::Ui` helper.
- A backend-neutral chip test proves the widget emits fill/stroke/text commands
  without egui.
- `keybinding_row`/`keybinding_row_h` now return `MaraResponse`.
- `keybinding_row_backend` renders through `UiBackend` and `PaintCmd`;
  `MaraUi` and pod keybinding rows now create the current backend adapter and
  call the backend-neutral renderer directly.
- The raw egui adapter wrappers for keybinding rows have been removed; public
  app code reaches keybinding rows through `MaraUi`, not a loose `egui::Ui`
  helper.
- A backend-neutral keybinding test proves the widget emits key-chip/action text
  commands without egui.
- `badge_row`/`badge_row_colored` now return `MaraResponse`.
- `badge_row_backend` renders through `UiBackend` and `PaintCmd`; it paints the
  label plus clipped chip commands without child egui Uis.
- `MaraUi` and pod badge rows now create the current backend adapter and call
  the backend-neutral badge renderer directly.
- The raw egui adapter wrappers for badges have been removed; public app code
  reaches badges through `MaraUi`, not a loose `egui::Ui` helper.
- A backend-neutral badge test proves the widget emits label/chip commands
  without egui.
- `button`/`button_h` now return `MaraResponse`.
- `button_backend` renders the plain button through `UiBackend` and `PaintCmd`;
  the egui entry points are adapter wrappers for the simple button path.
- Plain/action button theme-color derivation now stays in Mara `Color32`
  vocabulary, including button lerp/alpha helpers and backend-neutral paint
  command emission; egui color conversion remains only in the current animated
  egui adapter/content-rendering bridge.
- Animated button allocation and painting now route through Mara backend
  allocation and paint-command submission instead of direct egui
  `allocate_exact_size` / painter calls; the egui adapter only supplies the
  current animation clock/focus response bridge.
- Animated button geometry interpolation now uses Mara-local scalar helpers
  instead of raw `egui::lerp`.
- Button tooltip and animation-clock follow-ups now go through egui backend
  helper seams instead of direct widget-local `ui.ctx()` access.
- Inline color picker bodies now pass Mara `Color32` plus Mara
  `ColorPickerAlpha` policy into the egui backend adapter; the widget no
  longer owns raw `egui::Color32` picker state.
- Context-menu popup row spacing now lowers through a Mara
  `ItemSpacingSpec` and the current egui backend adapter; the raw
  `context_menu_mara` bridge is crate-internal backend plumbing, not public
  app API.
- The loose public plain/card/action button adapter functions are now
  crate-internal; public app code reaches them through `MaraUi`, not raw
  `egui::Ui` shortcuts.
- `Button::show` and `ActionButton::show` now take sealed `MaraUi` instead of
  raw `egui::Ui`; first-party egui hosting uses crate-internal `show_egui`
  adapters.
- The old `MaraUi::card_button` compatibility shortcut was removed; card-style
  buttons use the canonical `Button::new(...).glyph(...).subtitle(...).show(...)`
  builder path.
- `Button::show` now returns `MaraResponse`; plain builder buttons without a
  subtitle, glyph, or custom animation route through `button_backend`, so pod
  plain buttons use the same backend-neutral path as `MaraUi::button`.
- `button_content_backend` now renders non-animated button content, including
  card/subtitle/glyph buttons, through `UiBackend` and `PaintCmd`; the direct
  egui `Button::show` path is now limited to custom animated fill styles.
- Animated button content now shares the same Mara `PaintCmd` content lowering
  as the backend-neutral button path; `button.rs` no longer lays out or paints
  button text through raw egui font/layout/text APIs.
- Container title labels, brackets, title prefixes, inline Fluent glyphs, and
  rotated side titles now lower to Mara `PaintCmd::TextRuns`; the egui
  layout/galley/TextShape details live behind the backend adapter instead of
  in `container/normal`.
- `action_button_backend` renders the body chrome, independent tail action,
  body text, glyph, subtitle, and action glyph through `UiBackend` and
  `PaintCmd`; `ActionButton::show` is now an egui adapter wrapper, with tooltip
  attachment delegated through the backend side table.
- A backend-neutral button test proves the plain button emits fill/stroke/text
  commands without egui.
- A backend-neutral card-button test proves glyph, label, and subtitle commands
  are emitted without egui.
- A backend-neutral action-button test proves body/tail responses and action
  glyph painting work without egui.
- `progressbar`/`progressbar_h` now return `MaraResponse`.
- `progressbar_backend` renders the caption, track, fill, and clipped readout
  text through `UiBackend` and `PaintCmd`.
- `MaraUi` and pod progress bars now create the current backend adapter and
  call the backend-neutral progress renderer directly.
- The raw egui adapter wrappers for progress bars have been removed; public app
  code reaches progress bars through `MaraUi`, not a loose `egui::Ui` helper.
- The button, chip, badge, progressbar, toggle, slider, dropdown, text-input,
  select, color-swatch, foldable-section, and context-menu egui adapter entry
  points now accept Mara `Color32` vocabulary for accent/fill inputs instead of
  public raw egui color parameters.
- Progressbar, toggle, and slider backend renderers now keep track/fill/border,
  blend, segmented-dim, and contrast-color policy in Mara `Color32`/`Stroke`/
  `CornerRadius` vocabulary; they no longer convert to raw egui colors for
  their backend-neutral paint-command paths.
- A backend-neutral progressbar test proves label, track, fill, and clipped
  readout commands are emitted without egui.
- `toggle`/`toggle_h`/`toggle_track_only` now return `MaraResponse`.
- `toggle_backend` and `toggle_track_only_backend` render the label, track, and
  knob through `UiBackend` and `PaintCmd`, using Mara response state for
  click/change mutation.
- `MaraUi` and pod toggles now create the current backend adapter and call the
  backend-neutral toggle renderer directly.
- The raw egui adapter wrappers for toggles have been removed; public app code
  reaches toggles through `MaraUi`, not a loose `egui::Ui` helper.
- A backend-neutral toggle test proves label plus track/knob chrome commands
  are emitted without egui.
- `slider`/`slider_h` now return `MaraResponse`.
- `slider_backend` renders the caption, interactive bar, fill, and clipped
  value readout through `UiBackend` and `PaintCmd`, and mutates values through
  Mara interaction snapshots.
- The raw egui adapter functions for sliders are crate-internal; public app
  code reaches sliders through `MaraUi`, not a loose `egui::Ui` helper.
- Backend-neutral slider tests prove paint-command emission and click-driven
  value mutation/change reporting without egui.
- `drag_value`/`drag_value_h` now return `MaraResponse`.
- `drag_value_backend` renders the label, input chrome, and value text through
  `UiBackend` and `PaintCmd`, and handles horizontal drag mutation through
  Mara interaction snapshots. It intentionally does not implement click-to-type
  editing yet.
- The old loose `axis_drag`/`axis_drag_h` egui adapter shortcuts were removed;
  the reusable axis drag-value logic lives in the backend-neutral
  `axis_drag_backend` path.
- The raw egui adapter functions for drag values are crate-internal; public app
  code reaches drag values through `MaraUi`, not a loose `egui::Ui` helper.
- Backend-neutral drag-value tests prove paint-command emission and drag-driven
  value mutation/change reporting without egui.
- `select_row`/`select_row_h` now return `MaraResponse`.
- `select_row_backend` renders selected/hover row chrome plus label/trailing
  text through `UiBackend` and `PaintCmd`.
- `hybrid_select_row_backend` renders body/trailing text and the independent
  radio ring/dot through `UiBackend` and `PaintCmd`, with separate Mara
  responses for body and radio targets.
- Select row selected/hover fill policy now stays in Mara `Color32` vocabulary
  instead of converting to raw egui colors inside the backend-neutral row
  renderer.
- The raw egui adapter functions for select rows are crate-internal; public app
  code reaches select rows through `MaraUi`, not a loose `egui::Ui` helper.
- Backend-neutral select tests prove plain and hybrid select rows emit row/text
  and radio commands without egui.
- `dropdown`/`dropdown_h` now return `MaraResponse`.
- `dropdown_trigger_backend` renders trigger chrome, selected text, and chevron
  through `UiBackend` and `PaintCmd`.
- `dropdown_popup_row_backend` renders popup list-row selected/hover chrome and
  label text through `UiBackend` and `PaintCmd`; popup placement policy now
  lowers through Mara `PopupSpec`, and popup response/identity linkage lowers
  through Mara `PopupTrigger`; popup body spacing lowers through Mara
  `PopupListSpec`; popup row selection mutation is now a pure Mara-side helper,
  while popup open-state still remains egui-owned for now.
- Dropdown and text-input chrome color policy now stays in Mara `Color32` /
  `Stroke` vocabulary through trigger, popup-row, and text-field paint lowering;
  dropdown popup identity and toggle-response lookup go through egui-backend
  adapter helpers instead of direct `ui.id()` / `ui.ctx()` calls.
- Backend-neutral dropdown tests prove trigger and popup-row chrome emit paint
  commands without egui, popup anchor/list policy is Mara data, and popup row
  selection behavior does not depend on egui.
- The raw egui adapter functions for dropdowns are crate-internal; public app
  code reaches dropdowns through `MaraUi`, not a loose `egui::Ui` helper.
- `color_rgb`/`color_rgba` now return `MaraResponse`.
- `labelled_swatch_backend` renders color row label, swatch fill, and swatch
  border through `UiBackend` and `PaintCmd`; the actual HSV/alpha picker body
  remains egui-owned for now, but picker open-state identity/storage,
  toggle state, RGB/RGBA preview/write-back conversion, and picker host slider
  width / clip expansion now lower through Mara `Id`, `MaraMemory`, helper
  functions, and `InlinePickerSpec`.
- Color-row adapter code now obtains its scope ID, memory facade, picker
  spacing, and picker available width through backend helpers/specs instead of
  spelling direct `ui.id()`, `ui.ctx()`, `ui.add_space`, or
  `ui.available_width()` calls inline; swatch border color policy also stays in
  Mara `Color32`/`Stroke` vocabulary.
- The raw egui adapter functions for color rows are crate-internal; public app
  code reaches color widgets through `MaraUi`, not a loose `egui::Ui` helper.
- Backend-neutral color tests prove label/fill/border commands, toggle
  behavior, Mara-memory open-state storage, picker host policy, RGB
  conversion, and unmultiplied RGBA round-tripping without egui.
- `text_input`/`text_input_h` now return `MaraResponse`.
- `text_input_chrome_backend` renders field fill/border, leading search glyph,
  and optional clear glyph through `UiBackend` and `PaintCmd`; the actual
  single-line text editing remains egui-owned for now, but text-edit geometry
  and single-line widget policy now pass through Mara `TextEditRegion` and
  `TextEditSpec` before the egui backend shows the concrete editor and returns
  a Mara response.
- The raw egui adapter functions for text inputs are crate-internal; public app
  code reaches text inputs through `MaraUi`, not a loose `egui::Ui` helper.
- Backend-neutral text-input chrome tests prove field/search/clear paint
  commands are emitted without egui and the clear target is omitted when the
  field is empty.
- `section_header_backend` renders foldable-section chevron and caps title
  through `UiBackend` and `PaintCmd`; the open/closed state now lowers through
  Mara `MaraMemory`, and body indentation lowers through Mara
  `IndentedBodySpec` while the body is still hosted by the current egui `Ui`.
- Foldable-section adapter code now obtains its scope ID and memory facade via
  backend helpers instead of spelling direct `ui.id()` / `ui.ctx()` lookups.
- The raw egui adapter function for foldable sections is crate-internal; public
  app code reaches sections through `MaraUi`, not a loose `egui::Ui` helper.
- Backend-neutral foldable-section tests prove chevron/title commands,
  default-open fallback, persisted open-state storage, and click-toggle
  behavior/body-host policy without egui collapsing state.
- `palette_row_backend` renders command-palette result-row selected/hover
  chrome, label, and optional hint through `UiBackend` and `PaintCmd`; the
  overlay host, keyboard routing, scroll area, and text-edit query box remain
  egui-owned for now.
- Backend-neutral command-palette row tests prove selected rows emit
  background/label/hint commands and plain rows omit unnecessary chrome.
- `paint_separator_backend` and `paint_separator_resize_backend` render
  container/pod separator lines, dots, and resize-handle chrome through
  `UiBackend` and `PaintCmd`; the egui wrapper only supplies the host cursor
  affordance for resize handles.
- Separator resize handles now use Mara `CursorIcon` and Mara `Color32`
  vocabulary before the egui backend applies the concrete hover cursor and
  legacy host color conversion.
- Backend-neutral separator tests prove plain line separators and dot-grip
  resize separators emit the expected command vocabulary without egui.
- `palette_corner_ticks_backend` renders command-palette frame corner ticks
  through `UiBackend` and `PaintCmd`; the egui overlay still owns the floating
  area, shadowed frame, text-edit query box, and scroll host.
- Backend-neutral command-palette corner tests prove enabled ticks emit eight
  line commands and disabled ticks emit nothing.
- `palette_dash_separator_backend` renders command-palette input/result and
  inter-row dashed separators through `UiBackend` and `PaintCmd` line segments
  instead of direct egui painter calls; separator dash/color policy now lowers
  into Mara-side `PaletteSeparatorSpec` data before rendering.
- Backend-neutral command-palette dash tests prove dashed separators emit line
  segments, invalid dash settings emit nothing, and inter-row separator policy
  omits the divider after the last row.
- `palette_no_matches_backend` renders the command-palette empty-state row
  through `UiBackend` and `PaintCmd` instead of egui labels.
- A backend-neutral command-palette empty-state test proves the no-match row
  emits the expected text command.
- `palette_search_chrome_backend` renders the command-palette query-field fill
  and border through `UiBackend` and `PaintCmd`; query-field fill/text/hint
  colors now lower through Mara-side `PaletteSearchColors` policy, while the
  actual query text editing, focus, and keyboard behavior remain egui-owned for
  now.
- A backend-neutral command-palette search-chrome test proves the field emits
  fill/stroke commands and returns a carved text rect for the backend-owned
  editor; search-color policy tests prove glass-fill tinting and hint alpha are
  Mara color data.
- `palette_scrim_backend` allocates the full-window command-palette dismissal
  hit target through `UiBackend` and returns `MaraResponse`; the floating area
  host itself remains egui-owned for now.
- A backend-neutral command-palette scrim test proves the dismissal hit target
  allocates the requested rect without paint commands.
- All animated button fill styles now lower to `PaintCmd` through
  `fill_paint_cmds`, including polygon, rectangle phase, circle-grow,
  equalizer, and criss-cross effects; the old per-style egui painter helpers
  were removed.
- Animated-button click-pulse timestamp state now lowers through Mara
  `MaraMemory`, and click-pulse ring geometry now lowers to Mara
  `PaintCmd::RectStrokeOutside`; the current animation clock remains
  egui-backend-owned for now.
- Backend-neutral animated-fill and click-pulse-memory tests prove every
  `FillStyle` produces Mara paint commands without direct egui painter geometry
  and that pulse timestamps / pulse ring paint data do not depend on direct
  egui context data storage or direct egui painter geometry.
- Tree-row chevrons now lower to `PaintCmd::Polyline` through
  `chevron_paint_cmd`; the egui tree row wrapper still owns interaction,
  animation timing, labels, icons, and row layout for now.
- A backend-neutral tree chevron test proves the rotated chevron geometry is
  represented as Mara paint data.
- Tree-row indent guides and two-line tree-action branch guides now lower to
  `PaintCmd::Line` through Mara vocabulary geometry; egui remains only the
  current renderer for those commands.
- Backend-neutral tree-guide tests prove both ordinary indent guides and
  directory-style action-row guides emit Mara line commands without depending
  on egui shapes.
- Tree right-gutter slot icons now use Mara paint commands for eye, lock,
  glyph, and material swatch variants; `TreeIconKind::Color` stores Mara
  `Color32` instead of exposing raw `egui::Color32`.
- A backend-neutral tree-slot test proves all built-in slot icon variants emit
  Mara paint commands.
- Tree action-row body/tail rectangle chrome now travels through Mara
  `PaintCmd::RectFilled` / `PaintCmd::RectStroke` before the egui backend
  renders it.
- The egui backend can convert simple Mara paint commands back into egui
  shapes when existing z-ordered painter slots still need a shape value during
  migration.
- `PaintCmd::Clip` now exists and the egui backend renders child commands
  inside a clipped painter, giving non-egui backends an explicit clipping
  contract instead of relying on hidden egui painter state.
- Tree single-line labels and two-line action-row labels now lower to clipped
  Mara text commands; egui still owns the surrounding row interaction and
  type-icon painting.
- A backend-neutral tree-label test proves the single-line and two-line label
  helpers emit clipped text commands.
- Tree row/action-row accent inputs now accept Mara `Color32` vocabulary
  instead of naming `egui::Color32` in the public tree-row signatures.
- Loose raw-egui tree row functions and `TreeBody::new` are crate-internal;
  public app code reaches tree rows through `MaraUi::tree`, `Pod::with_tree`,
  and the typed `TreeBody` methods instead of constructing rows from a raw
  `egui::Ui`.
- `PaintCmd::TextWithFamily` and `TextFamily::Named` now let Mara paint data
  carry bundled icon glyphs without exposing an egui font type.
- `PaintCmd::Shadow` now lets Mara paint data carry shadow chrome; the egui
  backend renders it and can convert it back into an egui shape while legacy
  z-slot migration code still needs shape values.
- `PaintCmd::RectStrokeOutside` now lets Mara paint data carry outside-stroke
  chrome such as button pulse rings without baking egui's `StrokeKind` into
  widget logic.
- A backend-neutral egui-backend test proves Mara shadow commands can be
  converted to renderable egui shapes.
- Tree type-icons and action glyphs now lower to Mara text paint commands:
  bundled icons use named-font text, while unknown icon names fall back to
  normal text.
- Tree row type-icons now use the same Mara paint-command path as action-row
  icons instead of calling a raw egui icon painter.
- A backend-neutral tree type-icon test proves bundled icon names and fallback
  text both emit Mara paint commands.
- Ordinary tree-row body, chevron, type-icon, and slot geometry now lowers
  through Mara `Rect`/`Vec2` helpers before the current egui backend registers
  interactions.
- Two-line tree action-row body, chevron, type-icon, tail action, and label
  geometry now lowers through Mara `Rect`/`Vec2` helpers before the current
  egui backend registers interactions.
- Ordinary and two-line tree rows now register body, chevron, and tail/icon
  slot hit targets through the Mara `UiBackend::interact` contract and use
  Mara response snapshots for hover/click/pressed state before asking the
  current egui backend to attach tooltips or animate.
- Tree row paint submission now goes through egui-backend adapter helpers for
  paint commands and deferred background slots; `tree.rs` no longer calls raw
  `ui.painter()`, `render_paint_cmd`, `shape_from_paint_cmd`, `Shape::Noop`,
  or `egui::Color32` directly.
- Tree row access to current backend context, UI identity, available width,
  clip/visibility, input snapshots, animation clocks, and memory facades now
  goes through egui-backend adapter helpers; `TreeBody` exposes Mara memory
  helpers instead of raw context handles.
- Container tab-cell chrome has started moving to Mara paint commands: folder
  tab active fills/drop targets, GAME top-tab inactive fills/drop targets, and
  top-tab labels now lower through `PaintCmd`.
- Container GAME title/banner fills and corner ticks now lower through Mara
  paint data before the egui backend renders them.
- Backend-neutral container chrome tests prove tab rectangle chrome, top-tab
  labels, and corner ticks emit Mara paint commands.
- Container named icons now lower to `PaintCmd::TextWithFamily` for folder
  tabs, GAME top tabs, and floating title icons.
- Container SVG icons now lower to `PaintCmd::Svg` for folder tabs, GAME top
  tabs, and floating title icons; the current egui backend still owns the
  image/SVG loader implementation, but container code no longer calls the loose
  raw egui icon hook.
- Ribbon glyphs, draggable slot-item icons, and tab-drag preview icons now
  lower through Mara icon paint commands; SVG still flushes through the egui
  backend image loader, but the chrome no longer calls the raw egui icon hook.
- Ribbon button background/border chrome and drag-drop outlines now lower to
  Mara rect paint commands before the egui backend renders them; the raw
  `Painter::rect` / `StrokeKind` calls are guarded out of ribbon paint/chrome.
- Ribbon button foreground-colour policy now returns Mara `Color32` vocabulary,
  so glyph tint decisions no longer expose raw egui colours across ribbon paint
  and chrome helpers.
- Ribbon paint/chrome/slot helpers now take Mara `Color32` for accent and glyph
  tint at their internal seams; raw egui colour conversion remains only inside
  implementation details that query egui-backed RGB channels.
- Simple slot-ribbon origin/size/button-rect geometry now uses Mara `Pos2`,
  `Vec2`, and `Rect` vocabulary before converting at the current egui Area /
  interaction boundary.
- Featureful ribbon button placement now stores Mara `Rect`/`Align2`/`Vec2`
  data and resolves button screen rects as Mara `Rect` values; egui conversion
  is limited to the current Area positioning and interaction calls.
- Featureful ribbon rail, strip, and cluster hit-region geometry now returns
  Mara `Rect` values, so drag/drop target selection and native window chrome
  drag-region publication consume Mara geometry before converting at host
  boundaries.
- Featureful ribbon drag cursors now store Mara `Pos2` vocabulary in
  `RibbonDrag`; current egui pointer reads convert once at input capture and
  current Area painting converts once at the host boundary.
- Floating ribbon/pane chrome bounds are now stored in backend temp state as
  Mara `Rect` vocabulary instead of raw `egui::Rect`, so pane auto-folding,
  shelf publication, and ribbon placement share Mara geometry across the
  first-party chrome boundary.
- Floating pane anchor-to-position layout now consumes Mara `Rect` screen
  bounds plus Mara `Align2`/`Vec2` anchor data and returns a Mara `Pos2`, so
  pane placement no longer requires chrome-boundary geometry to be converted
  back into raw egui types before calculating the anchor position.
- Floating pane published input-firewall rects and one-frame clip recovery
  state now store Mara `Rect` values; the current egui frame response is
  converted once when captured, and the clip rect converts back only when
  applying the current egui host clip.
- Floating pane resize-handle rectangle geometry now lowers through Mara
  `Rect` helpers; the current egui host converts those rectangles only when
  registering interaction hit targets and painting hover/drag feedback.
- Floating pane resize-handle hover/drag indicator chrome now lowers to a Mara
  `PaintCmd::RectFilled`; egui only flushes that command through the current
  backend renderer.
- Button card glyphs and fullscreen minimize-chip glyphs now lower through
  Mara icon/text paint commands; startup rendering still skips named-font icon
  flushes until the egui font registry is installed.
- Public `Tab` construction/access now uses Mara-owned identity vocabulary:
  `Tab::new` no longer names raw `egui::Id`, `Tab::id` returns Mara `Id`, and
  the current egui id is kept behind a crate-internal adapter.
- `Tab::new` now accepts Mara `Id` vocabulary directly instead of a generic
  hash source, preventing already-built Mara IDs from being rehashed and
  breaking shared tab routing after containers move between panes.
- Public `ContainerSpec` / `PaneBody` identity helpers now use Mara `Id`
  vocabulary for container ids, pane ids, response-map keys, search-pod keys,
  and temp-string keys; current egui ids stay behind crate-internal adapters.
- Container title chevrons now lower to `PaintCmd::Polyline`.
- Backend-neutral container icon and chevron tests prove named icons and title
  chevrons emit Mara paint commands.
- Container title body-facing divider rules now lower to `PaintCmd::Line`
  instead of direct egui `hline`/`vline` calls.
- A backend-neutral title-divider test proves horizontal and vertical title
  dividers emit Mara line commands.
- Container title strips and tab cells now use Mara `CursorIcon::PointingHand`
  policy before the egui backend applies the concrete cursor.
- Container title drag starts, tab drag/drop targeting, top-tab animations, and
  corner-snap timing/repaint requests now read pointer/time/animation/repaint
  through the egui backend adapter instead of calling raw context input helpers
  directly from `container/normal`.
- Container title-strip and tab-cell hit targets now allocate/interact through
  `UiBackend` and consume `MaraResponse` snapshots, so `container/normal`
  no longer calls raw `allocate_exact_size` or `ui.interact` for those chrome
  targets.
- Container folder-tab strip/body union reservation now goes through
  `UiBackend::reserve_rect`, so `container/normal` no longer calls raw
  `ui.allocate_rect` to advance parent layout after projected tab chrome.
- Container body visible-slot reservation, title/body gaps, body top padding,
  inherited stack direction, and full-body child viewport creation now lower
  through `UiBackend`, `SpaceSpec`, `ItemSpacingSpec`, `StackDirection`, and
  `ChildRegion` before the egui backend performs concrete layout calls.
- Container folder-tab active-body child UI creation now goes through the egui
  backend adapter's current-layout child-rect helper instead of constructing
  raw `UiBuilder` / `new_child` hosts in `container/normal`.
- Container folder-tab max-rect reservation now starts from backend-provided
  Mara `Rect` geometry and returns Mara `Rect`, keeping that strip-reservation
  policy out of concrete egui rect constructors.
- Container folder-tab chrome, top-tab labels, and named tab icons now submit
  paint commands through `UiBackend::paint` instead of calling
  `render_paint_cmd(ui.painter(), ...)` directly from `container/normal`.
- Container title chevrons, title text runs, and title/body divider paint now
  submit through `UiBackend::paint` plus Mara clip scopes instead of creating
  `painter_at`/raw painter handles in `container/normal`.
- Container GAME title-banner deferred paint now uses the egui backend's
  deferred paint-command slot helper, so `container/normal` no longer creates
  raw `Shape::Noop` placeholders or calls `shape_from_paint_cmd` directly.
- Container floating title icons and corner ticks now render through a backend
  z-layer paint helper, moving raw `layer_painter`, per-tier painter creation,
  and SVG layer child-UI creation out of `container/normal`.
- Container tab SVG icon paint now also submits through `UiBackend::paint`; the
  egui backend's `UiBackend` paint implementation owns SVG/UI paint dispatch
  instead of requiring `container/normal` to call `render_paint_cmd_ui`.
- Container folder-tab cell geometry now lowers through a pure Mara
  `FolderTabCellGeometry` helper with Mara rect/corner data before the current
  egui backend consumes it.
- Container GAME/top-tab cell geometry now lowers through a pure Mara
  `TopTabCellGeometry` helper with Mara rect and icon/label center data before
  the current egui backend consumes it.
- Container folder-tab strip placement and GAME/top-tab title expansion now
  lower through pure Mara rect helpers before the current egui backend consumes
  the geometry.
- Container separator debug regions, GAME title-banner extents, and outer
  corner-tick bounds now lower through pure Mara rect helpers before the
  current egui backend consumes the geometry.
- Container floating title-icon placement now lowers through pure Mara
  position/alignment/rect geometry before the current egui backend renders it
  on the z-layer.
- Container title-slot sizing, body visible/full sizing, and body full-rect
  anchoring now lower through pure Mara `Vec2`/`Rect` helpers before the current
  egui backend performs allocation or child-viewport creation.
- Pane-level container dot handles now use Mara `CursorIcon` and Mara
  `Color32`, and their three-dot chrome lowers to `PaintCmd::CircleFilled`
  before the egui backend renders it.
- Pane drag previews and pane resize handles now use Mara `CursorIcon`
  policies before the egui backend applies concrete grabbing/resize cursors.
- Pane container/tab drag preview host policy now lowers through Mara
  `AreaHost`/`Layer` data, including non-interactive overlay behavior, before
  the egui backend constructs the concrete tooltip-layer area.
- Floating pane body areas and pane resize-handle areas now use Mara
  `AreaHost`/`Layer` data before the egui backend constructs the concrete
  current-frame area; pane code no longer names raw egui area/order
  construction for those hosts.
- Shelf container/tab drag previews and shelf resize handles now use Mara
  `CursorIcon` policies before the egui backend applies concrete grabbing and
  resize cursors.
- Shelf render hosts and shelf move/container ghost hosts now lower through
  Mara `AreaHost`/`Layer` data, including passive foreground ghost areas,
  before the egui backend constructs the concrete area.
- Shelf body scroll hosts now lower through Mara `ScrollRegion` data for
  horizontal bottom shelves and vertical side shelves, including max extent and
  zero item-spacing policy before the egui backend constructs the concrete
  scroll area.
- Shelf body child viewports now lower through Mara `ChildRegion` data for
  bottom horizontal stacks and centered side-shelf columns before the egui
  backend constructs the concrete child `Ui`.
- Shelf move/container ghost local allocation now uses Mara `UiBackend`
  allocation, leaving direct concrete `allocate_exact_size` calls out of the
  shelf renderer.
- Shelf background fill now lowers to `PaintCmd::RectFilled`, with a
  backend-neutral test proving the background command data; the current shelf
  renderer now submits that command through `UiBackend::paint` instead of
  reaching for `ui.painter()` directly.
- Shelf reservation ghost fill and center-facing border now lower to
  `PaintCmd::RectFilled` and `PaintCmd::Line`, with a backend-neutral test
  proving the command data; the current egui shelf renderer flushes those
  commands through the backend paint contract.
- Shelf container-slot ghost fill and stroke now lower to
  `PaintCmd::RectFilled` and `PaintCmd::RectStroke`, with a backend-neutral
  test proving the command data; the current egui shelf renderer flushes those
  commands through the backend paint contract.
- Maximizable embed overlay chips now use Mara `CursorIcon::PointingHand`
  before the egui backend applies the concrete hover cursor.
- Maximizable embed ribbon-style chip chrome now lowers to
  `PaintCmd::RectFilled` and `PaintCmd::RectStroke`, with a backend-neutral
  test proving the command data.
- Maximizable embed fullscreen placeholder text and full-window overlay
  background now lower to `PaintCmd::Text` and `PaintCmd::RectFilled`, with
  backend-neutral tests proving the command data.
- Maximizable embed fullscreen arrow glyphs now lower to `PaintCmd::Line` and
  `PaintCmd::Polygon`, with a backend-neutral test proving the command data.
- Maximizable embed chip snap-target ghost now lowers to `PaintCmd::RectFilled`
  and `PaintCmd::RectStroke`, with a backend-neutral test proving the command
  data.
- Maximizable embed fullscreen screen geometry now starts from a backend-owned
  host content-rect read returning Mara `Rect`; concrete egui rect conversion
  only happens at current host-area boundaries.
- Maximizable embed fullscreen chip anchor and nearest-anchor geometry now use
  Mara `Rect`/`Pos2` helpers, with backend-neutral tests proving the anchor
  positions and snap selection.
- Maximizable embed inline chip placement and overlay button position now use
  Mara `Rect`/`Pos2` vocabulary before conversion at the current egui `Area`
  boundary.
- Maximizable embed fullscreen overlay, content host, restore chip, and snap
  ghost now lower their area/layer/painter policy through Mara `AreaHost`,
  `Layer`, `Id`, and `Rect` contracts before the egui backend constructs the
  concrete area or layer painter.
- Maximizable embed accent inputs now use Mara `Color32` vocabulary internally;
  conversion to concrete egui colors is limited to legacy icon/style adapter
  calls.
- Maximizable embed inline size input now accepts Mara `Vec2` vocabulary while
  preserving compatibility through the current egui backend conversion.
- Maximizable embed paint-wrapper geometry now accepts Mara `Rect` vocabulary
  before the egui backend renders the current paint commands.
- Maximizable embed fullscreen owner/state keys now use Mara `Id` vocabulary
  for the public helpers and module comparisons, with egui ID conversion kept
  inside the current backend data-storage boundary.
- Graph/code fullscreen-owner helper comparisons now use Mara `Id` vocabulary;
  the graph fullscreen key no longer seeds through a raw egui ID.
- Graph/code no longer expose public `is_*_fullscreen(&egui::Context)`
  convenience helpers; callers compare the public Mara fullscreen keys against
  the embed fullscreen owner instead of using extra raw-context helpers.
- The graph sharp-zoom secondary egui context accessor is no longer a loose
  public `NodeViewState::ctx()` API; Mara's themed graph wrapper uses a hidden
  first-party hook while standalone graph consumers stay on the egui-backed
  renderer contract.
- Maximizable embed fullscreen chip drag tracking now stores Mara `Pos2`
  vocabulary in backend temp data and reads pointer-interaction positions
  through the egui backend adapter instead of naming raw `egui::Pos2` in the
  widget logic.
- Maximizable embed placeholder, fullscreen background, inline body, overlay
  button, and restore-chip allocations now route through `UiBackend` and return
  `MaraResponse`; hover text/cursor affordances go through egui backend adapter
  helpers instead of raw `egui::Sense`, `allocate_*`, or response extension
  calls in the widget logic.
- `ViewCtx::content_rect`, `ViewCtx::screen_rect`, and
  `ViewCtx::ribbon_avoiding_rect` now return Mara `Rect` vocabulary, so view
  code can reason about content geometry without receiving raw `egui::Rect`
  values from the sealed view context.
- `RibbonAvoidance::apply_to_rect` now returns Mara `Rect` vocabulary, and the
  raw-context `ribbon_avoiding_rect` / main-bar empty-drag helpers are
  crate-internal; public geometry access goes through `ViewCtx` and Mara
  vocabulary instead of loose egui-context helpers.
- The code-editor wrapper now accepts Mara `Color32`/`Vec2` vocabulary at its
  public entry points while keeping the concrete `egui_code_editor` theme
  conversion inside the current egui adapter boundary.
- The node-graph wrapper now accepts Mara `Color32`/`Vec2` vocabulary at its
  public entry points, and its resize/recenter bookkeeping stores Mara `Vec2`
  in backend temp data instead of raw `egui::Vec2`.
- The canvas module's retained stroke document now stores Mara `Pos2` and
  `Color32` vocabulary, and its canvas sizing path accepts Mara `Vec2` before
  painting through Mara `MaraUi::canvas` / `MaraPainter` instead of raw egui
  painter allocation and text/stroke APIs.
- The image module placeholder now paints through Mara `MaraUi::canvas` /
  `MaraPainter` and its inline module UI uses Mara labels/buttons instead of
  reaching for a raw egui UI.
- The map module's public tile prewarm and root-map show APIs now take Mara
  `ViewCtx` plus Mara `Vec2` vocabulary instead of raw `egui::Context`; the
  concrete egui prewarm/show hooks are crate-private internals, and the full
  demo builds a temporary Mara view context for root map rendering.
- The map module's retained annotation colors/strokes now use Mara
  `Color32`/`Stroke` vocabulary, and the old public `egui_id` helper was
  replaced with a Mara-id helper so annotation documents do not expose raw
  egui color/stroke/id types.
- The 3D module's renderer-facing viewport snapshot now stores Mara `Pos2` and
  `Vec2` for pointer/scroll data, the renderer pick hook accepts Mara `Pos2`,
  and its inline module summary uses Mara labels/buttons instead of reaching
  through a raw egui UI.
- The 3D module's public retained-scene color alias now resolves to Mara
  `Color32` vocabulary, and raw `egui::Response`/`egui::Ui` viewport snapshot
  construction is hidden behind first-party backend hooks instead of loose
  public APIs.
- The Bevy viewport bridge's picked-color output now uses Mara `Color32`
  vocabulary, and the egui-hosted Bevy viewport `show` entry accepts Mara
  `Color32` before converting at current egui painting boundaries.
- The Bevy viewport bridge now renders through a sealed `ViewCtx` entry point
  instead of requiring demo/app code to pass a raw `egui::Context`; the
  concrete egui context is reached only by the module's current backend
  implementation.
- The old `EmbeddedBevyViewport` compatibility alias was removed; app code
  uses the canonical `MaraBevyViewport` type exported through Mara's module
  facade.
- `ViewCtx`, `ModuleInlineCtx`, and `WorkspaceCtx` now carry their ambient
  accent as Mara `Color32` vocabulary, with raw egui color conversion kept at
  widget/backend boundaries.
- `MaraHostCtx::view_ctx` now builds sealed `ViewCtx` values from host glue, so
  the demo's map prewarm/root map, Bevy viewport, and 3D root surfaces no
  longer construct view contexts from loose raw egui context variables.
- `ViewCtx::new(&egui::Context, ...)` is no longer public app API; the raw
  context constructor is a hidden first-party hook used by `MaraHostCtx`, so
  app code receives view contexts through the sealed host facade.
- `ViewCtx::painter`, `ViewCtx::overlay_painter`, and `ViewCtx::body` now
  express their host/layer/clip policy with Mara `Layer`, `AreaHost`, `Id`, and
  `Rect` data, then route through the egui backend adapter; direct egui
  painter/layer/area construction is no longer in the sealed view context.
- `MaraHostCtx` now owns root-body and shelf layout/show facades, so the demo's
  root canvas surface no longer passes `host.__internal_egui()` into a loose
  root-view function or wraps its root body with `MaraUi::__internal_from_raw`
  in app code.
- Public shelf layout reservation now uses Mara geometry vocabulary:
  `ShelfLayout` stores Mara `Rect` values, `layout_shelves` accepts Mara
  geometry input, and `shelf_insets` returns Mara `Vec2`; egui rect conversion
  remains inside the current shelf/ribbon backend adapters.
- The shelf renderer that takes raw `egui::Context` is no longer root-
  reexported as app-facing API; views/hosts render shelves through
  `ViewCtx::show_shelves` or `MaraHostCtx::show_shelves`, with the concrete
  egui renderer kept behind a hidden first-party hook.
- Pod builder APIs that accept an accent now take `impl Into<Mara Color32>`,
  so sealed app code can pass Mara vocabulary accents without depending on raw
  egui color types while existing egui-backed hosts still convert at the edge.
- Public `Pod` identity APIs now use Mara `Id` vocabulary: construction,
  readback, search-buffer keys, resizable-height keys, fill-height keys, and
  module response IDs no longer expose raw egui IDs.
- `Pod::show(&mut egui::Ui)` is now crate-internal backend plumbing; public app
  code renders pods through `MaraUi::pod` or Mara container bodies instead of
  directly passing raw egui UIs.
- Workspace-level stack/bar/module context identity now uses Mara `Id`
  vocabulary for workspace level IDs, module owners, bar IDs, bar item IDs,
  module inline pod IDs, and module-workspace push IDs; raw egui conversion
  remains only at current ribbon scope / egui area boundaries.
- Ribbon identity now uses Mara `Id` vocabulary for slot actions, workspace
  scopes, overridable slot IDs, slot item IDs, slot definition IDs, resolved
  ribbon IDs, and click payload IDs; raw egui IDs remain only inside current
  egui rendering/chrome adapters.
- Public pane identity entry points now use Mara `Id`/`Color32` vocabulary for
  `Pane::new` and the ribbon-pane registry; the active-pane ctx-data key is no
  longer public API.
- Ribbon-pane publication now goes through `MaraHostCtx::publish_ribbon_pane_ids`
  with Mara `Id` vocabulary, so the demo no longer builds raw egui ids or calls
  a loose `mara_core::pane::*` raw-context helper before pane rendering.
- Featureful ribbon drawing, host input time, and repaint requests now go
  through `MaraHostCtx`, so the full demo's ribbon assembly no longer grabs a
  raw egui context or compares action clicks with raw egui IDs.
- Featureful ribbon button hosts, drag/drop outline hosts, and simple
  slot-ribbon hosts now lower through Mara `AreaHost`/`Layer` data before the
  egui backend constructs the concrete foreground/overlay areas.
- Featureful ribbon button allocation, slot-ribbon item hit tests, hover text,
  and area raise-to-top behavior now go through Mara `UiBackend` /
  `MaraResponse` data plus egui backend adapter helpers instead of inline raw
  egui response/area calls in ribbon code.
- Featureful ribbon drag-button paint positioning, glyph centering, and live
  drag cursor tracking now stay in Mara `Pos2`/`Rect` vocabulary; pointer
  interaction reads go through the egui backend adapter instead of direct
  `egui::Pos2` construction or raw context pointer calls in ribbon chrome.
- Featureful ribbon empty-main-bar drag detection now reads primary pointer
  press/interact position through an egui backend adapter helper and evaluates
  hit geometry in Mara `Rect`/`Pos2`, instead of calling raw egui input and
  `PointerButton` APIs in ribbon chrome.
- Slot-ribbon shelf/window-control augmentation now asks the egui backend
  adapter for host viewport maximized state instead of reading raw egui
  `ctx.input(... viewport().maximized ...)` from ribbon slot painting.
- Simple slot-ribbon screen geometry now starts from the egui backend adapter's
  Mara `Rect` content-rect read instead of calling raw `ctx.content_rect()` in
  slot painting before computing Mara-side ribbon origins.
- Featureful ribbon chrome bounds, ribbon-avoidance, and top-ribbon screen
  geometry now read host content geometry through the egui backend adapter as
  Mara `Rect`, instead of calling raw `ctx.content_rect()` from ribbon chrome.
- Pane animation clocks, overflow screen fallback, drag-release checks, and
  live drag cursor reads now go through the egui backend adapter, so pane
  rendering no longer calls raw egui input/content/pointer APIs directly before
  making Mara-side layout and drag decisions.
- Pane container-dot resize handles now allocate, hit-test, and return drag
  state through `UiBackend`/`MaraResponse`, store their hit rects as Mara
  `Rect` data, and keep the resize cursor as backend adapter policy instead of
  exposing a public raw-egui `Response` helper.
- Pane drag ghost gaps and floating drag previews now allocate/paint through
  `UiBackend`, `AreaHost`, and Mara `PaintCmd` rectangle commands; raw egui
  rectangle construction, strokes, senses, and direct painter rect calls are no
  longer used in the non-test pane-drag renderer.
- Tab-drag floating previews now take Mara `Vec2` sizing and lower their
  preview chrome through Mara `PaintCmd` rectangle commands inside an
  `AreaHost`; non-test tab-drag preview rendering no longer constructs raw
  egui positions/strokes or paints rectangles directly.
- Pane resize handles now hit-test through `UiBackend` and `MaraResponse`, and
  their hover/drag indicator paint is submitted through the backend paint
  contract, so pane resize logic no longer uses direct `ui.interact`,
  `Sense::click_and_drag`, or raw egui rect conversion in the resize renderer.
- Shelf container-drag, tab-drag, resize, move-target, and release/escape
  checks now read pointer/key state through the egui backend adapter instead of
  calling raw context pointer/input methods directly from shelf layout logic.
- Shelf background move dragging and shelf resize handles now hit-test through
  `UiBackend` and return `MaraResponse` snapshots; shelf layout logic no
  longer calls raw `ui.interact` / `egui::Sense` for those structural chrome
  targets.
- `make check` now guards the whole core crate against reintroducing raw
  foreground/tooltip/middle `egui::Area` / `LayerId` / `Painter`
  construction outside the egui backend adapter.
- `RibbonSlotClick` no longer exposes an optional raw egui response; ribbon
  dispatch carries Mara IDs and the resolved action only.
- Loose public raw-context ribbon draw functions are no longer re-exported as
  app-facing API. Current egui slot painting is behind hidden first-party
  hooks, with app/demo rendering routed through shell or `MaraHostCtx`
  facades.
- App-shell render helpers that take raw `egui::Context` are no longer
  root-reexported as app-facing API. The public app-shell surface now keeps
  resolution/dispatch data visible, while current egui rendering lives behind
  hidden first-party hooks until app-shell rendering has a Mara host facade.
- The command-palette raw-context renderer is no longer root-reexported as
  app-facing API; app hosts use `MaraHostCtx::command_palette`, which keeps
  the current egui context behind the sealed host boundary.
- Pane openness and user-resize persistence helpers are now crate-internal;
  public app code configures panes through the `Pane` builder instead of raw
  context-backed storage functions.
- Container layout persistence helpers now accept Mara `Id` vocabulary and are
  crate-internal implementation details for initial-flow, flow-size, and
  intrinsic-size storage; app code configures initial flow through container
  specs instead of reaching into raw context-backed storage helpers.
- Pane body bookkeeping helpers for fold state, min-flow caches, container CID
  caches, extra-flow caches, and ribbon-edge caches are now crate-internal; the
  only cross-crate pane-rect firewall output returns Mara `Rect` vocabulary.
- App-shell entry points now accept Mara `Color32` vocabulary for the ambient
  accent and convert to raw egui color only when invoking the current slot
  ribbon painter.
- Slot-ribbon paint entry points now accept Mara `Color32` vocabulary and
  convert once inside the current egui renderer implementation.
- The permanent shell no longer carries an unused raw-egui accent through
  `build_ribbon`; its accent now stays at the slot-ribbon backend boundary.
- `ShelfDef` now stores Mara `Color32` vocabulary for public shelf accent
  configuration and converts only at current egui shelf/pane painting edges.
- `ShelfDef` now stores Mara `Id` vocabulary for public shelf identity, with
  raw egui IDs generated only through the crate-internal shelf adapter for
  current egui state, area, drag, and resize keys.
- Command-palette floating-frame shadow, fill, and border now lower to
  `PaintCmd::Shadow`, `PaintCmd::RectFilled`, and `PaintCmd::RectStroke`;
  frame host width/inner-margin/corner policy now lowers through Mara
  `FrameHostSpec`, and deferred frame-chrome paint slots are now owned by the
  egui backend adapter; egui still owns the Area, concrete text edit, and
  concrete scroll host.
- A backend-neutral command-palette frame-chrome test proves the frame shadow,
  fill, and stroke emit Mara paint commands.
- Command-palette selection clamping, keyboard actions, and overlay placement
  are now pure Mara-side helpers; command-palette keyboard consumption now uses
  Mara `MaraKey` plus an egui backend adapter instead of inline egui key/input
  calls.
- Command-palette frame/content geometry now has a pure Mara-side
  `palette_frame_layout` helper for overlay position, outer width, content
  width, and result-list max height; frame host policy now lowers to
  `FrameHostSpec`, while egui still consumes the area and scroll values through
  the current concrete `Area` and `ScrollArea`.
- Command-palette fixed input/result vertical gaps now lower through Mara
  `SpaceSpec`, with egui only applying the concrete spacing.
- Command-palette screen/content geometry now starts from a backend-owned host
  content-rect read that returns Mara `Rect` data; overlay policy then stays in
  Mara-side helpers.
- Command-palette input/result and inter-row separator style now lowers through
  pure Mara-side separator policy helpers instead of recomputing dash/color
  data inline in the egui body.
- Command-palette query-field color policy now lowers through pure Mara-side
  color helpers instead of computing egui `Color32` hint/fill values inline in
  the egui body.
- Command-palette scrim/window area identity, position, and layer policy now
  lower into Mara layout `AreaHost` data, then through the egui backend area
  show adapter; egui still hosts the concrete `Area`.
- Command-palette query edit policy now has a pure Mara-side helper for
  selection reset and focus-request intent; the egui backend now executes both
  the concrete `TextEdit` and the concrete focus request.
- Command-palette query edit geometry now lowers to Mara layout
  `TextEditRegion` data before the egui backend creates the child UI for the
  current concrete `TextEdit`.
- Command-palette query edit widget policy now lowers to Mara layout
  `TextEditSpec` data before the egui backend creates the concrete single-line
  `TextEdit`.
- Command-palette keyboard navigation now snapshots consumed `MaraKey` values
  through the egui backend adapter and applies the Up/Down/Enter/Escape state
  machine through a pure Mara helper.
- Command-palette no longer builds the concrete egui query `TextEdit` inline
  and no longer branches on concrete focus state; it asks the egui backend to
  show the text editor from Mara `TextEditSpec` data and apply the
  focus-if-unfocused policy behind the backend helper.
- Command-palette accent input now uses Mara `Color32` vocabulary, and frame,
  search-field, and row color policy stays in Mara color data instead of
  converting through concrete egui colors inside palette logic.
- Command-palette result scroll-region policy now has a pure Mara-side helper
  returning Mara layout `ScrollRegion` data for list identity, shrink behavior,
  max height, and row spacing, then through the egui backend scroll adapter,
  including backend-owned row-spacing application; egui still hosts the
  concrete `ScrollArea`.
- Backend-neutral command-palette state/geometry tests prove keyboard movement,
  dismissal/pick behavior, overlay positioning, frame layout, and query-edit
  policy, area-host policy, spacing policy, separator policy, search-color
  policy, and scroll-region policy without egui.
- The window-owned runner no longer exports the old public `App` alias; apps
  use the canonical `AppRunner` name so the API surface does not carry
  compatibility clutter.

Still needed:

- continue reducing direct egui use in ribbon/tab geometry/layout and the
  command-palette Area/inner-margin/text-edit/scroll behavior.
- remove the remaining egui-`Ui` wrapper signatures once callers have migrated
  to `MaraUi`/backend entry points.

Port widgets in this order:

1. label/readout
2. chip/badge/keybinding
3. button/card button/action button
4. toggle
5. progressbar
6. slider
7. drag value without text editing

Leave difficult widgets on the egui backend until later:

- text input
- dropdown popup
- tree
- command palette
- code editor
- node graph
- 3D viewport

Acceptance:

- simple widgets do not call egui directly.
- their rendering goes through Mara paint/layout/input contracts.

### Phase 6: Port pane/container/ribbon/shelf layout

This is the real UI-engine step.

Done:

- Raw-egui `Normal::show` / `Normal::show_tabs` and container separator
  paint helpers are crate-internal backend plumbing now; public app code
  reaches container rendering through `PaneBody`/Mara container specs rather
  than passing raw egui UIs.
- Pane anchoring now computes `PanePlacement`/outer rects in Mara vocabulary;
  the egui backend receives the already-decided placement instead of owning the
  anchor math through `Area::anchor`.
- Container body layout now flows through `ContainerBodySpec` plus backend
  slot rendering, so scroll host sizing/end padding are described by Mara data
  instead of being open-coded in the container body helper.
- Pane inner flex/title/body-scroll policy now flows through `PaneFlexSpec` and
  `PaneBodyScrollSpec`; `Pane::lay_out_flex` no longer open-codes egui
  spacing, title allocation, or sticky scroll host setup.
- Simple slot-ribbon placement now flows through `SlotRibbonLayoutSpec` plus
  an egui backend area helper; the slot painter computes item rects in Mara
  geometry and lowers button chrome through `PaintCmd` instead of calling
  `ui.painter()` directly.
- Featureful ribbon buttons/drop outlines now reuse `SlotRibbonLayoutSpec` and
  backend area helpers too; the chrome renderer no longer opens egui Areas or
  paints button/outline chrome through raw `ui.painter()` calls.
- Shelf body viewport/scroll setup now routes through a backend helper fed by
  Mara `ChildRegion` and `ScrollRegion`; shelf rendering no longer manually
  creates child UIs, scroll areas, or sticky-scroll wrappers.
- Shelf main and ghost area hosts now route through `AreaSlotSpec` plus the
  egui backend area-slot helper, so shelf rendering no longer opens backend
  area hosts or sets area minimum size directly. Shelf resize/move repaint
  requests also go through the backend helper seam.
- Shelf resize, shelf move dragging, and container-move target/release updates
  now consume `MaraInput` snapshots from the backend adapter for pointer
  down/interact/latest/release state, reducing direct backend pointer helper
  calls in structural shelf logic.
- Shelf body container-drag and tab-drag viewport logic now also consumes a
  single `MaraInput` snapshot for interact/latest pointer and release state;
  non-test shelf code no longer calls direct backend pointer helper functions.

Port:

- pane anchoring
- pane resizing
- container flow
- tabbed containers
- shelf reservation
- ribbon placement
- app shell layout
- fullscreen embed layout

Acceptance:

- shell layout can be computed into Mara rects without egui.
- egui backend only paints/allocates based on Mara layout decisions.

### Phase 7: Advanced module portability

Treat modules as separate tracks.

#### Canvas

Likely easiest to port after paint/input abstraction.

Needs:

- stroke model already retained
- pointer input
- paint commands

#### Image

Needs:

- backend-neutral texture upload
- image draw command

#### Map

Needs:

- pointer transform/input
- vector paint commands; retained point/line/polygon annotations, selected
  feature chrome, draft geometry, Fluent/text/SVG icon annotations, MVT area
  fills, MVT ordinary feature lines, and MVT label paint already lower through
  Mara paint commands; `PaintCmd::Mesh` covers MVT extruded building side/roof
  geometry; `TextMeasureSpec` plus Mara `Rect`/`Vec2` state now covers MVT
  label measurement requests/collision data while the egui backend performs
  the current measurement
- retained map document is already mostly backend-neutral

#### Graph

Hard.

Current graph is an egui widget with egui-wgpu offscreen rendering. Keep as
egui-backed until the Mara runtime has:

- pan/zoom surface
- node layout
- cable/wire hit testing
- text labels
- texture/render target story

#### Code

Hardest after graph.

Needs:

- text editing
- cursor/selection
- keyboard navigation
- syntax highlighting
- scroll
- IME/copy/paste eventually

Keep egui-backed for a long time.

#### 3D

Needs a viewport contract:

```rust
trait ViewportBackend {
    fn allocate_viewport(&mut self, id: Id, min_size: Vec2) -> ViewportResponse;
    fn upload_texture(&mut self, image: ImageData) -> TextureId;
    fn paint_texture(&mut self, texture: TextureId, rect: Rect);
}
```

## Validation gates

Keep these as the default proof lanes:

```sh
nix develop --impure -c make check
nix develop --impure -c make test
```

Future gates to add:

```sh
make check-sealed
make check-boundary
make check-backend-egui
```

Suggested boundary checks:

- `example/Cargo.toml` must not mention `raw-egui`.
- `example/Cargo.toml` must not depend directly on `mara_core`.
- `example/Cargo.toml` must not depend directly on leaf module crates when they
  are exposed through `mara::ui::modules`.
- `example/sealed` must compile.
- public app-facing docs should point to `mara::ui`, not `mara_core`.

## Risks

### Text editing

Text input is expensive to own. Egui currently provides cursor movement,
selection, focus, clipboard, and some platform behavior. A custom backend will
need a real text-edit subsystem.

### Focus and keyboard routing

Command palette, dropdowns, text input, graph shortcuts, and editor shortcuts
all need a coherent focus model.

### Text shaping

Rendering text without egui means owning font loading, font atlas, shaping, and
measurement.

### Texture lifecycle

Images, graph render targets, 3D previews, and GPU-backed widgets need a
backend-neutral texture lifecycle.

### Keeping the API simple

Avoid leaking backend generics into app code. The public API should stay close
to the current sealed surface.

## Recommended next step

Continue **Phase 5: simple widget runtime migration**.

The next useful targets are:

- continue extracting command-palette host contracts: frame/content geometry,
  text-edit focus/query behavior, and scroll-region behavior.
- continue container/ribbon paint migration beyond titles: SVG image loading is
  now expressed as `PaintCmd::Svg` at container/map/ribbon/tab-drag call sites
  but still depends on the egui backend's image/SVG loader at flush time.
- keep separating harder widgets into backend-neutral chrome plus explicitly
  backend-owned behavior until Mara has popup, text-edit, focus, and scroll
  contracts.

Keep Phase 2 and Phase 3 open, but they are no longer the immediate bottleneck:
the current blocker is reducing direct egui layout/paint calls widget by
widget, without weakening the sealed consumer boundary.
