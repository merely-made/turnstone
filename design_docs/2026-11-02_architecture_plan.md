# Turnstone architecture: open work

**Date:** 2026-11-02
**Status (2026-11-02):** cut 2026-11-02 from [the architecture plan](archive_docs/2026-11-02/2026-07-10_turnstone_architecture_plan.md), archived whole with its 1,315 lines of rung history, progress log and inline run evidence. Rungs 1 to 7, the meerkat funeral, the toolkit migration, the resident answer card (I5), embedding recall, Alembic workshop receiving, source and service boundaries and native pending-frame adoption are landed or qualified in the archived plan (macOS or Metal where it says so). This plan also takes the open points of the archived rung 5 panes plan, place port plan, q-pc handoff, genet_probe plan and meerkat harvest.

## Rulings in force

The archived plan has no numbered rulings; its rulings are prose and stay in force, cited by section and archived line: doctrine 0 to 4 (L25-64), the spine rules and the gesture law (L78-94), the recorded decisions and their triggers (L96-136), the pane model is `frisket`, not platen (L278-292, L840-856), the meerkat funeral (L520-534), cambium adopted for panes and chrome (L563-578), peer-web rung 8 and its revisions (L375-398), Piccolo as the control lane (L173-196), private source protection before further workshop UI (Mark, 2026-10-28, L1010-1012), Alembic local scope (L1028-1044), the 600-line source ceiling (L1146-1153) and the native frame rules (L1286-1294). The rung 5, place port and genet_probe plans' own rulings stay in their archived files and are grandfathered. DOC_POLICY section 11 applies forward from 2026-11-02; new decisions are keyed `architecture D<n>`.

## Decisions

### architecture D1: receipts leave the repository (2026-11-02)

Mark, 2026-11-02: "Move all, fixture to tests/". The receipts this plan cites (resident inference I5, embedding recall, the scenario receipts of rungs 3 to 7) live in `Code/testing/turnstone/receipts/` and are cited as paths; `g3_turnstone_endpoint.html` became the test fixture `tests/fixtures/g3_turnstone_endpoint.html`.

Links: area receipts · applies DOC_POLICY section 8 · summary receipts moved to Code/testing/turnstone

### architecture D2: cut the plan, drafts first (2026-11-02)

Mark, 2026-11-02: "Everything, drafts first". The plan is cut under DOC_POLICY section 8 and archived whole after this draft is reviewed.

Links: area documentation · replaces 2026-07-10_turnstone_architecture_plan · summary plan cut to open work

### architecture D3: hashes (2026-11-02)

Mark, 2026-11-02: "Live docs: local + sibling". This plan cites main hashes; the archived plan keeps its own. Mapping: [the receipts file](archive_docs/receipts/2026-07-10_turnstone_architecture_plan_receipts.md).

Links: area documentation · summary live docs cite main hashes

### architecture D4: open points of archived docs come here by default (2026-11-02)

Mark, 2026-11-02: "Everything, drafts first" (extracted open items go to the most fitting successor, defaulting to this one). Carried here: the place port plan's excluded items (O8), the rung 5 plan's open questions and watch items (O9), the q-pc handoff's item 3 (O10), the genet_probe follow-ons (O11) and the meerkat harvest backlog (O12).

Links: area documentation · draws on 2026-07-28_turnstone_place_port_plan · draws on 2026-07-14_turnstone_rung5_panes_plan · draws on 2026-10-14_qpc_lane_handoff · draws on 2026-07-17_genet_probe_automatability_plan · draws on 2026-07-18_meerkat_harvest · summary extracted items

### architecture D5: genet_probe is handed to genet (2026-11-02)

Mark, 2026-11-02: "Fix status, archive here". The plan's slices 1 to 5 are marked landed (taproot rename, turnstone `6ec391f4`) and the plan is archived here; its four follow-ons are the note in O11.

Links: area documentation · draws on 2026-07-17_genet_probe_automatability_plan · summary status fixed, archived here, follow-ons handed to genet

## Open work

### O1. Rung 4 remainder: re-verify, then gate or drop

The archived plan still lists as not done the scripted and smolweb lanes, a scrying engine, non-focused nodes truly Live and typing into a page (L269-277, L428-430). The tree has moved past parts of that text: smolweb is compiled in (`Cargo.toml:192`, `mere-lanes` with `smolweb`), `genet-documents` has the `livery` feature (`Cargo.toml:187`) and `scrying-engine` is an optional dependency behind `scry` (`Cargo.toml:378`). No done-condition is stated.

**Done when:** each rung 4 claim is re-checked against the tree, and what remains is given its own dated gate in the browser plans or dropped.

### O2. Standing follow-ons with a recorded trigger

Action envelopes and ingress authorization (trigger: a non-local action source or multi-window, L117-120; multi-window landed, L312); async boot and shutdown (trigger: persistence goes async, L127-130); crate promotion (headless automation, a second shell, another app on a port, feature isolation, L140-169); gesture-end events in `observe` (L784-785); converting frame-counting receipts to `wait`/`settle` (L478-479); a kill-mid-save receipt (L509); pushing the `TreeUpdate` to an OS accessibility adapter, now `A11yCapability::Partial` (L494-495; `src/a11y.rs`); the tear-out fork arm, named and not built (L307-309); Piccolo running the scenario grammar, "worth leaving unbuilt until" a consumer appears (L480-484) and the Piccolo surfaces beyond the first slice, owned by Mere's participant-gate-packs plan (L193-196, L862-872). None states a done-condition.

**Done when:** each item is either pulled by a named consumer, with its own done-condition, or dropped; Mark names which Piccolo surfaces he wants.

### O3. Alembic workshop residuals

The archived plan qualifies bounded local private receiving (paused scene) and names what is open (L1011-1012, L1049-1050, L1061-1104, L1132-1133): the complete W3 per-host failure matrix; production custody; playing-scene responsiveness; journal I/O and snapshot replacement on the UI thread (a canonical owner queue or transaction gate is needed, because detached async saves could overwrite intervening edits); the bounded UI-thread acknowledgement wait at place identity retirement; production resident, MCP/A2A, remote-peer and fleet principals; key retention and rewrapping; migration of legacy LocalOnly captures; and the plaintext `TURNSTONE_SCENARIO` saved-source exports, which sit outside storage protection and must be avoided by private qualification. The first private slice admits only complete retained text up to 2,048 bytes.

**Done when:** the Alembic done-condition (L1045-1048) is met for the playing scene and the W3 matrix, with the other items each given a gate or dropped.

### O4. Interactive performance

The release dry run "remained visibly slow" and its cause is not established (L1140-1142, L1246-1247); the open Arrange fixture still spends substantial CPU outside the GPU completion waits, with a native sample pointing at retained accessibility geometry projection (`RetainedLayout::document_projection_at`, `painted_rect`, `snapshot_node_rect`, `accumulated_translation`) and repeated DOM walks (L1282-1294). Qualification is bounded Metal only (L1260). The cue and measurements live in [the retained projection plan](2026-10-30_retained_projection_plan.md).

**Done when:** the paired throughput comparison the archived plan names (presentation cadence, L1275) shows the open Arrange pane and a live document tile within a budget Mark names, on the platforms Mark names.

### O5. Residuals of the resident answer card, embedding recall and R3/R4

Installed Djinn binary and a physical second-device run (L930-931); physical assistive activation, broader responsive layouts, authored-note and voice recall, second-device acceptance (L967-969); the platform, custody, note and sharing boundaries of R3 and R4, qualified on macOS only (L887-889, L905-906), which live in [the page lifecycle plan](2026-11-02_page_lifecycle_plan.md); the short-word citation limit, in [the retained clip receiver plan](2026-11-02_retained_clip_receiver_plan.md).

**Done when:** each is named as a gate in the plan that owns it, or Mark drops it.

### O6. Toolkit and module-map staleness

The archived module map (L138-163) is stale: `settings`, `surface`, `pane` and `place` exist (`src/settings_pane.rs`, `src/surface.rs`, `src/pane.rs`, `src/place.rs`) and there is no `src/shell.rs` (the shell is `src/shell/mod.rs` and subdirectories); `comms` is still absent. The toolkit watch items (the Cambium copies in Genet, L614-622; `livery`, L623-628) predate the tree: `genet-livery` is in `Cargo.toml:187`, `:223`; a catalog caret-rendering field and a Genet `SubtreeView` style-sharing bug are the noted follow-ons (L584-591).

**Done when:** a current module map is generated from `src/` and cited from the README, and the toolkit watch items are closed or re-stated against the tree.

### O7. Chronology and matrix debt

The deletion-matrix rows for panes (L431-439) and the closing row (L518) carry no Met/open status, and the progress log is out of date order (L787-790). This cut retires both with the archived file; the open part is the row "same scenario through keys and automation", partly met (L461-484).

**Done when:** the rows are stated once, in this plan's Findings, as met or open.

### O8. Items carried from the archived place port plan

Automatic discovery and WAN reachability, current document projection and editing, and research-source and note attachment transfer remain separate from the macOS baseline of 2026-10-19 (archived place port plan, last section). Named-recipient publication has its gate in [the page lifecycle plan](2026-11-02_page_lifecycle_plan.md).

**Done when:** each is given a gate and an owner in this plan or the page lifecycle plan, or dropped.

### O9. Open questions and watch items of the archived rung 5 plan

The rung 5 plan's questions 2 to 7 and two watch items (archived rung 5 plan, "Open questions" and "Watch items") were written in July. Evidence found 2026-11-02: questions 2 and 3 (workbench renderer; view framework) are answered by the cambium adoption (L563-578); question 6 (Steward) is mooted because Steward was retired in the September 29 consolidation; question 7 (file ingress) has its deletion-matrix row Met on 2026-07-18 (L510); question 4 (whether `workbench` moves with platen) reads as answered by the funeral ruling that platen and workbench are the composition domain model (L520-534); question 5 (which settings are app truth, ownership) is not shown settled (`src/settings_provider.rs` and `src/settings_pane*.rs` exist). Watch item 1 (the Cambium duplication in Genet) and watch item 2 (`livery`) are O6.

**Done when:** Mark confirms question 4 and rules question 5, or drops them.

### O10. Known UI issues from the q-pc handoff

Floating-toolbar occlusion and confirmation-caption overflow, "recorded, not fixed" in the last visual review (archived q-pc handoff, item 3).

**Done when:** each is reproduced on current main, fixed with a headed capture, or dropped.

### O11. genet_probe follow-ons, handed to genet

taproot's four follow-ons (Mark, 2026-11-02: "Fix status, archive here"): contributed-pane geometry and identity, a manual assistive-technology receipt for registered surfaces, retiring `scenario.rs`'s loop into `Driveable::app_step`, and a second consumer (isometry) (archived genet_probe plan, after slice 5). Genet owns the crate (`genet/components/taproot`).

**Done when:** Genet's plans carry them, or Mark drops them; nothing remains here after that.

### O12. Meerkat harvest "Noted" backlog

Capabilities the donor shipped that Turnstone does not yet, each with its named home (archived meerkat harvest receipts file, section "Noted"). Donor files are in Mere's history: the focus card (cambium primitive only), command registry vocabulary (palette), settings page catalog (checklist only; settings-as-nodes is ruled a bust), web clip and in-page find (content lane backlog), theming lane (rung 8, genet livery engine), export profiles (rung 8), wallet pairing UX (comms rung), constellation actor pool (Gloss status controls), graph delta log (Mere journal), gnode pool and partitioned raster (a read if canvas node-body performance bites), knot completion, infer host, ingest and content affinity (rung 8 intel port), idle forgetting and snapshot refresh (alembic by-session eviction) and IME edge cases (diff-read if IME bugs surface).

**Done when:** an item is pulled by a consumer with its own done-condition, or Mark drops it; none is scheduled by this cut.

## Findings

- 2026-11-02: the archived plan's text still says frisket lives at turnstone's `crates/frisket` (L525-526, L854); it is `mere/crates/cambium/cambium/src/frisket.rs` (`Cargo.toml:151`, "`frisket` went to `cambium::frisket`").
- 2026-11-02: rung 8 has code (`src/place.rs`, `src/place/`, `src/place_service.rs`) though the module map says `place` is absent.
- 2026-11-02: source size is enforced in CI (`scripts/check_source_size.py:8`, `LIMIT = 600`; `.github/workflows/boundaries.yml:11`); `Cargo.toml:347` has `default = []`.
- 2026-11-02: the q-pc handoff's item 4 is answered by the archived resource handoff (done-conditions applied; its three-lane tail is in the unusual protocols plan).
- 2026-11-11: on Windows, session saving fails with `Access is denied (os error 5)`. The fork in the `overmap` scenario never publishes (`fork persistence failed before manifest publication`, `session persistence refused or failed`), so the overmap shows one session; and `turnstone --lib` has 48 failing names (5 `session::tests`, 43 in session persistence, source-capture persistence, trail memory and research records, all failing on a file write or replace). Identical on the pre-split build (Turnstone `ea971f17`; the base `fd57dd3f` fails the same 48 names). Evidence: `Code/testing/turnstone/2026-11-11_engine_split_headed/overmap/` and `Code/testing/turnstone/2026-11-11_engine_split_headed/presplit-ea971f17/overmap/`; the 48 names in `Code/testing/engine-split/slice5_app.md` (table row for `turnstone --lib`); headed report `Code/testing/engine-split/slice6_u6_headed.md`, section 3, item 1.
- 2026-11-11: the Roster pane runs off the right window edge (its Address column is cut off) when Trail and Roster are both open. Identical on the pre-split build (Turnstone `ea971f17`). Evidence: `Code/testing/turnstone/2026-11-11_engine_split_headed/u6_trail_roster_offline/02_roster_pane.png` and `Code/testing/turnstone/2026-11-11_engine_split_headed/presplit-ea971f17/u6_trail_roster_offline/`; headed report section 3, item 2.
- 2026-11-11: Pelt's `--workspace-receipt reader` receipt fails on both the post-split and the pre-split build (Turnstone `ea971f17`); the `gemtext` receipt needs the `smolweb` feature, which the default build lacks. Evidence: `Code/testing/graphshell/2026-11-11_engine_split_headed/pelt/` and `.../pelt-presplit-ea971f17/`; headed report section 3, item 4 and the surface table row P.
- 2026-11-11: the README canvas screenshots are stale: the background (17,20,26) is now (9,12,26), tile frames (219,128,230) are now (133,84,140), and beta's face changed. The pre-split build (Turnstone `ea971f17`) already draws the new colours, so the headed report attributes it most likely to Graphshell's iconography commits of 2026-11-09, not the split. Evidence: `Code/testing/turnstone/2026-11-11_engine_split_headed/readme_screenshots/` and `Code/testing/turnstone/2026-11-11_engine_split_headed/presplit-ea971f17/readme_screenshots/`; headed report section 3, item 5.

## Progress

- 2026-11-02: plan cut from the architecture plan; decisions D1 to D5 recorded; open points of five archived documents carried as O8 to O12. The evidence index and old-to-main hash table are in the receipts file.
