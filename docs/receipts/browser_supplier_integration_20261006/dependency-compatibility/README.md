# Browser adapter dependency compatibility, 2026-10-06

These receipts qualify bounded committed supplier revisions against Turnstone's
existing dependency family. They do not qualify the consumer's native build or
headed behavior. Those gates belong to the enclosing supplier integration receipt.

| Supplier | Qualified baseline | Compatibility revision |
| --- | --- | --- |
| Mere | `3d1cdacc90aa6a0736154d841223a3ae29e23aa5` | `db4ee31258b23c86572c429388c5d10bd4de9dc3` |
| Woodshed / Redshank | `9e982b88bf57e41ef4fa846c66ffdd4f05beb91d` | `b613fc55d2d59e865b6bb04e55a880ee4773b824` |
| Knot | `92719898b7101c1601f79183874debcdcf54d24f` | `91cb44a2c00c088f0677956d31f3cdca6828c59e` |

All three compatibility revisions were published to their repositories' temporary
`codex/browser-input-compat` branches and confirmed through `git ls-remote`.
The Git diff logs include the exact baseline, revision, complete diff, passing
diff check, and observed remote branch ref. Publication makes the exact committed
sources available to Cargo; it does not establish release readiness.

## Bounded changes

Mere contains the Weld mouse/CHAR adapter and ordered Graft host-event changes,
plus progress in the existing engine-picker plan. `Cargo.toml` and `Cargo.lock`
are unchanged from the qualified baseline. The three adapter source blobs are
identical to Mere's reviewed main-line commit
`69cb39214d5fedb5bfb665383393c383f01aaa45`; see
[the source equality log](mere-baseline-and-source-equality.log).

Woodshed changes only Redshank's eight nested workspace Mere aliases, three web
aliases, sixteen lockfile source entries, and its existing GUI plan. The unrelated
Woodshed root and Hocket pins remain unchanged. Knot changes only thirty-two
root Mere source references, two desktop aliases, seven narrow document aliases,
fifty-six root lockfile source entries, and its existing extraction plan. The
unused root patch accounts for one more Mere lockfile entry than active package.
All changed lockfile packages preserve versions and dependency lists.

The source-query changes replace Mere `3d1cdacc` with `db4ee312`. They retain Genet
`69a2383` and other dependency pins. Full exact package versions and source pins
are recorded in the source-family JSON files. There are no production path
overrides or synthetic source overlays. A same-repository Git patch was rejected
by actual Turnstone resolver evaluation because Cargo treats both revisions as
the same source; the committed compatibility ancestry is the adopted route.

## Recorded gates

- [Mere adapter test log](mere-adapter-tests.log): from the exact `db4ee312`
  worktree, four Graft and eleven Weld tests pass, zero failures or ignored tests.
  The stable shared target is `C:/t/cargo-targets/mere`. The observed unused-patch
  warnings are retained in the log. These tests exercise producer dispatch and
  callback order, not CEF runtime or native IME delivery.
- [Redshank offline locked metadata](redshank-metadata.log) resolves sixteen Mere
  packages from `db4ee312` and twenty-four Genet packages from `69a2383`; exact
  package versions are in [the source-family JSON](redshank-source-family.json).
- [Knot root offline locked metadata](knot-root-metadata.log) resolves fifty-five
  active Mere packages from `db4ee312` and twenty-two Genet packages from
  `69a2383`; exact versions are in [the source-family JSON](knot-root-source-family.json).
- The separately excluded Knot document manifest was also evaluated in the
  earlier gate: eight Mere packages from `db4ee312`, six Genet packages from
  `69a2383`. Its baseline has no committed nested lockfile. A generated ignored
  lock was used for a second offline locked metadata pass, marker-verified, then
  removed after the result was recorded. This is a diagnostic resolution, not
  preservation of an existing document-only lockfile. The raw earlier command
  output was not saved; the root metadata log and JSON are the durable rerun.

Weld raw down/up virtual and scan codes remain unchanged; pressed text is sent
as separate CHAR events using character codes in supplied scalar order. Mouse
dispatch preserves changed buttons, modifiers, and held-button state. Ordered
completion and frame ownership remain unchanged. Welding's pinned CEF conversion
casts a scalar to `u16`, so supplementary Unicode and OS IME delivery remain
separate supplier/native gates. BMP adapter tests cannot close them.

The touched Mere plan's focused documentation audit has no findings. The earlier
full repository audit reported fifty-five missing known-root paths and seven
stale historical annotations elsewhere; it was not a green repository audit.

## Isolation and cleanup ownership

The approved worktrees are real collision isolation, owned by the bounded browser
supplier integration:

- `C:/Users/mark_/Code/worktrees/mere-browser-input-compat`: primary Mere was
  concurrently receiving Graft adapter edits on the newer dependency family.
- `C:/Users/mark_/Code/worktrees/woodshed-browser-input-compat`: primary Woodshed
  carries the user's unrelated dirty Redshank transcript lane and differs from
  Turnstone's qualified baseline.
- `C:/Users/mark_/Code/worktrees/knot-editor-browser-input-compat`: primary Knot
  has a newer independent dependency family and unpublished local commits at
  `33cc855acd881586e454c95046224b0815277a13`.

All three clean worktrees and temporary branches remain until the coordinator
qualifies the exact Turnstone consumer. Ordinary integration will preserve each
compatibility revision as reachable main ancestry while retaining current main
source/dependencies. Woodshed's dirty primary checkout and Knot's unpublished
local main remain untouched. The owner then removes the temporary branches and
worktrees after recorded integration. No isolated Cargo home or new target was
created. The user-paused graph-semantics worktree was never touched.

## Actual Turnstone lock comparison

[The root lock comparison](turnstone-lock-source-comparison.json) inventories
the current consumer against its HEAD lock. It contains one Mere `db4ee312`
family (70 packages), Genet `69a2383` (31), Woodshed `b613fc55` (6), Knot
`91cb44a2` (4), Graft `01f3c9f3` (3), and upstream Servo `1d44e5dd` (56).
The lock has one `wgpu` 30.0.1 and one `inker` 0.1.1 identity. Every preexisting
registry package version remains. Servo adds parallel versions under 54
already present registry names; the JSON records those additional versions and
new names explicitly. This inventory is a source/lock comparison; package
activity and native behavior require their own feature-selected gates.
