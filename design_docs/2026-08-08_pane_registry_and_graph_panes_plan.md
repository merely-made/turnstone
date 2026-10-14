# Pane Registry, Graph Views, and Shell Composition Plan

**Date**: 2026-08-08

**Status (2026-10-14)**: A4/S3 implements shared Workspace custody, Tabard appearance, keyboard, floating/docking, per-window accessibility, retained inline component bodies and native image apertures in one window scene. Ordinary published Mere/Genet/Netrender suppliers pass eight affected test groups, the native build and fourteen headed cases. Local-source receipts retain their measured basis. Physical accessibility, OS activation, monitor DPI and early scrollbar-fade pixel timing remain open. A0, A1 and A6 complete and verified. Lanes open: **A2** and
**A7**; **A3** retains its A2 acceptance gate. The A4 draft below does not close A2/A3 or A5 acceptance.

**2026-09-29 pane consolidation: qualified.** Mark accepted operational status
in Gloss, document and graph-object analysis in Inspector, and `mere-apparatus`
for shared diagnostics. The current migration retires Turnstone's redundant
Steward and Apparatus pane kinds after preserving their implemented capabilities.
The [Gloss plan](2026-07-20_gloss_composite_pane.md#current-direction-2026-09-29)
records the new default (minimap plus Downloads), scope and saved-layout behavior.
Facet-backed Downloads render in configurable Gloss, Inspector writes the
followed member's viewer through the product spine, and frame/lens restore plus
native routing passed, as recorded in the [September 29 qualification receipt](2026-09-29_shared_diagnostics_gloss_inspector_receipt.md). Shared observations and any new operation
controls have separate ownership; this pane migration adds no job runtime.

The Shared Knot seam that blocked A1's verification cleared on its own; the
concurrent lane landed the five reader exports. A second, unrelated blocker
appeared and is worth recording because Git cannot see it: a machine-local
`.cargo/config.toml` path override still named `mere/crates/persona/personae`
after that crate moved to `crates/dramatis`. Thirty-eight of thirty-nine
overrides were correct, which is how a silent patch bypass hides, and the
file is gitignored, so no commit repairs it for the next machine.

**Scope**: Make pane instances independent, restore real multi-graph composition,
generalize the window layout, and make the shell configurable without collapsing
graph truth, Workbench arrangement, pane layout, and chrome into one model.

The short ruling:

- A window is a **space** containing panes. It is not owned by one graph.
- A graph runtime owns graph truth. A Forme runtime owns arrangement and physics.
  A **Graph pane** is a view onto both.
- A **Workbench** is a durable arrangement of opened graph members, hosted in a pane.
- A pane may publish graph, member, session, or application context for followers.
- The **shell** owns commands, navigation, focus, notifications, and transcript data.
  **Chrome** is the configurable projection of those services into a space.
- Tiled containers are recursive. Floating panes are a sibling layer above the tiled
  root. Tear-out moves the same pane instance into another space.
- Roster, Inspector, Trail, Alembic, Comms, Gloss, Overmap,
  Publishing, and Settings keep their separate meanings.

---

## 1. Prior decisions this plan carries forward

| Source | Durable decision | Consequence here |
| --- | --- | --- |
| [Multi-Graph Activation](../../mere/design_docs/mere_docs/implementation_strategy/2026-06-09_multi_graph_activation_plan.md) and the archived [Window Composition Plan](../../mere/design_docs/archive_docs/2026-06-19_completed_plans/2026-06-11_window_composition_plan.md) | Graph authorities are pooled by `GraphId`; panes resolve through their own graph binding; one window may contain panes from different graphs. | Turnstone restores this model instead of extending its current single `App::canvas`. |
| [Rung 5 Panes](2026-07-14_turnstone_rung5_panes_plan.md) | The window pane tree and Platen Workbench nest because they arrange different identities. | They may share a topology algebra, while pane identity and member identity remain distinct. |
| [Surfaces in Cambium](2026-07-15_turnstone_surfaces_in_cambium.md) | Cambium owns the furniture inside a pane rect; Turnstone owns rects, mixed surfaces, routing, and composition. | `cambium::frisket` does not become Turnstone's outer renderer. |
| [Gloss Composite Pane](2026-07-20_gloss_composite_pane.md) | Composition is per pane instance and travels with it. | `PaneConfig` remains instance state; the registry does not absorb it into pane-kind globals. |
| [Frisket Direction](../../genet/docs/2026-07-24_frisket_pane_component_direction.md) | `genet-host-api::TileTree` is presentation vocabulary; hosts remain authoritative. Turnstone's mixed-surface presenter deliberately differs from the DOM presenter. | Reuse of `TileTree` is proof-gated and does not imply adopting the Cambium DOM view. |
| [Overmap](../../mere/design_docs/mere_docs/implementation_strategy/2026-07-20_overmap_sessions_graph_plan.md) | Overmap derives a session graph from manifests and lineage. | A multi-graph space changes focus and composition, not Overmap's underlying truth. |
| [Configuration Ownership](../../mere/design_docs/mere_docs/implementation_strategy/2026-08-06_configuration_ownership_settings_projection_plan.md) | The configured product owns typed storage; providers describe; Cambium renders; hosts apply. | `SettingsRef` is a pane source, not a graph-binding policy or a universal settings store. |

The external donors agree on the broad shape:

- [Rerun Blueprints](https://rerun.io/docs/concepts/visualization/blueprints)
  separate recorded truth from saved view hierarchy and per-view configuration.
- [`egui_tiles`](https://docs.rs/egui_tiles/latest/egui_tiles/) uses one recursive,
  app-payload-generic tree with explicit
  [container cleanup policy](https://docs.rs/egui_tiles/latest/egui_tiles/struct.SimplificationOptions.html).
- [Zellij layouts](https://zellij.dev/documentation/creating-a-layout.html) treat
  tiled panes, stacked panes, floats, chrome choices, and named layouts as data.

They are donors, not replacement ontologies:

| Donor | Take | Keep out |
| --- | --- | --- |
| Rerun | Truth/blueprint separation; nested Grid, Horizontal, Vertical, and Tabs containers; editable, file-backed, default and heuristic layouts | Using application-id coupling as Mere's space identity; Mere spaces can deliberately compose several graphs |
| `egui_tiles` | Generic payloads, recursive containers, grid movement, drag/drop, garbage collection, and explicit simplification rules | Making a runtime UI tree durable authority before Turnstone proves serialization and mixed-surface routing |
| Zellij | Named nested layouts; tiled/stacked/floating stations; [pane identity and pin/show/hide operations](https://zellij.dev/documentation/plugin-api-commands) | Treating a terminal scrollback buffer as the semantic shell transcript; allowing floats to become a second copy of a tiled pane |

## 2. Live contradictions

This is the initial A0 source audit, retained as implementation history. Its
singleton renderer and old pane-name findings are superseded where later progress
records per-instance retention and the September 29 Gloss/Inspector migration.

### 2.1 Graph identity exists in the model and is dropped by the host

Every `PaneNode::Leaf` already carries `graph_id`, and
`retag_graph_bound_from` explicitly preserves a second graph pane
([src/panes/layout.rs](../src/panes/layout.rs)). The current host then loses the
binding:

- `App` owns one `canvas` and one current `session_id`
  ([src/app/mod.rs](../src/app/mod.rs)).
- Session adoption replaces the canvas graph in place
  ([src/app/session_lifecycle.rs](../src/app/session_lifecycle.rs)).
- `PanePlacement` carries pane id, content, and rect, but not `graph_id`
  ([src/pane.rs](../src/pane.rs)).
- Every Orrery leaf becomes the same `SurfaceKind::Canvas`, with one fixed
  `SurfaceId::CANVAS` ([src/surface.rs](../src/surface.rs)).
- A lens borrows the same canvas and temporarily swaps only its viewport
  ([src/shell/lens.rs](../src/shell/lens.rs)).

Two Orrery leaves can therefore duplicate one canvas view, but cannot present two
independent graphs. The old Mere pool is precedent, not current Turnstone proof.

### 2.2 Pane identity exists and renderer state is keyed by kind

`summon_pane` mints a fresh `PaneId` on every summon
([src/app/pane_arms.rs](../src/app/pane_arms.rs)), while the shell retains one
Roster, Gloss, Trail, Inspector, Workbench, Apparatus, Settings, and Publishing
runner ([src/shell/mod.rs](../src/shell/mod.rs)). Sharing one runner across a pane's
tear-out stations preserves identity. Sharing it across two distinct pane ids
does not.

### 2.3 Pane registration is distributed

`PaneKind`, `PaneContent`, `pane_content`, labels, palette rows, render arms, input
arms, observation, and accessibility all repeat pane-kind knowledge. Settings and
Publishing use `Custom("settings")` and `Custom("publishing")`; layout mutation uses
`Custom("__placeholder__")`; `System` has no live constructor.

### 2.4 The Workbench and chrome are app-global

Turnstone holds one `App.workbench`, so two Workbench panes cannot own different
member arrangements. The omnibar likewise queries one `App::canvas`; its width and
top offset are constants, and only the primary window renders it
([src/ui.rs](../src/ui.rs), [src/chrome_view.rs](../src/chrome_view.rs)). Application
settings already name `shellbar_edge` and `shellbar_hidden`, but Turnstone does not
apply them.

### 2.5 `AppEvent` is not shell scrollback

`AppEvent` is a useful semantic observation stream, but it is partial, drained each
frame, and frequently carries display text without command identity, target context,
correlation, or a typed result ([src/observe.rs](../src/observe.rs)). A visible shell
transcript can consume those events; it cannot truthfully be a direct view over the
current stream.

## 3. Separate source, context, and view state

The earlier model overloaded `PaneBinding` with three different questions. The
replacement keeps them distinct:

```rust
struct PaneSpec {
    id: PaneId,
    kind: PaneKindId,
    source: PaneSource,          // what this pane presents
    context: ContextBinding,     // what this pane follows
    config: PaneConfig,          // durable per-instance configuration
}

struct PaneRecord {
    spec: PaneSpec,
    view: PaneViewState,         // pane-local camera, selection, scroll, UI state
}

enum PaneSource {
    Fixed(SourceRef),
    FromContext(SourceSelector),
}

enum SourceRef {
    Graph(GraphId),
    Forme { graph: GraphId, forme: FormeId },
    Member { graph: GraphId, member: GraphMemberId },
    Settings(SettingsRef),
    Session(SessionId),
    SessionSet,
    Application,
    External {
        schema: SourceSchemaId,
        payload: SerializedSource,
    },
}

enum SourceSelector {
    Graph,
    Forme,
    Member,
    Session,
}

enum ContextBinding {
    Own,
    Follow(PaneId),
    FocusedInOwnSpace,
    Application,
}
```

The exact Rust shapes may narrow during A0. The separation is the requirement.

### Context publication

A focused pane publishes the context it can honestly supply:

```rust
struct PaneContext {
    graph: Option<GraphId>,
    forme: Option<FormeId>,
    member: Option<GraphMemberId>,
    session: Option<SessionId>,
}
```

- A Graph pane publishes its graph and selected member.
- A Workbench publishes its graph and active member.
- A pinned member pane publishes its graph and member.
- Overmap publishes the selected session.
- Application surfaces publish no graph context.

`FocusedInOwnSpace` resolves to the most recently focused pane in the pane's current
space that publishes the needed field. Moving or tearing out the follower therefore
changes its focus scope without rewriting a stored `SpaceId`. `Follow(pane)` remains
attached to one context source. A following Roster, for example, uses
`PaneSource::FromContext(SourceSelector::Graph)` with `FocusedInOwnSpace`; pinning it
resolves that pair to `PaneSource::Fixed(SourceRef::Graph(id))` with `Own`.

The omnibar captures a context snapshot when it opens. A later focus movement cannot
silently redirect a command that is already being composed.

### Graph authority and Graph pane

Graph truth and projected arrangement are runtimes, not panes — the same
split the platform later ruled wing-wide (2026-08-28, the consolidation map
in mere's `2026-08-22_conatus_engine_plan.md`): realms own truth, projection
shows them, and physics is a stratified capacity living in projection space
here (Forme) exactly as seiche does on the canvas:

```rust
GraphRuntimePool: HashMap<GraphId, GraphRuntime>
FormeRuntimePool: HashMap<(GraphId, FormeId), FormeRuntime>
```

`GraphRuntime` owns graph truth and graph-scoped live resources. `FormeRuntime` owns
arrangement geometry and its physics, with shared geometry addressed inside it by
projection kind and layout id. A Graph pane owns a `PaneId`, camera, selection, and
other view intent. Two panes may show:

- two different graphs, using two runtimes;
- one graph through independent cameras and selections;
- one graph through a shared `FormeId` geometry;
- explicit arrangement forks, using different `FormeId` values.

The identity Orrery arrangement is computed from graph truth. It is not persisted as
if it were a curated Workbench. Curated member arrangements and explicit Forme forks
are durable; projection geometry may be rebuilt from them.

`SurfaceKind::Graph(PaneId)` replaces the singleton canvas surface. Render, input,
accessibility, content routing, and commands resolve the pane first, then its graph.

### Session rule

A session remains a durable graph-shaped unit with `SessionId -> root GraphId`.
A space may reference several sessions and graph runtimes at once. Focus chooses the
default target for switch, save, rename, and navigation actions. Overmap continues to
derive the session set from manifests. Broader cross-session relations stay with the
Overmap and murm/moot owners.

This minimum rule is owned here because a two-graph receipt cannot be implemented
while `App::session_id` still means the only graph allowed in memory.

## 4. The Workbench

The Workbench is a graph-bound desk: a durable, curated arrangement of graph members
promoted into directly usable content surfaces. It answers which members are open,
how they are split or stacked, and which member is active. It does not replace graph
truth or the window layout.

Platen already owns a recursive split tree of member tab-stacks and projects it onto
`genet-host-api::TileTree`
([Platen README](../../mere/crates/platen/platen/README.md),
[workbench.rs](../../mere/crates/platen/platen/src/workbench.rs)). Turnstone currently
hosts that tree inside one Workbench pane and composites live document surfaces into
its cells ([src/workbench_pane.rs](../src/workbench_pane.rs)).

The durable relationship is:

```text
GraphId
  -> FormeId / Workbench arrangement
       -> Workbench pane view
            -> member tiles and document surfaces
```

A window blueprint stores the Workbench pane and its
`PaneSource::Fixed(SourceRef::Forme { graph, forme })` source. The member arrangement
stays with the Forme/Workbench store. Closing its last pane makes the arrangement
dormant; reopening it restores the same member tree.

### Shared topology, separate authority

The useful generalization is a topology algebra parameterized by leaf identity:

```text
window composition  = LayoutTree<PaneId>
workbench content   = LayoutTree<MemberRef>
```

They may share split, tab, grid, drag, fraction, and normalization code. They do not
share ids, registries, or persistence owners. A Workbench member tile is pane-shaped
and may render as its own surface, but it is not a registered window pane.

The current hard sentence "Workbench cells are not panes" is therefore refined:
they are nested layout leaves keyed by graph-member identity rather than top-level
pane identity.

## 5. Space topology, nesting, and floats

```rust
struct SpaceBlueprint {
    panes: Vec<PaneSpec>,
    tiled: LayoutNode,
    floating: Vec<FloatingPane>,
    chrome: ChromeBlueprint,
}

enum LayoutNode {
    Pane(PaneId),
    Split {
        axis: Axis,
        children: Vec<LayoutBranch>,
    },
    Tabs {
        children: Vec<LayoutNode>,
        active: usize,
    },
    Grid {
        children: Vec<LayoutNode>,
        columns: GridColumns,
        shares: GridShares,
    },
}
```

Containers recurse, so nested splits and tabbed sublayouts are ordinary structure.
The model must define normalization after every edit: empty containers disappear,
single-child containers collapse according to policy, adjacent same-axis splits may
join, fractions renormalize, and focus moves to a surviving neighbor. The policy is
saved with or derived from the named layout, not scattered through gesture handlers.

### Nested panes

Three cases must keep different names:

1. A recursively nested **container** holds ordinary pane leaves.
2. A **layout-host pane**, currently Workbench, renders a second tree whose leaves use
   another identity type.
3. A pane containing another arbitrary `SpaceBlueprint` is a future embedded space.
   It waits for a real second consumer beyond Workbench.

### Floating panes

A float is the same `PaneId` presented outside the tiled tree:

```rust
struct FloatingPane {
    pane: PaneId,
    rect: RelativeRect,
    z: u32,
    pinned: bool,
    visible: bool,
}
```

Each `PaneId` occupies exactly one station across all live spaces: one tiled leaf or
one floating entry. Floating is a change of station, not a second appearance.

Docking, floating, and tearing out relocate one pane instance. They do not clone its
runtime. Floats are scoped to a space, composite above the tiled root, and below
modal chrome. Clicking raises a float. A pinned float stays visible when the float
layer is toggled. Geometry supports proportional and absolute constraints so a
layout survives window resizing.

### The `TileTree` decision

The existing `genet-host-api::TileTree` is more capable than Turnstone's binary
split tree in some ways, but it is not the complete target above:

- it has N-ary row/column splits and leaf tab-stacks;
- tab-stacks contain tiles, not arbitrary child subtrees;
- it has no grid or float layer;
- its current types are runtime presentation values rather than serde persistence;
- its own module contract says Mere projects Forme onto it rather than making it
  arrangement truth.

A0 therefore defines the required topology before choosing the reused type. A4 must
prove one of these outcomes:

1. Extend the shared contract because Pelt and Turnstone both consume the added
   topology without importing Mere semantics; or
2. Keep Turnstone's serializable `SpaceBlueprint` authoritative and project it onto
   `TileTree` where Cambium furniture benefits; or
3. Keep the Turnstone presenter and tree independent while sharing only split/tab
   state math.

The mixed-surface compositor remains Turnstone-owned in every outcome.

## 6. Pane registry and taxonomy

```rust
struct PaneDefinition {
    id: PaneKindId,
    display_name: &'static str,
    source_shape: SourceShape,
    uniqueness: Uniqueness,
    capabilities: PaneCapabilities,
    default_placement: PlacementPolicy,
    config_schema: PaneConfigSchema,
    config_codec: PaneConfigCodec,
    view_persistence: ViewPersistencePolicy,
    renderer_factory: RendererFactory,
}

enum Uniqueness {
    Many,
    PerSpace,
    PerSpaceAndSource,
    PerSpaceAndContext,
}
```

Each definition owns the typed config schema, default, and encode/decode boundary
for its pane kind. `PaneConfig` is not a universal untyped property bag.
`PaneSource` names content authority rather than repeating pane kind; the
definition's `source_shape` validates which source forms that kind accepts.

Initial rulings (2026-08-08, retained history):

The September 29 ruling supersedes the pane jobs below for Gloss, Inspector,
Apparatus and Steward: Gloss is the configurable navigation/operational overview,
new instances default to minimap plus Downloads, and Inspector owns document and
graph-object analysis. Steward and object-analysis Apparatus are retired in source;
old saved leaves restore as Gloss with Downloads and Inspector respectively.
The other source/multiplicity rulings retain their own implementation gates.

| Pane | Plain job | Source/context | Uniqueness |
| --- | --- | --- | --- |
| Graph / Orrery | Spatial graph view and context source | fixed Forme | Many |
| Workbench | Curated member workspace and context source | fixed Forme | Many |
| Tile | One pinned member surface | fixed member | Many |
| Gloss | Graph navigator and projection | graph context; per-instance composition | Many |
| Roster | Graph manifest | graph context | Per space and context |
| Inspector | Addressed-content identity and diagnostics | member context | Per space and context |
| Apparatus | Selected object's facets and representation | member context | Per space and context |
| Trail | Navigation chronology | graph/session context | Per space and context |
| Alembic | Durable memory and engrams | application/persona source | Per space and source |
| Steward | Operational activity | application/space source | Per space |
| Comms | Conversation | place/session source | Per space and source |
| Overmap | Session set and lineage | session-set source | Per space |
| Publishing | Publishing workflow | explicit target source | Many |
| Settings | Provider-described configuration | `Settings(SettingsRef)` | Per space and source |

`System` is removed. `Custom(String)` becomes a namespaced `External` source whose
schema owns decoding and validation. The layout placeholder becomes an internal
`LayoutNode` edit state rather than a fake pane. `PaneKind` retires; the registry
supplies labels, palette entries, availability, capabilities, uniqueness, and
renderer creation.

Renderer state is keyed by `PaneId`. Tear-out preserves the id. Closing the pane
evicts the retained runner; restore rebuilds it lazily.

## 7. Shell services, chrome, and scrollback

Chrome is not the pane tree, its dividers, or graph content. It is the visual
projection of shell services above or beside the space:

- omnibar and command providers;
- focus and command targeting;
- navigation controls;
- notifications and transient status;
- layout editing affordances;
- shell transcript projections.

The shell owns the data and actions. `ChromeBlueprint` decides which projections
exist and where they appear.

```rust
struct ChromeBlueprint {
    omnibar: OmnibarPlacement,
    shellbar: Option<ShellbarLayout>,
    transcript: TranscriptPlacement,
    status: StatusPlacement,
}
```

Placements may be summoned overlay, docked edge, floating pane, ordinary pane, or
hidden. Application settings choose defaults and shortcuts. A named space blueprint
stores the actual composition. Per-space transient state such as open/closed, current
query, selection, and scroll offset stays outside the reusable blueprint.

### Shell transcript

The transcript is a typed, bounded record of intentional shell interaction, rather
than a tracing console:

```rust
struct ShellEntry {
    id: ShellEntryId,
    input: ShellInput,
    resolved_intent: Option<ShellIntent>,
    target: ContextSnapshot,
    outcome: ShellOutcome,
    timestamp: Timestamp,
    privacy: EntryPrivacy,
}
```

`AppEvent` and asynchronous updates enrich entries through correlation ids. They are
also usable independently by diagnostics and automation. Gloss composes operational
status through product-owned section providers; Comms keeps conversation; the Shell
Transcript keeps the user's command,
navigation, and result history.

The transcript can project as recent omnibar history, a docked Command Log, a
floating pane, or an ordinary pane. Entries may be copied, repeated, or opened at
their original target. Retention and persistence are configurable. Storage is local
by default; providers redact secret-bearing input before it reaches the ledger.

## 8. Settings

Settings remains addressed configuration content:

- `PaneSource::Fixed(SourceRef::Settings(settings_ref))` selects the provider and
  page;
- `ContextBinding` states whether the page follows a context or remains application
  scoped;
- the configured product owns typed values and storage;
- `SettingsProvider` describes and applies them;
- Cambium renders controls from `SettingControl` rather than setting ids.

Turnstone's current provider still uses `pelt/appearance`, exposes only theme id,
theme mode, and zoom, and marks them `Live` without updating the running shell
([src/settings_provider.rs](../src/settings_provider.rs),
[src/settings_pane.rs](../src/settings_pane.rs)). The first completion slice corrects
the namespace and makes every advertised `Live` value observable immediately.

Inspector owns object-facing analysis and handling controls. Shared diagnostic
observations belong to `mere-apparatus` beneath product-owned views; diagnostic
observations remain distinct from settings. Presentation as tab, pane, float, or
modal is a chrome/layout preference over one Settings source.

## 9. Persistence ownership

| State | Owner and storage |
| --- | --- |
| Graph truth and graph-scoped runtime metadata | Session/graph stores keyed by `GraphId` |
| Forme identity, lifecycle, and curated Workbench member arrangement | Durable Forme/Workbench store keyed by `(GraphId, FormeId)` |
| Identity Orrery arrangement | Computed from graph truth rather than copied into a window blueprint |
| Shared projection geometry and physics | `FormeRuntime`, addressed by `(FormeId, projection kind, layout id)` and rebuilt from graph/Forme truth |
| Graph-pane camera, selection, and view intent | Live `PaneRecord` keyed by `PaneId`; the pane definition decides which fields snapshot into a blueprint |
| Pane source, follower rule, and instance configuration | `PaneSpec` in `SpaceBlueprint` |
| Tiled topology and floats | `workbench::Workspace`, serialized by Turnstone as the saved layout (A4 as revised 2026-09-04; was `SpaceBlueprint`) |
| Chrome composition | `SpaceBlueprint`'s `ChromeBlueprint`, until A6/A8 place it |
| Application shell defaults and settings | Application settings provider |
| Shell transcript | Bounded local transcript store under explicit retention policy |

Rerun's lesson applies directly: truth and blueprint are independently useful. A
layout can be saved, shared, reset, or heuristically regenerated without editing a
graph or Workbench arrangement. A Workbench source in a portable blueprint may name
a role such as "focused graph's default workbench" rather than exporting a private
local `FormeId`.

The repository is pre-alpha. The first registry/layout format may replace existing
`frame.json`; unreadable legacy layouts fall back to a logged default. Graph,
Workbench, settings, and transcript stores are not discarded with it.

## 10. Implementation gates

### A0. Pure model and inventory

**Completed 2026-08-09.** The live pane audit is recorded in
[Pane Inventory](2026-08-09_pane_inventory.md). The rendering-free model lives in
`src/panes/blueprint.rs`; focused tests prove context resolution and pinning,
recursive normalization, tiled-to-floating relocation, cross-space movement,
global pane-station uniqueness, Forme/graph validation, and serialization.

- Inventory every current pane's source, published context, follower rule,
  uniqueness, instance state, capabilities, renderer, persistence, and evidence.
- Define `PaneSpec`, `PaneRecord`, `PaneSource`, `ContextBinding`, `PaneContext`,
  `SpaceBlueprint`, normalization, and focus resolution as data-only types.
- Add model tests for nested containers, focus-derived context, pinning, float moves,
  cross-space PaneId uniqueness, normalization, and serde round trips.

Done when graph/source/context/multiplicity are explicit and no rendering code is
needed to prove the state transitions.

### A1. Registry and instance correctness

**Complete and verified 2026-08-10.** The built-in registry now
owns stable ids, labels, source validation, multiplicity, capabilities, palette
availability, schemas, legacy construction, and renderer keys. `PaneKind`, `System`,
magic custom-pane strings, and fake layout leaves have been removed. Retained
Cambium runners are keyed and lazily created by `PaneId`; closing a pane or lens
evicts only its instances. Focused model/registry tests pass. The full library
currently stops on unresolved Knot share-reader imports outside this lane, before
crate tests can execute.

- Register existing panes and retire `PaneKind`, `pane_content`, `System`, magic
  custom strings, and the fake placeholder.
- Use Publishing as the plain workflow proof.
- Key retained runners by `PaneId`; preserve ids across docking and tear-out; evict
  on close and rebuild lazily on restore.

Done when adding a Publishing-like pane requires one registration plus its renderer,
and two supported same-kind panes retain independent scroll, selection, and controls.


### Running the open lanes side by side

A2, A6, and A7 are independent by design, but the tree is not: several agents
already commit here, and this session watched work swept into the wrong commit
twice and a red test appear in a file no lane had touched. Three rules, cheap
to keep and expensive to retrofit:

- **Each lane owns its files.** A1 owns the landed external source identity;
  A2 owns the graph pool and graph surface runtime identity (`PaneId` and
  `graph_id`); A6 owns chrome, omnibar, and transcript; A7 owns Cambium
  components and settings. Port surface admission is tracked separately in
  `mere/design_docs/mere_docs/implementation_strategy/2026-08-24_knot_shared_surface_and_port_contribution_plan.md`.
  Where a lane must touch a shared file, it touches only its own items in it.
- **Commit your own paths, not the tree.** The whole-tree default is right for
  one agent and wrong for three. Where a sweep happens anyway, name what was
  swept in the message, as this repo already does.
- **A red test outside your lane is a report, not a task.** The current
  `publish_pane::tests::unavailable_panel_is_an_honest_configured_surface`
  failure belongs to the Shared Knot lane and is outstanding; it is not A2's to
  silence.
### A2. Graph runtime pool and two-graph composition

**Open.** A1, its dependency, is signed off.

This lane does not own port-provider factories, retained-session erasure,
command or settings contribution, or live capability facts. A contributed
surface depends on A2 only when it needs multi-graph context.

- Replace `App::canvas` and exclusive `session_id` routing with a graph runtime pool
  plus Forme runtimes and focused-space context.
- Carry `PaneId` and `graph_id` through placement, surface planning, render, input,
  content contributions, accessibility, commands, save, and observation.
- Replace singleton Canvas focus/surface identity with pane-addressed Graph surfaces.

Done when graph A and graph B render side by side in one window, both receive pointer
and keyboard input, graph A mutation leaves B unchanged, and restart restores both.

### A3. Multiple views and Workbenches

**Gated on A2.**

- Allow two Graph panes over one graph with independent cameras and selections.
- Create or reuse the `FormeRuntime` named by each Graph and Workbench pane's Forme
  source.
- Share geometry only when Forme, projection kind, and layout id agree; keep camera,
  selection, and other view intent keyed by `PaneId`.
- Key Workbench state by graph/Forme source instead of `App.workbench`.
- Publish context from Graph, Workbench, and member panes; make Roster and Inspector
  follow explicit or focused context.

Done when one graph appears through two independent views, two Workbenches can show
different arrangements, and Inspector follows the active member of the intended
Workbench rather than a global canvas.

### A4. Containers, tabs, and shared-tree decision

**Status (2026-10-12): implementation draft for the tiled path; acceptance open.**
The earlier held checkpoint is superseded for this bounded draft. The tree
decision, not the tab decision: Turnstone's tree is binary, `TileTree` is
N-ary and recursive, and nested splits and nested panes both want that
recursion.

**Revised by Mark, 2026-09-04: outcome 1.** The 2026-08-11 decision below
kept a Turnstone-private topology because the shared contract could not own
mixed-surface routing, persistence, or stable pane identity. Mark's ruling:
those are the stack's to own, so the contract is extended rather than
worked around — `workbench` gains serde and the A5 float layer, Cambium's
`tab_strip` gains a close affordance, frisket gains a slot resolver, and a
`cambium::workspace` composition wires them. Turnstone then renders **one
frame per window**: its own panes as `Component`s in that frame, contributed
panes through the retained-surface trait, the graph canvas and WebViews as
holes it composites. Mark ruled one renderer over the first draft's
host-composited strip surfaces (2026-09-04): the host's own content is not
a hole. The user-facing result:
every tiled pane wears a tab bar (a one-tab stack is the sub-title bar), each
tab closes from its ×, panes stack by dragging one onto another, and the
layout is saved and restored. `SpaceBlueprint`, the binary `PaneNode` tree,
`legacy_bridge.rs` and `frame.json` retire; `Grid` is not ported, since
nothing constructs one. The stack half and the done-conditions live in
`mere/design_docs/cambium_docs/implementation_strategy/2026-08-31_workbench_component_plan.md`
(W5); the revisit trigger it fires is
`genet/docs/2026-07-24_frisket_pane_component_direction.md` follow-on §2.
Turnstone's half starts only after mere is pushed and this repository's
pin moves — Mark's steps.

**Decision, 2026-08-11: outcome 3 (superseded above).** `SpaceBlueprint` remains Turnstone's
serializable topology authority. `genet-host-api::TileTree` stays a useful
presentation donor for Cambium furniture, but it cannot own mixed GPU/document/
Cambium surface routing, persistence, stable pane identity, or the inactive-tab
lifecycle rule. Turnstone shares split state math where it helps and projects its
own active layout into the outer compositor and accessibility tree. No shared
contract is promoted until a second non-Mere host needs the whole topology.

- Prove nested N-ary splits and a tabbed subtree over mixed Graph, document, and
  Cambium surfaces.
- Compare the required `SpaceBlueprint` operations with `TileTree` and record outcome
  1, 2, or 3 from section 5.
- Keep inactive tabs out of render, hit testing, accessibility focus, and pumping
  unless their content lifecycle explicitly remains warm.

Done when a nested mixed-surface layout survives drag, resize, save, reload, and
tear-out, with one authoritative topology and the fallback recorded.

### A5. Floating layer

**Held** behind A4; floats layer onto whichever tree wins.

- Add float geometry, z-order, pinned visibility, focus raise, dock targets, and
  tear-out from a float.
- Reuse the same PaneId and renderer state through every station.

Done when one pane moves tile -> float -> nested split -> window and returns without
losing state or leaving an unreachable runner.

**Implemented 2026-08-12 (headed desktop receipt).** `SpaceBlueprint` owns
constrained proportional float geometry, per-space z-order, pin and visibility
policy, focus raise, dock targets, and transactional float tear-out. The legacy
Frisket layout is bridged one-way into that authority on the first float, and
the primary and lens compositors then project the live blueprint into their
surface plans. `scenarios/a5_floating_pane.scn` proves tile -> float -> nested
split -> lens -> return -> dock, retaining one `PaneId` and its renderer state;
it captures both spaces and drives real pointer focus raising. The receipt is
headed and passed on 2026-08-12.

Frisket still provides the existing leaf payload and at-rest layout path. Float
station topology and geometry remain transient until A8, so this is not yet a
persistence or raw drag-gesture proof.

### A6. Shell and configurable chrome

**Complete 2026-08-14.** 244 library tests pass.

This lane was marked Open in error. Most of it was already built when the
marking was made -- `ShellServices`, provider registry, shortcuts, omnibar
config, `ChromeBlueprint`, and a `ShellTranscript` with correlation, privacy,
retention and repeat -- and the marking came from reading this document rather
than the tree. Recorded because the same mistake was then made twice more
inside the lane: a `repeat_shell_entry` was written before discovering the one
that already existed, and a critique earlier in this plan asserted a reuse
target that the dependency graph forbade. Read the code before claiming a lane
is open.

Two clauses of the done-condition were genuinely unmet, and both are now
closed:

- **Nothing rendered the transcript.** Its `ChromePlacement` defaults to
  `Hidden` and no projection existed, so a command could not be "repeated from
  a docked or floating transcript" -- there was nothing to repeat it from.
  `transcript_pane` is that projection, registered through the registry as its
  second proof after Publishing (one registration plus a renderer), and wired
  through render, input, and the probe driver. Five receipts, two of them about
  restraint rather than capability: a pending entry is inert, because repeating
  a command whose first run has not landed doubles an effect by accident; and a
  redacted entry's text cannot be found by the resolver a scenario runner aims
  with, which is a stronger claim than not printing it.
- **Chrome never proved it restores.** Every blueprint test built
  `ChromeBlueprint::default()`, so the field rode through serde with no test
  able to tell a restored composition from a fresh default. Pinned now with
  four deliberately different placements.

Left for whoever takes chrome further, and deliberately not built here: the
transcript projects only as an ordinary pane. `Docked`, `Floating`, and
`Overlay` placements are declared in `ChromeBlueprint` and honoured by
`ShellChromeConfig`, but no host code positions a docked or floating transcript
yet, and floats as a structure belong to A5.

- Split shell service state from Chrome projections.
- Add provider registration and configurable omnibar placement, row limit, default
  scope, shortcuts, shellbar edge/visibility, and transcript placement.
- Add `ShellTranscript` with correlation, privacy, retention, and repeat/open actions.

Done when a command captures its original pane context, produces one correlated
result entry, can be repeated from a docked or floating transcript, and the chrome
composition restores from a named layout.

### A7. Cambium and Settings completion

**Open.** Self-contained; runs beside A2 and A6.

- Promote pane shell/header, settings form, empty/error/unavailable state, and the
  relevant Frisket specimens into Cambium's component catalog.
- Correct the Turnstone Settings namespace and render generically from
  `SettingControl`.
- Connect every `Live` setting to the running host; label restart/startup settings
  honestly.

Done when catalog specimens cover narrow and regular layouts, Turnstone consumes
them, theme/zoom/shell placement change live, and values survive relaunch.

### A8. Named blueprints

- Save, load, duplicate, rename, reset-to-default, and reset-to-heuristic layouts.
- Keep private ids out of portable exports; resolve role-based sources on import.
- Preserve graph, Workbench, Settings, and transcript stores across layout changes.

Done when Browse, Inspect, and Operate are editable examples rather than modes, and a
layout can be shared or reset without changing graph/session truth.

## 11. Stop rules

- Keep Turnstone's mixed-surface compositor as the outer presenter.
- Treat graph authority, Graph pane view state, Workbench arrangement, and window
  blueprint as separate owners even when they share tree or surface primitives.
- Keep PaneId stable across dock, float, and tear-out; use member identity inside a
  Workbench.
- Require a second consumer before promoting arbitrary embedded-space hosting or new
  shared topology into Genet.
- Keep `AppEvent` useful for observation; create a correlated transcript rather than
  changing it into a UI store.
- Keep generated proof outputs on disk and outside Git.
- Stop a shared-`TileTree` migration if it cannot preserve mixed-surface rendering,
  persistence, instance identity, and input authority in one completed proof.

## 12. Deferred decisions

- Write-side third-party pane registration through the participant gate.
- Stored cross-session Overmap relations beyond manifest-derived lineage.
- Persisting and restoring versioned contributed `PaneSource` values and their
  user-configurable settings. T5a's Sky source is intentionally immutable
  opening provenance; its valid draft survives only for the retained pane's
  current process lifetime.
- Product display-name changes such as Canvas versus Orrery and Navigator versus
  Gloss.
- Default layout details. Defaults remain editable and user-configurable.

## Progress

- 2026-10-06: received from Mere's S14 archive pass (Mere rulings S60 and
  S69). Mere archived its workbench component plan
  (`mere/design_docs/archive_docs/2026-10-06_completed_plans/2026-08-31_workbench_component_plan.md`); its S3, Turnstone's
  compositor walking the shared Workbench tree, belongs to A4 here. The pin it
  waited on (`bd5912fb`) is satisfied, and adoption has not started. Mere also
  archived the Knot shared surface plan, whose T lane noted a pointer-capture
  routing gap (2026-09-05): `ContributedSurface::pointer_move`
  (`src/contributed_surface.rs`) still dispatches moves without consulting the
  session's `pointer_capture()`. No Turnstone plan names that gap; it is noted
  here because this plan owns the contributed-pane path. Recorded in Mere's archived plan tails plan (`mere/design_docs/mere_docs/implementation_strategy/2026-07-03_archived_plan_tails_plan.md`, section "2026-10-06 archive pass").

- 2026-09-04: A4 revised to outcome 1 on Mark's ruling, from a screen full of
  panes with no way to close them: the pane frame becomes Workbench's tree
  with tabs, floats and serde, and Cambium's strip gets the close ×. The
  reasons outcome 3 gave are now the stack's work list. Also found this
  session, in a file the physics receipts touched: the `wait` verb returned
  before the layout settled because `busy` never asked the canvas; fixed in
  `39339e8`.

- 2026-08-26: the Sky T5a consumer exercised the generic contributed-pane path
  with a second concrete provider. Per-`PaneId` retention and versioned source
  admission are proven. Restart restoration of the source remains an A8
  blueprint/persistence concern rather than product-owned state in the Sky
  session.

- 2026-08-24: corrected the port-contribution accounting. A1 already owns the
  namespaced `External` source identity; A2 remains the graph runtime pool and
  `PaneId` / `graph_id` propagation lane. The shared Knot plan owns provider
  description, admission, and retained-session erasure.

- 2026-08-16 (multiplicity, found by looking at a screenshot): Device Receipts
  was registered `PaneMultiplicity::Many` while sourced from
  `FixedSourceKind::Application`. There is exactly one application, so a second
  pane could only ever render identical content, and because `summon_pane`
  deliberately skips the dedupe for `Many` kinds, every "Open Device Receipts
  pane" split the active pane and added another copy. Seven were reachable in
  one window. Now `PerSpace`, matching Steward, which is the only other
  `Application`-sourced pane and was already correct.

  **The rule the table now states consistently:** multiplicity has to agree
  with the source shape. A pane whose source is fixed to a singleton
  (`Application`, `SessionSet`) is `PerSpace` at most; `Many` is for panes
  whose source distinguishes instances (`Member`, `External(..)`, a `Forme` a
  reader may legitimately want two views of). The remaining `Many` rows all
  pass that test: Orrery and Workbench over `Forme`, Tile over `Member`,
  Publishing over a target, Shared Knot over a ticket.

  Worth keeping about how this surfaced: the duplicate panes were visible in a
  headed capture taken for an unrelated receipt, and went unremarked because
  the capture was being read for the one row it was taken to prove. The cost of
  the duplicates was separately real (see the layout retention entry in
  [`2026-07-15_turnstone_surfaces_in_cambium.md`](2026-07-15_turnstone_surfaces_in_cambium.md):
  each copy cost 24 ms a frame).

  Still open, and named here rather than left implicit: **a pane has no close
  affordance.** `Action::CloseActivePane` exists, reachable as the palette row
  "Close pane" and scriptable as `close_pane`, and it acts on the active pane.
  There is no control on the pane itself, no keybinding, and nothing marks
  which pane is active, so with several panes open there is no way to tell
  which one is about to close.


## 2026-09-29 findings and progress: operational overview and Inspector

- Verified `steward_pane.rs` had only inert download custody rows. The Downloads
  section now gathers those graph facets in `sections.rs`; no authority or
  operation control moved into a diagnostic store.
- Verified object facets, classifications and import provenance already lived
  in `inspector_view.rs`. The useful `apparatus_pane.rs` capability was the viewer
  radio and document capability/zoom readouts. Those now live in Inspector;
  `inspector_controls.rs` gathers facts from its followed member.
- Retired renderer and command paths after migration. `saved_content.rs` lowers
  old leaf tags immediately to current pane content, preserving actual user
  frame/lens arrangements; new saves contain current names only.
- Qualified by the [September 29 qualification receipt](2026-09-29_shared_diagnostics_gloss_inspector_receipt.md):
  the locked, offline workspace gate passed 612 tests with 9 ignored; five exact
  Piccolo/Wasm tests and 33 UI/Inspector regressions passed. The default-feature
  native build and fresh/restart scenarios passed, with three reviewed nonblank
  captures and persisted section order, Inspector and requested viewer.
- This bounded receipt leaves custom-leaf semantic parity, full built-in pane
  selectors, document selection, exact frame correlation and human AT open.

## Qualified bounded slice: semantic automation for contributed panes, 2026-09-30

**Status: implemented and machine/native qualified.** The coherent combined
Knot/Woodshed dependency gate and this contributed slice pass. The final workspace
gate passes 635 tests with nine existing ignores; five exact optional participant
tests pass separately. Three reviewed native Sky captures prove real scoped
delivery, including Calculate request 3 held while clipped and delivered after
scrolling. The helper-level admission refusal regression fails before repair;
the stale-generation and missing Gloss platform-arm controls fail deliberately
and exact source restoration passes. See the
[September 30 consumer receipt](2026-09-29_shared_diagnostics_gloss_inspector_receipt.md#september-30-qualified-consumer-cohort).
All built-in panes remain a later qualification slice. The shared
[Mere semantic and diagnostics plan](../../mere/design_docs/mere_docs/implementation_strategy/2026-06-08_system_diagnostics_and_accessibility_plan.md)
owns projection matching and evidence boundaries.

### Existing owner seams

The pre-slice `shell/drive.rs::with_surfaces` exposed contributed sessions with
`ProbeSurface.name = "contributed"`, their provider-owned DOM and complete
stylesheet. Shell inherited Taproot's raw role/name matching and a fresh layout
for resolution. Shipping paint and hit testing use `ui.rs::RetainedLayout`.
That distinction can place an automation click incorrectly after pane scrolling.

Contributed panes already expose their same provider DOM and retained fragments
through `ContributedSurfacePane::accessibility_tree`. `contributed_a11y.rs`
namespaces DOM nodes by `PaneId`, translates bounds and records action routes
into the actual platform tree. This is the qualification seam. Built-in panes
currently use separate app/frozen projections, while live content exposes a
partial structural outline; neither is covered by this contributed-pane proof.

### Minimal explicit scope

Taproot's `Selector` now has an optional generic surface-name filter, with
`.on_surface(...)` and a leading `surface:<name>` selector token. For example:

```text
click surface:contributed role:button Reset all controls
click surface:contributed .reset-control
```

The name is the existing `ProbeSurface.name`, a machine-facing host attribution,
not a new pane label or product UI. The remaining selector grammar keeps its
existing role/class, text and attribute meanings. Existing unscoped scenarios
and constructors remain compatible; no scope means their present behavior.
Implementation must inventory direct `Selector` construction before changing
its shared type, and preserve other hosts through optional defaults.

The scope selects a surface family, not a unique pane instance: every contributed
pane currently has that same name. Scoped semantic resolution must therefore
reject multiple matching contributed targets rather than choose a different
product by iteration order. Existing DOM attributes may narrow a selector where
the product already supplies stable identity; never infer scope from its label.
An unknown scoped name or an authoritative semantic miss is a final
`SelectorTarget::Miss`. It must not reopen raw matching in unrelated surfaces.
Unscoped legacy selectors and explicit class/text behavior remain separate from
the new qualification claim.

### Ownership and retained delivery

1. **Genet/Taproot:** add the optional generic scope parsing/filtering seam.
   Use `matching_with_projection` for scoped role/name requests, preserving
   explicit DOM class/text and attribute compatibility. It does not compute
   product labels or decide pane routing.
2. **Turnstone retained layout:** expose the renderer-neutral Genet document
   projection and painted/visible rectangles from the existing retained
   fragments and scroll state. Accessibility and automation consume the same
   computed semantic source. Do not lay out a second tree to resolve a click.
   Visibility must respect the viewport and applicable ancestor clipping.
3. **Turnstone contributed sessions:** retain target identity as pane, admission
   generation and DOM node. Assign a new bounded monotonic generation on session
   replacement, including removal/recreation of the same source; a reused local
   `NodeId` alone cannot authorize a held click. Preserve actual provider state,
   event handling and action refusal.
4. **Turnstone Shell:** implement scoped `selector_target`/`click_target` against
   those readings. For an offscreen match, scroll the owning pane, request a
   frame and hold `scenario_pump` until the new layout exists. Then revalidate
   generation, DOM identity, selector, visibility and action availability before
   delivering the pointer through the existing surface route. Hidden, renamed,
   removed, repinned or still-invisible targets fail explicitly. Report the
   failure in the requested scenario receipt, with a bounded wait.

Apparatus may retain product-redacted observations of these checks. That does
not invent operation IDs, computed revisions or pixel-frame correlation. Human
screen-reader evidence remains a separate gate.

### Required controls and done condition

- A contributed provider fixture has visual child text `Restore`, a conflicting
  `aria-label`, and a referenced accessible name `Reset all controls`. The scoped
  selector and AccessKit Click route identify the same DOM node and each exercise
  the shipping provider action exactly once. A missing first-layout projection
  is an honest miss. Substituting raw matching must fail this named-label test.
- A short scrolling pane places `Far` below the fold. The scoped click scrolls,
  waits for layout, then increments once; the following scenario step cannot
  race delivery. Explicit class/text selection keeps its original meaning.
- Rename, hide, remove and repin while a click waits. Each fails without
  activating a replacement. Two panes deliberately reuse local DOM IDs and
  labels to prove identity scoping and ambiguous-match refusal. Bypassing the
  admission-generation check must fail the repin control.
- A disabled control never increments. A nonexistent scoped role/name cannot
  fall through to an identically named raw control elsewhere in the window.

Done requires passing the generic scope compatibility tests, retained semantic/
scroll/stale fixtures and a native contributed-pane scenario receipt against
the coherent dependency pins. Record the negative-control failures, restore the
source and rerun the focused gate. This closes only contributed-pane automation
qualification; built-in pane migration, full document semantics, exact presented
frame correlation and human AT remain open.

## Next Inspector surface: ownership before semantic routes, 2026-09-30

This is a researched next slice, separate from the contributed/Diagnostics
qualification cohort. The working recommendation is to bind Clip to the
displayed document before exposing it through shared semantics; Mark's optional
scope prompt is pending. It is not a completed implementation or a new ruling.

Inspector already derives its followed graph/member in `inspector_view.rs` and
owns retained DOM/layout in `inspector_pane.rs`. Viewer radios expose real
radio roles, names and checked state through Cambium. Reuse that retained
projection for platform lowering and scoped Taproot selection, with actual
visible bounds. Read-only handling facts remain non-actionable; missing members,
sources or endpoints must produce disabled/no-action control state.

Two existing authority gaps precede the route:

- Clip availability uses the followed member, while unit `ClipToKnot` reaches
  `clip_focused_document_to_knot` and the global cursor. Bind the intent to the
  displayed member, current content-session admission and exact Knot endpoint
  admission; revalidate them before reading that member's current clip. Preserve
  ordinary `DocumentClip`, retained source artifacts and `KnotClipHandle.insert`
  custody. Matching a URL or endpoint label cannot establish an unchanged source.
- Viewer sidecars are written against the requested member, but live respawn
  finds its URL through the active legacy graph. Resolve the owning runtime and
  reject retired/nonmembers before sidecar or content mutation. Preserve domain
  `SetViewerOverride` and `SaveSession` authority.

Action tokens need pane, followed context, changed-binding generation, member
and DOM identity. Removing/recreating the pane, switching A/B/A, replacing a
source/session or replacing a same-label endpoint must retire old tokens.
Inspector focus/key delivery is currently absent; advertise keyboard behavior
only after actual runner focus/key dispatch and intent collection are wired.

Done requires followed A versus cursor B in the same and different graphs,
pointer/queued AT parity, hidden/disabled/stale controls, deliberately bypassed
binding guards that fail, and native persisted viewer plus source-bound Clip
receipts. The displayed A's body/provenance must reach the actual endpoint while
B and unrelated destination authority remain untouched. Human AT and immutable
presented-frame observations retain separate gates.


## A4/S3 tiled Workspace adoption draft — 2026-10-12

**Status:** implementation draft for review and native qualification. Mark asked
for the draft to be pushed to make the adoption concrete. This is the first
consumer step; it does not claim W5/S3's complete one-composition outcome.

**Findings.** Inspected current Turnstone main `411ad007`: outer panes still used
binary `PaneNode`/`FrisketLayout`, separate retained renderer maps and the
`frame.json`/`windows.json` sidecars. The inner Workbench pane already presents
Forme-backed content tiles. Its arrangement is a different owner from the
window's pane arrangement. Mere pin `74c1d810` already includes the shared
Workspace and Frisket frame; the pin remains unchanged.

**Implemented phase.** `src/panes/workspace.rs` holds the shared Workspace beside
opaque host payloads keyed one-to-one by the existing pane id. Pane content and
graph bindings stay in those payloads. `workspace_bridge.rs` imports binary
nodes without retargeting those bindings; `PaneNode::Workspace` becomes the
tiled runtime after import or an accepted legacy arrangement operation. Old
Leaf/Split constructors remain for actual legacy input and current pane
admission calls. There is no second mutable tiled topology inside that variant.
All retained tabs are enumerated for custody; only active tabs have bodies.
Unknown targets, duplicate stations and missing payloads refuse atomically;
outside drops keep custody until the existing host tear-out accepts it.

`src/outer_frame.rs` renders shared `cambium::workspace_view` furniture. Primary
and lens surface plans use its retained DOM content apertures. Header hits use
the shared close/tab/divider readers; tab-bar drops use the shared insertion
reader. Pointer activation, close, stacking, edge splitting and divider edits
lower through `Action::OuterWorkspace`, assigned to the existing Panes ring.
Closing preserves a surviving prior focus or chooses a surviving active body;
the existing last-pane guard remains and produces an attributable refusal.
Divider motion redraws and saves on release; captured gestures are discarded
if session authority changes. The new furniture has its own raster counters. The graph's selection, Forme
arrangement and content sessions are not reconstructed by these frame edits.

`workspace.json` and `window-workspaces.json` use a version-1 envelope. Load
validates the entire layout before writing a migrated sidecar, retains the old
files as recovery copies, and preserves null lens slots. Present malformed or
unsupported current files refuse rather than reviving the recovery copy. Both
current and legacy filenames remain in session-directory custody transfers.
Workbench serialization adds only its serde dependency edge to Cargo.lock.

**Qualification.** The standalone harness compiles the actual checked-in
custody and atomic-storage modules against the exact published Workbench pin.
Twelve portable tests pass: tab activation/close/restart, inactive custody,
invalid targets and shares, duplicate stations, retained tear-out custody,
shared model float/dock identity, legacy-file preservation, current-file
precedence, refusal without publication and absence without writes. These
prove those modules, not the complete PaneContent import, App or native frame.
The locked default-feature library and binary source checks pass, with 92
compiler warnings; no broad green-suite claim. Matching ALSA 1.2.15.3 Ubuntu
headers/runtime were staged in the task testing directory to complete those
checks. Full source-check results and hashes are recorded in
`docs/receipts/outer_workspace_20261012/qualification.json`; headed acceptance
has not run on S-PC. Native consumer tests in `panes::store` retain the actual
legacy content migration and are separate gates.

**Required continuation.** The native draft deliberately keeps pane bodies in
their existing per-PaneId retained surface renderers, as Frisket Surface slots.
A4's ruled in-house Component membrane and one composition per window are still
the target, not replaced by this checkpoint. Existing A5 blueprint-owned spaces
keep their current compositor and refuse shared-frame mutations; merge their
stations and persistence into Workspace before removing the blueprint/legacy
bridge. Shared model float/dock tests do not qualify that host join. Keyboard
and AccessKit header actions, exact frame-derived accessibility grafting,
Tabard theme/typography on the new furniture, motion previews, narrow/DPI
layout and presented-frame timing remain open.

**Native done-conditions for the next pass.** Compile/test the complete consumer
and run the original floating and physics scenarios unchanged in intent. In a
fresh disposable profile, summon graph/Inspector/Workbench/document panes;
stack and switch them, close an inactive and then active tab, resize an N-ary
split, edge-split a tab, and tear it into a lens and onward. Verify identities,
source bindings, retained scroll/editor state and the last-pane refusal. Restart
and check active tabs, nesting, ratios and lens null slots. Import real legacy
frame/windows fixtures once; corrupt a new file and verify explicit refusal
without legacy resurrection. Compare native frame timing to the previous
consumer and qualify the inline-body, floating and accessible-control joins
before marking A4/S3 complete.


## A4 shared-frame Tabard adoption draft — 2026-10-13

**Consumer change.** The outer tiled frame now consumes the existing shell-owned
live appearance snapshot in primary and lens windows. Tabard palette roles reach
tabs, active labels, close controls and dividers. The ordinary shared workshop
now supplies the installed-family picker and optional Interface family, CSS size
and leading. Frame layout, body apertures, paint and header hits consume the same
sheet. Automatic header height accommodates large type; clearing the override
restores host defaults. UI zoom scales host defaults; explicit authored font sizes
and exact CSS retain their cascade authority. Reading, Heading, Label and Code
role bindings in pane bodies remain separate consumer work.

**Selection and custody.** Appearance edits do not replace the Workspace, pane
identities, content/graph bindings or retained body renderers. The existing
save-versus-Apply distinction includes typography: saving a new definition under
the selected id leaves the currently applied value intact until explicit theme
Apply. Unrelated settings writes preserve the applied fonts as well as CSS.
Theme/mode changes retain ordered `ThemeChoice.fonts`, including unavailable
requests. Host inventory is process-local, requested names stay saved, fallback
notices are visible in Settings, and generic fallbacks reach Livery. Application
Interface family requests override generated theme defaults; exact authored CSS
remains last. Independent application-family picker mounting remains open; the
ordinary theme workshop is the font-authoring entry point in this slice.

**Geometry and gestures.** Divider drag arithmetic now uses actual retained DOM
gaps rather than a ten-pixel assumption, so authored divider widths participate
in the same geometry. An appearance change cancels an in-flight header drag
before old captured geometry can mutate the layout. The shared frame is exposed
as `surface:frame` to the existing scenario driver; `frame-css` and
`frame-header-height` observe the applied sheet and measured header.

**Supplier intake.** Turnstone adopts the published Mere `8568f42f`, Genet
`2c7ef73b`, Netrender `c74b7cb2`, maintained Classic Vello `2eae6243` and
Knot `3af93add` family used for current typography adoption. Dramatis and
Woodshed product pins are unchanged. Embedded Redshank's older Genet boundary
crates are bridged through the HTTPS www alias to that same Genet revision; the
resolved graph has no duplicate Genet package names and one Genet revision. The
Mere source set has size one. This source intake does not qualify the complete
engine, renderer recovery, Redshank, Knot or other contributed-surface lanes.

**Qualification and handoff.** The actual consumer stylesheet/shared Workspace
DOM/Livery CPU harness and default-feature Linux library/binary source checks
are recorded in `docs/receipts/frame_appearance_20261013/qualification.json`.
All 21 CPU tests pass (nine layout/cascade/font/scenario checks plus twelve
actual custody/sidecar checks against the new Workbench pin). The library and
binary typecheck passes with 92 warnings; warning-baseline equivalence is not
claimed. No headed run, release build or whole-library test run is claimed.
The provider's font-preservation/reopen unit test is defined for native owners;
whole-library test execution and headed acceptance remain open. Native owners
should run `frame_appearance.scn` with `frame_fonts_workshop.scn`, then reopen
with `frame_appearance_reopen.scn`. Use separate fresh stores for the
`frame_appearance_missing.scn`/`frame_fonts_missing_workshop.scn` pair and its
reopen fixture. These are parse-qualified drafts pending execution and capture
review. Use a lens to repeat live theme/font changes; test real installed faces,
custom divider width, same-id save/unapplied/Apply, failed-save rollback, narrow
windows, DPI, body continuity and the original floating/physics scenarios.
Keyboard/AccessKit header dispatch, float unification, inline bodies and actual
presented performance remain A4/S3 continuation gates.


## Native Pelt baseline adoption — 2026-10-13

**Status:** landed slice. Published Mere `0d1dc192` passes the normal Windows
library source check, ten selected-clip tests and five pane-store tests with
locked dependencies and no source overrides. The supplier closure has one Mere
source, one Genet revision and no duplicate Genet package names.

Turnstone now registers its native content engines through
`pelt_core::native::sessions_with_fetcher`, extending that registry with its
owned Micron and smolweb adapters. Transport, prompts, profile authority and
browser history remain Turnstone's. Selected clip address parsing, verification,
presentation and the retained document session now come from Mere's
`mere-document-lanes/retained-clips`; receiving, custody, onward export and
native audio actions remain in `src/retained_clip.rs` and the shell.

This is the baseline shared with Graphshell's retained-content Workbench.
Graphshell's browser host explicitly supplies its reading display fonts;
Turnstone continues to own browser appearance and authored-content policy.
The new `7d62a5a` Tabard/Interface-font frame draft is pulled in the same pass.
Its supplier receipt remains distinct from current Windows source qualification.

The adoption does not close A4's inline Component-body join, floats, physical
assistive technology or the complete native theme/narrow/DPI/performance wall.
It does not qualify optional external engine builds or simultaneous loading.
The shared implementation and receipts live in
`mere/ports/graphshell/docs/receipts/retained_content_20261012/`.


## A4 shared-frame keyboard and accessibility draft — 2026-10-13

**Interaction join.** `frame_controls` now owns the shared retained Workspace DOM
and Cambium tab dispatch. Ctrl+F6 enters pane furniture in the receiving primary
or lens window. Selected tabs use Cambium's arrow/Home/End automatic selection;
Tab/Shift+Tab traverse selected tab handles, close buttons and dividers and return
to the pane body at the ends. Escape and Ctrl+F6 also return to the body. Enter or
Space activates the focused close control; Ctrl+W closes the exact focused tab
handle. Existing last-pane refusal stays in the Panes action owner. Inactive
close buttons remain usable without selecting their body. Divider arrows follow
its axis, move one percent of the adjacent share pair (Shift: ten), and Home/End
reach the existing five/ninety-five-percent limits. Other N-ary shares stay fixed.
Keyboard and assistive divider edits use the existing Workspace action and save
a completed edit. Pointer gesture routing keeps its existing reducer path.

**Focus and session boundaries.** Header focus is transient shell state; it does
not enter the saved Workspace or pane payloads. It retains a validated return
target and otherwise chooses an active pane in its owning window. Closing or
reparenting a tab repairs focus by stable TileId; a divider additionally names
its adjacent tile groups so a reused split path cannot redirect old focus. A
profile change retires the frame admission. Header-owned keys and IME do not
reach body documents or the canvas. Modal input retains precedence. A body
press or native focus departure clears header focus, and window-local keyboard
routing avoids falling back into another window's frame. A Tab-boundary return
restores the pane lane; body-specific sequential traversal remains with each
body's existing input owner. Broader lens body/native-engine keyboard delivery
is a separate acceptance gate.

**Primary accessibility join.** Genet's neutral observation and AccessKit lowering
read the exact retained frame fragments. The frame subtree is namespaced by its
admission and revision, and existing active pane body subtrees attach beneath
the actual content apertures. Former structural containers are retired; missing
and inactive bodies are not retained as orphan outlines. Tab/close controls
expose Focus/Click; separators expose Focus/Increment/Decrement with measured
bounds, orientation and numeric values. Tab Focus follows Frisket's automatic
selection policy; Close and Divider Focus never activate or resize. Queued
actions revalidate scope, layout revision, visibility, modal ownership and
control identity before delivery. Stale/hidden/unsupported requests emit an
interaction miss. Frame focus/revision changes bypass the ordinary accessibility
publication cadence, and foreign document focus cannot override a foregrounded
primary frame control. The existing OS adapter still serves only the primary
window: no native lens accessibility adoption is claimed.

**Qualification.** The isolated harness imports the actual `frame_controls`,
`frame_a11y`, stylesheet and scenario parser modules against the upstream Mere
`0d1dc192` / Genet `2c7ef73b` supplier family. Turnstone `d5d4737` was
fast-forwarded during this draft; its shared native Pelt/retained-clip adoption
is preserved, and the final checks use that supplier pin. Thirty CPU tests pass: fourteen
control/composition tests, one new fixture grammar test and fifteen existing
scenario parser regressions. They cover actual shared tab callbacks, inactive
close, traversal exit, axis/pair resizing, focus repair, independent namespaces,
exact Livery bounds and separator actions, scope/revision rejection and tree
connectivity. Linux default-feature library/binary source checks and exact logs
are recorded in `docs/receipts/frame_controls_20261013/qualification.json`.
The final Linux library/binary check passes with 95 warnings; no warning-baseline
equivalence is claimed. Computed geometry and synthetic queue delivery do not qualify displayed focus,
OS adapter delivery or a human screen reader. No headed run, release build or
whole consumer test run is claimed.

**Native handoff.** Run `frame_controls_keyboard.scn` and
`frame_controls_accessibility.scn` in separate fresh private stores, then reopen
the latter stores with `frame_controls_reopen.scn`. The fixture grammar
`frame-a11y focus|click|inc|dec tab|close|divider ...` resolves one supported
current control and routes an ActionRequest through the actual platform queue
and drain; it is not an OS/screen-reader receipt. Review focus-ring captures,
body continuity and last-pane refusal. Add native nested-tab arrow/close and
N-ary divider cases, same-title independent accesses, custom theme hiding or
resizing a control, pending actions across move/close/profile changes, modal
ownership, DPI/narrow windows and actual Narrator/VoiceOver/AT-SPI delivery.
Repeat keyboard entry and return in a lens with its physical native window
focused. Session-scoped return to editors/media and original floating/physics
scenarios remain consumer gates. Native lens AT, float unification, inline
Component bodies and presented-frame timing remain open A4/S3 work.


## A4/A5 shared float and lens accessibility drafts — 2026-10-13

**Status: both implementations pushed as drafts; native acceptance remains open.**
This continuation replaces the preceding float-unification and lens-adapter
implementation gaps. It does not close A4/S3's inline Component-body join or
qualify physical UI, native engine delivery or human assistive technology.

**Shared stations and custody.** Float, dock, raise, tear-out and return now use
`workbench::Workspace` in primary and lens windows. Transfers clone and validate
both spaces before changing either station, preserving the same tile/PaneId,
host payload, provider descriptor, proportional rectangle, constraints, pin
and visibility. The destination assigns its own z-order. The shell's retained
body sessions remain keyed by PaneId; a station move does not evict them. Each
window keeps one pane. An unsuccessful destination or invalid geometry leaves
custody intact. No-op raises/rectangle edits do not produce a save.

Provider source/config descriptions now travel in the Workspace's optional
host descriptor map. Contributed panes no longer create a separate blueprint
station authority, and their saved descriptors are decoded and checked against
their owning pane before use. Existing compatible live blueprints import once,
including their float policy and provider descriptions. Unsupported legacy
grid/nested tab-sublayout shapes and invalid custody refuse before adoption;
all-window save preflight restores the former authorities if any import fails.
The unused blueprint float/transfer writers are retired. Legacy types and
read-only fallback presentation remain while their separate removal gate is
open.

New `workspace.json` and `window-workspaces.json` saves use **version 2**. Version
1 remains readable; missing descriptor maps default empty. New saves preserve
floating stations and provider descriptions. The older version-1 consumer
refuses the new version rather than discarding these descriptions. Invalid or
future current files still refuse without resurrecting the retained legacy
`frame.json` / `windows.json` recovery files.

**Float presentation and gestures.** Body apertures and floating outer boxes
come from the same retained Workspace DOM used for header hits and semantics.
Only actual Surface slots are body apertures; the floating container cannot
claim a second body. The compositor reuses the full-window shared frame texture
and crops it at each measured float box, interleaving float furniture and body
surfaces above tiled bodies in model z-order. Floating Workbench content slots
follow the same order. Appearance IDs and body owners remain intact. This is
still the host-composited body draft, not the ruled inline Component completion.

Drag a float's tab handle to move it. Hold Ctrl on release to dock through a
tiled body's centre (tab) or edge (split); an empty tiled root accepts docking.
An outside release tears out. The Dock command remains available. With a
floating tab focused, Alt+arrows move by one percent; Alt+Shift+arrows resize;
Ctrl adds a ten-percent step. Keyboard edits save immediately and pointer moves
save when the gesture completes. A session/appearance change cancels captured
geometry; float drags also reject changed window size. Body presses raise and
save a changed float order. Pinning, visibility and size constraints remain
shared typed Workspace events; this draft does not mount an additional controls
panel for them. Drop-region previews and authored shadow/constraint behavior in
narrow/DPI-changing windows still need native review.

**Per-window accessibility.** A lens is created hidden and receives its own
AccessKit adapter, activation snapshot, queue, route table and replay state
before being shown. Native events reach that adapter. Lens projection contains
that window's panes, actual contributed retained semantics and the same measured
shared frame controls; it does not import primary chrome or primary graph
outlines. Lens host NodeIds include window instance and session identity.
Child, label, control, focus and text-selection references are remapped together;
frame and contributed primary namespaces now use the same reference lowering.
Both windows lower header actions through the existing typed Workspace reducer.
Contributed actions additionally verify current pane/window visibility, source
and generation. Header focus returns to the owning window's body appearance.

Producer semantics are still polled once on the UI thread. Lens publication
caches share immutable retained node storage with that authoritative stream,
while mounting, action admission and activation replay remain independent.
A producer shown in multiple appearances has one semantic owner: the exact
focused appearance, otherwise the deterministic first window/appearance.
Losing parents are withdrawn before destination publication, including a
window-to-window swap. Navigation/retirement clears every mirror; old-window,
hidden and retired actions refuse. Consumer-library tests cover independent
mount/replay, retained deltas, shared storage and retirement. They do not prove
OS activation, screen-reader behavior or simultaneous visual appearances.

**Evidence and limits.** The actual-source CPU harness and preserved logs are
in `docs/receipts/stations_windows_20261013/qualification.json`. All 69 CPU tests
pass; the library/binary check reports 95 warnings. It includes the
shared controller/layout/semantic tests, exact generic custody and sidecar
modules, window namespace/appearance ownership, and the retained foreign forest
with current and pinned Windows/Unix consumer libraries. The default-feature
Linux library and binary source check passes with incremental compilation off;
warning-baseline equivalence is not claimed. Initial full-draft source attempts
hit the 1280 MiB compiler scope; disabling incremental compilation passed under
the same cap. One compiler, one Cargo job, no compiler swap, and the task memory
guard remain the local controls. No native run, release build, optional/platform
feature build or whole-library test run is claimed. Native App station/provider
regressions are supplied for a capable host; they have not been executed here.

**Handoff.** Run `frame_stations.scn` in fresh private stores, then
`frame_stations_reopen.scn` with those same stores. Separately run
`frame_lens_accessibility.scn` in fresh stores. Their grammar is CPU-qualified;
`frame-a11y-window <slot> ...` uses the window's actual published route and action
queue (0 primary, ordinal+1 lens). It remains a queued-action fixture, not a
screen-reader receipt. Re-run `a5_floating_pane.scn` with its original intent and
captures, plus the existing shared-frame keyboard/theme and physics fixtures.
On a capable host also run the native App `workspace_stations` and contributed
pane regressions, then review real pointer/Alt-key movement and resizing,
float-to-tab/split/window/reopen identity and editor/media/scroll continuity.
Exercise at least two lenses, node appearances repeated across windows,
activation/deactivation during ordered foreign batches, close/reopen/session
changes with queued old actions, and physical Narrator/VoiceOver/Orca focus and
announcements. Lens native-engine pixel/input delivery, constrained/authored
float layout, overlay/drop previews, inline bodies and presented performance
remain explicit acceptance work.

Upstream was checked before this continuation: Turnstone main contains the
shared native Pelt/retained-clip adoption (`d5d4737`) and preceding frame draft
(`74325ab`). The Mere design checkout was fast-forwarded to `5b3442dd`, including
viewer qualification and frame-window timings. This consumer keeps its already
adopted `0d1dc192` Mere and `2c7ef73b` Genet supplier family; no further supplier
pin change is part of these drafts.

## A4 retained inline bodies and final supplier adoption — 2026-10-14

This continuation supersedes the October 13 host-composited body draft above.
All thirteen in-house pane owners mount their existing retained bodies into the
receiving window's shared Workspace/Frisket DOM, cascade, fragments and paint
list. PaneId still owns the widget/app state. The shared Cambium retained mount
keeps the actual runner subtree through tree, float and lens moves; source-first
withdrawal parks it under a live hidden document root before collection. Runtime
teardown precedes final mount destruction. Custom/manual tab widgets retain their
focus behavior; only the stock tab widget explicitly requests roving focus.

Foreign Surface producers retain their acquisition, session, input, accessibility
and media owners. Their actual same-device views are staged as ordinary image
commands at exact appearance apertures inside the retained body's CSS clips,
transforms and opacity groups. The window scene is rasterized once. The former
cropped-frame replay compositor is replaced; equal source UUIDs and rectangles
cannot substitute for appearance identity. Classic raster sources declare
straight alpha, while supported native imports retain their explicit producer
convention. Unsupported hardware-decoded sRGB import views refuse staging.
Mask resources have window/session custody, reuse exact requests and explicitly
retire on withdrawal, replacement, recovery or window closure. An empty tiled
root remains an inert Hole rather than claiming a floating tile's body identity.

Each window uses a neutral layout scope with a real styled window-root child.
That child owns the viewport-sized authored backdrop even when every pane floats.
Theme Interface typography reaches the shared frame controls; authored font size
and line height, narrow resize, reset, persisted selection and unavailable-family
fallback use the real theme workshop and current owner routes. Body input, IME
and accessibility remain tied to the current receiving owner. Header focus does
not intercept an unrelated body gesture; lens accessibility snapshots read the
actual focused window rather than the primary window.

The bounded host wake repair preserves the final document paint before and after
a pump, observes Pelt's existing pending work without invalidating prepared routes,
and anchors supplied future timer delays at the actual pump start. Only current
admitted receiver windows are queued. Runnable work retains the configured clock
cadence; quiet lenses are not redrawn by a blanket timer. Lens pane wheel input
uses the actual receiving-window plan, and existing scrollbar continuation is
scoped to panes in that plan. This introduces no durable retention or destruction
policy.

The normal published consumer graph adopts Mere
`77fd235fe659c513dd0463ed57be32f02fb2e71a`, Genet
`b6bfbeb556a6a17eddf8d5ad8254d3fcac95b084`, and the single Netrender family
`6a24ae08f55129b474282b8e4911564f875ce5fd`. Its eight affected test groups pass
all 17 tests, and the default native binary builds with locked/offline source
resolution and no local path override. Earlier full-suite and local-path receipts
keep their original supplier authority; this continuation does not claim another
unfiltered or optional-engine suite. The restored old scope-root and timer-blind
controls fail as expected, and exact candidate bytes pass after restoration.

All fourteen final headed scenarios pass against that normal published binary:
inline tree/float/lens continuity, actual alpha/shadow overlap and resize, A5
float/dock/return, station reopening, keyboard and primary/lens accessibility
actions, pane wheel delivery, authored typography/reset and missing-family
fallback/reopening. The runs present 1,141 primary frames and produce forty
primary/lens captures, plus one real theme-editor capture. All forty targets were
visually reviewed, and eight actual shared-raster/final pairs additionally pass
the authored backdrop and pixel comparison checks. Alpha comparisons exclude
the independently composed chrome overlays rather than claiming whole-target
identity. Source, executable, normal manifest/lock, fixtures and artifact hashes
are bound by `docs/receipts/stations_windows_20261013/published-final-native-qualification.json`.
Preserved local-source runs retain their separate authority.
Physical OS activation, human assistive technology and monitor DPI transitions
remain separate gates. The immediate lens-scroll fixture's capture dispatch can
land after the existing scrollbar fade interval; actual body movement and the
scoped continuation control do not claim early scrollbar opacity pixels.
