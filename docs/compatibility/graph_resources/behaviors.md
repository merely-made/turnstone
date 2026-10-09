# Behavior compatibility for shared graph resources

`behaviors.patch` is an unapplied proposal for the Mere graph-semantics supplier
set. It changes only `src/behaviors.rs` when applied. Current production source,
dependency pins, and existing release receipts stay unchanged.

The proposal was prepared against the `6399fe6c` supplier worktree's uncommitted
P2 routing/content changes on 2026-10-08. That checkpoint alone does not contain
the reviewed APIs. Recheck this patch against the eventual immutable supplier
commit before integrating it.

The 2026-10-08 follow-up checked this patch against current Turnstone and the
committed `72c68b6d` APIs. The three new captured variants and required resource
and projection readers remain present. The patch still applies; this is not a
compilation or behavior qualification receipt. The supplier's exact-import repair
is underway, and the journal-runtime gate below remains open.

## Wake rules

| Captured change | Surface scopes projected at the after-dispatch drain |
| --- | --- |
| Existing Surface variants | Their directly named Surface identities |
| `ReplaySetShownResourceById` | Its explicitly named Surface, including after detach or removal |
| `ReplaySetResourceRecordById` | Every current Surface showing that Resource; also current Surfaces showing sources of incoming exact `taggedWith` assertions |
| `ReplaySetResourceEdgesByIds` | Every current Surface showing either explicitly named endpoint, including empty replacement payloads after retraction |
| Fields, couplings, session import records | No graph behavior wake, retaining the existing classification |

The exact tag predicate is `https://mere.computer/ns/rel#taggedWith`. An incoming
assertion with another predicate does not make its source a content dependency.
The relation, rather than the current concept facet, determines this dependency:
removing a concept facet still changes projected labels on resources tagging it.
There is no recursive propagation through arbitrary semantic relations.

Resource UUIDs never become bare Surface scopes. Projected Surface identities
are sorted and deduplicated before their ancestry is walked. Both endpoints of
an edge replacement are used even when its new edge list is empty, so the adapter
does not depend on discovering the removed relation in the final graph.

Ancestry must also adapt: supplier `containment_edges()` reads only Surface
relations. Production URL, domain, and filesystem containment now belongs to
Resource relations. The patch reads `projected_outgoing_relations()` and retains
payloads with populated Containment data. This joins those Resource relations
with Surface-owned user folders and collections. Every current view of a
Resource container supplies a distinct Surface ancestry, preserving the existing
UUID-based scope vocabulary, cycle guard, and depth bound.

The rest of the pipeline retains journal sequence order and author identity,
containment member-to-container direction, authority checks, cascade budgets,
and the no-self-wake subject comparison. Resource changes use that same pipeline;
they do not introduce a second watcher or authority vocabulary.

## Qualification and limits

`git apply --check docs/compatibility/graph_resources/behaviors.patch` passed.
The proposed full source parsed successfully with
`rustfmt --edition 2024 --emit stdout` through standard input. Neither check
applied the patch or compiled it. The six proposed regression tests have not run.

They cover multiple current views of one Resource, detached Resource negative
controls, incoming tag-concept dependencies after facet removal with an unrelated
predicate control, empty edge retractions and self-pair deduplication, detached
and removed Surface scopes, journal attribution and no-self-wake through
`entries_since`, and Resource versus Surface containment ancestry.

Integration requires compiling against the final supplier commit and running
these tests plus the existing behavior/cascade/denizen suites. Add an end-to-end
authorized behavior test in which a shared Resource edit wakes each appropriate
watch, supplies the expected trigger digest, and leaves unrelated watches asleep.
Run navigation, removal/restore, same-resource aliases, and multiple graph-runtime
controls before calling this consumer integration qualified.

The adapter resolves unseen entries against the currently active graph after
dispatch. It does not reconstruct each entry's earlier Resource binding or
ancestry. A Surface detached or navigated earlier in that batch is absent from
the old Resource's current views; explicit Surface binding/navigation/removal
deltas still name it, with current ancestry or a bare removed-Surface scope.
Removed container membership has the same existing limit. Historical exact
scopes would require captured prior associations or a qualified per-entry graph
projection, rather than guessing from Resource UUIDs.

The application journal also lacks a graph-runtime identifier. Two runtimes can
hold the same Resource identity, but this proposal cannot establish which runtime
originated an entry. Cross-graph wake attribution remains an integration gate.

## Proposed graph-origin contract (2026-10-08)

Status: Mark approved this contract on 2026-10-09 and directed scoped, continuous
execution. The seventh unapplied patch prepares it; production pins stay unchanged.

### Purpose and choice

A committed edit must retain its originating graph even when another pane or
session becomes active before the behavior drain. Equal Resource or Surface UUIDs
in different graphs must never transfer a trigger to another graph's participant.
Turnstone currently retains multiple graph runtimes, but its participant roster,
grants, run store and four watch tables belong to the adopted session. Recording
more graphs does not grant that roster authority over them.

Three approaches were considered:

| Approach | Consequence |
| --- | --- |
| Graph-bound capture with session-bound delivery, recommended | Fixes origin and rejects foreign or replaced runtime entries without changing signed scopes or participant protocols. Other open graphs are recorded but do not run background participants. |
| Move the complete participant runtime into every graph runtime | Supports concurrent background behavior, but also moves admission, run persistence, schedules and external effects. This is a larger independent change. |
| Prefix watch scopes with graph IDs | Changes the scope vocabulary used by grants and saved watches, requiring authority migration; it also does not itself make participant execution graph-specific. |

### Capture and runtime lifetime

Use one ordered, process-local host stream. Each entry owns an explicit sequence,
origin and Mere `AttributedDelta`. Origin comprises `GraphId`, optional `SessionId`
and a host-issued runtime generation. A generation identifies one accepted live
graph instance; reloading the same graph/session gets a different generation.
Neither graph UUIDs alone nor node membership qualify a queued entry after reload.

Bind `Graph::set_recorder` to that origin at installation. The closure appends the
complete current typed author and captured delta together under one stream lock.
Replace Turnstone's thread capture hook; retaining both would duplicate live edits
and capture scratch graphs. Detached construction, migration, replay and fork
assembly remain quiet. Clones already drop their Mere recorder. Constructors,
accepted graph replacement, sample-canvas replacement and fixture replacement
must all install a fresh binding before subsequent live actions. Refused adoption
has no writable recorder. Counter exhaustion refuses capture-dependent execution
with a diagnostic, rather than wrapping or silently dropping an edit.

Existing resident and endpoint author guards must retain all author fields and
restore the prior author on nested dispatch and unwinding. Source-graph origin
comes exclusively from the graph's recorder, never from the author guard, focus,
current URL, active cursor or a search for matching member IDs.

### Delivery and authority

Bind the adopted session's participant/watch context explicitly to its accepted
runtime origin. Before projecting any graph trigger, require that exact origin,
its still-live runtime and successful session adoption. Resolve Resource views
and containment through that runtime's graph. Foreign, detached and stale-generation
entries remain distinguishable in the host stream but supply no graph trigger to
this session. They are not promised a later background wake.

Participant execution still uses the incumbent active-canvas lane. Therefore
require that lane to name the bound graph before every automatic run; if it does
not, refuse the run rather than redirect it by matching UUID. Keep the existing
unique-subject mapping, live read checks, scoped read checks and world-write
admission. Graph origin is a host routing boundary, not an additional capability.
Root-scope watches are subject to the same origin check as exact-node watches.

Pin the session/runtime binding for each cascade. Revalidate before and after
each body invocation. A session switch, reload or runtime replacement stops the
old cascade; its temporarily removed watch table must not overwrite the newly
loaded table. Apply that protection to graph and app cascades and clock batches,
because all three invoke the same session-owned participant lane. Refusal is
observable without disclosing foreign delta contents in a trigger digest.

Servitor `WatchEvent`, `CommittedEntry`, signed `Cap` scopes and the participant
`TriggerContext` wire remain unchanged. Existing author-ID projection preserves
the no-self-wake comparison; origin filtering happens before their construction.
The trusted journal inspector filters by the bound origin before resolving author
IDs through the loaded roster, avoiding foreign scripts acquiring local labels.

### Sequence and restart boundary

Sequence is an explicit monotonically increasing host ordinal, not an entry's
vector index or stream length. On adoption, raise the allocator above both its
current high-water mark and the largest restored graph-watch cursor before
binding live capture. Existing saved graph watches need no wire migration and
the first new edit cannot be hidden behind a cursor from the previous process.
Exhausted restored cursors refuse graph behavior with a diagnostic.

The session's drain starts at its adoption boundary, excluding previous generations
and setup history. Advance its scan cursor through the complete considered host
tail, including foreign entries, while projecting only its bound origin. Snapshot
the tail before running bodies and use explicit ordinals for subsequent rounds,
resident write detection and trigger ranges. A tail containing only foreign edits
must neither wake a local root watch nor be rescanned indefinitely.

The incumbent journal is not persisted or restored by Turnstone. This contract
does not promise crash recovery of undrained edits. App-event ordinals remain a
separate sequence space; repairing their existing restart behavior is outside
this graph-stream change.

### Done conditions

Prepare a seventh unapplied patch after `host-persistence.patch`, with recorded
application, parser and focused test evidence. Prove two live graphs containing
equal Surface and Resource IDs with different views and containment: each capture
has one origin, projection uses that origin's graph, and a foreign edit cannot
wake the adopted session's exact or root watch. Change focus between capture and
drain. Reload the same IDs and prove old-generation entries are excluded.

Also prove scratch/fork silence, exactly-once live capture after every replacement,
typed nested-author restoration, no-self-wake, a foreign-only tail, restored
nonzero graph-watch cursors, cursor exhaustion, revoked read authority and a
session change during a cascade without overwriting new watch state. At the final
qualified immutable family, compile the full host and run the existing behavior,
participant admission/run, endpoint, inspector and session suites alongside these
controls. Isolated helper tests cannot qualify that wiring. Production application
and repinning remain gated by the owner-cleared Mere family adoption order.

## Graph-origin preparation receipt (2026-10-09)

Apply `journal-origin.patch` seventh, after `host-persistence.patch`. It replaces
the thread recorder with graph-owned capture carrying graph, session and lifetime
generation into an ordered host stream. The loaded participant/watch binding is
independent of focus. Resource projection and inspector labels use that binding;
automatic execution requires its graph in the incumbent active-canvas lane.
Graph and app cascades protect replacement watch tables, and clock batches stop
when their binding changes. Typed author guards and existing capability scopes
retain their meanings. Sample-canvas replacement also revokes prior placement.

The stream records explicit ordinals and raises its floor above restored graph
watch cursors. Foreign-only tails advance the scan boundary without providing a
trigger. A refused saved cursor permits a fresh binding retry. True capture or
allocation failures disable capture-dependent execution. Source review found and
fixed an exhaustion boundary after the final possible captured entry; its focused
regression failed before that fix and passed afterward. A drafted host test's
GraphDelta UUID field was also corrected against the immutable supplier API.

All seven patches apply in an isolated Git index, the previous six bytes are
preserved, and all 32 combined changed Rust files parse. The actual proposed
`host_journal.rs` passes 15 isolated tests with cached Mere and Servitor libraries,
including real Graph capture and clone silence. Five explicit bad-source controls
fail for broken origin filtering, sequence floors, generation distinction, table
restoration and nested authors. The initial missing-API compile failure and the
pre-fix exhaustion regression are distinguished from those mutation controls in
`journal-origin-tests.log`; exact source, dependency and artifact hashes are in
`journal-origin-checks.json`.

An attempted actual runtime-module build stops because cached Pandect does not
contain `graph_placement::PlacementProfile`. No full-host compilation occurred.
Eight resource-aware host controls are drafted, including a foreground body with
a foreign-tail negative control, but remain uncompiled. The direct table-restore
helper test does not prove interruption by a real session-changing body. That
graph/app/clock integration control, existing revoked-read and participant suites,
endpoint/inspector/session controls and final family qualification remain pending.
The process-local host stream still supplies no durable undrained-edit recovery.

Normalized UTF-8/LF consumer `src/behaviors.rs` SHA-256 used for patch preparation:
`6201fc7e09c2c882eeb432c8eec9b6d8811697010e74aede7e47772dc863b673`.

Reviewed supplier file SHA-256 values, relative to the supplier checkout:

```text
ae2d6e6204c3fad091fd5b6fc49f3501b215876ddd7fad6f3c64c137ebc4fcd5 crates/graph/graph-kernel/src/graph/capture.rs
9e83c8a69641c0e8ec342d765efb54c01e0622b1e3ee1c1739b564a5fc5d0189 crates/graph/graph-kernel/src/graph/resource.rs
a5ebba9c13bc27808c6a8e35b9bd81a21b8e48288203967e7d6ed4291993a81b crates/graph/graph-kernel/src/graph/resource_tags.rs
3ec7df7f8213752085c799a0e42be80750e690179648465f4103127cc598f537 crates/graph/graph-kernel/src/graph/resource_content.rs
7dec7416b19baace4038e833ea53836a0154139544a6e2699664f87889d38afe crates/graph/graph-kernel/src/graph/relation_read.rs
27b01f647fd70f4dd23e8e80a476d169f2178d11ee408a2b1545ce3ff36e4d55 crates/graph/graph-kernel/src/graph/query.rs
c51c942b9b89a9a2f68ec5caa2f54b93cb8fd25177dbaa8537d6efdc82678aec crates/graph/graph-kernel/src/graph/apply.rs
9d95c2fbc3263294d9ae46e52e3472032361cac70d315ecf78b372039fb1fa50 crates/graph/graph-kernel/src/graph/edge_payload.rs
```
