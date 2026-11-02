# Turnstone place port receipts

Receipts for the archived [plan](../2026-11-02/2026-07-28_turnstone_place_port_plan.md) (archived 2026-11-02 under DOC_POLICY section 8: run evidence is kept apart from the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/turnstone/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/turnstone/` on 2026-11-02; the folders `receipts/`, `design_docs_receipts/` and `support_receipts/` there were tracked here as `docs/receipts/`, `design_docs/receipts/` and `support/receipts/` until the closure's first commit moved them byte for byte (SHA-256 checked).

- `receipts/place_sharing_20261019/README.md` (line 1309): present

## Inline run evidence that stays in the archived plan

51 lines of the archived file mention a receipt. Sections that are run evidence:

- line 721: T3c. Render-free two-peer receipt - DONE 2026-08-01
- line 890: T5. Headed receipt
- line 950: T5a. Founder path and two-window proof (planned 2026-09-13; receipted 2026-09-13)
- line 1063: T5b. Shared address and refused write (planned 2026-09-14; receipted 2026-09-14)
- line 1159: T5c. Shared Knot document by projection (planned 2026-09-15; receipted 2026-09-15)

## Old-to-main hash table (2026-11-02)

The body keeps the hashes it was written with (Mark, 2026-11-02: "Live docs: local + sibling"; archived docs keep theirs). Each row is a cited commit that is not on its repository's main, with the commit on main that has the same author, author time and subject, re-verified with `git show` and `git merge-base --is-ancestor` on 2026-11-02. Line numbers are in the archived file.

| cited | repo | on main | subject | lines |
|---|---|---|---|---|
| `01658c7` | turnstone | `58dd48a471` | Seal retained clips through custody and qualify native producer transfer | 1343 |
| `6300d0da` | mere | `8a772211a4` | Name every sync lane: JoinedSpace requires a per-lane protocol id | 529 |
| `0db9d10d` | mere | `6e8198a792` | Pin mere-p2panda-net 0.7.4 with prompt idle sync shutdown | 1059 |
| `30c85702` | knot-editor | `11bd38c25c` | Align knot-document and the desktop app on the bumped mere revision | 1059 |
| `1a3dda4e` | mere | `dcdca8fbad` | Add a publish set to MootLanes so Gemot facts reach connected peers live | 1136 |
| `0eb684f7` | knot-editor | `7042a08df6` | Align knot-editor to mere 1a3dda4e | 1148 |
| `bce789e7` | mere | `22af447971` | Repin knot-editor at its mere 1a3dda4e alignment | 1149 |
| `a1c5772` | woodshed | `827fe95b7f` | Align the redshank port to mere 1a3dda4e | 1149 |
| `5ae30cad` | genet | `974f61622d` | Publish the taproot chain and tighten the seam pins it exposed | 1154 |
| `876320fd` | mere | `ef40e934a7` | Repin knot-editor at its genet 5ae30cad alignment | 1154 |
| `e0c0135` | knot-editor | `1d60526204` | Repin knot-editor at mere 876320fd so direct consumers hold one mere | 1154 |
| `f7262bb` | woodshed | `057ea9e1f7` | Align the redshank port to mere 876320fd | 1154 |
