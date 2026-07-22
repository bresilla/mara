# Plan 012: Characterization tests for the untested module crates (board, three_d)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat bcf6600..HEAD -- crates/modules/board/ crates/modules/three_d/`
> On mismatch with the stated shapes, re-read before proceeding.

## Status

- **Priority**: P3
- **Effort**: M
- **Risk**: LOW (tests only)
- **Depends on**: plans/010-recording-backend-spike.md (soft — board goldens
  use it if available; the plan works without it)
- **Category**: tests
- **Planned at**: commit `bcf6600`, 2026-07-20

## Why this matters

Test coverage is concentrated in `mara_core` (436 tests); the module crates
that constitute Mara's extensibility story ship with **zero**: `mara_board`
(recent, still-moving surface), `mara_three_d` (7,043 lines in one file),
`mara_graph`, `mara_code`. Any rework — including several plans in this
directory — has no regression net there. This plan adds characterization
tests (pin current behavior, no fixes) to the two highest-value targets:
`board` (small, recently built, feeds the MultiView/VT direction) and
`three_d` (largest untested surface, retained state machine worth pinning).
Graph gets its first tests via plan 011; `code` is a vendored editor facade
and is deliberately deferred.

## Current state

- `crates/modules/board/src/lib.rs` — ~170 lines, thin facade; the Board is
  a `MaraView` + `MaraModule` whose consumer draws `PaintCmd` primitives via
  `on_draw(|b: BoardPaint| …)` with an optional internal `Layout`
  (ARCHITECTURE.md §6: `b.painter`, `b.rect`, `b.response`, `b.accent`,
  `b.cells`; `b.cell("data_mask") → cell rect`). Read the actual struct and
  entry points first: `cat crates/modules/board/src/lib.rs`.
- Shared layout used by Board/MultiView — `mara_core`'s `Layout`
  (`crates/core/src/view/layout.rs` per ARCHITECTURE §6):
  `Layout::{cell, row, col}` → `layout.resolve(rect) -> Vec<(CellId, Rect)>`.
  Pure geometry — ideal test target; check what tests it already has in
  core (`grep -rn 'resolve' crates/core/tests/ crates/core/src/view/layout.rs | grep -i test`)
  and do NOT duplicate those — board tests cover board's *use* of it.
- `crates/modules/three_d/src/lib.rs` — 7,043 lines: retained
  scene/camera/object surface; geometry rebuilds are cached behind a
  signature check at `:3234` (per the perf audit). Public API: enumerate
  with `grep -n 'pub fn' crates/modules/three_d/src/lib.rs | head -80`.
- Existing module-test exemplars to copy structurally:
  `crates/modules/map/` has 24 tests, `crates/modules/canvas/` has 2 —
  locate them (`grep -rn '#\[test\]' crates/modules/map/src | head`) and
  match their placement convention (inline `#[cfg(test)]` modules).
- GPU constraint: `make test-all` runs headless in CI (plan 001) — tests
  must not require a window, GL context, or wgpu device. Retained-state and
  geometry logic only.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Board tests | `nix develop --impure -c cargo test -p mara_board` | all pass |
| three_d tests | `nix develop --impure -c cargo test -p mara_3d` | all pass (confirm package name from `crates/modules/three_d/Cargo.toml`) |
| Full suite | `nix develop --impure -c make test-all` | exit 0 |
| Full gate | `nix develop --impure -c make harden` | exit 0 |

## Scope

**In scope**:
- `crates/modules/board/src/` — `#[cfg(test)]` modules (+ a `tests/` dir if
  integration shape fits better).
- `crates/modules/three_d/src/lib.rs` — `#[cfg(test)]` module.
- Test-only visibility tweaks (`pub(crate)` helpers) — behavior must not change.

**Out of scope**:
- Fixing any behavior a test reveals as odd — characterization pins what IS;
  file oddities in the report instead.
- `graph` (plan 011), `code`, `image`, `canvas`, `map`, `bevy` modules.
- Anything needing a GPU/display.

## Git workflow

- Branch from `develop`: `feature/012-module-tests`
- Conventional commits, title only, ≤50 chars, no signature. Suggested:
  `test(modules): characterize board and three_d`
- Do NOT push or open a PR unless instructed.

## Steps

### Step 1: Board — surface contract tests (~6 tests)

Read `board/src/lib.rs` fully, then pin:

- Construction: `Board::new(id, title)` default state (no layout → whatever
  `cells` yields — empty or one full-rect cell; pin the actual behavior).
- `with_layout(Layout::row(...))` + a known rect → `b.cell("name")` returns
  the expected sub-rect (compute expected from the weights by hand).
- Unknown cell name → pin the actual behavior (None/panic — if it panics,
  wrap in `#[should_panic]` and flag in the report as a plan-002-style
  hardening candidate).
- `on_draw` callback receives the board rect it was given; if the recording
  backend (plan 010) is available, golden-test a trivial `on_draw` that
  paints one rect + one text: assert the emitted `PaintCmd`s. If 010 hasn't
  landed, assert via whatever paint-capture the module's types allow, or
  skip the golden and note it.

**Verify**: `nix develop --impure -c cargo test -p mara_board` → ≥6 pass.

### Step 2: three_d — retained-state tests (~8 tests)

From the `pub fn` inventory, target the retained scene/camera/object model
(no rendering):

- Scene: add object → present in whatever query/lookup API exists; remove →
  gone; duplicate/id-collision behavior pinned.
- Camera: default pose pinned; a setter (orbit/target/zoom) round-trips.
- The geometry-cache signature (`lib.rs:3234` area): calling the rebuild
  path twice with identical inputs hits the cache (expose the cache-hit
  observable via a `pub(crate)` counter or by checking the cached handle is
  identical — read the code and pick the least invasive observable); a
  changed input invalidates.
- Any pure math helpers (bounds, picking rays) with hand-computed cases.

If the crate's types cannot be constructed without a GL context, STOP per
the conditions below rather than mocking heavily.

**Verify**: `nix develop --impure -c cargo test -p mara_3d` (or actual
package name) → ≥8 pass.

### Step 3: Full gates

**Verify**: `nix develop --impure -c make test-all` → exit 0, total test
count increased by ≥14. `nix develop --impure -c make harden` → exit 0
(tests must also build under `--no-default-features` per harden — gate new
tests on the features they need with `#[cfg(feature = ...)]` matching how
the module's own code is gated).

## Test plan

This plan IS a test plan; structure per Steps 1–2. Pattern: `map`'s inline
test modules. Naming: behavioral sentences like the shelf tests
(`collapse_bottom_promotes_when_no_right_shelf` style).

## Done criteria

- [ ] `mara_board` ≥6 tests, `three_d` crate ≥8 tests, all passing
- [ ] Zero behavior changes outside `#[cfg(test)]` / test-only visibility
- [ ] Oddities found (panics, surprising defaults) listed in the report
- [ ] `nix develop --impure -c make test-all` and `make harden` exit 0
- [ ] `plans/README.md` status row updated

## STOP conditions

- `three_d` types genuinely require a live GL/render context even for scene
  bookkeeping — report which types; the fallback scope is board-only plus
  the pure-math helpers, and the report should recommend a
  construction-seam refactor as a future plan.
- A characterization test reveals a crash-on-misuse (unknown cell, duplicate
  object id) — pin it with `#[should_panic]`, do NOT fix it here; list it.
- Board's API surface differs materially from the ARCHITECTURE §6 sketch —
  the doc or the recon drifted; re-read the source and proceed from source,
  noting the doc drift.

## Maintenance notes

- These tests define the module contract for plans 013/014-era refactors and
  any board/VT direction work — breaking them knowingly requires updating
  the pinned expectation in the same commit, with the change called out.
- When plan 010's goldens exist, migrate board paint assertions to goldens.
- Follow-up candidates deliberately left out: `mara_code` facade smoke test,
  `mara_image`/`mara_canvas` state tests — cheap to add later using these as
  the pattern.
