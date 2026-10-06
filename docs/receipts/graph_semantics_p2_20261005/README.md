# Mere graph-semantics P2 consumer preparation, 2026-10-05

The authorized consumer slice is prepared in `behaviors.patch`, based on
Turnstone `2913a4f`. Its six bounded scope regressions now compile and pass
against reviewed committed supplier `cff35712a28419f921a3cb2ffe79ef02268af6c5` on
`graph-semantics`. The patch remains **unapplied** while a reviewed compatible
integration set and main integration review are pending. Production
source, portable pins and lock, and the frozen Scry native receipt are unchanged.
`preparation.json` records source/candidate/patch hashes and the limited checks.

## Contract and patch

`touched_ids(graph, delta)` stays exhaustive and returns owned surface strings.
Resource-record captures resolve their UUID through
`Graph::surface_ids_showing_resource`; exact resource-pair captures union the
two endpoint sets and deduplicate before the existing containment ancestry
walk. Invalid or unshown resource IDs produce no bare surface scope. Existing
surface captures keep their exact targets, including a shown-resource retarget
or clear. Record metadata is not inspected, and neither URLs nor current shown
resources are used to infer historical relation endpoints.

Mere owns the three appended capture variants, their stable serialization
ordinals, recorded resource writes, replay/migration, and current mapping reads.
Turnstone owns graph-event watch scopes and the ordinary behavior drain. This
patch retains its existing selected-graph authority and author/no-self-wake
protocol; it does not change graph-runtime or cross-graph watch policy.

## Checks and next gate

Rustfmt successfully parsed the candidate without reformatting the existing
file, and `git apply --check` passed against the unchanged production source.
These initial preparation checks alone are not compilation or regression proof.
Reproduce the preparation checks with
`python docs/receipts/graph_semantics_p2_20261005/verify_patch.py`; it verifies
the frozen source, reconstructed candidate and patch hashes, then checks
applicability and Rust parsing without writing production code.

Six regressions cover shared canonical aliases versus an unrelated
surface, both resource-pair endpoint sets and self-pair deduplication, invalid
or unshown resource identifiers, shown-resource retarget/clear, legacy exact
surface/edge targets, and expanded scopes with no-self-wake. They use recorded
kernel replay writes rather than reopening a fixture-only mutation boundary.

`consumer-gate.log` now records all six passing against real path-sourced
`mere-kernel` and `servitor`. `source-gate-inputs.json` records 209 source inputs,
all unchanged across the accepted run; `source-gate-result.json` binds the
fixture, patch, lock and output hashes. The fixture compiles the exact proposed
scope functions, ancestry walk and tests. It excludes the GUI and App drain,
so this is not full Turnstone integration. The unsuffixed artifacts qualify the
reviewed committed supplier, including its final checked-loader orphan/active
control; the supplier checkout is clean at that exact commit before and after
the run. The first six-pass attempt had two changed supplier
inputs; its separate `*-first` log/manifest/result remain an unaccepted source
checkpoint diagnostic. The second stable run is retained as `*-intermediate`;
the later owner-frozen WIP run accepted in Turnstone `feb2594` is retained as
`*-wip`. Each is evidence only for its own recorded inputs.

The actual API intentionally leaves a newly inserted surface unassociated.
Regression setup therefore replays explicit resource-record and shown-resource
writes; it never treats the current URL as an implicit association.

Reproduce the bounded gate with `verify_patch.py --prepare-source-gate`, then
`cargo test --manifest-path docs/receipts/graph_semantics_p2_20261005/consumer_gate/Cargo.toml --offline --locked --target-dir C:/t/cargo-targets/turnstone --lib`.
The fixture uses the recorded supplier worktree path. Recompute/compare input
fingerprints when that source changes; a later source is not this receipt.

Full Turnstone integration still requires a reviewed compatible dependency
set and main integration review; no dirty-main repin is implied. Mere's C13-C17
remain open, and its existing infallible snapshot wrappers/load boundaries
await C17. This consumer receipt does not close full P2 or those gates.
Apply the patch only with that integration. The new kernel API cannot compile
against the current portable Mere pin.

No isolated target, Cargo home, or worktree was created. The bounded test
reuses `C:/t/cargo-targets/turnstone`, retained for Turnstone repository reuse.
The documented `consumer_gate/` source fixture is retained as receipt evidence,
not as an additional checkout or build-output directory. The current
Turnstone source and unrelated `.github/` work are preserved.
An import used during patch preparation generated this receipt's ignored
`__pycache__/`. Automatic approval review rejected its cleanup with
`blocked by policy`; the small generated cache is retained, owned by this
consumer verification slice. No deletion retry or workaround was attempted.
