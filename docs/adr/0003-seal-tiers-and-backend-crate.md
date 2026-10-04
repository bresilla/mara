# ADR 0003 — Seal tiers are directories, and exactly one crate names egui

Status: accepted (2026-07-28) · Supersedes ADR 0002 (extras tier) ·
Relates to: ADR 0001 (backend seam scope), PLAN.md WS-C / WS-G

## Context

ADR 0002 proposed a per-crate "tier" allowlist: `mara_3d`, `mara_bevy` and
`mara_map` would be driven to **sealed**, while `mara_graph` and `mara_code`
were **declared unsealed** and carried as permanent exceptions.

Two measurements overturned half of that.

**Renderer-owning crates cannot be sealed, and pretending otherwise costs
more than it buys.** `mara_bevy` owns the Bevy render target, so Bevy hands
it back a `wgpu::TextureView` it must name to use. Wrapping the type in an
opaque handle does not help: whoever *mints* the handle names wgpu, and that
is the crate in question. The same holds for `mara_3d` and `three-d`.

**A per-crate allowlist fails open.** A new module crate is unsealed until
someone remembers to add it to the list, so the default is wrong and a
regression is a review miss rather than a build failure.

## Decision

**A crate's directory is its tier.** No allowlist, no per-crate exceptions
except one that is named and dated.

| Tier | Directory | Rule |
|---|---|---|
| Backend | `crates/backend-egui` | The **only** crate that names egui. Implements `mara_core`'s seams. |
| Core | `crates/core`, `crates/gpu` | The seams themselves. No egui edge with `--no-default-features`. |
| Sealed | `crates/modules/*` | Names no backend type; depends on no backend crate. |
| Host | `hosts/*`, `mara/`, `mara/plugin/*`, `example/src/host/` | Owns a renderer or an `eframe::App`. May name egui and wgpu. |

Host tier is a **declaration, not a loophole**: it exists because
implementing `eframe::App` means naming `egui::Ui`, and driving Bevy means
naming `wgpu::TextureView`. Those are facts about the foreign API, not
laziness. What the tier buys is that the boundary is greppable.

`mara_core`'s egui dependency is **optional**, enabled only by the default-on
`backend-egui-conv` feature, which gates `From`/`Into` impls between vocab
types and egui's. Those impls cannot live in the backend crate: both types
are foreign there, so the impl is E0117.

## Enforcement

Four mechanisms, each verified by planting a violation and watching it fail:

1. **`cargo tree -p mara_core --no-default-features | grep -q egui`** must
   find nothing. This is the headline claim, asserted rather than greped.
2. **`scripts/sealed_deps.sh`** reads *resolved package names* of *direct*
   dependencies from `cargo metadata`, for `crates/modules/*` and
   `example/sealed`. Structural, so a renamed dep
   (`ui_kit = { package = "egui" }`) is caught where a manifest grep is
   blind. `mara_backend_egui` is in the ban list too: depending on it
   reaches egui without writing the token.
3. **Source-token greps** over sealed-tier `src/`, which catch reaching a
   backend type through a re-export — something no dependency edge shows.
4. **`scripts/ratchet.sh`**, five counts that may only decrease. Retained
   until WS-D closes; see below.

## Consequences

- **ADR 0002's per-crate table is void.** `mara_3d` and `mara_bevy` moved to
  `hosts/` rather than being sealed. `mara_map` was sealed (WS-B) and stays
  sealed by directory, not by name.
- **`mara_code` graduated.** It was "declared unsealed" in ADR 0002; its only
  dependency today is `serde`.
- **`mara_graph` is the one remaining exception**, named explicitly in
  `make check` rather than silently skipped. Its model half is already
  backend-free; its renderer (`vendored/ui.rs`, ~2 900 lines) is not.
  Characterisation harnesses now exist for both its state machine and its
  rendering, so the port is verifiable — that was the blocker.
- **The ratchet stays until `mara_graph` closes.** Four of its five counts
  are already zero (`state_bypass`, `egui_ui_fns`, `ui_escapes`,
  `demo_egui`); the fifth counts core's gated conversion files. Retiring it
  early would remove the only thing holding the line on the crate that is
  still open.

## Notes

The ADR directory has duplicate numbers from parallel work (two `0001-`, two
`0002-`). This is `0003` on the seal thread specifically; ADR 0001
(backend-seam-scope) remains in force and is unaffected except that
"`backend/egui.rs` is the only place allowed to name `egui::Ui`" now reads
"the `mara_backend_egui` crate".
