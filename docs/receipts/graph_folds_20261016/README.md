# Turnstone native graph folds — 2026-10-16

The primary Orrery graph supports native selection folds, badge expansion,
expand-all, and session-local undo/redo. Native FoldRecord v2 records persist in
`sessions/<id>/folds.json`, separate from graph truth and arrangement facets.
Curation belongs to the graph runtime/session; panes sharing that runtime share
its fold set. Commands resolve the owning graph pane and its session rather than
the runtime pool's compatibility cursor. Primary and lens pointer release share the same
fold persistence boundary.

Source: `f74fb679f169106cb52d85777b93586d562f1230`. Turnstone was fetched and
fast-forwarded thirteen commits from `79f5f87` to `aaad30d` before implementation.
Canonical supplier pins stayed unchanged: Mere `65b97b0bfdf1eef5ba3b4ecdccba73f47a049d03`
and Genet `e37762d7` (full source identities in `inputs.json`). Adding the direct
`mere-curation` dependency changes only Turnstone's root dependency list in the
lockfile. Runtime source hashes match the recorded Git blobs; supplier tracked
trees were clean. `source-audit.json` also binds tracked auxiliary assets after
the run. The later receipt/plan commit changes no runtime source.

## Acceptance

- Focused fold-named suite: **13 passed**, including ten new app/sidecar tests.
- Full workspace: **958 passed, zero failed, ten ignored** (`--locked --offline`, serial).
- Native build: `cargo build --locked --offline --bin turnstone -j4` passed.
- Headed create/history/badge run: **288 presented frames, six captures**.
- Fresh ordinary app process on the same isolated profile: **91 presented
  frames, three captures**; exact native records restored, then expand,
  undo/redo and expand-all saved the expected record sets.
- Both processes returned `RESULT ok`, with successful launcher exit, no
  timeout and zero blank readbacks. All **nine 1024×600 PNGs** were individually
  reviewed. Source members, both summary bodies and their `+2` badges are
  framed and legible. See `visual-review.json`.
- Twelve checkpoints read the live graph, all member positions, native routes
  and actual stored fold sidecar. The fixture is an ordinary **New session**
  action followed by six `mere://` members with five navigation relation cells.
  No raw profile edit creates this graph.
- The complete graph snapshot at a fixed serialization date, every source
  member position, and the persisted graph/facet file bytes agree across all
  twelve checkpoints, including fresh reopen. All event times remain in the
  snapshot; only its serialization date is explicitly zero via the native
  `to_snapshot_at(0)` API.
- The two folds hide four source members behind two summaries, leaving two
  outside members visible. Their two directed boundary cells form two bundles;
  an independent verifier recomputes every checkpoint's counts from the raw
  source relations and the complete fold membership map. The outside-to-outside
  relation remains ordinary graph content.

The generated public custody fixture uses the normal Djinn broker and session
storage path. The exact frozen binary, scenarios, local driver, helper and
launcher hashes are recorded in `inputs.json`. Neither product process, its
custody helper, nor the bounded display-sleep assertion remains running. All
four original helper input files retain their hashes. Generated profile/vault
contents and build binaries remain outside this packet.

## Integration findings

Visual review caught a production `FitView` problem: changing the runtime camera
without stashing the pane viewport let the next render restore the old camera
and crop fold bodies. The command now uses the owning pane's view boundary;
a regression test and the final captures qualify it.

Fresh reopen restored fold records but initially restarted physics. View intent
now persists an optional explicit pause choice, applied after layout and scene
restoration. Existing sidecars with no pause field preserve their prior startup
behavior. The final run reopens paused and retains all six positions exactly.

Unreadable/future envelopes and interrupted replacements remain protected from
ordinary saves. Stale, foreign, future and overlapping records are retained in
a separate refused-record list. Expanding an accepted fold cannot cause a
previously refused overlap to become active at the next reopen.

The earlier starter-graph probe is excluded from acceptance. Its reopen sentinel
and process outcomes are preserved under `diagnostics/`; its missing pause and
cropped camera motivated the two integration repairs above. The starter graph's
legacy unprofiled snapshot also re-derives eleven Domain containment cells on
load, an existing normalization behavior outside fold curation. The final new
session avoids that migration and preserves its full source reading.

Cargo emits a nonfatal diagnostic while traversing an upstream Servo lint-test
manifest (`foo = true`) and existing supplier/deprecation warnings. The final
compiler/test exit codes, summaries and raw logs determine the software result;
these diagnostics have not been edited out of the logs.

## Rechecking the packet

Run from the Turnstone checkout with Python and Pillow available:

```sh
python3 docs/receipts/graph_folds_20261016/verify.py docs/receipts/graph_folds_20261016
```

The verifier checks the recorded source blobs and current source files, both
process outcomes, twelve full source/position/storage checkpoints, exact restored
records, directed boundary accounting, nine nonblank images and helper cleanup.
`sha256.json` binds the completed packet, including byte-preserved raw logs.
`run_native.py` is the exact local execution driver; `run_macos_scenario.py` is
the frozen LaunchServices launcher. `seed-config.json` and `reopen-config.json`
record their exact environments and arguments. Builds use the canonical lockfile
and the repository's build configuration, without local supplier patches.

This qualifies the default macOS primary Orrery consumer. Gloss and Overmap are
separate projection consumers. Windows/Linux presentation, physical GPU recovery,
headed lens interaction and human assistive-technology checks retain their own
gates. See `design_docs/2026-08-08_pane_registry_and_graph_panes_plan.md`, section
“Native graph folds”.
