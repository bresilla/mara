# Plan 008: Auto-derive pod/container ids from scope (kill mandatory Id plumbing)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat bcf6600..HEAD -- crates/core/src/pod/mod.rs crates/core/src/pane/ example/src/app.rs`
> Plans 004/005/007 change these files legitimately; re-read live shapes.
> On unexplained mismatch, STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (id stability across frames is state-critical)
- **Depends on**: plans/005-typed-pod-responses.md (keys reduce what ids must disambiguate); best after 007
- **Category**: tech-debt (API ergonomics)
- **Planned at**: commit `bcf6600`, 2026-07-20

## Why this matters

Every surface currently requires the consumer to mint a stable `Id`:
`Pod::new(id)`, `body.add_normal(container_id, …)`, pane ids, and so on. The
example app contains **218** id constructions and defines its own helpers
just to cope (`example/src/app.rs:3554-3558` — `cid(pane, name)` /
`pid(pane, name, i)`). Getting an id wrong silently breaks persisted state
and animations frame-to-frame. Meanwhile the framework already knows a stable
scope at every call site: the container knows its pane, the pod knows its
container and position. This plan derives ids from scope + declaration
context automatically, keeping explicit ids as the override for dynamic
content.

## Current state

- `example/src/app.rs:3554-3558`:

```rust
fn cid(pane: &str, name: &str) -> egui::Id { egui::Id::new((pane, name)) }
fn pid(pane: &str, name: &str, i: usize) -> egui::Id { egui::Id::new((pane, name, i)) }
```

  (Signatures approximate — read the live helpers; they may use vocab `Id`.)
  218 `Id::new(/cid(/pid(` call sites in `app.rs`.

- `Pod::new(id)` — sole constructor (`grep -n 'pub fn new' crates/core/src/pod/mod.rs`).
- `body.add_normal(container_id, title, icon, pods)` — container id supplied
  by caller (`example/src/app.rs:2798`).
- Internal id-salting convention — `crates/core/src/pane/mod.rs:105-170`
  salts sub-state as `pane_id.with("body_open")` etc.; the same `.with(...)`
  mechanism is available for derivation.
- Critical invariant: ids key **persisted** state
  (`ctx.data_mut(get_persisted/insert_persisted)`) — a derived id must be
  identical every frame for the same logical widget, and must NOT depend on
  render order alone if consumers conditionally show pods (the example's
  map-objects pane builds *different pod lists* depending on selection —
  `app.rs:2764-2796` — so a bare per-container counter would remap state
  when the list changes shape).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Core only | `nix develop --impure -c cargo check -p mara_core` | exit 0 |
| Check + sealed gates | `nix develop --impure -c make check` | exit 0 |
| Full test suite | `nix develop --impure -c make test-all` | exit 0 |
| Full gate | `nix develop --impure -c make harden` | exit 0 |

## Scope

**In scope**:
- `crates/core/src/pod/mod.rs` — `Pod::auto()` constructor (or
  `Pod::default()`-like anonymous form).
- The `add_normal`/`add_tabbed` family (locate:
  `grep -rn 'fn add_normal\|fn add_tabbed' crates/core/src`) — derive
  container ids from pane scope + title when the caller passes a new
  `Auto` marker; keep the explicit-id overloads.
- `example/src/app.rs` — migrate the static panes (those whose pod list
  shape is constant); leave dynamic pod lists on explicit ids.

**Out of scope**:
- Pane ids and view ids — top-level identity stays explicit (they key
  routing, ribbons, and cross-frame docking; the risk/benefit is worse).
- egui's own auto-id machinery — do not expose or imitate
  `Ui::auto_id_with`; derivation must go through Mara's stable scope data.

## Git workflow

- Branch from `develop`: `feature/008-auto-ids`
- Conventional commits, title only, ≤50 chars, no signature. Suggested:
  `feat(core): scope-derived pod/container ids`
- Do NOT push or open a PR unless instructed.

## Steps

### Step 1: Derivation rule

Implement id derivation as **scope × stable-name**, never bare position:

- Container: `pane_or_shelf_id.with(("container", title))` — the title
  string is the stable name (containers have titles; two same-titled
  containers in one pane get `.with(occurrence_index)` salting plus a
  `debug_assert!` advising explicit ids).
- Pod: `container_id.with(("pod", primary_key))` where `primary_key` is the
  pod's first widget key (from plan 005) — content-derived, so a
  conditionally-present pod keeps its identity regardless of neighbors.
  A pod with zero keyed widgets falls back to declaration index **with** a
  `debug_assert!` advising an explicit id.

API shape: `Pod::auto()` (no id) + existing `Pod::new(id)` untouched;
`add_normal` gains a sibling `add_normal_auto(title, icon, pods)` (or an
`impl Into<ContainerId>` accepting an `Auto` marker — pick whichever matches
the codebase's builder style, document the choice in the commit).

**Verify**: rule written as rustdoc on `Pod::auto()` before implementing.

### Step 2: Implement + wire the render path

The pod render path resolves `Pod::auto()` ids at the point where the
container id is known (the same place `PodResponse`s are keyed). Persisted
state must use the resolved id exactly as an explicit one would.

**Verify**: `nix develop --impure -c cargo check -p mara_core` → exit 0.

### Step 3: Characterization test BEFORE migrating the example

- `auto_pod_id_stable_across_frames`: render the same auto-pod twice;
  persisted slider state written in frame 1 is read in frame 2.
- `auto_pod_id_survives_neighbor_removal`: container with keyed pods A, B, C;
  remove B; A and C's derived ids are unchanged (assert directly on resolved
  ids).
- `auto_container_duplicate_titles_salted`: two "Settings" containers in one
  pane resolve distinct ids deterministically.

**Verify**: `nix develop --impure -c make test-all` → all pass.

### Step 4: Migrate static example panes

Convert panes in `example/src/app.rs` whose pod list is unconditional to
`Pod::auto()`/auto containers; delete `cid`/`pid` **only if** no call sites
remain, else leave them. Dynamic panes (map-objects selection pane, anything
building different pod lists per state) keep explicit ids — add a one-line
comment there: `// explicit ids: pod list shape is state-dependent`.

**Verify**: `nix develop --impure -c make check && nix develop --impure -c make test-all` → exit 0.
**Verify**: `grep -c 'Id::new(\|cid(\|pid(' example/src/app.rs` — expect a
substantial drop from 218; record the number in the report.

## Test plan

Step 3's three tests, in the pod test module (same harness as plans 005/007
tests). Pattern: existing pod/container tests.

## Done criteria

- [ ] `Pod::auto()` + auto-container path exist with the derivation rule in rustdoc
- [ ] 3 characterization tests pass
- [ ] Example's static panes migrated; id-construction count in `app.rs` reduced and recorded
- [ ] `nix develop --impure -c make check` / `make test-all` / `make harden` all exit 0
- [ ] `plans/README.md` status row updated

## STOP conditions

- Plan 005's keys are not landed (derivation depends on widget keys) — land
  005 first.
- The container render path cannot see a stable parent scope id at pod-id
  resolution time (i.e. pods render before their container id is fixed) —
  report the actual ordering; do not derive from render order.
- Migrating a pane in Step 4 visibly resets its persisted state (fold state,
  slider values) under `make run` — expected for the one-time id change, but
  if state *keeps* resetting across frames, the derivation is unstable: STOP.

## Maintenance notes

- The one-time state reset on migration (old explicit id → new derived id)
  is acceptable for the example but is a **breaking persistence change** for
  downstream apps — changelog entry required; downstreams opt in by calling
  `Pod::auto()`, so nothing silently remaps.
- Reviewer: hunt for order-dependence in the derivation — the two tests in
  Step 3 (neighbor removal, duplicate titles) are the review checklist.
- If plan 007's `with_ui` pods carry no widget keys, their auto-id falls to
  the index fallback — recommend explicit ids there until a better content
  key exists.
