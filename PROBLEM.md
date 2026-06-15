# PROBLEM.md — gaps found integrating a real consumer (mara 0.3.0)

> **STATUS: all three resolved.** See the per-item "Resolution" notes.
> - #1 resize tracking → core renderer now derives chrome bounds fresh
>   each pass (`shelf_layout.viewport ?? content_rect()`, no stale
>   self-reference) **and** `RibbonPlugin` auto-publishes a full-window
>   baseline each frame (order-independent via
>   `shelf_layout_published_this_pass`).
> - #2 window chrome → split into two concerns. The **permanent top bar
>   is UI, enforced on every platform** (`MaraShellPlugin`, always added
>   by `MaraPlugin`; opt out with `MaraTopBar { enabled: false }`). The
>   **native frame is capability-driven, opt-in per host**:
>   `MaraWindowChromePlugin` now forces `decorations = false` itself and
>   advertises maximize/close. Web/android don't add it → the same bar
>   drops the window buttons and reflows. (The earlier `MaraDesktopPlugin`
>   bundle was renamed/removed — "desktop" was the wrong axis.)
> - #3 version pins → the missed `mara/Cargo.toml` `mara_core = "0.2.0"`
>   pin is bumped to `0.3.0`.


Notes from porting an external Bevy app (`bevy_openusd` / usdview — a USD
viewer) onto **mara 0.3.0** as a normal consumer (path dep on `bevy_mara`
+ `mara_core`). The port works, but three things bit the consumer that
mara could **enforce / own** instead of silently leaving to the host.

Each item below has: the symptom, the root cause (with file:line in
mara), and options for how mara could make the right thing happen by
default.

---

## 1. Responsive layout silently breaks if the host forgets `publish_shelf_layout`

**Symptom.** The left ribbon rail (and the rail-anchored panes) do **not
track window resize** — grow/shrink the OS window and the rail + icons
stay put instead of following the edge / reflowing.

**Root cause.** The ribbon paints against a viewport that the *host* is
expected to publish every frame, but nothing requires or defaults it:

- The ribbon renderer reads the screen rect at
  `crates/core/src/ribbon/slot_paint.rs:611` (`let screen = ctx.content_rect();`),
  and the responsive reflow / pane-avoidance path consults the host-published
  `ShelfLayout`.
- The host publishes it via `mara_core::publish_shelf_layout(ctx, ShelfLayout { viewport: ctx.content_rect(), .. })`.
  The reference app does this **every frame** — `example/src/app.rs:1930`
  (inside `ui_system`).
- **`MaraPlugin` / `RibbonPlugin` never publish it.** A Bevy consumer that
  draws panes via `Pane::show` + `draw_slot_ribbons_featureful` but doesn't
  replicate the example's `publish_shelf_layout` call gets a stale/default
  viewport. There is **no panic and no warning** — just wrong layout. (By
  contrast, `Pane::show` *does* hard-panic when `publish_ribbon_pane_ids`
  is missing — see `crates/core/src/pane/mod.rs:238` — so the two
  "host must publish X each frame" contracts are inconsistent: one panics,
  one degrades silently.)

**How mara could enforce it**

- **(A, recommended) Auto-publish from the Bevy plugin.** Have
  `RibbonPlugin` (or `MaraPlugin`) add a system in `EguiPrimaryContextPass`,
  ordered before ribbon paint, that calls
  `publish_shelf_layout(ctx, ShelfLayout { viewport: ctx.content_rect(), .. })`
  using the primary egui context. Hosts that genuinely reserve shelf
  regions can still override later in the frame; the default just makes
  resize correct for free.
- **(C, fallback) Live fallback in the renderer.** If no `ShelfLayout`
  was published this frame, have the ribbon/pane layout fall back to
  `ctx.content_rect()` live instead of a stale value — so forgetting the
  call still yields a tracking layout.
- **(Minimum) Make the contract loud.** If neither of the above, at least
  `warn!` once when ribbons/panes paint without a published layout this
  frame, matching the `publish_ribbon_pane_ids` panic's intent.

The asymmetry is the real footgun: `publish_ribbon_pane_ids` is mandatory
(panics) but `publish_shelf_layout` is "optional" (silent wrong layout),
even though both are per-frame host contracts the consumer can't discover
without reading the example line-by-line.

**Resolution (done).** Root cause was narrower and nastier than "host forgot
to publish": the featureful renderer (`draw_unified_ribbon_chrome`) *read back*
the `chrome_bounds_key` it also *writes*, so after the first frame it
re-consumed its own stale value and froze the bounds at the initial window
size — side ribbons/panes stopped tracking resize even for hosts that did
nothing wrong. Fixes:
- **Core (host-agnostic):** the renderer now derives chrome bounds **fresh**
  each pass via `fresh_chrome_bounds` = `shelf_layout(ctx).viewport ??
  ctx.content_rect()` — never from the self-published key. Regression tests in
  `crates/core/src/ribbon/chrome.rs`.
- **Bevy (correct-by-default):** `RibbonPlugin` adds
  `auto_publish_shelf_layout_system`, which publishes
  `ShelfLayout::full(content_rect())` each frame *only if the app didn't
  publish this pass* (`mara_core::shelf_layout_published_this_pass`), so it
  never clobbers real shelf reservations regardless of system order.

---

## 2. mara does not own / enforce window chrome — OS decorations leak

**Symptom.** The app shows the **OS title bar**. The consumer expected
"if I use mara, mara owns the window chrome and there are no native
decorations."

**Root cause.** `MaraPlugin` is `ThemePlugin` + `RibbonPlugin` +
`EguiInputAbsorbPlugin` + `NodeViewPlugin` (`mara/plugin/bevy/src/lib.rs`) —
**it never touches the `Window`**. Borderless chrome is fully opt-in and
takes *two* separate, easy-to-forget steps, and the chrome plugin itself
explicitly defers the decision to the app:

- `mara/plugin/bevy/src/window_chrome.rs` doc:
  > "Apps still decide whether the OS window uses native decorations. This
  > plugin is intended for windows with `Window::decorations = false`."
- So to get mara's borderless chrome the host must **both** (1) set
  `Window { decorations: false, .. }` in `WindowPlugin`, **and** (2) add
  `MaraWindowChromePlugin`. Do neither (or only one) and you get the OS
  title bar with no warning. The reference Bevy host (`example/src/bevy.rs`)
  doesn't do either, so there's no copy-paste path that "just works."

**How mara could enforce it**

- **(A, recommended) An all-in-one desktop plugin.** Ship a
  `MaraDesktopPlugin` (or a flag on `MaraPlugin`) that = `MaraPlugin` +
  `MaraWindowChromePlugin` + a startup system that forces
  `Window.decorations = false` on the primary window. One add → mara owns
  the chrome, no native decorations, move/resize wired. Keep the granular
  plugins for hosts that want OS decorations on purpose.
- **(B) Make `MaraWindowChromePlugin` own the decoration.** Drive it from
  `MaraWindowChromeSettings` (already exists) so that when enabled it sets
  `decorations = false` itself instead of documenting "the app must do it."
- **(Minimum) Startup mismatch warning.** `warn!` at startup if
  `MaraWindowChromePlugin` is present but `Window.decorations == true`
  (chrome will double up), or if the theme is installed but decorations are
  on (likely-unintended OS title bar over a glass UI).

The doc-comment "apps still decide" is a deliberate choice, but it means
the *default* experience of "add MaraPlugin" is an OS-decorated window —
the opposite of what a glass editor-shell consumer expects.

**Resolution (done) — but split along the right axis.** "Desktop" was the wrong
framing: the *bar* is cross-platform UI and the *frame* is capability-driven, so
they were separated.

- **Bar = enforced, cross-platform, lives in `mara_core`.** The bar
  config+render is `mara_core::shell::ShellBar` (host-neutral); each host
  adapter invokes `ShellBar::show` once a frame, so it is identical and present
  everywhere. Bevy: `MaraShellPlugin`, always installed by `MaraPlugin`
  (desktop/web/android). eframe: the `mara::window` runner renders it via the
  new `WindowApp::configure_shell` / `on_shell_event`. A host with no adapter
  (raw `eframe::App` web) calls `ShellBar::show` itself. Window events
  (`CloseRequested`/`MaximizeToggleRequested`) are handled by the adapter; the
  rest are `ShellEvent`s for the app. Configure via the `ShellBar`
  resource/value (`app_menu`, `views`, `active`); opt out with
  `ShellBar { enabled: false }`. **The reference demo now dogfoods it on all
  three of its bins** (native/web/bevy) — the old hand-rolled persistent-top
  view switcher was removed.
- **Frame = opt-in per host, owns the decoration.** `MaraWindowChromePlugin`
  now forces `Window.decorations = false` itself (option B) and advertises
  `system_maximize` / `system_close`, gated by `MaraWindowChromeSettings`. Web
  and android just don't add it → no capabilities advertised → the same bar
  drops the window buttons and reflows. No per-platform branching in app code.

Copy-paste paths:

```rust
// Desktop-native: enforced bar + borderless frame + working controls.
App::new()
    .add_plugins(DefaultPlugins)
    .add_plugins(bevy_egui::EguiPlugin::default())
    .add_plugins(bevy_mara::MaraPlugin)                 // bar (all platforms)
    .add_plugins(bevy_mara::MaraWindowChromePlugin)     // native frame (desktop)
    .run();

// Web / android: identical minus the frame plugin — bar adapts automatically.
App::new()
    .add_plugins(DefaultPlugins)
    .add_plugins(bevy_egui::EguiPlugin::default())
    .add_plugins(bevy_mara::MaraPlugin)                 // bar, no window buttons
    .run();
```

---

## 3. (minor) Release hygiene — stale internal version pins

Building the consumer against the `0.3.0` working tree failed to resolve:
`bevy_mara 0.3.0` requires `mara_bevy ^0.2.0`, but every internal crate is
already `0.3.0`. The workspace dep table in `Cargo.toml` still pinned all
members at `version = "0.2.0"` (8 lines: `mara_core`, `mara_graph`,
`mara_code`, `mara_image`, `mara_canvas`, `mara_map`, `mara_3d`,
`mara_bevy`) while `workspace.package.version` was bumped to `0.3.0`.

Bumping those eight pins to `0.3.0` fixes it. Worth a `cargo-release` /
CI check that internal `path + version` pins match
`workspace.package.version`, so a release-prep bump can't half-land.

**Resolution (partial).** The eight workspace-table pins were already bumped to
`0.3.0`, but the bump missed one member manifest: `mara/Cargo.toml` still pinned
`mara_core = { version = "0.2.0" }`, which broke `cargo test` resolution. Bumped
to `0.3.0`. The CI/`cargo-release` assert that internal pins == workspace
version is still worth adding (not done here) to prevent the next half-landed
bump.

---

### TL;DR for enforcement

| Gap | Today | Make it correct-by-default | Status |
|---|---|---|---|
| Resize tracking | host must call `publish_shelf_layout` every frame, else silent wrong layout | auto-publish in `RibbonPlugin` + live-fallback to `content_rect()` in renderer | ✅ done (both) |
| Window chrome | OS decorations on by default; borderless = 2 manual opt-in steps | split: enforced cross-platform bar (`MaraShellPlugin` in `MaraPlugin`) + opt-in `MaraWindowChromePlugin` that owns `decorations:false` + advertises controls | ✅ done |
| Version pins | stale `0.2.0` pins broke resolve | CI assert internal pins == workspace version | ⚠️ pin fixed; CI assert still TODO |
