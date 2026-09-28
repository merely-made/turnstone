# Graphshell tree migration inventory

**Date:** 2026-09-27
**Status:** implementation inventory for phase 4, not a completion receipt.
**Parent:** [Graphshell on one Cambium tree](2026-09-25_graphshell_one_tree_plan.md).

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

This remains the first migration slice. `TreePage` currently opens a fixture
with `GraphshellApp<MemoryBackend>` and retains only its graph. The main browser
page retains `GraphshellApp<IndexedDbBackend>`, product state and integrations.
Adding controls to the fixture does not migrate that application's persistence,
remote sessions, editors, or practice workspace. Keep existing pages available
until those behaviors have crossed the boundary and passed their receipts.

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

Each panel is done when its existing product effect, keyboard operation,
accessibility projection and relevant scenario pass on the tree. Full phase 4
also requires the parent plan's entire receipt and five-page gates.

## Pages and scenario obligations

There are 40 `.scn` files in `ports/graphshell/web/scenarios/` at this inventory.
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

## Next physics slice (proposal, not a new ruling)

**Core implemented; host adoption pending.** Following approval to continue,
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
The full native gate also passes all 88 Seiche tests. Canvas and browser
callers remain on the existing deterministic path so this core addition does
not change the current rendering measurements. The host work below remains
pending, including visibility wiring, host timestamp reset and headed evidence.

Rendering currently advances inline physics once per Canvas frame. Reuse the
bounded elapsed-time accumulator in `web_practice.rs::PracticeHost::frame`
(50 ms contribution cap, fixed 60 Hz steps), rather than adding a scheduler.
Seiche should own configurable maximum elapsed contribution and steps per
frame; Canvas should compose once after those steps. Keep `advance_frame` as
the deterministic receipt/test path. The actor branch must remain a snapshot
drain because its simulation already runs independently.

Reset accumulated time on pause, reseed, restore and suspension. The web host
currently discards the animation-frame timestamp and does not wire document
visibility to host hidden state; use the existing producer suspension path
when making that connection. Record executed steps and discarded elapsed time
beside physics cost. The current practice cap is a starting configuration,
not a latency guarantee: one expensive step can still block input.

Done means equivalent visible elapsed time produces equivalent fixed-step
motion at different render rates; stalls have bounded work; pause and hidden
time never trigger catch-up; restore and dragging still pass; actor progress
remains independent of rendering; and a headed moving-graph receipt records
step counts, dropped time and input response. This is separate from the
rendering optimizations measured in the current slice.
