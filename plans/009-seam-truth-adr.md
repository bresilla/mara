# Plan 009: Make ARCHITECTURE.md honest about the backend seam, and record the seam decision as an ADR

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat bcf6600..HEAD -- ARCHITECTURE.md crates/core/src/widget/ crates/core/src/memory.rs`
> On unexplained mismatch with the excerpts, STOP.

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW (docs + one decision record; no behavior change)
- **Depends on**: none
- **Category**: docs / tech-debt
- **Planned at**: commit `bcf6600`, 2026-07-20

## Why this matters

ARCHITECTURE.md §3 claims the egui dependency is "contained in one adapter"
and that `MaraUi` routes "all ~40 internal call sites through the backend."
The code says otherwise: **all 10 widget files type their entry points on
`&mut egui::Ui`** (e.g. `widget/button.rs:193,405`), `egui::` appears in 57
files under `crates/core/src`, and per-id persisted state goes through
`ctx.data_mut` directly at ~150 sites across 19 files (including the new
`enforce.rs`) rather than through the sanctioned `MaraMemory`. A maintainer
or contributor relying on the doc would make wrong decisions (e.g. assume a
second backend is a weekend job, or that state is backend-neutral). This
plan corrects the doc to describe reality, and records — as a decision the
team can veto or amend — what the seam's *actual* contract is, so future
work (plan 010's recording backend, any migration) has a stated target
instead of a myth.

## Current state

- ARCHITECTURE.md §3 (excerpt): "**`MaraUi`** (`mui/mod.rs`) — what every
  widget and pane body receives. It holds an `EguiUiBackend`, *not* an
  `egui::Ui` … All ~40 internal call sites route through the backend, so the
  egui dependency is contained in one adapter."
- Reality checks (verified):
  - `crates/core/src/widget/button.rs:191-199`:

    ```rust
    pub(crate) fn show_egui(
        self,
        ui: &mut egui::Ui,
        accent: impl Into<MaraColor32>,
    ) -> ActionButtonResponse {
        ...
        let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
    ```

    All 10 files in `crates/core/src/widget/` contain `egui::Ui`-typed
    functions (`grep -rln 'egui::Ui' crates/core/src/widget/` → 10).
  - `grep -rln 'egui::' crates/core/src | wc -l` → 57 files.
  - `MaraMemory` (`crates/core/src/memory.rs`) is used in ~11 files, while
    direct `ctx.data_mut(/ctx.data(/animate_bool` hits ~149 sites across 19
    files (`pane/mod.rs:105-170` hand-written accessor pairs;
    `enforce.rs:49-136` stamps its keys directly).
  - The seam that IS real: the `UiBackend` trait (`layout.rs:641+`, with
    `begin_area/allocate/interact/paint/measure_text/push_clip/...`), the
    `PaintCmd` IR (`paint.rs`), `vocab` types, and `EguiUiBackend`
    (`backend/egui.rs`) — widgets *internally* build paint through
    `PaintCmd` and per-widget `*_backend` functions even though their entry
    signatures are egui-typed.
- ARCHITECTURE.md §12 states "persisted state goes through `MaraMemory`" —
  also overstated per above.
- No `docs/adr/` directory exists.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Check (nothing broke) | `nix develop --impure -c make check` | exit 0 |
| Grep evidence refresh | `grep -rln 'egui::Ui' crates/core/src/widget/ \| wc -l` | current count for the doc |

## Scope

**In scope**:
- `ARCHITECTURE.md` — §3 (the seam), §12 (state), and the §3 diagram caption.
- `docs/adr/0001-backend-seam-scope.md` (create, with the `docs/adr/` dir).

**Out of scope**:
- ANY Rust code change. This plan changes no behavior — migrating widget
  signatures or `MaraMemory` call sites is future work that the ADR frames.
- PLAN.md / PROGRESS.md — historical documents; leave them.

## Git workflow

- Branch from `develop`: `feature/009-seam-adr`
- Conventional commits, title only, ≤50 chars, no signature. Suggested:
  `docs: true up seam claims, add ADR 0001`
- Do NOT push or open a PR unless instructed.

## Steps

### Step 1: Refresh the evidence numbers

Re-run the greps in Current state against the live tree (counts may have
moved since `bcf6600`); use the fresh numbers in the doc text.

**Verify**: numbers recorded.

### Step 2: Correct ARCHITECTURE.md §3

Rewrite the overstated claims to describe the two-layer reality, keeping the
section's structure:

- The **paint/measure/interact seam is real**: widgets express painting as
  `PaintCmd` and interactions through `*_backend` functions over the
  `UiBackend` trait; `EguiUiBackend` is the only egui-aware *lowering* code.
- The **signature seam is not yet real**: widget entry points are typed on
  `&mut egui::Ui` and construct `EguiUiBackend` locally (N files, current
  count); `egui::` is referenced in N core files. Link the ADR for the
  migration direction.
- Replace "all ~40 internal call sites route through the backend, so the
  egui dependency is contained in one adapter" with the accurate statement.

Apply the same honesty pass to §12's `MaraMemory` sentence: `MaraMemory` is
the *widget-state* store; chrome/pane/shelf/enforce state currently uses the
egui data store directly (N sites), with consolidation tracked in the ADR.

**Verify**: `grep -n 'contained in one adapter' ARCHITECTURE.md` → 0 matches.

### Step 3: Write ADR 0001

`docs/adr/0001-backend-seam-scope.md`, standard ADR shape
(Status/Context/Decision/Consequences). Content to record — **as
"proposed", for the maintainer to accept/edit**:

- **Context**: the numbers from Step 1; the seam's original goal (backend
  swappability, per ARCHITECTURE.md §1); the cost paid today (wrapper
  indirection on every widget with no second backend).
- **Decision (proposed)**: keep the seam, scoped honestly —
  (a) the IR layer (`PaintCmd`, `vocab`, `UiBackend`, `MaraMemory` trait) is
  the portable contract; (b) *new* widget code must route through
  `UiBackend`/`MaraUi` and must not add `egui::Ui`-typed public/`pub(crate)`
  entry points; (c) existing egui-typed signatures migrate per PLAN.md's
  phased schedule, and each migrated surface **deletes its old egui-typed
  entry points in the same change** — greenfield rule, no back-compat, no
  grandfathered permanent residents (see PLAN.md "Ground rule");
  (d) `MaraMemory` is authoritative for widget state; chrome-level state may
  use the egui data store only until its Phase 2 migration lands, at which
  point the direct-access sites are deleted; (e) the proof obligation for the
  seam is a recording/headless backend (plans/010) — if that spike fails,
  this ADR must be revisited with "drop the trait" on the table.
- **Consequences**: what a contributor may/may not do; what `make check`
  gates enforce vs what remains convention.

**Verify**: file exists; `nix develop --impure -c make check` → exit 0
(docs-only change; gate confirms nothing else was touched).

## Test plan

None — documentation plan. The verification is grep-based (Step 2) plus the
gates confirming no code changed.

## Done criteria

- [ ] ARCHITECTURE.md no longer claims one-adapter containment or universal `MaraMemory` routing; describes the two-layer seam with current counts
- [ ] `docs/adr/0001-backend-seam-scope.md` exists, Status: Proposed
- [ ] `git diff --stat` shows only ARCHITECTURE.md + the new ADR
- [ ] `nix develop --impure -c make check` exits 0
- [ ] `plans/README.md` status row updated

## STOP conditions

- The live counts differ *drastically* from Current state (e.g. widgets were
  already migrated off `egui::Ui`) — the drift means someone is mid-refactor;
  report rather than documenting a moving target.
- You are tempted to "fix" any code while in there — that is out of scope by
  definition here.

## Maintenance notes

- The ADR's Status flips to Accepted only by the maintainer. Plans 010
  (recording backend) and 013 (theme/state consolidation) cite it; if the
  maintainer amends the decision, update those plans before executing them.
- Reviewer: check the corrected §3 still reads as *intent + current state*,
  not as an apology — the seam design is sound; the doc just claimed it was
  finished.
