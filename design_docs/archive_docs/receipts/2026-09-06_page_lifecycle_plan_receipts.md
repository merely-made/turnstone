# Page lifecycle plan receipts

Receipts for the archived [plan](../2026-11-02/2026-09-06_page_lifecycle_plan.md) (archived 2026-11-02 under DOC_POLICY section 8: run evidence is kept apart from the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/turnstone/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/turnstone/` on 2026-11-02; the folders `receipts/`, `design_docs_receipts/` and `support_receipts/` there were tracked here as `docs/receipts/`, `design_docs/receipts/` and `support/receipts/` until the closure's first commit moved them byte for byte (SHA-256 checked).

- `receipts/document_formats_20261019/README.md` (line 1071): present
- `receipts/place_sharing_20261019/README.md` (line 992): present
- `receipts/r0_source_save_20261017/qualification.json` (line 542): present
- `receipts/r1_source_keep_20261017/qualification.json` (line 638): present
- `receipts/r2_saved_sources_20261017/qualification.json` (line 698): present
- `receipts/r3_scoped_answers_20261017/README.md` (line 764): present
- `receipts/r4_research_workflow_20261018/README.md` (line 810): present
- `receipts/research_notes_20261018/README.md` (line 948): present

## Inline run evidence that stays in the archived plan

47 lines of the archived file mention a receipt. Sections that are run evidence:

- line 293: Receipts
- line 807: R4. One end-to-end acceptance receipt

## Old-to-main hash table (2026-11-02)

The body keeps the hashes it was written with (Mark, 2026-11-02: "Live docs: local + sibling"; archived docs keep theirs). Each row is a cited commit that is not on its repository's main, with the commit on main that has the same author, author time and subject, re-verified with `git show` and `git merge-base --is-ancestor` on 2026-11-02. Line numbers are in the archived file.

| cited | repo | on main | subject | lines |
|---|---|---|---|---|
| `428deb5` | turnstone | `684e16d821` | Move pinned Rust toolchain to 1.99.1 | 311, 469, 1212 |
| `45542b1` | turnstone | `aab17ac9c4` | Adopt verified input and engine ownership repairs | 410, 462 |
| `b7e1529` | turnstone | `497fa8674c` | Take welding 0.15.0 and scrying 0.7.2 from crates.io | 462 |
| `dc630b7` | turnstone | `be5f28a5af` | Make source capture saves acknowledged and recoverable | 600 |
| `0ab2d5e` | turnstone | `ab33def9fb` | Add private saved-source search and offline reading | 765 |
| `b34a03c` | turnstone | `ec19884f84` | Add scoped research Answers and exact saved-source citations | 812 |
| `d3d5d18` | turnstone | `e304735864` | Qualify the complete research workflow across cold reopen | 928 |
| `9c8420d` | turnstone | `bcb5ff70bf` | Adopt promoted foundation families from canonical repositories | 1006 |
| `725b0f35` | mere | `c014e77b54` | docs: scope captured collection implementation lanes | 1186 |
| `a9b5dd663` | mere | `14dea0d8a5` | Record publication of qualified scroll supplier and consumer | 699, 1249 |
| `a17e5bb97c79332b932d10cce70f465b022a3a1a` | mere | `a4b52bd87147f62ab235f2b78c885f2550390b56` | Adopt shared fetch semantics and supervised worker contracts | 313 |
| `a17e5bb9` | mere | `a4b52bd871` | Adopt shared fetch semantics and supervised worker contracts | 436, 1213, 1220 |
| `55c479bc` | mere | `9005deeca0` | Use published welding 0.15.0 instead of the main git pin | 463 |
| `00e444286` | mere | `17baa751bc` | Read canonical Resource tags in Sets scenes | 465, 535, 600, 1228 |
| `1be0bc04f` | mere | `c47e469bdf` | Preserve HTTP response provenance for explicit source capture | 601, 1236 |
| `e6250fd1` | mere | `120af70507` | Record graph label publication and merged supplier checks | 765, 812 |
| `4733c557` | genet | `dfa48090f3` | Propagate rendering admission through checked document frames | 766, 813 |
| `c5efc94f` | netrender | `2373693686` | Reject unsupported retained fragments and borders before rendering | 766 |
| `c5efc94` | netrender | `2373693686` | Reject unsupported retained fragments and borders before rendering | 813 |
| `dd9d5c523` | mere | `90eb943cff` | Allow headed inference acceptance across resident processes | 834, 1279 |
| `7e196c5` | knot-editor | `9bca9fa73e` | Move pinned Rust toolchain to 1.99.1 | 929 |
| `88eeb975` | knot-editor | `297902c0d1` | Intake reviewed resident materialization supplier | 929 |
| `9b8c161a` | mere | `922455a053` | Merge remote-tracking branch 'origin/main' into HEAD | 930 |
| `7114f91c` | genet | `54c1662294` | parley: load complex-script segmenter dictionaries at runtime | 930 |
| `ceaf41fbd4075a6746c316c46cc66bce976d0546` | knot-editor | `2ad3fff84d1f35c1e6b936cb11330fc11a36df1a` | Preserve and validate exact saved-source clip citations | 940 |
| `500bc78a7fa9e0b9abe8f35d189c465f93d3ccab` | mere | `63d97969fe80c15dc29b50f167bd98b6dd3c7363` | Use supported solid divider for resident overmap tier | 941 |
| `7114f91ce928937ac0891d7db816655706843204` | genet | `54c1662294126cb732b0bf3c430af5e00116bf02` | parley: load complex-script segmenter dictionaries at runtime | 942 |
| `2a4a204` | knot-editor | `7905b8294e` | Adopt promoted foundation families from canonical repositories | 1006 |
| `2a7231a` | knot-editor | `d50fe1d517` | Name document surfaces plainly and record the selected-sharing boundary | 1007 |
