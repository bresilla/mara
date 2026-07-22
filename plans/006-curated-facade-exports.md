# Plan 006: Curate the facade exports — one canonical import path

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat bcf6600..HEAD -- mara/src/ui.rs mara/src/lib.rs example/`
> On unexplained mismatch with the excerpts below, STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (breaking imports for any downstream consumer)
- **Depends on**: none (independent of 004/005, but coordinate merges — the
  example files are shared)
- **Category**: tech-debt (API surface)
- **Planned at**: commit `bcf6600`, 2026-07-20

## Why this matters

The `mara` facade re-exports its core **three overlapping ways**: the crate
itself (`pub use mara_core;`), a full glob (`pub use mara_core::*;`), and a
prelude that globs it again. Consumers demonstrate the confusion: the sealed
example imports `mara::ui::pod::Pod`, the main example imports
`mara_core::pod::Pod` directly and also reaches items through
`mara::ui::mara_core::…`. A glob re-export means every future `pub` item in
`mara_core` silently becomes facade API — the sealed surface cannot be
curated when it is defined as "everything". This plan replaces the globs with
an explicit re-export list and establishes `mara::ui` as the one canonical
path.

## Current state

- `mara/src/ui.rs:14-31` (uncommitted working-tree state included):

```rust
#[cfg(feature = "three-d")]
pub use mara_3d;
#[cfg(feature = "board")]
pub use mara_board;
...
pub use mara_core;
pub use mara_core::*;
...
pub use crate::host::{MaraHostCtx, MaraWindowHost};
```

  Plus `pub mod modules { ... }` (aliased module re-exports, `ui.rs:33-51`) and
  `pub mod prelude { ... pub use mara_core::*; ... }` (`ui.rs:52-70`).

- Import styles in first-party consumers:
  - `example/sealed/src/lib.rs:11` — `mara::ui::pod::Pod` style (via facade).
  - `example/src/app.rs:50-53` — `use mara_core::...` (direct crate paths;
    the example's Cargo.toml is *checked by a gate* to not list `mara_core`,
    so these resolve through the facade's `pub use mara_core;` — confirm in
    Step 1).
  - `example/src/app.rs` also uses `egui::Id::new(...)` (e.g. `:2770`) —
    determine in Step 1 which re-export makes `egui` nameable there; the
    `make check` gate forbids an `egui =` dependency line in
    `example/Cargo.toml`.
- `Makefile` `check` target (`Makefile:68+`) greps that must keep passing —
  notably `! grep -RInE '... ^\s*pub\s+use\s+egui([:;]|$)' crates/core/src mara/src`
  (no bare `pub use egui`) and the example-Cargo.toml dependency bans.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Check + sealed gates | `nix develop --impure -c make check` | exit 0 |
| Full test suite | `nix develop --impure -c make test-all` | exit 0 |
| Full gate | `nix develop --impure -c make harden` | exit 0 |
| Docs build | `nix develop --impure -c cargo doc -p mara --no-deps` | exit 0 |

## Scope

**In scope**:
- `mara/src/ui.rs` (re-export curation), `mara/src/lib.rs` (if it globs too —
  check), `mara/src/prelude` if separate.
- Import-line updates in `example/src/*.rs`, `example/sealed/src/lib.rs`.

**Out of scope**:
- Any `pub` item *inside* `mara_core` — visibility changes there are a
  separate decision (plan 009's ADR territory).
- `mara/plugin/bevy` exports.
- The `modules { ... }` aliased namespace — it is already curated; keep it.

## Git workflow

- Branch from `develop`: `feature/006-curated-exports`
- Conventional commits, title only, ≤50 chars, no signature. Suggested:
  `refactor(mara)!: curate facade re-exports`
- Do NOT push or open a PR unless instructed.

## Steps

### Step 1: Inventory what the glob actually supplies

Produce the real dependency list before deleting anything:

1. `grep -rn '^use \|^pub use \|    use ' example/src example/sealed/src | grep -v '^.*//'` — collect every path the examples import.
2. Determine how `egui::` resolves in `example/src/app.rs` (likely a
   re-export reachable through the glob, or a direct dep added by the
   uncommitted `example/Cargo.toml` change — `git diff bcf6600 -- example/Cargo.toml`).
3. Temporarily comment out `pub use mara_core::*;` in `mara/src/ui.rs`, run
   `nix develop --impure -c cargo check -p mara_example -p mara_example_sealed 2>&1 | grep 'unresolved\|cannot find' | sort -u`
   (get exact package names from `example/Cargo.toml` and
   `example/sealed/Cargo.toml`) — the error list **is** the required
   re-export set. Restore the line before continuing.

**Verify**: you have a written list (keep it in the commit message body? No —
title-only commits; keep it in `plans/006-notes.md` temporarily and delete
before finishing, or in the PR description if one is opened).

### Step 2: Replace the glob with a curated list

In `mara/src/ui.rs`:

- Delete `pub use mara_core::*;`.
- Keep `pub use mara_core;` **only if** Step 1 shows consumers using
  `mara_core::`-style paths through it that the curated list can't cover more
  cleanly; prefer deleting it and routing everything through named
  re-exports. (Recommended target: delete it; the module re-exports below
  cover namespacing.)
- Add explicit re-exports grouped and commented, e.g.:

```rust
// Surfaces
pub use mara_core::pane::{Pane, PaneBody, ...};
pub use mara_core::pod::{Pod, PodResponse, ...};
pub use mara_core::shelf::{...};
// Vocabulary
pub use mara_core::vocab::{Color32, Id, Pos2, Rect, Vec2, ...};
// Theme & style
pub use mara_core::style::{set_theme, Theme, ...};
...
```

  Exactly the Step 1 list plus items the facade's own docs/tests name —
  nothing speculative.
- Rebuild `prelude` from the same names (no glob), keeping it small: the
  types a hello-world needs (`MaraHostCtx`, `Pane`, `Pod`, `ShellBar`, vocab
  types, `set_theme`).

**Verify**: `nix develop --impure -c make check` → exit 0 (both examples compile; sealed gates pass).

### Step 3: Normalize example imports to the canonical path

Update `example/src/app.rs` (and siblings) to import via `mara::ui::…`
consistently instead of `mara_core::…`, matching the sealed example's style.

**Verify**: `grep -rn 'use mara_core' example/src/` → 0 matches (unless Step 2 deliberately kept `pub use mara_core;` — then this step documents the canonical style in module docs instead and the grep target is stated there).

### Step 4: Gates + docs

**Verify**: `nix develop --impure -c make test-all` → all pass.
**Verify**: `nix develop --impure -c make harden` → exit 0.
**Verify**: `nix develop --impure -c cargo doc -p mara --no-deps` → exit 0;
spot-check `target/doc/mara/ui/index.html` lists the curated set, not
hundreds of glob items.

## Test plan

No new unit tests — the "test" is that both examples and the sealed
compile-test build against the curated surface (that is exactly what
`example/sealed/` exists for: "compile-test that only the sealed API is
reachable", per ARCHITECTURE.md §2). Ensure `make check` covers both.

## Done criteria

- [ ] `grep -n 'mara_core::\*' mara/src/` → 0 matches
- [ ] `mara/src/ui.rs` re-exports are grouped, explicit, commented
- [ ] `nix develop --impure -c make check` exits 0
- [ ] `nix develop --impure -c make test-all` exits 0
- [ ] `nix develop --impure -c make harden` exits 0
- [ ] `plans/README.md` status row updated; temporary notes file deleted

## STOP conditions

- The Step 1 unresolved-name list exceeds ~120 items — the curation would be
  a rubber-stamp of the glob; report the list and recommend narrowing
  `mara_core`'s own `pub` surface first (plan 009 discussion).
- `egui::` in `example/src/app.rs` turns out to resolve **through the
  facade** (i.e. the glob currently leaks `egui` itself to consumers) — this
  is a sealed-API hole worth its own note; report it before deciding whether
  the curated list must preserve it (it should NOT — but removing it may
  break `Pod::new(egui::Id::new(...))` call sites, which then need vocab
  `Id` instead; that sweep is in scope only if small, else report).
- Any sealed-API grep gate in `make check` fails — never adjust the Makefile.

## Maintenance notes

- New public items in `mara_core` no longer auto-appear in the facade —
  adding one now requires a deliberate re-export line. That is the point;
  note it in `CLAUDE.md` (plan 001's file) if landed after it.
- Reviewer: check the prelude stayed minimal; preludes that re-glob defeat
  the change.
- Downstream (`bevy_openusd` per `MARA_ADDITIONS.md`) will need an import
  sweep on upgrade — changelog entry required at next release prep.
