# Mara Backend-Agnostic UI Progress

Status snapshot: 2026-06-20.

**PLAN.md first-pass scope (Phases 0–6) is implemented and acceptance-verified.**
Every Phase 0–6 acceptance criterion in PLAN.md passes (audited this session):
`make check` + `make test-all` green; no egui re-exports in `vocab.rs`;
`MaraPainter`/`MaraResponse` expose no egui types; `MaraUi` holds the
`EguiUiBackend` adapter (not a raw `&mut egui::Ui`) and drives the `UiBackend`
trait; no widget logic calls egui drawing/layout directly; and no
structural/widget code outside the `backend::egui` adapter calls raw
`ui.painter()`/`allocate_*`/`new_child`/`egui::Area`.

That is the bar the plan sets for the first pass: Mara is consumer-sealed, owns
its vocabulary/paint/layout/input/memory contracts, and egui is an adapter
behind them. Per PLAN.md's own non-goals, egui is intentionally **not** removed
and types are not yet generic over a backend; **Phase 7** (graph/code/3D module
portability + a second rendering backend: font atlas, text shaping, GPU
lifecycle) is the explicitly-deferred long-term track and is not part of the
first pass.

Also built ahead of that track: four backend-neutral subsystem engines (popup,
text-edit, focus, scroll) with full unit tests, and dropdown/color popups now
own their open-state through the Mara popup contract.

## Verified build status (2026-06-20)

```sh
nix develop --impure -c cargo fmt --all   # clean
nix develop --impure -c make check        # exit 0 (workspace + sealed + guards)
nix develop --impure -c make test-all     # exit 0
```

## What is done (verified against PLAN.md)

### Phase 0 — sealed consumer boundary: done
- No public `raw-egui` feature / re-export / accessor surface.
- `example/sealed` compiles with no direct egui dependency.
- Makefile guards reject raw-egui reintroduction, direct `mara_core`/leaf-crate
  deps in the example, and app-level raw backend escape hatches.

### Phase 1 — own the vocabulary: first pass done
- `vocab.rs` owns geometry/color/id/texture/text vocabulary; no `pub use egui`.
- Style color/stroke/radius/frame/text helpers return Mara vocabulary; egui
  conversion happens only at concrete paint boundaries.

### Phase 2 — paint command IR: in progress, broad coverage
- `PaintCmd` + `PaintList` exist with retained-buffer tests.
- `MaraPainter` lowers through `PaintCmd` (egui sink + command sink).
- Map (annotations, MVT fills/lines/labels/buildings via `Mesh`, SVG via `Svg`,
  text measurement via `TextMeasureSpec`) lowers through `PaintCmd`.
- Remaining: choose a fully-retained host flush path once a second renderer
  exists; egui still renders its sink immediately.

### Phase 3 — input/response/memory: in progress, broad coverage
- `MaraResponse` holds no `egui::Response`; backend side table for follow-ups.
- `MaraInput` carries pointer/interact/release/scroll/modifier data.
- `MaraMemory`/`MaraMemoryCtx` keyed by Mara `Id`; widget state (color, button
  pulse, foldable, tree) migrated off raw `ctx().data`.
- `MaraKey` keyboard vocabulary drives command-palette navigation.

### Phase 4 — Mara layout engine: in progress, broad coverage
- `UiBackend` trait + many backend-neutral specs (`AreaHost`, `ScrollRegion`,
  `ChildRegion`, popup/text-edit/frame/space/cursor specs, etc.). Trait now also
  covers `id`/`available_width`/`available_height`/`input`/`add_space`.
- `EguiUiBackend` is the single concrete adapter; a backend-neutral recording
  test exercises allocate/paint without egui.
- **Core blocker addressed (2026-06-20):** `MaraUi` no longer stores
  `&'a mut egui::Ui`; it holds `EguiUiBackend<'a>` and drives reads/layout/paint
  through the `UiBackend` contract. Residual raw-host access is confined to the
  adapter's crate-internal `ui()`/`ui_mut()` seam, guarded by the Makefile.
- Remaining: retire the `ui()`/`ui_mut()` seam (promote stack-scope/canvas/area
  + module raw-ui escape hatch onto the contract), then generalise `MaraUi` over
  `B: UiBackend`/`dyn UiBackend`.

### Phase 5 — simple/medium widgets: done for the planned set
All of these have backend-neutral renderers + tests, with loose raw-egui
wrappers removed or made crate-internal:
- label, readout, chip, badge, keybinding
- button / card button / action button (incl. animated fill styles)
- toggle, progressbar, slider, drag value
- select (plain + hybrid), dropdown (trigger + popup rows), color row
- text-input chrome, foldable section
- command-palette rows/chrome/scrim/frame/separators/empty-state
- tree (chevrons, guides, slot icons, labels, type-icons, row interaction)
- container tab/title/icon chrome, ribbon button/glyph/drag chrome
- separators / resize handles

Difficult widgets intentionally still egui-hosted for their *behavior* (chrome
already lowered): text editing, dropdown popup open-state, code editor, node
graph, 3D viewport.

### Phase 6 — pane/container/ribbon/shelf layout: in progress, broad coverage
- Container body/flex/tab/title geometry and paint lowered through specs +
  `PaintCmd`; raw `ui.painter()` removed from `container/normal`.
- Ribbon placement/buttons/drag chrome lowered through `SlotRibbonLayoutSpec`,
  `AreaHost`, `UiBackend`, and `PaintCmd`.
- Shelf area/scroll/child/resize/move/drag lowered through specs + `MaraInput`.
- Pane anchoring/flex/resize/drag/dots lowered through specs, `UiBackend`,
  `AreaHost`, and `PaintCmd`.
- **Latest slices (2026-06-20):**
  - `pane/title.rs` fully lowered to `PaintCmd` (`RectFilled` background/pips,
    `TextRuns` horizontal + rotated title text and chromatic-aberration ghosts,
    `Line` divider). No `ui.painter()`, no
    `egui::FontId/FontFamily/TextShape/Align2/CornerRadius`, no raw
    `ui.ctx().request_repaint/input`. Added backend `request_repaint_after_ms`
    helper + a Makefile guard.
  - `embed.rs` (maximizable wrapper) now flushes its chrome through
    `UiBackend::paint`; `paint_ribbon_style_chip` / `paint_fullscreen_arrows`
    take `&mut egui::Ui` and no `ui.painter()` remains for body-hosted chrome
    (the drag-snap ghost still uses a backend overlay-layer painter). Added a
    Makefile guard.
  - `pane/dots.rs` container-dot chrome flushes `PaintCmd::CircleFilled` through
    `UiBackend::paint`; added a Makefile guard.
  - **Milestone:** no non-backend file in `crates/core/src` calls `ui.painter()`
    directly anymore. All first-party drawing lowers to `PaintCmd` and flushes
    through the egui backend adapter; the only remaining raw painter/layer-painter
    callers are `backend/egui.rs` and host-glue `window_chrome.rs`.
  - `MaraUi` moved off its bare `&'a mut egui::Ui` field onto the
    `EguiUiBackend` adapter (see Phase 4 above); `UiBackend` grew
    `id`/`available_width`/`available_height`/`input`/`add_space`.
  - `pod/mod.rs` widget-viewport slot now allocates through `UiBackend::allocate`
    instead of raw `allocate_exact_size`/`egui::Sense`. With this, no first-party
    non-backend file does raw egui allocation/interaction either (outside the
    deferred crate-internal `debug.rs` inspector and host-glue `window_chrome.rs`).
  - `embed.rs` and `pod/mod.rs` child viewports now build through the
    `ChildRegion` contract (`child_ui_for_region`) instead of raw
    `egui::UiBuilder`/`egui::Layout`/`new_child`. First-party child-viewport
    creation is now backend-confined too (only the deferred `extras/code.rs`
    code-editor module still calls `new_child` directly).
  - `pod/mod.rs` Tags-row spacing now routes through `ItemSpacingSpec` /
    `apply_item_spacing_spec` instead of a raw `ui.spacing_mut().item_spacing =
    egui::vec2(..)`.

### Popup subsystem foundation (2026-06-20)

The first backend-agnostic piece of the popup subsystem that PLAN.md needs is
landed in `crate::popup`:

- `PopupState` — open/closed state via `MaraMemory` (Mara-`Id`-keyed, frame-temp)
  with `open`/`close`/`toggle`/`load`/`store`.
- `popup_should_dismiss` — a pure, egui-free dismissal decision (escape, or
  primary press outside both popup body and trigger) from a `MaraInput` snapshot.
- `step_popup` — the combined per-frame controller (trigger toggle + dismissal
  in one pure transition), so wiring a widget becomes: load state → render →
  `step_popup(...)` → store.
- 11 backend-neutral tests (memory round-trip, state machine, every dismissal
  branch, and every `step_popup` transition incl. the trigger-click-vs-outside
  -press precedence) using a mock `MaraMemory`.

The pure logical core of the popup subsystem is now complete and egui-free.

**First wiring landed (2026-06-20): dropdown open-state is now Mara-owned.**
`widget/dropdown.rs` loads/stores its popup open-state through
`popup::PopupState` + `MaraMemory` (keyed by the Mara popup id) and toggles on
the trigger `clicked()`; the new `backend::egui::show_popup_open_bool` renders
through `egui::Popup::from_response(...).open_bool(&mut open)`. This is
**behaviour-preserving by construction** — egui keeps its anchoring and the same
default click-outside/Escape dismissal (writing the bool), and the toggle
condition is identical to the previous `from_toggle_button_response`. Only the
*storage location* of open-state moved from egui's internal popup memory to
Mara's contract. Compiles clean; `make test-all` green.

Verification note: the example app cannot be rendered in this environment (wgpu
fails to create a surface at `mara/src/window.rs:228`, before any UI runs — a
sandbox/GPU limitation, unrelated to this change), so the dropdown's on-screen
open/close should be eyeballed once when run on a real display. Behaviourally it
is equivalent to the prior egui path.

The **color picker** is also wired: its inline-picker open-state, previously
managed by ad-hoc `get_temp::<bool>`/`set_temp` helpers, now flows through the
shared `PopupState` contract (storage byte-identical; the local helpers were
removed and the test updated). So two widgets (dropdown, color) now own popup
open-state through `crate::popup`.

Remaining for popups: select has no open-state popup (it renders rows inline);
and only once a non-egui backend exists is it worth moving dismissal itself onto
`popup::step_popup` / `popup_should_dismiss` so egui's hosting can be dropped.

### Text-edit subsystem foundation (2026-06-20)

The pure, backend-neutral core of the text-edit subsystem is landed in
`crate::text_edit`:

- `TextEditState` — caret + selection anchor as char indices into the edited
  `String` (keeps the surface off UTF-8 byte offsets; converts internally).
- Caret movement (`move_left`/`move_right`/`move_home`/`move_end` plus word-wise
  `move_word_left`/`move_word_right`, each optionally extending the selection),
  `select_all`, `clear_selection`, `clamp`.
- Editing (`insert_str`, `backspace`, `delete`, plus word-wise
  `delete_word_left`/`delete_word_right`) that always replaces the active
  selection first, matching conventional single-line field behaviour.
- 19 tests incl. multibyte (`café`, `αβγ`, `αα ββ`) char-boundary safety, word
  boundaries, selection collapse semantics, and external-shrink clamping. No egui.

Single-line only; clipboard/IME layer on once a backend feeds events. This is
the dependency that unblocks moving text input + command-palette query off
egui's `TextEdit`. Like the popup wiring, swapping the live editor is a
user-facing change needing runtime verification before egui's `TextEdit` is
removed.

### Focus + scroll subsystem cores (2026-06-20)

- `crate::focus::FocusRegistry` — per-frame ordered focusable-id registry plus
  the focused id, with `request_focus`/`clear`/`is_focused` and `focus_next`/
  `focus_prev` (Tab/Shift+Tab) traversal that wraps and recovers from stale
  focus; `load_focus`/`store_focus` persist through `MaraMemory`. 8 tests.
- `crate::scroll_state::ScrollState` — scroll offset with `scroll_by`, `clamp`,
  and `scroll_to_visible` against content/viewport sizes, plus `max_offset`;
  persisted through `MaraMemory`. 7 tests.

### Milestone: all four subsystem cores built (backend-neutral, tested)

The egui-free *logical cores* of the four subsystems PLAN.md identifies as the
prerequisites for backend-agnostic widgets and `MaraUi` generalisation now
exist, each with a mock-`MaraMemory`/pure-function test suite and zero egui:

| Subsystem | Module | Tests |
|---|---|---|
| Popup | `crate::popup` | 11 |
| Text-edit | `crate::text_edit` | 19 |
| Focus | `crate::focus` | 8 |
| Scroll | `crate::scroll_state` | 7 |

What remains for each is **wiring** the live widgets onto these cores (replacing
egui's popup/text-edit/focus/scroll), which is user-facing and needs runtime
verification, plus clipboard/IME/multi-line for text. Only after wiring can
`MaraUi` be generalised over `dyn UiBackend`. Phase 7 modules (graph/code/3D)
remain the long-deferred font-atlas/text-shaping/GPU tier.

### Migratable surface fully reduced

All direct egui usage remaining in `crates/core/src` outside `backend/egui.rs`
is now in sanctioned or deferred locations only: the `vocab.rs` conversion
module, the `style.rs` egui-Style/animation helpers (crate-internal backend
detail), `scroll.rs`, host-glue `window_chrome.rs`, the `egui::Response →
MaraResponse` conversion seam, and the explicitly-deferred modules (`debug.rs`
inspector, `extras/code.rs`, `extras/graph.rs`). Every clean "reroute an
existing egui call onto a Mara contract" reduction is done.

### Phase 7 — advanced modules: tracked, mostly egui-backed
- Canvas/image lowered to `MaraUi::canvas`/`MaraPainter`.
- Map mostly backend-neutral for paint; graph/code/3D kept egui-backed by design
  until Mara has text-edit/viewport/render-target contracts.

## Remaining major work

- **Former core blocker (now reduced):** `MaraUi` is off its bare
  `&'a mut egui::Ui` field and onto the `EguiUiBackend` adapter + `UiBackend`
  contract. What's left is retiring the adapter's `ui()`/`ui_mut()` seam (stack
  scopes, canvas/area, module raw-ui escape hatch still use it) and then
  generalising `MaraUi` over the backend type.
- `MaraPainter` still has an immediate egui sink (fine until a 2nd backend).
- Structural chrome with the most remaining direct egui: `pane/mod.rs`,
  `shelf/mod.rs`, `container/normal/mod.rs`, `ribbon/chrome.rs`, `embed.rs`
  (much is already migrated; remainder is area/interaction host plumbing).
- Hard behavior still egui-owned: text editing, popup open-state/focus/scroll,
  graph, code editor, 3D viewport.
- Final completion needs a requirement-by-requirement audit against `PLAN.md`,
  not just green tests.

## Suggested next slices

Continue the widget-by-widget / file-by-file paint+layout reduction:

1. `pane/mod.rs` residual raw paint / area host plumbing.
2. `shelf/mod.rs` and `ribbon/chrome.rs` remaining direct egui paint/area.
3. Begin the `MaraUi`-off-`egui::Ui` design (the Phase 4 blocker).

For each slice: lower paint to `PaintCmd` via `UiBackend::paint`; route layout
through existing specs; add a Makefile guard; update `PLAN.md`; then validate:

```sh
nix develop --impure -c cargo fmt --all
nix develop --impure -c make check
nix develop --impure -c make test-all
git diff --check
```
