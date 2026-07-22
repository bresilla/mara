# ADR 0002 — The extras tier: which module crates get sealed, and how

Status: accepted (2026-07-23) · Relates to: PLAN.md WS6, ADR 0001

## Context

The seal audit (PLAN.md findings B) showed the `mara_*` module crates form a
leakage ladder. `mara_board` proves a module can be **zero-egui**;
`mara_canvas`/`mara_image` joined it (WS5). But five crates cannot be sealed
by renaming: `mara_map`, `mara_3d`, `mara_bevy` crawl to the raw
`egui::Context` (`__internal_egui_ctx`) and build their own `CentralPanel`s;
`mara_graph` and `mara_code` are vendored egui widget crates whose public
APIs are egui-typed through and through.

## Decision (per crate)

| Crate | Tier | Path |
|---|---|---|
| `mara_3d` | **(b) sealed** — FIRST | Render into the `ViewCtx` region (sealed painters + GPU paint callback in the cell rect), stop building its own panel. This is also "GPU leaves can tile" (ADR 0001 Phase 6): a 3D view that renders into its cell can be a split cell. Public API drops `egui_wgpu::RenderState` in favor of the published context state (target format is already published by `view_ctx`) and an opaque `MaraRenderState` where a handle is unavoidable. |
| `mara_bevy` | **(b) sealed** — after 3D | Same shape once 3D proves it: region rendering, sealed input forwarding via `ViewCtx::input()`, `MaraRenderState` constructors. |
| `mara_map` | **(b) sealed** | Pure 2D — no GPU excuse. `MapPalette` + `mvt.rs` geometry move to `vocab` types; panel building moves to `ViewCtx` painters/canvas. Biggest sealed win after 3D. |
| `mara_graph` | **(a) declared unsealed** | Vendored fork with an egui-shaped public trait surface (`NodeViewer` receives `egui::Ui`) and its own secondary Context. Sealing means rewriting the vendored crate — not worth it now. Declared egui-native; never re-exported through sealed paths as if it were sealed; consumers use it knowing it couples them to egui. Revisit if/when the node graph becomes core product. |
| `mara_code` | **(a) declared unsealed** | Same reasoning — vendored egui editor. The sealed embedding (`extras::code` in core) remains the supported path for sealed apps. |

## Consequences

- `MaraRenderState`: opaque newtype in the `mara` host crate wrapping the
  egui-wgpu render state; module constructors take it instead of
  `egui_wgpu::RenderState`. Only host code can mint one.
- Tier (a) crates get a README/Cargo.toml `description` note: "egui-native
  extra — depending on this couples your crate to egui".
- The `make check` module-crate egui ban (board/canvas/image today) grows to
  cover map, three_d, bevy as each lands in tier (b).
- The demo migrates each view to the sealed path as its crate is sealed
  (PLAN.md WS7/WS8 tab-by-tab migration).
