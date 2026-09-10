# Page lifecycle plan

**Status:** plan, 2026-09-06. Rulings L1-L7 were made with Mark on 2026-09-06
from a working-tree assessment; no code written. This plan joins rules that
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

## Findings

Verified 2026-09-06 against the working tree.

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
  `AllEffective`. This choice currently survives worker resync only. Durable
  cold-reopen restoration and a headed selection surface remain open.
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
