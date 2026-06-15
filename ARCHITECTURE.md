# Mara Architecture

Mara is a reusable egui UI library with thin host adapters. The core rule is:

> Shared UI behavior lives in `mara_core`; host crates only translate host
> runtime details into that core contract.

This matters because the same UI should work in Bevy, native eframe/plain-egui,
and the browser without rewriting shelves, ribbons, panes, widgets, window
chrome, or view/module logic for each host.

## Workspace layout

```text
crates/
  core/              mara_core: framework-agnostic UI core
  modules/
    graph/           mara_graph: standalone vendored node graph widget
    code/            mara_code: standalone vendored code editor widget
    image/           mara_image: proof View + Module image surface
    canvas/          mara_canvas: proof View + Module canvas surface

mara/
  plugin/
    bevy/            bevy_mara: Bevy + bevy_egui plugin facade
example/             root native/web example consuming mara
```

Package names:

| Package | Role |
| --- | --- |
| `mara_core` | Core UI contracts, theme, panes, ribbons, shelves, views, modules, widgets, window chrome |
| `mara` | Unified public facade for UI-only use and Mara-owned native windows |
| `bevy_mara` | Bevy plugin, input firewall, Bevy window chrome adapter, Bevy node-view backend |
| `mara_graph` | Standalone graph widget |
| `mara_code` | Standalone code editor widget |
| `mara_image` | Image View + Module proof crate |
| `mara_canvas` | Retained canvas View + Module proof crate |

## Sealed API: no raw egui in app code

Mara's public surface is sealed by default: an app consuming `mara` (or
`mara_core`) with default features cannot reach `egui::Ui`, `egui::Context`,
`egui::Painter`, or `egui::Response`, so GUI elements can only be created
through Mara's typed surface. The mechanism:

- `mara`/`mara_core` only re-export egui's inert *data* vocabulary
  (`mara_core::vocab`: `Color32`, `Pos2`, `Rect`, `Stroke`, `Id`, texture
  data types, …). Holding these grants no ability to paint raw widgets.
- Consumer drawing code receives sealed wrappers instead of egui
  capabilities:
  - `MaraUi` — the widget surface (module inline bodies, view bodies,
    foldable sections). Exposes Mara widgets, layout, `Pod` hosting, a
    `canvas()` primitive, and nothing else.
  - `MaraPainter` — typed custom drawing (lines, rects, circles, polygons,
    text in theme fonts, images).
  - `MaraInput` — per-frame input snapshot.
  - `MaraResponse` — plain-data interaction flags (egui's `Response` leaks
    the whole `Context` via its public `ctx` field, so it is never returned).
  - `ViewCtx` — sealed: `body(|mui| …)`, `painter()`, `show_pane`,
    `show_shelves`, `load_texture`; the inner `egui::Context` is private.
- Functions that merely *take* `&egui::Context`/`&mut egui::Ui` as inputs
  (the host-boundary `show_app_shell`/`apply_theme`/… family) stay public:
  they are uncallable by code that can never obtain those values.

Escape hatches, in increasing order of "you really meant it":

1. The `raw-egui` cargo feature on `mara`/`mara_core` re-exports `egui` and
   unlocks `MaraUi::raw_ui_mut`, `ViewCtx::egui_ctx`, `MaraHostCtx::egui`,
   `TreeBody::ctx`, `PaneBody::ctx`. Host glue (frame-loop drivers) and the
   root example enable it; a sealed app enabling it is a visible, greppable
   line in its `Cargo.toml`.
2. `#[doc(hidden)] __internal_*` accessors — used by first-party module
   crates (`mara_canvas`, `mara_image`, …) so they do not have to enable
   `raw-egui`. This matters because cargo features are additive across the
   dependency graph: if a first-party dependency enabled `raw-egui`, it
   would silently unseal `mara_core::egui` for every consumer. These
   accessors are not semver-stable and not part of the public API.

Rust has no "friend crates", so the seal is a misuse-resistance boundary,
not a security boundary: a determined consumer can always add their own
`egui` dependency or call hidden internals — but both are deliberate,
auditable acts, and neither can inject raw widgets into Mara surfaces
without them.

`example/sealed` (`mara_sealed_check`) is the compile-time proof: it
depends only on `mara` (no egui, no `raw-egui`) and exercises views,
modules, pods, widgets, canvas drawing, and input through the sealed
surface. If a change makes raw egui reachable or the sealed surface
insufficient, that crate is where it should break.

## Core ownership boundaries

### `mara_core`

`mara_core` owns all reusable UI behavior:

- theme and font runtime
- ribbon and app-shell resolution
- panes and tabbed containers
- shelves and shelf drag/drop/resize behavior
- view routing and recursive workspace stacks
- module embedding contracts
- command palette and debug overlays
- widget styling and layout contracts
- host-neutral window chrome state and hit-testing

It must not depend on Bevy, eframe, winit, or browser APIs except behind optional
feature gates needed for trait derives such as Bevy `Resource`.

### Host facades and plugins

The unified `mara` crate exposes host-neutral UI APIs and the optional
Mara-owned native window runner. Host plugins translate core state into
external runtimes:

- `bevy_mara`
  - registers Mara state as Bevy resources
  - applies the theme each egui pass
  - installs the Bevy input firewall so UI clicks do not leak into a Bevy scene
  - maps host-neutral window chrome actions onto Bevy/winit native move/resize
  - exposes a Bevy-specific node-view backend

Host facades should not become the source of shelf, pane, theme, or app-shell
rules. If behavior needs to work in multiple hosts, move the contract to
`mara_core`.

## UI composition and hierarchy

The rendered UI is composed in layers. The host owns the frame loop, but the
structure below is Mara-owned:

```text
Host app frame
└─ Host facade/plugin
   ├─ bevy_mara
   │  ├─ Bevy resources + systems
   │  ├─ egui input firewall for Bevy scenes
   │  └─ native window chrome adapter
   └─ mara::window
      └─ Mara-owned native egui/wgpu window

Mara frame inside egui::Context
└─ Theme + global state
   ├─ style::apply_theme
   ├─ AccentColor / GlassOpacity
   └─ layer z-order constants

└─ App shell / top-level chrome
   ├─ permanent main bar
   │  ├─ system controls
   │  ├─ view switcher
   │  └─ app/workspace actions
   ├─ active view ribbons
   ├─ workspace/module ribbons
   └─ host window-chrome regions
      ├─ top-ribbon move region, native hosts only
      └─ corner resize L affordances, native hosts only

└─ Reserved layout regions
   ├─ shelf layout reservation
   │  ├─ left shelf
   │  ├─ right shelf
   │  └─ bottom shelf
   └─ ribbon avoidance / viewport rect

└─ Main workspace viewport
   ├─ ViewRouter
   │  └─ active MaraView
   │     ├─ ViewCtx
   │     └─ view body, for example Bevy viewport / canvas / image
   ├─ WorkspaceStack
   │  ├─ root workspace level
   │  └─ nested module workspace levels
   └─ floating / anchored panes
      └─ Pane
         └─ Container
            ├─ Normal container
            │  └─ Pod / widgets
            └─ Tabbed container
               ├─ tabs
               └─ active tab body

└─ Shelves
   └─ ShelfDef
      └─ ShelfContainer
         └─ typed tabbed container
            └─ module / view / pod content

└─ Embeddable modules and extras
   ├─ MaraModule
   │  ├─ inline body
   │  └─ optional nested WorkspaceCtx
   ├─ mara_canvas
   ├─ mara_image
   ├─ mara_core::extras::graph → mara_graph
   └─ mara_core::extras::code  → mara_code
```

At runtime the usual high-level order is:

```text
apply theme
→ resolve app shell + ribbons
→ compute shelf reservations
→ render active view/workspace body into remaining viewport
→ render shelves and pane/container chrome
→ publish/paint top-level overlays such as resize corners
```

The important hierarchy is not "Bevy demo first". It is:

```text
host facade → mara_core contracts → app shell/ribbons/shelves/views/modules → widgets/extras
```

The demo only wires this hierarchy together; it is not where reusable behavior
belongs.

### App shell and ribbons

The app shell resolves persistent top-level chrome and active workspace/view
ribbons into concrete slot ribbons. It handles:

- permanent main bar rules
- system window-control slots
- view switcher slots
- workspace/view override layers
- ribbon placement, opening, dragging, widths, and avoidance rects

The ribbon renderer publishes native-window drag regions only when the current
host reports that native move/resize is supported.

### Panes and containers

Panes provide anchored floating or reserved UI areas. Containers are the content
blocks inside panes. The important boundary is:

- panes own placement and outer chrome
- containers own tab/title/body structure
- container bodies accept typed `Pod`/module/view surfaces, not arbitrary app
  closures as the default public model

### Shelves

Shelves are persistent docked tabbed-container regions on the left, right, or
bottom edge. There is intentionally no top shelf because top-level chrome belongs
to the app shell/ribbon system.

Shelves reserve viewport space and publish layout so ribbons, panes, and views
can avoid them. Current shelf behavior includes:

- moving whole shelves between allowed edges
- moving containers into existing shelves or new edge shelves
- preview layouts that move ribbons before a drop is committed
- independent remembered sizes for side shelves and bottom shelves
- resize handles on the border between shelf and viewport
- cursor feedback for shelf resizing
- inward-facing reservation ghosts

### Views, modules, and recursive workspaces

Views are routable top-level content surfaces. Modules are embeddable content
surfaces that can live inside containers and can also expose nested workspace
levels.

The key types are:

- `MaraView`
- `ViewRouter`
- `WorkspaceStack`
- `WorkspaceCtx`
- `MaraModule`

Proof modules currently include canvas and image surfaces. The same surface can
be both a View and a Module when that makes sense.

### Extras

`mara_graph` and `mara_code` are standalone crates. `mara_core::extras` provides
Mara-tinted wrappers around them, including fullscreen/embed behavior. Host
facades provide backend-specific texture registration where needed.

## Window chrome

Window chrome is split into a host-neutral core and host adapters.

`mara_core::window_chrome` owns:

- resize-corner hit-testing
- resize-corner visuals
- top-ribbon drag-region publication
- input-claim state so a native move/resize press does not leak into app views
- host capability flags

The host owns:

- whether native move/resize exists
- the actual native API call
- cursor mapping if the platform needs host cursor APIs

For Bevy, `bevy_mara::window_chrome::MaraWindowChromePlugin` maps core hits to
`Window::start_drag_move` and `Window::start_drag_resize`. `MaraWindowChromeSettings`
gates move/resize and now also advertises the optional maximize/close *system
controls* (`system_maximize` / `system_close`) that the permanent top bar injects.

For web, native window chrome capabilities default to disabled. The browser owns
the real window, so Mara does not draw native resize corners or turn the top
ribbon into a native window drag strip.

### Shell bar vs window chrome — two separate concerns

The permanent top bar and the native window frame are deliberately split,
because they have different lifetimes across platforms:

- **The shell bar is UI, lives in `mara_core`, and is enforced by every host
  adapter.** `mara_core::shell::ShellBar` owns the config (`app_menu`, the
  `views` switcher, `active`) and the rendering — `ShellBar::show(ctx, …)`
  draws the bar and returns `ShellEvent`s. There is one implementation; each
  host adapter invokes it once per frame, so the bar is identical and present
  everywhere:
  - **Bevy** — `MaraShellPlugin` (always installed by `MaraPlugin`) stores the
    `ShellBar` as a `Resource`, renders it, handles the window events, and
    forwards the app-level `ShellEvent`s as a `Message`. Covers desktop, web,
    and android Bevy builds.
  - **eframe / `mara::window`** — the native runner renders the bar each frame
    and routes events through `WindowApp::configure_shell` /
    `on_shell_event`, so an app never has to draw or even ask for it.
  - **A host with no adapter** (e.g. raw `eframe::App` on the web) calls
    `ShellBar::show` itself — same core type, no fork.

  The bar is responsive (the slot renderer reflows it per `Breakpoint`) and the
  window-control buttons only appear where a host advertises native-frame
  capabilities. Opt out with `ShellBar { enabled: false, .. }`. Host-owned
  clicks (view switch / menu / shelf toggles) surface as `ShellEvent`;
  `CloseRequested` / `MaximizeToggleRequested` are handled by the host adapter
  (it owns the window). The reference demo dogfoods this on all three of its
  hosts — its view switcher is `demo_shell_views()`, not a hand-rolled bar.

- **Native window chrome is capability-driven, and opt-in per host.** It is
  *not* "desktop" — it is "this host draws its own OS window frame." Adding
  `MaraWindowChromePlugin` means Mara owns the frame: by default it forces
  `Window.decorations = false`, wires `start_drag_move` / `start_drag_resize`,
  and advertises the maximize/close system controls (which is what makes them
  appear in the always-present shell bar). It is all gated by
  `MaraWindowChromeSettings` (`force_decorations_off`, `resize`,
  `move_from_drag_regions`, `system_maximize`, `system_close`).

The adaptivity falls out of the capability model with no per-platform branching
in app code: a desktop build adds `MaraWindowChromePlugin` → borderless frame +
window buttons in the bar; a web or android build simply doesn't add it →
nothing is advertised → the same bar drops the window buttons and reflows. The
bar itself is identical everywhere.

### Floating-chrome layout tracks the live window automatically

Floating ribbons and panes lay out against the post-shelf viewport
("chrome bounds"). The unified renderer derives this **fresh every egui pass**
from the published `ShelfLayout::viewport`, falling back to the live
`ctx.content_rect()` when no shelves are reserved — it never reads back the
value it itself publishes, so the bounds can't freeze at the first frame's
window size (the old "forgot `publish_shelf_layout`" footgun). On Bevy,
`RibbonPlugin` additionally auto-publishes a full-window `ShelfLayout` baseline
each frame (`shelf_layout_published_this_pass` makes this order-independent:
app code that reserves real shelves still wins). Net effect: resize tracking is
correct with zero host wiring, and apps that do reserve shelves keep their
reservation.

## Input handling

There are two related but separate input rules:

1. Normal app UI input should not leak into host scenes.
2. Native window move/resize input should not leak into Mara views/modules.

In Bevy:

- `EguiInputAbsorbPlugin` masks Bevy-side mouse state when the UI owns pointer
  input.
- `MaraWindowChromeInputClaim` tracks native chrome claims so canvas/viewport
  content ignores the press that started move/resize.

In plain egui and web:

- egui already owns normal UI input.
- native chrome capabilities are host opt-in, so browser builds leave them off.

## Theme and styling

Themes live in `mara_core::style` and `mara_core::themes`. Themes own visual
metrics and colors, but not every behavior rule. For example:

- resize-corner size/color comes from theme metrics/accent
- shelf and tab orientation rules are structural behavior, not theme-only
- app-shell permanence rules are API contracts, not theme choices

This keeps themes isolated for visual decisions while preserving stable layout
contracts.

## Verification

Use the repo Makefile:

```sh
make fmt
make check
make test-all
make build TARGET=web
```

Inside nix/direnv, the development shell provides Rust, Bevy runtime libraries,
`trunk`, and the wasm target. If running nix commands without direnv, make sure
`NVIDIA_VERSION` is exported; `.envrc` normally does this automatically.
