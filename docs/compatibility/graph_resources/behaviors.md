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
