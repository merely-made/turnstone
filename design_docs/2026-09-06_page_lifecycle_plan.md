# Page lifecycle plan

**Status:** in progress, refreshed 2026-10-17. Rulings L1-L7 were made with Mark on
2026-09-06. The research-workflow section below is a proposed implementation
slice, not delivered behavior or a replacement for those rulings. Source
capture and collection-scoped search now have app consumers;
the Keep, bin custody, and other lifecycle joins remain governed by their
individual done-conditions. This plan joins rules that
already exist in three documents into one experience and adds the rules that
were missing between them. It consumes the
[browser surfaces implementation plan](2026-08-25_browser_surface_implementation_plan.md)
(K0 Keep, S0 shallows), the
[page capture and provenance plan](2026-08-28_page_capture_plan.md) (D1-D6, P3
envelope), and the [recycle bin and Athanor](2026-07-20_recycle_bin_athanor.md)
decision. Those documents keep their own done-conditions; this one owns the
joins.

**Owns:** what a person may expect of one browsed page across visiting,
keeping, capturing, annotating, collecting, sharing, revision and deletion, and
which store holds each state.

**Does not own:** the capture transport (D1), the envelope fields (P3), the
bin's storage (recycle bin slices 1-2), place authority, or publication
identity (mere's moot object plan).

## The walk

One page, start to finish. Each step names its home store and the rule in
force. The Alembic pane's tiers read straight off it: Recent is step 1, Saved
is steps 2 through 7, engrams are what step 8 leaves behind.

1. **Visited.** The address enters trail memory as a traversal
   (`src/trail_memory.rs`, `eidetic::browsing`, session-scoped store at
   `sessions/<id>/memory`). A node appears in the shallows. Nothing about the
   page is durable except where you went (L1).
2. **Kept.** One gesture tags the node `keep` and saves the session
   (`src/app/node_arms.rs` `keep_node`). Keep retains address, title, tags and
   facets. No bytes (L3).
3. **Captured.** On an unkept page, capture is the Keep: one atomic promotion
   admits the node and deposits the envelope (L2). On a kept page it adds an
   envelope to a time-ordered series on the node. Bytes go to the
   content-addressed representation store (`src/download.rs` `store`,
   `muniment::BlobStore`); a file appears only on Save (D5).
4. **Annotated.** The default gesture writes a Knot block into an annotation
   facet on the page node. Clips cite a capture hash and selector. A promote
   gesture mints the block into its own knot-addressed node and links it (L6).
5. **Collected.** Containment edges only, as the gap analysis already rules.
   Removing the page from a collection removes an edge and touches no capture,
   note or Keep state. Not a decision here; recorded so the walk is complete.
6. **Shared.** The node and title travel. Captures and annotation facets are
   attachments: shared per item through place authority, redacted from
   exports unless chosen (L7).
7. **Changed.** A revisit asserts nothing. Only a fresh capture whose content
   hash differs from the previous one states that the page drifted, and the
   Inspector shows the series (L5).
8. **Discarded.** The tombstone (`src/action.rs` `RemovedRecord`) carries
   identity, facets, nested world and, after this plan, the capture envelopes.
   Recovery restores all of it, not edges. Athanor retires the tombstone after
   the retention window (`src/recycle.rs` `retire_then_list`), bakes the
   engram, and purges any blob no living node or tombstone still references
   (L4).

## Decisions

Each names what the tree already forces, so the ruling is a boundary rather
than a taste. Options are kept so the rejected paths stay visible.

### L1. History remembers where you went, never what you saw

S0's done-condition says unkept shallows content never enters durable session
stores. Trail memory durably records every visited address, kept or not, and
the recall indexes are minted from that corpus. The two agree only under a
written rule.

Options: as stated; stricter, trail memory records only kept nodes' traversals
(recall then cannot find a page seen but not kept, changing W1 and W2 as
landed); configurable with the stated rule as default.

**Ruling: as stated.** Addresses, titles and traversal order are history and
are durable. Content, captures and notes exist only past Keep. Trail memory is
exempt from S0's content clause because it holds addresses, not content.

### L2. Capture on an unkept page is a Keep

Captures are durable and node-attached (D5: deposit always after an explicit
capture is admitted). Shallows nodes are ephemeral once S0 lands. Nothing said
what a capture of an unkept page means.

Options: capture admits the node in the same atomic promotion S0 describes;
capture stays ephemeral and dies with the node unless Keep follows; capture is
refused until the node is kept.

**Ruling: capture is a Keep.** The capture is the provenance decision S0 asks
for. One gesture, no orphaned evidence. Before S0 lands, the same rule holds
trivially: capturing an unkept node tags it `keep` in the same update that
deposits the envelope, and a failed deposit leaves the tag unset.

### L3. Keep retains the node only

Today Keep retains address, title, tags and facets and no bytes. S0 names
"representations selected by policy" without a default.

Options: node only; node plus a viewport capture where the engine supports it;
a visible policy setting defaulting to node only.

**Ruling: node only.** Keep is a bookmark. Bytes come only from an explicit
capture, so Keep never carries a typed partial result on retained engines and
never costs a screenshot. The S0 policy hook stays, with node-only as its one
value until a second consumer forces another.

### L4. Captures ride the tombstone; athanor reaps blobs

`RemovedRecord` carries identity, tags, facets and the nested world, and the
nested world already follows an archive-before-leave, purge-with-tombstone
rule (`src/denizen.rs` `archive_world`, `purge_archived_world`). It does not
reference captures. Nothing reaps the representation store.

Options: envelopes ride the tombstone and blobs are reaped when the tombstone
retires; envelopes and blobs die the moment the node leaves; envelopes outlive
the node as loose evidence in Alembic.

**Ruling: ride the tombstone.** The removed record carries the node's capture
envelopes. Recovery restores them with the node. Blobs stay until athanor
retires the tombstone; the retirement pass then purges any blob that no living
node's envelopes, no surviving tombstone's envelopes, and no download facet
still names. Downloads already reference blobs by content hash and join the
same reference set. This is athanor's existing image orphan sweep extended
to representation blobs, not a second reaper (see Findings).

### L5. Only a new capture asserts change

The envelope has a content hash (P3), so drift is detectable on a fresh
capture. No rule said whether a revisit asserts anything about earlier
captures.

Options: a new capture whose hash differs is the only statement of change; the
response-bytes hash of every fetch is a drift signal (depends on the
view-source sidequest and a Weld fetch path that exposes bytes); every revisit
takes an implicit comparison capture.

**Ruling: only a new capture asserts change.** Captures are a time-ordered
series on the node. A plain revisit says nothing, matching P3's rule that no
field is inferred. The fetch-hash signal stays available to the view-source
sidequest and would be a second, separately named fact, never a relabelling of
this one.

### L6. An annotation is a Knot block in a facet, promotable to a node

No annotation model existed. The stack offers three carriers: a facet on the
node (the `web.*` shape), a Knot node with its own `knot://` address, and Knot
clips that already carry a selector, fidelity and an observed edge to their
source.

The discriminator is whether anything needs to point at the note. A remark
only the page refers to is a leaf. A note another note clips from, that a
share names, that recall returns on its own, or that must survive independent
of one page, is a node. Metadata and versioning follow from that: a leaf's
history is the page node's facet history, a node has Knot revisions.

Options: a block in a facet with a promote gesture; always a linked Knot node
(uniform, but recovery must carry the link, and a two-word sticky costs a
node); always a facet block with no promotion.

**Ruling: block in a facet, promotable.** One format, two homes. The default
gesture writes a Knot block into an annotation facet on the page node. A
promote gesture mints the block into its own knot-addressed node and links it
with a typed relation, the same kind of deliberate tug as Keep and the feed
pop-off. Annotation facets are attachments under L7. Fleece extracts are
representations with provenance, not notes; the word annotation is reserved
for authored remarks.

### L7. Shared: address only, attachments opt-in

The peer-web proof shares an address into a place graph. P3 leaves it open
whether an engram export carries or redacts the capture record, and requires
that the choice be deliberate the way the `web.*` runtime facets were decided.
That precedent is mere's `pandect::graph_codicil::RedactionPolicy`
(`crates/system/pandect/src/graph_codicil.rs:105`): default-off flags over a
named facet list, with `include_all` reserved for a local, trusted freeze.

Options: address only with attachments shared per item; everything attached
rides by default; address and notes travel, captures never.

**Ruling: address only, attachments opt-in.** The node and title travel.
Captures and annotation facets are attachments, shared per item through the
same place authority as any durable action and redacted from exports unless
chosen. A viewport capture can carry logged-in content, so evidence never
leaves by accident.

## Phases

Order follows dependency. L-P1 and L-P2 are independent of each other; L-P3
needs L-P2's reference set; L-P4 and L-P5 are independent of the rest; L-P6 is
last because it reads the series the others create.

### L-P1. Capture admits Keep

Route the capture action through the Keep promotion so an unkept target is
tagged in the same update that deposits the envelope. Before S0 this is a tag
plus a deposit in one `update`; after S0 it is the atomic promotion S0
specifies.

Done-conditions:

- Capturing an unkept node leaves it kept, with `node-kept` emitted once and
  the envelope attached.
- A failed deposit leaves the node unkept and emits the typed capture failure.
- The strip, palette and automation catalog offer the same target-bound
  capture, and its label on an unkept node says it keeps.
- A focused unit receipt covers both outcomes.

### L-P2. Envelopes ride the tombstone

Extend `RemovedRecord` and the eidetic `DeletedNode` boundary to carry the
node's capture envelopes, and restore them on recovery.

Done-conditions:

- Delete of a node with captures stages every envelope in the record; the
  record round-trips through the bin port unchanged.
- Recovery restores the envelopes on the same node identity, and each still
  resolves in the representation store.
- The Trail's Removed row can say how many captures ride with a record without
  opening bytes.

### L-P3. Blob reaping in athanor's retirement pass

Give the retirement pass a reference set and let it purge unreferenced blobs
after it drops a tombstone. Reuse athanor's pure propose/apply split
(`propose_image_gc` marks only orphans; apply rechecks live references) and
extend it to muniment representation blobs. Turnstone supplies the reference
set; pandect owns the pass logic today, and its scheduling actor belongs to the
Distillery Athanor component (`ports/distillery/athanor`, `mere-athanor`,
a reservation stub since 2026-09-02 with no lane opened).

Done-conditions:

- The reference set is computed from living envelopes, surviving tombstones'
  envelopes, and download facets, in that session, and is asserted non-empty
  when any of those exist (a positive control, so an empty set is a finding,
  never a silent success).
- A blob shared by a retired tombstone and a living node survives.
- Emptying the bin on command reaps the same way and reports the count.
- Retention remains the Apparatus knob the recycle bin doc names.

### L-P4. Annotation facet and promote

Add the annotation facet holding one Knot block, its editor surface, and the
promote gesture that mints a knot node and links it.

Done-conditions:

- A block written into the facet renders through `DjotKnotEngine` on the same
  route a note node uses.
- Promote moves the block out of the facet, mints a `knot://` node under the
  existing note route, and records a typed relation to the page; the facet is
  then empty, not duplicated.
- The facet rides the tombstone and returns on recovery.
- A clip taken from a capture into the block cites the capture's content hash
  and selector.

### L-P5. Attachment class in the share and export rule

Classify capture envelopes and annotation facets as attachments wherever a
node leaves the session: engram export and place sharing.

Done-conditions:

- An engram export of a node with captures and a note carries neither unless
  each is chosen, and the export states what it redacted.
- Sharing a node into a place carries the address and title; a chosen
  attachment passes through place authority as its own durable operation.
- The `web.*` runtime-facet precedent and this rule are stated in one place,
  not two: capture envelopes and annotation facets join `RedactionPolicy` as
  default-off fields rather than a Turnstone-side filter.

### L-P6. The series in the Inspector

Show a node's captures as a time-ordered series and state drift only when two
adjacent hashes differ.

Done-conditions:

- The Inspector lists captures by completion time with scope, engine and hash,
  without opening bytes.
- Drift reads as a comparison between two named captures, never as a claim
  about the live page.
- Observation exposes the same series for automation and accessibility.

### Receipts

- Restart receipt: an unkept visited page leaves a trail trace and nothing
  else in durable stores (L1 with S0's own receipt).
- Headed scenario: visit, capture, annotate, delete, recover, empty the bin,
  and assert at each step what the representation store and bin contain.
- The trail recall evaluation corpus is unaffected by L1 and this is checked,
  not assumed.

## Research workflow: smallest complete slice (2026-10-17)

**Status: R0 implemented and qualified within the bounded receipt below; R1-R4 remain planned.** Mark asked
for the existing research workflow to be traced, its missing joins identified,
and the smallest complete slice planned. This section owns that product join;
the capture, pane, Knot and resident plans retain their domain gates.

### Source cut and evidence

Audited clean Turnstone `428deb5` (`428deb5` also matched fetched `origin/main`),
with Rust pinned to 1.99.1 and Mere locked to
`a17e5bb97c79332b932d10cce70f465b022a3a1a`. The October 16 scene/answer receipts
qualify their recorded earlier sources; they are supporting evidence, not a
research-workflow receipt for this source. The following is a current-source
inventory. Named tests/scenarios were inspected, not executed for this plan.

| Person's step | Existing implementation | Join still needed |
| --- | --- | --- |
| Start somewhere to work | New/Rename session, session switcher, graph/window/view persistence (`src/app/session_lifecycle.rs`, `src/action.rs`) | Use a named personal session; a new project/container type is unnecessary. |
| Gather and keep pages | Targeted `OpenAddress`; idempotent `keep_node` bookmarks one Surface (`src/app/node_arms.rs:123`) | Keep remains metadata-only under L3. An ordinary explicit source-capture control is missing. |
| Preserve evidence | Exact fetched HTML candidate, raw response + validated Fleece annotation in the existing trail-owned session store; ordered manifest facet (`src/content.rs:452`, `src/shell/effects.rs:1734`, `src/source_capture.rs`) | `SourceDocumentCaptured` attaches a reference and saves but does not Keep. L-P1's completion-bound admission and source/session ownership must be joined; current completion checks the member but appends through active-runtime facet convenience state. |
| Find one's saved material | Session recall and a local validated Fleece library exist; captured-page search is an authority-filtered shared-place projection (`src/trail_memory.rs`, `src/place/captured_collection.rs:104`) | Provide a private projection from this session's attached capture references. Do not require a Moot/share to find one's own captures, or admit orphan store records by scanning. |
| Open a saved passage | Answer citations have exact text/hash/Unicode-range validation (`src/inference_service.rs:318`) | Existing captured-search hits lower to `OpenAddress`, hence refetch. A capture-manifest-bound reader must work independently of origin and inference resident. |
| Ask over chosen evidence | Streamed/durable resident asks, source markers, exact-quotation labels and historical I5 reopen evidence | Current Answers publishes all PageText heads and sends empty scope (`src/trail_memory.rs:1625`, `src/inference_service.rs:220`). Add a capture-backed adapter and nonempty selected source scope. |
| Author a sourced note | Reusable local Knot document pane; separate addressed Knot/typed Inspector clip route (`src/knot_document_surface.rs`, `src/knot_authoring.rs`) | The file picker mounts a path-only pane, not a graph source or clipping destination. Destination is environment-configured; Inspector's offered and executed source can differ. This is a named follow-on, not a completed join. |
| Share selected work | Address-only place sharing, authority-filtered captures, live holder-backed Knot projection and selected clip packages | Deliberate attachment choice, destination/source binding and holder availability remain separate joins under L7/L-P5. |

### The first complete user task

In a named personal session, visit two small HTML sources, deliberately save
both as evidence, find a passage in the saved text, ask a question using an
explicit subset, inspect its exact cited passage, close everything and return
to the same evidence after the origin is gone. Ordinary browsing of unrelated
material must not enlarge that research corpus.

This delivers **collect -> find -> inspect -> ask -> verify -> return**. Authored
Knot notes and peer sharing are the next joins. They are not earned by this
slice and are not prerequisites for private saved-source reading. Generation
and durable answer history still require the admitted resident/model; reading
and searching locally held evidence must work with both origin and inference
resident stopped. A resident outage may report Answers unavailable; this slice
does not invent a second durable answer store in Turnstone.

Initial source capability is fetched HTML with observed final URL, acquisition
time and real content type. Hosted engines that cannot supply that candidate
show an unavailable reason. A PNG, current DOM, extracted recall head or live
URL is never substituted for an exact source capture. The supported reader is
an inert saved-text/provenance view, not offline replay of an interactive web
application. No new engine, inference wire, shared collection system or store
is required by the audited source contracts.

### Product and ownership boundaries

- **A session is the research scope.** Use existing New/Rename/Switch actions.
  “Saved sources” is a view of that session's attached captures, distinct from
  Gemot's versioned captured-search collections. Persist selected capture
  references and reader/view intent through the session's existing persistence
  paths; do not duplicate source bytes in settings or sidecars.
- **Capture identities remain immutable.** A saved-source reference binds the
  owning session/graph/member and annotation manifest. Load provenance and
  canonical text from the validated Fleece record. The live URL is displayed
  provenance and a separate “Open original address” action. A second deliberate capture
  is another ordered occurrence; identical bytes/acquisition context may resolve
  to the same immutable manifest. Distinct changed-content artifacts remain
  separately selectable, and no occurrence retargets an earlier artifact.
- **One store owner.** The trail actor remains the only Fjall owner. Its
  request/completion vocabulary supplies the admitted local capture projection,
  search and exact resolve. Correlate session/store generation as well as node
  and request; stale completion cannot attach or publish into another session.
  Disabled, missing, invalid, withdrawn and unavailable entries stay explicit.
- **Follow L1-L3 on new writes.** Ordinary visits retain address/title/order;
  Keep retains node metadata; an explicit capture admits source bytes. Wire the
  automatic extracted-body write gate accordingly. The current unconditional
  `consented_to_keep` conflicts with L1; the capture plan's later Full-vs-opt-in
  tail does not silently overturn that ruling. The proposed slice follows
  metadata-only ordinary history. Legacy PageText bytes/runs are preserved and
  excluded from the new evidence context, without silent purge or migration into
  source evidence. A general Full automatic-retention policy needs its own
  explicit reconciliation; this plan does not declare that policy shipped.
- **Preserve authored-note ownership.** Fleece's “annotation record” is an
  extraction/capture artifact, not the authored annotation promised by L6.
  `WriteNote` is a graph-body helper, not a substitute for Knot evidence.
- **Selection is not availability.** Publish all still-attached, admitted
  captures for a bounded research owner, then supply a nonempty exact source-ID
  scope per ask. Changing the chosen subset does not withdraw older citations
  while their source remains admitted. Explicit deletion/detachment, missing
  content or invalid integrity may withdraw availability. Never implement an
  empty UI selection as wire `scope: []`, which currently means all sources.
- **Preserve legacy Answers.** Add a versioned capture-research pane context;
  derive its owner from the validated session (for example
  `turnstone/research/<session UUID>`). Keep V1's `turnstone/<session UUID>`
  PageText context, history and strict resolver unchanged. Namespace strings in
  saved payloads are not authority. Use one coordinated service/publisher per
  derived owner, with exact namespace/run filtering and current actor admission.
- **Keep the existing publication ceiling.** The complete encoded corpus JSON,
  including metadata/escaping, must fit 128 KiB. Preflight and visibly refuse
  oversize before a generation request; do not truncate, silently publish a
  subset, broaden scope, or prevent independent saved-source reading. Capture
  and reading may outgrow this inference budget. Resource-backed large-corpus
  handoff belongs to the resident plan's separate open gate.

### Ordered implementation phases

#### R0. Save acknowledgement, integrity and partial-failure recovery

The persistence critique adds a prerequisite to the product join. At the
pre-implementation source (`45542b1`, fetched current on 2026-10-17), `SourceDocumentCaptured` is emitted before `Effect::SaveSession` runs
(`src/app/updates.rs:595`). `persist_session_graph` propagates facet/graph write
errors, but the shell's `save_session` returns `()` and logs most failures
(`src/app/session_lifecycle.rs:1464`, `src/shell/effects.rs:2162`). The capture
request therefore has no correlated session-save acknowledgement. Ordered
facet-before-graph writes (`src/session_persistence.rs:91`) do not make the
capture store and session files one transaction.

Before R1 can claim success, specify and implement the following bounded
contract through the existing owners:

- Correlate the source request, owning session/member and admitted artifact
  identities through deposit, attachment/Keep and persistence completion.
  Distinguish deposited, attached/pending-save, saved and failed/recoverable.
  Emit final saved success only after the required artifact and owning
  facet/graph writes acknowledge success. Identify which writes that promise
  covers; unrelated best-effort view/preferences sidecars must not supply its
  success signal. A failure may leave recoverable state; it must remain visible
  and retryable without losing evidence or duplicating the same completion.
- Inventory write ordering and recovery for raw bytes, raw/annotation manifests,
  facet references and graph/Keep metadata. Inject failure after each boundary,
  terminate/reopen and retry. Orphan deposits stay outside Saved sources and
  publication; incomplete references produce an attributable refusal. Preserve
  valid pre-existing state. Do not add destructive cleanup as compensation.
- Treat batch atomicity, successful write acknowledgement, process-restart
  survival and power-loss durability as separate claims. Locked Mere
  `a17e5bb9` has the same `Backend::apply` contradiction as current Mere: its
  all-or-none wording permits ordered partial application, and Fjall implements
  the latter. `transact` already provides a stronger read/check/write contract
  with typed `NotTransactional` refusal; Fjall does not implement it. Do not
  depend on atomic `apply` or fall back from required transactions. Any required
  shared-contract repair belongs in Muniment. Identify the actual persistence
  barrier before promising abrupt power-loss survival; a cold-reopen receipt
  alone does not establish it.
- Keep blocking Fjall calls on the existing trail actor thread. Turnstone's
  `spawn_named` already supplies a dedicated thread; a new executor/store owner
  or scattered per-call `spawn_blocking` wrappers are unnecessary here.
- Verify the exact addressed manifest and payload at the research admission
  boundary. Muniment's primitive `BlobStore::get` does not verify bytes, but
  Eidetic's typed resolver verifies payload hashes and
  `load_fleece_annotation` additionally validates annotation facts. Consume that
  existing checked path with `NoFetcher`; prove that wrong bytes, a substituted
  manifest identity and inconsistent raw/annotation links cannot enter reading
  or publication. Do not replace saved evidence with a network fetch.

**Done:** focused fault/retry/reopen controls demonstrate the declared saved
boundary, preserve previously saved evidence, and never announce saved success
on a required-write failure. Record the supported failure model and supplier
contracts. General Graphshell preference health and browser persistence UI
remain separate work; they are not prerequisites for this native slice.

**Implementation cut (2026-10-17, qualified below):** R0 starts from
Turnstone `45542b1` and was fast-forwarded to fetched `b7e1529` before final
qualification, adopting its locked Mere `55c479bc` supplier. The storage APIs
used by R0 were compared across that update. Final qualification adopts
published Mere `00e444286`, whose only runtime change reads canonical Resource
tags for Sets scenes. The unchanged Turnstone scene assertion exposed that
supplier mismatch; no consumer-side tag shim or weakened assertion was added.
The earlier
`428deb5` inventory above records the planning baseline. Concurrent dependency
adoption in the primary checkout required the isolated `codex/r0-source-save`
lane; those unrelated changes were preserved.

R0 now follows one request UUID through the existing trail actor and stores
intent/result/completed-save receipts in that actor's existing Eidetic
namespace. These receipts are recovery inventory, not a new store or corpus.
The app attaches each request once, preserves distinct same-manifest gestures,
and saves that explicit runtime's canonical facets and graph. Final success
waits for those writes and the verified completed receipt's acknowledgement.
The request receipt also names the original decoded-body hash. Its checked
UTF-8 body blob is retained before raw deposition, allowing faithful offline
retry for originally decoded non-UTF-8 HTML. An intent whose raw bytes never
landed remains an explicit original-response-required failure; it is never
replaced by a URL fetch. A partial intent reuses its exact local original even
if the live address now has a newer response; that replacement is never
deposited for the old request. A fresh request still rejects substituted
response identity before writing its intent.
Receipt/artifact retention roots remain subject to the
separate custody/GC gates; this slice performs no reclamation.
The bounded capture save excludes unrelated feed, pane, view and preference
sidecars; ordinary whole-session save retains their existing behavior. R0
persists existing Keep state; automatic Keep promotion remains R1.

Facet/request ledgers must agree with the ordered reference occurrences.
Incomplete or malformed ledgers fail visibly instead of being reconstructed by
scanning all artifacts. Explicit retry reuses the original request and checked
local evidence, including after a lost write acknowledgement. On reopen the
actor verifies completed receipts individually; the app rechecks their owning
session, graph, member and attachment before restoring Saved status without a
new success event. Historical saved evidence does not require the member's
current address to remain its original address. Failed/incomplete requests
remain attributable and offer an ordinary retry action. The observation
snapshot and event stream expose progress, artifact identities and failures.
Warm session adoption first makes prior Saved qualifications and unfinished
work await fresh verification, so an unreadable inventory or unavailable store
cannot preserve a stale Saved badge. An ephemeral opening token binds actor
commands, deposit/save completions and recovery inventory to the current
adoption. Queued replies from a prior opening cannot requalify the same request
after A/B/A adoption. The token is not persisted as authority: explicit retry
retains the original request/evidence and runs under the current opening.
Current-opening recovery inventory cannot overwrite a live operation or reuse
its request ID with different facts.
Forks preserve source references and unrelated facets while stripping the
donor's local request/ack ledger from reminted child nodes. A fork receives no
new local custody or Saved authority; its new explicit requests remain usable.

Shared-stack findings: Pandect file writes and Fjall's default buffered journal
acknowledge OS writes; the exposed APIs supply no cross-store transaction or
`sync_all` boundary. The acceptance claim is bounded to write acknowledgements
and tested process restart/exit survival, not power-loss durability. Eidetic's
typed resolver checks payload hashes; host admission additionally binds exact
manifest keys/IDs, local payload addresses/size, schemas, Fleece integrity and
raw/annotation acquisition links. Identical raw bytes share a manifest, so each
checked annotation owns its URL/time facts. No network fetch repairs evidence.
Before deposition, R0 preflights the raw, annotation and schema roles at their
content addresses. Compatible existing manifests retain their exact metadata;
incompatible roles or missing prior payloads are refused before typed writes.
This prevents response bytes that happen to equal an annotation/schema payload
from replacing that valid artifact's manifest. The guard is local to this
serialized actor; it does not strengthen Eidetic's general write contract.
The shared-place library remains lookup inventory intersected with authorized
fauna; its live refresh now follows save acknowledgement. R2/R3 must derive
private research admission from saved attachments, never deposit inventory.

**Qualification (2026-10-17):** the canonical locked run against published Mere
`00e444286` passed **940 tests**, with zero failures, ten existing ignores and
89 filtered tests. Its declared scope excludes `place::`, `sky_surface::` and
`sky_timeline::`; it is not an unrestricted-library pass. It includes all
**50 capture controls** and **16 unchanged scene controls**, including the Sets
assertion that exposed the supplier mismatch. Three captured-collection
lookup/search/withdrawal controls separately passed in that same frozen binary.
The source/lock inputs, binary digest, exact commands, test names and retained
diagnostics are in the [R0 receipt](../docs/receipts/r0_source_save_20261017/qualification.json).

The controls inject failures before and after each actual Fjall put, exercise
required facet/graph write failures and lost receipt acknowledgements, and
preserve earlier valid evidence. A joined four-process App/actor/Fjall run exits
without Rust Drop/Flush after facet failure, graph failure, then successful
offline retry; its final fresh process quietly revalidates exactly one saved
attachment. Additional controls cover warm A/B/A adoption and stale opening
messages, real same-directory actor reopening, foreign-session refusal before
deposition, original non-UTF-8 decoding, newer live-response replacement,
typed-role preservation, corrupt local evidence, ordered duplicate occurrences
and forked evidence without copied job authority. The final Clippy correctness
gate, scoped Rust formatting and whitespace checks pass. Other Clippy warning
groups remain warnings.

The earlier unrestricted parallel run was interrupted after twelve completed
failures, including the now-fixed scene assertion and shared-place controls;
three individual place controls subsequently passed. That does not qualify
the remaining place/Sky profile. Mere's full supplier run passed 516 tests,
failed its unchanged wall-clock speed budget control and retained 18 ignores;
the clock control also failed alone on this host. These diagnostics remain in
the receipt rather than being replaced by the bounded pass. No headed/GPU,
physical recovery, Windows/Linux or power-loss qualification is added by R0.

#### R1. Ordinary explicit capture, Keep and honest retention

Expose the existing source-capture action through the common action catalog and
an ordinary source-bound control, with proposed wording “Save source and keep”.
Bind the displayed pane/member, owning graph/session, current fetched-document
admission and request generation; revalidate on execution and completion. Use
existing target/placement authority, never the active compatibility cursor.
Write the resolved runtime's facets and persist its owning session. If that
source's session/store is not the actor's currently admitted owner, refuse
before deposit; do not reopen another store or relabel the current one.

On successful deposit, attach the manifest and apply idempotent Keep to that
same owner, emit `NodeKept` only on a new promotion, and request one session
save. Use R0's correlated acknowledgement for final saved success, not the
deposit/completion event. Failure/stale source must not Keep the wrong node or attach evidence.
This is L-P1's completion-bound state change, not a claimed transaction across
Fjall and graph/facet files. Successful deposit followed by attachment/save
failure remains attributable and recoverable; preserve bytes/refusal evidence
and show a failure instead of a false “Saved”. Unattached deposits do not enter
Saved sources or publication. Gate new automatic body writes under L1 as above;
preserve address-only history and existing legacy retained data.

**Done:** ordinary pointer/keyboard/queued semantic activation reaches the same
bound action; followed source A with cursor B changes only A; failed, same-URL
replaced-document, wrong-session and A/B/A stale actions fail; repeated
delivery of the same request completion produces no duplicate attachment or
Keep event. Distinct admitted capture gestures preserve their ordered entries
even when their artifact manifest is equal; manifest equality is not a request
idempotency key. A persisted success
has both Keep and the exact manifest reference. Unsupported engines explain
why the control is unavailable. Address recall survives with automatic body
writes disabled.

#### R2. Private saved-source search and retained reader

Derive a list/series from live node capture-reference facets, not every manifest
in the store. Reuse L-P6's per-node series facts in Inspector and the existing
omnibar/retained-surface patterns for “Search saved sources” and source opening.
Search exact validated canonical text; carry the capture identity and optional
half-open Unicode scalar selector through the result instead of lowering it to
`OpenAddress`. Show source title/origin, capture time and version; reveal the
matched passage and distinguish “Open original address”. Keep equal text's
source identities/provenance separate even if the index groups it. Preserve
occurrence order in the series while publishing one entry per exact
owner/member/manifest source identity, not duplicate wire source IDs.

Use the pinned public `fleece::load_fleece_annotation` with local/NoFetcher
validation, through the existing actor. Its canonical text and BLAKE3 text hash
are distinct from raw-response hashes and the canonical-text IRI's SHA256.
Render a host-resolved `EngineDocument` through the existing retained text
session pattern. The imported retained-clip route expects a ClipRecord in a
different store; do not convert/reimport a FleeceAnnotationRecord just to read
it. No shared reader abstraction is promoted for this first consumer.

**Done:** private sources can be listed, searched and opened with no joined
place, no network and no inference resident. Selection survives normal view
retargeting. Missing/corrupt records remain identifiable unavailable entries;
wrong session/member/ref, invalid hash or selector cannot open another record
or fetch live content. Revisit A with changed bytes leaves captured A1 intact;
explicit capture A2 creates another version. This phase may close the relevant
L-P6 source-document series gates only after their own assertions pass; it does
not qualify hosted screenshot/PDF series.

#### R3. Capture-backed, explicitly scoped Answers

Extend the same actor's owner interface with a capture-specific snapshot and
exact resolve; keep the legacy PageText resolver restrictions. The locked
`RecallSourceId` already supports `Fleece`/`Capture`, owner, source and node;
`RetainedSourceText` already carries version/title/exact text. Use immutable
annotation-manifest identity, owning member UUID and canonical-text hash; avoid
current URL/head identity. The producer must validate local facet admission,
record integrity and session ownership before publication, because the resident
publisher does not supply capture-specific validation on its behalf.

Version the Answers pane source for the derived research context; persist the
nonempty chosen capture IDs as view state, and revalidate on reopen/ask. Do not
persist a corpus copy. Preserve one publisher for this context across panes.
An empty, stale or wholly unavailable selection refuses before generation;
no PageText or all-session fallback. Invalid records are reported and excluded
from availability, while an ask naming one must fail. Existing history remains
available through the correct resident when its sources are still valid.

**Done:** the source-ID namespace is accepted through the real admitted route;
wrong owner fails; the recorded requested scope equals the chosen A/B
identities, every supplied passage/prompt source belongs to that scope, and the
fixture question supplies its relevant A passage while excluding visited-only
X and the other session's sentinel. B need not enter the prompt when the
ranker/context budget does not select it. Selection changes leave old
A1 citations resolvable; A2/live edits do not replace A1. Citation activation
reveals/highlights its exact Unicode passage and repeats hash/range validation.
Oversize encoded publication refuses visibly with zero new generation; source
reading remains available. Reconnecting preserves owner/run isolation and does
not let an old control mutate another session's ask.

#### R4. One end-to-end acceptance receipt

Use a new isolated profile, two small public fixture HTML pages A/B, a unique
sentinel on visited-only X, and a second session with a distinct sentinel. Use
ordinary named actions and actual visible controls; no direct store insertion
may stand in for capture admission, source selection or reopening.

1. New/Rename session; visit A/B/X; explicitly save A and B. Inspect durable
   references and Keep state independently. X has address/title history but no
   newly retained body and is absent from the evidence index/publication.
2. Search a non-ASCII phrase; open A1's saved text at the exact scalar range.
   Stop both origin and inference resident: list/search/read still work, with
   no acquisition attempt. Answers reports its separate resident requirement.
3. Start the declared real resident/model. Choose a nonempty A/B scope, ask,
   inspect supplied source IDs and prompt ledger, then open the exact citation.
   Assert mechanics and exclusions; do not infer general answer entailment or
   model quality from a source marker. Preserve any deterministic fixture
   controls separately from the real-model integration receipt.
4. Restart the controlled origin, change live A at the same URL, revisit,
   then explicitly capture A2. Compare
   A1/A2 identities and retain A1 passage/old citation. Change the selected ask
   subset without treating it as source withdrawal.
5. Save and terminate app/resident; stop origin; cold reopen the named session.
   Assert source graph/Keep, manifest identities, chosen IDs, reader version and
   arrangement. Source reading works first; restart resident separately and
   verify durable answer/history and exact old citation against A1.
6. Exercise failed deposit, failed attachment/save, empty selection, oversize
   publication, missing/corrupt Fleece, invalid Unicode range, wrong owner,
   stale queued action, session switch and late completion. None may claim
   Saved/answered, fetch a substitute, admit orphan data or leak the other
   session's sentinel. Deliberate source withdrawal may invalidate its citation
   with an attributable reason; it must never silently replace the source.

Fast tests qualify pure identity/filtering/retention policy and actor adapter
behavior. Real consumer tests qualify control/selector wiring. A new headed
macOS capture/read/ask/reopen run qualifies the assembled first slice on its
frozen source and coherent lock. Record build/test commands, final full
workspace result, model/source identities, independently checked stores, prompt
ledger, reviewed readbacks, unavailable outcomes and cleanup. Historical I5
and scene receipts cannot substitute for this run. Windows/Linux, human AT and
physical device/recovery qualification retain separate gates.

### Explicit next joins and exclusions

- **Sourced authored research note:** choose/create an actual Knot-owned
  document as this session's destination, make its identity visible and durable,
  and carry a selected saved passage with its artifact/hash/selector into the
  note beside authored commentary. Opening an existing local Knot file is not
  itself a graph binding or a clipping target. Reuse the pane plan's
  “Next Inspector surface: ownership before semantic routes” prerequisite:
  displayed source A versus cursor B, source/content/endpoint admission and
  stale-token revalidation. Preserve `DocumentClip` and `KnotClipHandle.insert`;
  the current environment-only target is not the finished chooser. A standalone
  research document can be a referenced note under L6; page-leaf annotations
  and promotion still belong to L-P4. Prove note/evidence/reopen together before
  claiming this next join complete.
- **Deliberate selected sharing:** consume L7/L-P5 and place authority for
  individually chosen attachments, disclose destination and transferred
  material, and qualify reader refusal/withdrawal plus exact selected export.
  A private capture list is not a shared collection. Live holder-backed Knot
  editing does not imply offline visitor editing. No capture/note leaves by
  being visited, kept, selected for an ask or placed in a research session.
- **Remain separate:** general annotation/promotion; deletion/recovery/blob-GC
  completion (L-P2/L-P3); hosted/viewport/PDF capture; automatic Full retention
  settings and legacy data disposal; arbitrary large-corpus publication;
  offline interactive-page replay; a new project type, universal collection
  schema or shared research framework. Existing deletion must conservatively
  preserve evidence and withdraw missing admission; no irreversible “Forget”
  promise is added before its custody/GC gates land.

### Implementation entry and stop rules

Start with R0's bounded save acknowledgement and recovery contract, then R1:
source-bound ordinary capture and completion -> Keep/reference persistence,
plus metadata-only new history writes. Their positive and negative
controls must pass before introducing the private reader or Answers context.
R2 then supplies resident-independent return; R3 connects the existing answer
engine; R4 qualifies the complete task. Individual phases may land separately,
but only R0-R4 earns the first research-workflow claim.

Implementation must re-fetch/check active work and confirm the locked Fleece,
recall and rendering contracts before editing. Preserve the existing single
store owner and legacy run namespaces. If the audited supplier lacks the exact
required contract at implementation time, stop at that named owner boundary
and record the missing contract instead of introducing a parallel store, wire
or fake success. If native desktop, resident/model or origin control is
unavailable, keep software evidence separate and leave that acceptance gate
open. No permissions, accounts or outside participants are created by this
plan.

## Findings

Verified 2026-09-06 against the working tree; capture and collection boundaries
refreshed 2026-09-12.

- Keep is a tag (`src/feed.rs` `KEEP_TAG`) applied by `keep_node`
  (`src/app/node_arms.rs:93`), idempotent, emitting `NodeKept` and requesting a
  session save. No bytes are touched.
- Trail memory is an observer of browsing, never a gate, and holds
  address-level traces in `sessions/<id>/memory` (`src/trail_memory.rs` module
  header). Current graph and bin titles overlay traces for recall only and do
  not rewrite history.
- `RemovedRecord` (`src/action.rs:1092`) carries id, url, title, tags,
  deletion time, nested world and the facet bundle. It has no representation
  field. Delete archives the nested world before the node leaves and aborts on
  archive failure (`delete_focused_node`, `src/app/node_arms.rs`).
- The representation store is `muniment::BlobStore` over a redb backend in the
  session directory (`src/download.rs` `store`). It is content-addressed and
  nothing reaps it.
- Athanor's retirement pass runs at session open (`src/recycle.rs`
  `retire_then_list`), not on a clock; the engram bake is still the recycle
  bin doc's open slice 3.
- The explicit source-capture path now stores exact response bytes and a
  `FleeceAnnotationRecord` in the session Eidetic store, then appends the
  annotation manifest to `capture.source-annotations/v1` only if the node and
  URL are still current. The P3c place consumer reads those records through a
  trail-actor-owned in-memory library, so the place worker never opens a
  second Fjall handle. Gemot authority and per-share withdrawal select the
  effective contributions; the app's grouped searchable collection is a
  rebuildable projection. This does not implement L2 Keep promotion or L4 bin
  custody.
- A note is already a routed `knot://` document, and Knot clips carry a
  selector, fidelity and an observed edge
  (`src/knot_authoring.rs`, mere's archived djot editor plan reframe of
  2026-06-27). Facet ids follow the `web.page`, `web.reader-lineage` shape
  (`src/content_classes.rs`, `src/content.rs`).
- Collections are containment edges and the Roster is the manifest view
  (browser gap analysis, Collections table).
- Captured-search collection scope is a local view choice over an exact Gemot
  `CollectionVersion`, not another collection or sharing fact. The worker
  admits only that version's current `effective_selected` contributions. A
  stale version exposes zero captured content; clearing the choice restores
  `AllEffective`. The exact choice now survives worker resync and cold reopen.
  The command palette offers authorized versions and a separate captured-page
  search mode. A stale selection requires an explicit switch to the latest
  version; source links navigate to the original address. Headed acceptance
  and offline snapshot replay remain open.
- The engram export is `pandect::graph_codicil`, not `graph_engram` as the
  capture plan named it; that citation was corrected in this pass. Its
  `RedactionPolicy` (`graph_codicil.rs:105`) drops thumbnail, favicon and
  session-state references by default and strips six named `web.*` facets;
  redaction drops the reference and leaves the blob to the orphan sweep.
- Athanor's forgetting logic is pure propose/apply in
  `crates/system/pandect/src/athanor.rs` (`propose_image_gc`,
  `propose_retirement`, `apply_image_reference_forgetting`), honouring
  eidetic R0: apply drops only cached content, never graph truth or
  codicils. The image orphan sweep already marks only unreferenced digests
  and rechecks live references before dropping.

## Pitfalls and open points

- L-P5 changes `RedactionPolicy` in mere, so it is a mere change with a
  Turnstone consumer, and the second consumer rule in the surfaces plan
  applies: Turnstone's export and place-share paths must both call it before
  the field is added.
- L2 before S0 relies on the capture and the tag landing in one `update`. If
  the deposit completes asynchronously through the hosted event stream (D1),
  the tag must wait for the completion event, not the request.
- L4's reference set must include download facets or emptying the bin can
  reap a download's bytes. The unit receipt in L-P3 covers this case
  explicitly.
- The label for capture on an unkept node is product wording and not ruled
  here.

## Progress

- **2026-09-06:** plan written. L1-L7 ruled from a working-tree assessment;
  cross-references added to the surfaces, capture and recycle bin documents
  and to `DOC_README.md`. No code written.
- **2026-09-09, P3a in progress:** Turnstone now has an explicit
  `CaptureSourceDocument` action/effect and a node-plus-current-URL-scoped
  fetched-document candidate. It deposits exact raw response bytes and a
  LocalOnly Fleece annotation into the existing session Eidetic/Fjall memory
  store; recall `PageTextStore` text and viewport PNG capture do not enter the
  path. The candidate accepts only an observed response URL from
  `PageStreamed`. The current terminal fetch contract has no final URL for a
  non-streamed HTTP response, so those captures refuse rather than inventing
  final-source evidence. Hosted Weld remains unsupported for the same reason:
  it does not expose source response bytes. Validation is pending the
  clean-source resolver gate. The Mere dependency transaction advances every
  Turnstone Mere git pin together from `2b1cd731` to `725b0f35`, the current
  shared commit that provides the Eidetic bridge. On a current completion,
  the node appends only the annotation manifest to its durable
  `capture.source-annotations/v1` facet series and triggers `SaveSession`; a
  stale completion remains in Eidetic but is reported as unattached rather
  than being rebound to a later target.
- **2026-09-10, collection-scoped capture search verified:** The app and
  shell now carry an exact collection version through a dedicated local-view
  action, effect, worker command, and completion. The place worker remints from
  `authorized_collection(...).effective_selected`; stale, unavailable, and
  foreign selections fail closed with zero captured content, and `None`
  returns to `AllEffective`. A worker/storage error keeps the prior snapshot.
  Nineteen focused tests pass across the app seam, worker, captured collection,
  and source-capture regression filters. Cold-reopen persistence and headed UI
  remain open.

- **2026-09-12, persisted captured-search consumer:** An exact local collection
  choice is restored before the first projection on reopen. The command palette
  exposes current authorized versions and a separate captured-text search mode;
  opening a result navigates to its source and records a navigation intent.
  Snapshot completions refresh the open field, so live withdrawal removes its
  results without typing. Corrupt selection sidecars fail closed. This does not
  close Keep promotion, bin custody, offline replay, or headed acceptance. See
  the [capture plan progress](2026-08-28_page_capture_plan.md#progress) for the
  implementation and validation receipt.

- **2026-10-17, research workflow planning:** audited current `428deb5` and
  locked Mere `a17e5bb9`; added the first complete private saved-source
  collect/find/read/ask/verify/return slice, current missing-join inventory,
  phased gates and explicit authored-note/sharing follow-ons. At that planning
  cut R0-R4 were proposed, not implemented or newly tested. Existing L1-L7 remain unchanged.

- **2026-10-17, persistence critique reconciled:** verified the Muniment batch
  contract mismatch, primitive unchecked blob reads and existing typed Fleece
  verification against both locked `a17e5bb9` and current Mere. Added R0 for
  correlated save acknowledgement, fault/retry/reopen controls and explicit
  durability claims. This does not claim a storage fix or execution receipt.

- **2026-10-17, R0 implemented and bounded qualification passed:** correlated
  request/receipt, required-write acknowledgement, checked replay and current
  opening-bound recovery passed the 940-test scoped run, including 50 capture
  and 16 scene controls. Three capture-library controls and the Clippy
  correctness gate also pass. Published Mere `00e444286` fixes canonical Sets
  tags at the supplier. The receipt above retains the broader failed runs and
  durability/platform limits. R1-R4 and headed research-workflow acceptance
  remain separate gates.
