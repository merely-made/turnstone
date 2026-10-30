# Retained semantic projection without repeated tree searches

**Status (2026-10-30):** landed and locally qualified, including shared-frame
paint deferral, Genet used-color receiving, current upstream integration and
semantic projection reuse.

The native render host now qualifies pending GPU frames away from the UI
thread. Arrange remained slow after that change. An earlier native stack sample placed
substantial work in the host's retained accessibility projection: each
semantic node asked for its painted rectangle, and each rectangle started
another search from the layout root. Semantic identity matching also scanned
the projected node vector for each DOM node. The same pattern appeared in
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
The admission-comparison cache is confined to one publication. Semantic
publication does not wait for a successfully presented pixel frame.

## Layout preparation without discarded paint

Frame rectangle and accessibility reads prepare the current Frisket layout.
They formerly also called `OuterFrame::scene`, emitting and discarding a complete
painted scene on every preparation pass. Inline mounting legitimately needs
multiple layout passes; those passes now use the existing retained `ensure`
operation to provide current layout without paint.

Done when actual `OuterFrame::prepare` calls emit no paint lists while keeping
cold, resized, scaled and changed-selection geometry current; explicit scene
production still emits its paint; and inline mounts, scrolling, hit testing,
accessibility publication and native primary/lens controls retain their current
behavior. This introduces no scene cache or new invalidation authority.

## Frame painting after producer staging

Primary and lens rendering formerly built standalone frame scenes in their
mutable scene pass. When the plan contained a shared native frame, the later
raster pass discarded these frame and floating-frame scenes before rasterization.
Native composition then installs current producer commands and emits the actual
frame scene with its shadow-mask requests. The earlier scene is not a fallback
on composition failure and cannot substitute for that current producer-bound
result.

Frame painting now defers to the existing native composition owner for shared plans.
Plans without a shared frame retain their standalone scene path. Keep the source
surface plan, geometry preparation, producer staging, mask custody, native frame
qualification and capture ordering unchanged.

Done when actual primary/lens plan regressions measure no discarded standalone
frame paints, standalone plans still paint once, and native composition continues
to emit current producer content with its masks and unchanged capture/refusal
behavior. This introduces no scene cache or second rendering authority.

## Shared used-color projection

Genet now owns a lazy numeric color view of each unchanged retained style plane.
The contextual source still owns inheritance, CSSOM and animation. Its generated
computed-value walk touches only color-bearing fields; mutable style access,
length resolution, removal, cascade and generated-counter updates retire the
view. Unchanged clones share it, and no-op liveness checks preserve it. Palette
and scheme changes already replace the source through device invalidation.

The owner contract and freshness controls live in
`genet/docs/2026-07-28_livery_contextual_color_computation_plan.md`. Turnstone
receives that owner revision through one coherent locked Genet cohort.

Done when repeated retained UI paint reuses the numeric view, source expressions
and next-frame changes remain current, the default product and optional services
compile against the selected owner, and matched native runs retain memory and
cadence evidence alongside current composed primary/lens captures.

## Semantic projection reuse

After paint deferral, the Arrange sample spent most of the main thread
publishing accessibility: each publish rebuilt the outer frame's whole
semantic projection, and the shell publishes twice per frame (actor drain
and redraw). `RetainedLayout` now keeps the last projection for each root.
The key holds the DOM mutation revision, focus, scale, viewport scroll
and element scroll. Snapshot replacement clears every entry. Any recorded
DOM change, including labels, referenced elements, form state and generated
content, therefore projects afresh. Inline bodies project through their owner's
layout under their own roots, so the frame and its bodies do not evict
each other. Callers still receive an owned projection.

Every surface plan prepares the outer frame, about ten times per frame, and
each preparation synced the frame controls, rescanning the frame DOM in
`enhance` and twice for focus. A sync that changes nothing now records a
stamp of the DOM revision, control admission and revision, host focus and
runner focus. The next sync with the same stamp returns at once. Any write,
invalidation, focus move or workspace change runs the full pass again.

Done when an unchanged layout reuses its projection without geometry or
semantic work, each key input and a restyle produce output equal to a fresh
projection, separate roots keep separate entries, a settled control sync skips
while outside writes, runner focus, workspace and invalidation each resume it,
and matched native Arrange runs measure the change.

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
The later shared-frame paint and used-color continuation retains its source,
failure and product evidence under `Code/output/genet-used-colors-20261030`;
earlier receipts remain at their original paths.

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
  style materialization was still a shared producer cost in that earlier sample. The observations are
  diagnostic cues, with no naming-policy or persistent paint-cache change in that
  geometry slice; the later used-color slice has its own owner invalidation
  contract below.
- 2026-10-30: `shell_render.rs` and `shell_render_lens.rs` eagerly paint frame
  and floating-frame scenes which their shared-frame raster filters discard.
  `native_composition::compose` later installs current producer commands before
  painting the scene that is actually rasterized; masks belong to that result.

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
- 2026-10-30: a common lazy scene-construction seam and actual primary/lens
  paint-count controls are prepared. Eager production behavior is retained for
  the pre-fix test run and matched baseline freeze.
- 2026-10-30: the shared-plan regression fails at the actual discarded-paint
  count while the standalone scene control passes. Eager behavior remains in
  place while the matched baseline release is frozen.
- 2026-10-30: shared primary and lens plans now skip standalone frame scene
  construction. Producer-bound scene emission and masks remain with native
  composition; standalone plans retain their existing paint path. Combined
  consumer and native qualification follows the retained baseline freeze.

- 2026-10-30: the full default application library passes with the shared-frame
  deferral and received Genet used-color view. Both actual plan paint controls
  are green. Optional-service and native receiving qualification continue.

- 2026-10-30: shared-frame deferral and Genet used-color receiving pass the
  application library and optional-service check. Actual document resize, lens
  input/close, floating Arrange tear-out/return and redocking controls pass.
  Representative composed captures agree with the established layouts.
- 2026-10-30: repeated matched native runs improve all observed cases. The
  Arrange case shows the largest gain and lower sampled process memory. A fresh
  native sample contains no discarded outer-frame scene and little used-color
  materialization. Shader refusal, cancellation, exact-pixel publication and
  controlled device-loss gates retain their assertions. Measurements and
  source cohorts remain in the continuation evidence directory.
- 2026-10-30: integrated current upstream search and note-editor fixes after the
  frozen matched runs. The measured render files and selected Genet lock remain
  byte-identical; the full application library passes after integration.
- 2026-10-30: semantic projection reuse and settled control sync landed with
  reuse and freshness controls; the application library passes (1239). Matched
  optimized Arrange runs against the frozen shared-frame build improve from
  14.4 to 18.6 fps, with p95 presentation from 81 to 61 ms. The follow-up sample
  shows no fresh projection or control rescans in steady state. Remaining
  main-thread cues: publication still runs twice per frame and is 57% of the
  sample; `body_slots` in frame preparation, the outer frame's projection clone,
  lowering and action-map walk, and `frame_with_guests` lead it. Evidence:
  `Code/output/turnstone-a11y-reuse-20261030`.
