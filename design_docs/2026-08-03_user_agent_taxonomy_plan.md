# User-agent taxonomy â€” the things a browser owes its user, by spec

**Date:** 2026-08-03
**Status, 2026-10-06:** in progress. Browser taxonomy and the Scry/Weld/Servo
integration take priority in this pass. The October implementation and release
qualification below supersede the August ordering and point-in-time inventory.
B0 is qualified. The current Windows Scry B1 and direct Weld B2 consumer
qualification is recorded in the [October 6 receipt](../docs/receipts/browser_scry_windows_20261006/README.md).
Servo B3 and the broader browser operations remain separate next slices.
The ask: all the things a browser needs, by
spec, on the **user-agent side**, not the engine side. Engines render;
the UA owns the browsing-context features around them. This plan inventories
those obligations against what Turnstone has and names the graph-native home
for each, because Turnstone is a graph browser and several classic UA
surfaces already exist here in a different, better shape.

## Historical inventory (verified 2026-08-03)

This table preserves the original framing. It is not the current acceptance
ledger: zoom, downloads, cookie persistence and pane ownership changed after
this inventory. The October matrix and dated consumer receipts below govern
the current implementation sequence.

| UA obligation | Spec anchor | Turnstone today | Graph-native home |
|---|---|---|---|
| Session history (back/forward, restore) | HTML session history | Trail pane + codicil-backed truth | Present. Trail IS history; per-tile back/forward reads the trail, not a parallel stack |
| Bookmarks | (convention, not spec) | Absent as a list; every kept node is bookmark-shaped | A content class / collection over persistent nodes, not a separate store. Taxonomy work, see below |
| Downloads | HTML `download`, Content-Disposition | Live with durable custody and collision-safe destination metadata | A download is a fetched representation with source URL, response metadata, exact bytes, and a Steward projection |
| Find in page | (UA convention; interacts with engine text) | Live through the owning retained or hosted engine | Turnstone owns the captured target and field; engines own matching, selection, and reveal |
| Page zoom | CSS `zoom`/UA zoom | Capability status is disclosed per live engine; canvas and UI zoom remain separate | Persist document scale per node or engine policy, then offer only controls the selected engine supports |
| View source | UA convention | Absent | Cheap and honest: the fetched bytes are already held; a source view is a content class |
| Reader view | (convention) | Live as `genet.reader`, derived by Fleece from held HTML | Reader is a viewer lane in the engine picker; extraction lineage remains attached to the representation |
| Trust / security posture | TLS UI conventions; per-protocol postures | Smolweb fidelity WS2 scoped, unbuilt; place lanes have real authority chrome | One posture vocabulary across https/gemini/reticulum/place lanes, shown in tile chrome (fidelity plan owns the descriptor) |
| Permissions (notifications, media, geolocationâ€¦) | Permissions spec | Live host-owned permission decisions; authentication callback delivery remains partial in Weld/CEF | Web-content requests retain exact node/request identity; standing permission and process-only credential policy remain separate |
| Per-site settings | (UA convention) | Per-node viewer override only | Per-host overrides exist in `EngineRoutePolicy`; widen to a per-host settings sidecar as needs appear |
| Cookies / storage inspection | (UA convention) | N/A (static lanes carry no cookie jar; scrying tier does) | Surface-engine profile dirs are the boundary; inspection UI deferred until the scrying lane lands |
| External protocol handling | HTML external handlers | `host.external-protocol` fallback exists, invisible | Engine plan E4 makes it legible |
| File upload/save pickers | HTML forms; save dialogs | Absent | Arrives with downloads + scripted forms; platform dialogs via the winit host |
| Context menus | UA convention | Absent | Pane/node action surface; interacts with the action-list flagship component |
| Print | (convention) | Absent | Deferred, recorded not planned |
| Autofill / credentials | Credential Management | Absent, deliberately | Personae owns identity; web-credential autofill is out of scope for now |

The remaining gaps identified by this August inventory were
**per-node page zoom, view source, and bookmark taxonomy**, plus making
existing invisible things legible (external protocols, trust). Downloads,
retained find, and the reader lane landed in the later browser-surface work.

## The taxonomy half

"Bookmarks" is where the taxonomy enrichment lands. Turnstone ships two
content classes today (`turnstone.web-page`, `turnstone.note` in
[content_classes.rs](../src/content_classes.rs)), registered through the same
chartulary seams a pack would use. The browser taxonomy grows the same way:

- classes for the artifact kinds the UA mints: downloaded file, source view,
  reader rendering, feed subscription, place, contact;
- membership/collection semantics so "bookmarks" is a user-curated collection
  over nodes (any class), not a parallel store;
- facets carrying the UA metadata each class owes: provenance and disposition
  on downloads, extraction lineage on reader renderings, posture on pages.

Nothing privileged: packs can define siblings. That rule is already in the
file's charter and it holds here.

## Steps

### U0. Downloads

Content-Disposition and `download`-attribute handling on the fetch lane: an
attachment becomes a file on disk plus a `turnstone.download` node carrying
provenance (URL, timestamp, size, disposition). Steward shows in-flight
progress honestly. Done when a download from a page yields a node whose file
opens, and a failed one shows the error on the node.

### U1. Find in page

A UA find bar (per focused tile) over the session's text. Uses the existing
structure/outline facts where they suffice; where they do not, the engine
seam grows a text-run query, which is engine-side plumbing serving a UA-side
feature. Done when find highlights and steps through matches on a static
page.

Accepted 2026-08-26 through the shared Inker retained-find contract. Livery
retains structural matches and reveal; Weld retains selection internally and
reports authoritative count/current state. Turnstone keeps the exact target,
query request, stale-answer guard, chrome, accessibility status, and
observation projection.

### U2. Per-tile zoom + view source

Per-node zoom was accepted on Livery and Windows Weld on 2026-08-27. Requested
scale persists; Weld effective readback remains Partial. Source-document
capture now stores response bytes, but a general source-view consumer remains
open. These are separate outcomes, not an unfinished zoom implementation.

Zoom is a document setting surfaced on the tile and persisted per node or
engine policy; view source is a content class over already-held bytes. The
shared capability report must precede the control so unsupported engines stay
honest. Done when Ctrl+wheel on content zooms the page without changing chrome
or the canvas camera, and view source opens for any fetched node.

### U3. Reader view as a lane

`genet_extract` promoted from title-only to a reader rendering, selectable
as a viewer lane in the engine picker (it is a rendering choice, so it lives
in that seam, not a separate toggle). Extraction lineage recorded on the
node. Done when a cluttered page has a readable lane and switching back is
lossless.

### U4. Bookmark taxonomy

The collection class + membership UX over existing nodes; keep/discard stays
the athanor's. Done when a user can curate a named collection, and it is a
pack-expressible structure, not a privileged store.

### U5. Legibility passes

External protocols (engine plan E4) and the unified trust posture (fidelity
plan WS2, widened to the reticulum posture from the
[Reticulum browsing plan](2026-08-03_reticulum_browsing_plan.md)). Owned by
those plans; listed here because they complete the UA obligations table.

## Not in scope

- Web-credential autofill and password management (Personae is identity;
  filling site forms is a different, riskier feature, deliberately parked).
- Print.
- Cookie/storage inspection was deferred in the original scope. The October
  browser matrix includes it as a separate product consumer gate.
- Engine-side capabilities (that is the
  [engine adoption plan](2026-08-03_turnstone_engine_adoption_plan.md)).

## Ordering

U0 and U2 are independent and small. U1 needs a look at what the structure
facts actually carry before committing to the engine-side query. U3 rides
the engine picker (E0). U4 is taxonomy work that can go any time. U5 belongs
to its owning plans.

## October browser integration

**Direction, 2026-10-05:** Mark asked to orchestrate browser taxonomy
implementation around the wgpu trio, recheck Turnstone against the current
stack, and use the application to expose practical producer requirements and
remaining release gates. This is an application consumer pass. It does not
authorize publication or establish a new universal browser runtime.

### Ownership

| Owner | Responsibility |
| --- | --- |
| Turnstone | Browser actions, target identity, graph admission, navigation policy, engine choice, profiles, permission decisions, and user-visible outcomes |
| Mere / Inker | Engine registries, routing and the shared document/browser-surface contracts and adapters |
| Genet | Native document behavior and reusable host/window/rendering mechanics |
| Scry | System-webview adaptation, events and native frame production |
| Weld | CEF adaptation, subprocess/runtime integration requirements and native frame production |
| Graft | Native resource import into the host's wgpu device; its Servo adapter/demos additionally illustrate upstream Servo embedding |

`mere-surface-api` is application surface discovery and settings vocabulary.
The browser control protocol is currently in Inker's `surface_engine.rs`.
They are different seams. The neutral browser protocol proposed by Mere's
surface assessment must not be treated as delivered solely because
`mere-surface-api` exists.

### Browser vocabulary and acceptance matrix

Each row follows one identified browsing context through an owning engine.
Opening a replacement document or switching a viewer invalidates pending work
for the old context. A graph node, a browser's native history entry, a profile,
and a captured representation remain distinct identities.

| Browser obligation | Host outcome | Backend obligation | Consumer gate |
| --- | --- | --- | --- |
| Engine choice | Selectable available engines and labelled unavailable choices; saved pins remain visible | Registration and explicit construction failures | Default and feature builds; preserved unsupported pins; switching and restart |
| Navigate / back / forward / reload / stop | Exact target, actual address/title/loading/failure and history state | Ordered navigation events and effective controls | Local redirects, failed load, stop, history and late-event rejection |
| Focus and input | Correct page receives keyboard, pointer, clipboard and composition | Platform input/focus APIs and honest IME limits | Two simultaneous pages, focus switching, text entry, resize and scale |
| Profiles and site state | Configurable profile boundaries and persisted settings | Profile-backed cookies/storage and asynchronous operations where needed | Restart and separate-profile isolation; no promise of cross-engine cookie portability |
| Find and zoom | One action vocabulary; requested and applied zoom remain distinct | Match count/current state/reveal and applied scale, or an explicit limit | Same local fixture across registered engines; chrome/canvas unchanged |
| Links / popups / external protocols | Explicit destination/admission decisions tied to the originating context | Source context, address and popup/lifecycle events | New-window request, denied request, external scheme and parent loss |
| Permissions / authentication | Exact request decisions; origin and lifetime visible; secrets excluded from graph/diagnostics | Correlated callbacks and terminal outcomes | Allow/deny/dismiss, navigation withdrawal and actual backend callback receipt |
| Downloads / upload / save dialogs | Destination decisions and durable downloaded-byte custody | Backend request/response metadata, completion/failure and picker callbacks | Actual hosted-page download and upload; fetch-lane success is a separate claim |
| Inspection / source / script / cookies | Clearly distinguished source, document facts and backend tools | Ordered asynchronous results with caller identity and honest tool availability | Unsupported and failed controls; no native DevTools call when backend refuses it |
| Capture / PDF / print | Node-attached provenance and separate artifact kinds | Correlated acquisition and completion or a typed unavailable outcome | Exact viewed page, cancellation/replacement and durable bytes; graph capture is separate |
| GPU composition / resource custody | Import on the existing host device; correct placement and input transform | Owned resource transport, synchronization and release | Imported pixels, multiple surfaces, resize, teardown and deliberately failed import |

Support is recorded at four separate levels: library API, adapter forwarding,
Turnstone product consumption, and executed consumer evidence. A library
capability is not automatically a Turnstone capability. Backend distinctions
and typed unsupported reasons stay visible; no lowest-common-denominator
promise hides capabilities that one engine can actually provide.

### Research lanes

1. Turnstone: current pins, registry construction, browser actions, control
   surfaces and accepted scenarios. Preserve the existing portable dependency
   graph until a specific changed interface requires an integration set.
2. Mere/Genet: current shared contracts and adapter wiring, source boundaries,
   and host donor implementations. Compare pinned sources with current main.
3. Scry: platform ceilings, Windows construction/focus/multiple-page behavior,
   and adapter losses. Separate compile coverage from native/pixel receipts.
4. Weld/Graft: CEF control/event forwarding, owned frame import, Servo host
   requirements, and exact publication/tag/consumer evidence.

### Implementation lanes and done-conditions

**B0. Engine inventory and truthful controls.** Project available registrations
and supported lazy construction into application data. Preserve an unknown or
unavailable saved engine as an explicit choice with a reason. An unavailable
row cannot invoke a viewer change. Inspector, actions and automation read the
same inventory. Remove claims that the backend explicitly refuses.

Done when a newly registered document engine appears without editing a picker
array; optional/runtime/platform gaps are labelled; an unsupported saved pin
does not silently look like Auto; target-bound selection and stale-pane guards
remain intact; default and relevant optional-feature consumer tests pass.

**B1. Windows Scry consumer.** The bounded primary-Workbench slice is qualified
under `--features scry,weld` on Windows in the
[October 6 receipt](../docs/receipts/browser_scry_windows_20261006/README.md).
The pinned Inker Scry adapter resolves `scrying` 0.7.1 at immutable queue repair
`39818a7` on wgpu 30. The host shares an owned
offscreen `CompositionRoot` and captures each page visual independently;
Turnstone's renderer composes its owned frame payload on the existing device.
Each producer has a dedicated fence, and the importer validates its identity
and waits on every paint, including a reused allocation. Profiles remain
explicit per-node directories under `scry/webview2-profiles/<node>`.

The current acceptance scope is two distinct pages in the primary Workbench.
The [October 5 control](../docs/receipts/browser_scry_windows_20261005/README.md)
preserves the earlier failures. Mere and Genet repository pins remain unchanged;
the October 6 root patch selects the immutable Scry queue repair.
In the earlier control, the full native script has four cookie assertion failures; separate
process restoration has only two cookie assertion failures. DOM input/owner,
switch/resize, zero-resource close and saved pin/localStorage/text restoration
checks pass. Its manual review finds stale images after input and blank reopened
tiles despite positive import/wait counts. These are bounded receipts, not
full B1 acceptance; the October 6 runs close cookies and current pixels.
Browser commands now follow the active Workbench member
without replacing graph selection; tab hit boxes stay inside their painted
cell even when titles are long.

Done when a local fixture is visible through Scry in a real Turnstone window;
navigation and supported input have observed outcomes; find/typed zoom refusal
is visible; two pages coexist and receive input independently; primary-pane
resize, switching, profile restart and teardown are qualified. Forwarded CDP
key/text delivery is distinct from physical keyboard and OS IME acceptance.
Typed find/zoom forwarding, IME, lenses, native rehosting and different-size
appearances of one node remain explicit gates. A single-page demo does not
close the simultaneous-page gate.

**B2. Weld adapter completeness.** The October 6 Windows consumer adopts the
direct `welding-0-15` adapter, preserves owned-frame and ordered-event custody,
and qualifies the bounded native input/find/zoom/permission/teardown slice.
The narrow mouse/CHAR bridge remains until Mere closes its shared input gate.
Keep runtime/subprocess/sandbox and
profile policy application-owned. Accepted script/cookie completions must not
be discarded into diagnostic strings when those operations are exposed.

Done when actual instance capabilities drive controls; unsupported native
DevTools stays refused; the chosen frame path keeps structural custody;
ordered/correlated events reach the owning consumer; find/zoom/permissions and
teardown retain their real native receipts. Capture, popups and browser-owned
downloads receive separate gates as their consumers land.

**B3. Servo/Graft consumer.** Reuse the existing adapter seam after locating
the live Servo construction and event owner. Graft's GPU import dependency in
the Weld build is not evidence that Turnstone already hosts Servo.

Done when an upstream Servo page renders in the current Turnstone device,
receives input, reports supported browser controls and survives resize and
teardown. Missing Servo browser APIs are attributed to the adapter/runtime,
with a typed unavailable result rather than invented equivalence.

**B4. Browser operations.** Promote the matrix rows into bounded product
slices: browsing-context lifecycle first, then popups/external schemes,
profiles/site state, transfer dialogs/custody and inspection/capture. Reuse
the existing action and observation spine. Each slice adds one fixture and
its cross-engine result; it does not create parallel per-engine app policy.

Done when every exposed action has an exact target, a capability-backed offer,
a terminal observable result, and the specified restart/privacy behavior.
Absent backend functions remain attributable and visible.

### Release qualification

The trio's authoritative release plan stays in
`wgpu-graft/design_docs/2026-09-03_wgpu_triplet_release_plan.md`.
Turnstone supplies an additional real consumer, not a replacement for the
producer hardware batteries. Record exact source revisions, feature/wgpu rows,
platform/runtime versions, commands and inspected outputs. A version bump,
source compile, imported-handle test and imported-pixel native run are separate
claims. Publication requires its own authorization and exact-source receipt.

### Findings

- **2026-10-05:** Turnstone main is `74a4689`, Mere main `c36641d6`, Genet
  main `f9c8b0a7186`; the trio checkout heads are Scry `99c0a9d`, Graft
  `403a30c` and Weld `65d057d`. Recheck before integration: Mere and Genet
  are actively changing in other lanes.
- **2026-10-05:** Turnstone pins Mere `bd5912fb` and Genet `69a2383b`;
  its direct Weld/Graft pins already match those trio checkout heads. Staleness
  must therefore be measured at the consumer and adapter boundaries rather
  than inferred from dependency age alone.
- **2026-10-05 initial audit:** `src/inspector_controls.rs` contained a
  feature-conditioned `VIEWER_OPTIONS` array, and unknown saved pins mapped to Auto.
  `src/shell/mod.rs` registers native document sessions and lazily constructs
  Weld on Windows; it does not construct Scry or Servo/Graft.
- **2026-10-05 initial audit:** `src/shell/weld.rs::web_capabilities` claimed native
  DevTools support, and `apply_settings` invokes it, although current Weld
  explicitly refuses that native window. The supported CDP path has a
  different contract and is not a Turnstone inspection surface yet.
- **2026-10-05 initial audit:** comparing the pinned and current Mere sources found no
  change in `crates/inker` or `crates/system/surface-api`. The checked Genet
  document-session, host API, document-engine and winit-host sources are also
  unchanged. Those particular seams do not require a broad repin.
- **2026-10-05:** hosted back/forward currently calls the owning
  `WebSurface` history controls (`src/shell/effects.rs`), despite the August
  table saying every per-tile action reads Trail. Trail traversal and native
  browsing history must remain distinct and receive a defined relationship
  before adding broader browsing-context lifecycle policy.
- **2026-10-05:** Scry's shared `CompositionRoot::new_attached` supports
  multiple producers on one HWND. Independent roots on the same HWND remain
  unsupported. Its offscreen capture constructor has a different input gate;
  attached-root keyboard evidence does not close offscreen-host input.
- **2026-10-05 initial audit:** Mere's Scry adapter retains owned frame payloads, while
  Turnstone's importer accepts only raw D3D12 handles. A reused allocation must
  still honor each paint's synchronization; caching the texture by epoch
  cannot skip a producer fence wait.
- **2026-10-05:** Scry's adapter does not implement typed find/zoom or host
  authentication answers despite its broader translated capability claims.
  The local Mere patch now reports those limits explicitly; forwarding them
  remains a Mere adapter gate. Servo/Graft also needs a real host composite
  and an ordered producing event queue, beyond Graft's texture importer.
- **2026-10-05:** fresh crates.io API reads report `grafting` 0.6.0,
  `grafting-frame` 0.1.0, `scrying` 0.7.1 and `welding` 0.14.1 as latest
  stable releases. Weld main is the unpublished 0.15 line. The previous
  registry-only four-host release receipt remains historical and scoped to
  its exact published sources.

### Product checkpoints

Keep the current per-node Weld profile behavior during B0. Selectable shared
profiles and cross-engine state transfer need an explicit product decision;
they are not implied by adding an engine picker. Likewise, document navigation
within a live surface, graph traversal and popup admission need a stated
relationship before changing their default behavior. The browser taxonomy
records these forks without silently choosing new policy.

### Progress

- **2026-10-05:** research lanes dispatched with read-only ownership; source
  edits will follow bounded owner assignments. Existing `.github/` WIP in
  Turnstone and active Mere/Genet builds are preserved. No new worktree is
  needed for the initial Turnstone consumer changes.
- **2026-10-05:** B0 source is implemented: registry-derived inventory,
  human labels, stable engine IDs, runtime availability and refusal reasons,
  preserved unknown saved pins, observation parity and failed-pin recovery.
  Default all-target compilation passed. The
  [qualification receipt](../docs/receipts/browser_engine_inventory_20261005/README.md)
  records 635 passing library tests (9 existing ignores), 14 Inspector tests
  in default and Weld builds, six Weld contract tests, and a passing native
  picker scenario with reviewed captures. The native run exposed ordinary
  scoped clicks being refused by scenario hooks; those hooks now defer to the
  shared pane probes, and the failed run remains preserved.
  At the B0 receipt boundary, Scry and Servo remained unavailable until their
  real factories were installed.
- **2026-10-05:** Mere's Scry capability projection was corrected where its
  command forwarding is absent or event payloads are incomplete. All 21
  `scrying-engine` library tests passed. Turnstone's local Weld patch refuses
  unsafe native DevTools before calling the producer.
- **2026-10-05 B1 source:** the optional Windows Scry factory and owned-frame
  importer are now implemented locally, without advancing repository pins.
  A shared offscreen composition root keeps native webviews from covering the
  product window. Dedicated per-producer fences avoid Scry's independent
  producer counters satisfying a sibling's wait. Every paint is synchronized;
  replacement retires old producer/cache/importer identity. Mouse input uses
  WebView2's mouse API with correct buttons and Shift/Control, while key/text
  dispatch uses the forwarded CDP route. Lazy capture/resize/import failures
  retire the producer and report failed content. Full qualification is **gated
  on native cookies and current pixels** under the [B1 receipt](../docs/receipts/browser_scry_windows_20261005/README.md).
  The target is two distinct primary-Workbench pages, with independent input,
  profile persistence, resize, switching and teardown. Typed find/zoom,
  permission/auth answers, correlated capture/PDF, DevTools opening and
  accessibility projection remain unavailable through the pinned adapter;
  physical keyboard, OS IME, lenses and native rehosting remain separate gates.
- **2026-10-05 B1 native:** executable `a245130a` completes the full script
  without external repaint. Only four native cookie assertions fail; the
  separate same-profile restart fails only its two cookie assertions. DOM
  input, member selection, switching, zero-resource close, and restored pins,
  localStorage and text pass. Pixel review additionally finds stale click/scroll
  images and blank reopened tiles, so frame counters do not close presentation.
  Next owner slices are native CookieManager/profile correlation, correlated
  captured/imported/composited images, and producer-to-host wake notification.
  The separate Mere graph-semantics C12 lane proposes appended
  `ReplaySetResourceRecordById`, `ReplaySetResourceEdgesByIds` and
  `ReplaySetShownResourceById` capture variants, preserving old fields and
  postcard ordinals. A future Turnstone consumer patch owns the exhaustive
  `behaviors.rs` match; the existing edge test literal remains compatible.
  Mark subsequently authorized the coordinated consumer patch. Its exhaustive
  graph-aware match and six regressions are prepared, unapplied, in the
  [P2 consumer receipt](../docs/receipts/graph_semantics_p2_20261005/README.md).
  Rust syntax and patch applicability pass. The exact proposed scope functions
  and all six regressions compile/pass against the real kernel/servitor API at
  reviewed Mere commit `cff35712a28419f921a3cb2ffe79ef02268af6c5`;
  209 input fingerprints are stable across that run. This excludes the GUI/App
  drain; compatible dependency and main integration review remain open.
  The patch and a tested
  integration pin stay outside the frozen Scry receipt's pins and proof scope.
  Resource changes
  must fan out to current showing surfaces; historical migration effects must
  retain their explicit resource endpoints rather than infer them from current
  surface URLs.
- **2026-10-05:** live CI closes Weld's old NVIDIA parity gate at `65d057d`.
  Scry Intel base capture cadence still fails acceptance, and both RADV
  hardware jobs stopped at stale logind session preflight. Local Scry/Weld
  preflight fixes passed eight mocked cases each; native reruns remain open.
  The cross-repository release plan records the exact runs and claim limits.
- **2026-10-06 B1/B2 Windows native:** the final locked production executable
  passes all four [consumer runs](../docs/receipts/browser_scry_windows_20261006/README.md):
  two-page Scry, same-profile separate-process restart, direct shared Weld
  adapter and real native permission denial/callback. Scry current input pixels,
  independent clicks/scroll, native cookies, resize, Reader switching,
  reconstruction and zero-producer/cache/importer close are qualified. The
  queue-freshness repair is pinned to immutable Scry `39818a7`; canonical
  Windows profile paths are lowered to ordinary DOS/UNC paths without changing
  the resolved directory. Mere and Genet pins remain unchanged.
  Weld uses Mere's `welding-0-15` adapter and runtime-equivalent `4784d07`
  package identity, with ordered completions and owned-frame custody retained.
  Every paint imports through the producer helper on the current host device.
  Workbench browser controls now follow their actual page while graph selection
  stays independent. Native find, requested zoom (Partial), two-page lowercase
  text/clicks, permission-denial navigation/render and teardown pass.
  The pinned/current shared adapter rejects richer mouse PointerEvent and lacks
  CEF CHAR delivery, so a narrow host bridge forwards to the same producer;
  its [Mere removal gate](../docs/receipts/browser_scry_windows_20261006/mere-weld-input-followup.md)
  includes actual character codes, buttons and modifiers. Six final Weld tests
  pass. The prior broad source snapshot has one parallel network sync-round
  failure that passes in isolation; its suite is not reported as clean.
  Earlier failures and captures remain immutable controls.
- **2026-10-06 next owners:** B3 needs a process-owned Servo event loop,
  immutable upstream source, per-view ordered events, explicit profile support,
  host-device import and frame-origin/input/resize/teardown proof. Graft's
  importer does not provide that browser producer. Broader Mere integration
  remains held at the reviewed boundary. Scry's fresh RADV run passes; native
  Mac capture cadence fails and fresh NVIDIA jobs remain queued. A next trio
  release still requires passing claimed hosts, exact-source package/tag and
  registry-only consumer proof. Physical keyboard, supplementary Unicode, OS
  IME, sandbox bootstrap, lenses and broader browser operations remain scoped
  gates, not inferred from the four passing Windows scenarios.
