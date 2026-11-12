# Co-op harness: `support/graphshell-coop`

**Date:** 2026-11-11
**Status (2026-11-11):** in progress. The repair is built on branch `coop/harness` (H1 to H5 below); H6, the first headed baseline receipt on the Fedora ThinkPad, is open. The two-machine co-op phases (H7) are unverified since 2026-10-24 and are not brought up by this plan's first pass.

This plan owns the native co-op harness in `support/graphshell-coop`. It came to Turnstone with the reference app at the engine split (Mere `design_docs/mere_docs/implementation_strategy/2026-11-07_engine_split_plan.md`, U6). Its last pass was 2026-10-24. Since then custody isolation (H2, 2026-10-26) made a harness Djinn reach only a custodian it is handed, admission by consent (2026-11-01) made a fresh profile admit no application, and the split put the resident and the seeder in Graphshell `host/`. Two Fedora runs on 2026-11-11 failed on the first two (`Code/testing/engine-split/slice6_u6_door_fedora.md`). The assessment the rulings answer is `Code/testing/engine-split/slice6_u6_harness_assessment.md`.

## Rulings in force

This plan records no decisions of its own yet. It carries out two of Mark's rulings, which live in Mere's engine-split plan and are cited by key; new decisions here are keyed `coop-harness D<n>`.

- `mere:engine-split D129` (2026-11-11): "Repair the harness now, inside U6", over recording the door surface as blocked and closing, and over "pre-seeding consent as a second test-only shortcut". The split stays open until the door surface passes headed.
- `mere:engine-split D130` (2026-11-11), seven rulings, six of them this plan's:
  1. Custody: "Fixture custodian process (Recommended)". A Graphshell `host/` example runs `djinn_custodian::fixture::start` with the packaged grants at consent `none`; Djinn is the production `--features passphrase-fd` build, never in-process custody, handed `--custodian`.
  2. Consent: "Grant written by Graphshell's own code (Recommended)". A `grant` subcommand of `coop_existing_graph` calls the resident's `grant_for(graphshell, declaration(), None)`, and the harness writes its output into the owner settings before Djinn starts. No Python-authored grant, no admit-all mode, no `--grant` flag.
  3. Two repositories: "Turnstone is the root (Recommended)". Graphshell is checked out at the revision Turnstone's `Cargo.lock` pins for graphshell.git and `host/` is built with its own lock `--locked`; both repositories' commits, locks and file sets are recorded and verified, and packages both locks share must resolve the same.
  4. Machines: "Single-machine baseline mode (Recommended)", with a positive control in the same run (the same viewer with the resident stopped must not reach ready). The two-machine co-op is not brought up now.
  5. Home: "Harness in Turnstone, new Turnstone plan (Recommended)". This plan.
  6. Owner platform: "Owner may be Linux or macOS, any arch (Recommended)", recorded in the receipt.

  The seventh (the custody fixture's S79 receipt note) belongs to Graphshell's document that cites it.

## Phases

### H1. Fixture custodian (Graphshell)
`host/ports/mere-resident/examples/coop_fixture_custodian.rs` serves the fixture over a new synthetic passphrase vault for one profile, reading the passphrase from the first line of stdin, and prints its client file. **Done when** it builds `--locked` from `host/` and a production `passphrase-fd` Djinn reaches `custody reached ... in_process=false` through it in a headed run (H6).

### H2. The grant from Graphshell's code (Graphshell)
`coop_existing_graph grant` prints `grant_for` over `graphshell_door::declaration::declaration()`, peer none. **Done when** the worker writes exactly that grant into `applications` of the owner settings before Djinn starts, saves it as `owner-grant.json` with its origin, and the seed and the viewer are admitted under it (H6).

### H3. Two-repository build and freeze
`freeze.py` reads the graphshell.git pin from Turnstone's lock, clones Graphshell at it when the checkout is absent (an existing checkout is only checked), builds the viewer here and Djinn (alone), the seeder and the custodian in `host/`, and records both commits, lock hashes, file sets, Git suppliers, Djinn's resolved features and the shared-lock comparison in `build-inputs.json`. **Done when** a freeze on the run machine passes with no scratch controller and refuses a Djinn resolved with `in-process-custody`.

### H4. Baseline mode and positive control
`run.py --mode baseline` drives the owner worker alone: custodian and Djinn start, seed, the baseline scenario (await `status=ready attached=<chosen>`, switch to the private graph and back, two captures), Djinn stops, then the same viewer, profile and domain as the control. The packet is `RUN/receipt`. **Done when** the control is recorded as failing with `status expected "ready"` in the viewer's own `await-failed` field and no window reaches ready.

### H5. Verifier
`verify.py` gains a baseline branch: custody mode, the grant's origin and its presence in the settings, both repositories' commits and lock hashes against the pin, host files against a Graphshell clone (`--host-repo`), owner OS and architecture (Linux or Darwin, any), the baseline and control receipts, cleanup and the capture review. The co-op branch accepts a Linux owner and checks host inputs when a packet carries them. **Done when** it passes the H6 packet and prints the owner platform, both commits and both lock hashes.

### H6. First headed baseline receipt
**Done when**, on one Linux or macOS desktop with no scratch controller: the harness's entry points freeze both repositories; the receipt is `ok` (resident ready, seed through the door as `graphshell`, viewer at `status=ready attached=<chosen>`, the switch, two captures over 200 by 200 pixels); the positive control fails as expected in the same run; the receipt names its custody mode, the grant's origin, both commits and lock hashes and the owner's OS and architecture; `verify.py` passes it; the captures are reviewed whole; README and this plan match the tree.

### H7. Two-machine co-op phases (not now)
`direct`, `relay` and `recovery` carry the custody, grant and build changes but have not run since 2026-10-24. **Done when** a Mac or Linux owner and the Linux guest pass the full workflow and `verify.py`, or the drifted phases are recorded as findings.

## Findings

- 2026-11-11: a `passphrase-fd` Djinn without `in-process-custody` reaches only the custodian named by `--custodian` (dramatis `crates/dramatis-resident/src/startup_vault.rs`, `open_custody`), and refuses a custodian whose current profile is not `--profile`; the fixture's first profile is the run's profile (`CrossGraphOwner`).
- 2026-11-11: building the examples in the same cargo invocation as Djinn would give Djinn `in-process-custody`, through `mere-resident`'s dev-dependency on itself (`host/ports/mere-resident/Cargo.toml`, dev-dependencies). `cargo tree --edges normal,build --depth 0 --format '{f}'` resolves `cxf,default,passphrase-fd,personal-sync`; with dev edges it adds `in-process-custody`. Djinn is built alone and the freeze checks the first.
- 2026-11-11: the two locks did not resolve their shared packages the same at Graphshell `9cd48e19` (host/Cargo.lock unchanged since `893818f6`) against Turnstone `0c1bb869`: 19 of 1,663 shared resolved packages differ (Mere `a9a125f2` against `325b0083`, Genet `3fa3ec62` against `7c6ffc87`, Distillery `8908f7e4` against `30bf5510`, wgpu-graft `67ebcc41` against `34af2599`). The shared custody pins agree (Armillary's `djinn`, Dramatis `30e9cc97`).

## Progress

- 2026-11-11: plan opened. H1 and H2 committed in Graphshell; H3 to H5, the README and this plan committed in Turnstone on `coop/harness`. Not yet run.
