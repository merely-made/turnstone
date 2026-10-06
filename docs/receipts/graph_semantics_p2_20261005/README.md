# Mere graph-semantics P2 consumer preparation, 2026-10-05

The authorized consumer slice is prepared in `behaviors.patch`, based on
Turnstone `2913a4f`. It is **unapplied** while Mere's new resource capture/read
APIs are under construction on `graph-semantics` over `e25e774b`. Production
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
These are **not compilation or executed regression receipts**.
Reproduce the preparation checks with
`python docs/receipts/graph_semantics_p2_20261005/verify_patch.py`; it verifies
the frozen source, reconstructed candidate and patch hashes, then checks
applicability and Rust parsing without writing production code.

Six proposed regressions cover shared canonical aliases versus an unrelated
surface, both resource-pair endpoint sets and self-pair deduplication, invalid
or unshown resource identifiers, shown-resource retarget/clear, legacy exact
surface/edge targets, and expanded scopes with no-self-wake. They use recorded
kernel replay writes rather than reopening a fixture-only mutation boundary.

The next gate requires Mere's exact reviewed API checkpoint, then compilation
and execution against that source. Full Turnstone integration additionally
requires a reviewed compatible dependency set; no dirty-main repin is implied.
Apply the patch only with that integration. The new kernel API cannot compile
against the current portable Mere pin.

No target, Cargo home, or worktree was created for preparation. The current
Turnstone source and unrelated `.github/` work are preserved.
An import used during patch preparation generated this receipt's ignored
`__pycache__/`. Automatic approval review rejected its cleanup with
`blocked by policy`; the small generated cache is retained, owned by this
consumer verification slice. No deletion retry or workaround was attempted.
