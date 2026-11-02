# Turnstone browser and resource integration handoff receipts

Receipts for the archived [plan](../2026-11-02/2026-10-08_browser_resource_handoff.md) (archived 2026-11-02 under DOC_POLICY section 8: run evidence is kept apart from the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/turnstone/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/turnstone/` on 2026-11-02; the folders `receipts/`, `design_docs_receipts/` and `support_receipts/` there were tracked here as `docs/receipts/`, `design_docs/receipts/` and `support/receipts/` until the closure's first commit moved them byte for byte (SHA-256 checked).

- `integration-completion-20261011/gap-audit.md` (line 746): not found at this path
- `integration-completion-20261011/graphshell-native-web-guard-v1-root-review.json` (line 766): not found at this path
- `integration-completion-20261011/progress.md` (line 744): not found at this path
- `integration-completion-20261011/source-work-checkpoint-20261011.json` (line 766): not found at this path
- `receipts/browser_family_20261007/README.md` (line 55): present
- `receipts/thinkpad_checkout_sync_20261008/README.md` (line 99): present
- `receipts/thinkpad_trio_archive_20261008/` (line 192): present
- `receipts/thinkpad_trio_archive_20261008/README.md` (line 108): present

## Inline run evidence that stays in the archived plan

33 lines of the archived file mention a receipt. Sections that are run evidence:

- line 273: Preparation completion receipt
- line 387: Graph-origin preparation completion receipt (2026-10-09)
- line 455: Qualified graph adoption in progress, 2026-10-09
- line 487: Native integration corrections, 2026-10-09
- line 522: Native integration completed, 2026-10-09
- line 601: Controller pumping qualification, 2026-10-09

## Old-to-main hash table (2026-11-02)

The body keeps the hashes it was written with (Mark, 2026-11-02: "Live docs: local + sibling"; archived docs keep theirs). Each row is a cited commit that is not on its repository's main, with the commit on main that has the same author, author time and subject, re-verified with `git show` and `git merge-base --is-ancestor` on 2026-11-02. Line numbers are in the archived file.

| cited | repo | on main | subject | lines |
|---|---|---|---|---|
| `965b64e206a47d1c8808472de9aa461233638768` | genet | `2d30a0d39c780af3f12a52dfc9ddbcbe5037d9be` | Align fontsan backend with Servo and test valid WOFF2 | 76 |
| `3ded2cd7c2370713118a2440c962c39262720205` | mere | `a7e12bcaa606df5d9c5c7aa9a685131ce2bee693` | Merge pelt-session-seam: the controller's session seam for host features (SC ste | 75 |
| `c4dd593b` | wgpu-weld | `d728db0172` | Update CEF runtime to stable 154 and qualify Windows sandbox import | 107 |
| `16aaff78` | knot-editor | `e2d67625ea` | Repin Knot's Mere rows to e48db814 for Turnstone's controller-level input | 468 |
| `ece6e9ea` | woodshed | `b64a86ad01` | Repin Redshank to Mere e48db814 for Turnstone's controller-level input | 470 |
| `802238cb` | knot-editor | `f73db80700` | Repin Knot to qualified Mere graph semantics | 7 |
| `e08bf4b` | woodshed | `11324f2991` | Adopt qualified Mere graph revision in Redshank | 7 |
| `3b3afa289` | mere | `647a8fce27` | Record verified P2 main publication and final qualification receipts | 6, 467 |
| `6e66f1abddd382cc50543e6db7e2b02537018fd1` | knot-editor | `865591fdbf13767b0b850f05f6994efc579a7e5c` | Repin Knot's Mere rows to 3ded2cd7 for the SC step 4 family | 77 |
| `3ef710406546241432b50df8fc27a4b93d915454` | woodshed | `ea3bb9ce7aff8b59e3a49eae33eaf861bacc853a` | Repin Redshank's Mere rows to 3ded2cd7 for the SC step 4 family | 78 |
| `bb48281bd4a88a726a009e0518706881b74b7604` | wgpu-graft | `e3bcbb4fb07c05ea41564fefc54d2fe809971072` | Trace shared DX12 resource descriptor for Servo controls | 80 |
| `c4dd593b7a730acb4aa6ffab0fe2edf8a0584083` | wgpu-weld | `d728db0172bd5787047cc4fee0c991e4f9796d21` | Update CEF runtime to stable 154 and qualify Windows sandbox import | 81 |
| `2c3ebd24118851cf6125cfb193f55aab20e3ad67` | wgpu-scry | `ee73a2496ed3595a64a107fd45ab60c210dc03f3` | Restore capture harness after freezing Mac visibility trials | 82 |
| `0b026a44` | wgpu-graft | `91b4cf50ec` | Record passive Intel session outcome and remaining hardware gate | 107 |
| `dc9bd1a0` | wgpu-scry | `8778589b9f` | Record Intel session visibility failure and recovered release bookkeeping | 107 |
| `72c68b6d6f6a5c9023999d9fdaf4b3db632e6874` | mere | `000d9519c715652e1f484ad74f283309835f846e` | Qualify resource graph replay and reader controls | 118 |
| `c49e3e0b` | mere | `0b5cd79c0f` | Save unqualified RDF envelope handoff | 127, 134 |
| `72c68b6d` | mere | `000d9519c7` | Qualify resource graph replay and reader controls | 134, 304 |
| `e48db81404eb580b7a7b3c01caeb410e539da4c8` | mere | `c4a6f9d457bd4f72aad7faec529854b4119155a3` | Smolweb submissions are POSTs a host-history host completes (SC controller-level | 416 |
| `15713014e2e23b887360471552f75f60684f5384` | genet | `e27294d1327a7c8bc33764e3da95899ff6ae21c5` | Index retained paragraph boundary ownership without changing break policy | 417 |
| `16aaff7804d49ba829af646c3a8eca2d6ec2fc2f` | knot-editor | `e2d67625ea46545ced3bb6911bbfab844ce9b72f` | Repin Knot's Mere rows to e48db814 for Turnstone's controller-level input | 418 |
| `ece6e9eaf7c11a37f4b1457895704b1e6aeb6b8c` | woodshed | `b64a86ad01f96530b85b8b4ae8a03a29a28c1a6e` | Repin Redshank to Mere e48db814 for Turnstone's controller-level input | 419 |
| `526f2ddb5f7d9e46a5f6670b2f9c7d48078152e9` | mere | `eb051f8442a482593cdfbbc95bcdfea9ca59144d` | Merge qualified identity and physics-channel main updates into graph semantics | 458 |
| `3b3afa2895424f4be4e1e5f23d48140bf24da5aa` | mere | `647a8fce27085ca341ea77a3df903ddd22056f98` | Record verified P2 main publication and final qualification receipts | 459 |
| `802238cb9aae33d5d0ce334c8adef60874e9a6bb` | knot-editor | `f73db8070058136a8017bf3504bb926a9d97cf5d` | Repin Knot to qualified Mere graph semantics | 532, 565 |
| `e08bf4b9875a7efb02b1c17bf9fac56708712cee` | woodshed | `11324f2991e6d570761eb35990018ee9fd72d575` | Adopt qualified Mere graph revision in Redshank | 534, 566 |
| `542ad5b4795e26e2a2ebc8f7699b7d780c2b3b8c` | genet | `795c656c17b062bd2332d53a661d26e71655ad54` | Merge remote-tracking branch 'origin/main' into codex/three-lanes-upstream-20261 | 708 |
| `4bc75583865c2fb862b7acd7fe16dc28eb591042` | genet | `8063e2861b8ce3d008c3d3ba5b654051dc5281e7` | Adopt complete producer-resource Netrender source | 709 |
| `df79e24feb8d157669bb7a94ff81e30cccfe569f` | mere | `97f8647411455142a84c97942c459cb0e3a74965` | Merge commit '4f70d5362f9c365b28caa5a8496755fab27a0eca' into codex/three-lanes-u | 711 |

Several commits on main share the cited commit's author time and subject, so no single equivalent is claimed (left as text in the body):

- `624e24c98d7cc84d941b0d1255c528d87dfb7ce5` (mere), candidates `491931dabe`, `4f019cf5f3`, Review shared semantic core against clip and live consumers; lines 712

No equivalent exists on the repository's main (the commit is on no branch, or only on an unmerged one); left as text in the body:

- `aac43a3f31a259f04a574f5ec4e959c943ad7cc7` (genet), deps: Use released version of stylo; lines 79
