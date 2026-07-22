# ADR 0001: Scope of the backend seam

- **Status**: Proposed (2026-07-20)
- **Deciders**: repo maintainer
- **Related**: `PLAN.md` (True Backend Independence — Second Pass),
  `docs/history/PLAN-first-pass.md`, ARCHITECTURE.md §3/§12

## Context

Mara's architecture promises a backend-swappable core: app code sees only the
sealed `MaraUi`/vocab surface, widgets express painting as the `PaintCmd` IR,
and an abstract `UiBackend` trait fronts egui. The first-pass plan delivered
the consumer seal and the IR. The implementation seam, however, is partial
(measured 2026-07-20, tracked by the coupling ratchet):

- `egui::` is referenced in **57** files under `crates/core/src`.
- All **10** `widget/` files keep `&mut egui::Ui`-typed entry points that
  construct `EguiUiBackend` locally; 7 widget families are fully
  backend-routed, 2 (`tree`, `context_menu`) have no backend twin, 2
  (`color` picker, `foldable` body) are partial.
- `MaraUi` stores the **concrete** `EguiUiBackend` and escapes through
  `ui_mut()` at **24** sites.
- Per-id state and animation hit egui's data store directly at **~139**
  sites across chrome (pane/shelf/container/ribbon/pod/palette/enforce),
  bypassing `MaraMemory`.
- `UiBackend` has exactly one production implementation (egui); the only
  other impls are 19 duplicated test-local `RecordingBackend`s.

The cost of the seam today is wrapper indirection with no second backend;
the risk of the *status quo* is that the swappability claim silently rots
and every egui major upgrade touches 57 files.

## Decision

Keep the seam, scoped honestly, and close it per `PLAN.md` Phases 0–3:

1. **The portable contract is the IR layer**: `PaintCmd`/`PaintList`,
   `vocab` types, the `UiBackend` trait, and the memory/animation contracts
   (`MaraMemory`, `MaraAnim`, object-safe `MemoryStore`).
2. **New code must target the contracts.** No new `egui::Ui`-typed
   public/`pub(crate)` entry points; no new direct `ctx.data*`/`animate_*`
   access outside `backend/` and `memory.rs`. Enforced by the coupling
   ratchet in `make check` (fail on any count increase).
3. **Existing egui-typed signatures migrate per PLAN.md's phased schedule,
   and each migrated surface deletes its old egui-typed entry points in the
   same change** — greenfield rule, no back-compat, no grandfathered
   permanent residents (see PLAN.md "Ground rule"). Persisted state may
   reset when key derivations change; a changelog note suffices.
4. **`MaraMemory` is authoritative for widget state**; chrome-level state
   may use the egui data store only until its Phase 2 migration lands, at
   which point the direct-access sites are deleted.
5. **The proof obligation is a second implementation.** The recording
   backend (`backend/record.rs`, PLAN.md Phase 1) must be able to run the
   widget set headlessly; golden paint tests freeze behavior. If the Phase 1
   probes reveal the trait cannot carry the widget set without unreasonable
   distortion, this ADR must be revisited with "drop the trait and own the
   egui coupling" explicitly on the table.
6. Egui remains the reference backend indefinitely. A second *production*
   backend is a Phase 6 go/no-go decision made only after Phases 1–5
   evidence exists (Option A: epaint-direct; Option B: fully independent —
   see PLAN.md §Phase 6).

## Consequences

- Contributors: reach for `UiBackend`/`MaraUi`/`MaraMemory` in all new
  internal code; `backend/egui.rs` is the only place allowed to name
  `egui::Ui`. The ratchet makes violations a CI failure, not a review nit.
- Reviewers: a PR that lowers a ratchet baseline must contain the deletion
  of the code it counts; a PR that raises one is rejected.
- The `raw-egui` feature + `__internal_*` hatch (sealed-API escape for
  consumers) is unaffected — this ADR governs internals, not the consumer
  hatch.
- ARCHITECTURE.md §3 documents the honest current state and links here;
  both must be updated when Phase 3 lands (the "not yet real" paragraph
  shrinks as counts drop).
