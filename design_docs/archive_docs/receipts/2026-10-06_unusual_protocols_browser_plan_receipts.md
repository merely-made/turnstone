# Unusual-protocols browser plan receipts

Receipts for the archived [plan](../2026-11-02/2026-10-06_unusual_protocols_browser_plan.md) (archived 2026-11-02 under DOC_POLICY section 8: run evidence is kept apart from the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/turnstone/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/turnstone/` on 2026-11-02; the folders `receipts/`, `design_docs_receipts/` and `support_receipts/` there were tracked here as `docs/receipts/`, `design_docs/receipts/` and `support/receipts/` until the closure's first commit moved them byte for byte (SHA-256 checked).

- `receipts/browser_family_20261007/final-s0-audit.json` (line 1530): present
- `receipts/browser_scry_windows_20261005/README.md` (line 380): present
- `receipts/hest-cambium-adoption-20261020/qualification.json` (line 2356): present
- `receipts/hest-h3-20261020/qualification.json` (line 2421): present
- `receipts/native_browser_receiving_20261022/integrated-sdk.json` (line 2223): present
- `receipts/native_browser_receiving_20261022/product-build-failed-20261022T203202Z.json` (line 2279): present
- `receipts/native_browser_receiving_20261022/product-error-census-summary.json` (line 2281): present
- `receipts/native_browser_receiving_20261022/product-error-census.json` (line 2280): present
- `receipts/native_browser_receiving_20261022/product-graceful-build.json` (line 2287): present
- `receipts/native_browser_receiving_20261022/product-graceful-tests.json` (line 2288): present
- `receipts/native_browser_receiving_20261022/product-integrated-build.json` (line 2230): present
- `receipts/native_browser_receiving_20261022/product-integrated-tests.json` (line 2233): present
- `receipts/nomadnet_acceptance_20260911/README.md` (line 119): present
- `receipts/smolweb_acceptance_20260821/README.md` (line 117): present
- `support_receipts/client-limits/consumer.json` (line 2171): present
- `support_receipts/errand-adoption/qualification.json` (line 63): present

## Inline run evidence that stays in the archived plan

81 lines of the archived file mention a receipt. Sections that are run evidence:

- line 772: S4. Gopher and the small lanes qualified
- line 1965: 2026-10-09, qualified resource family and SC/S8 resumption
- line 2072: 2026-10-10, resumed qualification
- line 2106: 2026-10-22, native receiving continuation

## Old-to-main hash table (2026-11-02)

The body keeps the hashes it was written with (Mark, 2026-11-02: "Live docs: local + sibling"; archived docs keep theirs). Each row is a cited commit that is not on its repository's main, with the commit on main that has the same author, author time and subject, re-verified with `git show` and `git merge-base --is-ancestor` on 2026-11-02. Line numbers are in the archived file.

| cited | repo | on main | subject | lines |
|---|---|---|---|---|
| `8eebb5a3` | turnstone | `7299178a72` | Integrate current main cohort with projection reading controls | 10, 2218, 2269 |
| `a622c960` | turnstone | `2b3323f116` | Receive shared MIME sessions on current Turnstone cohort | 10, 2221 |
| `636e7bd7` | turnstone | `b8e05b77df` | Merge branch 'main' of https://github.com/merely-made/turnstone into dedup/map-c | 2161 |
| `6737085d` | turnstone | `283e9a68c6` | Apply configurable client receive limits to browser and Micron | 2172 |
| `6e5da6dc` | turnstone | `64dbf84d1d` | Align Turnstone with the receiving SDK type cohort | 2227, 2282 |
| `0dd9b1b` | turnstone | `60674460f0` | Keep compact headers compact and check float transfers proportionally | 2331 |
| `1d13185` | turnstone | `9b3b1387f5` | Pin the qualified retained stack and use painted geometry in fixtures | 2371 |
| `8e3e5cb` | turnstone | `c201e94b9b` | Merge retained-stack updates while preserving Cambium 0.5 adoption | 2372 |
| `965b64e2` | genet | `2d30a0d39c` | Align fontsan backend with Servo and test valid WOFF2 | 1526, 1658 |
| `b613fc55` | woodshed | `0433a21d5b` | Keep Redshank on bounded browser-input compatibility baseline | 98 |
| `91cb44a2` | knot-editor | `a0c9740df4` | Align qualified Knot consumer with bounded browser-input Mere source | 98 |
| `69a2383b` | genet | `450f45b694` | Merge font-backed host layout with qualified semantic diagnostics | 94, 449, 1408 |
| `bd5912fb` | mere | `d6e7601209` | Merge command menus and font measures with bounded diagnostics | 457 |
| `3d1cdacc` | mere | `a1d5b14811` | Preserve browser worker string errors during stable device initialization | 94, 103, 153, 180, 195, 344... |
| `9e982b88` | woodshed | `6e9de9d0bf` | Align Redshank contracts with stable Burn Mere baseline | 95, 353, 459, 1409 |
| `5919dc64` | mere | `62d1fdc932` | App composition brief (Turnstone U8): retained sessions joined as AccessKit subt | 1301, 1437 |
| `ea74604b` | mere | `d62930fbfd` | Stack seams round 26: S76 (publish gopher-protocol 0.2.0, then repin) and S77 (k | 514, 515, 582, 879, 1416, 1419... |
| `d851a9db` | genet | `a78fcb2e5a` | Hide what a person cannot perceive from taproot and the styled projection | 349, 449, 515, 1617 |
| `f1d169c7` | mere | `38a50a234b` | Carry foreign accessibility through native surface producers | 1614 |
| `57b4893d` | mere | `a2e0982899` | Merge current main and repin browser prerequisite to current Genet and Weld | 609, 1526, 1531 |
| `0096591a` | knot-editor | `bfcbbec67c` | Align Knot with current Mere and Genet family | 1527, 1535 |
| `82271df2` | woodshed | `1883457078` | Adopt the qualified current Mere and Genet family in Redshank | 1527 |
| `a59e4c47` | mere | `99c810f423` | Merge the session seam and E1a (Turnstone U15, composition brief AC2 to AC5) | 1469, 1532 |
| `392630bb` | mere | `82505ed7e3` | Merge qualified browser adapter maintenance ancestry without reverting main | 102, 396, 1145, 1408 |
| `4e57913e` | mere | `0d120140c1` | WS4 R0 to R5 landed (b4f14f2c): the fidelity plan, DOC_README and stack seams pl | 105, 1416 |
| `b4f14f2c` | mere | `bec65ece89` | Merge WS4 R0 to R5: smolweb renders through one path (rulings S66, S70 to S75) | 106, 195, 198, 396, 407, 1408... |
| `ef89a186` | knot-editor | `b33e0a9540` | Repin genet to d851a9db0cd and every Mere row to e0cea3e0 (mer3ly Rulings 111-11 | 348, 454, 1408 |
| `e0cea3e0` | mere | `e31ae2b862` | Merge remote-tracking branch 'origin/main' into genet-image-decode | 349, 518 |
| `8106c7c2` | mere | `b167fe46d2` | Share Mesquite deferred clicks with product scenario commands | 378 |
| `33cc855` | knot-editor | `db05d84804` | Receive Mere's S14 archive tails: Mere's Knot plan copies archived, these copies | 454, 1409 |
| `9310518b` | mere | `c719978109` | Add opt-in validated local lexical reference packs | 454 |
| `3dfb70b0` | knot-editor | `eae29d1352` | Merge Commands and typography with qualified catalog diagnostics | 458 |
| `cefc903d` | woodshed | `ebe1cc0fbe` | Qualify Redshank diagnostics on combined command and font pins | 458 |
| `6cb57f1` | knot-editor | `3a0b99ed99` | Merge qualified Knot maintenance ancestry while preserving current main inputs | 518 |
| `06c2b13` | woodshed | `51646b3c96` | Merge qualified Redshank browser-input compatibility ancestry | 520 |
| `5011e2f9` | mere | `b11fbdfcae` | Reveal nested horizontal targets through the shared host | 521 |
| `4ba47666` | mere | `a8133704ee` | L3: the persisted lock and the lock triggers (vault lock rulings 3, 5, 20, 32, 7 | 1387 |
| `463c8d40` | mere | `0d5e432138` | scenomise: a v2 history gets 4 MiB; a single dataset keeps 1 MiB | 1391, 1782, 1784, 1786 |
| `227062a9` | mere | `66a5dabe16` | Adopt qualified Genet boundary lookup repair coherently | 1392, 1780 |
| `e1bd641` | mere | `5338423513` | App composition brief: AC1 to AC6 ruled; smolweb fidelity plan receives Turnston | 1438 |
| `fde06dc` | mere | `2906a82ffa` | Smolweb fidelity plan: S8's smolweb and Micron accessibility projections go to T | 1450 |
| `f2ad105a` | mere | `88c6bbb90e` | document-lanes: smolweb sessions publish an accessibility projection (Turnstone  | 1479, 1532 |
| `35bd7d77` | mere | `9ed2eefcba` | Smolweb projection: skip blank blocks; NomadNet pages read with headings and lin | 1479, 1532 |
| `edf175f9` | mere | `538fe76312` | Keep browser adapter baseline on compatible font backend | 1502, 1503, 1533 |
| `24f196f4` | woodshed | `ca54c141a4` | Keep Redshank browser baseline on aligned font backend | 1502 |
| `d041cc69` | mere | `cd815443eb` | E1b probe: two sessions joined as subtrees in one window, for headed screen-read | 1507, 1533 |
| `35e61050` | mere | `5bb051fa56` | Lift the fetch vocabulary and a sans-IO page load into page-load | 1550 |
| `eca623bc` | mere | `2647ef3f5c` | Give pelt-core a host-loading mode over PageLoad | 1551 |
| `1360d771` | mere | `8aed5cdd95` | Merge pelt-host-load: page-load and pelt-core host loading (SC step 1) | 1551 |
| `06246afe` | mere | `edb55a961a` | Build Pelt's Livery receipt fixture paths portably | 1571 |
| `7e5ca10c` | mere | `7cbb8d64db` | Merge pelt-portable-fixtures: portable Livery receipt fixture paths | 1571 |
| `831b5a82` | mere | `55d3ef7b69` | Put documents and surfaces behind one routed PeltContent | 1578 |
| `938c64be` | mere | `d2e0d69dbf` | Merge pelt-one-controller: documents and surfaces behind one routed PeltContent  | 1578 |
| `16c1ef8d` | mere | `f9ff471671` | Scenograph editor plan: C1 rebased onto G3, verification and the S1 overlap chec | 1603, 1695 |
| `eabd443` | knot-editor | `eb4d38e5ac` | Lend the signing seed instead of copying it (Mere vault lock ruling 49) | 1612, 1622 |
| `14cd06e` | knot-editor | `8b1f6fd3c4` | Adopt shared foreign accessibility contracts coherently | 1613 |
| `965b64e` | genet | `2d30a0d39c` | Align fontsan backend with Servo and test valid WOFF2 | 1614 |
| `eabd4434` | knot-editor | `eb4d38e5ac` | Lend the signing seed instead of copying it (Mere vault lock ruling 49) | 1616 |
| `e536d2a3` | mere | `4204972d21` | Let the host keep a Pelt controller's history | 1628 |
| `c32e4c03` | mere | `83ac0de28a` | Merge pelt-host-history: host-kept history for Pelt controllers (SC step 3) | 1628 |
| `3ded2cd7` | mere | `a7e12bcaa6` | Merge pelt-session-seam: the controller's session seam for host features (SC ste | 1654, 1658 |
| `6e66f1ab` | knot-editor | `865591fdbf` | Repin Knot's Mere rows to 3ded2cd7 for the SC step 4 family | 1657 |
| `3ef71040` | woodshed | `ea3bb9ce7a` | Repin Redshank's Mere rows to 3ded2cd7 for the SC step 4 family | 1657 |
| `18404a4c` | mere | `8d1f617d45` | scenograph editor plan: SE51, and C2 landed | 1740 |
| `5bef84c0` | knot-editor | `4b3bba3ddf` | Repin Knot's Mere rows to 463c8d40 for Turnstone's C2 and vault lock family | 1784 |
| `b39a38d3` | woodshed | `654dcb4dfa` | Repin Redshank to Mere 463c8d40 and Genet 15713014 for Turnstone's C2 family | 1786 |
| `52fbe468` | woodshed | `1345a15f7a` | Adapt Redshank to Cambium's app-owned textboxes | 1786 |
| `019e07a0` | mere | `f41bd20de8` | cambium: keep app text fields separate from native form values | 1787 |
| `341d5ca` | knot-editor | `73da7ae2cf` | Adapt Knot focus and field styling to app-owned Cambium textboxes | 1790 |
| `0357b286` | mere | `c53fea3983` | pelt-core: content holds a surface the host spawned and routed itself (SC step 4 | 1860 |
| `ee0512d0` | knot-editor | `88706e1f49` | Repin Knot's Mere rows to 0357b286 for Turnstone's SC step 4 pass B | 1865 |
| `2f8ac103` | woodshed | `ad19a8397f` | Repin Redshank to Mere 0357b286 for Turnstone's SC step 4 pass B | 1865 |
| `e48db814` | mere | `c4a6f9d457` | Smolweb submissions are POSTs a host-history host completes (SC controller-level | 1910, 1919 |
| `16aaff78` | knot-editor | `e2d67625ea` | Repin Knot's Mere rows to e48db814 for Turnstone's controller-level input | 1918 |
| `ece6e9ea` | woodshed | `b64a86ad01` | Repin Redshank to Mere e48db814 for Turnstone's controller-level input | 1918 |
| `802238cb` | knot-editor | `f73db80700` | Repin Knot to qualified Mere graph semantics | 1967 |
| `e08bf4b` | woodshed | `11324f2991` | Adopt qualified Mere graph revision in Redshank | 1968 |
| `3b3afa289` | mere | `647a8fce27` | Record verified P2 main publication and final qualification receipts | 1969 |
| `b6b551d44` | mere | `4ebcca6a95` | dramatis repo plan: ruling D37 (DR-B makes the cut; callers break until DR-C, no | 2017 |
| `a0d95ebd6e2` | genet | `11d71282dd` | Implement native form constraint validation and feedback | 2017 |
| `faa6f1fb3` | mere | `42160ecb73` | Propagate host client budgets through fetch and document receiving | 2160 |
| `36bad79bcc0` | murm | `a744af24e0` | Record clean native client-limit qualification | 2161 |
| `1f660dcb` | mere | `83bece36f5` | Integrate concurrent Knot checkpoint receiving without changing native hosting | 2247 |
| `3596dce3` | knot-editor | `3f3882cfec` | Drain native client handoff before browser receiving probe | 2265 |
| `500bc78a7` | mere | `63d97969fe` | Use supported solid divider for resident overmap tier | 2291 |
| `0151950f8` | mere | `cbb2974b40` | Prepare Cambium 0.5 and align the embedded Knot requirement | 2351 |
| `65a6632` | knot-editor | `02dd767f63` | Allow the embedded Knot surface to follow Cambium 0.5 | 2351 |
| `54d864c2` | netrender | `687ffb9dc0` | Share tile membership between invalidation and retained rendering | 2374 |
| `6ef3cd37` | genet | `7c8539c1b9` | Merge remote-tracking branch 'origin/main' into codex/accesskit-025-20261020 | 2374, 2417 |
| `9ef5e3026` | mere | `c9033723b6` | Add Hest invocation and terminal outcomes with host-owned adapters | 2410 |
| `ebf10c741` | knot-editor | `0ef13ff943` | Reconcile current Knot site and renderer work before Hest invocation adoption | 2410 |
| `ec15eb26` | netrender | `d9d6e778df` | Borrow scene sources across Classic tile and retained lowering | 2417 |
| `e2f19a3f` | murm | `5b78c0e676` | Align the Eidetic embedding-contract supplier cohort | 2417 |

Several commits on main share the cited commit's author time and subject, so no single equivalent is claimed (left as text in the body):

- `cab743c1` (turnstone), candidates `8d22304270`, `d2c4abc522`, Drain native page proofs and link closure before teardown; lines 12, 2287
- `934494bf` (turnstone), candidates `215e39f9fe`, `351e813baa`, Preserve response MIME and qualify native browser receiving; lines 2221
- `1e5c7f47` (turnstone), candidates `8d22304270`, `d2c4abc522`, Drain native page proofs and link closure before teardown; lines 2221
- `dddbc94e9` (turnstone), candidates `215e39f9fe`, `351e813baa`, Preserve response MIME and qualify native browser receiving; lines 2284
- `db4ee312` (mere), candidates `0cbdebd65f`, `c7aa7e571c`, Preserve ordered Graft host delegate events; lines 97, 104, 399, 520
- `af5c3ba85` (mere), candidates `3542263ca3`, `96f490ae26`, Preserve smolweb response MIME through document sessions; lines 2225, 2291

No equivalent exists on the repository's main (the commit is on no branch, or only on an unmerged one); left as text in the body:

- `679d8314` (genet), Align qualified font sanitizer backend with Servo host; lines 1502
- `211ff57a` (knot-editor), Keep Knot browser baseline on aligned font backend; lines 1502
- `f68af0d` (knot-editor), Repin every Mere row to ea74604b, with stack seams S77's preview chang; lines 1617
