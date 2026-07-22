# Plan 013: Consolidate the theme/style runtime (split style.rs, one snapshot, per-context metrics)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. This plan is **staged — each stage is
> independently landable**; stop cleanly at a stage boundary if risk
> materializes. When done, update the status row in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat bcf6600..HEAD -- crates/core/src/style.rs`
> Plan 003 adds a generation counter to this file (expected). Any other
> structural change: re-read before proceeding.

## Status

- **Priority**: P3
- **Effort**: L
- **Risk**: MED-HIGH (global reads are load-bearing across every widget)
- **Depends on**: plans/003-theme-read-fast-path.md (its generation counter
  is the invalidation seam); plans/009-seam-truth-adr.md (decision context)
- **Category**: tech-debt
- **Planned at**: commit `bcf6600`, 2026-07-20

## Why this matters

Theme and responsive state live in ~19 process-global statics scattered
through a 3,615-line `style.rs`: `ACTIVE_THEME` (RwLock), `ACTIVE_ACCENT`,
`RAW_ACCENT`, `SCREEN_WH`, `SCREEN_FLAGS`, `TOUCH_DENSITY_OVERRIDE`, glass
opacity, font-weight state, and assorted `LAST_*` dedupe caches. Three costs:
(1) **two windows/contexts of different sizes cannot coexist** — the last
context to stamp `set_screen_metrics` wins, so breakpoint/density/theme can
flip-flop per frame if a second sized context renders Mara widgets;
(2) the theme system is untestable without global mutation and unable to
support per-window themes, ever; (3) `style.rs` is a monolith mixing theme
data, responsive logic, fonts, glass, and color helpers. This plan splits the
file, gathers the globals behind one runtime struct, and moves *responsive
metrics* to per-`egui::Context` storage. Full per-context themes stay a
non-goal for now (ADR 0001 territory) — globals remain the storage, but
behind one door.

## Current state

- `crates/core/src/style.rs:2556-2570` — packed screen metrics:

```rust
static SCREEN_WH: core::sync::atomic::AtomicU32 = ...;   // u16 w | u16 h
// byte 0: breakpoint discriminant, byte 1: touch_density bool,
// bytes 2..4: pixels_per_point * 100 (u16). ...
static SCREEN_FLAGS: core::sync::atomic::AtomicU32 = ...;
static TOUCH_DENSITY_OVERRIDE: AtomicU8 = AtomicU8::new(0);
```

  Written by `set_screen_metrics(ctx)` (`:~2604`) from `ctx.content_rect()`
  each pass; read by `screen_class()` / `screen_metrics()` / `touch_density()`
  with **no context key**.
- `style.rs:2669-2724` — `ACTIVE_ACCENT` + `RAW_ACCENT` packed-u32 atomics
  with `set_active_accent`/`set_raw_accent` (private) and
  `active_accent()`/`raw_accent()` (public).
- `style.rs:2725+` — `ACTIVE_THEME: OnceLock<RwLock<Theme>>` (+ plan 003's
  `THEME_GENERATION` once landed).
- Other statics: enumerate live with
  `grep -n '^static \|^    static \|static [A-Z_]*:' crates/core/src/style.rs`
  (audit counted 19 including `GLASS_OPACITY`, `ACTIVE_FONT_WEIGHT`,
  `ACTIVE_TITLE_WEIGHT`, `TITLE_FONT_READY`, and `LAST_*` dedupe caches at
  `:73,:414,:420,:469,:560-578`).
- `enforce.rs` acknowledges multi-context reality: "a secondary offscreen
  context (e.g. the node-graph renderer) … is never touched" — but style
  globals have no such per-context isolation; the graph module's secondary
  context is exactly the hazard case if it ever drives Mara widgets.
- File size: 3,615 lines. Existing suite: `crates/core/tests/theme_contract.rs`
  + shelf/pane/container tests exercise themed rendering heavily — they are
  the net for this refactor.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Core check | `nix develop --impure -c cargo check -p mara_core` | exit 0 |
| Check + sealed gates | `nix develop --impure -c make check` | exit 0 |
| Full test suite | `nix develop --impure -c make test-all` | exit 0 |
| Full gate | `nix develop --impure -c make harden` | exit 0 |

## Scope

**In scope**:
- `crates/core/src/style.rs` → split into `crates/core/src/style/`
  (`mod.rs`, `theme_data.rs`, `runtime.rs`, `responsive.rs`, `glass.rs`,
  `fonts.rs`, `color.rs` — names indicative; keep the `style::` public paths
  identical via re-exports in `mod.rs`).
- `set_screen_metrics` callers (find:
  `grep -rn 'set_screen_metrics' crates/ mara/`) — signature keeps taking
  `ctx`; storage moves into `ctx.data`.

**Out of scope**:
- Changing any public function name or signature in `style::` — this is an
  internal reorganization; 138+ call sites must not change.
- Per-context *themes* (only metrics go per-context) — record as ADR
  follow-up if wanted.
- `themes/` built-in definitions (flat/pro/game) beyond moving them intact.

## Git workflow

- Branch from `develop`: `feature/013-style-runtime`
- One commit **per stage** below. Conventional commits, title only, ≤50
  chars, no signature. Suggested: `refactor(core): split style module` /
  `refactor(core): style runtime struct` / `fix(core): per-context metrics`
- Do NOT push or open a PR unless instructed.

## Steps

### Stage A: Mechanical file split (no logic change)

1. Create `crates/core/src/style/` and move code blocks wholesale:
   theme structs + built-ins → `theme_data.rs`; the statics + their
   accessors → `runtime.rs`; breakpoint/screen-metrics → `responsive.rs`;
   glass/fonts/color helpers → their files. `mod.rs` re-exports so every
   existing `crate::style::X` path resolves unchanged.
2. NO behavior edits — pure motion. If a block resists moving (private
   cross-references), move both halves to the same file rather than changing
   visibility semantics beyond `pub(crate)` promotions within `style/`.

**Verify**: `nix develop --impure -c make check` → exit 0;
`nix develop --impure -c make test-all` → identical pass count to before
(record before/after counts). `git diff --stat` shows only `style/` +
`lib.rs` mod declaration. Commit.

### Stage B: One runtime struct behind the same doors

In `runtime.rs`, define:

```rust
/// All process-global style state, in one place. Public accessors
/// (`theme()`, `active_accent()`, …) delegate here — the statics are
/// an implementation detail of this struct's single instance.
struct StyleRuntime { ... }
```

Fold the scattered statics into (or behind) it — the packed-atomic encodings
may stay as fields; the point is one type owns them, one doc comment
explains lifecycle (who writes when: theme hooks per-frame, `set_theme` by
app), and the `LAST_*` dedupe caches are named fields with comments instead
of loose statics. Public accessor signatures unchanged.

**Verify**: `nix develop --impure -c make test-all` → identical pass count.
`grep -c '^static \|    static ' crates/core/src/style/*.rs` → count
reported; target is a handful (the OnceLock singleton + atomics inside the
struct where `&'static` access needs them). Commit.

### Stage C: Per-context responsive metrics

1. Change `responsive.rs` storage: `set_screen_metrics(ctx)` writes the
   packed metrics into `ctx.data_mut` under a Mara key AND (unchanged) the
   global — dual-write.
2. Readers that have a `ctx` in reach read per-context; readers with no ctx
   (the deep widget paths — this is most of them) keep the global.
   **Pragmatic goal**: the *entry points* that decide layout per-context
   (shelf/ribbon reflow, `screen_class()` calls inside functions that
   already take `ctx` — find them: `grep -rn 'screen_class()\|screen_metrics()\|touch_density()' crates/core/src | grep 'ctx' `)
   go per-context; leaf widgets inherit whatever their pass stamped, which
   is correct because a pass renders one context.
3. The remaining hazard window (context A stamps globals, context B's leaf
   widget reads mid-B-pass) closes because B's pass *begins* by stamping B's
   metrics (`set_screen_metrics` runs per-pass — verify the call site order
   in the theme-apply/enforce path before relying on this; if a Mara-widget
   render can precede the stamp in a pass, report it).

**Verify**: `nix develop --impure -c make test-all` → green. New tests
(below) pass. Commit.

## Test plan

- Stage A/B: the existing 436-test suite at identical counts is the test.
- Stage C, add to `crates/core/tests/theme_contract.rs` (or a new
  `responsive_contract.rs` beside it):
  - `metrics_are_per_context`: two `egui::Context`s, run a pass on each with
    different `content_rect` sizes (drive via `ctx.run` with synthetic
    `RawInput::screen_rect`), call `set_screen_metrics` inside each pass,
    assert `screen_class()`-relevant reads inside pass A see Phone while
    pass B sees Desktop (use sizes straddling the breakpoints — read the
    breakpoint thresholds from `responsive.rs` and pick e.g. 350×700 vs
    1600×900).
  - `second_context_does_not_thrash_first`: after both passes, re-run pass A
    and assert its class is stable frame-over-frame.

## Done criteria

- [ ] `style.rs` replaced by `style/` module; all public `style::` paths unchanged (`nix develop --impure -c make check` exits 0 with zero call-site edits outside style/)
- [ ] Loose statics consolidated; count recorded in the report
- [ ] Per-context metrics tests pass; full suite green at every stage commit
- [ ] `nix develop --impure -c make harden` exits 0
- [ ] `plans/README.md` status row updated (note which stages landed)

## STOP conditions

- Stage A moves force visibility changes visible outside `crate::style` — a
  public path would change; report instead of renaming.
- Stage C: you find a Mara widget render path that runs **before**
  `set_screen_metrics` in a pass (stamp-order assumption false) — report
  with the call chain; the dual-write must not land without it.
- Test count drops at any stage boundary — a test silently stopped
  compiling; find it before committing.
- Merge conflicts with plans 003 (same file) — rebase on the landed 003; if
  003 is unlanded, land it first.

## Maintenance notes

- This is deliberately NOT per-context themes. If that becomes a goal
  (multi-window with different skins), `StyleRuntime` is the type to
  instantiate per context and the ADR to amend.
- Reviewer: Stage A should be verifiable as pure motion (`git diff
  --color-moved=dimmed-zebra` makes it obvious); flag any hunk that isn't
  a move.
- Plan 011's `RenderStamp` can adopt the theme generation from plan 003
  after this lands.
