# Resource graph consumer preparation

Turnstone retains its declared Mere revision
`3ded2cd7c2370713118a2440c962c39262720205`. The supplier work inspected here is
uncommitted work on Mere's `mere-graph-semantics` worktree, based on `6399fe6c`.
This document does not authorize a supplier merge or an independent repin.

The Inspector, both Roster gathers, and recycle-label capture now use the
central `content_tags` reader. At the current pin it reads legacy node tags.
When Mere exposes its inherent `Graph::node_content_tags` method, Rust method
resolution selects that reader instead of the temporary local fallback. Remove
the fallback as part of qualification against the coordinated immutable supplier
set. Current-pin tests cannot establish shared-resource behavior.

`content-tests.patch` and `behaviors.patch` are prepared, unapplied integration
patches. They depend on the new resource APIs and captured-delta variants. The
existing behavior match remains exhaustive for the current pin. Apply checks
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

Mark asked whether keeping one node would also preserve its related nodes and
their neighbors. A proposed bounded policy would keep the selected placement
and resource, retain references, and stop before recursively keeping neighboring
content. That is a proposal, not a ruling or an implemented retention gate.
Keep's member-versus-resource scope is pending; production Keep remains unchanged
in this preparation. Feed membership and unread state also need explicit scope
qualification before their tags can share content identity.

Recycle records preserve string labels, not tag concept ownership or assertion
attribution. The prepared label capture is not an attributed recovery receipt.

## Integration conditions

1. Mere supplies a qualified immutable resource/content revision and its owner
   clears integration. Repin the family as a tested set.
2. Apply and compile both prepared patches; retain exhaustive delta admission.
3. Run the shared-resource Inspector/navigation and recycle-label controls,
   behavior fanout/retraction/ancestry controls, and existing consumer suites.
4. Resolve Keep, feed control ownership, attributed recovery, and behavior
   routing across multiple graph runtimes. Do not infer those policies from
   content tag labels.

Linux validation uses the ThinkPad with one build job at low priority and the
existing reusable `/home/markik/Code/target`. Its primary Turnstone checkout has
older untracked receipts that collide with upstream tracked files, so a temporary
`/home/markik/Code/worktrees/turnstone-resource-compat` checkout isolates this
gate. Windows accessibility receipts remain separate.

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
