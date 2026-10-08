# ThinkPad trio archival refresh, 2026-10-08

The user authorized preserving the stale local work before bringing the three
ThinkPad primary checkouts current. All three were on `main`, had no detected
process using their source directories or loaded files, and fast-forwarded to
the fetched `origin/main` without changing upstream history.

| Repository | Original HEAD | Refreshed HEAD | Preserved tracked files | Preserved untracked files |
| --- | --- | --- | ---: | ---: |
| wgpu-graft | 17615cf973ce | 0b026a44b3e2 | 3 | 0 |
| wgpu-scry | 1b555ceee70c | dc9bd1a08d90 | 10 | 0 |
| wgpu-weld | a1eaa267ae9e | c4dd593b7a73 | 5 | 934 |

Each archive remains on the ThinkPad at:

`/home/markik/Code/repos/<repository>/docs/receipts/thinkpad_trio_archive_20261008/`

The archives contain exact changed file contents under `preserved/`, separate
binary patches for the index and worktree, the original status and index
entries, and a manifest recording original HEAD, hashes, sizes, modes and
modification times. Weld's 934 untracked files comprise three loose source
files and the 931-file CEF 147 SDK. They total 1,787,654,373 bytes. The SDK was
moved within the same checkout filesystem without duplicating its contents.

Every preserved file was checked before clearing the corresponding original
edit, and checked again after the fast-forward. Patch hashes were also checked.
Tracked files are clean in all three checkouts; the only untracked content in
each is its archive directory. The JSON files beside this receipt contain the
summary and full archival manifests. The SDK and archived source bytes remain
on the ThinkPad and were not added to Git.

For recovery, consult the repository's manifest for its original HEAD and
`preserved/` for its exact bytes. The separate patches retain the original
index/worktree distinction. Recovery must account for the newer checkout and
any intervening work rather than applying an old patch blindly.

This records checkout maintenance and preservation, not compilation, native
runtime qualification or release readiness. Existing Cargo configuration,
targets and auxiliary worktrees were left untouched. No new Cargo target,
Cargo home or worktree was created.
