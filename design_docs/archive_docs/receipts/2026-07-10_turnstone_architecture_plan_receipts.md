# Turnstone architecture: obviating meerkat receipts

Receipts for the archived [plan](../2026-11-02/2026-07-10_turnstone_architecture_plan.md) (archived 2026-11-02 under DOC_POLICY section 8: run evidence is kept apart from the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/turnstone/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/turnstone/` on 2026-11-02; the folders `receipts/`, `design_docs_receipts/` and `support_receipts/` there were tracked here as `docs/receipts/`, `design_docs/receipts/` and `support/receipts/` until the closure's first commit moved them byte for byte (SHA-256 checked).

- `receipts/embedding_recall_20261020/` (line 947): present
- `receipts/embedding_recall_20261020/qualification.json` (line 950): present
- `receipts/resident_inference_i5_windows_20261016/native/` (line 925): present

## Inline run evidence that stays in the archived plan

53 lines of the archived file mention a receipt. Sections that are run evidence:

- line 934: Embedding recall headed continuation (2026-10-20)
- line 1258: Native frame qualification (2026-10-30)

## Old-to-main hash table (2026-11-02)

The body keeps the hashes it was written with (Mark, 2026-11-02: "Live docs: local + sibling"; archived docs keep theirs). Each row is a cited commit that is not on its repository's main, with the commit on main that has the same author, author time and subject, re-verified with `git show` and `git merge-base --is-ancestor` on 2026-11-02. Line numbers are in the archived file.

| cited | repo | on main | subject | lines |
|---|---|---|---|---|
| `ec9881ea` | turnstone | `349e2fb243` | Reopen the retained answer pane with its origin offline | 919 |
| `a048adc` | turnstone | `3f8047916a` | Merge remote-tracking branch 'origin/main' into HEAD | 1000 |
| `eb336b7` | mere | `9618a94fc2` | canvas: center_on_selected — the restore framing | 296 |
| `a32443a` | mere | `6d4648acdb` | session-runtime: content_on rides the browser-state sidecar | 443 |
| `890c4e86612` | genet | `cdd327be90` | Point misfin and the cambium name-claims at live repos | 471 |
| `271e79d` | mere | `10fb8098fc` | Point Meerkat UI at Cambium | 619 |
| `c80c78d` | genet | `e2b8d402e7` | checkpoint 2026-07-10: in-flight wpt harness + layout + css-animations + scripte | 721 |
| `65b97b0b` | mere | `79dd4b159c` | Publish retained inference sources through the admitted app route | 920 |
| `67a514e8` | mere | `3cc79d9d17` | Add native resident fixture for headed Turnstone inference | 920 |
| `125df8df6` | mere | `aa19a8ebfb` | Record Hest H3 Git delivery and fresh remote source resolution | 937 |
| `34105d63` | eidetic | `937d777ac6` | Define exact embedding space and corpus compatibility envelopes | 938 |
| `6d371901` | esp | `b290346e51` | Validate guarded query vectors and stored sparse capability | 938 |
| `7edfe3b09` | mere | `82522850e8` | Consume the shared Apparatus core through the existing Mere facade | 975 |

Several commits on main share the cited commit's author time and subject, so no single equivalent is claimed (left as text in the body):

- `f7b3c53` (genet), candidates `2e022a9a9c`, `976a127fcf`, serval-layout: multi-root documents lay out, style, and paint every ro; lines 728

No equivalent exists on the repository's main (the commit is on no branch, or only on an unmerged one); left as text in the body:

- `a4356b74` (woodshed), Separate Redshank playback contracts from native audio; lines 1205
