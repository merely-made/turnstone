# Resource graph consumer preparation

Turnstone retains its declared Mere revision
`3ded2cd7c2370713118a2440c962c39262720205`. The original preparation inspected
uncommitted work based on `6399fe6c`. The 2026-10-08 follow-up reviews immutable
supplier checkpoint `72c68b6d`; its independent gates passed, but its exact RDF
import repair is still being implemented and is not qualified. Mark chose the
additive import envelope in the Mere chat. No direct profile-RDF parse/apply
pipeline in Turnstone, Knot or Woodshed requires a contribution DTO change.
`checkpoint-review.json` records this review and committed supplier file hashes.
This preparation does not merge the supplier or change the family pins.

The Inspector, both Roster gathers, and recycle-label capture now use the
central `content_tags` reader. At the current pin it reads legacy node tags.
When Mere exposes its inherent `Graph::node_content_tags` method, Rust method
resolution selects that reader instead of the temporary local fallback. Remove
the fallback as part of qualification against the coordinated immutable supplier
set. Current-pin tests cannot establish shared-resource behavior.

`content-tests.patch` and `behaviors.patch` are prepared, unapplied integration
patches. They depend on the new resource APIs and captured-delta variants.
`reader.patch` removes the legacy fallback after the supplier is qualified.
The existing behavior match remains exhaustive for the current pin. Apply checks
establish patch shape only. See `content-controls.md` for content, feed,
attribution, and reference boundaries, and the behavior notes for trigger gates.

## Persistence and Keep

Current production behavior saves the complete session graph through
`session::save_session_graph` and Pandect's graph snapshot. Keep writes a tag on
one selected member and requests a session save. Saving is not filtered to kept
members and does not recursively fetch or keep linked pages.

The inspected Mere resource model persists explicit Surface-to-Resource
associations. A resource can therefore identify every live surface showing it
in that graph. Navigating a surface changes its shown resource; deleting a
surface removes its association. A surviving resource is not an archive of
deleted surface references or prior arrangements.

Mark ruled on 2026-10-08: **Keep, feed subscriptions and unread status remain
specific to each view; only descriptive tags share resource identity.** Retained
graph references do not recursively keep linked pages. This sets control scope,
not a new retention, eviction or garbage-collection algorithm. Production Keep
still uses the old selected-member tag at this pin; it must adopt explicit
per-view controls before the resource repin. See `controls.md` for the prepared
adoption and its migration limits.

Recycle records preserve string labels, not tag concept ownership or assertion
attribution. The prepared label capture is not an attributed recovery receipt.

## Integration conditions

1. Mere supplies a qualified immutable resource/content revision and its owner
   clears integration. Repin the family as a tested set.
2. Apply and compile the prepared reader, content, behavior and control patches;
   retain exhaustive delta admission. Adopt the safe profiled session load and
   migration-save patch; carry qualified placement through ordinary runtime saves
   and prevent a later fresh-graph save from overwriting a refused input before
   claiming full session integration. See `session-profile.md`.
3. Run the shared-resource Inspector/navigation and recycle-label controls,
   behavior fanout/retraction/ancestry controls, and existing consumer suites.
4. Qualify the ruled per-view Keep/feed controls, attributed recovery, and
   behavior routing across multiple graph runtimes. Do not infer per-view control
   state or placement qualification from shared content tags or resource columns.

Linux validation uses the ThinkPad with one build job at low priority and the
existing reusable `/home/markik/Code/target`. The older receipt collisions were
archived and the primary checkout refreshed on 2026-10-08. The existing
`/home/markik/Code/worktrees/turnstone-resource-compat` worktree remains owned by
this consumer lane pending publication/integration; reuse it for the final gate.
Windows accessibility receipts remain separate. No new target, Cargo home or
worktree was created for the checkpoint review.

At Mark's handoff request, all five patches were applied in sequence to an
isolated Git index, with all 13 proposed Rust files parsing and clean whitespace.
`handoff-checks.json` binds patch bytes, ordered application and proposed-source
hashes. The generated index was removed. These are static checks only: no new
test was compiled or executed, and production source and pins stayed unchanged.
Resume from the [browser/resource handoff](../../../design_docs/2026-10-08_browser_resource_handoff.md).

At source commit `78a59e172c78c70e943332ade5492b91ff09fea7`, the locked Linux test
build passed. The same test binary then passed all seven Inspector read-model
tests, the strengthened recycle-label round trip, and the unchanged Keep control:
**9 passed, 0 failed**. `linux-current-tests.json` binds the exact eight compiled
source/manifest inputs, binary SHA-256, clean source guard, and test counts;
`linux-current-tests.log` preserves those three test executions. The new
Inspector test also passed during the initial Cargo build invocation.

This is a current-pin, default-feature Linux receipt. The resource patches are
still unapplied and unexecuted. It is not a Windows, macOS, browser accessibility,
shared-resource, attributed recovery, or full-suite receipt. The initial Cargo
scan reported Servo's malformed tidy fixture manifest and continued successfully;
no supplier fixture or manifest was modified to obtain this result.


The approved host contract now has a sixth unapplied successor patch,
`host-persistence.patch`, covering retained runtime placement, refusal protection,
canonical-first graph saving, fork publication ordering and exact feed-binding
repair. Apply it after the original five, whose bytes are preserved.
`host-persistence-checks.json` records six ordered applications, 25 parsed Rust
files and 24 passing isolated module tests; `host-persistence-tests.log` records
positive and negative controls. This does not compile the host or qualify the
supplier. Full integration, shell refusal/close/switch controls and graph-runtime
journal attribution remain open. Details and source-review corrections are in
`session-profile.md` and `controls.md`.
