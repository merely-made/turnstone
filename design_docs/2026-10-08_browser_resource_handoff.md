# Turnstone browser and resource integration handoff

**Status, 2026-10-08:** paused at Mark's request for a fresh chat. Current browser
family adoption is published; the next resource-graph adoption is prepared,
unapplied and unqualified. This handoff does not authorize merging an unqualified
Mere checkpoint. Read the live tree before resuming.

## Start here

Read [the active index](DOC_README.md), [resource preparation](../docs/compatibility/graph_resources/README.md)
and this handoff. Keep the browser release lane separate from the forthcoming
Mere resource-graph migration. The earlier S0 tested-set browser repin is closed;
it must not be redone merely because older plan prose mentions a pending repin.

Mark's latest instruction is to make a handoff. No further dependency change,
build, release or publication was started after that instruction.
Mark is making **turnstone chat** on the ThinkPad to continue this work.
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
