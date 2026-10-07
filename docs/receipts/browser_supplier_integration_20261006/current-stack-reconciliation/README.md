# Current-stack remote checkpoint, 2026-10-07

The [new JSON checkpoint](remote-refresh-20261007.json) records read-only
fetches, exact refs, immutable manifest/document reads and ancestry checks.
It supplements earlier evidence; it does not replace a frozen qualification,
adopt paused work or establish contact with another owner. No source or Cargo
file changed, and no build or application ran.

| Published source | Observed revision | What moved / remaining gap |
|---|---|---|
| Mere main | `a59e4c47a2a5c2024427eeb54b689f2e4f2c345b` | R6 `1d87808a` is now an ancestor; gopher-protocol 0.2.0 is in the registry lock. Shared helper/session seam and E1a merged. Genet still pins `d851a9db`. |
| Genet main | `965b64e206a47d1c8808472de9aa461233638768` | Decoder repair is published, but absent from Mere's selected Genet; the repair descends from that older pin. |
| Knot main | `a41ef7cedfa639152d76a4bc8c1fd3596608152b` | Still Mere `e0cea3e0` / Genet `d851a9db`; the delta from `6cb57f10` changes five plans. |
| Knot seed-borrow | `f68af0dc441aa10fddc33b249f3e35e1f1a56cc4` | Live remote branch pins Mere `ea74604b`; it is not an ancestor of Knot main and is not the new Mere family. |
| Woodshed main | `06c2b13ce296c82fafc394e0c90da21458542803` | Nested Redshank workspace remains Mere `db4ee312` / Genet `69a2383b`; Woodshed root has a different pin scope. |
| Turnstone main | `032463aedd49234ef07050a8cb2035d39eb83bf1` | No published U15/U16/U17 receiving record or new coordinated tested-set landing. |

At this snapshot **U9's WS4 publication prerequisite is satisfied; its final
tested set is not**. Mere's [published app-composition brief](https://github.com/merely-made/mere/blob/a59e4c47a2a5c2024427eeb54b689f2e4f2c345b/design_docs/cambium_docs/research/2026-10-06_app_composition_brief.md#L591-L610)
records E1a's 27 model-tree and 18 real-session checks against consumers 0.35,
0.36 and 0.38. They were not rerun here. E1b's human screen-reader walks remain
open, as do actual foreign-browser exports, action coverage and host/native
acceptance. Successful pixels do not close those gates.

The same brief [records U15 and U17](https://github.com/merely-made/mere/blob/a59e4c47a2a5c2024427eeb54b689f2e4f2c345b/design_docs/cambium_docs/research/2026-10-06_app_composition_brief.md#L740-L752):
the unusual-protocols lane owns the shared helper/session seam/E1, and retained
sessions have their own leaf registries. Producers stay with the host device.
Its [smolweb plan records U16](https://github.com/merely-made/mere/blob/a59e4c47a2a5c2024427eeb54b689f2e4f2c345b/design_docs/nematic_docs/implementation_strategy/2026-07-01_smolweb_fidelity_plan.md#L50-L53),
assigning Smolweb/Micron accessibility projection to that lane. These are
Mere-recorded coordination facts; published Turnstone has not received the
records. AC6's future cross-process paint-list/accessibility channel remains
research. Our supplier lane consumes the shared helper and supplies browser
exports/actions.

The minimal proposed closure is one published Mere descendant that retains
R6/the helper and acquires the decoder repair, followed by an owner-qualified
Knot candidate, Redshank, and finally Turnstone on that same family. Direct,
nested and standalone manifest queries must be checked; matching top-level
pins alone cannot prove a single type family. This proposal is not an adopted
repin or a claim that unreachable-cloud coordination occurred.

The [published S0 plan](https://github.com/merely-made/turnstone/blob/032463aedd49234ef07050a8cb2035d39eb83bf1/design_docs/2026-10-06_unusual_protocols_browser_plan.md#L402-L419)
still requires the locked graph, full clean library suite, cargo-mode verify,
typed physics/projection refusals and matching pin documentation. Current
three-engine build/native coexistence, runtime-module identity, shutdown,
foreign-tree actions and three-platform AT remain separate owner gates.
Supplier CI/MSRV/hardware/package/registry-only release gates also remain
separate. Existing [browser qualification](../README.md) and
[accessibility inventory](../accessibility-reconciliation/README.md) retain
their exact earlier scope.
