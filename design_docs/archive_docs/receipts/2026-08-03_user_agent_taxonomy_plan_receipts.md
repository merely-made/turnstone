# User-agent taxonomy â€” the things a browser owes its user, by spec receipts

Receipts for the archived [plan](../2026-11-02/2026-08-03_user_agent_taxonomy_plan.md) (archived 2026-11-02 under DOC_POLICY section 8: run evidence is kept apart from the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/turnstone/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/turnstone/` on 2026-11-02; the folders `receipts/`, `design_docs_receipts/` and `support_receipts/` there were tracked here as `docs/receipts/`, `design_docs/receipts/` and `support/receipts/` until the closure's first commit moved them byte for byte (SHA-256 checked).

- `receipts/browser_a11y_20261007/README.md` (line 617): present
- `receipts/browser_engine_inventory_20261005/README.md` (line 418): present
- `receipts/browser_main_integration_20261006/README.md` (line 13, 514): present
- `receipts/browser_scry_windows_20261005/README.md` (line 254, 439): present
- `receipts/browser_scry_windows_20261006/README.md` (line 12, 244, 478): present
- `receipts/browser_scry_windows_20261006/mere-weld-input-followup.md` (line 494): present
- `receipts/browser_supplier_integration_20261006/README.md` (line 15, 554): present
- `receipts/browser_supplier_integration_20261006/sync-diagnostic/README.md` (line 302): present
- `receipts/graph_semantics_p2_20261005/README.md` (line 460): present

## Inline run evidence that stays in the archived plan

27 lines of the archived file mention a receipt. Sections that are run evidence:

- line 333: Release qualification

## Old-to-main hash table (2026-11-02)

The body keeps the hashes it was written with (Mark, 2026-11-02: "Live docs: local + sibling"; archived docs keep theirs). Each row is a cited commit that is not on its repository's main, with the commit on main that has the same author, author time and subject, re-verified with `git show` and `git merge-base --is-ancestor` on 2026-11-02. Line numbers are in the archived file.

| cited | repo | on main | subject | lines |
|---|---|---|---|---|
| `b613fc55` | woodshed | `0433a21d5b` | Keep Redshank on bounded browser-input compatibility baseline | 545 |
| `91cb44a2` | knot-editor | `a0c9740df4` | Align qualified Knot consumer with bounded browser-input Mere source | 545 |
| `69a2383b` | genet | `450f45b694` | Merge font-backed host layout with qualified semantic diagnostics | 349, 546 |
| `dec11bbd` | wgpu-graft | `b25df3b706` | Fix Servo ANGLE swaps and qualify GPU frame delivery | 595 |
| `bd5912fb` | mere | `d6e7601209` | Merge command menus and font measures with bounded diagnostics | 349 |
| `39818a7` | wgpu-scry | `557da58f69` | Drain coalesced WGC arrivals to the newest queued frame | 246, 483 |
| `c36641d6` | mere | `7e0cf33e22` | Vault lock plan: rulings 1-4 (scope, Distillery's own transport key, triggers, u | 345 |
| `f9c8b0a7186` | genet | `e86fa4feb3` | Merge exact Vano repin acceptance receipt | 346 |
| `99c0a9d` | wgpu-scry | `465d9a48f7` | Integrate macOS capture frame-status diagnostics | 346 |
| `403a30c` | wgpu-graft | `25e03c3362` | Record grafting-frame release receipt [skip ci] | 347 |
| `65d057d` | wgpu-weld | `356ff0e756` | Adopt the canonical rustfmt policy | 347, 472 |
| `cff35712a28419f921a3cb2ffe79ef02268af6c5` | mere | `bb1388f55d519de1a0ff00207fb4e726e90e8e93` | Add resource graph storage, typed captures and checked snapshots | 463 |
| `4784d07` | wgpu-weld | `dfdbe24963` | Fix portable CEF script dispatch | 486 |
| `3d1cdacc` | mere | `a1d5b14811` | Preserve browser worker string errors during stable device initialization | 511, 524 |
| `9e982b88` | woodshed | `6e9de9d0bf` | Align Redshank contracts with stable Burn Mere baseline | 511 |
| `5919dc64` | mere | `62d1fdc932` | App composition brief (Turnstone U8): retained sessions joined as AccessKit subt | 581 |
| `1d87808a` | mere | `28c288fed0` | Merge WS4 R6: gopher-protocol 0.2.0 repinned (ruling S76) | 582 |
| `ea74604b` | mere | `d62930fbfd` | Stack seams round 26: S76 (publish gopher-protocol 0.2.0, then repin) and S77 (k | 582 |
| `d851a9db` | genet | `a78fcb2e5a` | Hide what a person cannot perceive from taproot and the styled projection | 585 |
| `f1d169c7` | mere | `38a50a234b` | Carry foreign accessibility through native surface producers | 606 |
| `14cd06e1` | knot-editor | `8b1f6fd3c4` | Adopt shared foreign accessibility contracts coherently | 611 |
| `317968ba` | woodshed | `0474131891` | Repin Redshank to qualified Mere accessibility family | 612 |

Several commits on main share the cited commit's author time and subject, so no single equivalent is claimed (left as text in the body):

- `db4ee312` (mere), candidates `0cbdebd65f`, `c7aa7e571c`, Preserve ordered Graft host delegate events; lines 544

No equivalent exists on the repository's main (the commit is on no branch, or only on an unmerged one); left as text in the body:

- `1d44e5dd` (genet), script_webgpu: Enable webgpu feature on bindings; lines 570, 597
- `1d44e5dd6a8b64c02f9dbf7fcbdf4ebdd0740019` (genet), script_webgpu: Enable webgpu feature on bindings; lines 401
- `f68af0dc` (knot-editor), Repin every Mere row to ea74604b, with stack seams S77's preview chang; lines 582
