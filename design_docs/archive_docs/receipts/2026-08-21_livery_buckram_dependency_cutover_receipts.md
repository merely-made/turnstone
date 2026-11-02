# Livery and Buckram dependency cutover receipts

Receipts for the archived [plan](../2026-11-02/2026-08-21_livery_buckram_dependency_cutover.md) (archived 2026-11-02 under DOC_POLICY section 8: run evidence is kept apart from the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## Old-to-main hash table (2026-11-02)

The body keeps the hashes it was written with (Mark, 2026-11-02: "Live docs: local + sibling"; archived docs keep theirs). Each row is a cited commit that is not on its repository's main, with the commit on main that has the same author, author time and subject, re-verified with `git show` and `git merge-base --is-ancestor` on 2026-11-02. Line numbers are in the archived file.

| cited | repo | on main | subject | lines |
|---|---|---|---|---|
| `04e303ac` | mere | `62c928cabf` | Migrate consumers to Livery and Buckram | 27, 37 |
| `b9457041` | genet | `792706dfe8` | A11y tree reports physical coordinates | 28, 37 |
