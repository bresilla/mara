# ADR 0001 — The recursive view tree and three-tier chrome

Status: Accepted (2026-07-22)

## Context

A tab's content today is one of two *different* shapes — a bare leaf
`MaraView`, or a `MultiView` of child views — and almost all chrome (panes,
ribbons, backdrop painter, fullscreen) anchors to the **whole window**
regardless of where a view actually sits. Only `ViewCtx::content_rect`,
`screen_rect`, and `body` honor a scoped rect (`explicit_content`). Shelves are
published app-wide, and ribbons anchor to window edges even when scoped to a
view. This makes "a view is a self-contained region" false, and makes tiling
(MultiView) a special case rather than the norm.

## Decision

**1. One recursive content node.** Replace the leaf-vs-`MultiView` duality with
`ViewNode = Leaf(Box<dyn ViewContent>) | Split { layout, children }`. `Split` is
pure structure (a folder — divides a rect, draws nothing); every module is a
`Leaf` (a file — canvas, image, map, graph, code, board, three_d). There is no
module categorization: any module can be the whole tab (the sole leaf under the
root `Split`) or one cell deep. The tab root is **always** a `Split`.

**2. Every node is a fully-scoped region.** A node renders into a rect, and
*everything* it does — body, panes, painter, input, fullscreen, ribbons — scopes
to that rect. `ViewCtx` carries a `region: Rect` (always set) instead of an
`Option`, and every method honors it.

**3. Chrome has three scope tiers.**
- **Top bar** — window frame, always present (menu / maximize / close / tab
  switcher + the active tab's own buttons). The only always-on chrome.
- **Shelves** — *per tab*. Docked panels of content (containers → pods →
  widgets) that reserve edge space and frame the tab's view-tree region.
  Re-scoped from app/window level; **not deleted**.
- **Ribbons** — *per view*. Thin icon-button strips on left/right/bottom, one
  set per leaf, inside the leaf's rect. Re-scoped from window edges. Top edge is
  reserved for the shell bar; it is not a per-view ribbon.

The space narrows in stages: window → minus top bar → minus shelves → view-tree
→ split into leaves → minus each leaf's ribbons → leaf content.

## Consequences

- MultiView stops being a special case; single-view and tiled tabs are the same
  code path (a `Split` with one vs many children).
- Panes, painter, input, and fullscreen become per-node instead of per-window,
  which is what lets a leaf be a self-contained app inside its cell.
- Shelves and ribbons keep their kind but change scope; persisted UI state may
  reset (greenfield).
- GPU / whole-window leaves (`three_d`, `bevy`) still own the window and cannot
  tile into a cell yet — the one real per-module limitation, tracked as future
  work, not a design rule.

See `PLAN.md` (repo root) for the phased implementation.
