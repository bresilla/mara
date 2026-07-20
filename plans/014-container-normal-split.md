# Plan 014: Split the container god-object into cohesive modules

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. This is a **pure-motion refactor** — zero
> behavior change; every stage must keep the full suite green. When done,
> update the status row in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat bcf6600..HEAD -- crates/core/src/container/`
> Plans 004/005/007 may have touched pod-adjacent code; the container file
> itself should be structurally as described. On mismatch, re-read.

## Status

- **Priority**: P3
- **Effort**: L
- **Risk**: MED (highest-traffic layout code; mitigated by pure-motion discipline)
- **Depends on**: plans/012-module-characterization-tests.md not required;
  the existing shelf/container/pane suites are the net. Land AFTER the
  ergonomics track (004/005/007) to avoid churn collisions.
- **Category**: tech-debt
- **Planned at**: commit `bcf6600`, 2026-07-20

## Why this matters

`crates/core/src/container/normal/mod.rs` is 3,742 lines / 76 functions —
the single largest core module. It carries title-zone layout, body-zone
layout, resize bookkeeping, folder-tab AND top-tab painting, icon/SVG paint,
banner/floating-icon geometry, and the tab-state machine. The "tabbed" and
"body" container variants (`container/tabbed/mod.rs` 121 ln,
`container/body/mod.rs` 98 ln) are thin shells that delegate INTO `normal`,
so every container change funnels through one file, and unrelated concerns
(geometry math vs. paint vs. persisted state) interleave. A cohesive split
makes the ergonomics-track changes (pods render inside containers) and any
future container work reviewable.

## Current state

Function map of `container/normal/mod.rs` (from `grep -n 'pub fn\|^fn '`):

- Builder/config: `new` (:145), `reserve_tab_strip_in_parent` (:176),
  `min_body_flow` (:182), `initial_flow` (:202), `min_width` (:212),
  `icon` (:220), `tabbed_strip_side` (:229), `body_flow` (:237)
- Render entry: `show` (:248, `pub(crate)`), `show_tabs` (:289),
  `show_raw` (:779, `pub(crate) fn show_raw(self, ui: &mut Ui, body: impl FnOnce(&mut Ui))`)
- Geometry helpers (pure or near-pure): `tabbed_container_max_rect` (:1412),
  `folder_tab_strip_rect` (:1439), `top_tab_title_rect` (:1475),
  `separator_debug_rect` (:1482), `rect_expanded_by_margin` (:1486),
  `title_banner_rect` (:1499), `floating_icon_geometry` (:1533),
  `tabbed_strip_outer_inset` (:1620), `title_slot_size` (:1678),
  `body_slot_sizes` (:1686), `body_full_rect` (:1705),
  `folder_tab_cell_geometry` (:1723), `top_tab_cell_geometry` (:1809),
  `active_tab_border_points` (:2304)
- Tab state: `active_tab_id_key` (:1627), `resolve_active_tab_idx` (:1631 —
  plan 002 hardens this; land 002 first or rebase)
- Painting: `paint_folder_tabs` (:1852), `paint_top_tabs` (:2086),
  `paint_tab_rect_chrome` (:2351), `tab_rect_chrome_paint_cmds` (:2364),
  `top_tab_label_paint_cmd` (:2381), `paint_icon_or_svg` (:2392),
  `icon_name_paint_cmd` (:2419), `icon_svg_paint_cmd` (:2429),
  `title_divider_paint_cmd` (:2440), `paint_title` (:2484),
  `paint_floating_icon` (:2828), `paint_cmd` (:2891),
  `paint_cmd_clipped` (:2896)
- Also: ~49 direct `ctx.data_mut` state-accessor lines and `#[cfg(test)]`
  content at the tail (:3505+).

Sibling shells: `container/tabbed/mod.rs` (121 ln) and `container/body/mod.rs`
(98 ln) delegate into `normal`.

Existing regression net: `crates/core/src/shelf/tests.rs` (107 tests —
heavy container coverage), `crates/core/tests/` suites, pane tests.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Core tests | `nix develop --impure -c cargo test -p mara_core` | all pass, count unchanged |
| Check + sealed gates | `nix develop --impure -c make check` | exit 0 |
| Full suite | `nix develop --impure -c make test-all` | exit 0 |
| Full gate | `nix develop --impure -c make harden` | exit 0 |

## Scope

**In scope**:
- `crates/core/src/container/normal/` — new sibling files:
  `geometry.rs`, `tabs.rs` (state + both tab-painting families),
  `paint.rs` (icon/title/banner paint + `paint_cmd*`), keeping `mod.rs` for
  the builder + `show*` orchestration.

**Out of scope**:
- ANY logic change — including "obvious" cleanups, dedup between
  `paint_folder_tabs`/`paint_top_tabs`, or the `ctx.data_mut` →
  `MaraMemory` migration (ADR 0001 scope). Motion only.
- `container/tabbed/`, `container/body/` — the delegation pattern stays;
  re-pointing their imports is allowed, restructuring them is not.
- Visibility widening beyond `pub(super)`/`pub(crate)` within `container/`.

## Git workflow

- Branch from `develop`: `feature/014-container-split`
- One commit per extraction below. Conventional commits, title only, ≤50
  chars, no signature. Suggested: `refactor(core): extract container geometry`
  etc.
- Do NOT push or open a PR unless instructed.

## Steps

Record the baseline first: `nix develop --impure -c cargo test -p mara_core 2>&1 | tail -3` — note the exact pass count; every step must reproduce it.

### Step 1: Extract `geometry.rs`

Move the 14 geometry helpers listed above (pure rect/size math — verify each
has no `ctx`/`ui` param before moving; any that do stay in `mod.rs` for now,
note them). Adjust to `pub(super)` as needed.

**Verify**: test count identical; `wc -l crates/core/src/container/normal/mod.rs` shrinks accordingly. Commit.

### Step 2: Extract `tabs.rs`

Move tab state (`active_tab_id_key`, `resolve_active_tab_idx`) and the tab
paint families (`paint_folder_tabs`, `paint_top_tabs`,
`folder_tab_cell_geometry`, `top_tab_cell_geometry`,
`active_tab_border_points`, `paint_tab_rect_chrome`,
`tab_rect_chrome_paint_cmds`, `top_tab_label_paint_cmd`) plus any
`#[cfg(test)]` tests that target them (move tests WITH their subjects).

**Verify**: test count identical. Commit.

### Step 3: Extract `paint.rs`

Move `paint_icon_or_svg`, `icon_name_paint_cmd`, `icon_svg_paint_cmd`,
`title_divider_paint_cmd`, `paint_title`, `paint_floating_icon`,
`paint_cmd`, `paint_cmd_clipped`, `title_banner_rect`-adjacent paint pieces
if not already in geometry.

**Verify**: test count identical. Commit.

### Step 4: Final shape check

`mod.rs` should now hold: config builder, `show`/`show_tabs`/`show_raw`
orchestration, and the persisted-state accessors (deliberately kept — their
migration is ADR-scoped). Target: `mod.rs` under ~1,500 lines.

**Verify**: `nix develop --impure -c make check` → exit 0;
`nix develop --impure -c make test-all` → exit 0 at baseline count;
`nix develop --impure -c make harden` → exit 0. Commit.

## Test plan

No new tests — the invariant is the existing suite at an identical count at
every commit boundary, plus `git diff --color-moved=dimmed-zebra` reviewable
as pure motion.

## Done criteria

- [ ] `normal/mod.rs` ≤ ~1,500 lines; `geometry.rs`/`tabs.rs`/`paint.rs` exist
- [ ] Test pass count identical to the recorded baseline at every commit
- [ ] No visibility wider than `pub(crate)`; no public path changes
- [ ] `nix develop --impure -c make check` / `make test-all` / `make harden` exit 0
- [ ] `plans/README.md` status row updated

## STOP conditions

- A "move" cannot compile without changing logic (hidden coupling through
  local state or macro) — report the function and coupling; do not refactor
  logic to force the move.
- Test count changes at any boundary — find the silently-dropped test first.
- Plans 004/005/007 are mid-flight in the same area — coordinate; rebase
  hazards in `pod`-adjacent code are likely. Land after them.

## Maintenance notes

- The tab-painting families (`folder` vs `top`) remain near-duplicates BY
  DESIGN of this plan — dedup is a follow-up once they live side by side in
  `tabs.rs` and the diff is readable. Do not attempt both in one PR.
- Reviewer: `--color-moved` should show almost everything as moved lines;
  scrutinize any non-moved hunk.
- The ~49 `ctx.data_mut` accessors intentionally stay in `mod.rs` — they are
  the visible TODO for the ADR-0001 state-migration decision.
