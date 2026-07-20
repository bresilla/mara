# Plan 005: Replace the positional `PodResponse` bag with keyed, typed lookup

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat bcf6600..HEAD -- crates/core/src/pod/mod.rs example/src/app.rs`
> If these files changed since this plan was written (plan 004 intentionally
> changes both — that is expected and fine; anything else, compare excerpts),
> on an unexplained mismatch treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED (breaking public-API change on the response side)
- **Depends on**: plans/004-ambient-accent-pods.md (same file; land 004 first)
- **Category**: tech-debt (API ergonomics)
- **Planned at**: commit `bcf6600`, 2026-07-20

## Why this matters

`PodResponse` is 19 parallel `Vec` fields (`buttons`, `toggles`, `sliders`,
`colors`, …), correlated to the widgets that produced them **only by
declaration order within each kind**. Consumers write
`pod_responses.iter().find_map(|p| p.colors.first()).filter(|c| c.changed)`
to read one color picker, and reordering `with_*` calls silently rebinds
results to the wrong handler — a refactor hazard with no compile-time or
runtime signal. This plan gives every widget a stable key (auto-derived from
its label, overridable) and gives `PodResponse` typed keyed accessors, so
reads become `resp.color("color")` and reordering is harmless.

## Current state

- `crates/core/src/pod/mod.rs:44-67`:

```rust
/// What a [`Pod`] surfaces to the caller per frame. One vec per
/// widget kind, in declaration order within that kind.
#[derive(Clone, Debug, Default)]
pub struct PodResponse {
    pub searches: Vec<SearchResponse>,
    pub buttons: Vec<ButtonResponse>,
    pub card_buttons: Vec<ButtonResponse>,
    pub action_buttons: Vec<ActionButtonPodResponse>,
    pub toggles: Vec<ToggleResponse>,
    pub progress: Vec<ProgressResponse>,
    pub sliders: Vec<SliderResponse>,
    pub drag_values: Vec<DragValueResponse>,
    pub dropdowns: Vec<DropdownResponse>,
    pub selects: Vec<SelectResponse>,
    pub hybrid_selects: Vec<HybridSelectPodResponse>,
    pub colors: Vec<ColorResponse>,
    pub readouts: Vec<ReadoutResponse>,
    pub select_lists: Vec<SelectListResponse>,
    pub hybrid_select_lists: Vec<HybridSelectListResponse>,
    pub tags: Vec<TagsResponse>,
    pub keybindings: Vec<KeybindingsResponse>,
    pub badges: Vec<BadgesResponse>,
    pub modules: Vec<ModulePodResponse>,
}
```

- Consumer pattern — `example/src/app.rs:2800-2814`:

```rust
let responses = body.render();
let Some(pod_responses) = responses.get(&container_id) else { return; };
let Some(color) = pod_responses
    .iter()
    .find_map(|pod_response| pod_response.colors.first())
    .filter(|color| color.changed)
else { return; };
```

- Builders push `WidgetSpec` variants carrying a config; each widget family
  renders into its family Vec. Most builders take a `label`/`placeholder`
  string that is a natural key. Documentation on builders references the
  positional contract, e.g. `with_slider` doc: "Read the resolved value back
  from `PodResponse::sliders[i].value`."

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Core only | `nix develop --impure -c cargo check -p mara_core` | exit 0 |
| Check + sealed gates | `nix develop --impure -c make check` | exit 0 |
| Full test suite | `nix develop --impure -c make test-all` | exit 0, all pass |
| Full gate | `nix develop --impure -c make harden` | exit 0 |

## Scope

**In scope**:
- `crates/core/src/pod/mod.rs` — `PodResponse`, `WidgetSpec` configs (add a
  `key: String`), builders (derive/accept keys), the render site that fills
  responses.
- `example/src/app.rs`, `example/src/enforced.rs`, `example/sealed/src/lib.rs`
  and any module-crate consumers of `PodResponse` fields
  (`grep -rln '\.buttons\b\|\.toggles\b\|\.sliders\b\|\.colors\b' example/ crates/modules/ mara/`).

**Out of scope**:
- Builder *input* parameters beyond adding key derivation (004 already
  reshaped them; 007 owns the immediate-closure alternative).
- `MaraResponse` / the `MaraUi` immediate surface.
- Container/`PaneBody::render()` return shape (`HashMap<Id, Vec<PodResponse>>`
  stays as-is in this plan).

## Git workflow

- Branch from `develop`: `feature/005-typed-pod-responses`
- Conventional commits, title only, ≤50 chars, no signature. Suggested:
  `feat(core)!: keyed typed pod responses`
- Do NOT push or open a PR unless instructed.

## Steps

### Step 1: Key every widget spec

- Add `key: String` to each `WidgetSpec` config struct (Button, Toggle,
  Slider, Color, …).
- Derivation rule in each `with_*`: `key = label` (or `placeholder` for
  search, first badge label for badges — pick the human-visible string the
  builder already requires; for builders with no string at all, derive
  `format!("{kind}-{index}")` from the widget's position). Document the rule
  on each builder.
- Add one override builder applying to the **most recently added widget**:

```rust
/// Set an explicit response key for the widget added by the
/// immediately preceding `with_*` call. Defaults to its label.
pub fn key(mut self, key: impl Into<String>) -> Self { ... }
```

- Duplicate keys within one pod: salt with `-2`, `-3`… by occurrence at
  render time, and `debug_assert!` with a message naming the pod id — silent
  salting in release, loud in dev.

**Verify**: `nix develop --impure -c cargo check -p mara_core` → exit 0.

### Step 2: Add keyed accessors to `PodResponse` (keep the Vecs)

Keep the 19 public Vec fields **unchanged** (avoids breaking every consumer
in one step), add parallel key storage + typed accessors:

```rust
pub fn button(&self, key: &str) -> Option<&ButtonResponse>;
pub fn toggle(&self, key: &str) -> Option<&ToggleResponse>;
pub fn slider(&self, key: &str) -> Option<&SliderResponse>;
pub fn color(&self, key: &str) -> Option<&ColorResponse>;
... (one per family)
```

Implementation: per-family `Vec<String>` of keys filled at the same render
site that pushes each response (keys and responses stay index-aligned within
a family — the invariant is local to one function; add a
`debug_assert_eq!(keys.len(), responses.len())` after fill).

Also add the aggregate helpers the example needs:

```rust
/// First changed color across these pods, by key.
pub fn find_color<'a>(pods: &'a [PodResponse], key: &str) -> Option<&'a ColorResponse>;
```

(free function or trait — match existing helper style in `pod/mod.rs`).

**Verify**: `nix develop --impure -c make check` → exit 0.

### Step 3: Migrate first-party consumers to keyed reads

Sweep `example/src/app.rs` (and the other Scope files): replace positional
digs with keyed lookups, e.g. the excerpt above becomes:

```rust
let Some(color) = pod_responses.iter().find_map(|p| p.color("color")).filter(|c| c.changed)
else { return; };
```

Update the builder doc comments that referenced `PodResponse::sliders[i]` to
reference keyed lookup.

**Verify**: `nix develop --impure -c make check` → exit 0.
**Verify**: `grep -n '\.colors\.first()\|\.buttons\.first()\|\.sliders\.first()' example/src/` → 0 matches.

### Step 4: Gates

**Verify**: `nix develop --impure -c make test-all` → all pass.
**Verify**: `nix develop --impure -c make harden` → exit 0.

## Test plan

New tests in the pod test module (locate existing pod tests first:
`grep -rn '#\[test\]' crates/core/src/pod/ crates/core/tests/ | grep -i pod`):

- `pod_response_keyed_lookup`: pod with two buttons "a", "b" → `button("b")`
  returns the second button's response.
- `pod_response_reorder_safe`: build the same two widgets in swapped order →
  `button("b")` still resolves to the same logical widget.
- `pod_response_duplicate_keys_salted`: two buttons labelled "x" →
  `button("x")` and `button("x-2")` both resolve (this test must tolerate the
  `debug_assert` if it fires under test — if it panics in the test profile,
  assert the panic message instead, or downgrade the duplicate signal to the
  salting alone; report which you chose).
- `pod_response_explicit_key_override`: `.with_button("Long Label").key("k")`
  → `button("k")` resolves.

## Done criteria

- [ ] Typed keyed accessors exist for every response family (19)
- [ ] Positional field reads are gone from first-party consumers (Step 3 grep = 0)
- [ ] `nix develop --impure -c make check` exits 0
- [ ] `nix develop --impure -c make test-all` exits 0 with ≥4 new tests
- [ ] `nix develop --impure -c make harden` exits 0
- [ ] `plans/README.md` status row updated

## STOP conditions

- A builder family has **no** natural string to key on *and* appears multiple
  times per pod in real consumers (check the example) — the positional
  fallback key may collide with user expectations; report with the list of
  affected families.
- `PodResponse` is constructed or pattern-matched exhaustively somewhere
  (struct literal / destructuring) that the added private fields break —
  report the sites (the struct derives `Default`, so literals may use
  `..Default::default()` and survive; verify).
- Plan 004 has not landed and `pod/mod.rs` still has accent params — land
  004 first; do not interleave.

## Maintenance notes

- Deprecating the public Vec fields (turning them `#[doc(hidden)]` or
  private) is an explicit follow-up once downstream consumers (see
  `MARA_ADDITIONS.md`'s consumer) migrate — kept out of this plan so the
  change lands in two observable stages.
- Plan 007 builds the immediate-closure alternative; keyed responses remain
  the contract for the declarative path.
- Reviewer: check the key-salting order is deterministic (declaration order),
  and that no accessor allocates per call.
