# Acknowledgements

Parts of this project are derived from third-party open-source
crates, vendored into the relevant `crates/*` sub-tree so we
can modify them in-place without forks / upstream round-trips.
Full copies of each upstream license live alongside the vendored
sources.

Per-crate acknowledgements:

- [`crates/core/ACKNOWLEDGEMENTS.md`](crates/core/ACKNOWLEDGEMENTS.md)
  — bundled fonts (Iosevka) and other assets used by `mara_core`.
- [`crates/modules/graph/ACKNOWLEDGEMENTS.md`](crates/modules/graph/ACKNOWLEDGEMENTS.md)
  — vendored `egui-snarl` used by `mara_graph`.
- [`crates/modules/code/ACKNOWLEDGEMENTS.md`](crates/modules/code/ACKNOWLEDGEMENTS.md)
  — vendored `egui_code_editor` used by `mara_code`.
- [`example/assets/stanford-bunny.obj`](example/assets/stanford-bunny.obj)
  and [`example/assets/stanford-dragon.obj`](example/assets/stanford-dragon.obj)
  — Stanford Bunny and Dragon OBJ meshes used only by the root demo's OBJ loader.

---

## `crates/modules/graph/src/vendored/`

Derived from **egui-snarl** v0.9.0 — a node-graph widget for
`egui`.

- Upstream: <https://github.com/zakarumych/egui-snarl>
- Author: [@zakarumych](https://github.com/zakarumych)
- License: MIT OR Apache-2.0
- License files (verbatim copies):
  - [`crates/modules/graph/src/vendored/LICENSE-MIT`](crates/modules/graph/src/vendored/LICENSE-MIT)
  - [`crates/modules/graph/src/vendored/LICENSE-APACHE`](crates/modules/graph/src/vendored/LICENSE-APACHE)

---

## `crates/modules/code/src/vendored/`

Derived from **egui_code_editor** v0.2.21 — a syntax-highlighting
multi-line text editor for `egui`.

- Upstream: <https://github.com/p4ymak/egui_code_editor>
- Author: Roman Chumak
  ([@p4ymak](https://github.com/p4ymak))
- License: MIT
- License file (verbatim copy):
  - [`crates/modules/code/src/vendored/LICENSE`](crates/modules/code/src/vendored/LICENSE)

---

## `crates/core/src/fonts/`

The nine Iosevka weights bundled with `mara_core` are licensed
under the **SIL Open Font License 1.1**.

- Upstream: <https://github.com/be5invis/Iosevka>
- License: SIL Open Font License 1.1
- Loaded by: `style.rs::install_fonts` via `include_bytes!`.

---

## `example/assets/stanford-bunny.obj` and `example/assets/stanford-dragon.obj`

The root example includes the Stanford Bunny and Stanford Dragon
meshes as demo-only imported OBJ assets.

- Upstream OBJ mirrors:
  - <https://github.com/alecjacobson/common-3d-test-models/blob/master/data/stanford-bunny.obj>
  - <https://github.com/alecjacobson/common-3d-test-models/blob/master/data/xyzrgb_dragon.obj>
- Original source:
  <https://graphics.stanford.edu/data/3Dscanrep/>
- Loaded by: `example/src/app.rs::add_demo_obj_model`.

---

## Why vendored instead of depending directly

Both `egui-snarl` and `egui_code_editor` are excellent upstream,
but we expect to modify them for project-specific needs (per-node
colour, custom syntax rules, editor behaviour changes, layout
primitives that plug directly into the Mara theme tokens, …) and
have no intention of upstreaming every change. Vendoring lets us
iterate without forks / PR roundtrips and keeps every dependency
visible in this repo's source tree.

If you contribute a change to the vendored code that could
benefit upstream too, please send it to the original repo first
— see the links above.
