# Pane registry, graph views and shell composition: open work

**Date:** 2026-11-02
**Status (2026-11-02):** cut 2026-11-02 from [the pane registry plan](archive_docs/2026-11-02/2026-08-08_pane_registry_and_graph_panes_plan.md), archived whole with its 1,660 lines of lane history, drafts and inline run evidence. Closed in the archived plan: A0, A1, A6, the 2026-09-29 pane consolidation, A4/S3 as an implementation (shared Workspace custody, Tabard appearance, keyboard, floating and docking, per-window accessibility, retained inline bodies; 14 headed cases on the published binary, 2026-10-14), contributed-pane semantic automation (2026-09-30), the native Orrery selection folds and the graph scene refresh (complete for the default macOS Orrery host, 2026-10-16 to 2026-10-17; 982 workspace tests). Open: A2 (partly built), A3, A4/A5 acceptance, A7 and A8 and the items below.

## Rulings in force

The plan has no numbered rulings; they are prose and stay in force, cited by archived line: window, graph, Forme and Workbench ownership (L35-48); source, context and view state are separate (L146-220); the session rule (L255-264); pane stations, floats and tear-out (L369-377); the registry and typed per-kind config (L402-464); the 2026-09-29 ruling (Gloss the configurable overview, Inspector document and graph-object analysis, `mere-apparatus` shared diagnostics, Steward and object-analysis Apparatus retired; L12-21, L433-438); the A4 outcome 1 ruling (Mark, 2026-09-04, L674-696); the multiplicity rule (L878-887); and the stop rules (L810-823). Old ids (A0 to A8) are lane names, not rulings, and stay grandfathered. DOC_POLICY section 11 applies forward from 2026-11-02; new decisions are keyed `pane_registry_and_graph_panes D<n>`.

## Decisions

### pane_registry_and_graph_panes D1: receipts leave the repository (2026-11-02)

Mark, 2026-11-02: "Move all, fixture to tests/". The receipts (`outer_workspace_20261012`, `frame_appearance_20261013`, `frame_controls_20261013`, `stations_windows_20261013`, `graph_folds_20261016`, `graph_scenes_20261016`) are in `Code/testing/turnstone/receipts/`, cited as paths.

Links: area receipts · applies DOC_POLICY section 8 · summary receipts moved to Code/testing/turnstone

### pane_registry_and_graph_panes D2: cut the plan, drafts first (2026-11-02)

Mark, 2026-11-02: "Everything, drafts first". The plan is cut under DOC_POLICY section 8.

Links: area documentation · replaces 2026-08-08_pane_registry_and_graph_panes_plan · summary plan cut to open work

### pane_registry_and_graph_panes D3: hashes (2026-11-02)

Mark, 2026-11-02: "Live docs: local + sibling". Mapping: [the receipts file](archive_docs/receipts/2026-08-08_pane_registry_and_graph_panes_plan_receipts.md). `7441f2f3` has no equivalent on main and stays as text.

Links: area documentation · summary live docs cite main hashes

### pane_registry_and_graph_panes D4: the reader CPU/FPS sampling item lands here (2026-11-02)

Mark, 2026-11-02: "Everything, drafts first". The archived reader appearance isolation plan is landed except its CPU and frame-rate sampling (O11).

Links: area documentation · draws on 2026-09-05_reader_appearance_isolation_plan · summary extracted item

## Open work

### O1. A2: finish the graph runtime pool and two-graph composition

Built already (checked 2026-11-02): `graph_runtimes: GraphRuntimePool`, `graph_views`, `forme_runtimes` and `pane_context` on `App` (`src/app/mod.rs:109-117`, no `canvas` or `workbench` field), `SurfaceKind::Graph(PaneId)` (`src/surface.rs:79-84`), `graph_for_pane` (`src/app/app_shell_chrome_config.rs:209`), the test `graph_pool_keeps_two_graph_authorities_independent` (`src/app/runtime_pool/tests.rs:34`). Still open: the `active` compatibility cursor and Deref to the active canvas (`src/app/runtime_pool.rs:48-51`, `src/app/runtime_pool/impl_deref_for_graph_runtime_pool.rs:6-14`), the single `session_id` and wholesale `adopt_session` (`src/app/mod.rs:158`, `src/app/session_lifecycle/app_adopt_session.rs:13-26`), the legacy `PaneContent` adapter (`src/panes/mod.rs:171-173`), and the A2 TODO comments (`src/action/action.rs:377`, `src/app/app_shell_chrome_config.rs:73,81`, `src/shell_services.rs:12`). No scenario for the done-condition exists. Per-pane independent fold sets and independent scene-driving views of one graph are A2 boundaries (L1557-1558, L1633-1634).

**Done when:** graph A and graph B render side by side in one window, both receive pointer and keyboard input, a mutation in A leaves B unchanged, and restart restores both (L646-647).

### O2. A3: multiple views and Workbenches

One graph through two independent views, two Workbenches with different arrangements, the Inspector following the active member of the intended Workbench (L663-664). Gated on A2 (L651). `FormeRuntimePool` and `GraphPaneViews` exist (`src/app/mod.rs:112-115`); no scenario or test for the done-condition was found.

**Done when:** L663-664 holds with a headed receipt.

### O3. A4/S3 and A5 acceptance

The implementation is closed; acceptance is not: physical OS activation, human assistive technology (Narrator, VoiceOver, Orca), monitor DPI transitions, the native theme, narrow-window, DPI and performance wall, early scrollbar-fade pixel timing, drop-region previews and lens native-engine pixel and input delivery (L7-8, L1131-1137, L1233-1235, L1308-1312, L1416-1422, L1493-1497); merging A5 blueprint stations and persistence and removing the blueprint and legacy bridge, whose removal gate is open (L1338-1340; `src/panes/blueprint.rs`, `src/panes/legacy_bridge.rs` still exist); the pinning, visibility and constraints panel is not mounted (L1366-1367); the plan does not say the L1139-1149 native done-list was closed as a whole.

**Done when:** a nested mixed-surface layout survives drag, resize, save, reload and tear-out with one authoritative topology and the fallback recorded (L712-713), and a pane moves tile, float, nested split, window and back without losing state (L723-724), with the physical items above receipted or dropped by Mark.

### O4. A7: Cambium and Settings completion

Marked open (L789), but the tree is ahead of the label: no `pelt/appearance` string remains (`src/settings_provider.rs:38` is `turnstone/application`), several specs are `SettingMutability::Live` (`src/settings_provider.rs:276,292,327,376,397,407`) and `set_shellbar_edge` applies placement (`src/shell_services/shell_chrome_config_layout_scale.rs:23`), so the claim at L130-131 that Turnstone does not apply them is stale. Remaining per the plan: Cambium catalog promotions (pane shell and header, settings form, empty and error states, Frisket specimens), generic render from `SettingControl`, every Live setting connected to the running host, restart-only settings labelled honestly (L791-796).

**Done when:** "catalog specimens cover narrow and regular layouts, Turnstone consumes them, theme/zoom/shell placement change live, and values survive relaunch" (L798-800), after an audit of the three bullets against the tree.

### O5. A8: named blueprints

Save, load, duplicate, rename, reset-to-default and reset-to-heuristic layouts, portable exports without private ids, role-based import, stores preserved (L803-805). No named-layout code exists; `src/panes/workspace_store.rs` persists the current Workspace only. Persistence of contributed `PaneSource` values is an A8 concern (L829-832, L861-862).

**Done when:** Browse, Inspect and Operate are editable examples rather than modes, and a layout can be shared or reset without changing graph or session truth (L807-808).

### O6. Deferred decisions and unclaimed gaps

Write-side third-party pane registration through the participant gate; stored cross-session Overmap relations; versioned contributed `PaneSource` persistence; product display names (Canvas versus Orrery, Navigator versus Gloss); default layout details (L825-835); an embedded `SpaceBlueprint` waits for a real second consumer (L352-353, L817-818); the pane close affordance written as open on 2026-08-16 (L893-899), probably covered by the A4 close controls but unverified; `ContributedSurface::pointer_move` dispatching without consulting `pointer_capture()` (L844-848; `src/contributed_surface/contributed_surface_pane_new.rs:238-240`, `:269`, not checked inside the session); transcript placements Docked, Floating and Overlay with no host code positioning them (L772-776).

**Done when:** each has a done-condition or Mark drops it.

### O7. Built-in pane semantic qualification and the next Inspector surface

All built-in panes are a later qualification slice: custom-leaf semantic parity, full built-in selectors, document selection, exact frame correlation, human assistive technology (L918-919, L925-926). The next Inspector slice binds Clip to the displayed document member, resolves viewer sidecar respawn through the owning runtime and adds action tokens with pane, context, generation, member and DOM identity; it is "a researched next slice, not a completed implementation" and Mark's optional scope prompt is pending (L1030-1068).

**Done when:** Mark rules the scope; then L1063-1068 holds (followed A versus cursor B in the same and different graphs, pointer and queued assistive-technology parity, stale controls fail, native persisted viewer and source-bound Clip receipts, B untouched).

### O8. Folds and scenes: separate boundaries

Gloss and Overmap projection consumers, headed lens behavior, other platforms, physical GPU recovery and human assistive technology (L1559-1561); Windows and Linux, portable `SceneDocument` hosting, remote or frozen arrangement expansion and independent scene-driving views (L1568-1569, L1660-1662).

**Done when:** each is a gate in the owning plan or dropped.

### O9. Native Pelt, supplier intake and Windows headed gap

The Pelt baseline adoption does not close the A4 inline body join, floats, physical AT or the native wall (L1233-1236); the supplier intake does not qualify the complete engine, renderer recovery, Redshank, Knot or other contributed-surface lanes (L1189-1190, L1477-1479); the S-PC headed run "has not run" (L1124-1125).

**Done when:** Mark names which of these Windows or engine gates he wants.

### O10. Build hygiene

A machine-local, git-ignored `.cargo/config.toml` path override named `dramatis/crates/personae` after the crate moved; the plan does not say it was repaired (L25-29).

**Done when:** each working copy's override is checked once against the current crate paths.

### O11. Reader appearance isolation: CPU and frame-rate sampling

Landed 2026-09-06 except CPU and frame-rate sampling (archived reader appearance plan, status line and closing paragraph: the idle predicate and shutdown were observed, "not zero background CPU or a frame-rate target"; active place membership and an ongoing publishing transfer were not exercised).

**Done when:** a paired CPU and frame-rate sample of the shared Reader across two appearances exists, with budgets Mark names, or Mark drops the item.

## Findings

- 2026-11-02: A6 was once marked open because the document was read, not the tree (L744-751); A2 and A7 had the same risk, so O1 and O4 name the code.
- 2026-11-02: a pane whose source is fixed to a singleton must be `PerSpace` at most (Device Receipts was `Many` and seven copies were reachable in one window, L869-887).
- 2026-11-02: `AppEvent` cannot be the shell scrollback: partial, drained each frame, no command identity or correlation (L133-139).
- 2026-11-02: the full-workspace baselines the plan records: 612 passed with 9 ignored (L914), 635 with 9 (L925), 958 with 10 (L1503), 982 with 10 (L1655); the 973-pass run had one stale settings-pane test (L1618).
- 2026-11-02: locked-console macOS sessions present zero frames; such attempts are excluded from acceptance (L1614-1616).

## Progress

- 2026-11-02: plan cut from the pane registry plan; decisions D1 to D4 recorded; the reader appearance CPU/FPS item carried as O11. The evidence index and old-to-main hash table are in the receipts file.
