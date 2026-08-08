# PLAN_NODE — grouping, subgraphs and visual richness for `mara_graph`

Status: **P1–P11 implemented.** Written 2026-08-03.

| phase | state |
|---|---|
| P1 · node shadow + drag lift | ✅ done |
| P2 · seam prep, serde, primitives | ✅ done |
| P3 · stable identity + wire index | ✅ done |
| P3b · headless input harness | ⚠️ mostly — see below |
| P4 · frames, model | ✅ done |
| P5 · frames, rendering + interaction | ✅ done |
| P6 · chrome, LOD, camera spring | ✅ done |
| P7 · GraphDoc, ports, nav, `show_doc` | ✅ done |
| P8 · collapse + expand | ✅ done |
| P9 · subgraph visuals, portal dive | ✅ done |
| P10 · wire order, hover focus | ⚠️ partial — see below |
| P11 · culling, node resize | ✅ done |

Landed so far: `--features serde` compiles for the first time and is a
permanent `make check` gate; `NodeViewer` is at **zero** egui references
(`has_node_style`/`apply_node_style` deleted, `draw_background` on
`MaraPainter`); `NodeState` moved off the raw egui context onto Mara
memory; the always-broken `get_selected_nodes` replaced by
`GraphState::selection`; `mara_graph::prelude` closes the two-export-list
drift. `make check` and the full workspace test suite are green.

**Frames work.** `frames.rs` (model) and `ui/frame_paint.rs` (render +
interaction) are both at zero egui references and are held there by a
`make check` grep. Title-band drag moves members; the body stays
click-through so canvas panning survives; adoption resolves on drag stop
and releases to the *parent* frame rather than to root; eight resize
handles keep a constant screen size across zoom; collapsed frames skip
their members in the node loop entirely; nesting alternates lightness per
depth. `F` groups the selection, nesting inside whatever frame the
selection already shares. 24 tests (18 model, 6 interaction).

Not done from P5's spec: inline rename on create (needs in-canvas text
editing) and `GraphStyle::frame_style` (the box currently themes off
`FrameRole::Group` with fixed proportions).

**The genericity seam is in.** `chrome.rs` carries `NodeChrome`,
`WireFx`, `Status`, `Badge`, `NodeShape`, `DetailTier`, `Routing`,
`Flow` — every enum `#[non_exhaustive]` from the first commit — plus the
fixed 8-colour OKLCH palette. `NodeViewer` gained `node_chrome`,
`wire_fx`, `decorate_node` and `frame_spec`, all defaulted. The layered
selection halo and header accent bar deferred from P1 now land here,
driven by `NodeChrome::accent`.

`camera.rs` and `ui/lod.rs` are pure and unit-tested (14 tests). The
camera eases zoom in **log2** space, pinned by a test that runs a
1×→16× zoom beside a 0→4 pan and asserts `log2(scale)` tracks the
translation step for step; a companion test asserts the raw scale does
*not*. Both are wired in: `GraphState::fly_to` sets a target, the
renderer steps the spring and requests repaints only while it moves.

The LOD test counts **viewer calls, not geometry** — egui's tessellator
already drops fully-clipped shapes, so a geometry assertion would pass
with no LOD code at all. It asserts the viewer is never asked about node
bodies at blob tier.

**The subgraph model is in.** `subgraph.rs` and `nav.rs`, both at zero
egui references: `GraphDoc`, `GraphDef`, `Ports` with stable `PortId`s
separate from presentation order, `NodeFactory`, `NodePath`, breadcrumbs,
`iface_snapshot`, recursion guards, a depth cap returning a typed error,
and the pure `derive_ports`. 22 tests.

`instance_count` walks rather than caching. A cache would need
invalidating on six mutation paths, and holding one behind a `RefCell`
would make the whole document `!Sync` — which matters, because Mara
state types live in Bevy resources.

Found while testing: `remove_node` left stale entries in `ext.instances`,
so a `NodePath` through a *deleted* instance still resolved — navigating
into a level that no longer exists would have silently succeeded. Fixed
at the source, plus a `repair` sweep for documents that lose nodes
without going through `remove_node`.

**`show_doc` and collapse/expand are in.** `show_doc` salts every id by
level — sublayer, `GraphState`, node state and the wire cache together —
so entering a definition does not inherit the parent's camera, selection
or per-node size cache. A `DocViewer` wrapper answers an instance's pin
count from the definition's interface, so the app is never asked about a
node it did not create; a test with a viewer that would answer *wrongly*
asserts it is never consulted. Stale paths prune to the deepest level
that still resolves rather than failing to render.

`collapse` snapshots the boundary wires **before** any mutation —
`remove_node` drops every wire incident to the node it removes, and
those are exactly the ones that must reach the new instance. `expand` is
the inverse and ships in the same commit, returning a `UidRemap` so an
app can migrate per-instance state that expanding would otherwise
orphan. `make_local_copy` is the escape hatch from sharing. 14 tests,
headed by `collapse_then_expand_restores_the_exact_wire_set`.

**Fixed a pre-existing flaky test** in `mara_backend_egui` while using
`make test-all` as a gate: `core_reproduces_the_accent_the_theme_adapts`
asserted `theme_accent()` matched the accent *it* had just applied, but
the accent is process-global and a concurrent test overwrote it about
one run in ten. It now reads `raw_accent()` and asserts the invariant
that actually matters — `theme_accent()` is the theme's adaptation of
whatever is stored. 20/20 clean afterwards.

**P9–P11 landed.** Instances paint a deck of cards behind them and a
`×N` badge when a definition is shared — the sharing has to be *visible*
or editing one chip silently changes seven others. `dive_target` and
`settled_target` are pure and tested; entering a subgraph seeds the
child level's camera at the block's own screen rect and aims it at the
interior, so the spring turns it into a dive rather than a cut.

Wire paint order is now sorted. That was a real defect: the wire set is
a `HashSet`, so overlapping wires swapped z-position frame to frame.
Hover focus dims everything outside the pointed-at node's one-hop
neighbourhood — one BFS and some colour arithmetic, no extra geometry.

Viewport culling skips nodes *before* `draw_node`, so an off-screen node
costs no viewer calls, no pin construction and no layout — proven by
counting viewer calls over a 200-node graph, with a complement test
asserting on-screen nodes are still drawn. `size_override` finally has
an owner and behaves as a floor, not a cap: a node told to be 1×1 must
still fit its own content.

**Not done from P10:** mesh-gradient wires and the feathered-glow
replacement for the multi-pass bloom. `Color32::lerp` and `PaintCmd::Mesh`
are both in place, so the remaining work is the triangle-strip builder in
`wire.rs` and its arclength table. The existing multi-pass glow still
renders; this is a quality and performance upgrade, not a gap in
function. `Routing::{Orthogonal, Subway}` are declared but not yet
consumed by the router — `WireStyle::AxisAligned` already provides
right-angle wires today.

**Open issue blocking the enter gesture's test coverage.** The P3b harness drives frames, pointer motion,
drags and keys — a synthesised drag moves a node, which is the mechanism
P5 needs. Synthesised **clicks do not reach the graph widget**: the press
frame shows `is_pointer_button_down_on() == true` on the node's own
response, and the release then produces no `clicked()`. Not the harness's
fault — the identical sequence yields a click against a bare
`ui.interact(.., Sense::click_and_drag())` *and* against a widget inside a
transformed sublayer, which is how the graph draws nodes. So the release
is lost inside the graph's own interaction stack. Two self-tests are
`#[ignore]`d with the findings recorded on them. This must be solved
before P8, whose enter-a-subgraph gesture is a double click.

One correction to §6 while implementing: `GraphState::selection` has to
convert `vocab::Id -> egui::Id` exactly as `GraphWidget::id` does.
`mara_core` documents those two conversions as **not inverses**, so
"set id X, read state back with X" silently read a key nothing wrote —
a quieter instance of the same defect as the old `get_selected_nodes`.

This plan covers two grouping features and a visual overhaul for the node
editor in `crates/modules/graph`. It is deliberately separate from `PLAN.md`,
which tracks the egui-seal closure (WS-A..WS-G). The two interact: every phase
here must respect WS-D1's constraint that new code speaks `mara_core`'s sealed
API and adds no `egui::` references. Two phases below actively *reduce* the
crate's egui surface, so this plan advances WS-D1 rather than competing with it.

---

## 1 · What is being built

**Frame groups** — a named, coloured box drawn behind a set of nodes. Dragging
its title bar moves every member. Purely organisational: no pins, no effect on
dataflow. Blender frames, Unreal comment boxes, ComfyUI groups.

**Subgraphs** — collapse a selection into one block on the parent canvas, with
input/output pins mapped to interior pins. Double-click enters it. Arbitrary
nesting. Blender node groups, Unreal collapse-to-function, Houdini subnets,
Logisim subcircuits.

**Visual richness** — shadows, layered selection halos, mesh-gradient wires,
hover focus/dim, a zoom level-of-detail ladder, a camera spring, portal-dive
navigation.

### Genericity is a hard requirement

The machinery must serve three unrelated app shapes without the crate learning
any of their semantics:

1. a logic simulator / 8-bit breadboard CPU — gates, buses, clocked signals,
   one chip stamped out many times, hundreds to thousands of nodes;
2. n8n / Node-RED style automation — per-node run status, error branches,
   sub-workflows;
3. an AI/ML pipeline — including a node whose body is a live image.

Node payloads stay app-owned through `NodeViewer<T>`. The crate supplies
containers, navigation, port machinery and paint primitives. It never defines
signal levels, bus widths, mute/bypass, clocking or evaluation.

---

## 2 · Provenance

Produced by an 11-agent workflow: six parallel survey agents (renderer map,
model + `mara_core` primitive inventory, prior art on grouping/subgraphs, prior
art on visuals, domain requirements, Rust API constraints), two competing
designs deliberately split on the shared-vs-flat fork, a synthesising judge, and
an adversarial critic that opened every file and compiled its claims.

Every `file:line` citation below was produced by an agent reading the file. The
compile-verified findings in §3 were re-verified by hand before this document
was written. Claims that turned out to be wrong were corrected in place, and the
corrections are listed in §11 so the provenance stays auditable.

---

## 3 · Pre-existing defects found while designing

These are not new work items invented by the plan — they are live bugs in
`develop` that the design tripped over. Each is assigned to a phase.

- **`--features serde` has never compiled.** `cargo check -p mara_graph
  --features serde` → 46 errors. `Cargo.toml:30` is `serde = ["dep:serde",
  "egui/serde"]`; it never enables `mara_core/serde`, and `vocab::{Pos2, Vec2,
  Rect}` carry no derives at all (`Id`, `Color32`, `Stroke`, `CornerRadius`
  do). **Consequence: no saved graph file exists anywhere, so there is zero
  backward-compatibility burden. Settle the format now.** → P2
- **`GraphWidget::get_selected_nodes` always returns empty.** `SelectedNodes::save`
  writes to Mara memory (`mem.set_temp(mara_id(id), self)`, state.rs:206) while
  `get_selected_nodes_at` reads egui's store with an unconverted id
  (`ctx.data(|d| d.get_temp::<SelectedNodes>(graph_id))`, state.rs:631). Both are
  on `GraphWidget`, not `GraphState`. Zero callers, which is why nobody noticed.
  → P2 (deleted, replaced)
- **Pin resolution is O(nodes × pins × wires).** `wired_inputs`/`wired_outputs`
  (mod.rs:185-197) linearly scan the whole wire `HashSet`; `InPin::new`/
  `OutPin::new` (mod.rs:822-836) call them; `draw_node` builds one `InPin` and
  one `OutPin` for every pin of every node every frame (ui.rs:2037-2044). At
  2000 nodes × 4 pins × 3000 wires that is ~24M iterations and ~8000 `Vec`
  allocations per frame before anything is painted. → P3
- **`NodeId` is not stable.** It is a raw `slab` key (mod.rs:45) and slab
  recycles vacated keys. The renderer keys persistent per-node state off the raw
  index (`NodeState` and `animate_bool` at ui.rs:2049-2054). Anything persisting
  a node reference — frame membership, instance bindings, breadcrumb paths —
  silently rebinds after a delete + insert. → P3
- **Wire paint order is nondeterministic.** The wire set is a `HashSet`
  (mod.rs:199), so overlapping wires shimmer frame to frame and order-keyed
  caching is defeated. → P10
- **`impl Scale for GraphStyle` covers 16 of 33 fields** (scale.rs:168-187).
  `node_halo`, `pin_inset` and `wire_smoothness` genuinely should scale and
  silently do not under `crisp_magnified_text`. → P2
- **Nothing reconciles a wire's pin index against the viewer's pin count.** The
  renderer silently `continue`s a wire whose endpoint has no drawn pin
  (ui.rs:1264-1270), so orphaned wires accumulate invisibly in the `HashSet`
  forever and reappear if the count later grows back. Port reordering makes this
  routine rather than rare. → P3 (`trim_wires_to`), enforced from P7

### The backend trap that shapes the whole visual design

`fill_paint_slot` routes through `shape_from_paint_cmd`
(crates/backend-egui/src/lib.rs:1984-1991), which maps
`Arrow | Text | TextWithFamily | TextRuns | Image | Svg | Clip | Noop` to
`egui::Shape::Noop`. `Group` recurses and `Shadow` maps to a real shape.

**A frame title, a wire label or a thumbnail routed through a reserved paint
slot renders as nothing — no error, no panic.** Default `wire_layer` is
`WireLayer::BehindNodes` (wire.rs:21), so all wire geometry already goes down
this path. Consequences baked in below: reserved slots carry only
rect/shadow/mesh geometry; frames paint inline; wire labels paint inline after
the slot fill.

---

## 4 · The three structural calls

### Call 1 — subgraph definitions are shared by reference

`GraphDoc<T> { root: Graph<T>, defs: Slab<GraphDef<T>> }`. Instances point at a
definition. Build one full-adder, place eight, fix it once.

The 8-bit CPU target is decisive and containment alone cannot express it. Every
reuse-oriented tool surveyed converged here: Blender datablocks, Node-RED
subflows, Logisim subcircuits, Turing Complete components, n8n sub-workflows.

Copy-by-value is not a second mechanism — it is `DefScope::Local`: a definition
hidden from the library, deleted when its sole instance is expanded.
`promote(def)` flips it to `Shared`. One bit.

**Accepted cost:** all instances of a chip show identical interior layout,
because positions live in the definition. Correct, and surprising the first
time. Mitigated by the instance-count badge (§8) and `make_local_copy`.

### Call 2 — nested `Graph<T>` per definition, not one flat arena

The wire `HashSet`, the swept wire cache (`WireId::Connected { graph_id, .. }`,
ui.rs:1280), `draw_order`, pin hit-testing and background painting all stay
per-level and byte-for-byte unmodified. A flat model with parent pointers needs
a `GraphIndex` maintained across six mutation paths plus retroactive wire
materialisation at level boundaries.

The strongest argument for flat was serde-format stability across depths. Per
§3, there is no format to preserve.

**Accepted cost:** `reorder_port`/`remove_port` must rewrite instance wires
doc-wide, and that rewrite is where an off-by-one will hide. The flat model
makes that class of bug structurally impossible. Taken knowingly, paid for with
the heaviest test coverage in the plan (P7, P8).

**Rejected:** widening the payload to `enum NodeKind<T> { User(T), Subgraph(..),
Port(..) }`. It changes the type of `graph[node]` and `get_node`, breaking all
three in-repo `NodeViewer` impls (example/src/app.rs:6142,
mara/src/extras/graph.rs:779, tests/render_characterisation.rs:38) plus the
demo's `eval_output`/`eval_input`. **Also rejected:** a `T: GroupPayload` bound
— splits the widget API in half and is unimplementable for a foreign `T` under
the orphan rule.

### Call 3 — frames and subgraphs stay separate records

Frames never have ports, never enter recursion checks, and must stay
click-through so rubber-band select and canvas panning work over them.
Unifying them into one `Container` with a presentation flag drags frames through
the port and rewiring machinery for no gain.

The *operation* is kept: `GraphDoc::promote_frame` turns a frame into a subgraph
in one call.

---

## 5 · Data model

### Identity: `NodeUid`

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize), serde(transparent))]
pub struct NodeUid(pub u64);   // 0 == unassigned sentinel, repaired on load
```

Unique **within one `Graph<T>` level** only — every consuming map is per-level —
so no doc-wide id coordination is needed. `NodePath` stays globally unique
because each element is unique at its own level.

**Rule: anything persisted uses `NodeUid`; anything per-frame uses `NodeId`.**

Rejected as breaking: a generation counter inside `NodeId` (it is
`#[repr(transparent)] pub usize`, the `Index<NodeId>` key, and
`serde(transparent)`), and making `NodeId` a path (breaks the `Wire` hash set,
the hand-written wire serde seq at mod.rs:103-149, and `remove_node`).

### `Node<T>` — three private fields

Safe: `#[non_exhaustive]` (mod.rs:50) with no public constructor.

```rust
#[non_exhaustive]
pub struct Node<T> {
    pub value: T,
    pub pos: Pos2,
    pub open: bool,
    #[cfg_attr(feature = "serde", serde(default))] uid: NodeUid,
    #[cfg_attr(feature = "serde", serde(default))] frame: Option<FrameId>,
    #[cfg_attr(feature = "serde", serde(default))] size_override: Option<Vec2>,
}
```

All three are **private**, read via `Graph::{uid_of, frame_of, size_override_of}`
and written via `Graph::{set_node_frame, set_node_size_override}`.
`get_node_info_mut` (mod.rs:388) hands out `&mut Node<T>`, so a public `frame`
field would let an app write a foreign or freed `FrameId` and skip the
`revision` bump.

`size_override` is the only home for the ML display node and for a subgraph
block sized to hold a thumbnail — node size is otherwise derived purely from
drawn content (`NodeState`, state.rs:16-143). **It must ship with an owner:**
clamped in `NodeState::set_size` (ui.rs:2743) plus node resize handles, or it is
dead weight. See P11.

### `Graph<T>` — exactly one new field

```rust
pub struct Graph<T> {
    nodes: Slab<Node<T>>,
    wires: Wires,
    #[cfg_attr(feature = "serde", serde(default))] ext: GraphExt,
}

#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GraphExt {
    next_uid: u64,
    frames: Slab<Frame>,
    instances: HashMap<NodeUid, DefId>,     // nodes at this level that are instances
    port_nodes: HashMap<NodeUid, PortId>,   // non-empty only inside a definition body
    #[cfg_attr(feature = "serde", serde(skip))] revision: u32,
    #[cfg_attr(feature = "serde", serde(skip))] by_uid: HashMap<NodeUid, NodeId>,
}
```

One field keeps the serde diff minimal and keeps every new map together for
`repair()` to sweep. `revision` is session-only and resets to 0 on load —
documented, and cache consumers must force a miss after a load rather than
comparing a persisted revision.

### `Wires` — two multimaps

```rust
struct Wires {
    wires: HashSet<Wire>,
    in_of:  HashMap<InPinId,  SmallVec<[OutPinId; 2]>>,
    out_of: HashMap<OutPinId, SmallVec<[InPinId;  2]>>,
}
```

Maintained by `insert`/`remove`/`drop_node`/`drop_inputs`/`drop_outputs`.

**`Wires` has a hand-written `Deserialize` (mod.rs:118-149), so `#[serde(skip)]`
on these fields would be inert.** The `visit_seq` impl must build both maps as it
reads. An empty index after load is a correctness bug — every pin loses its
`remotes` and every wire vanishes from the viewer's point of view — not a perf
regression.

### Frames

```rust
#[derive(Clone, Copy, ...)] pub struct FrameId(pub usize);   // slab key, reused; always pruned by liveness

pub struct Frame {
    pub title: String,
    pub color: Color32,
    pub parent: Option<FrameId>,
    pub shrink: bool,          // auto-fit; mutually exclusive with manual resize
    pub bounds: Rect,          // authoritative when !shrink; last computed fit when shrink
    pub move_mode: FrameMove,  // WithContents (default) | BoxOnly
    pub collapsed: bool,
    pub label_size: f32,       // 8..64
}
pub enum FrameMove { WithContents, BoxOnly }
#[non_exhaustive] pub enum DisposeMode { Dissolve, Purge }
```

Membership is `Node.frame`, **not** a `Vec<NodeUid>` on the frame: re-parenting
is a single write, a deleted node cannot resurrect into a group, and there is no
set to keep in sync with the slab. The reverse view is one pass over `nodes`,
negligible beside existing per-node work. Frames never touch wires.

### Subgraph definitions

```rust
#[derive(Clone, Copy, ...)] pub struct DefId(pub usize);
#[derive(Clone, Copy, ...)] pub struct PortId(pub u32);   // stable within a def, NEVER reused
#[non_exhaustive] pub enum PortDir { In, Out }
#[non_exhaustive] pub enum DefScope { Local, Shared }

pub struct PortDef { pub id: PortId, pub name: String, pub color: Option<Color32> }

#[derive(Default)]
pub struct Ports {
    inputs: Vec<PortDef>,    // Vec order == pin index on the instance node
    outputs: Vec<PortDef>,
    next: u32,
}

pub struct GraphDef<T> {
    pub name: String,
    pub color: Option<Color32>,
    pub scope: DefScope,
    pub ports: Ports,
    pub body: Graph<T>,
}

pub struct GraphDoc<T> {
    #[cfg_attr(feature = "serde", serde(default = "doc_version"))] version: u32,
    pub root: Graph<T>,
    defs: Slab<GraphDef<T>>,
    #[cfg_attr(feature = "serde", serde(skip))] counts_gen: Cell<u32>,
    #[cfg_attr(feature = "serde", serde(skip))] counts: ...,   // eagerly maintained; NOT RefCell
}
```

**Order is presentation; id is identity.** Blender rebuilt its entire socket API
in 4.0 — moving socket definitions off the node tree onto an `interface` object —
precisely because sockets were positional, and every pre-4.0 script broke. Wires
still address pins positionally *inside* a level; the mapping
`instance pin index i ↔ ports.inputs[i].id` is re-derived whenever ports change.

`GraphDoc` must stay `Sync` — `mara_core` has a `bevy` feature deriving
`Resource` on state types, and the crate already pins `graph_style_is_send_sync`
(ui.rs:2921). Hence `Cell` + eager counts, never `RefCell<Option<HashMap<..>>>`.

### The interface is boundary nodes, not a pin list

Inside `def.body`, each port has exactly **one** node, tagged in
`ext.port_nodes`. An input port's node has 0 inputs / 1 output; an output port's
node has 1 input / 0 outputs.

One node per port rather than one aggregate node per side, because each port then
has exactly one pin so all existing pin layout, hit-testing and wire rendering
apply verbatim; ports become positionable, which is how every logic simulator
works; and it avoids the 16-socket wall Blender users complain about. Blender
(Group Input/Output), Logisim (named Pins), Max (inlet/outlet), TouchDesigner
(In/Out COMPs) and Unreal (tunnel nodes) converged here independently.

Boundary nodes need a `T` and the crate cannot mint one — `Graph<T>` carries no
`T: Default` bound (mod.rs:216). The viewer mints it (§6).

### Per-instance state

```rust
pub struct NodePath(SmallVec<[NodeUid; 4]>);   // instance uids from root; root == empty
```

The definition owns topology, layout and payload *templates*. Runtime state — a
carry bit, an n8n run result, an ML preview texture — is app-owned and **must be
keyed by `NodePath.push(node_uid)`**, not by `NodeId`, or eight full-adders alias
to one carry bit. ComfyUI's `1:2:3` scheme. The crate publishes the path via
`begin_level` once per level per frame and stores no app state itself.

`GraphDoc` has no evaluation, no mute/bypass, no typed ports.

### Serde

- Feature-gated derives on `vocab::{Pos2, Vec2, Rect}`.
- `mara_graph/serde = ["dep:serde", "mara_core/serde", "slab/serde"]` — drop
  `egui/serde`; nothing serialised is an egui type once this lands.
- Every new field `#[serde(default)]`, so a `{nodes, wires}` document loads as a
  groupless graph.
- `version: u32` on `GraphDoc`; `repair()` is the single migration entry.
- `Graph::repair()` is idempotent: allocates uids for `NodeUid(0)`, rebuilds
  `by_uid` and the wire multimaps, drops `frame`/`instances`/`port_nodes` entries
  whose uid no longer resolves, breaks `Frame.parent` cycles, re-fits shrink
  frames. Called by the renderer on the O(1) check `by_uid.len() != nodes.len()`.

---

## 6 · API surface

Every new public item must be re-exported **twice**: from
`crates/modules/graph/src/lib.rs` (`mod vendored;` at lib.rs:34 is private) and
from `mara/src/extras/graph.rs:32-36` (Makefile:128 forbids
`example/Cargo.toml` from naming `mara_graph`). An item exported from only one
place is dead for the demo **and the build stays green**.

This plan adds ~25 public names to two lists that cannot be checked against each
other. **Add `pub mod prelude` to `mara_graph` and `pub use
mara_graph::prelude::*` in `mara`**, plus a `make check` grep asserting every
`pub use` name in lib.rs appears in `mara/src/extras/graph.rs`.

### `Graph<T>` — new methods

```rust
// identity
pub fn uid_of(&self, id: NodeId) -> Option<NodeUid>;
pub fn by_uid(&self, uid: NodeUid) -> Option<NodeId>;
pub fn repair(&mut self);
pub fn revision(&self) -> u32;
pub fn contains(&self, id: NodeId) -> bool;
pub fn len(&self) -> usize;
pub fn is_empty(&self) -> bool;
pub fn wires_of(&self, node: NodeId) -> impl Iterator<Item = (OutPinId, InPinId)> + '_;
/// Drop wires addressing pin indices at or beyond the given counts.
/// The owner of the reconciliation gap in §3.
pub fn trim_wires_to(&mut self, node: NodeId, inputs: usize, outputs: usize) -> usize;

// frames
pub fn insert_frame(&mut self, title: impl Into<String>, color: Color32, bounds: Rect) -> FrameId;
pub fn remove_frame(&mut self, f: FrameId, mode: DisposeMode);
pub fn frame(&self, f: FrameId) -> Option<&Frame>;
pub fn frame_mut(&mut self, f: FrameId) -> Option<&mut Frame>;   // bumps revision
pub fn frames(&self) -> impl Iterator<Item = (FrameId, &Frame)> + '_;
pub fn frame_of(&self, node: NodeId) -> Option<FrameId>;
pub fn set_node_frame(&mut self, node: NodeId, f: Option<FrameId>);
pub fn frame_members(&self, f: FrameId) -> impl Iterator<Item = NodeId> + '_;  // direct only
pub fn frame_depth(&self, f: FrameId) -> u32;
pub fn can_parent_frame(&self, child: FrameId, parent: Option<FrameId>) -> bool;
pub fn set_frame_parent(&mut self, child: FrameId, parent: Option<FrameId>) -> bool;

// pure, unit-testable, consumed by the renderer's single deferred-move site
pub fn drag_targets_node(&self, node: NodeId, selected: &[NodeId]) -> SmallVec<[NodeId; 16]>;
pub fn drag_targets_frame(&self, f: FrameId) -> SmallVec<[NodeId; 16]>;   // transitive
pub fn frame_at(&self, p: Pos2, rects: &dyn Fn(NodeId) -> Rect) -> Option<FrameId>;  // innermost
```

`fit_frame_bounds` is **not** a method — it needs node rects, which only the
renderer knows. Free functions in `frames.rs` taking a rect provider, which is
what makes them unit-testable with synthetic geometry:

```rust
pub fn fit_frame_bounds<T>(g: &Graph<T>, f: FrameId, rects: &dyn Fn(NodeId) -> Rect) -> Rect;
pub fn title_band_rect(frame: &Frame, bounds: Rect) -> Rect;
```

### `GraphDoc<T>`

```rust
impl<T> GraphDoc<T> {
    pub fn new() -> Self;
    pub fn from_graph(g: Graph<T>) -> Self;
    pub fn root(&self) -> &Graph<T>;   pub fn root_mut(&mut self) -> &mut Graph<T>;

    pub fn defs(&self) -> impl Iterator<Item = (DefId, &GraphDef<T>)> + '_;
    pub fn def(&self, d: DefId) -> Option<&GraphDef<T>>;
    pub fn def_mut(&mut self, d: DefId) -> Option<&mut GraphDef<T>>;

    pub fn level(&self, path: &NodePath) -> Option<&Graph<T>>;
    pub fn level_mut(&mut self, path: &NodePath) -> Option<&mut Graph<T>>;
    pub fn resolve(&self, path: &NodePath) -> Option<DefId>;             // None == root
    pub fn breadcrumb(&self, path: &NodePath) -> SmallVec<[Crumb; 8]>;

    /// Cheap snapshot of every definition's interface, cloned once per frame.
    /// Exists so the renderer can hold `&mut` one level and still know pin counts.
    /// Keep it to counts/name/colour/revision — widening it gets expensive fast.
    pub fn iface_snapshot(&self) -> IfaceTable;

    pub fn instance_count(&self, d: DefId) -> u32;
    pub fn can_contain(&self, host: Option<DefId>, d: DefId) -> bool;    // DFS recursion guard
    pub fn depth_of(&self, d: DefId) -> u32;

    /// What ports WOULD collapse derive? No mutation. Order: member pos.y,
    /// then pos.x, then pin index — deterministic, therefore assertable.
    pub fn derive_ports(&self, path: &NodePath, members: &[NodeUid]) -> Vec<PortDef>;

    pub fn collapse(&mut self, path: &NodePath, members: &[NodeUid], scope: DefScope,
                    name: String, f: &mut dyn NodeFactory<T>) -> Result<(DefId, NodeUid), GraphError>;
    pub fn expand(&mut self, path: &NodePath, instance: NodeUid) -> Result<UidRemap, GraphError>;
    pub fn instantiate(&mut self, path: &NodePath, def: DefId, pos: Pos2,
                       f: &mut dyn NodeFactory<T>) -> Result<NodeUid, GraphError>;
    pub fn make_local_copy(&mut self, path: &NodePath, instance: NodeUid) -> Result<DefId, GraphError>;
    pub fn promote(&mut self, def: DefId);            // Local -> Shared
    pub fn promote_frame(&mut self, path: &NodePath, f: FrameId, scope: DefScope,
                         fac: &mut dyn NodeFactory<T>) -> Result<(DefId, NodeUid), GraphError>;

    pub fn reorder_port(&mut self, def: DefId, dir: PortDir, from: usize, to: usize);
    pub fn remove_port(&mut self, def: DefId, port: PortId);
    pub fn rename_port(&mut self, def: DefId, port: PortId, name: String);
    pub fn repair(&mut self);
}

#[non_exhaustive]
pub enum GraphError {
    WouldRecurse, DepthLimit(u32), ViewerDeclined,
    NoSuchLevel, NoSuchNode, EmptySelection, MixedLevels,
}
```

### `NodeFactory<T>` — object-safe, so algorithms are testable with `T = ()`

```rust
pub struct PortSpec<'a> { pub id: PortId, pub dir: PortDir, pub name: &'a str, pub index: usize }

pub trait NodeFactory<T> {
    fn instance_node(&mut self, def: DefId, name: &str, ports: &Ports) -> Option<T>;
    fn port_node(&mut self, spec: &PortSpec<'_>) -> Option<T>;
}
```

`None` aborts with `GraphError::ViewerDeclined`, leaving the doc byte-identical.
The renderer wraps the viewer in `struct ViewerFactory<'a, V>(&'a mut V)`.
`NodeViewer<T>` cannot be the bound — it is dyn-incompatible via RPITIT
(`-> impl NodePin + 'static`, viewer.rs:130/143).

### `NodeViewer<T>` — twelve additions, all defaulted; two removals

A defaulted method can never invent a `T`, so `-> Option<T>` returning `None` is
the only shape that is simultaneously defaultable and payload-producing. Two of
the three in-repo impls (`TestViewer`, `MinimalViewer`) implement only the 5
required methods, so any addition without a default breaks the build immediately.

```rust
// payload minting
fn make_instance_node(&mut self, def: DefId, name: &str, ports: &Ports) -> Option<T> { None }
fn make_port_node(&mut self, spec: &PortSpec<'_>) -> Option<T> { None }

// level context — THE per-instance-state hook
/// Called once per level per frame, before anything is drawn. Key your
/// per-instance state by `path`, NOT by NodeId — otherwise every instance
/// of a definition aliases to one state.
fn begin_level(&mut self, path: &NodePath, def: Option<DefId>) { let _ = (path, def); }

// id-taking widenings; the renderer calls these, not the &T forms
fn inputs_of  (&mut self, n: NodeId, g: &Graph<T>) -> usize  { self.inputs(&g[n]) }
fn outputs_of (&mut self, n: NodeId, g: &Graph<T>) -> usize  { self.outputs(&g[n]) }
fn title_of   (&mut self, n: NodeId, g: &Graph<T>) -> String { self.title(&g[n]) }
fn has_body_of(&mut self, n: NodeId, g: &Graph<T>) -> bool   { self.has_body(&g[n]) }

// port pins: concrete PinInfo, NOT `impl NodePin`
fn show_port_pin(&mut self, ctx: PortPinCtx<'_>, ui: &mut MaraUi<'_>) -> PinInfo {
    ui.label(ctx.name); PinInfo::circle()
}

// visual descriptors: all the richness, zero domain semantics
fn node_chrome(&mut self, n: NodeId, tier: DetailTier, g: &Graph<T>) -> NodeChrome { NodeChrome::default() }
fn wire_fx(&mut self, from: &OutPinId, to: &InPinId, g: &Graph<T>) -> WireFx { WireFx::default() }
fn decorate_node(&mut self, n: NodeId, rect: Rect, tier: DetailTier,
                 painter: &MaraPainter<'_>, g: &Graph<T>) { }

// frames
fn frame_spec(&mut self, default: FrameSpec, f: FrameId, depth: u32, g: &Graph<T>) -> FrameSpec { default }

// structural notifications
fn on_structure(&mut self, ev: &StructureEvent<'_>) { let _ = ev; }
```

```rust
#[non_exhaustive]
pub enum StructureEvent<'a> {
    Entered(&'a NodePath), Exited(&'a NodePath),
    Collapsed { def: DefId, instance: NodeUid, members: &'a [NodeUid] },
    Expanded  { def: DefId, remap: &'a UidRemap },
    PortsChanged(DefId),
}
```

`show_port_pin` returns a concrete `PinInfo` rather than a second RPITIT method
because `PinWireInfo` (pin.rs:28) is re-exported from neither ui.rs nor lib.rs,
so external code cannot implement `NodePin` at all. Note also that
`NodePin::draw(self, ..)` takes `self` **by value** (pin.rs:54), so the trait is
not object-safe and a `Box<dyn NodePin>` cannot unify the `show_input` branch
with the `show_port_pin` branch — the renderer duplicates the pin-draw call site
per branch.

**Removed** (grep confirms nothing in the workspace overrides any of the three):

- `draw_background` changes from `&egui::Painter` to `&MaraPainter<'_>`,
  dropping the `__internal_painter_from_egui` bridge at viewer.rs:365.
- `has_node_style` / `apply_node_style(&mut egui::Style, ..)` (viewer.rs:62-83)
  are **deleted**; per-node appearance is `node_chrome` plus the existing
  `node_frame`/`header_frame`.

This is mandatory early: Makefile:136 bans an `egui::` token in
`example/src/*.rs`, so a viewer method the demo cannot spell is a method the
demo cannot demonstrate. The trait's net egui surface goes to **zero** while its
method count rises.

Also deleted: `Graph::show(&mut self, viewer, style, id_salt, ui: &mut egui::Ui)`
(ui.rs:2882-2898) — public, raw-egui, zero callers, contradicts WS-D1, and is a
second entry point grouping would have to keep in sync.

### `GraphWidget` / `GraphState`

```rust
impl GraphWidget {
    pub fn show<T, V: NodeViewer<T>>(&self, g: &mut Graph<T>, v: &mut V, ui: &mut MaraUi<'_>) -> MaraResponse;   // unchanged
    pub fn show_doc<T, V: NodeViewer<T>>(&self, doc: &mut GraphDoc<T>, v: &mut V, ui: &mut MaraUi<'_>) -> GraphOutcome;
    pub fn id(self, id: vocab::Id) -> Self;            // was egui::Id (ui.rs:958)
    pub fn open_path(self, path: &NodePath) -> Self;   // deep-link / programmatic exit
    pub fn nav(self, nav: NavMode) -> Self;            // NavMode::{Enabled, Locked}
}

pub struct GraphOutcome {
    pub response: MaraResponse,
    pub path: NodePath,
    pub breadcrumb: SmallVec<[Crumb; 8]>,
    pub selection: SmallVec<[NodeUid; 8]>,
}

impl GraphState {
    pub fn path(cx: &dyn MaraCtx, id: vocab::Id) -> NodePath;
    pub fn set_path(cx: &dyn MaraCtx, id: vocab::Id, path: &NodePath);
    pub fn selection(cx: &dyn MaraCtx, id: vocab::Id) -> SmallVec<[NodeId; 8]>;
}
```

`Graph<T>` stays fully usable standalone and `GraphWidget::show` is unchanged.
Two entry points is a real cost, taken deliberately: making `Graph<T>`
recursively contain sub-graphs would make it self-referential, make it impossible
to hold `&mut` the current level while reading def interfaces, and multiply the
serde surface.

The breadcrumb is returned as **data**, not painted on the canvas by default —
the host owns chrome (`MaraShellPlugin`) and a canvas-drawn breadcrumb would
fight the enforced top bar. The crate paints a small in-canvas exit chip as a
fallback when nesting is active, gated on a style flag.

`GraphState::nudge_saved_translation(.., egui::Vec2)` (state.rs:357) ports to
`vocab::Vec2`, fixing the call sites at mara/src/extras/graph.rs:455-470.
`NodeState::load/store/clear` (state.rs:37/58) port from `&egui::Context` to
`&dyn MaraCtx` + Mara memory — the P5 frame pre-pass calls `NodeState::load` and
must not reintroduce an egui reference. Note `NodeState::load` also takes
`&egui::style::Spacing`; `mara_core::style::item_spacing()` exists and ui.rs
already wraps it as `mara_item_spacing()` (ui.rs:906), but `NodeState::initial`
uses more of `Spacing` than item spacing alone.

### `GraphStyle` must stay `Copy`

`GraphStyle` is `Copy` (ui.rs:355), `GraphWidget` is `Copy` (ui.rs:916),
`GraphWidget::style` is a `const fn` (ui.rs:972). One `String`/`Vec`/`HashMap`
field breaks all three, with errors at every call site rather than at the
definition. All new fields are fixed-size:

```rust
pub node_shadow:    Option<ShadowSpec>,
pub select_halo:    Option<HaloSpec>,
pub header_accent:  Option<f32>,
pub frame_style:    Option<FrameStyle>,
pub subgraph_style: Option<SubgraphStyle>,
pub lod:            Option<LodLadder>,
pub camera_spring:  Option<f32>,
```

`GraphStyle` also derives `PartialEq` and, under the feature, `Serialize` +
`Deserialize` (ui.rs:357-359), and has a trailing `pub _non_exhaustive: ()`
(ui.rs:626). **Every new field therefore needs five things**: `Copy`,
`PartialEq`, serde derives, an entry in `GraphStyle::new()` (ui.rs:830, lists
fields exhaustively), and an entry in `impl Scale` (scale.rs:168-187). Naming
only `Copy` and `new()` is how the serde gate added in P2 starts failing in P5.

Per-frame titles and colours are **model data** on `Frame`/`GraphDef`, never
style.

### `mara_core` additions — five, all verified absent

```rust
// crates/core/src/vocab.rs
impl Color32 {
    /// Per-channel linear lerp on premultiplied bytes.
    pub fn lerp(self, other: Color32, t: f32) -> Color32;
}
// + serde derives on Pos2, Vec2, Rect

// crates/core/src/mui/mod.rs — MaraInput (has modifiers_shift/ctrl/alt at :235-237)
pub modifiers_command: bool,   // platform-folded cmd/ctrl, populated in backend-egui

// crates/core/src/layout.rs — CursorIcon (has only PointingHand, Grabbing,
// ResizeHorizontal, ResizeVertical at :793-798)
ResizeNwSe, ResizeNeSw,        // or restrict frame resize to the four edge handles

// crates/core/src/style.rs
FrameRole::Group,              // BREAKING: FrameRole is not #[non_exhaustive]
                               // and frame_for matches exhaustively (:2912-2955)
```

**`Color32` is already premultiplied** — `from_rgba_unmultiplied` premultiplies
on construction (vocab.rs:689-703). A `premultiplied()` helper would return
`self`; it is not part of this plan. A per-channel linear lerp on premultiplied
bytes is the mathematically correct operation for both the opaque gradient and
the feathered edge, and `Color32::TRANSPARENT` is `[0,0,0,0]`, which is already
the correct feather colour. If a perceptual mix is wanted for opaque-to-opaque
gradients, add `lerp_gamma` separately and use it only where alpha is constant.
While here, fix or delete `with_alpha_factor`'s doc comment at ui.rs:2806, which
incorrectly claims it returns an un-premultiplied colour.

`MaraCtx::set_cursor_icon` (context.rs:224) has a no-op default, so a backend
that does not override it silently drops every cursor hint.

No new `PaintCmd` variant. No GPU work. No shaders.

---

## 7 · Interaction

### Frames

**Create.** `F` with a non-empty selection creates a frame fitting the
selection, sets `Node.frame` on each member, and opens an inline rename editor on
the title band immediately. Blender moved all frame ops to `F` in 4.5 because
implicit-only creation "felt like it got in the way"; typing the label right away
is what makes frames worth making. Also in the graph context menu.

**Drag — the title band is the only handle; the body is click-through.**
Non-negotiable. `graph_resp` is allocated at ui.rs:1120 and
`Scene::register_pan_and_zoom` runs at :1124-1134, so a `Sense::Drag` hotspot
spanning the whole frame rect — registered later, therefore winning on hit
priority — would steal canvas panning everywhere a frame exists. Click-through
also keeps rubber-band selection working over a frame.

The title-band interact is registered in the frame pass, **before** the node loop
(ui.rs:1215), so nodes and pins inside keep pointer priority.

Delta is already graph-space (`Response::drag_delta()` divides by the layer
transform's scaling). Carried as `frame_moved: Option<(FrameId, Vec2)>` beside
the existing `node_moved` and applied at the **same single deferred site**
(ui.rs:1604-1617):

```rust
if let Some((f, d)) = frame_moved {
    match graph.frame(f).unwrap().move_mode {
        FrameMove::WithContents => for n in graph.drag_targets_frame(f) { graph.nodes[n.0].pos += d; }
        FrameMove::BoxOnly      => { /* only frame.bounds += d; requires !shrink */ }
    }
}
```

**Adopt / detach.** Never recompute membership from geometry after creation —
that is Unreal's model and it produces surprising captures when comments overlap.
Instead: while a node drag is in flight, compute the innermost frame containing
the node's centre and highlight it (stroke alpha 0.45 → 0.90, fill 0.10 → 0.16),
so the prospective parent is visible before release. On drag stop that frame
becomes `node.frame`; dragging fully out releases to the frame's **parent**, not
to root, so nesting survives. `F` during a node drag toggles attach/detach
against the frame under the cursor. `Alt` suppresses adoption entirely.

**Resize.** Only when `shrink == false` — auto-shrink and manual resize are
mutually exclusive modes. Eight handles, drawn and hit-tested only on hover or
selection. Handle hit rects are constant **screen** size: `handle_px /
to_global.scaling` in graph space (Blender filed a bug for exactly this, PR
#108359; it costs one division). Starting a resize sets `shrink = false` and
keeps the bounds it had at that moment — no jump.

**Nesting.** `Frame.parent`, with `can_parent_frame` rejecting ancestor cycles.
Nested frames alternate lightness by depth (`L *= 1 ± 0.06` per level) so
dark-on-dark nesting stays visible.

**Collapse.** `Frame.collapsed` renders the frame as a pill `"Name (12)"` and
hides members from the node loop. Distinct from a subgraph block: no pins, no
wires touched; wires to hidden members draw as stubs to the pill edge.

**Keys.** `F` create / toggle-attach, `Shift+F` unframe selection, `Del` on a
title removes the frame (`Dissolve`), `Alt+drag` on a title forces `BoxOnly` for
that drag, `Ctrl+G` on a selected frame promotes it to a subgraph.

### Subgraphs

**Collapse** — `Ctrl+G` on a selection, or the node menu.

1. Snapshot `wires()` **before** any mutation — `remove_node` (mod.rs:299) calls
   `Wires::drop_node` (:166), which silently drops every incident wire, and those
   are exactly the boundary wires that must be preserved.
2. Partition into interior / crossing-in / crossing-out / external. A multi-level
   selection is rejected with `MixedLevels` rather than silently flattened.
3. Derive ports via the pure `derive_ports`: one **input** port per distinct
   interior `InPinId` targeted from outside (two external sources into one
   interior pin share one port); one **output** port per distinct interior
   `OutPinId` feeding outside (fanning to all consumers). Order: member `pos.y`,
   then `pos.x`, then pin index.
4. Move members into `def.body` preserving uid, positions normalised to the bbox
   origin; insert one boundary node per port at the bbox left/right edge.
5. Remove members from the parent, insert the instance node at the bbox centre,
   reconnect crossing wires to the instance's derived pin indices.

Default scope is `Local`. `Ctrl+Shift+G` collapses to `Shared` and prompts for a
name. Auto-derivation **seeds an editable list** — it is not the storage model.

**Expand** — `Ctrl+Shift+G` on an instance. **Ships in the same release as
collapse.** ComfyUI still has not shipped it and it is their top subgraph
complaint; retrofitting is hard because it needs interior re-parenting, boundary
rewiring, uid collision resolution and position restoration. Interior nodes get
fresh uids (uid uniqueness is per-level and the def's uids may collide with the
host's); the `UidRemap` goes to the viewer so it can migrate per-path state. A
`Local` def with no remaining instances is deleted.

**Enter.** Double-click the instance node's frame. The node frame response at
ui.rs:2105-2111 already carries `double_clicked()`, and the node widget is
registered **after** `graph_resp`, so it does not collide with the background
double-click-to-centre at ui.rs:1425-1428. A dive chevron at header-right is the
discoverable equivalent for people who do not guess double-click.

**Exit.** `Esc`, `Backspace` on empty canvas, a breadcrumb click (every segment
jumps directly to that level — Houdini and TouchDesigner path-jump rather than
tab-switch), or the in-canvas exit chip. Exiting selects the instance you came
out of, so the eye lands where it left.

**Per-level state — the single thing that makes nesting work.** `graph_id`
(ui.rs:996-998) keys the sublayer (:1081), the whole `GraphState`, every
`NodeState` (`graph_id.with(("graph-node", node))`, :2049) and the wire cache
(`WireId::Connected { graph_id, .. }`, :1280). `show_doc` salts once:
`level_id = graph_id.with(("lvl", &path))`, and re-keys **all four, node state
included**.

> Node state must be salted too. `NodeId` is a per-`Graph<T>` slab index, so
> root's `NodeId(0)` and `defs[d].body`'s `NodeId(0)` are different nodes with
> the same id — they would share one `NodeState` slot and one openness
> animation, and `node_state.clear()` on a removed node (ui.rs:2137, 2151, 2779)
> would wipe an unrelated level's cache. The cost is a one-frame re-measure on
> entry, already handled: `NodeState::load` calls `request_discard` for unknown
> ids.

The path itself lives in a `NavState { path: NodePath }` side-struct in Mara
memory keyed by `mara_id(base_graph_id)`, saved/loaded exactly as `DrawOrder`
does (state.rs:182-198) — UI state, not model state, so it must not enter
`GraphDoc` and costs nothing in the file format. Pruned on load by truncating at
the first element whose uid no longer resolves, mirroring `prune_selected_nodes`
(state.rs:246).

**Guards.** `can_contain` rejects a definition containing itself transitively
(`WouldRecurse`); a depth cap (default 16) returns `DepthLimit` — a typed error,
never a panic. Without this the first user who drags an ALU into itself gets a
stack overflow at render time.

**Structural edits are intents, applied after rendering.** During the frame the
renderer holds `&mut Graph<T>` for one level and cannot touch `doc`. Gestures
push into `Vec<GraphIntent>` (`Enter`, `ExitTo(i)`, `Collapse`, `Expand`,
`Instantiate`, `MakeLocalCopy`, `PromoteFrame`) and `show_doc` drains them after
`graph_state.store` (ui.rs:1619) — the same discipline as the existing deferred
`node_moved`/`node_to_top`.

**Instance pin counts never reach the viewer.** The renderer checks
`graph.ext.instances` first and answers from the `IfaceTable` snapshot; only
non-instance nodes fall through to `inputs_of`/`outputs_of`. This is why the
snapshot is cloned once per frame — it dodges the borrow conflict between
`&mut doc.defs[d].body` and `&doc.defs`.

### Pin exposure

After auto-derivation, ports are fully owned. Inside the definition each port is
a real boundary node, so it is draggable, wireable and hit-testable with zero new
machinery. Right-click a boundary node or an instance pin → crate-drawn menu:
Rename, Remove, Move up/down. Right-click any interior pin → "Expose on ⟨def⟩".
`reorder_port`/`remove_port` rewrite instance wires doc-wide and must call
`trim_wires_to`; `rename_port` touches no wire. Instance-level
connect/disconnect still consults `NodeViewer::connect`/`disconnect`, so the
app's validity policy applies unchanged and the crate learns nothing about types.

### Selection

Unchanged. Rubber-band and ctrl+click operate on the current level only. A plain
primary click on a node still does not change selection (ui.rs:2117-2123),
ctrl+click on empty canvas still clears (:1430), rubber-band still requires shift
(:1174). All new gestures are additive. Every ctrl/cmd gesture needs
`MaraInput::modifiers_command`, which does not exist yet.

---

## 8 · Visuals

All `PaintCmd` variants named below are verified present in
crates/core/src/paint.rs:34-165 — `Line, Polyline, Polygon, RectFilled,
RectStroke, RectStrokeOutside, CircleFilled, CircleStroke, Ellipse, Arc, Sector,
Arrow, Text, TextWithFamily, TextRuns, Image, Svg, Mesh, Shadow, Clip, Group` —
reached through `MaraPainter` (mui/mod.rs:571-939). **Zero GPU or shader work.**

Re-read §3's backend trap before adding anything to a reserved slot.

### Tier 1 — no model dependency

**Node drop shadow + drag lift.** `PaintCmd::Shadow { offset: [0,4], blur: 12,
spread: 0, black a=0.35 }` at rest; on drag `offset: [0,10], blur: 24, a=0.45`
plus a 1.02× scale about the node centre over a 120 ms ease. The cheapest
premium cue there is.

**Selection halo, not a selection border.** Replace the single `select_style`
rect (ui.rs:2084-2095) with a core 2 px accent stroke plus three outside strokes
at widths 4/7/11 and alphas 0.18/0.09/0.04. 1 px borders are illegible in a
multi-select over a dense graph. The last-clicked node breathes at 0.5 Hz between
0.85 and 1.0 alpha so "active" is distinguishable from "also selected".

**Header accent bar.** A 3 px `RectFilled` across the node top using per-corner
`CornerRadius { nw, ne }`, header background = accent at 12 % over the panel
colour, title accent-tinted. Colour comes from `NodeChrome.accent`, and the crate
ships a **fixed 8-colour palette** (constant L≈0.65 / C≈0.14 in OKLCH at hues
20/55/95/145/190/250/300/340) rather than a free picker — free pickers make
user-coloured graphs look chaotic, and ComfyUI's ecosystem converged on preset
swatches for exactly this reason. **Meaningless before `NodeChrome` exists**, so
it lands in P6, not P1: without a per-node accent it paints the same global
colour on every node.

**Camera spring — a prerequisite dressed as a visual.** One critically-damped
integrator, `x += (target - x) * (1 - exp(-dt * rate))`, with **zoom interpolated
in log space** so it feels linear. `MaraCtx::dt()` and `now()` exist
(context.rs:155-158). Every camera change routes through it: the existing
double-click fit-to-content (ui.rs:1425), `GraphState::look_at` (state.rs:365),
fit-to-selection, breadcrumb jumps, minimap pans and the portal dive. ~30 lines,
and each of those then costs ~10 lines instead of a bespoke animation.

**Zoom LOD ladder — the primary perf lever.** On transform scale `s`:
`>0.9 Full`; `0.5..0.9 Compact` (drop pin labels); `0.25..0.5 Pins` (pins become
dots, title truncated, no body, no animation); `≤0.25 Blob` (solid rounded rect
in the node's **status** colour, not its category colour, so errors stay findable
in a 300-gate graph; wires collapse to 1 px straight lines with bezier sampling
skipped). Each tier cross-fades with `alpha = smoothstep(t0, t1, s)` — hard
switches read as glitches. The tier reaches the viewer as `DetailTier` so
app-drawn bodies degrade in step.

**Background: two-level grid + vignette.** `background_pattern.rs` is already at
zero egui refs. Dots at low zoom; at high zoom minor grid lines at alpha 0.04
with a major every 5 at 0.09, `alpha *= smoothstep(0.4, 0.8, s)`. The vignette is
an 8-vertex `PaintCmd::Mesh` with per-vertex alpha.

**Glassmorphism without blur** is free and worth taking: node background at ~0.82
alpha over the canvas, a 1 px white-alpha-0.06 highlight along the **top edge
only**, a 1 px dark outer stroke. Because the canvas already carries a dot/grid
pattern, semi-transparent bodies read as frosted glass with no blur pass — 80 %
of the look for two extra paint commands.

### Frames

`FrameSpec` from `FrameRole::Group` so it themes for free, overridable via
`NodeViewer::frame_spec`. Corner 8; fill = `Frame.color` at alpha 0.10; 1 px
stroke at 0.45; title band of height `1.6 * label_size` at fill alpha 0.22 with
the title at full brightness; low-blur `PaintCmd::Shadow`. Nested depth alternates
lightness ±6 % per level.

**The shrink bounds must reserve the title band**, not merely union child rects —
Blender bug T40094 was exactly this, the topmost child overlapping the title
text. `bounds = union(member rects) ∪ union(child frame bounds)`, expanded by
padding, then `min.y -= title_band_height`.

Frames paint **inline**, between `viewer.draw_background` (ui.rs:1161-1167) and
the wire slot reservation (:1199-1202), which puts them under wires and under
nodes automatically. Computing bounds up front with no one-frame lag means
reproducing `draw_node`'s prologue — extracted into one `node_frame_rect_of(..)`
used by both so they cannot drift. Do **not** depend on
`DrawNodeResponse.final_rect`, which is only collected when a rubber-band just
ended (:1247).

### Subgraphs

**Deck of cards.** Two extra rounded-rect `RectStroke`s offset (+3,+3) and
(+6,+6) behind the instance node at alpha 0.35 / 0.15. The cheapest possible
"there is a world inside" read; works at every zoom, survives LOD.

**Dive chevron** at header-right, growing on hover while the border brightens.

**Depth tint.** Each nesting level shifts the canvas pattern lightness ~+4 %, so
"I am two levels down" is pre-attentive rather than something you read.

**Instance-count badge.** `"×8"` on the instance header when
`instance_count(def) > 1`. Blender shows a datablock user count for the same
reason: **the sharing model must be visible or users are blindsided when editing
one chip changes seven others.**

**Exposed pins as double-ring sockets** — two `CircleStroke`s at different radii
— so mapped-through pins are distinguishable from leaf pins.

**Portal dive.** On enter, set the camera-spring target so the instance's screen
rect maps to the interior's bounding box, then let the spring run ~200 ms while
the parent layer fades out scaling to 1.06× and the interior fades in from 0.94×.
`MaraUi::multiply_opacity` (mui/mod.rs:1260), `set_opacity` (:1254) and
`set_layer_transform` (:1728) all exist. Reverse on exit. Cut it and entering
reads as a page reload; nearly free once the spring exists.

### Wires — the highest impact/cost item

**Gradient wires.** Sample the bezier at `N = clamp(screen_len/8, 8, 64)` points;
`wire.rs` already samples and already has a `SweptCache` (wire.rs:705-723), so
extend the cache entry with a cumulative arclength table. Emit **one**
`PaintCmd::Mesh` triangle strip per wire, per-vertex colour lerped by arclength.
~40 lines, no new primitive, and it is the single visual that reads as "this
editor was designed".

**Feathered glow, replacing the multi-pass stroke bloom** (ui.rs:1318-1354
re-tessellates each wire five times). Three vertex columns per sample instead of
two: left edge alpha 0, centre alpha 1, right edge alpha 0, half-width
`w/2 + glow`, falloff `(1 - d/h)^2`. Four triangles per sample versus five full
bezier tessellations per wire — cheaper *and* better looking, and it survives
zoom-out.

`MaraPainter::mesh` **silently no-ops on bad index counts** (mui/mod.rs:872-879),
so a malformed strip renders as nothing rather than as garbage. Assert
`indices.len() == 12 * (samples - 1)`.

**Wire drop shadow.** The sampled polyline drawn once at +2 px screen y in black
alpha 0.22 at width `w+1`, before the coloured pass. `PaintCmd::Shadow` is
**rect-only** — do not reach for it here.

**Hover focus / dim-the-rest.** On node or wire hover, BFS the 1-hop
neighbourhood; everything else gets colour pulled 40 % toward the canvas
background **and** alpha ~0.30 (alpha alone leaves it too saturated), eased per
item by `e += (target - e) * (1 - exp(-dt/0.08))`. For a 300-gate CPU this is the
difference between usable and unusable, and it is pure colour arithmetic at paint
time.

**Routing as a style knob**, not baked beziers: `Bezier` (tangent
`clamp(0.5*|dx|, 30, 200)`, with a 1.5× boost and vertical bow when `dx < 0`) /
`Orthogonal` (corners as real `PaintCmd::Arc`s of `r = min(corner_r, half of each
leg)`) / `Subway` (45°). Orthogonal-with-arc-corners is what the breadboard-CPU
target actually wants.

**Bus rendering.** `width = base * (1 + 0.35*log2(bits))` clamped to
`base..3*base`, and above `width > 1` render as two thin parallel strokes
separated by the bus width — an actual electrical-schematic convention. Driven
entirely by `WireFx.width`/`double_line`; the crate knows nothing about bits.

**Event pulses beat marching ants.** A small `Vec<(Wire, t0)>` emitting one
bright head dot with a 3-dot fading tail per value change, lifetime ~300 ms.
More informative than continuous flow — you watch causality propagate through the
CPU — and an idle graph then costs **zero repaints**. There is no dash primitive
(`vocab::Stroke` is width+colour only) so marching dashes would have to be
hand-segmented anyway.

**Wire labels and any `Flow` head marker built from `Arrow` must be painted
inline after the slot fill** (ui.rs:1586-1589), never inside `wire_shapes`. Add a
one-line contract comment at that site: only
`Line/Polyline/Polygon/Rect*/Circle*/Ellipse/Arc/Sector/Mesh/Shadow/Group` may
enter `wire_shapes`.

### Genericity: two descriptors carry all of it

```rust
pub struct NodeChrome {
    pub accent: Option<Color32>,
    pub status: Option<Status>,     // Ok | Error | Running{progress} | Waiting | Disabled
    pub badges: SmallVec<[Badge; 2]>,
    pub thumbnail: Option<TextureId>,
    pub shape: NodeShape,           // Card (default) | Dot
    pub emphasis: f32,              // 0..1, drives dim/focus
}
pub struct WireFx {
    pub color_a: Option<Color32>, pub color_b: Option<Color32>,
    pub width: Option<f32>, pub double_line: bool,
    pub routing: Option<Routing>,
    pub flow: Option<Flow>,         // Continuous{speed,dots} | Pulse{t0}
    pub label: Option<String>,
    pub emphasis: f32,
}
```

The logic sim maps hi/lo to `color_a`/`color_b` plus a `Pulse` on value change;
n8n maps run state to `status`; the ML pipeline maps a preview image to
`thumbnail`. **One machinery, three domains, zero domain semantics in the crate.**
`Status`, `Routing`, `Flow`, `NodeShape` are all `#[non_exhaustive]` from the
first commit or the next variant is a breaking change.

`NodeShape::Dot` is not optional garnish: a chromeless reroute/pass-through node
is the primary long-wire readability tool in every logic simulator and in
Blender, and cannot be expressed at all today — the crate would wrap a 1-in/1-out
reroute payload in a full node frame, header frame and collapse chevron. Adding
the field in P6 costs one variant; retrofitting it after P5 hard-codes
header+frame geometry is a breaking layout change.

For signal encoding the crate documents the convention but does not enforce it:
**brightness and glow carry the value, hue carries the type.** Hue differences
vanish at zoom-out and for colourblind viewers; luminance differences do not.
Logisim's canonical set modernised (bright green 1 / dark green 0 / blue floating
/ orange width-mismatch) lives in the demo's viewer, not the crate.

### Missing primitives, stated honestly

- **Added:** `Color32::lerp`, `MaraInput.modifiers_command`,
  `CursorIcon::{ResizeNwSe, ResizeNeSw}`, `FrameRole::Group`, serde derives on
  `vocab::{Pos2, Vec2, Rect}`.
- **Absent, worked around:** dashed strokes (hand-segment; precedent at
  command_palette.rs:413-419); gradients beyond raw `Mesh` (Gouraud only, no UVs,
  no linear/radial helper); bezier/spline commands (wires are CPU-tessellated to
  Polyline already); rounded-rect clipping (`Clip` is axis-aligned); real blur or
  bloom (N-pass alpha only); wrapped or ellipsised text (`measure_text` uses
  `layout_no_wrap`, backend-egui/src/lib.rs:155-168 — the caller truncates frame
  titles itself); rotated plain text (only `PaintCmd::TextRuns` carries an angle,
  paint.rs:125).
- **Explicit non-goals:** real backdrop-blur glass, real bloom post-process, A*
  obstacle-avoiding routing, wire bundling, Nuke-style in-place group peek,
  instanced rendering beyond ~10k nodes, cross-document definition libraries.

---

## 9 · Phases

Roughly 7–9 weeks total. **P1–P5 deliver most of the organisational value and
are independently shippable at any point.** P7–P8 are where the risk
concentrates and where a partial landing is worst.

### P1 · Visible polish — drop shadow and drag lift

`ui.rs`, `ui/scale.rs`, `tests/render_characterisation.rs`

Zero model change, zero trait change, zero new `mara_core` primitives. Add a
`ShadowSpec` Copy struct next to `NodeHalo` (ui.rs:314-340) and
`node_shadow: Option<ShadowSpec>` on `GraphStyle`. Generalise the existing halo
slot (ui.rs:2172-2174) into one always-reserved **underlay slot**, filled at
ui.rs:2755-2770 with a `PaintCmd::Group(vec![shadow, existing_halo])`.
`Group` of rect+shadow commands is the one thing `shape_from_paint_cmd` fills
correctly.

Two traps, both concrete:

- **Capture `let lifted = r.dragged_by(PointerButton::Primary);` at ui.rs:2112**,
  before `r` is shadowed at ui.rs:2176 by the frame's `InnerResponse` — whose
  `.response` was allocated by `egui::Frame::show` with no drag sense, so the
  lift would never fire.
- **Spell `mara_core::vocab::CornerRadius` in full.** `CornerRadius` and `Margin`
  in `ui.rs` resolve to *egui's* via the bulk import at ui.rs:5-11. The existing
  halo fill already spells it out (ui.rs:2762).

Add the new field to `GraphStyle::new()` (ui.rs:830), `impl Scale`
(scale.rs:168-187), and give it `PartialEq` + serde derives. Add
`const _: () = { const fn is_copy<T: Copy>() {} is_copy::<GraphStyle>() };` next
to `graph_style_is_send_sync` (ui.rs:2923).

The layered selection halo moves to P6 (it wants `SelectionStyle` exported
first) and the header accent bar moves to P6 (meaningless before
`NodeChrome.accent`).

**Verify.** `cargo test -p mara_graph`. New tests in
`tests/render_characterisation.rs` keeping the two-pass discipline at lines
83-131: `shadow_adds_geometry_behind_the_node`,
`two_identical_passes_are_byte_identical`. **Assert on `vertex_count`, not
`mesh_count`** — `mesh_count` is per tessellated texture/clip batch, not per
shape, which is why every existing assertion uses vertex counts. The
`is_copy::<GraphStyle>()` const assertion fails the build if `Copy` breaks.

*Effort: half a day.*

### P2 · Seam prep — close the egui holes, add the primitives, un-break serde

`crates/core/{vocab,layout,style,mui}.rs`, `crates/backend-egui/src/lib.rs`,
`graph/Cargo.toml`, `graph/src/{lib.rs, vendored/{ui.rs, ui/state.rs, ui/viewer.rs}}`,
`mara/src/extras/graph.rs`, `Makefile`

**mara_core.** Serde derives on `vocab::{Vec2, Pos2, Rect}`. `Color32::lerp`.
`MaraInput::modifiers_command`, populated in backend-egui from egui's
platform-folded `Modifiers::command`. `CursorIcon::{ResizeNwSe, ResizeNeSw}` and
their backend mapping. `FrameRole::Group` plus a `frame_for` arm — **this is
semver-breaking**, `FrameRole` is not `#[non_exhaustive]` and `frame_for` matches
exhaustively (style.rs:2912-2955).

**Cargo.** `serde = ["dep:serde", "mara_core/serde", "slab/serde"]`. Add a `gpu`
feature forwarding to `mara_core/gpu` (P11 needs it; nothing enables it today
except `mara/Cargo.toml:60`, which is not in `default`).

**Delete dead egui surface.** `Graph::show(.., &mut egui::Ui)` (ui.rs:2882-2898).
`GraphWidget::get_selected_nodes`/`get_selected_nodes_at` (state.rs:621/630) —
zero callers *and* already broken per §3.

**Port the remaining egui-typed public API.** `draw_background` →
`&MaraPainter<'_>`. Delete `has_node_style`/`apply_node_style` (viewer.rs:62-83)
and their call site (ui.rs:2176-2178) plus `use egui::{Painter, Style};`
(viewer.rs:1). `GraphWidget::id` → `vocab::Id`. `nudge_saved_translation` →
`vocab::Vec2`. `NodeState::load/store/clear` → `&dyn MaraCtx` + Mara memory.

**Fix `impl Scale`** by adding `node_halo`, `pin_inset` and `wire_smoothness`
only. Do **not** scale `wire_glow`/`pin_glow` — they are unitless alpha
multipliers consumed as `a_mul * glow` (ui.rs:1325), so scaling them by
`max_scale` under `crisp_magnified_text` blows the bloom to full opacity — nor
`wire_color_mode`, a fieldless enum.

**Add `pub mod prelude`** and widen the re-export list with the real holes:
`PinWireInfo`, `WireStyle`, `WireLayer`, `SelectionStyle`, `NodeLayoutKind`.
(`NodeHalo` is already exported from both places.) `PinWireInfo` being
unreachable is why `mara/src/extras/graph.rs:182` can only ever write
`wire_style: None`.

**Makefile.** Add `cargo build -p mara_graph --features serde` to `make check` as
a permanent gate. Add a grep banning `egui::` in the files P4+ create. Add a grep
asserting every `pub use` in lib.rs appears in `mara/src/extras/graph.rs`.

**Verify.** `cargo build -p mara_graph --features serde` compiles for the first
time. `cargo test -p mara_core`: `lerp_scales_all_four_channels_uniformly_toward_transparent`.
`make check` still holds — `mara_core` has no egui edge, and
`! grep -RInE '(^|[^:a-z_])egui::' example/src/*.rs` still passes. A never-called
`fn _public_api_is_egui_free()` naming every public signature, in the style of
`_offscreen_path_is_reachable` (mara/src/extras/graph.rs:841).

*Effort: 1.5–2 days, mostly the `apply_node_style` retirement.*

### P3 · Stable identity and the wire index

`vendored/mod.rs`, `src/lib.rs`, `tests/model_identity.rs`

`NodeUid(u64)`; private `Node.{uid, frame, size_override}` with
`Graph::{uid_of, frame_of, size_override_of}` readers; `GraphExt` as one
`#[serde(default)]` field. Uid allocation in `insert_node` (mod.rs:243) and
`insert_node_collapsed` (:263). `uid_of`, `by_uid`, `revision`, `repair()`, plus
the missing conveniences `contains`, `len`, `is_empty`, `wires_of`,
`trim_wires_to`.

**Same pass: the O(nodes × pins × wires) fix.** Two multimaps on `Wires`,
maintained by all five mutators. **Change the hand-written `Deserialize`
(mod.rs:118-149) to build them as it reads the seq** — `#[serde(skip)]` is inert
on a hand-written impl, and an empty index after load is a correctness bug, not
a slow path.

**Verify.** `tests/model_identity.rs`, all `T = ()`, no rendering:
`uid_survives_slab_key_reuse` (insert A and B, remove A, insert C; assert C's uid
differs from A's former uid while C's `NodeId` equals it — the hazard shown
directly); `repair_is_idempotent`;
`repair_assigns_uids_to_a_document_deserialised_without_them`;
`serde_round_trip_of_graph_unit_is_lossless`;
`wired_inputs_agree_with_brute_force` and `wired_outputs_agree_with_brute_force`
after a randomised connect/disconnect/remove sequence over a seeded RNG;
`deserialised_graph_has_a_populated_wire_index`;
`trim_wires_to_removes_rather_than_orphans`.

*Effort: 1.5–2 days.*

### P3b · Headless input harness

`tests/harness/mod.rs`

Its own phase because it is a 1–2 day job and it **gates P5 and P8**: a headless
`egui::Context` driver feeding real `RawInput.events` — pointer moves, button
down/up, double-click with correct time deltas, keys with modifiers — modelled on
`crates/backend-egui/src/frame_tests.rs`. The existing harness sets only
`screen_rect` (render_characterisation.rs:85-90), so no interaction is testable
today, and the app may not be launched.

**Verify.** `synthetic_double_click_reaches_the_node_response` — a synthesised
double-click over a node's rect produces `double_clicked()` on that node's
response, proving the harness before anything depends on it.

### P4 · Frames — model and pure logic, no rendering

`vendored/frames.rs`, `vendored/mod.rs`, `src/lib.rs`, `tests/model_frames.rs`

Everything in §5's frame block plus the operations in §6. The two free functions
take a `&dyn Fn(NodeId) -> Rect` provider so they are testable with synthetic
geometry and cannot pull rendering into the model.

`fit_frame_bounds` = union(member rects) ∪ union(child frame bounds), expanded by
padding, then `min.y -= title_band_height(label_size)` — the T40094 fix.

Extend `remove_node` (mod.rs:299) to clear frame membership and bump `revision`.
Extend `repair()` to prune dangling `Node.frame` and break `Frame.parent` cycles.

**Verify.** `tests/model_frames.rs`, `T = ()`, zero rendering:
`removing_a_node_prunes_it_from_its_frame`;
`dissolve_reparents_members_to_the_frames_parent`; `purge_removes_members`;
`can_parent_frame_rejects_self_and_every_ancestor`;
`drag_targets_frame_is_transitive_and_deduped`;
`fit_bounds_reserves_exactly_one_title_band_above_the_topmost_member`;
`fit_bounds_grows_to_contain_a_nested_child_frame`;
`a_non_shrink_frame_ignores_member_movement`;
`frame_at_returns_the_innermost_frame`; `serde_round_trip_with_frames`;
`repair_breaks_an_artificially_cyclic_parent_chain`.

*Effort: 1.5–2 days.*

### P5 · Frames — rendering, dragging, adoption, resize

`vendored/ui/frame_paint.rs`, `vendored/ui.rs`, `tests/interaction_frames.rs`,
`tests/render_characterisation.rs`

The first half of the user-visible feature set, and it needs no `T` at all — the
three existing `NodeViewer` impls are untouched.

Extract `node_frame_rect_of<T,V>(..)` from `draw_node`'s prologue
(ui.rs:2050-2082) and call it from both the frame pass and `draw_node`.

> **Cost note.** That prologue calls `viewer.node_frame(default, node, &inputs,
> &outputs, graph)` (ui.rs:2062-2068), which needs `inputs`/`outputs` from
> `InPin::new`/`OutPin::new`. Calling the helper from both sites doubles that
> work *and* calls `&mut self` viewer hooks twice per node per frame
> (`DemoViewer::header_frame`, example/src/app.rs:6161, is exactly such a hook).
> `animate_bool` is time-based and idempotent; the viewer calls are not. Either
> memoise the pass results for the frame or accept and document the double call.

New `frame_paint.rs`, written entirely against `MaraUi`/`MaraPainter` through
`with_mara_ui` (ui.rs:2934). Insert **inline** between `viewer.draw_background`
(ui.rs:1161-1167) and the wire slot reservation (:1199-1202) — not through
`reserve_paint_slot`, or the title silently vanishes. Add a doc comment at that
site saying so. Paint innermost-last ordered by `frame_depth`, lightness
alternating ±6 % per level.

Register **only** the title-band interact and the eight resize handles, never the
body. Add `frame_moved: Option<(FrameId, Vec2)>` beside `node_moved`
(ui.rs:1169) and fan it out at the existing deferred site (:1604-1617).

Resize handles only when `!shrink`, hit rects sized `handle_px /
to_global.scaling`. Adoption per §7. Collapsed frames render as a pill and their
members are skipped in the node loop.

Add `GraphStyle::frame_style: Option<FrameStyle>` with all five requirements.

**Verify.** `tests/interaction_frames.rs` on P3b's harness:
`title_band_drag_moves_every_member_by_the_delta` and moves nothing outside;
`box_only_mode_moves_bounds_and_no_node`;
`dragging_the_frame_body_pans_the_canvas` (the click-through proof — assert the
stored `to_global.translation` changed);
`dropping_a_node_inside_adopts_it_and_outside_releases_to_the_parent_frame`;
`resize_handle_hit_rect_is_the_same_screen_size_at_zoom_0_5_and_2_0`.
render_characterisation, two-pass discipline preserved, **vertex counts not mesh
counts**: `a_frame_adds_geometry_whose_bbox_strictly_contains_its_members_bbox`;
`frame_geometry_precedes_node_geometry_in_index_order`;
`a_collapsed_frame_removes_its_members_geometry`;
`nested_frames_paint_two_distinct_fill_colours`;
`frame_title_contributes_text_geometry` (the guard against anyone later routing
it through a paint slot); `two_identical_passes_are_byte_identical`.

*Effort: 4–5 days.*

### P6 · Chrome descriptors, LOD ladder, camera spring

`vendored/{chrome.rs, camera.rs, ui/lod.rs, ui.rs, ui/background_pattern.rs,
ui/viewer.rs}`, `src/lib.rs`, `mara/src/extras/graph.rs`, `example/src/app.rs`

The genericity seam and the two mechanisms every later visual depends on.

`chrome.rs`: `NodeChrome`, `WireFx`, `Status`, `Badge`, `NodeShape`,
`DetailTier`, `Routing`, `Flow` — all `#[non_exhaustive]` from the first commit.
The fixed 8-slot palette as `Color32` constants. The defaulted
`NodeViewer::{node_chrome, wire_fx, decorate_node}`.

`camera.rs`: `CameraSpring` with `step(dt, rate)`, zoom in log2 space, driven by
`MaraCtx::dt()`. Route the existing double-click fit-to-content (ui.rs:1425) and
`GraphState::look_at` (state.rs:365) through it. `request_repaint_after` only
while settling.

`ui/lod.rs`: `tier_for(scale, ladder) -> (DetailTier, f32)` with smoothstep
bands, and `animations_enabled(scale)` gating at 0.35. Thread `DetailTier` into
`draw_node` and into `node_chrome`/`decorate_node`.

**Deferred from P1:** the layered selection halo and the header accent bar, the
latter now driven by `NodeChrome.accent`. Status ring: `PaintCmd::Arc` sweeping
270° at ~1.2 rev/s for Running, `CircleFilled` + glow for Error, 0.5 Hz alpha
pulse for Waiting, a 3 px bottom-edge `RectFilled` for progress. `Blob`-tier fill
uses the **status** colour.

`background_pattern.rs`: two-level grid with `alpha *= smoothstep(0.4, 0.8, s)`
and the 8-vertex vignette.

Demo: implement `node_chrome`/`wire_fx` on `DemoViewer` so the existing eval
graph shows live values.

**Verify.** Pure unit tests: `spring_converges_monotonically_and_never_overshoots`;
`zoom_halves_in_equal_time_steps` (the log-space proof);
`tier_boundaries_are_exact`;
`crossfade_alpha_is_continuous_across_every_band`. render_characterisation:
`blob_tier_paints_less_geometry_than_full_tier` (same graph, two scalings);
`pin_geometry_disappears_below_the_pins_tier`;
`the_vignette_adds_eight_vertices`;
`blob_tier_uses_the_status_colour_not_the_accent_colour`. A never-called
`_viewer_with_only_required_methods` fixture proving every addition is
source-compatible. `an_idle_graph_requests_no_repaints` via a `MaraCtx` test
double counting `request_repaint_after`.

*Effort: 4–6 days.*

### P7 · GraphDoc, ports, level navigation — nesting without collapse

`vendored/{subgraph.rs, nav.rs, mod.rs, ui.rs, ui/state.rs}`, `src/lib.rs`,
`tests/model_nav.rs`, `mara/src/extras/graph.rs`

Navigation before collapse, because id-scoping is the single thing that makes
nesting work at all and it de-risks everything downstream.

`subgraph.rs`: everything in §5 and §6's `GraphDoc` block, including the pure
`derive_ports` (no mutation — it is the testable core of P8) and the three port
mutators, each rewriting instance wires doc-wide **and calling `trim_wires_to`**.

`nav.rs`: `NodePath`, `NavState` with `save`/`load` keyed by
`mara_id(graph_id)`, mirroring `DrawOrder` (state.rs:182-198) rather than
widening `GraphStateData`; pruned on load by truncating at the first unresolvable
uid.

`show_doc` + `GraphOutcome`: clone the iface snapshot; resolve the level; compute
`level_id = graph_id.with(("lvl", &path))` and re-key the sublayer (ui.rs:1081),
`GraphStateData`, `DrawOrder`, `SelectedNodes`, the wire cache (ui.rs:1280)
**and node state (ui.rs:2049)** onto it. Call `viewer.begin_level(&path, def)`,
run the existing `show_graph` body, drain the `GraphIntent` queue after
`graph_state.store` (ui.rs:1619).

Add `entered: Option<NodeId>` to `DrawNodeResponse` (ui.rs:880-886), set from
`r.double_clicked()` at ui.rs:2113. `Esc`/`Backspace` pop. Breadcrumb returned as
data plus an optional crate-painted chip row on the **outer** ui, before the
sublayer is created (shrink `content_rect` at ui.rs:1073-1079) — inside the
sublayer it would pan and zoom with the canvas.

**Budget the `mara/src/extras/graph.rs` wrapper here.** The existing path is
~200 lines of embed/maximise/`node_view` plumbing (lines ~400-660) built around
`GraphWidget::show` on a bare `Graph<T>`, and `node_view::show` takes
`impl FnOnce(&mut egui::Ui)`. `show_doc` cannot reuse it, and Makefile:130 blocks
the example from bypassing it.

**Verify.** `tests/model_nav.rs`, `T = ()`, hand-built doc:
`level_mut_addresses_the_right_graph_at_depth_three`;
`breadcrumb_returns_the_full_chain`;
`a_path_whose_middle_uid_is_removed_truncates_on_load`;
`can_contain_rejects_direct_and_transitive_self_containment`;
`instantiate_beyond_the_depth_cap_returns_DepthLimit_not_a_panic`;
`instantiate_with_a_factory_returning_none_leaves_the_doc_byte_identical` (full
structural equality against a clone);
`reorder_port_moves_every_instance_wire_and_leaves_unrelated_wires_alone`;
`rename_port_touches_no_wire`; `instance_count_is_correct_across_nested_defs`;
`serde_round_trip_of_a_two_level_doc`. render_characterisation:
`entering_an_instance_shifts_the_painted_bbox_to_the_interior`;
`enter_then_exit_leaves_the_parent_to_global_unchanged` — seed two different
`Transform`s under the two `level_id` keys and assert the painted bbox differs,
which fails loudly if any consumer is still on the wrong key.

*Effort: 5–6 days.*

### P8 · Collapse and expand, shipped together

`vendored/{subgraph.rs, ui.rs, ui/port_paint.rs, ui/viewer.rs}`,
`tests/{model_collapse.rs, interaction_subgraph.rs}`, `example/src/app.rs`

The centrepiece. The algorithms are in §7. `make_local_copy` (Blender's Make
Single User), `promote`, `promote_frame`.

Defaulted `NodeViewer::{make_instance_node, make_port_node, show_port_pin,
on_structure}` plus the `ViewerFactory<'a, V>` adapter. New `ui/port_paint.rs`:
boundary nodes are ordinary nodes so pin layout and hit-testing apply verbatim;
this file only draws the crate-owned rename/remove/reorder menu.

Gestures: `Ctrl+G` → `Collapse{Local}`, `Ctrl+Shift+G` → `Shared` with a name
prompt, or `Expand` when the selection is a single instance. All push
`GraphIntent`. Requires P2's `modifiers_command`.

Demo: add `GraphNode::{Subgraph(DefId), Port(PortId, PortDir)}` variants,
implement the two minting hooks, and **key sim state by `NodePath` in
`begin_level`** so the shared-instance semantics are actually exercised.

**Verify.** `tests/model_collapse.rs`, `T = ()`:
**`collapse_then_expand_restores_the_exact_wire_set`** — the headline claim;
`two_external_sources_into_one_interior_pin_produce_one_input_port`;
`one_interior_outpin_feeding_three_external_pins_produces_one_output_port_with_three_wires`;
`port_order_is_deterministic_across_repeated_runs_on_a_shuffled_slab`;
`collapsing_a_selection_containing_an_instance_of_D_makes_the_new_def_depend_on_D`
and `collapsing_something_into_itself_is_rejected`;
`a_factory_returning_none_leaves_the_doc_byte_identical`;
`expanding_a_local_def_deletes_it_but_expanding_one_of_three_shared_instances_does_not`;
`collapsing_nodes_that_all_share_a_frame_leaves_the_instance_in_that_frame`;
`mixed_level_selection_is_rejected`; `serde_round_trip_of_a_collapsed_doc`.
`tests/interaction_subgraph.rs` on P3b's harness:
`ctrl_g_on_a_two_node_selection_reduces_the_parent_node_count_by_one`;
`double_click_on_an_instance_enters_it`.

*Effort: 5–7 days — port derivation and crossing-wire reconnection are the hard
parts.*

### P9 · Subgraph visuals and the portal dive

`vendored/{ui.rs, chrome.rs, camera.rs, ui/background_pattern.rs}`,
`mara/src/extras/graph.rs`, `example/src/app.rs`

Everything in §8's subgraph block. The dive target is a pure function
`dive_target(instance_screen_rect, interior_bbox, viewport) -> Transform`.
In-canvas exit chip as fallback; host-side breadcrumb chips consuming
`GraphOutcome.breadcrumb`, each segment clickable and emitting `ExitTo(i)`.
Add `subgraph_style: Option<SubgraphStyle>` with all five requirements.

**Verify.** Pure: `dive_target_maps_the_instance_rect_onto_the_interior_bbox_exactly`;
`dive_target_is_the_inverse_of_the_exit_target`. render_characterisation:
`an_instance_paints_three_rounded_rect_stroke_groups_offset_down_right`;
`depth_tint_changes_the_background_pattern_mean_colour_between_depth_0_and_2`;
`the_instance_badge_is_absent_when_instance_count_is_one`;
`exposed_pins_paint_two_circle_strokes_and_leaf_pins_one`.

*Effort: 3–4 days.*

### P10 · Wire richness

`vendored/ui/wire.rs`, `vendored/ui.rs`, `vendored/chrome.rs`,
`tests/wire_mesh.rs`

Everything in §8's wire block. Extend the existing `WiresCache` entry
(wire.rs:705-723) to hold the sampled polyline **and** its cumulative arclength
table. Replace the 5-pass glow (ui.rs:1318-1354) with one three-column mesh strip
— simultaneously a visual upgrade and a ~5× cut in the wire pass.

**Make wire iteration order deterministic.** Sort the visible subset by
`(out_pin, in_pin)` once per frame; `HashSet` order (mod.rs:199) makes
overlapping wires shimmer and defeats order-keyed caching.

**Verify.** `tests/wire_mesh.rs`, pure geometry:
`arclength_table_is_monotonic_and_its_last_entry_equals_the_summed_segment_lengths`;
`the_three_column_strip_satisfies_indices_len_equals_12_times_samples_minus_1`
(and passes `MaraPainter::mesh`'s own validity check at mui/mod.rs:872-879,
which **silently no-ops**, so a malformed strip renders as nothing);
`feathered_edge_vertices_are_premultiplied_transparent`;
`orthogonal_corners_stay_within_the_leg_budget`; `bus_width_scale_is_clamped`.
render_characterisation:
`replacing_the_glow_reduces_the_vertex_count_for_the_same_graph`;
`no_wire_mesh_is_emitted_below_zoom_0_35`;
`wire_paint_order_is_deterministic_across_identical_passes` (the shimmer guard).

*Effort: 4–5 days.*

### P11 · Perf hardening, node resize, viewport culling

`vendored/ui.rs`, `vendored/ui/wire.rs`, `tests/perf_invariants.rs`,
`example/src/app.rs`

Viewport-cull nodes before `draw_node` using `pos` + the cached `NodeState.size`
against `MaraUi::is_rect_visible` (mui/mod.rs:1584, currently unused here while
ui.rs:1215 draws every node unconditionally). Bbox-reject wires before `hit_wire`
(ui.rs:1272-1304) and before painting. Gate every animation behind an
on-screen-and-active check.

**Give `Node.size_override` its owner**: clamp in `NodeState::set_size`
(ui.rs:2743) and add node resize handles. For the AI-pipeline target a
user-resizable node is the actual missing feature.

Demo: an image-display node using `MaraUi::load_texture` (mui/mod.rs:1831 —
returns `None` on the recording backend, handle it) + `TextureHandle::set` to
re-upload in place keeping the id + `MaraPainter::image` inside `show_body`,
sized by `size_override`. That is the AI-pipeline target demonstrated end to end.

> **Interior thumbnails are deferred out of this plan.** A live miniature of a
> definition's interior inside its instance node (Max/MSP's bpatcher) is the
> remaining wow item, but the claim that `node_view.rs` "already drives that
> path" is **false** — `node_view.rs` owns its own `egui::Context`,
> `egui_wgpu::Renderer` and `wgpu::Texture` (node_view.rs:44-92) and never calls
> `ViewCtx::offscreen`. Moving thumbnails onto `render_offscreen` is a rewrite of
> `node_view.rs`, i.e. PLAN.md WS-D1.4, not a reuse of it. It also requires
> building the whole workspace with `mara/gpu` on, which is not in `default`.
> Revisit after WS-D1.4 lands.

**Verify.** `tests/perf_invariants.rs`:
`with_500_nodes_and_a_viewport_containing_10_the_viewer_is_asked_about_10` — a
**counting `NodeViewer`** whose `title`/`inputs` increment a cell, asserting 10
calls not 500. (Do *not* assert on emitted geometry: `offscreen_geometry_is_culled`
already passes today with zero culling code, because egui's tessellator drops
fully-clipped shapes — such a test would prove nothing.)
`two_identical_passes_over_a_500_node_graph_are_byte_identical`;
`no_animated_element_means_zero_request_repaint_across_three_consecutive_passes`;
`in_pin_new_performs_no_wire_set_scan` (an iteration-count invariant, not wall
time, proving P3's index is on the hot path);
`a_node_with_a_size_override_paints_a_wider_bbox`;
`image_node_degrades_without_a_texture_store`.

*Effort: 3–4 days.*

---

## 10 · Risks

- **Interior layout is genuinely shared, and this is the most likely bug report.**
  All instances of a chip show identical interior positions. Correct (Blender,
  Node-RED, every logic simulator) but surprising. Mitigated by the instance-count
  badge and `make_local_copy` — but if the demo does not exercise both, the first
  real user hits it cold. The price of Call 1.
- **Per-instance app state is only as correct as `begin_level` is used.** An app
  keying sim state by interior `NodeId` will silently alias all eight full-adders
  to one carry bit, and the crate cannot detect it. The single most likely way the
  shared model produces a wrong-looking simulation. The P8 demo **must** key by
  `NodePath`, and `begin_level`'s doc comment says why in one blunt sentence.
- **`show_doc`'s borrow conflict is the load-bearing structural risk.** The
  renderer needs `&mut Graph<T>` for the current level while needing every
  definition's interface, and the current level may itself be `doc.defs[d].body`.
  Resolved by the `IfaceTable` snapshot plus the deferred intent queue. If the
  snapshot ever needs more than counts/name/colour/revision, this gets expensive
  fast. Keep it strictly to those fields.
- **Reserved paint slots silently drop text, images and clips.** §3. The design
  avoids slots for anything but rect/shadow/mesh geometry, but any future
  contributor reaching for `reserve_paint_slot` to fix a z-order problem walks
  straight into it. P5 adds a doc comment at the call site and a render test
  asserting the frame title contributes text geometry.
- **Port reordering rewrites wires doc-wide and is the classic long-term
  breakage.** Blender rebuilt its entire socket API in 4.0 over exactly this class
  of bug; ComfyUI shipped duplicate-input and broken-connection regressions across
  several versions. The stable `PortId` plus a separate ordered `Vec` is the right
  shape, but the rewrite is where an off-by-one will hide. The flat model would
  make this class structurally impossible — that is the real cost of Call 2, taken
  knowingly and paid for with the heaviest test coverage in the plan.
- **Expand orphans per-path state by construction.** Interior nodes get fresh
  uids, so every `NodePath` under the expanded instance becomes invalid. The
  `UidRemap` makes migration possible but not automatic; an app that ignores it
  loses state on expand with no warning.
- **Interaction is untestable until P3b lands.** `render_characterisation.rs:85-90`
  builds `RawInput` with only `screen_rect`, and the app may not be launched.
  Double-click-to-enter, title-band dragging and adopt-on-drop are unverifiable by
  construction until the harness exists. This is why every decision is pushed into
  a pure function — `drag_targets_frame`, `frame_at`, `fit_frame_bounds`,
  `derive_ports`, `can_contain`, `dive_target` — with only single-line dispatch
  glue left in the renderer.
- **`GraphStyle` must stay `Copy`, and it is a soft trap.** One `String` or `Vec`
  breaks `GraphStyle`, `GraphWidget` and the `const fn` style setter at once, with
  errors at every call site rather than at the definition. P1 adds the const
  assertion so it fails at the struct instead.
- **P2 changes `mara_core`, which is outside this crate's blast radius.** All
  additions are additive except `FrameRole::Group`, which is semver-breaking
  because `FrameRole` is not `#[non_exhaustive]` and `frame_for` matches
  exhaustively. `make check` enforces the no-egui-edge invariant on `mara_core`;
  `modifiers_command` is populated in `backend-egui` where egui already lives.
- **`NodeViewer` grows twelve methods and loses two.** Every addition is defaulted
  and all three in-repo impls keep compiling, but the trait is already large and
  dyn-incompatible, and each addition raises the cost of finishing WS-D1.
  `node_chrome`/`wire_fx` deliberately absorb what would otherwise be a dozen
  separate hooks, and P2 retires the two surviving egui-typed methods, so the
  trait's net egui surface goes to **zero** while the method count rises.
- **Shipping collapse without expand** would repeat ComfyUI's most-complained-about
  gap. P7–P8 land together or not at all.

---

## 11 · Corrections applied to the design during review

Recorded so the provenance stays auditable. Each was found by the critic opening
the file or compiling, and verified by hand before this document was written.

- `Color32::premultiplied` **dropped** — `Color32` is already premultiplied
  (vocab.rs:689-703), so it would return `self`, and the stated rationale
  (transparent-black causing dark fringes) was inverted. `Color32::lerp` is
  specified as a linear per-channel lerp on premultiplied bytes, not a gamma-space
  mix.
- **Node state must be salted by level** (ui.rs:2049). The original rationale — "a
  node exists at exactly one level" — confused a node with a `NodeId`, which is a
  per-`Graph` slab index.
- **`Wires`' hand-written `Deserialize` must build the new multimaps.**
  `#[serde(skip)]` is inert on a hand-written impl; the original spec would have
  loaded an empty index, making every wire invisible.
- **P1's drag lift would never have fired** — `r` is shadowed at ui.rs:2176 —
  and its `CornerRadius` would have resolved to egui's.
- **P1 cut** to shadow + drag lift. The layered halo needs `SelectionStyle`
  exported; the header accent bar is meaningless before `NodeChrome.accent`.
- **P3 split**; the input harness is its own 1–2 day phase and gates P5 and P8.
- **`mara_core` additions are five, not three** — `CursorIcon::{ResizeNwSe,
  ResizeNeSw}` were missing from the count, and `FrameRole::Group` is breaking,
  not free.
- **Test assertions moved from `mesh_count` to `vertex_count`.** `mesh_count` is
  per tessellated texture/clip batch, not per shape, which is why every existing
  assertion already uses vertex counts.
- **P11's culling test replaced** — the original would have passed before the
  change, since egui's tessellator already drops fully-clipped shapes.
- **Interior thumbnails deferred out of the plan.** `node_view.rs` does not drive
  `ViewCtx::offscreen`; moving thumbnails there is WS-D1.4, not a reuse. Node
  resize took its place in P11.
- **`Node.frame` made private** — `get_node_info_mut` (mod.rs:388) would otherwise
  let an app write a foreign `FrameId` and skip the revision bump.
- **`RefCell` → `Cell` + eager counts** on `GraphDoc`, which must stay `Sync` for
  the Bevy resource path.
- **`NodeShape::{Card, Dot}` added** — a chromeless reroute node cannot be
  expressed at all today and is the most-used node type in the breadboard-CPU
  target.
- **`trim_wires_to` given an owner** (P3, enforced from P7) — the wire/pin-count
  reconciliation gap had no API, no phase and no test.
- **`pub mod prelude` added** — ~25 new public names across two export lists that
  nothing checks against each other, with a `make check` grep to keep them honest.
- **Three factual claims corrected**: `get_selected_nodes*` are on `GraphWidget`
  and are already broken; `ui.rs` has 29 `egui::` refs plus a 15-name bulk import,
  not 18; `NodeHalo` is already exported from both places, so the real holes are
  `PinWireInfo`, `WireStyle`, `WireLayer`, `SelectionStyle`, `NodeLayoutKind`.
