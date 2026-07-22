# Plan 011: Skip redundant node_view sub-context renders (dirty-check + texture reuse)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat bcf6600..HEAD -- crates/modules/graph/src/node_view.rs`
> On mismatch with the excerpts, STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (an over-eager skip visually stalls the graph)
- **Depends on**: none
- **Category**: perf
- **Planned at**: commit `bcf6600`, 2026-07-20

## Why this matters

`mara_graph`'s node_view renders the graph through a **secondary
`egui::Context`** into an offscreen wgpu texture (for sharp zoom). Today that
sub-render is unconditional: every time the parent repaints — which the
native runner does on every mouse move anywhere in the window
(`mara/src/window.rs:473-476` requests repaint on `CursorMoved`), on the
Bevy viewport's 24fps idle tick, on any cursor blink — each visible graph
re-runs a full egui pass (layout of every node), tessellation, and a GPU
render pass, even when the graph did not change. With N graph editors open
that is N wasted full passes per parent frame. The composite step already
re-blits a retained texture, so a skip path is nearly free.

## Current state

- `crates/modules/graph/src/node_view.rs:628-662` (the unconditional pipeline):

```rust
state.ensure_renderer(backend);
state.ensure_target(backend, size_pixels);
let sub_ctx = state.sub_ctx.clone();
sub_ctx.begin_pass(raw);
#[allow(deprecated)]
{
    egui::CentralPanel::default()
        .frame(egui::Frame::new())
        .show(&sub_ctx, |ui| { body(ui); });
}
let full = sub_ctx.end_pass();
let primitives = sub_ctx.tessellate(full.shapes, sub_ppp);
render_into_target(state, backend, primitives, full.textures_delta, size_pixels, sub_ppp);

// Composite the rendered texture into the parent UI.
if let Some(target) = &state.target
    && let Some(tex_id) = target.parent_tex_id
{
    parent_ui.painter().image(tex_id, rect, ..., Color32::WHITE);
    backend.after_render(&target.texture, tex_id, target.size_pixels);
}
```

- Above this excerpt, the function assembles `raw: RawInput` for the
  sub-context, including forwarded pointer events (there is a
  `raw.events.push(egui::Event::PointerGone);` path at `:618` when the
  pointer leaves) and a zoom animation state (`node_view.rs:536-554` — an
  in-flight zoom animates over frames).
- `end_pass()` returns `FullOutput` whose `viewport_output` carries
  `repaint_delay` — the sub-context's own statement of whether it needs
  another pass (animations inside the graph body, cursor blink in node text
  fields). This is the authoritative "the sub-UI is animating" signal.
- The graph body content is caller-supplied (`body(ui)`), so the module
  cannot hash graph data itself — content changes must be signaled by the
  caller or inferred from forwarded input.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Graph crate | `nix develop --impure -c cargo check -p mara_graph` | exit 0 |
| Check + sealed gates | `nix develop --impure -c make check` | exit 0 |
| Full test suite | `nix develop --impure -c make test-all` | exit 0 |
| Run the demo | `make run` (needs display) | graph view stays interactive |

## Scope

**In scope**:
- `crates/modules/graph/src/node_view.rs` — the show/render path and its
  state struct; a new public `revision(u64)` (or similar) on the node-view
  builder.

**Out of scope**:
- `crates/modules/graph/src/vendored/` — the vendored snarl fork's internals.
- The wgpu renderer (`render_into_target`) and target allocation.
- Parent-side repaint policy (`mara/src/window.rs`) — reducing parent
  repaints is a separate concern; this plan makes graphs cheap under them.

## Git workflow

- Branch from `develop`: `feature/011-nodeview-dirty-check`
- Conventional commits, title only, ≤50 chars, no signature. Suggested:
  `perf(graph): skip clean node_view sub-renders`
- Do NOT push or open a PR unless instructed.

## Steps

### Step 1: Capture the skip-relevant state

In the node-view state struct (the one holding `sub_ctx`/`target`), add a
`last_render: Option<RenderStamp>` where `RenderStamp` records:

- `size_pixels`, `sub_ppp`, `zoom` (bits of the f32), pan/offset if present,
- `caller_revision: u64` (new API — see Step 2),
- `theme/accent stamp` if the sub-render reads Mara style (check whether the
  body uses Mara theming — search the file for `style::`/`theme()`; if the
  graph is vendored-egui-styled only, omit),
- `needs_repaint_after: bool` — whether the LAST `end_pass` requested a
  repaint (from `full.viewport_output` values; treat any finite
  `repaint_delay` ≤ one frame as "animating").

### Step 2: Add the caller revision API

On the node-view builder/entry (`show_with_anchor` or its public wrapper —
identify the public entry: `grep -n 'pub fn' crates/modules/graph/src/node_view.rs`),
add `.revision(u64)`: "bump this whenever graph content changes; unchanged
revision + no input + no animation ⇒ the cached texture is composited
without re-running the graph UI." Default when not called: revision `u64::MAX`
meaning "always dirty" — **opt-in optimization, zero behavior change for
existing callers**.

**Verify**: `nix develop --impure -c cargo check -p mara_graph` → exit 0.

### Step 3: The skip gate

Immediately before `sub_ctx.begin_pass(raw)`, compute `dirty`:

```
dirty = caller_revision changed or is u64::MAX
      || size_pixels/sub_ppp/zoom/pan changed
      || zoom animation in flight (node_view.rs:536-554 state)
      || raw.events is non-empty            // any forwarded input
      || pointer is over `rect`             // hover styling may change
      || last pass requested repaint        // sub-UI animating
      || state.target is None               // first frame / target lost
```

If `!dirty`: skip begin_pass/tessellate/render entirely and fall through to
the existing composite block (it already re-blits `target.parent_tex_id`).
**Keep calling `backend.after_render(...)` in the skip path** only if the
backend requires a per-frame copy (read `after_render`'s contract — the
comment at `:658-661` says Bevy queues a source-to-GpuImage copy "so the
parent UI sees this frame's render"; determine whether the GpuImage persists
across frames — if yes, skip `after_render` too; if it is re-uploaded each
frame from `target.texture`, the copy must still run. Record which in a code
comment).

After a live pass, refresh `last_render`.

**Verify**: `nix develop --impure -c cargo check -p mara_graph` → exit 0.

### Step 4: Instrument and validate by hand

Behind the existing frame-time env-var convention (the repo uses
`MARA_FRAME_TIME` for runner diagnostics — mirror it as `MARA_GRAPH_TRACE`),
`eprintln!` one line per skipped/live pass. Then `make run`:

1. Open the graph view; move the mouse in a *different* pane → trace shows
   skips.
2. Hover/drag nodes, edit, zoom → trace shows live passes, no visual stall.
3. Stop moving entirely → passes stop (parent goes idle).

**Verify**: all three behaviors observed; remove or keep the trace guarded
by the env var (keep — it matches `MARA_FRAME_TIME` precedent).

## Test plan

Unit-testable pieces (module has zero tests today — these are its first;
put them in `node_view.rs`'s new `#[cfg(test)]` module):

- `stamp_equality_detects_zoom_change`, `..._size_change`,
  `..._revision_change`: construct two `RenderStamp`s, assert dirty logic.
- `default_revision_always_dirty`: `u64::MAX` never matches.
The full render path needs a GPU and stays manual (Step 4).

## Done criteria

- [ ] `.revision()` API exists and is doc-commented with the invalidation contract
- [ ] Skip gate implemented; default behavior (no `.revision()` call) unchanged
- [ ] Step 4's three manual observations recorded in the report
- [ ] `nix develop --impure -c make check` / `make test-all` / `make harden` exit 0
- [ ] ≥4 new unit tests pass
- [ ] `plans/README.md` status row updated

## STOP conditions

- `after_render`'s contract makes texture reuse impossible without a
  per-frame copy on the Bevy path AND the copy is the dominant cost — the
  skip then saves little there; report measurements before deciding.
- The sub-context's `repaint_delay` signal proves unreliable (text-cursor
  blink never settles → never skips) — report; consider treating
  cursor-blink as skippable only when the graph lacks focus, but do not
  improvise beyond that without reporting first.
- Any visual stall reproducible in Step 4.2 after a fix attempt — revert to
  always-dirty and report.

## Maintenance notes

- Callers that mutate graph data **must** bump `.revision()` or they keep
  paying the old cost (safe default) — document in the crate root docs.
- If plan 013 later makes theme changes observable via a generation counter,
  add that generation to `RenderStamp` and remove any theme-stamp hack.
- Reviewer: the dirty conditions are a disjunction — scrutinize each removal
  someone proposes later; every term guards a real invalidation source
  listed in Current state.
