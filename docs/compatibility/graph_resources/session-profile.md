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
