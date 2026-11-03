# Turnstone design documents

Supplier cutover (2026-11-03): Turnstone uses the extracted Distillery/ESP
supplier and Dramatis custody clients. Door contracts come directly from
Armillary's Ostiary; graph retention calls `pandect::furnace`. The canonical
scope and exact consumer receipts are in
`mere/design_docs/mere_docs/implementation_strategy/2026-10-31_distillery_extraction_plan.md`.
Locked native compilation qualifies this intake; previous headed and physical
receipts retain their original source cohorts.

Model-free retained research (2026-11-01): M1–M5, including bounded storage
reads, are complete for the recorded Linux cohort (Turnstone `554f323f`).
The canonical plan and receipt are in
`eidetic/design_docs/eidetic_docs/implementation_strategy/2026-11-01_memory_gates_plan.md`.
Current-source follow-ups for restored reader recovery and clipped research text
are tracked in [the recovery plan](2026-11-01_retained_reader_recovery_plan.md).
Later source tests do not extend the original native qualification.

Rust policy (2026-10-13): working toolchain 1.99.1 and shared MSRV 1.97.1, qualified separately. Canonical policy: `mere/design_docs/2026-08-12_family_composition_thesis_brief.md`, section "Shared Rust toolchain and minimum version".

Murm source adoption (2026-10-13) is tracked in
`murm/design_docs/archive_docs/2026-11-02/2026-10-13_murm_family_move_plan.md`. Active consumer roots
use the imported transport supplier and coherent Mere/Dramatis/p2panda
composition patches. Compile and software-test receipts are scoped to this
cutover; previous physical and headed receipts retain their original inputs.

This is the canonical index for active Turnstone documentation. A document's
own dated status still matters, but this index decides what is in the active
set.

## Working principles for AI assistants

- Terminology (2026-09-20): use **participant** for platform admission and
  **servitor** for resident helpers. **Denizen** belongs to Isometry's simulation
  vocabulary, per `mere/design_docs/TERMINOLOGY.md`. Existing `Denizen*` APIs,
  saved paths, signed scopes, facet names, action IDs, and event IDs keep their
  compatibility spellings.

- Read this index and [the documentation policy](DOC_POLICY.md) before changing
  code or plans.
- Verify the live tree, current branch, dependencies, and dirty paths before
  treating status prose as authority. Current code and an explicit active plan
  outrank stale summaries; reconcile the prose in the same pass.
- Preserve unrelated work. Inspect staged paths immediately before committing,
  and commit only the paths owned by the current slice.
- Work directly on `main` when there is one worker and no collision. Avoid
  branches, pull requests, and worktrees unless isolation is needed; merge and
  prune temporary lanes when it is no longer needed.
- Keep authority boundaries sharp: Turnstone owns product actions and policy,
  Mere owns reusable graph/projection primitives, and Genet engines own
  document behavior. Promote a shared contract when a real consumer forces it.
- Name the actual document format in user-facing choices (Djot, Markdown,
  Gemtext, and so on); use document when no format has been selected. Preserve
  provider, route, schema, action and compatibility-format identities.
- Use one action catalog and one observable state spine. UI, automation, and
  accessibility should consume the same named actions and facts.
- Apply graph camera edits through the owning pane's `with_graph_pane` boundary.
  Each render/input pass installs its cached viewport; a runtime-only camera
  change is lost at the next pass. Native badge edits that bypass Actions share
  the primary/lens pointer-release persistence boundary.
- Resource journal captures resolve through explicit current showing-surface
  identity before behavior ancestry scopes; resource UUIDs are not surface
  scopes, and current URLs cannot recover historical resource endpoints.
- Match proof to the claim: focused unit tests for state and identity, real
  consumer tests for wiring, restart receipts for persistence, and headed
  interaction receipts for visible UI.
- For Turnstone native verification, launch the application only with an
  explicit isolated `TURNSTONE_ROOT`; flags such as `--list` may reach the app
  entrypoint instead of selecting an inspection mode. Choose a test executable
  from Cargo's `compiler-artifact` metadata for the exact target/profile, never
  from a filesystem glob or by probing executables with `--list`.
- When restoring source snapshots, ensure the compiler sees the restored bytes
  as newer than prior outputs (or otherwise force a fresh compile) before
  claiming a repaired binary; byte-identical file copies can preserve stale
  timestamps and let Cargo reuse the prior artifact.
- Treat a modern canonical facet file as complete authority, including absent
  fields. Snapshot reconstruction defaults belong to legacy migration and must
  not silently change an owner's saved revision token.
- Auxiliary native owner windows request their own redraw after asynchronous
  state updates; the primary/lens redraw fanout does not cover them.
- Private source protection includes raw, decoded and recovery copies plus
  transient derived caches. LocalOnly is a distribution classification, not an
  encryption receipt; custody continuity needs the live binding generation.
  Legacy data retains its identity until an explicitly qualified migration.
- Record sidequests, contradictions, unusual synergies, and still-open gates in
  the active plan. Do not let a successful focused check stand in for a green
  workspace gate.

## Policy

- [Documentation Policy](DOC_POLICY.md): canonical workspace documentation
  rules plus the Turnstone addendum.
- `PROJECT_DESCRIPTION.md`: reserved for the maintainer and not present yet.
  Until it exists, the repository [README](../README.md) stands alone, as the
  local policy addendum specifies.

## Current implementation plans and product direction


- [Compare: selection sources, selection commands and shared scenes](2026-10-21_compare_selection_and_shared_scenes_plan.md):
  proposed next slice. A snapshot, live or frozen selection source in `curation`;
  Compare, Show connections, Read in sequence and Search within selection; the
  Graphshell scene format lifted to `pandect` and extended for comparisons.
  Phases 0–2 can start; phase 4 waits on four open questions.


- Resident answer card: source-owner publication, exact-version citations and
  headed I5 acceptance are tracked in
  `mere/design_docs/mere_docs/implementation_strategy/2026-10-28_resident_inference_plan.md`.
  Native I5 is qualified with the pinned GPU model and an offline-origin
  restart. The October 20 embedding continuation is qualified on macOS:
  current SDK intake, real MiniLM fused recall through headed retained sources,
  and an independent cold product/resident pair with a new ask. Its
  receipt (`Code/testing/turnstone/receipts/embedding_recall_20261020/qualification.json`)
  preserves the exact envelope/index, source/citation and private-journal evidence. Turnstone's client integration is recorded in the
  [architecture plan](archive_docs/2026-11-02/2026-07-10_turnstone_architecture_plan.md).

- [Trail recall evaluation](2026-09-02_trail_recall_evaluation_plan.md): private captured-corpus protocol for selecting or rejecting configurable phrase recall against BM25 with held-out ranking and resource budgets.
- [Page capture and provenance](2026-08-28_page_capture_plan.md): ruled path
  for durable node-attached page captures; P1 owner contracts and exact pins
  and its clean-source compile gate are landed. Source-document capture and
  authorized collection search have app consumers; hosted Weld capture,
  offline replay, and the remaining custody work are still open.
- Shared-contract Servo receiving (2026-10-23): the browser surfaces plan
  records the concrete Metal/Cue binding and same-host Genet/Servo receiver;
  Linux/Vulkan and physical platform qualification remain separate.
- Native Orrery selection folds (2026-10-16): qualified on current published
  suppliers with 958 workspace tests, native build, 379 presented create/reopen
  frames and nine reviewed readbacks. The receipt (`Code/testing/turnstone/receipts/graph_folds_20261016/README.md`)
  and pane registry plan section "Native graph folds" retain scope and inputs.
- [Turnstone engine adoption](2026-08-03_turnstone_engine_adoption_plan.md): selectable engine routing and unavailable-engine behavior.
- [Reticulum browsing](2026-08-03_reticulum_browsing_plan.md): native NomadNet page routing and Micron preview implemented; current shared presentation, typed interaction and independent carrier acceptance scopes.
- [Peer-web reframe](2026-07-28_turnstone_peer_web_reframe.md): current product direction for local-first personal and shared places.

- [Turnstone architecture: open work](2026-11-02_architecture_plan.md): draft, cut from the architecture plan. Alembic workshop residuals, interactive performance, standing follow-ons with triggers, and the open points of the archived rung 5, place port, q-pc, genet_probe and meerkat harvest documents.
- [Unusual-protocols browser: open work](2026-11-02_unusual_protocols_browser_plan.md): draft. Stages S1 to S11, SC5 to SC7, the three-lane tail, accessibility on three systems, and the Errand, native receiving and Hest residuals (Hest H1 to H3 qualified 2026-10-20).
- [User-agent taxonomy: open work](2026-11-02_user_agent_taxonomy_plan.md): draft. U2 view source, U4 bookmark taxonomy, B3 Servo/Graft, B4 browser operations and the product forks.
- [Retained clip receiver: open work](2026-11-02_retained_clip_receiver_plan.md): draft. Personal vault migration, editor input, audio, formats, P6 policy and the quality limits.
- [Pane registry, graph views and shell composition: open work](2026-11-02_pane_registry_and_graph_panes_plan.md): draft. A2 (partly built), A3, A4/A5 acceptance, A7, A8, deferred decisions and the reader CPU/FPS item.
- [Page lifecycle: open work](2026-11-02_page_lifecycle_plan.md): draft. The named-recipient gate, L-P2 to L-P6, platforms and durability.
- [Browser surfaces implementation: open work](2026-11-02_browser_surface_implementation_plan.md): draft. A0, S0 (shallows), X0, the E0 tails, default Servo correctness, accessibility and Cue platform qualification.

## Current architecture, audits, and design records

- [Redshank episode surface](2026-09-14_redshank_episode_surface_plan.md):
  Turnstone as the listening port's second host — the contributed compact dock
  as an episode tile, enclosure routing, one app-owned audio and model
  authority, and the item/progress/note graph projection. Landed; the fetch
  path closed 2026-09-22 through Mere's fetch handle and the pins are
  aligned. Embedded selected text-note retention has an October 20 headed
  source-absent cold/export receipt using a typed owner-model fixture; full
  owner authoring/menu, embedded audio and microphone gates remain separate.
- [Turnstone surfaces in Cambium](2026-07-15_turnstone_surfaces_in_cambium.md): mapping from product surfaces to Cambium components or non-Cambium lanes.
- [Turnstone founding](2026-07-08_turnstone_founding.md): naming, role swap, and original sequencing, with later authority corrections called out in the document.
- [Gloss composite pane](2026-07-20_gloss_composite_pane.md): configurable navigation and operational overview; September 29 implementation adds Downloads by default, moves object handling into Inspector, and preserves saved arrangements while retiring Steward/Apparatus panes. Automated and native restart gates pass; shared diagnostics live in `mere-apparatus`.
- [Recycle bin and Athanor](2026-07-20_recycle_bin_athanor.md): recoverable deletion, identity-preserving restore, and eventual permanent forgetting.

## Completed plans and receipts

- [Retained semantic projection](2026-10-30_retained_projection_plan.md):
  shared Genet geometry traversal, Cambium and Turnstone receiving, bounded
  accessibility admission comparisons, layout-only frame preparation, and
  completed local native performance qualification. Its Findings record the remaining Arrange publication hot spots (the publication cue); the performance follow-up is in the architecture plan's open work.

## Archived 2026-11-02

Landed, superseded, cut or historical documents were moved whole to `archive_docs/2026-11-02/`
(Mark, 2026-11-02: "Everything, drafts first"; "Archive all 8"). Each carries a dated
header naming its landing commit and where its open points went; open points live in the
2026-11-02 successor plans. Run evidence is in `archive_docs/receipts/` (see below).

- [Turnstone place port](archive_docs/2026-11-02/2026-07-28_turnstone_place_port_plan.md): shared-place composition and the two-peer acceptance path; landed, macOS baseline qualified 2026-10-19.
- [Turnstone rung 5 panes](archive_docs/2026-11-02/2026-07-14_turnstone_rung5_panes_plan.md): pane, surface-composition, focus, workbench and window foundations; slices A to F landed.
- [Browser/resource handoff, 2026-10-08](archive_docs/2026-11-02/2026-10-08_browser_resource_handoff.md): the resource-integration resume point; its done-conditions were applied 2026-10-09.
- [q-pc lane handoff, 2026-10-14](archive_docs/2026-11-02/2026-10-14_qpc_lane_handoff.md): stale lane snapshot.
- [Relationship navigation and visible pane bindings](archive_docs/2026-11-02/2026-10-21_relationship_navigation_and_pane_bindings.md): landed 2026-10-21.
- [Genet probe automatability](archive_docs/2026-11-02/2026-07-17_genet_probe_automatability_plan.md): slices 1 to 5 landed; the crate is genet's taproot.
- [Reader appearance isolation](archive_docs/2026-11-02/2026-09-05_reader_appearance_isolation_plan.md): landed 2026-09-06 (native verification is in its receipts file).
- [Platform families audit](archive_docs/2026-11-02/2026-08-03_platform_families_audit.md): superseded; its one recommendation was executed.
- [Browser gap analysis](archive_docs/2026-11-02/2026-08-17_smolweb_browser_gap_analysis.md): historical gap map and acceptance ledger.
- [Livery and Buckram dependency cutover](archive_docs/2026-11-02/2026-08-21_livery_buckram_dependency_cutover.md): done 2026-08-21.
- [Command palette open lag](archive_docs/2026-11-02/2026-08-22_command_palette_open_lag.md): closed with measured receipts.
- [Turnstone architecture plan](archive_docs/2026-11-02/2026-07-10_turnstone_architecture_plan.md): rungs 1 to 7, the meerkat funeral, toolkit migration, Alembic receiving, source and service boundaries and native frames; cut (history only).
- [Unusual-protocols browser plan](archive_docs/2026-11-02/2026-10-06_unusual_protocols_browser_plan.md): rulings U1 to U17, AC1 to AC6, SC1 to SC4, Errand adoption, native receiving and Hest H1 to H3 history; cut (history only).
- [User-agent taxonomy plan](archive_docs/2026-11-02/2026-08-03_user_agent_taxonomy_plan.md): U0 to U5 and B0 to B4 history; cut (history only).
- [Retained clip receiver plan](archive_docs/2026-11-02/2026-10-11_retained_clip_receiver_plan.md): receiver stages S1 to S11; cut (history only).
- [Pane registry plan](archive_docs/2026-11-02/2026-08-08_pane_registry_and_graph_panes_plan.md): lanes A0 to A8, folds and scenes; cut (history only).
- [Page lifecycle plan](archive_docs/2026-11-02/2026-09-06_page_lifecycle_plan.md): rulings L1 to L7 and the research workflow R0 to R4; cut (history only).
- [Browser surfaces implementation plan](archive_docs/2026-11-02/2026-08-25_browser_surface_implementation_plan.md): T0 to E0.2 and the Cue receiving history; cut (history only).
- [Sky home daily timeline](archive_docs/2026-08-26/2026-08-26_sky_home_timeline_plan.md): landed T5a astronomy consumer with an exact-source retained pane and headed receipt (`Code/testing/turnstone/receipts/sky_home_20260826/README.md`).

Recovered research, not current direction (archived whole, unchanged, by Mark's ruling; their relative links into the
deleted Graphshell docs tree are dead by design):
[RSS/Atom feed graph model](archive_docs/2026-11-02/2026-03-28_rss_feed_graph_model.md),
[Smolnet dependency health audit](archive_docs/2026-11-02/2026-03-28_smolnet_dependency_health_audit.md),
[Smolnet follow-on audit](archive_docs/2026-11-02/2026-03-28_smolnet_follow_on_audit.md),
[Smolweb browser capability gaps](archive_docs/2026-11-02/2026-04-09_smolweb_browser_capability_gaps.md),
[Smolweb discovery and aggregation signals](archive_docs/2026-11-02/2026-04-09_smolweb_discovery_and_aggregation_signal_model.md),
[Smolweb graph enrichment and accessibility](archive_docs/2026-11-02/2026-04-09_smolweb_graph_enrichment_and_accessibility_note.md),
[Smolnet capability model and scroll alignment](archive_docs/2026-11-02/2026-04-16_smolnet_capability_model_and_scroll_alignment.md),
[Smolweb compliance and Middlenet HTML contract](archive_docs/2026-11-02/2026-04-16_smolweb_compliance_and_middlenet_html_contract.md).

## Archived receipts

Per core section 8, each archived plan keeps one file in `archive_docs/receipts/` with its evidence index and old-to-main hash
table. The run evidence itself is in `Code/testing/turnstone/receipts/`, `.../design_docs_receipts/` and `.../support_receipts/`
(moved out of the repository 2026-11-02; Mark: "Move all, fixture to tests/"). Files for plans archived on 2026-10-30 and 2026-11-01:
[retained projection](archive_docs/receipts/2026-10-30_retained_projection_plan_receipts.md),
[retained reader recovery](archive_docs/receipts/2026-11-01_retained_reader_recovery_plan_receipts.md).

Receipt-like documents, moved under their own stem (Mark: "Own stem in receipts/"):

- [Screen-reader pass](archive_docs/receipts/2026-08-20_screen_reader_pass_receipts.md): manual Narrator and UIA acceptance record for the Frozen Projection pane.
- [Shared diagnostics and Gloss/Inspector adoption](archive_docs/receipts/2026-09-29_shared_diagnostics_gloss_inspector_receipts.md): September 30 combined source graph, 635 workspace tests, five native Sky/Diagnostics images.
- [Current consumer qualification](archive_docs/receipts/2026-10-13_current_consumer_qualification_receipts.md): the October 13 to 14 consumer qualification narrative (was not indexed before).
- [Pane inventory](archive_docs/receipts/2026-08-09_pane_inventory_receipts.md): A0 inventory of pane kinds, sources, multiplicity and registry targets.
- [Meerkat harvest](archive_docs/receipts/2026-07-18_meerkat_harvest_receipts.md): port, note or leave audit made before retiring the donor application; the "Noted" backlog is carried by the architecture successor.

Receipts files for archived plans:
[turnstone_architecture](archive_docs/receipts/2026-07-10_turnstone_architecture_plan_receipts.md), [unusual_protocols_browser](archive_docs/receipts/2026-10-06_unusual_protocols_browser_plan_receipts.md), [user_agent_taxonomy](archive_docs/receipts/2026-08-03_user_agent_taxonomy_plan_receipts.md), [retained_clip_receiver](archive_docs/receipts/2026-10-11_retained_clip_receiver_plan_receipts.md), [pane_registry_and_graph_panes](archive_docs/receipts/2026-08-08_pane_registry_and_graph_panes_plan_receipts.md), [page_lifecycle](archive_docs/receipts/2026-09-06_page_lifecycle_plan_receipts.md), [browser_surface_implementation](archive_docs/receipts/2026-08-25_browser_surface_implementation_plan_receipts.md),
[place port](archive_docs/receipts/2026-07-28_turnstone_place_port_plan_receipts.md),
[rung 5 panes](archive_docs/receipts/2026-07-14_turnstone_rung5_panes_plan_receipts.md),
[genet probe](archive_docs/receipts/2026-07-17_genet_probe_automatability_plan_receipts.md),
[reader appearance isolation](archive_docs/receipts/2026-09-05_reader_appearance_isolation_plan_receipts.md) (includes the folded native verification receipt),
[browser/resource handoff](archive_docs/receipts/2026-10-08_browser_resource_handoff_receipts.md),
[q-pc lane handoff](archive_docs/receipts/2026-10-14_qpc_lane_handoff_receipts.md),
[browser gap analysis](archive_docs/receipts/2026-08-17_smolweb_browser_gap_analysis_receipts.md),
[Livery and Buckram cutover](archive_docs/receipts/2026-08-21_livery_buckram_dependency_cutover_receipts.md),
[command palette open lag](archive_docs/receipts/2026-08-22_command_palette_open_lag_receipts.md).

Foundation source adoption (2026-10-19): canonical promoted-family revisions and consumer source validation are recorded in `mere/design_docs/mere_docs/implementation_strategy/2026-10-28_family_promotion_plan.md`. Existing application and domain authority remain with their owning components.
