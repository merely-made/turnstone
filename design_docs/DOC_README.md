# Turnstone design documents

Model-free retained research (2026-10-30): M1–M3 implementation have landed;
the canonical plan is `eidetic/design_docs/eidetic_docs/implementation_strategy/2026-10-11_eidetic_development_plan.md`, P4a.
The ordinary saved-source search covers admitted captures, selections and notes,
with configurable search and comparison, explicit Left/Right selection, exact
side citations and retained readers. Comparison preserves full source identities;
current head and lineage remain unknown unless declared by their owners.
The default path does not require inference. Shared/provider and host regression
checks, optional-inference compilation and dependency boundaries pass. M4 structured
queries and M5 headed product/offline restart qualification remain open.
Alembic retains proposal and acceptance ownership. Evidence lives in the canonical
plan's linked receipt archives.

Rust policy (2026-10-13): working toolchain 1.99.1 and shared MSRV 1.97.1, qualified separately. Canonical policy: `mere/design_docs/2026-08-12_family_composition_thesis_brief.md`, section "Shared Rust toolchain and minimum version".

Murm source adoption (2026-10-13) is tracked in
`murm/design_docs/2026-10-13_murm_family_move_plan.md`. Active consumer roots
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

## Project and policy

- [Reader appearance isolation](2026-09-05_reader_appearance_isolation_plan.md):
  shared Reader content with independent pane state and stable appearance roles.
- [Reader native verification receipt](2026-09-06_reader_appearance_native_check_receipt.md):
  headed shared-source, independent-scroll, and sibling-close evidence, with idle limits.

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
- [Relationship navigation and visible pane bindings](2026-10-21_relationship_navigation_and_pane_bindings.md):
  landed `57be5ab`. Related disclosure; Inspector and Roster show and change
  their source binding (Follow active pane, follow a publisher, Pin).

- [Hest command invocation and Cambium 0.5 adoption](2026-10-06_unusual_protocols_browser_plan.md#hest-h3-command-invocation-2026-10-20): canonical H3 keyboard/reducer receiving qualified on Mere `9ef5e3026` and embedded Knot `ebf10c741`; 1107 full-library passes, all-targets and portable metadata checks pass. Deferred dispatch remains pending; Git delivery is authorized and crate publication remains held.

- Resident answer card: source-owner publication, exact-version citations and
  headed I5 acceptance are tracked in
  `mere/design_docs/mere_docs/implementation_strategy/2026-10-15_resident_inference_and_recall_plan.md`.
  Native I5 is qualified with the pinned GPU model and an offline-origin
  restart. The October 20 embedding continuation is qualified on macOS:
  current SDK intake, real MiniLM fused recall through headed retained sources,
  and an independent cold product/resident pair with a new ask. Its
  [receipt](../docs/receipts/embedding_recall_20261020/qualification.json)
  preserves the exact envelope/index, source/citation and private-journal evidence. Turnstone's client integration is recorded in the
  [architecture plan](2026-07-10_turnstone_architecture_plan.md).

- [Retained clip receiver](2026-10-11_retained_clip_receiver_plan.md): Fleece
  selected-package admission, local Inker reading and Redshank voice-body
  playback in the receiving mere; published suppliers and bounded headed sealed
  Knot/voice receive, cold playback and onward export are qualified locally.
  Embedded Knot primary-mouse selection, immediate retention and cold export
  are qualified in the October 20 continuation. A separate October 20 receipt
  qualifies embedded Redshank selected text-note retention, source-absent cold
  reopen and onward export through an independent receiver. Personal
  vault/migration, broader editor input and format gates remain. The October 21
  continuation qualifies both retained text bodies in explicit Answers scope,
  real fused recall, local citations and source-absent cold export on its recorded
  receiving cut. A companion packet also qualifies the final shared shell and
  shared Apparatus-core SDK `7edfe3b09`; the four-scalar citation/model limit
  remains explicit. The mixed-source continuation records a four-member scope, 52 native warm/cold
  captures and lexical-versus-fused measurements, with generated-reference and
  exact runtime-cut limits explicit.
- [q-pc lane handoff, 2026-10-14](2026-10-14_qpc_lane_handoff.md): the lane's
  resume point on q-pc (Mac), with Windows gates on o-pc. Main is the only live line after the ThinkPad
  branch/worktree reconciliation; lists pins, the lens-label fix, ordered open
  gates and what stays on the ThinkPad (evidence, Mere's unpublished
  three-lanes half).
- [Browser/resource handoff, 2026-10-08](2026-10-08_browser_resource_handoff.md):
  resume point for browser adoption and applied resource-graph integration.
  Mere P2 is qualified at `3b3afa289`; per-view controls, retained session
  placement, refusal protection and graph-origin routing are applied. Default/Piccolo
  native suites, all-target checks and source review pass. Final receipts bind
  the tested source; sibling and consumer commits are published to main.
  The SC continuation centralizes controller pumping and placement-derived
  visibility. The current three-lane pass implements live document
  accessibility, shared content custody/input and Graphshell's five public
  mounts; new source and receipts remain under qualification.
- [Unusual-protocols browser plan](2026-10-06_unusual_protocols_browser_plan.md):
  Errand adoption on refreshed October 26 main is qualified with 172 native
  passes, all-target compilation, production Clippy and portable metadata;
  browser receiving and session policy remain unchanged. The plan records
  the bar ("can you use this as a web browser for the unusual protocols"),
  per-protocol state, ranked gaps and stages S0 to S11 plus SC. Ruled U1 to
  U14: accessibility on Windows, macOS and Linux leads; Pelt convergence
  (SC) comes before the keyboard and accessibility stages. The plan's S0
  execution snapshot is historical: Knot and Redshank now publish the tested
  family, and [Turnstone's S0 receipt](../docs/receipts/browser_family_20261007/README.md)
  records published source and passing Windows/Linux source qualification.
  Its browser-sidecar scope proposal separates local/attached session authority
  from PWA/extension capabilities; native SC/session/S8 ownership and stage
  order remain unchanged. The October 22 receiving continuation commits typed
  Scroll MIME replacement, shared explicit-Submit guard adoption and real native
  Micron controller tests. Its final port preserves current upstream `8eebb5a3`
  reading/navigation/command behavior and changes only receiving files plus 62
  Mere source pins. All 70 focused tests and the governed Djinn process probe
  pass on the integrated executable, covering exact native bytes, the 128 KiB
  Resource, retained controller and native link navigation. Old isolated SDK
  adoption and failed process receipts remain historical evidence. Client limits
  and headed/physical evidence retain their separate gates; HTTP policy is
  unchanged relative to the current upstream base.
- [Page lifecycle](2026-09-06_page_lifecycle_plan.md): one page across
  visiting, keeping, capturing, annotating, collecting, sharing, revision
  and deletion; rulings L1-L7 join the surfaces, capture and recycle bin
  plans and add phases for the missing joins. Source capture and persisted
  collection-scoped search are implemented. The October 17 research workflow
  audited `428deb5` and sequences save acknowledgement/recovery, ordinary
  capture/Keep, private saved-source search and offline text reading, then
  explicitly scoped capture-backed Answers and cold-reopen acceptance. Authored
  Knot notes now join this workflow: the [October 18 receipt](../docs/receipts/research_notes_20261018/README.md)
  qualifies ordinary note creation/choice, exact source citations and a cold
  origin-gone return. The October 19 named-recipient assessment records the
  missing source-owner publication handoff and durable withdrawal decision;
  deliberate attachment sharing remains unqualified. The
  [format/local-note receipt](../docs/receipts/document_formats_20261019/README.md)
  records the bounded naming and exact-citation regression checks. R0 save
  acknowledgement/recovery is implemented from fetched `b7e1529`, adopting
  published Mere `00e444286`:
  940 bounded regression tests pass, including 50 capture and 16 scene controls;
  three capture-library controls and the Clippy correctness gate also pass.
  The [R0 receipt](../docs/receipts/r0_source_save_20261017/qualification.json)
  preserves broader failures and process/durability/platform limits. R1 now
  qualifies the source-bound Inspector/catalog action, completion-bound Keep
  and metadata-only new history writes: 953 bounded tests, three capture-library
  controls, 46 supplier tests and two fresh headed processes pass. The
  [R1 receipt](../docs/receipts/r1_source_keep_20261017/qualification.json) retains
  12 reviewed captures, exact offline recovery, the HTTP provenance/host wake
  repairs and earlier failed diagnostics. The private saved-source search and
  offline text reader now have bounded macOS qualification: 969 regression
  passes, three capture-library controls, Clippy/Cargo verification and two
  headed processes with seven reviewed captures. The
  [R2 receipt](../docs/receipts/r2_saved_sources_20261017/qualification.json)
  proves exact A1 search/read and fresh reopen after actual origin shutdown and
  TCP refusal, with A2/X/foreign-session isolation. Its internal view URL binds
  session, graph, node, annotation, text hash and scalar selector; the original
  URL is a separate explicit link. R3 scoped saved-source Answers now has bounded
  macOS qualification: 979 regression passes, three capture-library controls,
  an actual admitted-route wrong-owner probe, Clippy/Cargo verification and two
  fresh headed processes with 12 reviewed captures. The
  [R3 receipt](../docs/receipts/r3_scoped_answers_20261017/README.md) independently
  binds chosen A1+B then B-only scope, sealed prompts, exact-quotation citation
  hashes/Unicode ranges and cold old-A1 reading after actual origin shutdown.
  Oversized publication refuses while full offline reading remains available.
  Earlier uncited marker attempts and repaired native failures remain diagnostics.
  [R4](../docs/receipts/r4_research_workflow_20261018/README.md) now qualifies one
  complete private saved-HTML collect/find/inspect/ask/verify/return task: two
  app launches, distinct resident OS processes, actual origin shutdown, source
  reads before both resident startups, 19 reviewed captures and independently
  checked Fleece/prompt evidence. The full workspace command passes (1,071
  library tests, 12 ignored); live wrong-owner and post-exit store probes run
  separately. Re-entry preserves the existing Answers view, one shared reveal
  operation fixes hidden destinations, and detached choices can be removed
  without admitting them to an ask. Authored Knot notes, deliberate sharing,
  personal custody installation and other-platform/physical recovery retain
  their separate gates.
- [Trail recall evaluation](2026-09-02_trail_recall_evaluation_plan.md): private captured-corpus protocol for selecting or rejecting configurable phrase recall against BM25 with held-out ranking and resource budgets.
- [Page capture and provenance](2026-08-28_page_capture_plan.md): ruled path
  for durable node-attached page captures; P1 owner contracts and exact pins
  and its clean-source compile gate are landed. Source-document capture and
  authorized collection search have app consumers; hosted Weld capture,
  offline replay, and the remaining custody work are still open.
- Shared-contract Servo receiving (2026-10-23): the browser surfaces plan
  records the concrete Metal/Cue binding and same-host Genet/Servo receiver;
  Linux/Vulkan and physical platform qualification remain separate.
- [Browser surfaces implementation plan](2026-08-25_browser_surface_implementation_plan.md): the [current tested family](../docs/receipts/browser_family_20261007/README.md) publishes runtime adoption `abb349cf` and test-only portability follow-up `8d1f907f`, adopting Servo 0.7 and CEF154 after Mere/Genet, Knot and Redshank. S0 passes 681 Windows combined-browser and 650 Linux default-feature library tests, plus locked all-targets, Cargo verification and single-source sibling trees. Linux uses a measured child-only descriptor budget. Seven current-binary native controls exit cleanly. Default Servo stale resized pixels, foreign-tree accessibility and physical three-platform AT remain release gates; diagnostic positive pixels do not close them.
- Native Orrery selection folds (2026-10-16): qualified on current published
  suppliers with 958 workspace tests, native build, 379 presented create/reopen
  frames and nine reviewed readbacks. The [receipt](../docs/receipts/graph_folds_20261016/README.md)
  and pane registry plan section "Native graph folds" retain scope and inputs.
- [Pane registry, graph views, and shell composition](2026-08-08_pane_registry_and_graph_panes_plan.md): registry and multi-graph pane roadmap, with remaining A-lane gates; The October 12 A4/S3 draft adopts shared tiled Workspace custody, Frisket furniture and versioned sidecars; the October 13 continuations connect Tabard themes/Interface typography, keyboard, shared floating/docking custody and per-window accessibility; the October 14 continuation implements retained inline bodies and native image apertures inside the shared window scene; normal published-source qualification passes 17 affected tests, the native build and 14 headed cases with 40 reviewed primary/lens captures; physical OS activation, human AT, monitor DPI transitions and early scrollbar fade pixel timing remain separate gates. The final receipt is `docs/receipts/stations_windows_20261013/published-final-native-qualification.json`. September 30 contributed semantics qualify shared retained projection, scoped automation and platform actions with scroll/stale/refusal controls and native Sky delivery. Inspector source/endpoint binding repairs precede its next semantic route; broader surfaces and human AT remain open. The October 16 graph scene refresh adopts bounded arrangements, Mosaic and shared attention/seed controls; source ownership and exact session-reopen repairs are recorded in `docs/receipts/graph_scenes_20261016/`; qualification passes 982 workspace tests, eight shared supplier tests, both headed processes and exact fresh-process reopening on macOS.
- [Turnstone engine adoption](2026-08-03_turnstone_engine_adoption_plan.md): selectable engine routing and unavailable-engine behavior.
- [Reticulum browsing](2026-08-03_reticulum_browsing_plan.md): native NomadNet page routing and Micron preview implemented; current shared presentation, typed interaction and independent carrier acceptance scopes.
- [User-agent taxonomy](2026-08-03_user_agent_taxonomy_plan.md): browser obligations assigned to graph-native Turnstone homes; B0 and scoped Windows Scry/Weld controls retain their [supplier receipt](../docs/receipts/browser_supplier_integration_20261006/README.md). U14's older publication is historical; the [U9 coordinated family](../docs/receipts/browser_family_20261007/README.md) is now published with passing Windows/Linux S0 source checks. Default Servo pixel failures, foreign accessibility and library/application release gates remain separate.
- [Turnstone place port](2026-07-28_turnstone_place_port_plan.md): shared-place product composition and two-peer acceptance path. Historical T5b/T5c Windows receipts retain their scope. The [current macOS baseline](../docs/receipts/place_sharing_20261019/README.md) qualifies create/invite/join, chosen address and chat sharing, reader refusal, and origin-gone cold return with explicit contact renewal. Current document projection, research attachments, automatic discovery/WAN and named-recipient publication remain separate.
- [Peer-web reframe](2026-07-28_turnstone_peer_web_reframe.md): current product direction for local-first personal and shared places.
- [Turnstone architecture](2026-07-10_turnstone_architecture_plan.md): structural obviation ladder and current Mere/Turnstone boundaries, including Alembic workshop receiving, enforced source and service boundaries, and qualified native pending-frame adoption across primary, lens and checked child hosts.
- [Turnstone rung 5 panes](2026-07-14_turnstone_rung5_panes_plan.md): pane, surface-composition, focus, workbench, and window foundations; October 10 application appearance adoption uses shared Tabard definitions and native workshop bindings; current-origin native13 macOS production acceptance passed four serialized processes, 521 presentations and nineteen inspected captures, including repaired 34 CSS px Theme name/seed fields; final 32 Turnstone / 74 family targeted CPU checks and exact Save/Apply/restart evidence in the [receipt ledger](../docs/receipts/tabard_adoption_20261010/README.md). Reader/browser rollout gates remain separate.
- [Genet probe automatability](2026-07-17_genet_probe_automatability_plan.md): proposed shared diagnostics, accessibility, and automation contract for Genet apps.

## Current architecture, audits, and design records

- [Browser gap analysis](2026-08-17_smolweb_browser_gap_analysis.md): historical browser and smolweb gap map, acceptance ledger, and research basis for the successor implementation plan.
- [Platform families audit](2026-08-03_platform_families_audit.md): live reachability audit across physics, graph, inference, and personal-mesh families.
- [Pane inventory](2026-08-09_pane_inventory.md): A0 inventory of pane kinds, sources, multiplicity, and registry targets.
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
- [Meerkat harvest](2026-07-18_meerkat_harvest.md): port, note, or leave audit made before retiring the donor application.
- [Gloss composite pane](2026-07-20_gloss_composite_pane.md): configurable navigation and operational overview; September 29 implementation adds Downloads by default, moves object handling into Inspector, and preserves saved arrangements while retiring Steward/Apparatus panes. Automated and native restart gates pass; shared diagnostics live in `mere-apparatus`.
- [Recycle bin and Athanor](2026-07-20_recycle_bin_athanor.md): recoverable deletion, identity-preserving restore, and eventual permanent forgetting.

## Completed plans and receipts

- [Shared diagnostics and Gloss/Inspector adoption receipt](2026-09-29_shared_diagnostics_gloss_inspector_receipt.md): September 30 combined source graph qualifies 635 workspace tests with nine ignores, five exact optional participant checks, repaired admission refusal, restored failure controls and five native Sky/Diagnostics images. Earlier 612-test migration receipts retain their original graph. The October 11 owner update points to the Fleece/Muniment source-qualified compile receipt. Human AT and exact Turnstone presented-frame correlation remain open.

- [Sky home daily timeline](archive_docs/2026-08-26/2026-08-26_sky_home_timeline_plan.md): landed T5a astronomy consumer with an exact-source retained pane and [headed receipt](../docs/receipts/sky_home_20260826/README.md).
- [Livery and Buckram dependency cutover](2026-08-21_livery_buckram_dependency_cutover.md): completed removal of retired layout edges with clean-source acceptance evidence.
- [Command palette open lag](2026-08-22_command_palette_open_lag.md): closed retained-layout and repaint diagnosis with measured receipts.
- [Screen-reader pass receipt](2026-08-20_screen_reader_pass_receipt.md): manual Narrator and UIA acceptance record for the Frozen Projection pane.

## Recovered research, not current direction

These documents remain at the active root as explicitly marked donor research.
They are inputs, not implementation authority.

- [RSS/Atom feed graph model](2026-03-28_rss_feed_graph_model.md): recovered feed-to-graph research whose current implementation differs.
- [Smolnet dependency health audit](2026-03-28_smolnet_dependency_health_audit.md): recovered dependency triage for the older Verso/Middlenet architecture.
- [Smolnet follow-on audit](2026-03-28_smolnet_follow_on_audit.md): recovered protocol and ecosystem follow-on research.
- [Smolweb browser capability gaps](2026-04-09_smolweb_browser_capability_gaps.md): recovered predecessor to the current browser gap analysis.
- [Smolweb discovery and aggregation signals](2026-04-09_smolweb_discovery_and_aggregation_signal_model.md): recovered discovery, feed, and social-signal model.
- [Smolweb graph enrichment and accessibility](2026-04-09_smolweb_graph_enrichment_and_accessibility_note.md): recovered enrichment and accessible-structure research.
- [Smolnet capability model and scroll alignment](2026-04-16_smolnet_capability_model_and_scroll_alignment.md): recovered protocol-capability and text-view alignment note.
- [Smolweb compliance and Middlenet HTML contract](2026-04-16_smolweb_compliance_and_middlenet_html_contract.md): recovered constrained-HTML contract research.

Foundation source adoption (2026-10-19): canonical promoted-family revisions and consumer source validation are recorded in `mere/design_docs/mere_docs/implementation_strategy/2026-10-19_family_promotion_plan.md`. Existing application and domain authority remain with their owning components.
