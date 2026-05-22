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
`Window::start_drag_move` and `Window::start_drag_resize`.

For web, native window chrome capabilities default to disabled. The browser owns
the real window, so Mara does not draw native resize corners or turn the top
ribbon into a native window drag strip.

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
make build-web
```

Inside nix/direnv, the development shell provides Rust, Bevy runtime libraries,
`trunk`, and the wasm target. If running nix commands without direnv, make sure
`NVIDIA_VERSION` is exported; `.envrc` normally does this automatically.
