# Plan 010: Promote the recording backend — headless `UiBackend` + golden paint tests

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. This is a **spike with a defined deliverable**:
> a partial result plus an honest report is success; forcing completeness is
> failure. When done, update the status row in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat bcf6600..HEAD -- crates/core/src/layout.rs crates/core/src/backend/ crates/core/src/paint.rs`
> On unexplained mismatch with the excerpts, STOP.

## Status

- **Priority**: P2
- **Effort**: L
- **Risk**: MED (exploratory; may surface IR gaps — that is a finding, not a failure)
- **Depends on**: plans/009-seam-truth-adr.md (the ADR frames what this spike proves)
- **Category**: direction / tests
- **Planned at**: commit `bcf6600`, 2026-07-20

## Why this matters

The entire Mara architecture — `PaintCmd` IR, `vocab` types, the `UiBackend`
trait — exists to make the rendering backend swappable, and the project's own
PROGRESS.md names "generalising `MaraUi` over the backend type" as the
remaining major work. But no second backend exists, so the abstraction is
unproven and the module crates ship with **zero tests** (graph/code/three_d/
board). One artifact fixes both: a headless **recording backend** that
implements `UiBackend` and captures the `PaintCmd` stream. It proves (or
honestly disproves) the seam, and it gives every widget and module a golden
test: "this widget, at this size, in this theme, emits exactly these paint
commands."

## Current state

- The trait — `crates/core/src/layout.rs:641-697`:

```rust
pub trait UiBackend {
    fn begin_area(&mut self, host: AreaHost, rect: Rect);
    fn allocate(&mut self, size: Vec2, sense: Sense) -> MaraResponse;
    fn reserve_space(&mut self, size: Vec2) -> Rect { ... }        // default
    fn reserve_rect(&mut self, rect: Rect, sense: Sense) -> MaraResponse { ... } // default
    fn interact(&mut self, rect: Rect, id: Id, sense: Sense) -> MaraResponse;
    fn available_rect(&self) -> Rect;
    fn id(&self) -> Id { ... }                                     // default, "adequate for stateless recording/test backends"
    fn available_width(&self) -> f32 { ... }
    fn available_height(&self) -> f32 { ... }
    fn input(&self) -> MaraInput { MaraInput::default() }          // default
    fn add_space(&mut self, _spec: SpaceSpec) {}
    fn push_clip(&mut self, rect: Rect);
    fn pop_clip(&mut self);
    fn measure_text(&self, text: &str, size: f32, mono: bool) -> Vec2;
    fn paint(&mut self, cmd: PaintCmd);
}
```

  Note the trait's own doc on `id()` anticipates "stateless recording/test
  backends" — the design intends this backend to exist.
- A `RecordingBackend` already exists **in `layout.rs`'s test module** (the
  architecture audit located it as the trait's only non-egui impl; find it:
  `grep -n 'RecordingBackend' crates/core/src/layout.rs`). It is
  test-private today.
- Widget internals reachable without `egui::Ui`: per-widget `*_backend`
  functions (e.g. `toggle_backend`, `label_backend` called from
  `mui/mod.rs:690-740`; `action_button_backend` from `widget/button.rs:199+`).
  These take `&mut impl UiBackend`-ish parameters — confirm each signature
  (`grep -rn 'fn.*_backend' crates/core/src/widget/ crates/core/src/mui/`).
- `PaintCmd` (`crates/core/src/paint.rs`) derives — check which
  (`grep -n 'derive' crates/core/src/paint.rs | head`); golden serialization
  needs `Debug` (already used by the IR per ARCHITECTURE §3).
- Golden-file convention: none exists in-repo. Use plain committed `.txt`
  snapshots + `assert_eq!` against `format!("{:#?}", cmds)` — **no new
  dependencies** (no `insta`), keeping the sealed/no-default-features builds
  clean.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Core only | `nix develop --impure -c cargo check -p mara_core` | exit 0 |
| Core tests | `nix develop --impure -c cargo test -p mara_core` | all pass |
| Check + sealed gates | `nix develop --impure -c make check` | exit 0 |
| Full gate | `nix develop --impure -c make harden` | exit 0 |

## Scope

**In scope**:
- `crates/core/src/backend/record.rs` (new) + `backend/mod.rs` registration.
- `crates/core/tests/golden_paint.rs` (new) + `crates/core/tests/golden/`
  snapshot directory.
- Minimal visibility adjustments (`pub(crate)` → `pub` or test re-export) for
  the `*_backend` widget functions the golden tests call — **only** through a
  `#[doc(hidden)] pub mod __test_surface` or existing internal paths; do not
  add egui-typed public API.

**Out of scope**:
- Migrating widget entry signatures off `egui::Ui` (ADR 0001 scopes that).
- A visual/pixel backend, wgpu, or anything that renders.
- Module-crate golden tests (plan 012 consumes this infra there).

## Git workflow

- Branch from `develop`: `feature/010-recording-backend`
- Conventional commits, title only, ≤50 chars, no signature. Suggested:
  `feat(core): recording backend + golden paint tests`
- Do NOT push or open a PR unless instructed.

## Steps

### Step 1: Promote `RecordingBackend` to a real module

Move/reimplement the test-private `RecordingBackend` from `layout.rs` into
`crates/core/src/backend/record.rs`:

- Fields: recorded `Vec<PaintCmd>`, a configurable `available: Rect`, a
  cursor for `allocate` (stack children top-down like the egui backend's
  vertical flow — read `EguiUiBackend::allocate` in `backend/egui.rs` and
  mirror its cursor semantics), clip stack, and a deterministic
  `measure_text` (e.g. `Vec2::new(0.6 * size * text.chars().count() as f32, size * 1.25)`
  — document that the constants are arbitrary-but-frozen; goldens depend on
  them).
- `interact`/`allocate` return inert `MaraResponse`s (no hover/click);
  construct them the way the existing test backend does.
- Public, `#[doc(hidden)]` if the sealed gates require (check `make check`
  gate list — a new `pub` item in `backend/` must not trip the
  `pub use egui`-shaped greps; it won't, since it exposes no egui types —
  the whole point).
- Keep the old test-module alias in `layout.rs` delegating to the new type
  so existing layout tests stay green.

**Verify**: `nix develop --impure -c cargo test -p mara_core` → all existing tests pass.

### Step 2: First golden tests — leaf widgets through `*_backend` fns

`crates/core/tests/golden_paint.rs`: for 3 widgets to start —
`label_backend`, `toggle_backend`, one button-family backend fn:

1. Fix the environment: `set_theme(theme_pro(Mode::Dark))`, a fixed accent,
   `RecordingBackend` with `available = Rect from (0,0) to (320,64)`.
2. Run the widget backend fn; serialize `format!("{:#?}", backend.commands())`.
3. Compare against `tests/golden/<widget>.txt`; on mismatch print a unified
   diff hint (assert_eq output suffices). Regenerate-mode: an env var
   (`MARA_UPDATE_GOLDEN=1`) rewrites the file instead of asserting — ~10
   lines of helper, document it at the top of the test file.

**Verify**: `nix develop --impure -c cargo test -p mara_core --test golden_paint` → 3 tests pass; delete one golden file, re-run with `MARA_UPDATE_GOLDEN=1`, confirm regeneration, `git diff` on the golden is empty.

### Step 3: Probe the seam's edges (the actual spike)

Attempt goldens for 2 harder cases and **record what happens**:

- A pod render (post-plan-004/005 shape) — likely blocked: the pod path
  runs through `egui::Ui` (`container/normal/mod.rs:2891 fn paint_cmd(ui: &mut Ui, ...)`).
  If blocked, capture *where* the egui dependency bites (function + reason)
  — this is the IR-gap evidence ADR 0001 asks for.
- A `MaraUi`-driven sequence: construct `MaraUi` over the recording backend
  if its constructor permits (check whether `MaraUi` hard-requires
  `EguiUiBackend` — `grep -n 'EguiUiBackend' crates/core/src/mui/mod.rs`).
  If it hard-requires egui, that is finding #1 for the generalization work —
  record it; do not force a refactor.

**Verify**: a `## Spike findings` section appended to the bottom of THIS plan
file listing: what golden-tests work today, each blocker with `file:line`,
and a recommendation (generalize `MaraUi` next / IR gaps to fill / drop).

### Step 4: Gates

**Verify**: `nix develop --impure -c make check` → exit 0 (sealed greps
unaffected). `nix develop --impure -c make harden` → exit 0 (including
no-default-features — the new module must not be feature-gated behind egui;
if `backend/record.rs` accidentally depends on egui types beyond `vocab`,
fix that, it defeats the purpose).

## Test plan

Steps 2–3 ARE the test plan: ≥3 leaf-widget goldens passing, regeneration
mechanism proven, and the two probe attempts documented (pass or blocked).

## Done criteria

- [ ] `backend/record.rs` exists; existing layout tests green via the alias
- [ ] ≥3 golden paint tests pass; `MARA_UPDATE_GOLDEN=1` regeneration works
- [ ] `## Spike findings` appended to this plan with blockers as `file:line`
- [ ] `nix develop --impure -c make check` / `make harden` exit 0
- [ ] `plans/README.md` status row updated (DONE even if probes were blocked — the report is the deliverable)

## STOP conditions

- Goldens are nondeterministic across runs (ordering, floats formatting) —
  report the nondeterminism source; do not "fix" it by loosening comparison.
- The existing test `RecordingBackend` turns out to be load-bearing for
  behavior you'd change by mirroring `EguiUiBackend`'s cursor — keep the old
  one intact and give the new module its own semantics; report the delta.
- `make harden`'s no-default-features build fails because of the new module
  — the recording backend must build without egui-adjacent features; report
  if the crate's feature graph makes that impossible.

## Maintenance notes

- Plan 012 (module tests) builds on this: `mara_board`'s `on_draw` +
  internal `Layout` should golden-test cleanly (it draws raw `PaintCmd`s —
  ARCHITECTURE §6).
- Golden churn policy: theme changes will legitimately rewrite goldens —
  reviewers should expect golden diffs alongside intentional visual changes
  and treat unexplained golden diffs as regressions. Add this sentence to
  CLAUDE.md when convenient.
- The frozen `measure_text` constants are a contract; changing them
  invalidates every golden — never "tune" them casually.
