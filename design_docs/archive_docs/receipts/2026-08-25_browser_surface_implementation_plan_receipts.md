# Browser surfaces implementation plan receipts

Receipts for the archived [plan](../2026-11-02/2026-08-25_browser_surface_implementation_plan.md) (archived 2026-11-02 under DOC_POLICY section 8: run evidence is kept apart from the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/turnstone/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/turnstone/` on 2026-11-02; the folders `receipts/`, `design_docs_receipts/` and `support_receipts/` there were tracked here as `docs/receipts/`, `design_docs/receipts/` and `support/receipts/` until the closure's first commit moved them byte for byte (SHA-256 checked).

- `p4_content2` (line 950): present
- `p4_repoint` (line 922): present
- `receipts/browser_family_20261007/README.md` (line 173, 352): present
- `receipts/browser_scry_windows_20261005/README.md` (line 219): present
- `receipts/browser_supplier_integration_20261006/README.md` (line 9, 207): present
- `receipts/browser_supplier_integration_20261006/accessibility-reconciliation/README.md` (line 284): present
- `receipts/browser_supplier_integration_20261006/current-stack-reconciliation/README.md` (line 326): present
- `receipts/browser_supplier_integration_20261006/dependency-currency/README.md` (line 268): present
- `receipts/browser_supplier_integration_20261006/fontsan-compatibility/qualification-summary.json` (line 241): present
- `receipts/browser_supplier_integration_20261006/fontsan-conflict-tree.log` (line 236): present
- `receipts/browser_supplier_integration_20261006/sync-diagnostic/README.md` (line 255): present
- `receipts/browser_supplier_integration_20261006/turnstone-all3-build.log` (line 233): present
- `receipts/cue_geometry_linux_20261027/README.md` (line 55, 100): present
- `receipts/cue_ime_linux_20261024/README.md` (line 84): present
- `receipts/cue_native_linux_20261023/README.md` (line 116): present
- `receipts/cue_password_geometry_linux_20261027/README.md` (line 34): present
- `receipts/cue_scroll_repair_20261023/README.md` (line 1140): present
- `receipts/cue_taffy_geometry_linux_20261029/receipt.json` (line 29, 1166): present
- `receipts/cue_text_input_20261024/README.md` (line 95): present
- `receipts/cue_ui_20261023/README.md` (line 1021): present
- `receipts/cue_ui_linux_20261023/README.md` (line 1100): present
- `receipts/servo_cue_20261023/README.md` (line 1003): present

## Inline run evidence that stays in the archived plan

64 lines of the archived file mention a receipt. Sections that are run evidence:

- line 104: Linux native Cue receiving, 2026-10-23
- line 189: Windows Scry B1 qualification, 2026-10-06
- line 965: Native custody and shared controls

## Old-to-main hash table (2026-11-02)

The body keeps the hashes it was written with (Mark, 2026-11-02: "Live docs: local + sibling"; archived docs keep theirs). Each row is a cited commit that is not on its repository's main, with the commit on main that has the same author, author time and subject, re-verified with `git show` and `git merge-base --is-ancestor` on 2026-11-02. Line numbers are in the archived file.

| cited | repo | on main | subject | lines |
|---|---|---|---|---|
| `5e9e3bfea88e9e9b9133d01ec9486ed7fd056322` | turnstone | `058130f45f0fb39d6d2f6ef3e04562db321d5dbb` | Adopt Cue in Turnstone's content pool, windows and Inspector | 1019 |
| `86ccb619ba506d2a17d3fea0f26d0cf82566abd0` | turnstone | `6babba5e0e9b485028620384c2518937e7622c98` | Qualify Turnstone Cue UI receiving on Metal | 1061 |
| `b4628119` | turnstone | `c668f7bf50` | Repin to the cohort: eidetic aa94fe22, gemot 8ffdc9a5 | 1064 |
| `965b64e2` | genet | `2d30a0d39c` | Align fontsan backend with Servo and test valid WOFF2 | 174, 329 |
| `69a2383b` | genet | `450f45b694` | Merge font-backed host layout with qualified semantic diagnostics | 234 |
| `dec11bbd` | wgpu-graft | `b25df3b706` | Fix Servo ANGLE swaps and qualify GPU frame delivery | 247 |
| `39818a7` | wgpu-scry | `557da58f69` | Drain coalesced WGC arrivals to the newest queued frame | 195 |
| `4784d07` | wgpu-weld | `dfdbe24963` | Fix portable CEF script dispatch | 195 |
| `114f4ea26` | mere | `5d54a141a6` | Receive Vulkan Cue 0.1.2 and shared Genet GPU construction | 117 |
| `57b4893d` | mere | `a2e0982899` | Merge current main and repin browser prerequisite to current Genet and Weld | 174, 329 |
| `0096591a` | knot-editor | `bfcbbec67c` | Align Knot with current Mere and Genet family | 174, 334 |
| `82271df2` | woodshed | `1883457078` | Adopt the qualified current Mere and Genet family in Redshank | 175, 335 |
| `bb48281b` | wgpu-graft | `e3bcbb4fb0` | Trace shared DX12 resource descriptor for Servo controls | 175, 339 |
| `2c3ebd24` | wgpu-scry | `ee73a2496e` | Restore capture harness after freezing Mac visibility trials | 175, 331 |
| `c4dd593b` | wgpu-weld | `d728db0172` | Update CEF runtime to stable 154 and qualify Windows sandbox import | 175, 330 |
| `01f3c9f` | wgpu-graft | `02ddc11ef4` | Restore Servo pre-present imports at an immutable source | 195 |
| `a59e4c47` | mere | `99c810f423` | Merge the session seam and E1a (Turnstone U15, composition brief AC2 to AC5) | 327 |
| `7907ff29` | wgpu-graft | `285ff4dd29` | Update Servo integrations to immutable v0.7.0 | 336 |
| `0e031fa5` | mere | `2e9c07168a` | Rename the platform's denizen to participant; add servitor resident runs | 421 |
| `8610058b` | knot-editor | `365afad13d` | Align knot-editor to mere 0e031fa5 and genet 9976945 | 421 |
| `f5a66493` | woodshed | `d869c75b7d` | Repin Redshank at mere 0e031fa5 and genet 9976945 | 421 |
| `94a4dc4155b` | genet | `0e976a3d0d` | Expose the Livery Parley fork to consumers | 709, 716, 821 |
| `11d996a6` | mere | `3b6cf5d6eb` | Record Knot invocation and read-only gates | 716 |
| `6f1a4fe70` | netrender | `bd14f7be8e` | Integrate the published 0.1.2 renderer stack | 716, 727, 823 |
| `a7d410c1ca8` | genet | `d886705cbf` | Keep current Taffy fork after release merge | 719, 830, 834 |
| `caaa6dd2cdaf` | mere | `2242d0030f` | chore(deps): align Genet capability source | 747, 869 |
| `633c7afc8a5a` | genet | `b7e276d80e` | feat(inker): report document control capabilities | 748, 866 |
| `da8762fd910` | genet | `e4f84037c0` | inker: add correlated page capture contract | 752, 875 |
| `844f949a9f` | wgpu-weld | `7c36a64bb7` | welding: correlate admitted PNG snapshots | 754, 876 |
| `42941fe18bfa` | genet | `c34cd5fe93` | Add retained document find capability | 840 |
| `138b6aca6e27` | genet | `b140743f34` | chore(deps): pin Netrender source revision | 858 |
| `d7cd4a87f782` | mere | `9a5dc5465f` | chore(deps): follow immutable Genet source | 862 |
| `115d348dedd` | genet | `4b6108f228` | Widen the ortet forbidden list as the last commit claimed, and correct its comme | 885, 932 |
| `b57d2021bac` | mere | `6cb02e6720` | Refresh the Cambium catalog receipts and pin them to LF | 885, 901, 928 |
| `3c2d30d69de` | knot-editor | `9918ba8d84` | Repoint to genet and mere after the boundary moves | 886, 901 |
| `d82afa17e2c` | mere | `d7dbf6b6e7` | Generate the Circuit workspace graph instead of committing it | 902, 924 |
| `fcd004b655b` | knot-editor | `cce6d605b0` | Pin mere at d82afa17, the P4 family revision | 925, 929 |
| `d82afa17` | mere | `d7dbf6b6e7` | Generate the Circuit workspace graph instead of committing it | 930 |
| `1c1d1b29` | mere | `2e66dbcf56` | Stop collecting image sources as outgoing document links | 935 |
| `737fec2df` | mere | `9e9acc6ed5` | Refuse retained input while a shared surface is hidden | 1002 |
| `86796a2447d68b3e6538d0ceff9f387a5d56e29c` | mere | `1ed26df1eee5e2d60e6b5e3466bb6741de26ad3e` | Drain shared controls without invalidating retained publications | 1020, 1062 |
| `b0e7647f` | mere | `7d9aa60c54` | Disambiguate HTML atom strings for the current Genet cohort | 1127 |
| `8a3d289a` | netrender | `66b2f72e9e` | Replace a cached image when its Blob identity changes under a reused ImageKey | 1128 |

Several commits on main share the cited commit's author time and subject, so no single equivalent is claimed (left as text in the body):

- `db4ee312` (mere), candidates `0cbdebd65f`, `c7aa7e571c`, Preserve ordered Graft host delegate events; lines 194

No equivalent exists on the repository's main (the commit is on no branch, or only on an unmerged one); left as text in the body:

- `1d44e5dd` (genet), script_webgpu: Enable webgpu feature on bindings; lines 270
- `aac43a3f` (genet), deps: Use released version of stylo; lines 146, 337
- `679d8314aab4ec9f57a903c79dde244c3c565c1e` (genet), Align qualified font sanitizer backend with Servo host; lines 239
