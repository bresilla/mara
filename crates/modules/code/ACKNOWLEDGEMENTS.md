# `mara_code` — third-party acknowledgements

This crate vendors a fork of `egui_code_editor` so we can modify
the editor in-place without upstream round-trips. The upstream
license file is kept verbatim alongside the vendored sources.

## Vendored sources (`src/vendored/`)

### `egui_code_editor` 0.2.21

- **Path:** `src/vendored/`
- **Upstream:** <https://github.com/p4ymak/egui_code_editor>
- **Author:** Roman Chumak
  ([@p4ymak](https://github.com/p4ymak))
- **License:** MIT
- **License file (verbatim copy):**
  - [`src/vendored/LICENSE`](src/vendored/LICENSE)
- **Why vendored:** we expect to keep iterating on syntax rules,
  highlight palettes, the auto-completer, and the editor's
  rendering layout. Vendoring keeps the editing surface in this
  repo without forks / PRs.
