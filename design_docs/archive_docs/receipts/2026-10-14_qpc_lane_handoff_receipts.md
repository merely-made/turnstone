# Turnstone lane handoff to q-pc, 2026-10-14 receipts

Receipts for the archived [plan](../2026-11-02/2026-10-14_qpc_lane_handoff.md) (archived 2026-11-02 under DOC_POLICY section 8: run evidence is kept apart from the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/turnstone/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/turnstone/` on 2026-11-02; the folders `receipts/`, `design_docs_receipts/` and `support_receipts/` there were tracked here as `docs/receipts/`, `design_docs/receipts/` and `support/receipts/` until the closure's first commit moved them byte for byte (SHA-256 checked).

- `integration-completion-20261011` (line 93): not found at this path
- `macos-qualification-20261014` (line 60): not found at this path
- `receipts/lens_fix_windows_20261014/` (line 67): present
- `receipts/stations_windows_20261013/published-final-native-qualification.json` (line 35): present

## Inline run evidence that stays in the archived plan

7 lines of the archived file mention a receipt. Sections that are run evidence:


## Old-to-main hash table (2026-11-02)

The body keeps the hashes it was written with (Mark, 2026-11-02: "Live docs: local + sibling"; archived docs keep theirs). Each row is a cited commit that is not on its repository's main, with the commit on main that has the same author, author time and subject, re-verified with `git show` and `git merge-base --is-ancestor` on 2026-11-02. Line numbers are in the archived file.

| cited | repo | on main | subject | lines |
|---|---|---|---|---|
| `77fd235fe659c513dd0463ed57be32f02fb2e71a` | mere | `ff5d9bbefa3f1e00c019b754841417466d010611` | Qualify shared query views and retained document scheduling on published supplie | 29 |
| `b6bfbeb556a6a17eddf8d5ad8254d3fcac95b084` | genet | `092145f3dac90ac7491a7791fa3f1dc40a69175f` | Delegate Netfetcher wire carriage to Murm while retaining browser policy | 30 |
| `8ed49d43f049921d56ab5a0c091a2c37c0970ee7` | knot-editor | `afc82633b71dcbd5ca9011cfe57d2eebdf854154` | Qualify Knot font roles with native and retained-state evidence | 31 |
| `f0a91f7` | genet | `ddfbd6603a` | Land ValueEngine, the value-level script contract beside ScriptEngine | 45 |

Several commits on main share the cited commit's author time and subject, so no single equivalent is claimed (left as text in the body):

- `17f71d6` (mere), candidates `5ade7327cb`, `f12e438080`, Specify tenant-owned browser presentation and viewer acceptance; lines 100

No equivalent exists on the repository's main (the commit is on no branch, or only on an unmerged one); left as text in the body:

- `abab46f` (mere), Preserve qualified integration checkpoint before October 14 intake; lines 101
- `71a1bbe` (mere), Merge fixed October 14 upstream while preserving integration repairs; lines 103, 109
