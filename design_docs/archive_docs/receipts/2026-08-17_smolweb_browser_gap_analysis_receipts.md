# Browser gap analysis receipts

Receipts for the archived [plan](../2026-11-02/2026-08-17_smolweb_browser_gap_analysis.md) (archived 2026-11-02 under DOC_POLICY section 8: run evidence is kept apart from the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/turnstone/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/turnstone/` on 2026-11-02; the folders `receipts/`, `design_docs_receipts/` and `support_receipts/` there were tracked here as `docs/receipts/`, `design_docs/receipts/` and `support/receipts/` until the closure's first commit moved them byte for byte (SHA-256 checked).

- `receipts/nomadnet_acceptance_20260911/README.md` (line 97): present
- `receipts/smolweb_acceptance_20260821/README.md` (line 50): present

## Inline run evidence that stays in the archived plan

20 lines of the archived file mention a receipt. Sections that are run evidence:

- line 325: The gaps, ranked by how much they disqualify us today

## Old-to-main hash table (2026-11-02)

The body keeps the hashes it was written with (Mark, 2026-11-02: "Live docs: local + sibling"; archived docs keep theirs). Each row is a cited commit that is not on its repository's main, with the commit on main that has the same author, author time and subject, re-verified with `git show` and `git merge-base --is-ancestor` on 2026-11-02. Line numbers are in the archived file.

| cited | repo | on main | subject | lines |
|---|---|---|---|---|
| `1777b19840a6478a0faf3ab060a3ff71d5532321` | mere | `3884e62288cf4252e7d9ccbb7abdae68a05c28a0` | Add evidence-qualified Micron preview subset | 83 |
| `0c8cdefb17ffa86a4e6b2a508d9a0aeb8fd2ccac` | knot-editor | `f8605b024dd59383128b8e3e7cccdbf91c09b74c` | Serve saved Micron pages through native NomadNet sessions | 84 |
| `4ac591c9070b5624d9f58b6b511e3a5485e6c87b` | mere | `18030af39e19fbcbff8166977e2be4dc15050310` | Preserve Micron syntax and render native links and tables | 127, 149 |
| `317593383a4bacb60600c5532812f7df4757bed9` | knot-editor | `f56249e092a3776ae9027783a596c8c0690f9808` | Integrate native Micron preview | 128 |
