# `mara_core` — third-party acknowledgements

The UI core crate ships bundled font assets in addition to its
own code; this file lists what's embedded and where it came from.
The standalone host widgets the kit ships with — `mara_graph`
(node graph) and `mara_code` (code editor) — live in their own
crates with their own acknowledgements; see
[`../modules/graph/ACKNOWLEDGEMENTS.md`](../modules/graph/ACKNOWLEDGEMENTS.md) and
[`../modules/code/ACKNOWLEDGEMENTS.md`](../modules/code/ACKNOWLEDGEMENTS.md).

## Bundled font assets (`src/fonts/`)

### Iosevka (9 weights)

- **Files:** `iosevka-thin.ttf`, `iosevka-extralight.ttf`,
  `iosevka-light.ttf`, `iosevka-regular.ttf`, `iosevka-medium.ttf`,
  `iosevka-semibold.ttf`, `iosevka-bold.ttf`, `iosevka-extrabold.ttf`,
  `iosevka-heavy.ttf`.
- **Upstream:** <https://github.com/be5invis/Iosevka>
- **Author:** Belleve Invis
  ([@be5invis](https://github.com/be5invis))
- **License:** SIL Open Font License 1.1.
- **Loaded by:** `style.rs::install_fonts` via `include_bytes!`.
