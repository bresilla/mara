# Plan 003: Make `theme()` reads cheap (generation-checked thread-local cache)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat bcf6600..HEAD -- crates/core/src/style.rs`
> If `style.rs` changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: perf
- **Planned at**: commit `bcf6600`, 2026-07-20

## Why this matters

`theme()` is called from every widget paint path — 31 call sites in
`widget/tree.rs` alone, 26 in `pod/mod.rs`, 20 in `container/normal/mod.rs`,
138 static call sites overall, most of them executed one-or-more times per
widget per frame. Each call acquires an `RwLock` read guard and **memcpys the
entire `Theme` struct** (106 direct fields plus nested `Copy` sub-theme
structs — on the order of a kilobyte). In a frame rendering hundreds of
widgets that is thousands of lock acquisitions and roughly hundreds of MB/s
of pointless copying at 60fps. The theme is effectively constant within a
frame, so a generation-checked thread-local copy makes every call after the
first a single relaxed atomic load + pointer-stable read.

## Current state

- `crates/core/src/style.rs:1937-1938` — `#[derive(Copy, Clone, Debug)] pub struct Theme { ... }` (106 fields).
- `crates/core/src/style.rs:2725-2745`:

```rust
static ACTIVE_THEME: std::sync::OnceLock<std::sync::RwLock<Theme>> = std::sync::OnceLock::new();

fn theme_lock() -> &'static std::sync::RwLock<Theme> {
    ACTIVE_THEME.get_or_init(|| std::sync::RwLock::new(theme_pro(Mode::Dark)))
}

fn read_unpoisoned<T>(lock: &std::sync::RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> { ... }
fn write_unpoisoned<T>(lock: &std::sync::RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> { ... }

/// Replace the active theme. Takes effect on the next paint —
...
    *write_unpoisoned(theme_lock()) = t;
```

- `crates/core/src/style.rs:2748-2754`:

```rust
/// Return a copy of the active theme. `Theme` is `Copy`, so widgets
/// can call this freely — no lifetimes, no allocation. Reads are
/// `RwLock::read`; under typical UI contention (none) the cost is a
/// single relaxed atomic.
pub fn theme() -> Theme {
    *read_unpoisoned(theme_lock())
}
```

  (The doc comment's cost claim describes lock acquisition, not the
  ~1KB struct copy the `*` performs. The public signature returning `Theme`
  by value is used at 138 call sites — **do not change the signature**.)

- Existing test coverage: `crates/core/tests/theme_contract.rs` exercises
  theme switching; `make harden` runs the suite with all feature
  combinations.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Check + sealed gates | `nix develop --impure -c make check` | exit 0 |
| Full test suite | `nix develop --impure -c make test-all` | exit 0, all pass |
| Full gate | `nix develop --impure -c make harden` | exit 0 |

## Scope

**In scope**:
- `crates/core/src/style.rs` — only the `theme()` / `set_theme` /
  `theme_lock` cluster and a new generation counter + thread-local.

**Out of scope**:
- The 138 call sites — the whole point is that they keep compiling unchanged.
- The other style globals (`ACTIVE_ACCENT`, `SCREEN_WH`, …) — plan 013 owns
  their consolidation.
- Changing `Theme`'s derive set or field layout.

## Git workflow

- Branch from `develop`: `feature/003-theme-fast-path`
- Conventional commits, title only, ≤50 chars, no signature. Suggested:
  `perf(core): cache theme reads per thread`
- Do NOT push or open a PR unless instructed.

## Steps

### Step 1: Add a theme generation counter

Next to `ACTIVE_THEME`, add:

```rust
/// Bumped on every theme replacement; lets readers detect staleness
/// without taking the lock.
static THEME_GENERATION: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
```

In every function that writes through `write_unpoisoned(theme_lock())`
(search `write_unpoisoned(theme_lock())` — `set_theme` and any sibling
mutators; also check for in-place field mutation via a write guard), add
`THEME_GENERATION.fetch_add(1, Ordering::Release);` **after** the write guard
is dropped (end of the statement/scope).

**Verify**: `grep -n 'write_unpoisoned(theme_lock())' crates/core/src/style.rs` — every hit is followed by a generation bump. `nix develop --impure -c make check` → exit 0.

### Step 2: Add the thread-local fast path in `theme()`

```rust
pub fn theme() -> Theme {
    thread_local! {
        static CACHE: std::cell::Cell<(u64, Theme)> =
            std::cell::Cell::new((u64::MAX, theme_pro(Mode::Dark)));
    }
    let generation = THEME_GENERATION.load(Ordering::Acquire).wrapping_add(1);
    CACHE.with(|c| {
        let (cached_gen, cached) = c.get();
        if cached_gen == generation {
            return cached;
        }
        let fresh = *read_unpoisoned(theme_lock());
        c.set((generation, fresh));
        fresh
    })
}
```

Notes for the executor:
- The `wrapping_add(1)` offsets the sentinel so a fresh thread (sentinel
  `u64::MAX`) never matches generation 0's offset value of 1. Keep it exactly
  as written.
- The function still *returns* `Theme` by value, so call sites are untouched;
  the win is eliminating the RwLock acquisition on the hot path and making
  the copy come from thread-local memory. (A same-thread copy of a `Copy`
  struct is what call sites already pay; the lock + shared-cacheline read was
  the contended part.)
- Update the doc comment on `theme()` to describe the actual cost model:
  "one relaxed atomic load; falls back to the lock only after `set_theme`."

**Verify**: `nix develop --impure -c make check` → exit 0.

### Step 3: Prove freshness across `set_theme`

Add to the existing `#[cfg(test)]` module in `style.rs` (there is one — the
raw `.expect` at `style.rs:3601` lives in it):

- `theme_read_sees_set_theme_immediately`: read `theme()`, capture a field
  with a known distinct value between two built-in themes (e.g. switch
  between `theme_pro(Mode::Dark)` and `theme_flat(Mode::Light)` — pick any
  field that provably differs; assert inequality first), call `set_theme`,
  assert `theme()` reflects the new value on the same thread.
- `theme_read_stable_within_generation`: two consecutive `theme()` calls with
  no `set_theme` between them return identical values (use a handful of
  fields or `format!("{:?}", t)` equality).

**Verify**: `nix develop --impure -c make test-all` → all pass including the 2 new tests; `theme_contract.rs` still green.

## Test plan

Step 3's two unit tests plus the existing `crates/core/tests/theme_contract.rs`
suite as the regression net. No benchmark harness exists in-repo
(`make bench` runs `cargo bench` but no benches are defined) — do not add one
under this plan.

## Done criteria

- [ ] `theme()` contains no unconditional `read_unpoisoned` on the hot path (lock hit only on generation mismatch)
- [ ] Every `write_unpoisoned(theme_lock())` site bumps `THEME_GENERATION`
- [ ] `nix develop --impure -c make check` exits 0
- [ ] `nix develop --impure -c make test-all` exits 0 with 2 new tests
- [ ] `nix develop --impure -c make harden` exits 0
- [ ] No files outside `crates/core/src/style.rs` modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

- You find a code path that mutates the theme **through the read guard** or
  holds a write guard across a paint (would make the generation counter
  unsound) — report it.
- You find `theme()` being called from a thread that must observe another
  thread's `set_theme` *mid-frame* (search for `std::thread::spawn` in
  `crates/core/src` — none is expected) — the thread-local design assumes
  set-then-paint on the same thread; report if violated.
- `theme_contract.rs` fails after the change — do not weaken the test.

## Maintenance notes

- Plan 013 (theme runtime consolidation) subsumes this mechanism into a
  per-context snapshot; the generation counter survives as its invalidation
  signal. Land this first — it is the low-risk seam 013 builds on.
- Reviewer: check the `Ordering` pair (Release on bump, Acquire on load) and
  that the bump happens after the write-guard drop.
