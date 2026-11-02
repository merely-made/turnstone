# Page lifecycle: open work

**Date:** 2026-11-02
**Status (2026-11-02):** cut 2026-11-02 from [the page lifecycle plan](archive_docs/2026-11-02/2026-09-06_page_lifecycle_plan.md), archived whole with its 1,279 lines of research-workflow history and inline run evidence. Landed in the archived plan: L-P1 (capture admits Keep, through R1), the research workflow R0 to R4 (R0 qualified, R1 to R4 bounded macOS qualification, 2026-10-17 to 2026-10-18), the authored research note join (macOS, 2026-10-18) and the place sharing address and chat baseline (macOS, 2026-10-19). Open: L-P2 to L-P6 and the named-recipient sharing gate. The archived plan's status line is "in progress, refreshed 2026-10-19"; its Progress log stops at 2026-10-18 and records neither the authored-note join nor the 2026-10-19 sharing work, which are in its body only.

## Rulings in force

L1 to L7 (Mark, 2026-09-06) stay in force and are cited: L1 history remembers where you went, never what you saw (L71-84); L2 capture on an unkept page is a Keep (L86-99); L3 Keep retains the node only (L101-112); L4 captures ride the tombstone and athanor's retirement pass reaps unreferenced blobs (L114-132); L5 only a new capture with a different content hash asserts change (L133-148); L6 an annotation is a Knot block in a facet on the page node (L150-173); L7 shared means address and title only, attachments are shared per item through place authority and redacted from exports unless chosen (L175-191). The unnumbered ownership rulings of the research workflow (L355-403) and the named-recipient route (one recipient, one chosen document, individually chosen saved-source attachments, L986-990) also stand. Old ids are grandfathered. DOC_POLICY section 11 applies forward from 2026-11-02; new decisions are keyed `page_lifecycle D<n>`.

## Decisions

### page_lifecycle D1: receipts leave the repository (2026-11-02)

Mark, 2026-11-02: "Move all, fixture to tests/". The receipts (`r0_source_save_20261017`, `r1_source_keep_20261017`, `r2_saved_sources_20261017`, `r3_scoped_answers_20261017`, `r4_research_workflow_20261018`, `research_notes_20261018`, `place_sharing_20261019`, `document_formats_20261019`) are in `Code/testing/turnstone/receipts/`, cited as paths. Eight distinct folders.

Links: area receipts · applies DOC_POLICY section 8 · summary receipts moved to Code/testing/turnstone

### page_lifecycle D2: cut the plan, drafts first (2026-11-02)

Mark, 2026-11-02: "Everything, drafts first". The plan is cut under DOC_POLICY section 8.

Links: area documentation · replaces 2026-09-06_page_lifecycle_plan · summary plan cut to open work

### page_lifecycle D3: hashes (2026-11-02)

Mark, 2026-11-02: "Live docs: local + sibling". Mapping: [the receipts file](archive_docs/receipts/2026-09-06_page_lifecycle_plan_receipts.md).

Links: area documentation · summary live docs cite main hashes

### page_lifecycle D4: the place port's named-recipient item lands here (2026-11-02)

Mark, 2026-11-02: "Everything, drafts first". The archived place port plan leaves named-recipient document publishing on its source-owner gate; that gate is O1.

Links: area documentation · draws on 2026-07-28_turnstone_place_port_plan · summary extracted item

## Open work

### O1. Named-recipient selected publication (the 2026-10-19 gate)

One named recipient, one chosen document, individually chosen saved-source attachments. Blocked at the source-owner handoff (L984-985, L1009-1018): the hosted directory constructor and the persona-vault mode return no `KnotPublishSource` (`src/knot_authoring/knot_authoring_engine_from_env.rs:256`, `publish_source: None` in most constructors); only the startup-unlocked personal vault supplies one, and reopening the live resident's store in Turnstone would break its single-owner contract. `src/publish_service.rs` starts with an empty catalog and keeps tickets and revocations in memory; `src/share_reader_service.rs` refreshes only on a new Open (L1028-1032). Knot's `KnotFileCapturePort` is a retention seam, not publication authority (L1019-1027).

**Done when:** the five points of the smallest complete owner continuation hold (L1036-1063): (1) a Knot-owned selected-publication handoff over the existing signed source and publisher, binding document revision and bytes, destination persona and endpoint, recipient key, expiry and each chosen source's immutable identity; (2) a complete disclosure inventory before issue, saying full saved text or passage per attachment; (3) selection and signed withdrawal persisted through the owner before acknowledgement and restored after holder restart, retry unable to widen or resurrect; (4) recipient reads revalidated, body cleared on refusal, fail closed after cold reopen, first route read-only; (5) a receipt with independent holder and recipient processes, origin stopped, exited stores inspected and the receipt bound to committed inputs.

### O2. L-P5 and deliberate attachment sharing

Engram export of a node with captures and a note carries neither unless chosen and states what it redacted; a chosen attachment goes through place authority as its own durable operation; capture envelopes and annotation facets join `RedactionPolicy` as default-off fields in Mere, not a Turnstone-side filter (L265-278, L968-973, L1157-1160). `RedactionPolicy` and `graph_codicil` do not appear in `src/`. A private capture list is not a shared collection.

**Done when:** the L-P5 done-condition (L265-278) holds with reader refusal and withdrawal qualified.

### O3. L-P2 and L-P3: envelopes ride the tombstone, blobs are reaped

L-P2: delete stages every envelope in the record, recovery restores them on the same identity, and the Trail Removed row shows the count without opening bytes (L215-227). `RemovedRecord` (`src/action.rs:97-114`) has no capture field. L-P3: athanor's retirement pass reaps blobs no living envelope, surviving tombstone envelope or download facet names; the reference set is asserted non-empty when any exist; emptying the bin reaps the same way (L229-247). `retire_then_list` (`src/recycle.rs:119`) purges only archived nested worlds. The scheduling actor is Distillery Athanor, "a reservation stub since 2026-09-02 with no lane opened" (L234-237). Until both land, deletion conservatively preserves evidence and adds no irreversible "Forget" promise (L974-980); the engram bake is open in [the recycle bin plan](2026-07-20_recycle_bin_athanor.md).

**Done when:** L215-247 holds, including the positive control and the shared-blob survival case.

### O4. L-P4 annotation facet and promote; L-P6 the series in the Inspector

L-P4: a Knot block in a facet on the page node renders on the note route, promotes to a `knot://` node with a typed relation, rides the tombstone, and a clip from a capture cites content hash and selector (L249-263); no annotation-facet code found (`capture.source-annotations/v1` at `src/source_capture.rs:250` is the Fleece manifest facet). L-P6: the Inspector lists captures by completion time with scope, engine and hash without opening bytes, and drift reads as a comparison of two named captures (L280-291); R2 "may close the relevant source-document series gates only after their own assertions pass" and the plan never says it did; `src/trail_memory/research_compare_tests.rs:8` exists, Inspector rendering is unverified. Hosted screenshot and PDF series are not qualified.

**Done when:** L249-263 and L280-291 hold with headed receipts.

### O5. Headed receipts for the L-P phases

A restart receipt for L1 (an unkept visited page leaves only a trail trace), a headed scenario that visits, captures, annotates, deletes, recovers and empties the bin, and a check that the recall evaluation corpus is unaffected by L1 (L293-300). None is cited as run.

**Done when:** each is run and receipted, or Mark drops it.

### O6. Platforms, custody and durability

R0 to R4 and the note join are macOS-bounded. Open: Windows and Linux, OS assistive activation and human assistive technology, physical device recovery, power-loss durability (no `sync_all` barrier is exposed; durability claims stop at acknowledgement plus process-restart survival, L438-443, L516-518), personal custody installation and the installed-resident lifecycle (L781-782, L845), receipt and artifact retention roots (L487-488), place, Sky-surface and Sky-timeline suites excluded from the R0 to R3 counts and Mere's wall-clock speed-budget control failing in R0 (L538-540, L560-563).

**Done when:** each is named as a gate with a done-condition or Mark drops it.

### O7. Retention, corpus and hosted capture

Automatic Full retention and disposal of legacy `PageText` need their own reconciliation; the plan does not declare Full shipped (L380-382, L975); arbitrary large-corpus publication waits on the resident plan's resource-backed handoff, and the 128 KiB corpus ceiling stays with a visible refusal (L398-403, L976); hosted and viewport capture and PDF show an unavailable reason and hosted Weld exposes no source bytes (L347-350, L975-976, L1181-1183); offline interactive-page replay and a new project type are excluded by design (L976-977); captured-search (Gemot collection) headed acceptance and offline snapshot replay "remain open" (L1140-1142).

**Done when:** each has a done-condition in the plan that owns it (page capture, resident), or Mark drops it.

### O8. Questions for Mark

The label for capture on an unkept node is unruled (L1167-1168; R1 shipped "Save source and keep", `src/app/palette/app_lower_denizen_actions.rs:203`); whether the excluded suites in O6 are acceptable; whether to publish the named-recipient route through the persona vault or the resident (L1012-1018).

**Done when:** Mark answers each; each answer becomes a decision here.

## Findings

- 2026-11-02: code cites in the archived plan have drifted: `keep_node` is `src/app/node_arms/app_browser_command_target.rs:85` (plan: `src/app/node_arms.rs:93,123`); `RemovedRecord` is `src/action.rs:97` (plan: `:1092`).
- 2026-11-02: the capture reference facet id is `capture.source-annotations/v1` (`src/source_capture.rs:250`); the research Answers owner is `turnstone/<session>/research/v1` (`src/research.rs:414`).
- 2026-11-02: wire `scope: []` means all sources in the inference service; the capture-backed UI must never emit it (L390-391).
- 2026-11-02: Muniment `Backend::apply` is not all-or-none and Fjall lacks `transact`; do not rely on atomic apply (L435-443).
- 2026-11-02: `RecordText` is gated, not removed (`src/trail_memory/keep_page_text.rs:7`, `TrailCommand::RecordText` at `src/trail_memory/worker.rs:406`), consistent with "refuses new writes" (L621-622).
- 2026-11-02: the saved-source address `mere://saved-source/...` is a persisted view intent, not an origin request (L668-671).

## Progress

- 2026-11-02: plan cut from the page lifecycle plan; decisions D1 to D4 recorded. The evidence index and old-to-main hash table are in the receipts file.
