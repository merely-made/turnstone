# Pane Inventory

**Date**: 2026-08-09

**Status**: A0 entry-state receipt for the [Pane Registry, Graph Views, and Shell
Composition Plan](2026-08-08_pane_registry_and_graph_panes_plan.md). A1 began
immediately afterward; its changes are summarized below.

This inventories the model at the A0 boundary, including summonable, internal,
and dormant pane variants. The target column records the source and multiplicity
the registry must enforce in A1.

## Reading the live model

- A legacy leaf stores `PaneId`, `PaneContent`, and `graph_id` in `frame.json`.
  Lens placement is stored in `windows.json`.
- `PaneContent::follows_active_graph()` is the only current follower policy. It
  rewrites a leaf's `graph_id`; there is no explicit published `PaneContext`.
- `summon_pane` normally permits repeated kinds and mints a new `PaneId`, but
  the shell retains one renderer per kind. Two same-kind ids therefore do not
  yet have independent retained state.
- The evidence column distinguishes headed scenarios from unit-only or absent
  evidence. A scenario proves the named interaction, not arbitrary same-kind
  independence.

## Current and dormant panes

| Pane | Live source and follower | Published context today | Instance state and persistence | Renderer and capabilities | Evidence | A1 target |
| --- | --- | --- | --- | --- | --- | --- |
| Orrery, the current Graph pane | Leaf `graph_id`; marked active-graph-following | Implicitly supplies the global canvas graph and focused member | Camera, selection, and graph runtime live in the singleton `App.canvas`, not under `PaneId`; leaf persists in `frame.json`; restore removes duplicate Orreries for one graph | Native mixed-surface canvas; pointer, keyboard, animation, accessibility, and content contribution | `rung5_panes.scn`, `rung5_panes_restore.scn` | Fixed Forme; **Many** |
| Workbench | Leaf `graph_id`; active-graph-following | Implicitly supplies global graph and active member | One `App.workbench` and one retained runner for every Workbench leaf; arrangement persists separately in `workbench.json`, while the leaf persists in `frame.json` | Cambium furniture plus nested live document surfaces; tab, split, stack, close, drag, and tear-out actions | `rung5_workbench.scn`, `rung5_workbench_restore.scn`, app persistence tests | Fixed Forme; **Many** |
| Tile | Member UUID in `PaneContent` plus leaf `graph_id`; currently active-graph-following despite being a pinned member | Implicit graph/member through the leaf and global content session | Member id and graph tag persist in the leaf; document session is runtime state | One live document surface when available; pointer and keyboard routing; honest placeholder otherwise | `rung7_tile_tearout.scn` | Fixed member; **Many** |
| Gloss | Leaf `graph_id`; active-graph-following | Implicit global graph/member | Composition section ids are genuinely per leaf and persist in `frame.json`; camera/paint runner is global per kind | Retained Cambium pane with custom-paint minimap and section composition | `rung5_gloss.scn`, `rung5_gloss_composite.scn` | Graph context with per-instance config; **Many** |
| Roster | Leaf `graph_id`; active-graph-following | Implicit global graph | One retained grid for every Roster id; no durable pane-local selection or scroll | Retained Cambium graph manifest; pointer and DOM/accessibility contribution | `rung5_roster.scn`, tear-out and persistence scenarios | Graph context; **Per space and context** |
| Inspector | Leaf `graph_id`; active-graph-following; reads the global focused canvas member | Implicit global graph/member | One retained Inspector for every id; selection follows global canvas focus | Retained Cambium detail sections and clip status | `rung5_inspector.scn` | Member context; **Per space and context** |
| Apparatus | Classified graph-independent, but reads the global focused canvas member | No explicit publication | One retained Apparatus for every id; viewer override persists with graph/session data rather than the leaf | Retained Cambium facet/viewer controls | `rung4_settings.scn`, `rung4_settings_verify.scn` | Member context; **Per space and context** |
| Trail | Leaf `graph_id`; active-graph-following | Implicit global graph/member/session | One retained Trail for every id; scroll is not pane-keyed | Retained Cambium chronology and recent/removed sections | `rung5_trail.scn` | Graph or session context; **Per space and context** |
| Overmap | Application session set; classified graph-independent | Selection is observable but not published as pane context | Composition section ids persist per leaf; one retained paint runner for every id | Retained Cambium custom-paint session-lineage graph with section composition | `overmap.scn`, `overmap_composite.scn`, `overmap_o3.scn` | Session-set source; **Per space** |
| Settings | Magic `Custom("settings")`; application settings provider | None | One retained Settings runner; leaf placement persists separately from provider-owned values | Retained Cambium settings projection with routed controls | Summon/render/input unit coverage; no dedicated headed Settings-pane scenario found | `Settings(SettingsRef)`; **Per space and source** |
| Publishing | Magic `Custom("publishing")`; one shell-owned publishing service | None | One retained pane and service for every id; leaf persists, workflow state is service-owned | Retained Cambium owner workflow | Summonability unit test; no headed pane scenario found | Explicit publishing target; **Many** |
| Shared Knot | Magic `Custom("shared-knot")`; one shell-owned ticket-reader service | None | One retained pane and service for every id; leaf persists, ticket/fetch state is service-owned | Retained Cambium recipient workflow | Summonability unit test; implementation is currently landing; no headed scenario found | Explicit share-ticket source; multiplicity must be decided in A1 |
| Steward | Application/space activity by intended meaning; classified graph-independent | None | No pane-local runtime beyond the leaf | Generic labeled placeholder | No headed receipt found | Application or space source; **Per space** |
| Comms | Place/session conversation by intended meaning; classified graph-independent | None | No pane-local runtime beyond the leaf | Generic labeled placeholder | No headed receipt found | Place or session source; **Per space and source** |
| Alembic | Application/persona memory by intended meaning; classified graph-independent | None | Dormant `PaneContent` variant; no summon path or pane-local runtime | Generic labeled placeholder if loaded | No live receipt found | Application or persona source; **Per space and source** |
| System | Classified graph-independent | None | Dormant `PaneContent` variant; no constructor or pane-local state | Generic labeled placeholder if loaded | No live receipt found | Remove; diagnostics belong to addressed panes or shell services |

`Graph pane` is the general role that Orrery currently occupies. It is not a
second live `PaneContent` variant. The registry may keep the user-facing name
Orrery while giving it the general fixed-Forme contract.

## Cross-cutting findings

1. `PaneId` is already the durable movement identity, and tear-out preserves it.
   The retained runner maps must become pane-keyed before repeated kinds are
   truthful.
2. Graph identity is stored on leaves but is dropped before placement and
   surface planning. The singleton canvas prevents the two-graph receipt.
3. `PaneContent::follows_active_graph()` conflates source and context. Tile and
   Apparatus expose the clearest contradictions.
4. Workbench arrangement is valid separate authority, but its current
   application-global location makes a second Workbench a duplicate view of one
   arrangement.
5. `Custom("settings")`, `Custom("publishing")`, and
   `Custom("shared-knot")` were already typed product concepts. A1 registered
   them directly and kept a namespaced schema boundary for external sources.
6. Layout movement and restore have stronger evidence than per-kind runtime
   independence. Existing tear-out receipts must not be cited as proof that two
   same-kind panes keep separate state.

## A0 handoff

The rendering-free model now lives in `src/panes/blueprint.rs` with context
resolution and recursive topology helpers beneath `src/panes/blueprint/`.
Its invariants are:

- source, context binding, config, and view state are distinct;
- one pane id has one specification and one tiled or floating station across
  live spaces;
- tear-out moves the same specification between spaces;
- nested splits, tabs, and grids normalize without render code;
- fixed identity Formes must name their actual graph;
- focused-context following resolves within the follower's current space, and
  pinning converts it to a fixed source.

## A1 changes after this snapshot

- `PaneKind`, `System`, magic custom-pane strings, and fake layout placeholders
  have been removed.
- `PaneDefinition` now owns built-in pane ids, labels, source validation,
  multiplicity, capabilities, palette entries, config/view schemas, legacy
  construction, and renderer keys.
- Simple registered panes use a namespaced `PaneKindId` payload rather than
  gaining another `PaneContent` variant.
- Summoning enforces the registry's current per-space multiplicity policy.
- Every retained Cambium runner map is keyed by `PaneId`; render, input,
  automation, and lens paths resolve the same instance, and close evicts it.
- A two-Roster unit receipt changes one runner's selected tab while the other
  remains unchanged. It is written but cannot execute in the full crate until
  the concurrent Knot share-reader import mismatch is resolved.
