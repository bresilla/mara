# `mara_graph` — third-party acknowledgements

This crate vendors a fork of `egui-snarl` so we can modify the
node-graph implementation in-place without upstream round-trips.
The upstream license files are kept verbatim alongside the
vendored sources.

## Vendored sources (`src/vendored/`)

### `egui-snarl` 0.9.0

- **Path:** `src/vendored/`
- **Upstream:** <https://github.com/zakarumych/egui-snarl>
- **Author:** [@zakarumych](https://github.com/zakarumych)
- **License:** MIT OR Apache-2.0
- **License files (verbatim copies):**
  - [`src/vendored/LICENSE-MIT`](src/vendored/LICENSE-MIT)
  - [`src/vendored/LICENSE-APACHE`](src/vendored/LICENSE-APACHE)
- **Why vendored:** we expect to keep iterating on node visuals,
  pin geometry, header/halo rendering and the sharp-zoom
  pipeline. Vendoring keeps the editing surface in this repo.

## Sharp-zoom pipeline (`src/node_view.rs`)

The "secondary `egui::Context` rendered into a wgpu texture at
zoom-compensated `pixels_per_point`" idea is adapted from
**Blackjack** (<https://github.com/setzer22/blackjack>),
specifically `blackjack_ui/src/render_context.rs`. The wgpu glue
in `node_view.rs` is a reimplementation against the modern
`egui-wgpu` API; no Blackjack source is copied verbatim.
