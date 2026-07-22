# Plan 002: Make caller-contract violations non-fatal in release builds

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat bcf6600..HEAD -- crates/core/src/pane/mod.rs crates/core/src/container/normal/mod.rs`
> If either file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none (001 recommended first so gates are CI-enforced)
- **Category**: bug
- **Planned at**: commit `bcf6600`, 2026-07-20

## Why this matters

Two library code paths currently turn a *caller mistake* into a process
abort. (1) Rendering a pane without first publishing ribbon pane ids hits an
`expect` + `assert!` and panics mid-frame — in an immediate-mode loop, a
transient ordering slip (e.g. panes drawn before the ribbon on one frame)
takes down the whole window. (2) `resolve_active_tab_idx` guards its
"non-empty tabs" precondition only with `debug_assert!`, then computes
`tab_ids.len() - 1`, which underflows to `usize::MAX` in release and panics
on the subsequent index. Mara's stated contract ("using the toolkit at all
yields a correct Mara app") argues for degrading gracefully in release while
keeping the loud signal in debug builds.

## Current state

- `crates/core/src/pane/mod.rs:254-262`:

```rust
fn assert_pane_has_ribbon_button(ctx: &egui::Context, pane_id: Id) {
    let ids = ctx
        .data(|d| d.get_temp::<Vec<Id>>(ribbon_pane_ids_key()))
        .expect("pane rendering requires published ribbon pane ids; publish the current ribbon pane buttons through MaraHostCtx before rendering panes");
    assert!(
        ids.contains(&pane_id),
        "pane {:?} was rendered without a registered ribbon button; add a ribbon item for it or do not render the pane",
        pane_id
    );
}
```

  Called unconditionally from `Pane::__internal_show` (`pane/mod.rs:~584`),
  immediately after `crate::enforce::__internal_enforce_defaults(ctx);`.

- `crates/core/src/container/normal/mod.rs:1631-1650`:

```rust
fn resolve_active_tab_idx(ctx: &egui::Context, active_idx_key: Id, tab_ids: &[Id]) -> usize {
    debug_assert!(!tab_ids.is_empty());
    ctx.data_mut(|d| {
        if let Some(active_id) = d.get_persisted::<Id>(active_tab_id_key(active_idx_key))
            && let Some(idx) = tab_ids.iter().position(|id| *id == active_id)
        { ... }
        let stored = d.get_persisted::<usize>(active_idx_key).unwrap_or(0);
        let clamped = stored.min(tab_ids.len() - 1);   // underflow if empty
        ...
        d.insert_persisted(active_tab_id_key(active_idx_key), tab_ids[clamped]); // then panics
        clamped
    })
}
```

  Today's callers guard (`:294` returns early when `tabs.is_empty()` before
  the `:332` call), so the underflow is **latent** — this is hardening
  against a future caller, not a live bug.

- Convention: the crate has no logging dependency; misuse feedback in this
  codebase is delivered via debug assertions and doc comments. Match that —
  do not add a `log` dependency.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Check + sealed gates | `nix develop --impure -c make check` | exit 0 |
| Full test suite | `nix develop --impure -c make test-all` | exit 0, all pass |
| Full gate | `nix develop --impure -c make harden` | exit 0 |

## Scope

**In scope**:
- `crates/core/src/pane/mod.rs` (the guard function and its call site only)
- `crates/core/src/container/normal/mod.rs` (`resolve_active_tab_idx` only)

**Out of scope**:
- `crates/core/src/enforce.rs` — adjacent but separate machinery; its
  hysteresis logic must not be touched here.
- Publishing ribbon ids automatically for the caller — that changes the
  enforcement contract and belongs to a deliberate design discussion.
- Any other `assert!`/`expect` in the workspace — an audit checked them; the
  rest are in `#[cfg(test)]` or on infallible paths.

## Git workflow

- Branch from `develop`: `feature/002-nonfatal-guards`
- Conventional commits, title only, ≤50 chars, no signature. Suggested:
  `fix(core): degrade pane/tab misuse gracefully`
- Do NOT push or open a PR unless instructed.

## Steps

### Step 1: Convert the pane guard to a checked predicate

Replace `assert_pane_has_ribbon_button` with a predicate returning `bool`:

```rust
/// True when `pane_id` has a published ribbon button this frame.
/// Rendering a pane without one is a caller-contract violation:
/// loud in debug builds, skip-render in release.
fn pane_has_ribbon_button(ctx: &egui::Context, pane_id: Id) -> bool {
    ctx.data(|d| d.get_temp::<Vec<Id>>(ribbon_pane_ids_key()))
        .is_some_and(|ids| ids.contains(&pane_id))
}
```

At the call site in `__internal_show`, replace the call with:

```rust
let has_button = pane_has_ribbon_button(ctx, self.id);
debug_assert!(
    has_button,
    "pane {:?} rendered without published ribbon pane ids; publish ribbon pane buttons through MaraHostCtx before rendering panes",
    self.id
);
if !has_button {
    return; // release: skip this pane rather than aborting the app
}
```

Keep the original message text so existing debug workflows recognize it.

**Verify**: `nix develop --impure -c make check` → exit 0.

### Step 2: Harden `resolve_active_tab_idx`

At the top of the function, replace the bare `debug_assert!` with:

```rust
debug_assert!(!tab_ids.is_empty());
if tab_ids.is_empty() {
    return 0; // release: caller invariant broken; 0 is the safe sentinel
}
```

Nothing after the early return needs to change.

**Verify**: `nix develop --impure -c make check` → exit 0.

### Step 3: Add regression tests

Tests compile with `debug_assertions`, so test the **predicate**, not the
panic path:

- In `crates/core/src/pane/mod.rs`'s existing `#[cfg(test)]` module (there
  are 12 tests there already — match their style), add:
  - `pane_ribbon_predicate_false_when_unpublished`: fresh `egui::Context`,
    assert `pane_has_ribbon_button(&ctx, Id::new("p"))` is `false` and that
    the call did not panic.
  - `pane_ribbon_predicate_true_after_publish`: publish ids via the existing
    publish path (see `ribbon_pane_ids_key()` writer at `pane/mod.rs:248-252`),
    assert `true`.
- In `container/normal/mod.rs` tests: `resolve_active_tab_idx` cannot be
  called with an empty slice under debug (the `debug_assert` fires), so add
  the test the other way: assert the non-empty path still resolves and clamps
  (`stored=5`, 2 tabs → returns 1). The empty-slice release behavior is
  covered by the early-return being before any arithmetic — note this in a
  comment.

**Verify**: `nix develop --impure -c make test-all` → all pass, including the new tests.

## Test plan

Covered in Step 3. Pattern: the existing `#[cfg(test)] mod tests` blocks in
the same files (e.g. the pane tests around fold/anchor logic). Total: 3 new
tests.

## Done criteria

- [ ] `grep -n 'assert_pane_has_ribbon_button' crates/core/src` → no matches (renamed to predicate)
- [ ] `nix develop --impure -c make check` exits 0
- [ ] `nix develop --impure -c make test-all` exits 0 with 3 new tests
- [ ] `nix develop --impure -c make harden` exits 0
- [ ] No files outside the in-scope list modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

- The excerpts don't match the live code (the pane file has uncommitted
  churn in the working tree — the enforce call site was added recently).
- Skipping an unregistered pane in release turns out to break an existing
  test that *relies* on rendering panes without published ribbon ids — that
  would mean the contract is looser than documented; report instead of
  weakening the debug assert.
- You find other callers of `resolve_active_tab_idx` beyond the two known
  sites (`:332`, `:514`) with different guarding — report.

## Maintenance notes

- If a future "headless pane" feature legitimately renders panes without
  ribbon buttons, the predicate is the single place to add that allowance.
- Reviewer: scrutinize that the release fallback is `return` (skip), not
  rendering the pane anyway — skipping keeps the ribbon/pane invariant
  visible in release rather than silently diverging chrome.
