# Plan 004: Remove the per-call `accent` parameter from Pod builders (ambient accent)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat bcf6600..HEAD -- crates/core/src/pod/mod.rs crates/core/src/style.rs example/src/app.rs`
> If these files changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED (breaking public-API change; large mechanical call-site sweep)
- **Depends on**: none (001 recommended first for CI)
- **Category**: tech-debt (API ergonomics)
- **Planned at**: commit `bcf6600`, 2026-07-20

## Why this matters

Nearly every `Pod` builder method demands an `accent: impl Into<Color32>`
argument — `with_button`, `with_toggle`, `with_slider`, `with_color_rgb`,
`with_tag_items`, `with_badges`, and more. The accent is a **global the
framework already tracks** (`style::active_accent()`), and the parallel
immediate-mode surface (`MaraUi`) already resolves it ambiently —
`mui.button("apply")` takes no accent. The result: `accent` appears 172 times
in the example app alone, pure boilerplate on every widget declaration. This
plan makes `Pod` match `MaraUi`: accent is ambient by default, overridable
once per pod. This is a **deliberate breaking change**; the crate is pre-1.0
and the repo has precedent for compile-breaking API corrections.

## Current state

- `crates/core/src/pod/mod.rs:809-821` (representative — the same shape
  repeats for every widget family):

```rust
/// Add a plain button widget. `label` is the centred caption.
/// Click status is reported in `PodResponse::buttons[i]`.
pub fn with_button(mut self, label: impl Into<String>, accent: impl Into<Color32>) -> Self {
    let label = label.into();
    let accent = accent.into();
    assert_non_empty("buttons", "label", &label);
    self.widgets.push(WidgetSpec::Button(ButtonConfig {
        label,
        accent,
        ...
    }));
    self
}
```

  All `with_*` methods taking `accent` (find them:
  `grep -n 'accent: impl Into<Color32>' crates/core/src/pod/mod.rs` —
  expect roughly: search/button/button_subtitle/button_animated/
  button_styled/card_button/card_action_button/toggle/slider/drag_value/
  color_rgb/tag_items/badges/…). Each stores the resolved color in its
  `WidgetSpec::*Config` struct.

- The ambient source already exists — `crates/core/src/style.rs:~2698`:

```rust
/// Read the current accent colour. Hosts apply the Mara theme each
/// frame through facade/internal hooks, which keeps this in sync. ...
pub fn active_accent() -> MaraColor32 { ... }
```

- The precedent surface — `crates/core/src/mui/mod.rs:696-700`:

```rust
// ── widgets (ambient accent) ─────────────────────────────────
pub fn button(&mut self, label: &str) -> MaraResponse {
    button(self.backend.ui_mut(), label, self.accent)
}
```

- The consumer pattern being eliminated — `example/src/app.rs:2760+`:

```rust
fn map_objects_pane(body: &mut PaneBody, map: &mut MapViewState) {
    let accent = body.accent();
    ...
    .with_color_rgb("color", map_annotation_rgb(annotation), accent),
```

  `PaneBody::accent()` exists, meaning pane bodies already know their accent
  ambiently — consumers fetch it only to thread it back into pods.
  `grep -c accent example/src/app.rs` → 172.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Check + sealed gates | `nix develop --impure -c make check` | exit 0 |
| Full test suite | `nix develop --impure -c make test-all` | exit 0, all pass |
| Full gate | `nix develop --impure -c make harden` | exit 0 |

## Scope

**In scope**:
- `crates/core/src/pod/mod.rs` — builder signatures, `WidgetSpec` config
  structs (accent fields become `Option<Color32>`), render-time resolution.
- `example/src/app.rs`, `example/src/enforced.rs`, `example/sealed/src/lib.rs`
  — call-site sweep.
- Any other first-party caller of the changed builders:
  `grep -rln 'with_button\|with_toggle\|with_slider\|with_color_rgb\|with_tag_items\|with_badges' crates/ mara/ example/` (module crates may build pods).
- `crates/core/src/pod/` tests if they construct pods with accents.

**Out of scope**:
- `MaraUi` (`mui/mod.rs`) — already ambient; untouched.
- `PodResponse` shape — plan 005 owns response ergonomics; do not merge the
  two changes.
- `style.rs` — `active_accent()` is consumed as-is.
- Widget implementations under `crates/core/src/widget/` — they keep
  receiving a concrete accent from the pod render path.

## Git workflow

- Branch from `develop`: `feature/004-ambient-accent`
- Conventional commits, title only, ≤50 chars, no signature. Suggested:
  `feat(core)!: ambient accent for pod builders`
- Do NOT push or open a PR unless instructed.

## Steps

### Step 1: Add pod-level accent state and resolution

In `Pod`, add a field `accent_override: Option<Color32>` (default `None`) and
a builder:

```rust
/// Override the ambient accent for every widget in this pod.
/// Without this, widgets use the active theme accent.
pub fn accent(mut self, accent: impl Into<Color32>) -> Self {
    self.accent_override = Some(accent.into());
    self
}
```

Add a private `fn resolved_accent(&self) -> Color32 {
self.accent_override.unwrap_or_else(|| crate::style::active_accent().into()) }`
(match the actual conversion — `active_accent()` returns `MaraColor32`; check
what `Color32` means inside `pod/mod.rs`, it is the vocab type).

**Verify**: `nix develop --impure -c make check` → still exit 0 (additive so far).

### Step 2: Drop `accent` parameters from the builders

For every `with_*` method carrying `accent: impl Into<Color32>`:
1. Remove the parameter.
2. Change the config struct's `accent: Color32` field to
   `accent: Option<Color32>` **or**, simpler and preferred: leave the config
   field as-is and resolve at push time is NOT possible (the ambient value
   must be read at *render* time, after the theme hook ran) — so make the
   config field `Option<Color32>` set to `None`, and at the single render
   site where each config is consumed, substitute
   `config.accent.unwrap_or(pod_resolved_accent)`.
3. Keep doc comments; delete their accent-parameter sentences.

Order of work: change `pod/mod.rs` fully first; the workspace will not
compile until Step 3 — that is expected. Commit only after Step 3.

**Verify**: `cargo check -p mara_core` (inside the devshell) → exit 0
(core compiles standalone before sweeping consumers).

### Step 3: Sweep the consumers

Mechanically update every call site that passed an accent:

- `example/src/app.rs` — the dominant pattern is a local
  `let accent = body.accent();` threaded into builders. Remove the argument
  from the builder calls; delete the now-unused `let accent = ...;` bindings
  (clippy `-D warnings` in `make harden` will catch leftovers).
  **Judgment rule**: if a call site passes something *other than* the local
  ambient accent (a hard-coded color, a per-item color), preserve behavior
  with `.accent(that_color)` on the pod — or if it differs per widget within
  one pod, STOP and report (see STOP conditions).
- `example/sealed/src/lib.rs`, `example/src/enforced.rs`, and any module-crate
  callers found by the Scope grep.

**Verify**: `nix develop --impure -c make check` → exit 0.
**Verify**: `grep -n 'accent: impl Into<Color32>' crates/core/src/pod/mod.rs` → no matches.

### Step 4: Full gates + visual smoke

**Verify**: `nix develop --impure -c make test-all` → all pass.
**Verify**: `nix develop --impure -c make harden` → exit 0 (catches unused
variables from Step 3 and fmt drift).
Optional if a display is available: `make run` and confirm accent-tinted
widgets (buttons, toggles, color rows) still render tinted, not gray.

## Test plan

- In `pod/mod.rs`'s test module (or `crates/core/tests/` if pod tests live
  there — locate with `grep -rn 'Pod::new' crates/core --include='*tests*'`),
  add:
  - `pod_accent_defaults_to_active_accent`: set a known accent through the
    style API, build a pod with one button, render through the existing test
    harness, assert the resolved config/paint uses that accent.
  - `pod_accent_override_wins`: `.accent(RED)` beats the ambient value.
- Pattern: match whatever existing pod/container tests do for render-path
  assertions; if no render-path test harness exists for pods, assert on the
  resolved config level and note it.

## Done criteria

- [ ] `grep -c 'accent' example/src/app.rs` drops from 172 to < 40
- [ ] `grep -n 'accent: impl Into<Color32>' crates/core/src/pod/mod.rs` → 0 matches
- [ ] `Pod::accent(...)` override exists and is doc-commented
- [ ] `nix develop --impure -c make check` exits 0
- [ ] `nix develop --impure -c make test-all` exits 0 (incl. 2 new tests)
- [ ] `nix develop --impure -c make harden` exits 0
- [ ] `plans/README.md` status row updated

## STOP conditions

- A pod passes **different accents to different widgets within the same
  pod** somewhere in the sweep — the pod-level override can't express that;
  report the sites instead of inventing a per-widget API.
- `active_accent()` turns out to be stale at pod-render time on some host
  (widgets render before any theme hook ran) — check `enforce.rs` applies the
  theme before pod rendering; if not reproducible, report.
- The sealed example (`example/sealed`) uses an accent type that can't reach
  `.accent(...)` through the sealed surface — report; do not unseal anything.
- Any `make check` grep gate fails — never edit the Makefile to pass.

## Maintenance notes

- Plan 005 (typed responses) and plan 007 (immediate pod closures) touch the
  same file; land this first — it shrinks every signature they build on.
- `MARA_ADDITIONS.md` contains proposed widget signatures with explicit
  accent params; a docs follow-up should align them (tracked in plan README
  notes, not this plan).
- Reviewer: diff should show *only* removed parameters, `Option` plumbing,
  and call-site deletions — any behavioral change to widget painting is a
  red flag.
