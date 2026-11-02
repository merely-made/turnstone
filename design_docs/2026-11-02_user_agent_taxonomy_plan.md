# User-agent taxonomy: open work

**Date:** 2026-11-02
**Status (2026-11-02):** cut 2026-11-02 from [the user-agent taxonomy plan](archive_docs/2026-11-02/2026-08-03_user_agent_taxonomy_plan.md), archived whole with its August inventory and October browser appendix. Landed in the archived plan: U0 downloads (2026-08-21), U1 find in page (accepted 2026-08-26), U3 reader lane, B0 engine inventory (qualified 2026-10-05), B1 Windows Scry and B2 direct Weld (bounded slices qualified 2026-10-06). Open: U2 view source, U4 bookmark taxonomy, B3 Servo/Graft, B4 browser operations and the residual gates below.

## Rulings in force

Cited in the archived plan, not re-decided: the Servo profile ruling (Mark, 2026-10-06: an explicitly named, configurable Servo profile shared by its views in the process; Servo only, Scry and Weld keep per-node profiles; L399-406), Mark's 2026-10-05 direction to orchestrate the taxonomy around the wgpu trio, which "is an application consumer pass" and authorizes neither publication nor a new universal browser runtime (L167-171), the authorization of the coordinated graph-semantics consumer patch (L458-460) and U14 of the unusual-protocols plan. The archived plan numbers its steps U0 to U5 and B0 to B4; those are steps, not rulings, and stay grandfathered. DOC_POLICY section 11 applies forward from 2026-11-02; new decisions are keyed `user_agent_taxonomy D<n>`.

## Decisions

### user_agent_taxonomy D1: receipts leave the repository (2026-11-02)

Mark, 2026-11-02: "Move all, fixture to tests/". The B0 to B3 receipts (`browser_engine_inventory_20261005`, `browser_scry_windows_20261005/6`, `browser_main_integration_20261006`, `browser_supplier_integration_20261006`, `graph_semantics_p2_20261005`, `browser_a11y_20261007`) are in `Code/testing/turnstone/receipts/`, cited as paths.

Links: area receipts · applies DOC_POLICY section 8 · summary receipts moved to Code/testing/turnstone

### user_agent_taxonomy D2: cut the plan, drafts first (2026-11-02)

Mark, 2026-11-02: "Everything, drafts first". The plan is cut under DOC_POLICY section 8.

Links: area documentation · replaces 2026-08-03_user_agent_taxonomy_plan · summary plan cut to open work

### user_agent_taxonomy D3: hashes (2026-11-02)

Mark, 2026-11-02: "Live docs: local + sibling". The archived plan's 2026-10-05 pins (Turnstone `74a4689`, Mere `7e0cf33e22`, Genet `e86fa4feb3`) are historical and not carried; the root manifest now tracks `branch = "main"` rows. Hashes with no equivalent on main (`1d44e5dd`, `f68af0dc`) or several (`db4ee312`) stay as text. Mapping: [the receipts file](archive_docs/receipts/2026-08-03_user_agent_taxonomy_plan_receipts.md).

Links: area documentation · summary live docs cite main hashes

### user_agent_taxonomy D4: the q-pc handoff's item 6 lands here (2026-11-02)

Mark, 2026-11-02: "Everything, drafts first". Item 6 of the archived q-pc handoff (a general source-view consumer and named collections, U2 and U4) is O1 and O2; its clip-system contracts are in [the retained clip receiver plan](2026-11-02_retained_clip_receiver_plan.md).

Links: area documentation · draws on 2026-10-14_qpc_lane_handoff · summary extracted item

## Open work

### O1. U2: view source

A source view as a content class over already-held bytes. Zoom is accepted on Livery and Windows Weld (2026-08-27; Weld effective readback stays Partial, L113-115); source-document capture stores response bytes (`src/source_capture.rs:7`, `StoredSourceDocument` at `:77`) but nothing consumes them: a case-insensitive search of `src/` and `scenarios/` for view source finds nothing, and `src/content_classes.rs:17-19` registers only web-page, note and download.

**Done when:** "view source opens for any fetched node" (L121-122), with the capability report preceding the control so unsupported engines stay honest.

### O2. U4: bookmark taxonomy

A named collection class and membership UX over existing nodes; keep/discard stays the athanor's (L135-139). No collection or bookmark class exists (`src/content_classes.rs:17-19`; the `collection` hits in `src/place/captured_collection.rs` are a different concept). The Fleece/Eidetic/Muniment adoption does not close it (L77-82).

**Done when:** a user can curate a named collection and it is a pack-expressible structure, not a privileged store (L138-139).

### O3. B1 residual gates (Windows Scry)

Typed find and zoom forwarding, permission and authentication answers, correlated capture and PDF, DevTools opening and accessibility projection are unavailable through the pinned adapter (L441-443); physical keyboard, supplementary Unicode, OS IME, sandbox bootstrap, lenses, native rehosting and different-size appearances of one node are separate gates (L271-275, L506-508). Forwarding is a Mere adapter gate (L379-383).

**Done when:** each gate is stated as a done-condition against the adapter's current release, or dropped by Mark.

### O4. B2 residual gates (direct Weld)

Accepted script and cookie completions must not be discarded into diagnostic strings when exposed (L283-285); capture, popups and browser-owned downloads each get a separate gate (L289-290).

**Done when:** each has a done-condition and a receipt, or is dropped.

### O5. B3: Servo/Graft consumer

Source is implemented and the locked three-engine build passes; the full native gate is pending (L17). Open: the intermittent white reopened Servo A (a synchronization diagnostic reproduced it in 1 of 2 controls and is "not a proved production fix", L295-312); foreign-tree accessibility (Servo 0.7 forwards only Click, no focus, text or scroll; action masks are never advertised; pinned Servo exports no DOM focus; no Windows TextPattern on document ancestors; the action router ignores `target_tree`, L596-628); the fontsan exactly-one decoder guard against Servo's `wuff` (L559-564, L585-586); and Servo 0.7 versus CEF 151 currency (L569-576).

**Done when:** the B3 done-condition holds: an upstream Servo page renders in the current device, takes input, reports supported controls, survives resize and teardown, and its accessibility subtree and actions reach UIA, NSAccessibility and AT-SPI with receipts (L314-321). The same gates are listed in [the browser surface plan](2026-11-02_browser_surface_implementation_plan.md).

### O6. B4: browser operations lane

Browsing-context lifecycle, popups and external schemes, profiles and site state, transfer dialogs and custody, inspection and capture, in that order; not started, no progress entry (L323-331).

**Done when:** every exposed action has an exact target, a capability-backed offer, a terminal observable result and the specified restart and privacy behavior; absent backend functions stay attributable and visible (L329-331).

### O7. Graph-semantics P2 consumer patch

Prepared and authorized but unapplied (appended Replay variants, an exhaustive `behaviors.rs` match, six regressions); "compatible dependency and main integration review remain open" (L452-470, L520, L586-587).

**Done when:** the patch is applied or retired against current Mere, and its six regressions run.

### O8. Product forks Mark owns

(i) Selectable shared profiles and cross-engine state transfer "need an explicit product decision" (L392-394); (ii) the relationship between document navigation inside a live surface, Trail traversal and popup admission before default behavior changes (L366-370, L394-396); (iii) cookie and storage inspection is a separate consumer gate (L153-154).

**Done when:** Mark rules each; each ruling becomes a decision here.

### O9. Release qualification

The authoritative release plan is `wgpu-graft/design_docs/2026-11-01_triplet_release_plan.md`; Turnstone is an additional consumer and publication needs its own authorization (L335-341). Scry's Intel capture cadence and native Mac cadence still fail, NVIDIA jobs are queued (L472-476, L503-508). A parallel network sync-round failure passes in isolation and its suite "is not reported as clean" (L496-497).

**Done when:** the release plan's hosts pass and a registry-only consumer proof exists.

## Findings

- 2026-11-02: hosted back and forward call the owning `WebSurface` history controls, not Trail (L366-370); that is fork O8(ii).
- 2026-11-02: support is recorded at four levels (library API, adapter forwarding, Turnstone consumption, executed consumer evidence); a library capability is not a Turnstone capability (L211-215).
- 2026-11-02: frame counters do not close presentation; pixel review found stale images and blank reopened tiles despite positive counts (L447-450).
- 2026-11-02: `mere-surface-api` (settings and discovery vocabulary) is a different seam from the browser control protocol in Inker's `surface_engine.rs` (L184-188).
- 2026-11-02: resource changes must fan out to current showing surfaces; historical migration effects keep explicit resource endpoints (L468-471).

## Progress

- 2026-11-02: plan cut from the user-agent taxonomy plan; decisions D1 to D4 recorded. The evidence index and old-to-main hash table are in the receipts file.
