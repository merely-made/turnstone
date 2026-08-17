# Turnstone's surfaces expressed in cambium

2026-07-15. With cambium adopted (the Roster grid landed, d05f24d), every turnstone
surface gets asked the same question: what cambium component expresses it? The
answer is one of three — an existing catalog entry, a new catalog addition, or
"stays non-cambium." This doc is that mapping.

> Sibling: the cambium adoption also gave every turnstone surface a
> semantic, hit-testable DOM — the substrate for driving the app by name. Where
> that generalizes across the genet apps is
> [`2026-07-17_genet_probe_automatability_plan.md`](2026-07-17_genet_probe_automatability_plan.md).

The framing is the serval/genet-era development pattern: **a consumer's need is a
good reason to expand the catalog.** Not the only reason — cambium has its own
sense of what belongs — but a real one. So where a pane wants a primitive cambium
lacks, the addition is named and justified by the consumers that pull it, and the
strongest additions are the ones several panes pull at once.

## The seam (recap)

A cambium view renders into a `ScriptedDom`; turnstone lays that out with
genet-layout and composites it at the pane's surface rect (`ui::scene_from_dom`
under a host sheet). Events feed into the view's `GenetAppRunner`, which returns
Actions turnstone lowers through its spine. So "express surface X in cambium" means:
X's content is a cambium view over a `GenetAppRunner`, composited at X's rect. The
host keeps the surface plan, the compositor, and the canvas; cambium owns what a
pane draws inside its rect.

## The mapping

| Surface | cambium expression | Status |
| --- | --- | --- |
| **Chrome / omnibar** | `text_field_typed` + `action_list` (the find lane) / `command_surface` (the `>` lane) | existing — migrate |
| **Caption chip** | `el` (a positioned label) | existing — trivial |
| **Roster** | `data_grid` + **tab strip** (Nodes/Links/Graphlets/Fields) + `detail_popover` (facet cards) | grid DONE; tabs NEW |
| **Trail** | **sectioned list** (Recent/This-node/Removed) + `button` (Recover) | list NEW |
| **Inspector** | **detail panel** (key/value sections, diagnostics) | NEW |
| **Gloss** | `graph_canvas_swatch` (minimap) + **tree/outline** (structure) | swatch existing; tree NEW |
| **Steward** | **sectioned list** + `Meter` leaf (progress) | list NEW; Meter existing |
| **Comms** | **message list** (chat) | NEW |
| **Alembic** | **sectioned list** / `data_grid` (Recent/Saved/engrams) | reuses Trail/Roster |
| **Apparatus** | `checkbox` / `toggle` / `radio_group` / `select` / `slider` / `text_field_typed` (settings) + **sectioned list** (diagnostics) | controls existing |
| **Workbench** | **tab strip** (tile tabs) + **split** (tiling) + `Swatch` leaf (node bodies) | swatch existing; tabs/split NEW |
| **Notes / knot editor** | `editor` / `styled_textarea` | existing |
| **Pane furniture** (frisket dividers, stacked-pane tabs, maximize/close) | **split** + **tab strip** | NEW |
| **Orrery (the canvas)** | stays `mere::canvas` | non-cambium |
| **Content documents** (live pages) | stays genet document sessions | non-cambium |
| **Surface plan / layered present** | stays turnstone (host composition) | non-cambium |

## The catalog additions the consumers justify

Ranked by pull — how many turnstone surfaces want each. The multi-consumer ones are
the real catalog candidates; the single-consumer ones can compose from existing
primitives first and graduate to the catalog if a second consumer appears (the
family's crate-promotion gate, applied to components).

1. **Tab strip** — the strongest. Roster's four data tabs, the Workbench's tile
   tabs, and any stacked frisket panes all want the same "one strip of labeled
   tabs, one active, click/keys to switch" widget. Three distinct consumers, one
   shape. cambium has `keyed` for the reconciliation and `arrangement` for
   placement; the tab strip is the composition worth naming.

   **LANDED 2026-07-17** — `cambium::tab_strip` (catalog: a tab strip) + the
   Roster's four tabs (Roster: tabs, from cambium's catalog). Two things the
   first consumer pulled that the sketch above did not predict:

   - **Generic over `Action`.** The strip emits none (switching a tab is a state
     change), but its siblings do — the Roster's grid bubbles a `Navigate`. A
     `()`-actioned strip would force every such caller through `map_action`, so
     the strip is generic like `data_grid`, not `()`-actioned like the controls.
   - **The host owns the strip's geometry, so the host must state its height.**
     The strip sets none by design. That makes `TABLIST_HEIGHT` a host-side
     restatement of a sheet fact, which is a drift risk, so it is test-held. A
     tab's *x* cannot be restated at all (flex + text measurement), so the host
     asks `absolute_rect` rather than computing. **Any pane that composes a
     cambium widget inherits this shape**: the widget's geometry is knowable only
     from the layout, so ask it.

2. **Split / divider pane** — the pane furniture. Today turnstone hand-computes pane
   rects in `pane.rs` and has no divider drag. A cambium `split` (two children, a
   draggable divider, a ratio) owns the resize gesture and the seam, and both the
   frisket pane tiling and the Workbench's platen tiling pull it. This is the
   shell's tiling chrome becoming a component — the largest single migration, and
   the one that shrinks `pane.rs` to a walk over cambium splits.

3. **Sectioned list** — grouped rows under section headers, each row navigable or
   a button. Trail (Recent/This-node/Removed), Steward (active/queued), Alembic
   (Recent/Saved), and Apparatus's diagnostics all want it. Either a new `list`
   with sections or `action_list` grown a section grouping. Four consumers; a
   catalog primitive.

4. **Tree / outline** — a collapsible hierarchy. Gloss's document outline pulls it
   first; Roster's Graphlets tab and any settings tree follow. Two-plus consumers.

5. **Detail panel** — a pane-filling structured read view: labeled key/value rows
   in sections, some values rich (a trust badge, a parse-diagnostic list).
   `detail_popover` exists but is a transient popover, not a resident panel;
   Inspector and Roster's facet cards pull the resident form. Two consumers.

6. **Message list** — Comms's chat. One consumer, distinctive shape; compose from
   `el` + `arrangement` until Moot gives it a second.

Everything else a pane needs already exists: the `data_grid`, the full control set
(`button`, `checkbox`, `toggle`, `radio_group`, `select`, `slider`,
`text_field_typed`), the editors (`editor`, `styled_textarea`), `menu`,
`overlay_surface`, `detail_popover`, `command_surface`, and the sprigging leaves
(`GraphCanvas`/`graph_canvas_swatch`, `Swatch`, `Meter`, `Knob`, `GraphGlyph`).

## What stays non-cambium, and why

- **The Orrery canvas.** `mere::canvas` is the graph-truth-plus-physics
  presentation library — selection, layout, the arrangement geometry sidecar. It
  is not a widget; it is the space-view. sprigging's `GraphCanvas` leaf is the
  right tool for an *embedded* graph (a Gloss minimap, a swatch), not for the full
  interactive canvas. The canvas surface stays mere's, composited beside the
  cambium panes.
- **Live content documents.** genet's document sessions render web pages to a
  `netrender::Scene`. That is the engine, not the toolkit; it stays.
- **The host composition** — the surface plan, the compositor, event routing into
  each pane's runner. This is turnstone's job as the host; cambium owns pane
  interiors, not the window.

So the boundary is clean: **cambium owns what is inside a pane rect; turnstone owns
the rects, the canvas, the documents, and the composition.**

## Sequencing

The order follows pull and dependency, not pane-by-pane:

1. **Finish Roster on the grid** — event dispatch (`runner.dispatch_click`), which
   also proves the general pane-event path every cambium pane reuses.
2. ~~**Tab strip**~~ — DONE 2026-07-17. Unlocked Roster's four tabs; the first
   multi-consumer catalog addition. Only Nodes has a gatherer; Links / Graphlets
   / Fields say so until the edge-family walks land (no general edge iterator —
   `semantic_edges` / `arrangement_edges` / `containment_edges` each need one).
3. **Gloss** — minimap half DONE 2026-07-17: `graph_canvas_swatch` on the new
   leaf pipeline (`scene_from_dom_with_leaves` — sizes each `<custom-leaf>` from
   its laid-out box, repaints dirty leaves through the pane's `LeafRegistry`,
   splices at the box). Data via `Canvas::minimap_geometry`; node colour from
   mere's palette (NODE_SHEET carries into the minimap); a node click drains a
   Navigate intent -> `OpenAddress`. NOTE the swatch's interaction contract is
   **mirror-then-drain, not action bubbling**: its handlers are `Fn(&mut State,
   Id)` mutators, so the pane records intents in its own state and the shell
   drains them after dispatch — the tab strip's shape, not the grid's. The
   outline half still pulls **tree** when outline data lands.
4. **Split** — DONE 2026-07-17: `pane.rs` walks cambium's `Split::slots` /
   `divider_rect` (the component's pure state math is the geometry truth), each
   seam is a thin divider surface (empty scene over the seam clear), and drag
   works: press captures the seam, moves become ratios through
   `Split::ratio_at`, release persists once. NOTE turnstone consumes the
   component's MATH, not its view — a surface-compositing host needs the rects
   before layout and keeps one hit-test authority (the surface plan). The
   split's VIEW (slots + ARIA separator + keyboard resize + on_pointer drag)
   is built and tested in cambium for the in-tree consumers: the Workbench's
   platen tiling and stacked frisket panes pull it when they land. Both
   postures are deliberate; the module docs on both sides say which and why.
5. **Sectioned list** — DONE 2026-07-17 (`cambium::sectioned_list`): Trail
   migrated off hand-DOM onto it, and with it the fixed-height row geometry
   (`pane_rows`, `trail_scene`, both `row_at`) is DELETED — both list panes ask
   the layout for a row's rect now (Roster's grid, Trail's rows). NOT grown out
   of `action_list` (palette-shaped; a resident pane wants none of that filter
   machinery). Rows bubble their affordance (Navigate/Recover) out of the
   runner, so a Trail row's activation reaches the graph on the same spine as a
   keypress. Steward/Alembic/Apparatus follow when their data lands.
6. **Inspector** (detail panel), **Comms** (message list) as their data lands.
7. **Chrome/omnibar** onto `text_field_typed` + `command_surface` — the last
   hand-DOM holdout retires.

Each cambium addition is a small PR to the cambium catalog, justified in its
description by the turnstone surfaces that pull it — the serval/genet pattern, now
running with turnstone as the consumer.

## What the seam cost until 2026-08-16: a layout per pane per frame

Every pane built its scene through `ui::scene_from_dom*`, and each of those
constructed an `IncrementalLayout` from scratch. Thirteen call sites did it, so
a pane re-cascaded, re-boxed and re-shaped its whole document on every paint to
draw a screen that had not changed. `PaneScroll`'s own doc comment recorded the
consequence as a fact of life ("panes rebuild their `IncrementalLayout` every
frame; anything the layout retained would be discarded"), which is how a
workaround outlives the bug that motivated it.

Measured in release on the Device Receipts pane (37 cards, scratch profile,
frame timing in the shell's render loop):

| condition | mean frame |
| --- | --- |
| empty shell, 2 surfaces, 1024x600 | 6.1 ms |
| one Device Receipts pane, 1024x600 | 30.4 ms |
| the same pane at 3174x1729 | 47.5 ms |

**One pane cost four times the entire rest of the shell.** The window that
provoked the investigation had seven of them, which is what "moving the window
lags insanely" was.

Three things the measurement settled that inspection had guessed wrong:

- **It reflows.** The plan's right edge tracked 1024 to 3174 exactly on resize.
  What looked like a frozen resize was a 47 ms frame arriving late.
- **It does not spin.** The frame log went silent for 48 seconds while idle, so
  the renderer is properly change-driven. Each individual frame was simply
  expensive.
- **Debug and release differ enormously for the baseline** (34 to 48 ms against
  6 ms empty) **and not at all for the conclusion**: the content cost survives
  optimization.

The fix is `ui::RetainedLayout`, one per pane, holding the layout across frames
and bringing it forward from the DOM's own mutation batch. That is what
`IncrementalLayout` is for: it owns a persistent Stylist whose rule tree the
retained styles point into, a box tree, and a shaped-text context, and `apply`
carries all three. **The shape is borrowed, not invented:** cambium's
`frame::relayout` already solved this for the winit host's own document, so
this is that logic per pane rather than a second design of the same thing.

Rebuild remains the fallback for exactly three cases, and the reasons are worth
keeping distinct: a structural mutation (adding or removing nodes is the
relayout-scope path's job, not the attribute invalidator's), a size change, and
a different stylesheet (a layout's sheets are fixed at construction, because
rebuilding the Stylist under live rule nodes would dangle them). A rebuild
carries both scroll planes across, or a list pane would snap to the top every
time its content refreshed.

After: **6.9 ms with the pane open against 7.4 ms without**, and 588 of 589
frames took the `Unchanged` path with exactly one rebuild, the initial
construction. The pane is free.

Two things fell out on the way. The list panes hit-tested at offset zero while
painting scrolled, so a click after scrolling landed on whatever row shared
those coordinates at the top; routing hit tests through the retained layout
fixes that by construction, because paint and pointer now resolve against one
layout rather than two built moments apart. And `swatch_pane::hover` built a
layout per pointer move.

**The chrome is the one remaining one-shot site**, and knowingly: it lays out
through a `SubtreeView` rather than the DOM directly, so retaining its layout
means keeping that view stable across frames. The chrome card is a handful of
rows against a pane's hundreds, so it is the cheap site left rather than an
oversight. The one-shot builders stay for tests and single measurements, each
now carrying a doc line saying not to paint through them repeatedly.

Retention is guarded by count rather than by feel, in `ui::retained_layout_tests`:
ten identical frames build one layout, a resize reflows once and then settles,
an attribute batch restyles in place while a new node reflows, and a rebuild
keeps a scrolled pane where it was. `RetainedLayout::rebuilds()` is the seam
those read, and it is the number a regression shows up in: retention is not
"it looks fast", it is "the second frame did not rebuild."

Frame timing is now a `debug!` in the render loop
(`RUST_LOG=turnstone::shell::render=debug`) and the relayout path a `debug!`
per pane, so the next person to ask why a window feels heavy does not have to
instrument the frame loop by hand first.

turnstone `675f7f0`, 289 pass.
