# Graphshell tree migration inventory

**Date:** 2026-09-27
**Status (2026-10-08):** phase 4 in progress; saved-graph Title/Tags and durable
address/file intake have native and browser reopening receipts at `da466027a`,
based on `79fbbeb75` with Genet `15713014`. Remaining product migration and
large-graph responsiveness are open.
**Parent:** [Graphshell on one Cambium tree](2026-09-25_graphshell_one_tree_plan.md).

*Annotation, 2026-10-06 (S14 pass):* this inventory predates the physics
panel and the remote session on the tree. The Graph tools physics panel
landed on 2026-10-01 (`6279ca31`), which also moved the old page's physics
form onto a typed `PhysicsChoice` applied through `canvas_physics`. The
Remote session section landed on 2026-10-01 (`e9d95554`) and starts closed
since 2026-10-02 (`9adc4415`). So `TreePage` now holds the physics panel and
session state beside its graph, and the "First slice" and "Owners and seams"
paragraphs and migration steps 2 and 3 below describe the tree before them.
The [controls receipt](../testing/2026-09-27_graphshell_controls_physics_receipt.md)'s
2026-10-01 and 2026-10-02 sections record both. The dated inventory below preserves the earlier sequence; the current
annotation following it records the bounded intake progress.

**Current intake slice (2026-10-08):** `tree.html?app=local` now adds an address
and optional title through the existing `MereHost`, or a file through Cambium's
shared `open_file` seam. File identity hashes the returned bytes; name, media
type, length and modification time remain owner facets. Success waits for the
existing IndexedDB persistence acknowledgement, ingests the owner's graph,
and selects/opens that member without replacing camera, arrangement roles or
physics choice. Repeated address/file identity reuses the member. Refusals
leave the graph untouched; a failed write retains a pending member and Retry
intake. Selection is guarded during the chooser/write, while Tab remains
available. The floating panel is anchored below the wrapped toolbar, and
empty/long single-line fields have current-stack sizing receipts.

The [controls receipt](../testing/2026-09-27_graphshell_controls_physics_receipt.md)
owns the exact native and 1280/420 px browser evidence. Chooser cancellation
uses the real DOM cancel listener plus the headless shared callback fixture;
OS-dialog Cancel is still unverified. This completes the bounded address/file
part of step 3, not all detail actions or phase 4. Facets, relations, handlers,
representation, find/arrange, saved scene/transfer, projection/capture/practice
and the five public-page cutover remain open.

Mark approved proceeding to Cambium while treating rendering performance and
live physics as open work. A paused graph rendered through a producer establishes
the integration path; it does not establish moving-graph behavior or acceptable
cost.

## First slice

Move the canvas controls into the existing tree page: pan, zoom, fit, and
play/pause and restore arrangement, with actual captured pointer dragging. Both browser presentations
must call the same DOM-free `graphshell::canvas_controls::CanvasCommand` for
these operations. A new toolbar must not become a second implementation of the
canvas's camera or physics state.

The slice is done when the Cambium controls are named and keyboard reachable,
their actions reach the canvas, dragging uses the host pointer path, pause holds
positions and resume permits motion, and Mesquite records headed captures and
observations. Graph keys must apply only when the graph has focus. The slice
disables the proof's global `key_intercept` and checks actual focus in its
scenario. Observe simulation stepping separately from the render interval.

Mark's pause ruling is to freeze the currently visible positions. Stored
arrangement slots remain separate; Restore arrangement explicitly reapplies
them and pauses. Resume seeds from the frozen positions and currently clears
velocity. This establishes position continuity, not momentum continuity.

The default `TreePage` opens a fixture with `GraphshellApp<MemoryBackend>` and
retains only its graph. The opt-in `app=local` route now retains the existing
IndexedDB application, Title/Tags editor and durable address/file intake, as
recorded in the current annotation above and dated receipts below. The main
browser page still owns the broader product state and integrations. Keep
existing pages available until each remaining behavior has crossed the
boundary and passed its receipt.

**Corrected 2026-10-06 (S14 pass):** `TreePage` now holds more than its
graph. The Graph tools physics panel (`physics`, `6279ca31`, 2026-10-01) and
the Remote session section (`session`, `sections`, `draft`, `remote_seen`,
`remote_link_seen`, `tools_open`; `e9d95554`, 2026-10-01, and `9adc4415`,
2026-10-02) are on the tree, on both tree routes
(`ports/graphshell/src/web_tree.rs`).

## Owners and seams

| Concern | Existing owner and integration point |
| --- | --- |
| Graph and product operations | `ports/graphshell/src/app.rs`: `GraphshellApp<B>`; `mere_host.rs` and `product.rs` retain product authority. |
| Canvas rendering, picking and physics | Pictograph `Canvas`; `web_tree.rs::CanvasProducer` borrows the host's `ProducerContext.core`. Retain this producer rather than replacing it with Cambium's separate graph swatch renderer. |
| Canvas commands | `canvas_controls.rs::CanvasCommand`; browser `web.rs::pan`, `zoom`, `fit_content` and `web_product.rs::toggle_physics` call it. Bindings choose distances; Canvas owns state. |
| Application state still in the old presenter | `web.rs::BrowserHost`: IndexedDB app, remote session and physics board, selection/detail, action draft, saved scene, filters, representation, projection editor, live projection and practice state. Separate these from `GpuPresenter`, canvas DOM handles, chrome scene and scenario plumbing as each surface moves. |
| UI components | Cambium `button`, `text_field`/`textarea` with `TextInput`, `checkbox`, `select` with `SelectState`, `tab_bar_view`, `detail_panel`, `status_bar`, `open_file`. Use `lens` to bind control state. |
| Input | Cambium `on_pointer` and `PointerPhase` provide primary capture and leaf-local coordinates. Forward these to Canvas down/move/up. `on_wheel`, focus and keyboard routing stay shared. |
| Browser host | `cambium-genet-web-host::mount`, accessibility mirror, file chooser and `WebCapture`; preserve one host renderer and device. |
| Scenario lifecycle | Mesquite `Lane<Product>` owns stepping, capture completion and result handling. `web_tree/lane.rs` supplies product observations and semantic actions. Taproot retains selector grammar. |

The old physics form combines DOM reads, Canvas setters and DOM synchronization
in `web_product.rs::apply_physics_from_form` and `apply_profile_from_form`.
When moving that form, split typed inputs from browser adapters and reuse
`CANVAS_PHYSICS_*` catalogs. Preserve source-before-law application order and
profile synchronization. Do not copy the form handler into the tree.

**Corrected 2026-10-06 (S14 pass):** this split landed in `6279ca31`.
`apply_physics_from_form` now builds a typed `PhysicsChoice` and calls
`canvas_physics::apply_physics`, and `apply_profile_from_form` calls
`canvas_physics::apply_profile`.

## Shared capabilities to check before dependent controls move

- Basic buttons, text fields, checkboxes, selectors, tabs and file requests
  already exist. Label each control explicitly and verify its projected role,
  name and value. A Graphshell-only replacement widget is unnecessary.
- Cambium's `WheelEvent` has delta, local position, size and propagation but no
  modifier state. The old browser reads Ctrl+wheel for zoom. Preserve that
  gesture through a shared input extension before claiming input parity.
- The first slice routes captured primary down/move/up through the existing
  host pointer path and confines graph keys to graph focus. Middle-button
  parity and Ctrl+wheel remain open shared-input work.
- A settled canvas still needs later raster frames after asynchronous Vello
  buffer recovery. Keep continuous rendering until the renderer supplies a
  reliable completion/invalidation contract; physics settling alone cannot
  justify caching the first texture.
- The plan records Genet layout/clipping limitations. Verify the actual panel
  composition and narrow layout before expanding it; do not assume a valid DOM
  tree is a valid painted frame.

## Remaining migration sequence

1. Share typed canvas commands and prove the tree toolbar and live pointer path.
2. Move actual app boot and state behind the tree: IndexedDB reopen, selection,
   local/remote switching and detail. Reuse `GraphshellApp`, not another fixture
   host. Move painted chrome into the same retained view.
3. Move panels by behavior: add/file and detail actions; find/arrange/physics;
   saved scene/transfer; projection editor and executable projection; capture
   history and practice. Extract typed state/actions from DOM adapters as each
   panel moves. Reuse the existing components above.
4. Route the public component mount through the web host on all five pages.
   Retire `web_gpu.rs` and `component.html` only after their last live consumer
   has moved. Remove the old scenario pump when its product semantics and
   observations have migrated to Mesquite.

**Annotation (2026-10-01):** Mark ruled the physics panel next, ahead of the
rest of step 3, together with the remote session from step 2 that two of its
scenarios need. The rulings (base, done-condition, routes, execution) are in
the parent plan's §1, "Phase 4 physics-panel rulings".

**Corrected 2026-10-06 (S14 pass):** both have moved: the physics panel
(`6279ca31`, 2026-10-01) from step 3 and the Remote session (`e9d95554`,
2026-10-01; `9adc4415`, 2026-10-02) from step 2 are on both tree routes.

Each panel is done when its existing product effect, keyboard operation,
accessibility projection and relevant scenario pass on the tree. Full phase 4
also requires the parent plan's entire receipt and five-page gates.

## Pages and scenario obligations

There are 40 `.scn` files in `ports/graphshell/web/scenarios/` at this inventory.
**Corrected 2026-10-06 (S14 pass):** that count is the tree before this
inventory's own commit: 40 at `bb52201c^`, 87 at `26060e88`.
Preserve assertions about product behavior while replacing component DOM verbs
with generic Cambium actions and observations. External embedding-page controls
remain browser DOM controls and need a browser-level boundary test.

| Page | Behavior that its tree mount must preserve |
| --- | --- |
| `index.html` | Full-page sizing, title ownership, local IndexedDB and remote-session behavior. |
| `embed.html` | Container sizing, host title, surrounding content, colliding unprefixed IDs, host keyboard and button independence. |
| `practice.html` | Practice state, persisted reopening, projection behavior and reduced-motion preference. |
| `practice-embed.html` | The same practice state in a bounded embedding, without taking the embedding page's title or input. |
| `co_op.html` | Graphshell's retained executable projection, supplied through `co_op.js`, plus the surrounding shared-practice proof's lifecycle and controls. The wrapper's controls are outside the Graphshell component today. |

Scenario groups cover H3 boot; C4 full-page/embed/live/reconnect; physics laws,
profiles, sources, add, drag and remote board; projection authoring/reopen/
refresh; derived faces/detail; practice and reservoir reopen/migration;
Distillery binding/projection; co-op retained state; and phase-3 comparison.
The two `p3_presenter_*` scenarios are historical comparisons once the presenter
retires, rather than evidence of ongoing product behavior. Keep their original
receipts. Classify old runs explicitly; re-run every still-live behavior on the
tree and inspect its whole-frame captures.

The existing physics scripts carry useful behavior assertions, but run through
the old DOM adapter. Porting them must retain settling laws versus deliberately
restless laws, drag return/hold semantics, pause/resume and the remote-board
boundary. Their existence is not a current passing tree receipt or a performance
acceptance result.

## Elapsed-time physics slice

**Core implemented; host adoption added 2026-09-29.** Following approval to continue,
Seiche now exposes opt-in `Physics::advance_elapsed`, `ElapsedStepConfig` and
`ElapsedStepReport`. It accepts caller-supplied elapsed time, runs bounded
fixed steps, publishes one snapshot, reports discarded time, and carries only
a substep fraction. Defaults are a configurable 50 ms contribution cap and
three-step cap. `reset_elapsed`, seed, halt and deterministic advancement clear
fractional debt; the actor path only drains accepted snapshots and sends no
stepping commands. `TICK_DURATION` rounds nominal 60 Hz to 16,666,667 ns,
explicitly documented and tested alongside the unchanged solver `TICK_DT`.

The focused runtime gate passes 12 tests with default features and eight with
`--no-default-features`, both offline and locked. These cover render-rate
equivalence, long stalls, zero limits, rounding boundaries, idle/pause/reseed/
suspension resets, deterministic one-step compatibility, and actor isolation.
The original full native gate also passed all 88 Seiche tests. Those earlier
rendering measurements used deterministic advancement and remain historical.
Pictograph now offers `frame_at` and `frame_profiled_at`: they accept monotonic
host timestamps, compose once after the bounded steps, and expose the last
`ElapsedStepReport`. The first call after reset establishes a baseline.
Backwards/repeated timestamps do not accrue time. Pause/resume, explicit seeds,
restore and producer suspension clear the timestamp and fractional debt.

Rootstock's `Host::redraw_at` scopes the supplied timestamp to one draw and
passes it in `ProducerFrameInfo`. Untimed redraw remains available. The web
host forwards animation timestamps, uses the same performance clock for
immediate input draws, and routes document visibility through `set_hidden`.
Hidden pages stop drawing; suspension resets the Canvas clock and visibility
requests a fresh frame. Actor simulation retains its independent pacing.

The Graphshell tree opts into timed advancement, with configurable caps from
`physics_max_steps` and `physics_max_elapsed_ms` page parameters. Defaults are
three steps and 50 ms. Timing receipts include executed steps, discarded
microseconds and carried microseconds beside physics cost. Existing native
Canvas callers, the old presenter, camera inertia and backdrop pacing are
unchanged. One expensive physics step can still block input; the caps are not
a latency guarantee.

Done means equivalent visible elapsed time produces equivalent fixed-step
motion at different render rates; stalls have bounded work; pause and hidden
time never trigger catch-up; restore and dragging still pass; actor progress
remains independent of rendering; and a headed moving-graph receipt records
step counts, dropped time and input response. This is separate from the
rendering optimizations measured in the current slice.

## Local saved-graph slice (2026-09-29)

`tree.html?app=local` mounts the portable `GraphshellApp` over the existing
`graphshell-reference-host-h5` / `muniment` IndexedDB store and the same
`profile:graphshell-h3` selection. It reopens the saved graph instead of
constructing a separate comparison fixture. Generated-graph parameters cannot
be combined with this route.

Selecting an object exposes Open details; Enter on the focused graph also
opens the selected object's panel. Cambium's named Title and Tags fields edit
a draft, with host caret and IME routing. Save changes validates the member,
normalizes metadata and awaits persistence. A refused write reports failure
and keeps edits available for retry. The asynchronous task owns the app, and
selection and draft editing are held while the save is in flight.

Successful saves update canvas metadata through its existing title/tag seams.
They preserve geometry, camera, selection and physics state. A projection
refresh error after persistence is described as a refresh failure after a
successful save, rather than a failed storage write.

The paired `p4_tree_saved_edit` and `p4_tree_saved_reopen` scenarios exercise
the rendered fields and a separate load of the same browser profile. Their
receipts export session and member IDs for comparison across loads. This
bounded route does not complete phase 4: remote sessions, saved-scene
restoration, other product panels and the five public wrappers remain open.

Four focused native tests pass for durable identity/metadata reopening,
stale-selection rejection without writes, refused-write retry and Canvas
state preservation. On 2026-09-30, the paired headed edit/reopen scenarios pass
on the merged current-main bundle. The independent load retains the same
session and member UUIDs, `TreeSavedTitle` and normalized `alpha, beta` tags.
Controls and elapsed-physics scenarios also pass. A real background-tab mount
passes its first visible resume after about 32 seconds hidden, with a
preexisting accessibility mirror and no hidden producer calls. Moving-graph
hide/show and intentional hidden-timing rejection remain pending. The
[controls receipt](../testing/2026-09-27_graphshell_controls_physics_receipt.md)
records the exact baseline, hashes and qualified diagnostic timings.

Mark describes current physics as slightly laggy but acceptable, while leaving
room for different layouts and physics laws to behave differently at different
scales. This is a user observation and a comparison hypothesis. Current
generated-graph timing receipts do not establish the performance of other
layouts, laws, graph densities or hardware.
