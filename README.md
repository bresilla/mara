# Mara

Mara is a reusable egui editor UI kit with a framework-agnostic core and thin
host adapters for Bevy, native egui/eframe, and the web.

The main crates are:

- `mara_core` — panes, ribbons, shelves, widgets, themes, views, modules, and
  host-neutral window chrome
- `mara` — unified UI/window facade
- `bevy_mara` — Bevy plugin integration under `mara/plugin/bevy`
- `mara_graph` — standalone node graph widget
- `mara_code` — standalone code editor widget
- `mara_image` / `mara_canvas` — proof View + Module surfaces

## What it provides

- glass-themed editor UI primitives
- persistent top app shell and ribbon slots
- docked shelves with tabbed containers
- floating/anchored panes
- reusable widgets and pods
- view routing and nested workspace/module support
- graph, code editor, image, and canvas examples
- native borderless window chrome on hosts that support it
- web-safe behavior where browser window chrome is left to the browser

## Repository layout

```text
crates/
  core/                 mara_core
  modules/graph/        mara_graph
  modules/code/         mara_code
  modules/image/        mara_image
  modules/canvas/       mara_canvas
mara/
  plugin/bevy/          bevy_mara
example/                root native/web example
```

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the design boundaries.

## Run

Use the Makefile targets from the repo root:

```sh
make run          # native Mara example
make serve-web    # web demo at http://localhost:8080
```

The Makefile defaults to the root example package:

```sh
make build
make check
make test
```

Use the repo Nix shell when running the gates; the ambient Rust toolchain
may be too old for the current dependency set:

```sh
nix develop --impure -c make check-all
nix develop --impure -c make test-all
```

See [`TOOLCHAIN.md`](TOOLCHAIN.md) for details.

Full workspace gates:

```sh
make fmt
make check-all
make test-all
```

## Web

The web host is built with `trunk`:

```sh
make serve-web
make build-web
```

On web, native resize corners and native top-bar window dragging are disabled
by default because the browser owns the outer window.

## Development notes

- Put reusable behavior in `mara_core`.
- Keep host/plugin crates as adapters.
- Use `make` targets rather than ad-hoc cargo commands when working in this
  repo.
- Do not add Bevy-only logic for behavior that should also work in egui/eframe
  or web.
