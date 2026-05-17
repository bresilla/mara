# egui_mara_web

The mara UI kit compiled to WebAssembly and run in a browser via
[`eframe`]'s `WebRunner`.

`mara_core` (the UI core — panes, ribbons, containers, widgets,
theme) is host-agnostic egui. `bevy_mara` renders it inside a Bevy
app; `egui_mara`'s native examples render it in an `eframe` window.
This crate is just the third host: **eframe on `wasm32`**. The UI
itself is unchanged — see `src/demo.rs`, a port of
`bevy_mara --example demo`: the full ribbon / pane / widget-gallery
/ theme / canvas-whiteboard / node-graph / code-editor demo, with the
Bevy 3D scene dropped and the Bevy `Resource`s / systems replaced by a
single `DemoApp` struct driven from `eframe::App::update`.

## Prerequisites

Inside the repo's nix devshell (`nix develop`, or direnv) the
`wasm32-unknown-unknown` target and `trunk` are already provided by
`flake.nix` — nothing to install.

Outside nix, set them up once:

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
```

## Run

From the repo root:

```sh
make serve-web      # trunk serve --open  → http://localhost:8080
make build-web      # trunk build --release → api_crates/web/dist/
```

Or directly:

```sh
cd api_crates/web && trunk serve
```

## How it differs from the native examples

* **Entry point** — `eframe::WebRunner` mounting onto
  `<canvas id="mara_canvas">` in `index.html`, instead of
  `eframe::run_native`.
* **Renderer** — eframe's wgpu backend pinned to WebGL2. `wgpu` is
  built with only the `webgl` feature, and `src/main.rs` forces
  `Backends::GL` so eframe's WebGPU auto-detection can't select a
  backend that isn't compiled in (the cause of a black canvas). The
  wgpu backend — rather than `glow` — is required because the node
  graph's `EframeNodeViewBackend` needs eframe's wgpu render state.
  No native windowing (`x11` / `wayland`).
* **`getrandom`** — pulled transitively via `egui → ahash`, it uses
  its `wasm_js` backend on the web. That backend is selected by the
  `getrandom_backend="wasm_js"` rustflag in the repo-root
  `.cargo/config.toml` (scoped to `wasm32-unknown-unknown`).

On non-wasm targets this crate is an inert stub, so
`cargo check --workspace` stays fast and never builds eframe / wgpu /
winit for a native target.

[`eframe`]: https://docs.rs/eframe
