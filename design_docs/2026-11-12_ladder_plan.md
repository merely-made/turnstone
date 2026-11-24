# Turnstone's ladder: rungs, platforms and what each can do

**Date:** 2026-11-12
**Status (2026-11-14):** Superseded by [native Graphshell integration](2026-10-28_graphshell_host_plan.md#graphshell_host-d2-native-turnstone-first-then-the-browser-extension-2026-11-14). This file records the earlier proposal. The uncommitted rung implementation was removed. [Producer composition](2026-10-28_graphshell_host_plan.md#graphshell_host-d3-compose-the-frame-from-attached-producers-2026-11-14) carries forward the pattern and platform evidence.

**For:** Turnstone as the reference app that ships in rungs, from a web viewer to a build with foreign engines, and a graded record of what each platform and each producer supports. Opened by [graphshell_host D1](2026-10-28_graphshell_host_plan.md) (Turnstone is the reference app and ladders; Graphshell is the host library).

Not here:
- How an engine is registered, selected and enabled: [the engine adoption plan](2026-08-03_turnstone_engine_adoption_plan.md) (`turnstone_engine_adoption D1`: compiled in or a separate process, one inventory).
- The engine consumer gates (Scry, Weld, Servo/Graft): [the user-agent taxonomy plan](2026-11-02_user_agent_taxonomy_plan.md).
- The open phases of the app in `ports/graphshell`: [the host plan](2026-10-28_graphshell_host_plan.md).
- The arrangement, scene and dynamics catalogs themselves: `graphshell/design_docs/graphshell_docs/research/2026-08-15_projection_grammar_catalog.md`, `.../2026-08-18_scenograph_content_catalog.md` and `.../implementation_strategy/2026-10-28_dynamics_grammar_plan.md`.

## Decisions

### ladder D1: two tables, platform first (2026-11-12)

Mark, 2026-11-12: "Ahh, a typo from me: meant platform. But hey, 1 is interesting", then "Platform table + pattern table (Recommended)". Capability by platform is the main table, each cell an evidence level, and a grade per platform is derived from it. A second table records which composition patterns each producer supports. The grade is computed, never assigned.

Links: area grading · draws on graphshell_host D1 · plans L2 · plans L3 · summary capability by platform is the truth table, producer by pattern the second, grades derived

### ladder D2: the ladder gets its own plan (2026-11-12)

Mark, 2026-11-12: "New dated plan (Recommended)". This plan names each rung, what it contains, its platforms and its done-conditions; the host plan and the engine adoption plan point here.

Links: area documentation · draws on graphshell_host D1 · summary the ladder lives in its own Turnstone plan

### ladder D3: native mobile is a reserved rung (2026-11-12)

Mark, 2026-11-12: "We haven't planted it yet", then "Reserve the rung (Recommended)". Android and iOS are named as targets of the light rung and their constraints stay in view. They get no phases until Mark plants them.

Links: area platforms · plans L1 · summary Android and iOS are reserved targets of the light rung with no phases

### ladder D4: a platform's grade is its rung (2026-11-13)

Mark, 2026-11-13: "Grade is the rung (Recommended)". A platform's grade is the highest rung whose every capability has executed evidence on that platform. One unproven cell holds the platform at the rung below.

Links: area grading · amends ladder D1 · plans L2 · summary a platform's grade is the highest fully evidenced rung

### ladder D5: the pattern list (2026-11-13)

Mark, 2026-11-13: "Display,Interaction,Multiplicity,Physics body" (all four offered). The pattern table grades seven patterns in four groups: display (snapshot card, live tile), interaction (takes input, accessibility subtree with actions), multiplicity (lens, one node at two sizes) and physics body.

Links: area grading · amends ladder D1 · plans L3 · summary seven patterns in four groups

### ladder D6: the host plan's open phases are Turnstone's by default (2026-11-13)

Mark, 2026-11-13: "Turnstone by default (Recommended)". Every open phase of the host plan becomes Turnstone work on a named rung unless there is a reason it belongs to the library in graphshell.git or should drop; only the exceptions come back to Mark, with evidence.

Links: area product · draws on graphshell_host D1 · plans L5 · summary host plan phases become Turnstone work unless excepted

### ladder D7: rungs are defined in an authored `rungs.toml` (2026-11-13)

Mark, 2026-11-13: "Authored rungs.toml (Recommended)". One TOML file lists each rung and its cargo features; the composition check reads it, the build reads it so the app can report its rung, and L2's table cites the same names.

Links: area builds · plans L1 · summary one authored rungs.toml is read by the check and the build

### ladder D8: the top rung is mods (2026-11-13)

Mark, 2026-11-13: "Top rung is 'mods' (Recommended)". The fourth rung is every opt-in mod, engines included: `scry`, `servo`, `weld`, `nomadnet`, `wasm`, `piccolo`. Each mod is its own row and the rung is never all-or-nothing, so under D4 a platform's grade tops out at services, with its evidenced mods listed beside it (confirmed by D13).

Links: area builds · amends the rungs table · amends ladder D4 · plans L1 · summary the top rung is every opt-in mod, each its own row

### ladder D9: a partial build reports its rung plus extras (2026-11-13)

Mark, 2026-11-13: "Rung plus extras (Recommended)". The app reports the highest rung it contains whole and lists the features enabled above it, for example "light native + peers, scry".

Links: area builds · plans L1 · summary a build reports the highest whole rung plus its extra features

### ladder D10: grafting is plumbing, not an engine (2026-11-13)

Mark, 2026-11-13: "Grafting is plumbing (Recommended)", after the rung check found `grafting` in the default build under `wgpu-cue`. `grafting` is the shared-texture import under Cue and leaves the engine list. The constraint forbids the engines themselves on the light rungs: `graft-engine`, `grafting-servo`, `servo`, `scrying`, `scrying-engine`, `webview2-com`, `welding`, `weld-engine`, `cef`.

Links: area builds · amends graphshell_host D1 · plans L1 · summary grafting is allowed on every rung; the engines are what the light rungs forbid

### ladder D11: the workshop rule follows athanor (2026-11-13)

Mark, 2026-11-13: "Point it at athanor (Recommended)". The composition check requires `mere-athanor/workshop-native` for `workshop` and treats it as a leak without it; the `mere-alembic` rule is gone with the package.

Links: area builds · plans L1 · summary the workshop composition rule names mere-athanor

### ladder D12: place-reticulum is a mod, peer-tests is test-only (2026-11-13)

Mark, 2026-11-13: "Mod and test-only (Recommended)". `place-reticulum` has its own row on the mods rung; `peer-tests` is listed as test-only and is on no rung.

Links: area builds · amends ladder D8 · plans L1 · summary place-reticulum joins the mods, peer-tests stays off the ladder

### ladder D13: no platform is graded mods (2026-11-13)

Mark, 2026-11-13: "Yes, as read (Recommended)". A platform's grade tops out at services; its evidenced mods are listed beside it, for example "services + scry".

Links: area grading · amends ladder D4 · plans L2 · summary the grade tops out at services with mods listed beside it

## The rungs

Working names. A rung contains the one below it.

| Rung | Contains | Built from today | Platforms |
|---|---|---|---|
| Web viewer | The graph and its scenes as a remote lens; no local engine | `ports/graphshell/web` (wasm, WebGL2) | any evergreen browser, desktop and mobile |
| Light native | Linked data, the smolweb lanes, Reader, Genet/Livery as the app frame, Errand, graph scenes | `default = []` | desktop; Android and iOS reserved (D3) |
| Services | Peers, authoring, inference, workshop, audio, sky | `full-services` and its parts | desktop; mobile unassessed |
| Mods | Each its own row (D8). Engines: Scry (system webview), Servo through Graft, Weld (bundled Chromium). Others: NomadNet pages, the wasm participant lane, Piccolo control scripts | `scry`, `servo`, `weld`, `nomadnet`, `wasm`, `piccolo`, `place-reticulum` | as each platform allows |

The light rungs never link an engine (graphshell_host D1, as D10 reads it); the list is `rungs.toml`'s `[engines]`.

## Phases

### L1. Rungs as named builds

Each rung is a build someone can ask for by name, and a rung's contents are checked rather than assumed.

- Done when: each rung has a named build recipe; a check fails if the web viewer or light native rung's dependency graph contains an engine package (D10); the app reports which rung it is; the light rung's row for Android and iOS reads reserved, with the known constraints listed and no claim.
- Needs: nothing.

### L2. The platform table

Capability by platform, one evidence level per cell (library API, adapter forwarding, Turnstone consumption, executed consumer evidence), with absent and reserved as values.

- Done when: the table is one authored TOML file read by the app and by the docs; every capability the engine inventory reports has a row; every cell above "library API" cites a test or a receipt path; the per-platform grade is computed from the table and shown where an engine or rung is chosen; a cell without evidence cannot raise a grade.
- Needs: L1 for the rung names. The grade is the rung (D4), so each capability row names the rung it belongs to.

### L3. The pattern table

Producer by composition pattern: what a producer can be in a scene (for example a snapshot card, a live tile, a tile that takes input, an accessibility subtree, a lens, a physics body, one node shown at two sizes).

- Done when: the table's columns are D5's seven patterns; each registered producer has a row with the same evidence levels as L2; a surface that needs a pattern asks the table and shows a named refusal when the producer lacks it.
- Needs: L2's file format.

### L4. One catalog on every rung

The same saved arrangement, scene and dynamics settings drive the web viewer and the native app.

- Done when: one serialized scene opens on the web viewer rung and the light native rung with the same members and arrangement, and a setting a rung cannot honour is refused by name rather than dropped.
- Needs: L1; the host plan's T4 for the viewer.

### L5. Integrate `ports/graphshell`

What the Graphshell app does becomes Turnstone's, rung by rung, and the app name retires.

- Done when: every open phase of the host plan names the rung it lands on as Turnstone work (D6), each exception (library, or drop) is ruled by Mark, and the host plan is cut to a successor that no longer calls the app Graphshell.
- Needs: Mark's rulings on the exceptions only.

## Findings

- 2026-11-12: Turnstone's default build has no foreign engine: `Cargo.toml:373` is `default = []`; `weld`, `scry` and `servo` are opt-in at `Cargo.toml:402`, `:405`, `:408`, and all three name `dep:windows`.
- 2026-11-12: `EngineEnableSet` does not appear in `src/`, `ports/` or `crates/` (0 hits), so nothing enables or disables an engine at run time yet.
- 2026-11-12: there is no `target_os = "android"` or `"ios"` in any `.rs` or `.toml` under `src/`, `ports/` or `crates/` (0 hits), and no Android or iOS build has been tried.
- 2026-11-12: the web viewer is its own lock root (`ports/graphshell/web`, package `graphshell-web`) under a 3.50 MiB gzip cap per route artifact (host plan, Constraints).
- 2026-11-12: support is already recorded at four levels, and "a library capability is not a Turnstone capability" (taxonomy plan, Findings). Those levels are L2's cell values.
- 2026-11-12: candidate patterns already named in the docs: the representation slots glyph, card, sprite, live pane, snapshot and nested (`graphshell/design_docs/graphshell_docs/research/2026-07-21_projection_engine_prior_art_brief.md`), Body and Face (`.../implementation_strategy/2026-06-23_node_body_face_model_plan.md`), and the Scry residual gates for lenses and different-size appearances of one node (taxonomy plan, O3).
- 2026-11-13: most of L1's engine check already exists for the native build. `scripts/check_default_links.py` reads `cargo tree -p turnstone -e normal --target <triple>` and fails when a package appears without its feature (`OPTIONAL`, `:16-27`: servo, weld, scry, nomadnet, wasm, piccolo and four services); `.github/workflows/boundaries.yml` runs it for six feature sets on two targets. Gaps against L1: the bare `grafting` package is in no list (`:21` names `graft-engine` and `grafting-servo` only); the web viewer's own lock root (`ports/graphshell/web`) is not checked; nothing names a rung; the app reports no rung. The script passes `--locked` to `cargo tree` although no lock is tracked.
- 2026-11-13: three opt-in features sit outside `full-services` and are not engines: `nomadnet`, `wasm` and `piccolo` (`Cargo.toml:391`, `:395`, `:398`). The four rungs as named do not place them.
- 2026-11-13: `grafting` is in the light native graph on both checked targets. `cargo tree -p turnstone -e normal -i grafting` on the default features shows `grafting 0.6.2` under `wgpu-cue 0.1.4`, which Turnstone and `pelt-core` both name. Ruled plumbing (D10).
- 2026-11-13: the script's `workshop` rule is stale. It requires `mere-alembic/workshop-native` (`REQUIRED`, last changed 2026-10-29 in `571c2f3c`), and no `mere-alembic` package is in the `full-services` tree; `mere-athanor` carries `workshop-native` there. The services rung failed the check on both targets for that reason; repointed under D11.
- 2026-11-13: the web viewer rung passes: 495 package names on `wasm32-unknown-unknown`, none of them an engine package.
- 2026-11-13: two cargo features were on no rung, `place-reticulum` and `peer-tests`; placed by D12.
- 2026-11-13: `scripts/check_default_links.py` and the pre-commit hooks pass `--locked`, and `Cargo.lock` is tracked here and in `ports/graphshell/web`. The build dependency this plan adds (`toml`, for `build.rs`) added one line to the root lock.

## Progress

- 2026-11-12: plan founded; decisions D1 to D3 recorded.
- 2026-11-13: decisions D4 to D6 recorded; L1 is the first phase (Mark: "L1 named builds").
- 2026-11-13: L1 built, uncommitted: `rungs.toml`; `build.rs` and `src/rung.rs` (the observation snapshot carries the rung and its extras); `scripts/check_default_links.py --rung <name>` with the engine-package rule and the repointed workshop rule; the web viewer rung added to `boundaries.yml`. Decisions D7 to D13 recorded. Light native and services pass on `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu`, the web viewer on `wasm32-unknown-unknown`; the script's 10 unit tests pass and `cargo check -p turnstone --lib` passes. Not done: no rung is shown anywhere in the UI, and `boundaries.yml` has not run.
- 2026-11-14: removed the uncommitted L1 rung machinery, including `rungs.toml`, `build.rs`, `src/rung.rs`, the snapshot field, the build-time `toml` dependency, `--rung` and its CI entry. Turnstone `494efe06` kept the `mere-athanor` workshop fix and recognizes `scrying` as an engine package. The composition check still permits `grafting` for shared-texture import.
