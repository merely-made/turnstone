# Turnstone design documents

This is the canonical index for active Turnstone documentation. A document's
own dated status still matters, but this index decides what is in the active
set.

## Working principles for AI assistants

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
- Use one action catalog and one observable state spine. UI, automation, and
  accessibility should consume the same named actions and facts.
- Match proof to the claim: focused unit tests for state and identity, real
  consumer tests for wiring, restart receipts for persistence, and headed
  interaction receipts for visible UI.
- Record sidequests, contradictions, unusual synergies, and still-open gates in
  the active plan. Do not let a successful focused check stand in for a green
  workspace gate.

## Project and policy

- [Documentation Policy](DOC_POLICY.md): canonical workspace documentation
  rules plus the Turnstone addendum.
- `PROJECT_DESCRIPTION.md`: reserved for the maintainer and not present yet.
  Until it exists, the repository [README](../README.md) stands alone, as the
  local policy addendum specifies.

## Current implementation plans and product direction

- [Trail recall evaluation](2026-09-02_trail_recall_evaluation_plan.md): private captured-corpus protocol for selecting or rejecting configurable phrase recall against BM25 with held-out ranking and resource budgets.
- [Page capture and provenance](2026-08-28_page_capture_plan.md): ruled path
  for durable node-attached page captures; P1 owner contracts and exact pins
  are landed, its clean-source Turnstone compile gate remains open, and P2
  custody has not begun.
- [Browser surfaces implementation plan](2026-08-25_browser_surface_implementation_plan.md): sequenced Keep, find, decision UI, engine parity, arrivals, shallows, and extension work.
- [Pane registry, graph views, and shell composition](2026-08-08_pane_registry_and_graph_panes_plan.md): registry and multi-graph pane roadmap, with its remaining A-lane gates.
- [Turnstone engine adoption](2026-08-03_turnstone_engine_adoption_plan.md): selectable engine routing and unavailable-engine behavior.
- [Reticulum browsing](2026-08-03_reticulum_browsing_plan.md): idiomatic NomadNet and Reticulum content routing queued behind the protocol adapter gate.
- [User-agent taxonomy](2026-08-03_user_agent_taxonomy_plan.md): browser obligations assigned to graph-native Turnstone homes.
- [Turnstone place port](2026-07-28_turnstone_place_port_plan.md): shared-place product composition and two-peer acceptance path.
- [Peer-web reframe](2026-07-28_turnstone_peer_web_reframe.md): current product direction for local-first personal and shared places.
- [Turnstone architecture](2026-07-10_turnstone_architecture_plan.md): structural obviation ladder and current Mere/Turnstone boundaries.
- [Turnstone rung 5 panes](2026-07-14_turnstone_rung5_panes_plan.md): pane, surface-composition, focus, workbench, and window foundations.
- [Genet probe automatability](2026-07-17_genet_probe_automatability_plan.md): proposed shared diagnostics, accessibility, and automation contract for Genet apps.

## Current architecture, audits, and design records

- [Browser gap analysis](2026-08-17_smolweb_browser_gap_analysis.md): historical browser and smolweb gap map, acceptance ledger, and research basis for the successor implementation plan.
- [Platform families audit](2026-08-03_platform_families_audit.md): live reachability audit across physics, graph, inference, and personal-mesh families.
- [Pane inventory](2026-08-09_pane_inventory.md): A0 inventory of pane kinds, sources, multiplicity, and registry targets.
- [Turnstone surfaces in Cambium](2026-07-15_turnstone_surfaces_in_cambium.md): mapping from product surfaces to Cambium components or non-Cambium lanes.
- [Turnstone founding](2026-07-08_turnstone_founding.md): naming, role swap, and original sequencing, with later authority corrections called out in the document.
- [Meerkat harvest](2026-07-18_meerkat_harvest.md): port, note, or leave audit made before retiring the donor application.
- [Gloss composite pane](2026-07-20_gloss_composite_pane.md): design for configurable section composition in Gloss and Swatch projections.
- [Recycle bin and Athanor](2026-07-20_recycle_bin_athanor.md): recoverable deletion, identity-preserving restore, and eventual permanent forgetting.

## Completed plans and receipts

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
