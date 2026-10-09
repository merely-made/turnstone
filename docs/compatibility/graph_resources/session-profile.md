# Session placement consumer preparation

`session-profile.patch` is an unapplied, bounded proposal against Turnstone
`9ad64030133c124b4885fe319945afb159b96670`. It uses only APIs committed in Mere
`72c68b6d`, an in-progress supplier checkpoint. It does not qualify that checkpoint,
change pins, authorize integration, or alter ordinary save policy.

## Trigger and behavior

Turnstone `src/session.rs:587` currently reads `load_snapshot`, which discards
an explicit recorded placement, then calls `Graph::from_snapshot` at line 595.
With the checkpoint supplier, that conversion panics on invalid explicit
resource data. A `RecordedStrataV1` input is also rematerialized through legacy
URL-derived containment rather than its declared recorded strata. One-time
image or facet migration then calls the ordinary unprofiled save, losing the
input's placement metadata.

The proposal reads `load_profiled_snapshot`, retains its exact
`Option<PlacementProfile>`, and uses fallible `materialize_snapshot`.
`RecordedStrataV1` retains recorded relations; absent metadata keeps the
supplier's existing unqualified compatibility path. Declared `LegacySurfaceV1`
refuses without qualified replay. Invalid inputs refuse before image deposition.
The existing host result remains `Option<Graph>` with logged refusal, preserving
its current fresh-graph fallback contract.

For valid legacy imagery, the initial materialization validates the untouched
snapshot. Externalization deposits image references as before, then the graph is
rematerialized from those references if images changed. Existing facet overlays
and their precedence remain intact. Migration re-save uses `save_profiled` with
exactly the loaded placement, including `None`; successful load and resource
columns do not establish qualification. Ordinary `save_session_graph` is
unchanged and remains unqualified.

## Committed supplier API references

All references below are to immutable `72c68b6d` source:

- `crates/system/pandect/src/session_graph_store.rs`: `ProfiledGraphSnapshot`,
  `load_profiled_snapshot`, and `save_profiled`. The profiled save uses a
  recoverable file replacement; `load_snapshot` discards placement except for
  refusing a declared legacy input.
- `crates/system/pandect/src/graph_placement.rs:25`: `materialize_snapshot`
  dispatches explicit recorded input to `Graph::try_from_recorded_snapshot`,
  absent metadata to `Graph::try_from_snapshot`, and refuses declared legacy.
- `crates/graph/graph-kernel/src/graph/snapshot/from.rs:51`: `from_snapshot`
  wraps the fallible resource validation with a panic.
- `crates/graph/graph-kernel/src/graph/snapshot/checked.rs:380`: the two fallible
  constructors share explicit resource validation; only recorded construction
  skips legacy URL-derived containment.
- `ports/graphshell/src/mere_host.rs:365`: `snapshot_placement()` returns a
  profile from its qualified `GraphSession`, rather than inferring one from
  graph contents. `begin_profiled_session` accepts explicit caller placement.
- `crates/system/pandect/src/graph_session.rs:988`: `GraphSession::placement()`
  exposes retained session qualification. Turnstone currently owns bare
  `Canvas` graph runtimes and a thread capture hook, not these GraphSession
  instances. That API is a usable model for a subsequent explicit host lane,
  not qualification already present in Turnstone's runtime pool.

## Proposed regression controls

Three unexecuted tests are included in the patch:

1. A recorded graph deliberately lacking URL containment survives both facet
   migration and image externalization, including the saved profile and a
   second reopen. A deliberately unprofiled construction re-derives relations,
   proving that the fixture detects profile loss. The image case also checks
   that its referenced blob was deposited.
2. Invalid explicit resource association data returns `None` without panic,
   rewriting its graph file, or depositing image blobs. A declared legacy
   placement also refuses and preserves its original graph file bytes.
3. An absent profile stays absent after facet migration. Existing legacy
   compatibility is not silently promoted to a qualified recorded session.

`git apply --check docs/compatibility/graph_resources/session-profile.patch`
passed against the current source. The complete proposed source parsed with
`rustfmt --edition 2024 --emit stdout` through standard input. Neither check
applied the patch or compiled tests. Only this patch and this note were written;
production `src/session.rs` remains unchanged.

Normalized UTF-8/LF hashes:

```text
4fc3d020e2842f36a2f13fd9ae23e71ff3379d2a36aee4a75d09a4c2e68ad785 original src/session.rs
e9688e2b725d023d567abb81d16a661b448e82db7a1af635148903bd5dfb9118 proposed src/session.rs
```

## Remaining integration gates

This patch stops at loaded-input preservation. The ordinary save API receives
only a graph, so it cannot distinguish qualified recorded truth from an
unprofiled legacy or mixed input. A runtime/session must carry explicit placement
through boot, session adoption, graph replacement, fork and ordinary saves before
all of those paths can use profiled persistence. New-session placement and legacy
activation require explicit qualification; do not infer them from resource rows.
The subsequent host work must settle that authority boundary before repinning.

The existing `None` result lets session adoption start with a fresh graph after
load refusal. This patch preserves the refused file during the load operation,
but does not prevent a later ordinary save from replacing it. Durable refusal
state and blocked-save or recovery behavior require a separate host contract;
the negative tests here establish immediate preservation only.

Migration save errors retain the existing best-effort logged behavior. This
proposal is not a durable success receipt when a save fails.

Compile and run the `session_profile_` tests at the final qualified immutable
supplier set, then existing session, legacy-node-facet, image externalization,
session-switch and fork tests. Qualify ordinary save/reopen separately once the
host carries placement, including an absent-profile control and a refused-input
save control. Resource content/behavior tests, feed/Keep ownership and graph
runtime wake attribution remain separate gates. Use ThinkPad Linux at low
priority with one build job and the existing target; this preparation ran no
builds and created no target, Cargo home or worktree.

## Continuation design for host persistence (2026-10-08, proposed)

Status: Mark approved this contract; implementation is prepared as the sixth,
unapplied `host-persistence.patch`. The original five patches remain unchanged.
See `host-persistence-checks.json` and `host-persistence-tests.log` for exact
combined-source hashes, static checks and isolated test evidence. Full host
compilation and supplier qualification remain pending.

### Purpose and authority

Preserve the exact declared placement and old disk evidence through boot,
session adoption, graph replacement, fork and ordinary saves. Keep, feed and
unread stay per view. A failed load must remain distinct from a missing file.
Successful resource materialization cannot grant placement qualification.
This is host persistence work; it does not clear Mere's supplier gate.

The proposed owner is `GraphRuntime`, beside its Canvas and session identity.
Store an explicit persistence state: writable with the loaded optional placement,
or refused with the originating session/path and diagnostic. Fresh sessions
begin writable with absent placement until a supplier-backed qualification path
explicitly establishes otherwise. An absent profile remains absent on save.

Replace the `Option<Graph>` adoption seam with a result distinguishing missing,
loaded (graph plus exact placement), and refused. Read canonical facets fallibly
before any migration writes or image deposition. Missing facets are allowed;
malformed or unreadable facets refuse persistence rather than becoming empty.
The loader returns the refusal to the host. A temporary display graph may still
be shown, but that runtime cannot persist into the refused session directory.
Avoid automatic subscription work and recovery writes for that display graph.

### Save, replacement and fork rules

One fallible host save entry point reads persistence state from the exact runtime,
refreshes its canonical facets, saves those facets, and only then saves the graph
with its retained optional placement. A facet failure prevents graph replacement.
A graph failure prevents image garbage collection. This ordering preserves
migration evidence; it is not a transaction spanning every session sidecar.
Feed-sidecar errors must remain observable and cannot be reported as a complete
session-save success. Refused state prevents all writes and image collection
under that session directory, including close, autosave and departing-session
saves. Retrying load can clear refusal only after a successful fresh read.

Boot and adoption install graph and persistence state together. Replacing a graph
without a declared preservation or qualification contract clears any old placement
authority; it cannot leave the donor's profile attached accidentally. Replacement
also cannot clear a refused destination implicitly. Audit direct `Canvas::set_graph`
access rather than relying only on the pool's compatibility dereference.

Fork construction must use an explicit fallible recorded constructor or other
supplier-qualified operation before assigning recorded placement. Carrying a
donor's profile alone is insufficient for a newly assembled component graph.
Persist fork facets and graph successfully before publishing its manifest and
switching sessions. Refuse a fork from a refused display graph. If an unqualified
donor is supported, its fork stays unqualified unless the exact construction
operation supplies qualification. No resource-column heuristic is allowed.

### Reviewed defects and regression requirements

The combined controls proposal overlays
`load_node_facets(data_root).unwrap_or_default()`, then saves migration facets.
The wrapper conflates absent and corrupt sidecars. Reproduction fixture: reserved
raw node labels plus malformed facets; load must leave both files byte-identical.
Canonical false and malformed facet values need separate preservation controls.

Ordinary shell save writes graph before facets; fork also publishes its manifest
before best-effort graph/facet persistence. Inject facet and graph write failures
with deterministic path collisions, not permissions that root can bypass.
Assert original bytes, no premature manifest, no image collection, and successful
retry. Test recorded and absent-profile ordinary save/reopen, refusal followed by
SaveSession/close/switch/fork, and runtime replacement without qualification.

The feed draft removes first-URL-match recovery, but `FeedSubscriptions::merge`
still suppresses unchanged entries with missing explicit bindings. Reconcile can
therefore leave an unchanged entry permanently invisible. Project unchanged
unbound entries into newly minted views, preserving sidecar read status. Bind first,
then derive the control value from that exact binding; the current unconditional
Unread=true assignment must not turn binding repair into a new unread item.
Retain an equal-URL sibling as a negative control and suppress subsequent duplicate
projection once the new explicit binding exists.

### Behavior attribution is a separate deliverable

The immutable Mere checkpoint already has graph-owned `Graph::set_recorder`.
Turnstone installs one thread recorder and projects its shared journal through the
active graph. Prepare a graph-bound recorder carrying runtime identity into a
single ordered host event stream, preserving the complete typed author and the
existing cascade cursor ordering. Stop recording scratch/fork graphs into the
live stream. Install the recorder after each accepted replacement. Equal Surface
and Resource UUIDs in two runtimes must still produce different origins.
Do not infer origin from focus, current URLs, membership lookup or shared IDs.
The behavior wire's scope and authorization vocabulary must be audited before
choosing per-runtime drains or changing the host envelope; this design does not
authorize a new protocol DTO. That routing decision remains open separately.

### Completion boundary

Implement host persistence first with regression controls, then binding repair,
then graph-origin routing after its envelope/authority review. At the final
qualified immutable family, compile and execute all proposed controls and existing
session/feed/behavior suites with the shared Linux target and one low-priority job.
Static application and parser checks remain preparation evidence only.

## Implemented preparation and review (2026-10-08)

Apply `host-persistence.patch` after reader, behaviors, content-tests,
session-profile and controls. It carries explicit optional placement in each
GraphRuntime and distinguishes missing, loaded and refused adoption. Canonical
facet corruption and failed migration writes refuse before recovery. Ordinary
host save persists feed evidence, facets, then the profiled graph; primary failure
stops shell sidecar writes and image collection. Graph replacement revokes old
placement, while a refused destination stays blocked. Fork construction is
explicitly rematerialized at the donor's retained optional placement; child
facets/graph/worlds precede manifest publication, and a failed manifest is removed
from the live store before returning without a switch.

Refused boot starts bin and trail actors without an open store. Refused switching
releases those writers instead of reopening them on the refused directory.
Recovery records, content callbacks, behavior drains, graph writes and startup
fetches are suppressed. The command palette remains available with Retry session,
New session and switching; a same-session retry reloads explicitly. Suspended
Redshank retains output configuration and the exact fetch handle, with no model
store or playback runtime, so a subsequent successful adoption recovers normally.

Review additionally identified interrupted replacement evidence: graph.json may
be missing while graph.json.previous or graph.json.tmp retains the input.
The std-only completeness check refuses missing graph/facet targets with retained
replacement evidence instead of granting fresh-session write authority. No backup
is promoted or discarded automatically. Existing blobs must have the expected
bytes before migration records a reference.

Fresh checks: all six patches apply in sequence; all 25 resulting changed Rust
files parse and proposed whitespace passes. Six actual std-only persistence
helper tests, eight actual feed-module tests and ten actual Redshank-module tests
pass with Rust 1.98.1. Feed/Redshank harnesses use the recorded compiled dependency
set and actual proposed module sources. They do not compile the application or
qualify new supplier semantics. Mutation controls detect reversed write ordering,
missing-target-as-fresh and lost playback configuration; pre-repair feed controls
fail for missing binding, unread loss and GUID collisions. Exact sources,
dependencies, executable hashes and output are retained in the receipt/log.

Prepared full-host controls include recorded/absent save/reopen, unqualified
replacement, corrupt facets, failed migration persistence, refused saves/forks,
recovery palette/retry, primary graph/facet failures, failed fork publication and
interrupted-save backup preservation. These full-host tests are uncompiled and
unrun. Shell close/switch/startup actor controls, image collection after graph
failure, fork storage-failure injection and physical platform behavior remain
required at the qualified immutable family. Graph-origin journal routing remains
its separately open authority/envelope design; this patch adds only refusal
suppression to the prepared behavior drain.
