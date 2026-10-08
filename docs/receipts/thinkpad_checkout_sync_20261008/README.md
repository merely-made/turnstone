# ThinkPad checkout refresh, 2026-10-08

Requested by Mark in the Turnstone lane. Host: `thinkpad-l14-f`. This is Git
checkout maintenance, not a build or dependency compatibility qualification.

Twelve checkouts advanced, three already matched their tracked upstreams, and
seven remain protected or unavailable. Fetches used at most two low-priority
workers, with automatic Git maintenance disabled for the command. No new
build, worktree, Cargo home, Cargo target, or global Cargo configuration was
created or changed. The existing Genet build was left alone.

| Checkout | Outcome | Current head | Branch |
| --- | --- | --- | --- |
| `crates/arboard` | Updated | `57a68d037e` | `custom-formats` |
| `crates/boa` | Updated | `4b6d316d1f` | `genet` |
| `crates/firefox-gfx-wr` | Local shader branch preserved; upstream fetched | `31042cde7b` | `shader-census-wgpu-30` |
| `crates/imaging` | Already current | `a8ac08d498` | `mere-wgpu-29-vello-0-9` |
| `crates/piccolo` | Already current | `e77309c648` | `master` |
| `crates/vano` | Updated | `1816f328fa` | `genet-embedder` |
| `crates/xilem` | Updated | `271a27a6d4` | `main` |
| `repos/cambium` | Endpoint inaccessible; checkout preserved | `f1251f7ebb` | `main` |
| `repos/cleromancy` | Updated | `ff3b1f5302` | `main` |
| `repos/genet` | Already current | `6cb2284a33` | `main` |
| `repos/hocket` | Endpoint inaccessible; checkout preserved | `0923086d67` | `main` |
| `repos/isometry` | Updated | `40a31d61b2` | `main` |
| `repos/knot-editor` | Updated | `6e66f1abdd` | `main` |
| `repos/mere` | Updated | `7a5bedd13c` | `main` |
| `repos/mere-receipt` | Pinned receipt worktree preserved | `ff68e86c59` | `detached` |
| `repos/merecat` | Updated | `1802a68371` | `main` |
| `repos/netrender` | Updated | `0edc60771e` | `main` |
| `repos/turnstone` | Updated | `1802a68371` | `main` |
| `repos/wgpu-graft` | Local source edits preserved; upstream fetched | `17615cf973` | `main` |
| `repos/wgpu-scry` | Local source edits preserved; upstream fetched | `1b555ceee7` | `main` |
| `repos/wgpu-weld` | Local source edits preserved; upstream fetched | `a1eaa267ae` | `main` |
| `repos/woodshed` | Updated | `9e4b37238d` | `main` |


All fifteen updated/current checkouts were verified equal to their tracking
references after the refresh. These are the configured fork branches; this
does not assert that every fork tracks the latest upstream library release.

Boa follows `origin/genet` and Vano follows `origin/genet-embedder`; their old
local branches remain at their original commits. Xilem and Mere now use `main`
and retain their earlier fork/receipt branches. Cleromancy reattached to `main`;
its previous detached commit remains in the published history. Mere's secondary
`mere-receipt` worktree and Firefox's shader-census branch remain pinned.

The Cambium and Hocket fetches failed for their configured endpoints. A bounded
check against current namespace endpoints also did not find an accessible
repository. No credentials or remote configuration were changed.

Seventeen Knot and fifty-six Turnstone receipt files collided with new tracked
files. Only those receipt files were relocated under each repository's
`docs/receipts/thinkpad_checkout_sync_20261008/preserved/` directory before
fast-forwarding. Their original bytes, sizes, mtimes, preserved paths, and the
upstream versions' hashes are recorded in `sync-results.json` and per-repository
`preservation.json`. All 73 saved originals were rehashed successfully. No
receipt was discarded. All three dirty trio HEADs and modified tracked-file
hashes were verified unchanged.

The earlier Turnstone validation worktree remains at
`/home/markik/Code/worktrees/turnstone-resource-compat`, owned by this lane. It is
clean and has no active process owner, but its `eeb4a50` preparation commit is
not yet an ancestor of the published main, so it remains pending publication
and integration. Its existing shared target is `/home/markik/Code/target`.

Linux work defaults to the ThinkPad with one heavy build at a time, explicit
bounded job counts, and low priority. Native Windows acceptance remains on
Windows. The offered s-pc-2 has not been configured in this maintenance pass.

`postflight.json` records all freshness, preservation, and retained-worktree
checks. These files contain technical paths and hashes, without credentials.
