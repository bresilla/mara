# Plan 007: Unify the widget models — immediate `MaraUi` content inside pods

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat bcf6600..HEAD -- crates/core/src/pod/mod.rs crates/core/src/mui/mod.rs example/src/app.rs`
> Plans 004/005 legitimately changed `pod/mod.rs` and `app.rs`; re-read the
> live builder shapes before starting. On unexplained mismatch, STOP.

## Status

- **Priority**: P2
- **Effort**: L
- **Risk**: MED-HIGH (new public API on the hottest surface; borrow/lifetime design)
- **Depends on**: plans/004-ambient-accent-pods.md, plans/005-typed-pod-responses.md
- **Category**: tech-debt (API ergonomics — the core "two models" fix)
- **Planned at**: commit `bcf6600`, 2026-07-20

## Why this matters

Mara has two widget-authoring models. The **immediate** one (`MaraUi`) is
pleasant: `mui.toggle("dark", &mut on)` binds by `&mut` and returns a
response inline. The **deferred** one (`Pod` builders) is what containers
mandate ("A container accepts only Pods" — ARCHITECTURE.md §5): values pass
by value, results come back in a response bag after `render()`. Every
non-trivial app therefore lives in the awkward model, and the pleasant one is
reachable only inside view/module bodies. This plan bridges them **without
abandoning the Pod invariant**: a pod gains `with_ui(closure)`, hosting
immediate `MaraUi` content as one composable unit. Response collection and
layout bookkeeping stay uniform (the container still only sees Pods), while
consumers get `&mut`-binding widgets everywhere.

## Current state

- The deferred model — `crates/core/src/pod/mod.rs` (post-004/005 shapes;
  re-read live): `with_button(label)`, `with_slider(label, value, range,
  decimals, suffix)` pushing `WidgetSpec` variants; `body.add_normal(id,
  title, icon, pods)` then `body.render()` → `HashMap<Id, Vec<PodResponse>>`.
- The immediate model — `crates/core/src/mui/mod.rs:696-760`:

```rust
pub fn button(&mut self, label: &str) -> MaraResponse {
    button(self.backend.ui_mut(), label, self.accent)
}
pub fn toggle(&mut self, label: &str, on: &mut bool) -> MaraResponse { ... }
pub fn slider(&mut self, label: &str, value: &mut f64, range: RangeInclusive<f64>,
    decimals: usize, suffix: &str) -> MaraResponse { ... }
```

- `MaraUi` construction: it wraps an `EguiUiBackend` (see `mui/mod.rs` header
  and `widget/button.rs:185-190`, where `ButtonSpec::show(self, ui: &mut
  MaraUi)` reads `ui.accent()` then calls `ui.backend.ui_mut()`), so anywhere
  the pod render path has an `&mut egui::Ui` it can construct a `MaraUi`.
  Find the existing constructor: `grep -n 'fn.*-> MaraUi\|pub(crate) fn new'
  crates/core/src/mui/mod.rs`.
- Precedent for closure-carrying specs: `PaneBody<'_, 'spec>` already
  threads a `'spec` lifetime for deferred bodies (`pane/mod.rs:579-583`), and
  `WidgetSpec::Module`/`ModulePodResponse` already host non-trivial embedded
  content in pods.
- ARCHITECTURE.md §5 justification for the Pod invariant: "this keeps
  response collection and layout bookkeeping uniform." The design below
  honors it — the closure is *inside* a Pod.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Core only | `nix develop --impure -c cargo check -p mara_core` | exit 0 |
| Check + sealed gates | `nix develop --impure -c make check` | exit 0 |
| Full test suite | `nix develop --impure -c make test-all` | exit 0 |
| Full gate | `nix develop --impure -c make harden` | exit 0 |

## Scope

**In scope**:
- `crates/core/src/pod/mod.rs` — new `WidgetSpec::Ui` variant + `with_ui`.
- `crates/core/src/mui/mod.rs` — only if a `pub(crate)` constructor from an
  `&mut egui::Ui` context is missing.
- `example/src/app.rs` — migrate 2–3 representative panes as proof (not a
  full sweep).
- ARCHITECTURE.md §5 — one paragraph documenting the third pod content kind.

**Out of scope**:
- Removing or deprecating the existing `with_*` builders — the declarative
  path remains fully supported (it is the right tool for uniform lists).
- Containers accepting raw closures without a Pod — explicitly rejected to
  preserve the invariant.
- `PaneBody`/container internals beyond what executing the closure requires.

## Git workflow

- Branch from `develop`: `feature/007-immediate-pod-content`
- Conventional commits, title only, ≤50 chars, no signature. Suggested:
  `feat(core): immediate MaraUi content in pods`
- Do NOT push or open a PR unless instructed.

## Steps

### Step 1: Design the closure storage (spike, throwaway allowed)

The hard part is lifetimes: pods are built in app code borrowing app state
(`&mut` captures), stored in a `Vec<Pod>`, and executed later inside
`body.render()`. Establish the pattern from the existing machinery:

1. Read how `body.add_normal(...)`'s pods flow into rendering — find the
   struct field that stores pods and the render call chain
   (`grep -n 'add_normal\|fn render' crates/core/src/pane/body.rs
   crates/core/src/pod/mod.rs crates/core/src/container/normal/mod.rs`).
2. Determine the lifetime available for closures. Target signature:

```rust
pub fn with_ui(mut self, build: impl FnOnce(&mut MaraUi<'_>) + 'spec) -> Pod<'spec>
```

   which likely requires `Pod` to grow a `'spec` parameter, defaulted so
   existing `Pod` usage (`Pod::new(id)`) still compiles as `Pod<'static>`.
   `PaneBody<'_, 'spec>` proves the codebase already threads exactly this
   lifetime — mirror it.
3. If `Pod<'spec>` infects public signatures beyond `PaneBody`'s existing
   `'spec` (e.g. `HashMap<Id, Vec<PodResponse>>` is unaffected, but
   `add_normal`'s pod parameter type changes), enumerate the fallout. If it
   reaches **shelf/container public APIs that don't already carry `'spec`**,
   STOP and report the design (an alternative — `Box<dyn FnOnce + 'spec>`
   inside an existing lifetime — may fit better; the reporter decides).

**Verify**: a 30-line design note appended to this plan file's Maintenance
section (or reported back), naming the chosen storage type and the affected
signatures. `cargo check -p mara_core` passes with the skeleton compiled.

### Step 2: Implement `WidgetSpec::Ui` end-to-end

- Add the variant storing the closure and an optional fixed height hint
  (`with_ui_h(height, build)` twin, matching how other pod widgets size —
  check how `WidgetSpec::Module` sizes itself and copy that mechanism).
- At the pod render site: allocate the widget's rect like other specs,
  construct a `MaraUi` over the current backend/ui scope (reuse the
  `pub(crate)` constructor found in Current state; set its accent from the
  pod's resolved accent per plan 004), run the closure.
- Response: closures communicate through their `&mut` captures, so
  `PodResponse` needs no new family. Add nothing to `PodResponse`.

**Verify**: `nix develop --impure -c cargo check -p mara_core` → exit 0.

### Step 3: Prove it in the example

Migrate 2–3 pods in `example/src/app.rs` where the deferred model is at its
worst (candidates: a toggle+slider settings pod; any pod whose response
handling spans >8 lines — the map-objects color pod at `app.rs:2760-2814` is
a documented pain site). Before/after must show: the `&mut` state binding
replaces response-bag digging.

**Verify**: `nix develop --impure -c make check` → exit 0.
**Verify** (ergonomics evidence, goes in the PR/report): line counts of the
migrated functions before vs after — expect a meaningful reduction.

### Step 4: Document the third content kind

ARCHITECTURE.md §5: add `with_ui` to the Pod description — "a Pod is built
fluently from declarative `with_*` widgets, an embedded module, **or an
immediate `MaraUi` closure** — containers still only accept Pods."

**Verify**: `nix develop --impure -c make test-all` → all pass.
**Verify**: `nix develop --impure -c make harden` → exit 0.

## Test plan

In the pod test module (same location as plan 005's tests):

- `pod_ui_closure_runs_once_per_render`: counter captured by the closure
  increments exactly once per render pass.
- `pod_ui_closure_mut_binding`: closure flips a captured `&mut bool` via
  `mui.toggle(...)`'s simulated interaction if the harness supports
  interaction; otherwise assert direct mutation from within the closure is
  visible after `render()` (the binding semantics, not the widget).
- `pod_ui_sizes_like_other_widgets`: pod with `with_ui_h(64.0, ...)` reserves
  64px — assert via whatever geometry the harness exposes (match how
  existing pod sizing tests assert; if none exist, assert on the spec's
  stored height).

## Done criteria

- [ ] `Pod::with_ui` + `with_ui_h` exist, doc-commented with an example
- [ ] 2–3 example pods migrated; response-bag digging removed there
- [ ] ARCHITECTURE.md §5 updated
- [ ] `nix develop --impure -c make check` exits 0
- [ ] `nix develop --impure -c make test-all` exits 0 with ≥3 new tests
- [ ] `nix develop --impure -c make harden` exits 0
- [ ] `plans/README.md` status row updated

## STOP conditions

- Step 1's lifetime fallout reaches shelf/container public APIs that don't
  already carry `'spec` — report the design note instead of forcing it.
- The pod render site turns out not to have an `&mut egui::Ui`/backend in
  scope where widgets are painted (i.e. it renders purely from `PaintCmd`
  without a live Ui) — the closure cannot host interactive widgets there;
  report (this would be surprising: `container/normal/mod.rs:779 show_raw(self,
  ui: &mut Ui, body: impl FnOnce(&mut Ui))` suggests a live Ui exists).
- The `make check` sealed-API greps flag the new code (e.g. a
  `pub fn`-taking-egui pattern) — adjust the code shape, never the gate.

## Maintenance notes

- This is the strategic bet of the ergonomics track: if `with_ui` proves out,
  a future major version can shrink the declarative `with_*` families to the
  genuinely-list-shaped ones and let everything else be immediate. Revisit
  after the example migration has lived for a while.
- Reviewer: scrutinize closure execution order relative to other widgets in
  the pod (must be declaration order) and double-render hazards (closure must
  run exactly once per pass — watch for layout measure+paint double calls in
  the pod render path; if the path measures by running specs twice, the
  closure needs a measure-guard).
- Interaction with plan 008 (auto-ids): `with_ui` content salts its widget
  ids from the backend scope — confirm no collisions when two `with_ui` pods
  share a container (the pod's own id must salt the scope; note this in the
  design step).
