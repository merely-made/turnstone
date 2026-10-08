# Resource content consumer controls

This is a prepared integration gate, not a passing supplier receipt. Turnstone's
declared Mere pin remains unchanged. The unapplied `content-tests.patch` adds two
focused app tests when the exact coordinated supplier set is ready.

The API inspection used Mere worktree `mere-graph-semantics`, based on
`6399fe6cb4ff4773d189330188619bbd803dc4f6` with uncommitted resource-content work.
That base commit alone does not supply the inspected API. The patch checked
cleanly against Turnstone `e77e1e2db88344c651c1d4bf31e1802a2b142fc0` and its current
consumer edits. Neither test has been compiled or executed.

## What the patch checks

- Two distinct Surface IDs use case-normalized, fragment-different page URLs
  and share one recorded Resource ID. The fixture uses public live
  `apply::add_node`, rather than crate-private `Graph::add_node` or
  `refresh_surface_resource`.
- An explicit `Author::person` writes two resource tags, `Paper` and `paper`,
  through `tag_resource_with_concept`. Both raw Surface tag columns stay empty.
  Inspector must show both exact labels when following either member.
- Committed navigation moves only the addressed member to a different Resource.
  Its Inspector tags become empty; the sibling's original resource tags remain.
  Navigating the member back restores its resource-aware read without copying
  labels into the raw Surface column.
- Deletion captures both exact labels in `RemovedRecord.tags`. The sibling,
  Resource, and sibling's shown-resource association survive both the live delete
  and graph snapshot reload. Its Inspector still shows the resource labels.

`RemovedRecord.tags` is `Vec<String>`. The capture test establishes labels and
member identity only. It does not claim to preserve concept owners, tagger
attribution, assertion IDs, or an attributed recovery operation. Recovery needs
a separately specified record shape or resource rebinding policy before such a
receipt is meaningful.

Apply and run these tests only against a qualified immutable supplier revision.
Use `git apply --check docs/compatibility/graph_resources/content-tests.patch`
first, then qualify the `resource_content_` test filter. A successful apply check
is patch-shape evidence; the current pin cannot compile the new supplier APIs.

## References and persistence

Mere owns resource identity and the persisted Surface-to-Resource associations.
The inspected `surface_ids_showing_resource` reader returns live Surface IDs.
Deleting a Surface removes that Surface's shown-resource association. It retains
the shared Resource and a surviving sibling's association. Persistence therefore
retains the surviving reference; it does not create a historical reference to
the deleted member. A richer retained-reference model requires a new explicit
contract. This does not resolve the pending Keep scope decision.

## Feed and application-control gate

Turnstone owns feed subscriptions, entry state, read status, and retention
commands. Feed state is keyed by member ID in `src/feed.rs`. The following
production actions currently write labels through `Canvas::tag_node` and
`untag_node`:

- `subscribe_focused_feed`: `keep` and `feed`.
- `unsubscribe_focused_feed`: removes `feed` and entry `unread` labels.
- `mark_focused_feed_entry_read`: removes `unread` on the selected member.
- feed entry projection: writes `feed-entry` and `unread`.
- `reconcile_feed_tags`: rewrites source and entry labels from per-member state.
- node deletion: removes `unread` labels for the removed source's entries.

These labels currently serve Surface/session controls, rather than descriptive
page-content tags: `keep` marks retention of a member, `feed` marks that member's
subscription, and `feed-entry`/`unread` reflect the per-member feed sidecar.
Their writers and readers remain unchanged in this preparation. Concrete owner
paths are `src/app/node_arms.rs:125` (Keep read and write),
`src/app/feed_arms.rs:44` (subscription labels), `:70` (unsubscribe),
`:98` (read status), `:236` (entry labels), and `:279` (reconciliation).
Keep/feed policy is outside the supplier's bounded content-reader adoption.

With the inspected supplier, these writers target the shown shared Resource.
Two members may retain distinct subscriptions or unread state while exposing
the same resource label set. Unsubscribing or marking one member read can affect
labels visible through its sibling. Reconciliation can also alternate writes
for two members sharing a Resource but having different entry state. The supplier
retraction selects the current author's assertions; equal labels belonging to
another author can remain visible after that retraction.

The existing feed tests also assert raw `node_tags` values. Updating those reads
alone would conceal the policy mismatch. Before integration, Turnstone must
explicitly settle which controls belong to a member and which to shared content,
then test shared-resource subscription, unsubscribe, read/unread, reconciliation,
and deletion. Mere owns content tag identity and attribution, not Turnstone's
subscription or retention policy. Keep remains a separate pending user ruling.

Roster now gathers resource-aware tags, but its public rendered row shapes omit
tag labels. This patch therefore makes no Roster display-tag claim; expose a
real tag-bearing row or gather output before adding that display assertion.
