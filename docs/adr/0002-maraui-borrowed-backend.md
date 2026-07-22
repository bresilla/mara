# ADR 0002: `MaraUi` borrows its backend (layout-engine prerequisite)

- **Status**: Implemented (2026-07-21) — `MaraUi<'a>` holds
  `&'a mut dyn UiBackend`. A blanket `impl<T: UiBackend + ?Sized>
  UiBackend for &mut T` lets every widget `*_backend(&mut …)` call
  compile unchanged; the egui escape is the object-safe trait methods
  `egui_ui_mut`/`egui_ui_ref` (single lifetime, no invariance wall —
  cleaner than the two-lifetime enum this ADR originally proposed).
  `__internal_from_raw` split into `__internal_backend_from_raw` +
  `__internal_over`. Commit `61d3b63`.
- **Status (original)**: Proposed (2026-07-21)
- **Deciders**: repo maintainer
- **Related**: `PLAN.md` Phase 4, ADR 0001, `docs/history/PLAN-first-pass.md`

## Context

Phase 4 (chrome on the contracts) is gated on a single architectural
decision uncovered while migrating the foldable `section` to render
headlessly.

Today `MaraUi<'a>` **owns** its backend by value:

```rust
pub struct MaraUi<'a> {
    pub(crate) backend: MaraBackend<'a>, // MaraBackend { Egui, Recording }
    accent: vocab::Color32,
}
```

Every nesting chrome surface (section, pane, shelf, container) runs its
body against a *freshly constructed* child `MaraUi`. For the egui
backend that works, because egui hands out a fresh child `egui::Ui`:

```rust
// mui.section, today:
section(self.egui_ui(), …, |child_ui| {
    body(&mut MaraUi::new(child_ui, accent)); // child OWNS a new MaraBackend::Egui
});
```

For any **headless / non-egui** backend this is impossible: there is no
"child `egui::Ui`" to wrap, and the recording backend *is* the parent —
the child body must render against the **same** backend, scoped to an
indented sub-region. An owned-by-value `MaraUi` cannot express that: you
cannot own a second `MaraBackend` that shares the parent's live cursor
and paint stream.

This is why no chrome surface can move off egui yet, and why the layer
contract + flow cursor (already landed) are as far as Phase 4 goes
without resolving ownership.

## Decision (proposed)

**`MaraUi` borrows its backend instead of owning it:**

```rust
pub struct MaraUi<'a> {
    backend: &'a mut MaraBackend<'a>,
    accent: vocab::Color32,
}
```

Consequences of the model:

1. A **nested/child region** re-scopes the *same* `MaraBackend` (indent
   the flow cursor / push a child `egui::Ui`) and lends it to a child
   `MaraUi` that borrows it — no second owner, no shared-state problem.
   The child-region primitive lives on the concrete `MaraBackend` enum
   (Sized → can take closures), not the `UiBackend` trait (must stay
   object-safe for `dyn` in `TreeBody`).
2. **`MaraUi::new(ui: &mut egui::Ui, …)` goes away.** Callers that own an
   `egui::Ui` first construct a `MaraBackend::Egui(EguiUiBackend::new(ui))`
   as a local, then `MaraUi::over(&mut backend, accent)`. Blast radius:
   the ~16 construction sites — `view/context.rs`, `pod/mod.rs`,
   `mui` stack/section/context-menu closures — plus the sealed
   `__internal_from_raw` hosts (`mara/src/host.rs`, `crates/modules/canvas`,
   and ~10 `example/src/app.rs` sites). `__internal_from_raw` changes
   shape (it can no longer *return* an owning `MaraUi` from a bare
   `&mut egui::Ui`); per the greenfield ground rule its old form is
   deleted, not kept.
3. The `raw-egui` consumer hatch and the sealed-API `make check` gates
   are preserved — this is an internal ownership change, and app code
   still only ever sees `&mut MaraUi`.

## Alternatives considered

- **Owned backend + `Box<dyn>` child**: still can't share the recording
  backend's live state between parent and child owners. Rejected.
- **Child-region as a `UiBackend` trait method taking a closure**: breaks
  object-safety (`dyn UiBackend` in `TreeBody`). The primitive belongs on
  the Sized `MaraBackend` enum instead. Rejected as a trait method.
- **Keep owned `MaraUi`, special-case egui**: leaves every chrome surface
  permanently egui-only — defeats Phase 4. Rejected.

## Consequences

- This is a prerequisite: **land the borrowed-backend rework before any
  chrome surface migrates.** It touches the sealed `__internal_from_raw`
  API and ~16 sites, so it is a focused change with its own PR and full
  gate run (including the sealed-API greps), not folded into a surface
  migration.
- Once it lands, the sequence is: child-region + frame primitives on
  `MaraBackend` → migrate `section` (first proof, headless golden) →
  shelf → pane → container → ribbon → palette.
- Reviewers: scrutinise that every `MaraUi` construction site owns its
  `MaraBackend` for at least as long as the `MaraUi` borrow, and that no
  child `MaraUi` outlives its parent's scoped region.
