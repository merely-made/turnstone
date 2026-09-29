# Pane Inventory

**Date**: 2026-08-09

**Status**: A0 entry-state receipt for the [Pane Registry, Graph Views, and Shell
Composition Plan](2026-08-08_pane_registry_and_graph_panes_plan.md). A1 began
immediately afterward; its changes are summarized below.

This inventories the model at the A0 boundary, including summonable, internal,
and dormant pane variants. The target column records the source and multiplicity
the registry must enforce in A1.

## Current consolidation, 2026-09-29

Gloss now composes durable Downloads alongside its minimap by default and retains
per-instance add/remove/reorder configuration. Inspector includes document and
graph-object readouts plus the former Apparatus viewer/capability controls.
Steward and the object-analysis Apparatus pane are retired; saved leaves become
Gloss with Downloads and Inspector in place, preserving arrangement identity.
See the [current Gloss ruling and implementation](2026-07-20_gloss_composite_pane.md#current-direction-2026-09-29).
Downloads rows are inert and older Gloss compositions retain their configured
sections. Within-document selection inspection, background/sync operation controls,
built-in pane semantic automation and human AT remain separate qualification work.

The table retains the dated A0/A1 entry-state receipt, with Gloss, Inspector and
retired pane rows refreshed on September 29. Other rows retain their A0/A1 scope;
the September consolidation's automated and native gates passed, as recorded
in the [September 29 qualification receipt](2026-09-29_shared_diagnostics_gloss_inspector_receipt.md). Custom-leaf semantic parity, full built-in selectors, document
selection and exact frame correlation remain open alongside human AT.

## Reading the live model

The bullets below record the historical A0 model. The September 29 rows explicitly
supersede its renderer/follower description for Gloss and Inspector; the other
rows retain their dated scope rather than claiming a fresh whole-stack audit.

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
| Gloss | Leaf `graph_id`; active-graph-following; minimap and section gathers still read the active runtime cursor | Reads active graph truth; no independent member publication | Per-`PaneId` retained swatch runner and camera; ordered section ids persist on the leaf in `frame.json` and `windows.json` | Minimap plus Downloads by default; configurable Recent/Removed/Nodes/Downloads sections | Existing Gloss scenarios; download-to-DOM and section persistence tests; native migration/restart passed ([receipt](2026-09-29_shared_diagnostics_gloss_inspector_receipt.md)) | Graph context with per-instance configuration; **Many** |
| Roster | Leaf `graph_id`; active-graph-following | Implicit global graph | One retained grid for every Roster id; no durable pane-local selection or scroll | Retained Cambium graph manifest; pointer and DOM/accessibility contribution | `rung5_roster.scn`, tear-out and persistence scenarios | Graph context; **Per space and context** |
| Inspector | Resolves graph/member through `follower_context(pane)` in its own space, with the pane-bound graph/member fallback | Consumes the last graph/member context publisher in its own space | Per-`PaneId` retained Inspector and scroll; viewer control mirrors product browser state and its typed write carries the followed member | Document/object facets, provenance, classification, viewer override, capability/zoom readouts and Knot clipping | Inspector and migrated viewer tests; followed-member regression; native migration/restart passed ([receipt](2026-09-29_shared_diagnostics_gloss_inspector_receipt.md)) | Member context; **Per space and context** |
| Apparatus | Retired September 29; old saved leaves restore as Inspector | Replaced by Inspector context consumption | Existing leaf identity, graph binding and split placement preserved on restore; viewer state remains product-owned | Viewer/capability controls migrated to Inspector; shared diagnostics belong to `mere-apparatus` | Frame/lens migration tests; migrated viewer scenarios | Retired pane; Inspector provides object analysis |
| Trail | Leaf `graph_id`; active-graph-following | Implicit global graph/member/session | One retained Trail for every id; scroll is not pane-keyed | Retained Cambium chronology and recent/removed sections | `rung5_trail.scn` | Graph or session context; **Per space and context** |
| Overmap | Application session set; classified graph-independent | Selection is observable but not published as pane context | Composition section ids persist per leaf; one retained paint runner for every id | Retained Cambium custom-paint session-lineage graph with section composition | `overmap.scn`, `overmap_composite.scn`, `overmap_o3.scn` | Session-set source; **Per space** |
| Settings | Magic `Custom("settings")`; application settings provider | None | One retained Settings runner; leaf placement persists separately from provider-owned values | Retained Cambium settings projection with routed controls | Summon/render/input unit coverage; no dedicated headed Settings-pane scenario found | `Settings(SettingsRef)`; **Per space and source** |
| Publishing | Magic `Custom("publishing")`; one shell-owned publishing service | None | One retained pane and service for every id; leaf persists, workflow state is service-owned | Retained Cambium owner workflow | Summonability unit test; no headed pane scenario found | Explicit publishing target; **Many** |
| Shared Knot | Magic `Custom("shared-knot")`; one shell-owned ticket-reader service | None | One retained pane and service for every id; leaf persists, ticket/fetch state is service-owned | Retained Cambium recipient workflow | Summonability unit test; implementation is currently landing; no headed scenario found | Explicit share-ticket source; multiplicity must be decided in A1 |
| Steward | Retired September 29; old saved leaves restore as Gloss with Downloads | Downloads provider reads active graph custody facets | Existing leaf identity and placement preserved; new Gloss configuration contains Downloads | Read-only download status, bytes, destination and errors now compose in Gloss | Download-to-DOM and frame/lens migration tests; `smolweb_download.scn` targets Gloss | Retired pane; Gloss provides operational overview |
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
- 2026-09-03: an **Arrange** pane (`turnstone.arrange`, renderer `Arrange`,
  graph source, per space and source, palette "Open Arrange pane") — the
  native counterpart of the web host's Find-and-arrange section: the
  arrangement choice (the canvas strategies, or Free), the physics law, a
  toggle per overlay, the kind / mass / depth sources, and the profile, all
  as `cambium::setting_row` controls built from the canvas catalogs. Applied
  rows leave as `ArrangeIntent`s the shell lowers to the same `Action`s the
  palette's `Physics:` / `Overlay on|off:` / `Profile:` / `Kinds:` / `Mass:`
  / `Depth:` rows fire; the observe snapshot's `arrange_rows` feed
  `assert row`; the choice rides `ViewIntentV1` per session. Receipt:
  `scenarios/physics_native.scn`. Plan:
  `mere/design_docs/mere_docs/implementation_strategy/2026-09-02_physics_catalog_plan.md`.
