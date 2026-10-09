# Turnstone browser and resource integration handoff

**Status, 2026-10-09:** local native integration completed with Mark's
authorization to continue. The qualified fixed family is Mere `3b3afa289`,
Genet `15713014`, Knot `802238cb` and Redshank `e08bf4b`. All seven reconciled
patches and native repairs are applied. Default libraries: 749 passed, nine
existing ignored. Piccolo libraries: 782 passed, nine existing ignored.
Both locked all-target checks and the fresh source review pass. Knot, Redshank
and consumer integration commits remain local. The final continuation and
[source-bound receipt](../docs/compatibility/graph_resources/native-integration-checks.json)
record qualification and publication scope.

## Start here

Read [the active index](DOC_README.md), [resource integration](../docs/compatibility/graph_resources/README.md)
and this handoff. Keep browser release qualification separate from the completed native
Mere resource-graph integration. The earlier S0 tested-set browser repin is closed;
it must not be redone merely because older plan prose mentions a pending repin.

The original handoff stopped further dependency changes, builds and publication.
Mark subsequently resumed this **turnstone chat** on the ThinkPad and authorized
the update. No further permission gate is needed for the recorded consumer work.
**Forms** is the Genet/Vano chat, not this lane. The handoff and patch checkpoint
are being transferred through Git objects to the existing consumer worktree;
no new worktree is needed. No message was sent to Forms.

## Decisions and operating constraints

- Accessibility on Windows, macOS and Linux leads the browser release bar.
  Foreign page trees need host AccessKit subtree joins and routed actions;
  pixels or source support do not establish physical assistive-technology proof.
- Servo uses an explicitly named, configurable process profile shared by its
  views. Do not silently replace it with per-node Servo profiles.
- Mark's 2026-10-08 resource ruling: **Keep, feed subscriptions and unread status
  remain per view; only descriptive tags share resource identity.** Retained
  graph references do not recursively keep linked pages. This is not a new
  eviction or garbage-collection policy. Current whole-session persistence is
  not filtered by Keep.
- Cambium's maintained home is Mere; Hocket's is Woodshed. The old standalone
  ThinkPad checkouts are legacy evidence, not failed authentication repairs.
- Use the ThinkPad for Linux work and keep the Windows Asus light. M4, Intel
  and ThinkPad were unlocked by Mark, but recheck session state for headed runs.
  `s-pc-2` was offered for setup; it has not been configured in this lane.
- Preserve shared WIP. Work on main; reuse existing worktrees only when an
  actual collision warrants them. At most three simultaneous tasks per agent.
  Linux builds use low priority and the existing target, normally one job for
  this resource gate. Windows builds use BelowNormal, bounded jobs and approved
  stable targets. Do not create Cargo homes or fresh target variants by default.

## Published browser family versus newer primary checkouts

The [browser family receipt](../docs/receipts/browser_family_20261007/README.md)
records published runtime adoption `abb349cf`, test-only follow-up `8d1f907f`,
681 Windows combined-browser tests and 650 Linux default-feature tests passing,
locked all-targets checks and exact source identities. Native and release gates
retain their separate scope; do not relabel an old executable after a repin.
The active browser plans and latest native receipts are linked from the index.
Check current default Servo resize/reopen pixels, foreign-tree admission/actions
and physical three-platform accessibility before claiming browser release readiness.

For the trio's own release/hardware bookkeeping, read
`wgpu-graft/design_docs/2026-09-03_wgpu_triplet_release_plan.md` and each trio
repository's `design_docs/DOC_README.md`. Their latest primary commits record
Intel visibility/foreground failures; an unlocked session is not by itself a
passing animation/cadence receipt. Do not turn the successful ThinkPad archive
refresh into release proof or redo an already published release blindly.

At the start of this handoff, Turnstone's manifest selects:

| Owner | Selected revision |
| --- | --- |
| Mere | `3ded2cd7c2370713118a2440c962c39262720205` |
| Genet | `965b64e206a47d1c8808472de9aa461233638768` |
| Knot | `6e66f1abddd382cc50543e6db7e2b02537018fd1` |
| Woodshed/Redshank | `3ef710406546241432b50df8fc27a4b93d915454` |
| Servo 0.7 | `aac43a3f31a259f04a574f5ec4e959c943ad7cc7` |
| Graft | `bb48281bd4a88a726a009e0518706881b74b7604` |
| Weld / CEF154 supplier | `c4dd593b7a730acb4aa6ffab0fe2edf8a0584083` |
| Scry | `2c3ebd24118851cf6125cfb193f55aab20e3ad67` |

Those consumer pins are not the latest primary checkout HEADs. Live checkout
currency, supplier publication, imported pixels, native headed controls and
consumer qualification are different evidence.

## Work completed in this chat

Reader preparation `78a59e1` centralizes Inspector/Roster/recycle label reads,
with temporary legacy fallback at the existing Mere pin. `eeb4a50` records
unapplied resource tests and behavior scopes. Nine focused ThinkPad tests passed
against the existing pin, with source and executable identity recorded in
`docs/compatibility/graph_resources/linux-current-tests.json`. They do not prove
shared-resource semantics.

ThinkPad checkout refresh receipts were committed locally as `b99ddab` and
`b323617`. Seventy-three colliding old receipts were preserved and rehashed,
never discarded. See [the refresh receipt](../docs/receipts/thinkpad_checkout_sync_20261008/README.md).

The dirty trio contained stale versions of already landed work, not a unique
feature needing a port. After Mark authorized archival, exact dirty file bytes,
separate index/worktree patches and original metadata were saved; Weld's three
loose sources and 931-file CEF147 SDK were moved intact. All hashes/metadata
were rechecked after fast-forward. The SDK remains on the ThinkPad, not in Git.
All three primary tracked trees are clean and match their fetched upstreams:
Graft `0b026a44`, Scry `dc9bd1a0`, Weld `c4dd593b`. The receipt is local commit
`9ad6403`; see [trio preservation](../docs/receipts/thinkpad_trio_archive_20261008/README.md).
This maintenance did not rebuild or qualify the trio for release.

## Next resource supplier and prepared adoption

Mere's original owner is the chat **Explain Mere’s graph database model**,
thread `01a10806-9320-79b2-85fc-66b96dfe868f`. Its isolated Windows worktree is
`C:/Users/mark_/Code/worktrees/mere-graph-semantics`, branch `graph-semantics`.
Do not mutate or build that active lane, or integrate its dirty source.

Published checkpoint `72c68b6d6f6a5c9023999d9fdaf4b3db632e6874` has passing
independent kernel/store, Pandect, Eidetic/Fjall, Canvas, Cartography,
document-lanes, Graphshell, locked workspace and wasm gates. Four exact RDF
round-trip failures remained. Mark chose an additive import envelope and matching
parse/apply APIs in that chat. The repair was active at handoff, including exact
opaque/empty/UUID-shaped assertion IDs and paired identical-value assertions.
It is not a final supplier qualification. Read its latest owner receipt before
selecting a pin; old passing counts do not qualify the repair.

The owner's subsequent handoff reports pushed `c49e3e0b` as **unqualified RDF
envelope WIP**, with no Cargo run on that new source. Known Surface exact-replay
parallel-record loss and paired-ID controls remain. Its Windows worktree is clean
and original agents stopped. Continuation moves to **Discuss graph semantics**,
thread `01a11e41-8ff4-7992-9dac-d3fac4af35b9` on the ThinkPad. The compact supplier
handoff is in `mere/design_docs/mere_docs/implementation_strategy/2026-10-04_graph_semantics_plan.md`,
section "Continuation handoff (2026-10-08)" (reported near line 2100). The independent
qualified base remains `72c68b6d`; `c49e3e0b` does not clear repin/integration gates.

Turnstone, Knot and Woodshed have no direct profile-RDF parse/apply call site
requiring contribution DTO changes. The envelope adoption belongs in Mere's
linked-data/document-lanes. Turnstone's generic `linked-data.ingest` Session
dispatch is a separate host route gap, not a reason to alter the existing wire DTOs.

All new adoption work lives as **unapplied patches** in
`docs/compatibility/graph_resources/`. Production source and dependencies were
not changed by this preparation:

- `reader.patch`: remove the legacy tag fallback after qualified adoption.
- `behaviors.patch`: resolve resource changes to current showing surfaces,
  exact incoming tag dependencies, endpoint retractions and projected containment.
- `content-tests.patch`: shared-resource Inspector/navigation and recycle labels.
- `session-profile.patch`: fallible profiled load; preserve declared placement
  during image/facet migration re-save. It does not qualify ordinary saves.
- `controls.patch`: draft per-view Keep/feed/unread adoption and migration.
  Consult `controls.md` for its final saved state and validation limits.

`checkpoint-review.json` binds the committed supplier APIs; working supplier
files were not used for its hashes. The final `handoff-checks.json` records all
five patches applying sequentially in an isolated index and all 13 resulting
Rust files parsing successfully, including the three newly drafted app tests.
Proposed-source whitespace passes; production source and pins remain unchanged.
No proposed test was compiled or executed. Controls remains a draft with the
semantic and persistence gates listed in `controls.md`; syntax is not qualification.

## Remaining done-conditions

1. Obtain the owner's final qualified immutable Mere revision and integration
   clearance. Update the family as a tested set, Knot then Woodshed/Redshank then
   Turnstone, retaining one Mere source and its matching Genet. No independent
   Turnstone repin to `72c68b6d` was made.
2. Verify the saved controls draft and ordered application with the other
   patches. Per-view feed entries must not attach unread state to the first
   arbitrary URL match. Persist migrated control evidence before stripping old
   disk labels; resource tags cannot prove a particular view's old control state.
3. Carry explicit qualified placement through Turnstone's bare Canvas runtime,
   session adoption, replacement, fork and ordinary save. Graphshell already
   exposes qualified placement, but Turnstone has no equivalent retained value.
   Do not mint it merely from resource columns. A refused load must not later
   be overwritten by an automatic fresh-graph save. The load patch establishes
   only immediate file preservation.
4. Attribute behavior journal entries to their originating graph runtime.
   The shared journal currently has no such identity and projects entries
   through the active graph. Equal resource IDs in two graphs are insufficient.
5. At the qualified family, compile and run proposed tests plus existing
   behavior/session/feed/recycle controls, restart/save/fork refusal controls,
   shared-resource sibling controls and appropriate consumer suites. Preserve
   meaningful negative/positive controls and exact source/manifest/binary hashes.
   Browser and physical accessibility gates remain separate.

## Machines, Git and retained files

Windows primary Turnstone is `C:/Users/mark_/Code/repos/turnstone`, `main`.
Before the handoff commit it was at `9ad6403`. The preparation/handoff commit is
its successor. These latest administrative/reader-preparation commits have not
been pushed in this continuation. Inspect `git log origin/main..HEAD` before
publication; the two forms-publication merge commits do not justify absorbing
other concurrent files. `.github/` is unrelated untracked WIP and was preserved.

ThinkPad SSH alias is `thinkpad-l14-f`, user `markik`. Primary repositories are
under `/home/markik/Code/repos`, forks under `/home/markik/Code/crates`.
At final transfer the bare hostname stopped resolving; the same SSH identity
worked with `-o HostName=thinkpad-l14-f.local`. This is a per-command fallback,
not a shared SSH configuration change.
Primary Turnstone remained `1802a68371327343539b3e73d4517248cf3361b1` at handoff,
with untracked refresh/trio receipts. Preserve colliding local receipts before
a future fast-forward. Trio archives live inside each primary repository at
`docs/receipts/thinkpad_trio_archive_20261008/`.

Retained worktree `/home/markik/Code/worktrees/turnstone-resource-compat` was clean
at detached `eeb4a501aa63fe4c41a7a5abef29db77fcb87418` before this handoff transfer.
It will fast-forward to the saved handoff checkpoint, retaining the same production
source and pin. Verify its live HEAD before using it. Owner: this consumer lane,
available to turnstone chat. Reason: current-pin preparation is not yet published/integrated
into remote main, and the pending resource gate may reuse it. Remove only after safe
publication/integration and checking active owners. The existing auxiliary
`wgpu-graft-radv` lane belongs to earlier GPU work and was left untouched.

Reuse `/home/markik/Code/target` for Linux and approved Windows targets under
`C:/t/cargo-targets/<repo>`. No new target directory, Cargo home or worktree was
created. Temporary marker-identified Git indexes used for patch checks were
removed after their owning process ended. No Linux build was running in this
lane at the last preflight; recheck before launching the next gate.

## ThinkPad continuation review (2026-10-08)

The existing consumer worktree was verified clean at `53de12f` before review.
All five unchanged patches again apply in sequence and their proposed Rust files
parse; `docs/compatibility/graph_resources/continuation-review.json` records exact
bytes. No production edit, repin or Cargo run was performed. The controls review
found corrupt-sidecar overwrite and permanently unbound unchanged-feed paths;
ordinary save/fork retain ordering and refused-input hazards. The concrete host
persistence proposal is in `docs/compatibility/graph_resources/session-profile.md`,
section "Continuation design for host persistence". It needs design review before
runtime implementation. Mere's qualified family gate remains open.

## Host persistence implementation plan (2026-10-08)

Status: implementation preparation checkpointed; full host compilation and
qualification remain pending. Mark approved the written host persistence contract at
`64d0221`. Execute inline in the existing consumer worktree. Preserve production
source and pins; save an ordered `host-persistence.patch` after the original five.
Spec: `docs/compatibility/graph_resources/session-profile.md`, continuation design.
Rust 2024, Mere's existing immutable APIs; Linux uses the existing shared target.

Review focus: corrupt canonical facets; refused input followed by autosave;
facet/graph write failure; graph replacement retaining stale qualification;
unchanged missing feed binding resurrecting unread or adopting a sibling.

### Task 1: persistence state and ordered writer

- [x] Add standalone std-only regression tests in proposed
  `src/session_persistence.rs` for destination mismatch, refusal, canonical-first
  write ordering and facet/graph failures, then run them with `rustc --test`.
- [x] Implement `SessionPersistence<P>` with Unbound, Writable(directory,
  optional placement) and Refused(directory, reason); `save_with` checks identity,
  saves facets, then invokes graph writer with retained placement. Test recorded
  and absent placement, refusal persistence across unqualified replacement,
  and successful retry. These tests cover the actual proposed helper, not a model.

### Task 2: host adoption and persistence wiring

- [x] Carry `SessionPersistence<PlacementProfile>` in GraphRuntime. Load fallibly
  with canonical facets before migration. Distinguish missing and refused inputs;
  install graph and state together before recovery or feed work.
- [x] Centralize ordinary graph/facet/feed saving through the runtime state. Stop
  sidecar saving and image collection on refusal or primary persistence failure.
- [x] Guard fork, save child before publishing manifest, preserve absent placement
  unless explicit recorded construction qualifies the newly assembled snapshot.
- [x] Add future-family corruption, save/reopen, fork/refusal and replacement tests.
  Check every changed call site and static patch application. Compilation remains
  deferred until a qualified immutable family is available.

### Task 3: feed binding repair and checkpoint

- [x] Add unchanged-unbound entry controls. Repair projection without sibling
  adoption; derive unread from exact sidecar binding and retain read status.
- [x] Run feasible isolated feed tests against existing compiled dependencies;
  future-family app controls remain distinct from that narrower evidence.
- [x] Apply all six patches in an isolated index, parse proposed Rust, check hashes
  and whitespace, self-review the combined source and save receipts. Commit only
  preparation artifacts. Graph-origin routing remains its separately open design.

Execution ruling: approval to implement the reviewed contract already supplies
scope; do not repeat a planning approval question. Future-supplier patches cannot
claim compiled qualification. Standalone helper tests prove only their stated
persistence contract; downstream consumer gates remain pending.

### Preparation completion receipt

The original five patches are preserved; `host-persistence.patch` applies sixth.
It implements the approved persistence contract and binding repair as proposed
source, including review fixes for reachable recovery commands, preserved
Redshank configuration, unread/GUID identity and interrupted replacement evidence.
All six patches apply in sequence and 25 changed Rust files parse. The actual
standalone helper/feed/Redshank modules pass 6/8/10 tests respectively (24 total).
Full-host controls are drafted but uncompiled; no production edit, dependency
change or supplier integration occurred. See
`docs/compatibility/graph_resources/host-persistence-checks.json` and its test log.

Execution ruling: supplier qualification prevents claiming integrated completion.
Isolated actual-source tests provide a real local gate for supplier-independent
helpers; app/runtime/shell wiring still requires the final immutable family.
The source review was required by the execution skill and performed read-only.
No chat was messaged and no new worktree, Cargo home or target was created.
Graph-origin behavior attribution was explicitly left outside this approved
persistence contract; its authority/envelope choice remains open.

## Graph-origin design review (2026-10-08)

The next written contract is in
[`behaviors.md`](../docs/compatibility/graph_resources/behaviors.md#proposed-graph-origin-contract-2026-10-08).
It recommends graph-bound capture into one ordered host stream, with explicit
session/runtime-generation binding for delivery. The existing participant roster,
authority, run store and watch tables are session-owned; recording other live
graphs cannot authorize background execution under the adopted roster.

The source audit covered runtime replacement, session adoption, graph/app/clock
drains, participant lowering, endpoint author guards, saved watch cursors and the
journal inspector. Mere `72c68b6d` supplies `Graph::set_recorder`, clone recorder
isolation, complete typed `AttributedDelta` and unchanged Servitor watch scopes.
The host stream is not currently persisted, so restored graph-watch cursors must
not be compared with a new zero-based vector index. The proposed contract uses
explicit ordinals above the restored cursor floor and excludes old generations.

Mark approved the written design on 2026-10-09, then explicitly directed scoped
continuous execution without further process reviews. That instruction overrides
the skills' additional plan approval stages. The supplier owner's compact status
still supplies no final qualified immutable family or integration clearance.

## Graph-origin implementation plan (2026-10-09)

Execute inline in the existing consumer worktree. Spec: `behaviors.md`, proposed
graph-origin contract. Produce `journal-origin.patch` after the existing six;
preserve their bytes, production source and dependency pins. Keep Rust 2024 and
the shared Linux target; do not build the supplier's active lane.

Ruling: use this existing handoff as plan and ledger, and retain the scratch
source/test receipt until the immutable-family host gate can run. Mark's scope
instruction takes precedence over more elaborate skill bookkeeping. One fresh
source reviewer remains useful for the runtime/authority boundary.

### Task 1: ordered host stream

- [x] Create proposed `src/host_journal.rs`, register in `src/lib.rs`. Define
  `RuntimeOrigin { graph, session, generation }`, `HostEntry { seq, origin, edit }`
  and `HostJournal` retaining typed author guards and attributed edits.
- [x] Test origin distinction for equal IDs, explicit ordinal ordering after a
  restored cursor floor, foreign-tail filtering, generation replacement, and
  counter exhaustion. Compile actual proposed module with cached Mere/incipit
  dependencies using a small `rustc --test` harness; preserve red/green evidence.
- [x] Interfaces: `allocate_origin(GraphId, Option<SessionId>) -> Result<RuntimeOrigin, JournalError>`;
  `raise_cursor_floor(u64) -> Result<(), JournalError>`;
  `record_as(RuntimeOrigin, Author, CapturedDelta) -> Result<u64, JournalError>`;
  `high_water() -> u64`; `entries() -> &[HostEntry]`; `author()` / `set_author()`;
  shared stream alias and a graph-owned recorder constructor. Latch capture
  failure for host diagnostics; never wrap counters.

### Task 2: bind live runtimes and session authority

- [x] In proposed `src/app/runtime_pool.rs`, bind all installed live graphs to
  the shared stream, with a fresh generation on graph/canvas replacement.
  Detached adoption setup and refused graphs remain quiet. Keep a session-watch
  origin in `App`, independent of the compatibility active-graph cursor.
- [x] Update `src/app/{mod,fixtures,session_lifecycle}.rs`, bootstrap and sample
  replacement. Restore graph watches and raise cursor floor before capture;
  install watch origin only after successful setup, clear it before adoption.
- [x] Draft full-host replacement, scratch/fork silence, stale-generation and
  two-runtime controls in `src/app/journal_origin_tests.rs`. Their execution
  requires the qualified family; parser success cannot substitute for it.
- [x] Interfaces: runtime `origin() -> Option<RuntimeOrigin>` and explicit bind,
  suspend and canvas replacement seams; `App::behavior_origin()` resolves the
  loaded authority binding, requiring session/runtime identity and stream health.

### Task 3: guarded delivery and existing consumers

- [x] Update `src/behaviors.rs` to filter host entries by bound origin before
  Resource projection; use explicit ordinals and acknowledge foreign-only tails.
  Require the bound graph in the incumbent execution lane before any automatic
  invocation. Keep watch scopes, trigger wire and read/write admission unchanged.
- [x] Pin origin across graph/app cascades and clock batches. Stop on session or
  generation change; never restore an old taken table over newly loaded watches.
- [x] Update `src/app/{denizen_arms,palette}.rs`, `src/remote_projection.rs` and
  `src/inspector_view.rs` for the stream type, high-water cursors and origin-filtered
  label lookup. Retain nested author restoration. Audit every journal use.
- [x] Add graph/root-watch foreign controls, focus-change and origin projection,
  no-self-wake, revoked read, nested attribution, restored cursor, and interrupted
  cascade controls. Run supplier-independent actual-source tests; mark full-host
  controls uncompiled until the final family gate.

### Task 4: verify, review and checkpoint

- [x] Generate seventh patch from the six-patch source baseline. Apply all seven
  in an isolated Git index, compare source hashes, parse every changed Rust file,
  and check whitespace and preserved patch bytes.
- [x] Run fresh-context read-only source review, resolve important findings with
  focused regression controls, save `journal-origin-checks.json` and test log.
- [x] Update existing behavior/handoff/index status and commit only preparation
  artifacts. Record pending full-host behavior, participant, endpoint, inspector
  and session qualification against the owner-cleared immutable family.


### Graph-origin preparation completion receipt (2026-10-09)

The approved contract is prepared in `journal-origin.patch`, seventh after
host-persistence. All seven patches apply in an isolated index and 32 combined
changed Rust files parse. The previous six bytes and production source/pins are
unchanged. Actual host-stream source passes 15 isolated tests against existing
compiled Mere/Servitor libraries; five bad-source controls and the reviewed
execution-exhaustion regression demonstrate failure sensitivity. See
`docs/compatibility/graph_resources/journal-origin-checks.json` and its test log.

Fresh read-only source review found a drafted test UUID type mismatch and a
last-ordinal execution-exhaustion gap. Both were fixed; the reviewer verified the
latter correction. Full-host compilation is pending: the attempted standalone
runtime-module build stopped at cached Pandect's absent PlacementProfile module.
Eight new host controls are drafted, but a real session-changing body in all
three wake tiers and final family suites remain explicit integration gates.
Direct restore-table tests qualify only the helper, not that complete cascade.
No production edit, dependency repin, supplier build or chat message occurred.
The supplier owner is still working; no final immutable integration clearance
was available in the compact status checked during this continuation.


## Current upstream family continuation, 2026-10-09

Mark authorized updating after discussing Mere's qualification. Read-only remote
refresh found newer published browser work. Consumer merge `1a992214` incorporates
upstream `22361c9`, preserving the prepared graph work and the newer controller
input, command catalogue, physics and pending profile identity behavior.

The production family is now Mere `e48db81404eb580b7a7b3c01caeb410e539da4c8`,
Genet `15713014e2e23b887360471552f75f60684f5384`, Knot
`16aaff7804d49ba829af646c3a8eca2d6ec2fc2f` and Redshank/Woodshed
`ece6e9eaf7c11a37f4b1457895704b1e6aeb6b8c`. Cargo.lock contains one Mere Git
source. This updates the browser baseline; it does not adopt the resource-graph
supplier's active merge worktree. At that checkpoint its owner, **Discuss graph
semantics**, had green native tests and was completing workspace and wasm checks.

Upstream's controller-input receipt records Knot 661 passing tests, Redshank
242 passing tests with exclusions, and Turnstone all-target compilation and
verification. Its macOS library run had 683 passes and eight failures: four
platform failures and four place tests that passed separately. This consumer
continuation does not turn that receipt into a clean Linux full-suite claim.

The original seven `.patch` files remain byte-for-byte unchanged. The first five
still apply on this baseline. The sixth needed context reconciliation with the
new command catalogue and session changes. The seventh also required keeping
`identity: Some(identity)` in the fixture while retaining its new graph-bound
journal. Use this current order:

1. `reader.patch`
2. `behaviors.patch`
3. `content-tests.patch`
4. `session-profile.patch`
5. `controls.patch`
6. `host-persistence-current-family.patch`
7. `journal-origin-current-family.patch`

[Current checks](../docs/compatibility/graph_resources/current-family-checks.json)
record clean ordered application in an isolated index, exact resulting hashes
and 32 Rust parser checks. The actual journal module is unchanged from the
previous compiled receipt; its existing binary was rerun with 15 passes.
`cargo metadata --offline --locked --no-deps` passes for the merged production
manifest. None of the resource patches is applied to production, and the merged
host has not been compiled or tested here. Remaining full-host gates in the
previous continuation still apply after the final immutable graph supplier and
coordinated family update.


## Qualified graph adoption in progress, 2026-10-09

Mark said continue. Mere's owner has qualified and normally main-published P2:
source `526f2ddb5f7d9e46a5f6670b2f9c7d48078152e9`, documentation receipt
`3b3afa2895424f4be4e1e5f23d48140bf24da5aa`. Native supplier/consumer suites,
locked workspace, kernel/Pandect wasm and standalone browser wasm checks passed.
The source includes the reviewed physics and identity main tail. Adopt the fixed
main-published documentation revision; its matching Genet stays `15713014`.
The owner is merging P3 and proceeding toward P4 in its own lane; this consumer
does not read mutable supplier files or repin to those ongoing changes.

The seven patches from current-family-checks.json are now applied and staged in
this existing consumer worktree. Turnstone's Mere rows are updated to `3b3afa289`.
Knot's primary main was fast-forwarded to `16aaff78` and its Mere rows updated in
root, desktop and standalone knot-document manifests. Woodshed's primary main
was fast-forwarded to `ece6e9ea`; only the consumed Redshank workspace/web Mere
rows were updated. Untracked ThinkPad receipt files were preserved. Both sibling
lock graphs resolve one Mere Git source. Tests run at low priority, one Cargo
job, with the existing shared /home/markik/Code/target; no new Cargo home, target
or worktree was created. Current process state is recorded in
/tmp/turnstone-resource-adoption-state.json and logs are under the shared target
with the turnstone-resource- prefix. Full host compilation and final receipts
remain pending at this execution checkpoint.

Source inspection confirms session switches are effects consumed by the shell
after App::update and its synchronous behavior drain. A native interpreted body
cannot itself call session adoption mid-drain. The same-thread graph/session
origin guards still protect replacement and nested delivery; full-host tests
must preserve the distinction between those guards and the deferred shell
switch boundary.


### Native integration corrections, 2026-10-09

All-target compilation passed after repairing two stale test APIs: the page type
import and the current MenuSession command lane. The initial default Linux run
reported 742 passes, six failures and nine existing ignored controls. This is a
retained failing receipt, not a clean final gate.

The failures exposed fixture assumptions and a real fork-copy gap. Persistence
roots are now bound at test construction; positive forks use a rooted manifest
store and the manifest-failure control uses an isolated directory. Redshank's
note check reads descriptive tags through the resource-aware reader. Profile
image controls now inject historical JSON fields explicitly because the Rust
serializer omits those deserialize-only shadows. All five profile controls pass.
The live Graphshell receipt changed only its graph revision counter from 5 to 11;
its cards, layout and authority outcomes are unchanged.

The supplier's older copy_component_from walks raw Surface edges. New browse
links are Resource relations, so a real two-view browse fork copied only one
view; a separate new regression showed that a one-view fork lost its resource
tag. Turnstone now uses session::copy_session_component: walk both strata,
mint fresh Surface IDs with CopiedFrom provenance, retain the connected Resource
closure including unshown intermediates and tag concepts, copy records and held
assertions verbatim, remap Surface edges and shown bindings, validate a quiet
recorded snapshot, then carry host facets/placement and publish in the approved
order. All six focused fork controls pass, including exact original resource
records and held assertions. This does not fetch pages or change Keep policy.

Ruling: preserve the existing fork behavior through the new graph model rather
than weakening the connected-component assertion or changing the qualified
supplier. The implementation remains in the consumer worktree. Final full
default and interpreted-behavior gates, source binding and the single fresh
whole-branch review are in progress. Compiler jobs remain one at low priority;
final host test scheduling is bounded to two threads. Supplier lanes are untouched.


## Native integration completed, 2026-10-09

Default libraries: 749 passed, 9 existing ignored. Piccolo libraries: 782 passed, 9 existing ignored. Both suites ran against the unchanged final source and locked family,
with Rust 1.98.1, one low-priority compiler job, the existing shared target and
two bounded test threads. Default and Piccolo all-target checks pass. Offline
locked metadata reports one Mere Git source in each feature graph. The final
receipt binds 156 source/manifest inputs plus the live HTML fixture,
exact default/Piccolo test binaries, all preserved original/reconciled patch
hashes and the full logs. Persisted result lines accompany it.

Knot commit `802238cb9aae33d5d0ce334c8adef60874e9a6bb` passes selected workspace
library, standalone document and desktop/integration gates: 522 passed, two
ignored. Redshank commit `e08bf4b9875a7efb02b1c17bf9fac56708712cee` passes its
standalone workspace libraries and desktop bins: 225 passed, seven ignored.
Its earlier parallel HTTP connection-count failure is preserved; isolated and
final serial runs passed. The only fixture repair supplies a real temporary
PNG/file URL instead of a Windows-only repository path; geometry checks remain.

The single fresh-context reviewer inspected the whole consumer branch and the
immutable sibling commits and found no critical, important or minor actionable
issue. Source review was completed before final native counts; those counts are
recorded independently in the receipt. The reviewer confirmed the deferred
shell switch boundary. The Piccolo suite exercises actual supported bodies,
including the foreign-only tail control; a native mid-body session-adoption API
was not invented. Attributed recycle recovery and durable journal restart
recovery remain deferred. The nine ignored host controls require a live Knot endpoint, private trail
corpora or explicit measurements. Browser, optional engine/wasm, physical GPU
and assistive-technology qualification are separate.

Publication state: Mere and Genet are published; Knot, Redshank and this consumer
integration remain local. Initial sibling fetches used process-scoped local Git
URL rewrites while retaining canonical Cargo source identities. No global Git
configuration, supplier worktree, primary Turnstone checkout or unrelated WIP
was changed. Publishing this family requires making the two sibling commits
available before the Turnstone pin update. Continue from these final receipts;
do not reapply the historical patch artifacts or repin to the supplier's moving
P3/P4 lane.
