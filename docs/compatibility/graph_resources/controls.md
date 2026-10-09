# Per-view controls draft

Mark selected: "Keep these controls per view; share only descriptive tags."
`controls.patch` is a saved, UNAPPLIED draft for review and handoff. No production
source or dependency pins changed. Supplier checkpoint `72c68b6d` is unqualified.
The user's request to stop this thread arrived before combined apply validation.
Patch SHA-256: `0aed7862f888f2aa4740db3de58310d67a0dfdd75c52a0a0402f97cf609d297e`.

## Application order and ownership

Apply `session-profile.patch` before `controls.patch`. The controls loader hunks
are based on the proposed profiled load source, rather than current production.
Other prepared patches are `reader.patch`, `behaviors.patch`, and
`content-tests.patch`. Joint sequential apply checks passed in the final handoff and were repeated
on 2026-10-08; compilation and semantic checks remain pending.
Controls puts its new app tests in `src/app/surface_control_tests.rs`, avoiding
`content-tests.patch`'s insertion into `src/app/tests.rs`. It changes node_arms
Keep/deletion-unread/recovery hunks; inspect combined hunk conflicts before use.

## Prepared implementation

Four boolean Surface facets under `turnstone.controls.v1` own `keep`, `feed`,
`feed-entry`, and `unread`. Reads and writes require the exact live member UUID.
Canvas::facets_mut and Chartulary FacetStore::set/get are available at both
current declared pin `3ded2cd7` and supplier checkpoint `72c68b6d`. No supplier
API edit is proposed. Resource tags, including a descriptive label spelled
`keep`, never authorize a view control. Malformed existing facet values are
retained, refused by the writer, and logged. Missing values read false.

Keep remains idempotent: repeating Keep does not save or emit NodeKept again.
It changes one Surface facet and requests the existing full-session save. It
introduces no recursive fetch, retention closure, GC policy, neighbor content
copy, or new durable-reference model. Subscription sets Keep and Feed on only
its source Surface; unsubscribe clears that Surface's Feed but preserves Keep.
FeedEntry is historical membership and remains after unsubscribe. Unread is a
current projection from the member-keyed feed sidecar.

Feed projection mints its own entry Surface when no explicit live binding
exists. Equal URL/resource identity does not adopt an unrelated existing view.
Restore no longer reconstructs missing bindings through first URL match.
Sidecar projection aggregates unread only across bindings to exactly the same
Surface, preserving old explicit shared-member bindings rather than guessing.
Reconciliation runs after canonical facets load and visits all live runtimes.
A source that navigated or disappeared clears its cached controls via ordinary
reconciliation; fetch ownership, cadence and transport behavior are unchanged.

## Migration and evidence

Before supplier graph materialization, extract only reserved labels from each
snapshot's raw `nodes[].tags` column, addressed by that node's UUID. Preserve
those exact labels (including their order) in a Surface `legacy-labels` facet,
and remove those labels from the in-memory snapshot before descriptive content
migration. Never read shared Resource tags to reconstruct per-Surface choices.
Canonical sidecar controls, including explicit false, override legacy defaults.
Feed subscriptions and unread status then reconcile from the existing sidecar.

The load migration saves the facet store first. If that persistence fails, it
logs the failure and does NOT re-save the stripped graph snapshot; old disk
labels remain available for retry. If graph save fails after facets succeed,
evidence survives and the original snapshot still supplies retry input. The
existing session-profile patch's placement-preserving re-save remains in use.
Original immutable on-disk receipt bytes are not edited by this preparation.

Recycle records with typed control facets restore those facets. Reserved strings
in an unqualified `RemovedRecord.tags` vector are kept in a separate
`unqualified-recycle-labels` evidence facet and excluded from content-tag writes.
They do not alone recreate Keep or activate a subscription. This avoids treating
resource-derived recycle labels as per-Surface evidence. Recovery attribution
and richer retained references remain separately qualified work.

## Actual checks and remaining gates

The draft-save check parsed the new `src/surface_controls.rs` and eight modified
existing files with explicit UTF-8. The final root handoff check then applied all
five patches in order to an isolated Git index and parsed all 13 resulting Rust
files, including the three appended app tests. `handoff-checks.json` binds this
check, patch hashes and proposed source hashes. Proposed whitespace passes and
production source/pins are unchanged. No Cargo build, compilation or test
execution occurred. The marker-identified temporary index was removed. No new
worktree, target directory or Cargo home was created.

Prepared tests cover two Surface UUIDs sharing one Resource: local idempotent
Keep and facet reload; a shared descriptive `keep` label that grants neither
view Keep; separate duplicate feed subscriptions/entry views, mark-read,
unsubscribe, reconciliation and sidecar reload. A helper migration test checks
raw label capture, descriptive label preservation, idempotence, sibling
nonpromotion and canonical false precedence. These four new tests are unrun.

The existing subscription projection test's raw tag assertions were updated to
Surface control reads in the patch. Existing Keep test remains unchanged and
should exercise the new helper after integration. Existing recycle/session/feed
suites were neither changed broadly nor executed. Migration failure injection,
recycle recovery proof, and duplicate-resource full-session restore are pending.

Before integration: repeat the combined patch check if source or patches change;
compile and run against a qualified immutable supplier with unchanged protocol
DTOs; check normal SaveSession graph/facet ordering under write failure as well
as loader migration ordering; verify a missing entry binding on an unchanged
feed eventually gets a new explicit placement without adopting a sibling.
Inspect tombstone facet restoration and exact migration evidence preservation.
The new helpers use existing non-journaled host facet access, so behavior-wake
expectations also need explicit qualification rather than being inferred from
tag captures. The draft does not establish release readiness.

## Continuation review (2026-10-08)

The unchanged draft has two additional explicit defects: canonical facet load
errors become empty stores before migration writes, and unchanged feed entries
with missing bindings remain suppressed by merge. The host ordinary save and
fork paths also retain the ordering/refusal gaps described in
`session-profile.md`, section "Continuation design for host persistence".
These are source-review findings, not executed regression failures. The proposed
repair design and required positive/negative controls are saved there. All five
patches remain unapplied and byte-identical to the handoff.

## Host successor patch (2026-10-08)

`host-persistence.patch`, applied sixth, repairs the reviewed corruption and
missing-binding defects without changing this draft's bytes. Feed reconciliation
retains stored read status when a binding disappears. Unchanged unbound entries
mint new views instead of adopting an equal-URL sibling, and the host binds using
the exact projected GUID/entry identity before deriving its unread facet.
Compatibility URL binding refuses ambiguity; GUID promotion applies only to old
unguided URL-keyed entries, preserving distinct GUID entries on the same URL.

Eight isolated actual-source feed tests pass, including both read states through
sidecar save/reopen and repair, duplicate GUID placements and ambiguous-URL
refusal. The additional app-level repaired-view control is still uncompiled.
See `session-profile.md` and `host-persistence-checks.json` for persistence,
refusal, review corrections and evidence limits. Production source and pins
remain unchanged; full current/future-family consumer suites remain pending.
