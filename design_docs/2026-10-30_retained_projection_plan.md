# Retained semantic projection without repeated tree searches

**Status:** landed and locally qualified, 2026-10-30.

The native render host now qualifies pending GPU frames away from the UI
thread. Arrange remains slow with that change. A native stack sample places
substantial work in the host's retained accessibility projection: each
semantic node asks for its painted rectangle, and each rectangle starts
another search from the layout root. Semantic identity matching also scans
the projected node vector for each DOM node. The same pattern appears in
Cambium's geometry and producer semantics, and in repeated accessibility
source comparisons during Turnstone action admission.

This slice removes demonstrated repeated work at its owner. Genet supplies
the reusable geometry traversal; Cambium and Turnstone consume it. The host
continues publishing current semantics independently of GPU acceptance.
Alternate Vello implementations remain separate choices: changing the
rasterizer cannot remove these host-side DOM searches.

## Shared geometry and semantic traversal

Genet Livery owns one flat-tree traversal that carries inherited CSS
translation, element scroll and rendering facts. The DOM view passed by a
consumer defines its root boundary. The traversal reads current retained
fragments, styles and offsets on every call; it does not introduce a cache
whose invalidation would become another host responsibility. Its geometry
preserves the translate-only convention already used by these host queries.
The full affine paint geometry remains a separate existing Genet API.

Genet's neutral semantic traversal carries inherited scroll rather than
asking for every node's ancestor chain. Cambium batches its geometry and
style observations before semantic projection and reuses the existing
semantic ID index when adding producer trees. These changes belong below
the applications so the consumers share both behavior and improvements.

Done when nested scroll, percentage translation, visibility and generated
text controls agree with their established owner queries; subtree scopes
exclude outside ancestors; retained motion agrees with fresh layout; and
deterministic traversal counters establish linear host work. Existing
neutral semantic ordering and action declarations remain intact.

## Turnstone receiving and action admission

Turnstone builds a semantic ID index once and updates bounds from the
shared geometry traversal. The traversal uses the snapshot owner's DOM
scope, which may be wider than the semantic subtree being projected.
Viewport scroll and physical scale remain host adjustments. Controlled
Cambium text fields continue publishing the runner's committed field
value, editability and multiline state. Their draft DOM text does not
replace the controlled value.

Action-map construction uses an ID set rather than scanning the projected
tree once per node. Document lowering and old-subtree retirement use temporary
ID indices; host route membership uses temporary ID sets. Pelt document
placements share one immutable admission snapshot among their controls.
Accessibility admission compares each shared contributed or Pelt document
source once for each old/new allocation pair within one publication. The
first comparison still performs exact structural equality. Allocation
identity only permits reuse of that already computed result in the same
publication; it is not a semantic revision or equality assertion.

Done when actual application-path regressions demonstrate bounded geometry
visits and semantic probes, unchanged contributed sources preserve queued
action generations, changed sources still invalidate them, and live
validation rejects stale actions after earlier actions in the same drain.
No cache survives publication, and semantic publication does not wait for
a successfully presented pixel frame.

## Layout preparation without discarded paint

Frame rectangle and accessibility reads prepare the current Frisket layout.
They currently also call `OuterFrame::scene`, emitting and discarding a complete
painted scene on every preparation pass. Inline mounting legitimately needs
multiple layout passes; those passes do not each need a painted scene. The
existing retained `ensure` operation already provides layout without paint.

Done when actual `OuterFrame::prepare` calls emit no paint lists while keeping
cold, resized, scaled and changed-selection geometry current; explicit scene
production still emits its paint; and inline mounts, scrolling, hit testing,
accessibility publication and native primary/lens controls retain their current
behavior. This introduces no scene cache or new invalidation authority.

## Consumer and product evidence

Done when the source owners and receiving application pass focused
correctness gates, the existing native render/quality controls still pass,
optional-service receiving compiles, and owned Rust files meet Turnstone's
source-size policy. The published consumer locks must select one coherent
Genet source cohort and the current Mere owner change, with unrelated lock
identities preserved or explicitly reconciled against upstream.

The retained optimized baseline supplies the before side of another local
macOS/Metal measurement. Matching fixture runs must exclude compiler
contention, retain captures and exact source inputs, and distinguish
forced redraw cadence from natural idle behavior. These observations do
not establish Linux performance, physical IME behavior or human
accessibility qualification. Run receipts live separately under
`Code/output/genet-geometry-projection-20261030`.

## Findings

- 2026-10-30: `src/ui/retained_layout_new.rs` performs per-node linear
  semantic matching and delegates every bound to `painted_rect`.
  `src/ui/accumulated_translation.rs` starts a root search for each query.
- 2026-10-30: Cambium Rootstock repeats ancestor geometry/style queries;
  its producer projection already owns an ID index that the graft helper
  can reuse. Genet owns the shared traversal and neutral scroll behavior.
- 2026-10-30: contributed source equality is repeated for each action
  route. Revision zero is deliberately not an immutable semantic version;
  using it to suppress publication would weaken correctness.
- 2026-10-30: Pelt placement admission copies an owned document projection
  per action. Document lowering and structural retirement repeatedly search
  node vectors, and host route publication scans those vectors per route.
  These paths can reuse publication-local indices without persistent caches
  or changes to the serialized neutral projection contract.
- 2026-10-30: Genet already owns neutral-to-AccessKit field conversion.
  Turnstone can validate reachable IDs, reuse that lowerer, and retain only
  its explicit multiline, visible-focus and support-description policies.
- 2026-10-30: Cambium publication already inventories the scoped flat tree.
  Reusing membership from that inventory avoids another ancestor walk per
  node; later action capture still checks current ancestry and contributor
  liveness.
- 2026-10-30: the first native comparison improves Arrange modestly. A fresh
  profile locates its larger remaining cost in `OuterFrame::prepare` painting
  through rectangle and publication queries. `RetainedLayout::ensure` already
  supplies fresh geometry separately from `scene_resources`.
- 2026-10-30: the naming audit identifies additional repeated label, scope and
  description work, but naming is a small branch in this Arrange sample.
  Unexecuted probe tests and source findings are retained as diagnostic cues;
  this pass follows the measured preparation-to-paint path.
- 2026-10-30: the final native sample contains no painting below frame
  preparation. Remaining paint enters through explicit render paths; used-color
  style materialization is still a shared producer cost. The observations are
  diagnostic cues, with no naming-policy or persistent-cache change in this
  implementation.

## Progress

- 2026-10-30: isolated owner changes and actual-path traversal/comparison
  regressions prepared; source and product qualification in progress.
- 2026-10-30: shared Genet geometry and neutral scroll projection landed;
  complete owner library gates pass. Cambium receiving, producer geometry
  and publication membership are implemented and pass their owner gate.
- 2026-10-30: Turnstone geometry indexing, shared admission snapshots,
  publication-local comparisons and shared field lowering implemented.
  Published-cohort receiving and local performance qualification continue.
- 2026-10-30: combined consumer library and optional-service checks pass.
  The first matched native comparison and follow-up sample are retained;
  layout-only frame preparation is now the measured continuation.
- 2026-10-30: layout-only preparation landed with actual paint-count regression
  controls and a passing full consumer library gate. Matched optimized native
  comparisons improve all observed conditions; primary resize and lens
  input/close controls pass. Sources, failure receipts, profiles and captures
  are retained separately under the evidence path above.
