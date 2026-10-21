# Compare: selection sources, selection commands and shared scenes

**Status (2026-10-21):** proposed, not started. Phases 0 to 2 can begin now;
phase 4 waits on the open questions at the end. Source state: Turnstone
`45054b0`, Mere `origin/main` `8008eba7`; Turnstone pins Mere `3cce9fe37`, and
every Mere path cited here is unchanged at that pin.

This is the next slice after
[relationship navigation and pane bindings](2026-10-21_relationship_navigation_and_pane_bindings.md).
That pass gave the graph influence over reading through the Related disclosure
and visible Follow/Pin bindings. This one lets a *set* of documents drive a
presentation, and lets the result be kept.

## The walkthrough (acceptance)

1. Read an article. Related lists its sources.
2. Select two sources without leaving the article.
3. Choose **Compare**. Both open side by side for reading; the article stays
   where it was, pinned.
4. Activate either compared document; the Inspector follows it.
5. **Show connections** reveals, on the Map, the paths joining the article and
   the two sources.
6. **Save scene**. Reopen it later, in Turnstone and in Graphshell: the same
   selection and Map state come back, and Turnstone restores the comparison.

Each phase below names the steps it makes pass.

## What exists

- **Selection is single-member everywhere it is published.** Multi-select lives
  only in Mere's `Canvas` (`select_member`, `toggle_select_member`,
  `set_selected_members`). Turnstone keeps per-Map selections process-local
  (`GraphPaneViews.selections`, `src/app/runtime_pool.rs:435`); `focused_member`
  returns a member only when exactly one is selected, so two selected documents
  publish nothing and the Inspector says "No document selected". `PaneContext`
  (`src/panes/blueprint.rs:182`) is a `Copy` struct of four single options.
  Shift is never forwarded to the canvas (`src/shell/events.rs:343`); only the
  right-drag marquee multi-selects.
- **A snapshot-or-live set already exists in Mere.** `curation::SubgraphSpec`
  (`crates/forme/curation/src/lib.rs:28`) with forme's `SubgraphBinding`
  (`UnlinkedSession` fixed, `Linked { spec }` re-derived on revision;
  `crates/forme/forme/src/subgraph.rs:51`), and graph-kernel's
  `FrozenSelection` (`graph.frozen-selection/v1`: spec, revision, members,
  statements). Only `Component`, `Ego` and `Sparql` derive today; `Corridor` is
  declared but falls back to its seed. Turnstone uses none of it.
- **Compare has vocabulary but no consumer.** Forme's `Arrangement` names
  `CompareWith`, `PinnedIn` and `FocusPath` (`arrangement.rs:108-113`); only
  `StackedWith` and `SplitWith` have builders. `FormeRef::Stored` is documented
  as the "workbench / compare bench". Turnstone's Read is one Workbench pane
  keyed on `FormeRef::Identity` (`forme_for_pane`, `src/app/mod.rs:645`);
  `Workbench::open_split` is used only by a test.
- **Paths.** `Graph::shortest_path` (graph-kernel `graph/query.rs:734`) returns
  one undirected unit-weight path and has no callers. The canvas can emphasise
  only through the graph grammar (selected, foreground-pinned, hovered); there
  is no API to emphasise an arbitrary member or link set.
- **The shared scene is Graphshell's.** `SavedSceneV3` on facet
  `graphshell.saved-scene/v4`, on a `mere://scene/<name>` node
  (`ports/graphshell/src/product.rs:67, :329`): name, `selected: Vec<Uuid>`,
  dynamics, camera, default handler and cartography (seed, regions, Mosaic). It
  describes one canvas: no panes, no reading arrangement. It is port code, and
  Turnstone deliberately does not depend on `graphshell`, so it cannot read or
  write it today.
- **Turnstone's "scene" is something else.** `src/graph_scenes.rs` writes one
  Map's arrangement as pandect `arrangement.*` facets on the graph's container
  node; window and pane layout live in `workspace.json`; A8 "Named blueprints"
  is the unbuilt home for saved layouts.

Rulings this plan keeps (quoted from the documents named):

- Saved queries share the spec and freeze on demand (Mere graph semantics plan,
  Ruling 3); a frozen selection is an immutable resource (Ruling 46); every
  result carries a coverage note (Ruling 4).
- Selection is not availability: an empty selection never becomes "all"
  (page lifecycle plan, R3). A subgraph selects; it does not make nodes.
- Scenes are a node plus a facet (scenograph editor plan, SE14), at
  `saved-scene/v4` with strict own versioning (dynamics grammar plan, F142,
  F98); they sync as an opaque facet (F158) and travel in codicil v3, which
  remaps member UUIDs (F159).
- Source and context stay in `PaneSpec` descriptors; no second binding map or
  persistence file; unavailable states never fall back to another selection
  (relationship navigation, 2026-10-21).
- Authorities stay separate: graph truth, Map view state, the Workbench's forme
  arrangement, and the window blueprint (pane registry plan).

## Design

### Selection source

A **selection source** names a set of members and says whether it is fixed:

- **Members** — an ordered list of member UUIDs. Order matters: Compare lays
  out and Read-in-sequence reads in that order.
- **Linked** — a `SubgraphSpec`, re-derived on each graph revision, with the
  coverage note Ruling 4 requires.
- **Frozen** — a reference to a `FrozenSelection` resource, made on demand
  ("Keep this selection").

It lives in `curation`, which already owns "what a view chooses to show of a
graph", so Graphshell and Turnstone share one type. An empty source is a
visible empty state, never the whole graph. Members that no longer exist are
shown as unavailable, not dropped or substituted.

In Turnstone the window's current selection is published beside the focused
member: `PaneContext` gains a selection id (keeping it `Copy`), resolved through
the existing `ContextIndex`, so the Inspector can say "2 selected" and commands
can read the set. A pane that should keep a set uses a `PaneSource` carrying a
selection source, in the existing descriptor.

Ways to select while reading (step 2): the Related rows and the Documents menu
gain a selection toggle beside the existing open action, Shift-click toggles on
the Map, and the marquee stays. Selecting never navigates.

### Selection commands

All four are ordinary `Action`s declared through Hest, following the
the Hest command architecture
(`mere/design_docs/cambium_docs/implementation_strategy/2026-10-20_hest_command_architecture_plan.md`):
a variant, an explicit `command_metadata` row, a ring entry (Panes), a dispatch
arm, and a palette row offered only when the selection qualifies.

- **Declaration.** Effect class per Hest's taxonomy (Mark's ruling 2,
  2026-10-20): Compare, Show connections, Read in sequence and Search within
  selection are `Curation`, since they change what is shown and how, not graph
  truth. Keep this selection (freezing) and Save scene write graph resources
  and are `DomainTruth`. None is `destructive`; saving over an existing scene
  of the same name is, and is offered separately.
- **Invocation.** A Hest `Invocation` whose arguments carry the selection id and
  the ordered members it was offered for, under a new schema id rather than a
  loosened `turnstone.command/v1` (which denies unknown fields). The outcome is
  Hest's: `Stale` when the selection no longer matches (the
  `SetAnswerSelection` pattern), `Refused` with a reason (fewer than two
  members, a member live in another surface), and `Done` only once the new
  arrangement is observed, as Graphshell's receiver requires.
- **Ids.** Turnstone owns these ids for now. They join `hest::catalogue` as
  shared verbs only under the admission rule (ruling 1): a second host offers
  the verb, or ux-events reasons about it. Graphshell offering Compare over a
  shared scene (phase 4) is the natural second host.
- **Automation.** Mesquite's `Product::invoke` and Rhai command collection
  invoke by Hest id, so the walkthrough scenario drives Compare and Save scene
  by id as well as through the chrome.

- **Compare** (2 to 4 members). Opens the members side by side in a Read
  Workbench bound to a stored forme document (`FormeRef::Stored`): one tile
  intent per member, related by `SplitWith` and `CompareWith`, the first
  consumers of `CompareWith`. The document Compare started from keeps its pane
  and becomes a pinned source (the existing Pin binding; `PinnedIn` in the
  arrangement). The Inspector, following the active pane, follows whichever
  compared document is activated, which needs Read cell activation and Tile
  panes to publish (Tile panes do not today). A member already live in another
  surface follows the one-surface rule; Compare reports that rather than
  opening a second session.
- **Show connections**. Shows the paths joining the selection (and a pinned
  source, if any) on the Map. Needs Mere work: `Corridor` derivation over the
  anchors from `shortest_path`, and a canvas API to emphasise a member and link
  set through the graph grammar. The result can be kept as a Linked source.
- **Read in sequence**. Opens the selection as one stacked reading in source
  order, recorded with `FocusPath`, with next and previous.
- **Search within selection**. Scopes the omnibar's search to the selection's
  members. An empty selection disables the scope rather than widening it (R3).

### Scenes

The scene format stays Graphshell's, lifted to where both applications can use
it, and extended once for what a comparison needs:

- **Lift.** The scene envelope, its facet constants and its capture/restore
  types move from `ports/graphshell` to `pandect`, which already owns the scene,
  arrangement and view-intent facets. Graphshell consumes the lifted types with
  no re-export shim. This also unblocks the planned `personal_sync` lift, which
  currently imports scene types from the port.
- **Extend.** One new version, strict as F98 requires, adds an optional
  selection source (alongside `selected`) and an optional reference to the
  stored forme document holding the reading arrangement. Window and pane
  geometry stay in the workspace layout; the scene names what is compared, not
  where windows are.
- **Hosts.** Turnstone saves named scenes as scene nodes and reopens them,
  restoring the Map, the selection source and the stored forme arrangement.
  Graphshell restores the Map and the selection, and opens the arrangement in
  its Forme workspace, whose reading tiles already use member identity.
- **Travel.** Codicil export and import remap the members in the selection
  source and the arrangement; sync still carries the facet opaquely.

## Phases

**P0 — Lift the scene envelope (Mere).** Move the saved-scene types and facet
constants to `pandect`, unchanged in format. *Done when* Graphshell's scene,
codicil and sync tests pass on the lifted types and `pandect` exposes them
without `graphshell`.

**P1 — Selection (Mere `curation`, Turnstone).** The selection source type; its
publication through `PaneContext`; Shift-click; selection toggles in Related and
Documents; the Inspector's N-selected state. *Done when* steps 1–2 pass in a
native scenario and an empty selection never scopes anything.

**P2 — Compare (Turnstone, forme).** The Compare action with its metadata,
ring, palette and stale guard; the stored forme arrangement with `SplitWith`,
`CompareWith` and `PinnedIn`; Read and Tile publication so the Inspector follows
the active document. *Done when* steps 3–4 pass in a native scenario.

**P3 — Connections, sequence, scoped search (Mere, then Turnstone).** `Corridor`
derivation and the canvas emphasis API in Mere; Show connections, Read in
sequence and Search within selection in Turnstone. *Done when* step 5 passes.

**P4 — Shared scenes (Mere, both hosts).** The new scene version, Turnstone save
and reopen, Graphshell reading it, codicil remapping. *Done when* step 6 passes
in both applications and a codicil round trip keeps the selection.

Evidence for each phase: focused tests and one native scenario (the drive
verbs, such as `select-graph-member`, already exist), with captures outside Git
and a short findings note here.

## Open questions

These are proposals, not rulings.

1. **What a saved comparison contains.** Recommended: the scene references a
   stored forme document for the reading arrangement. Alternatives: embed the
   arrangement in the scene facet; or keep scenes canvas-only and save the
   comparison as a separate forme document related to the scene node.
   Interacts with the open facet-dissolution brief (stack lift R5); this plan
   keeps scenes as node plus facet until that brief rules.
2. **Where the selection source lives.** Recommended: a small type in
   `curation` reusing `SubgraphSpec`. Alternatives: forme's `SubgraphRef` as it
   stands (its `anchors` serve as both seed and roster), or a Turnstone-only
   `SourceRef::External` schema like the answers source.
3. **What "scene" means in Turnstone.** Recommended: a scene is a saved, named
   scene node, as in Graphshell; the per-graph state `graph_scenes.rs` writes
   is the Map's arrangement. Alternative: keep the current meaning and call
   saved ones something else.
4. **Compare's panes.** Recommended: one Read Workbench split per comparison,
   bound to its stored forme document. Alternative: one Tile pane per member,
   placed beside the active pane.
