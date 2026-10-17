# Turnstone graph scene refresh — 2026-10-16

Status: implementation and scoped software checks complete; headed/reopen
acceptance is blocked by the locked Q-PC macOS console. Nothing from this
slice is pushed yet. The full workspace run passed 973 tests with ten existing
ignores and one stale settings assertion. Its corrected nine-test module passes;
a second completely green full run was not performed. Production code did not
change after the full run.

Runtime candidate: `c0cdc2c0e415558f840413f8a57ace276d3683cc`. Mere supplier `7441f2f399701c14a7a6abb67146bb08c0a8055e`
merges the existing inference wire from `65b97b0` with Mere main `d1e1d7aae`,
then corrects Sets to read canonical Resource tags. The newer inference
continuation was excluded. The supplier remains on the isolated
`codex/turnstone-scene-supplier` lane; Mere primary and other active work are
preserved. `supplier-source-audit.json` binds the selected native dependency
cohort to clean, matching Git manifest heads, without local path patches.

## Product scope

The ordinary Orrery host now carries Partition, Sets, Provenance and Mosaic
through Canvas-owned projection, including bounded regions and nondefault
Mosaic organization. The native graph grammar distinguishes foreground,
retained and ambient nodes. Arrange and the palette expose reset to seed, set
seed, restore recipe seed, foreground pin, dismiss retained, grow and shrink.

Commands resolve the owning graph pane, including Arrange controls in a lens.
Scene saves resolve that graph runtime's own session and placement authority.
Seeds, region geometry and Mosaic organization persist through Pandect's
member-keyed facets. Refused session authority blocks a write before disk or
live facets change. Attention
is transient and remains coupled to the incumbent graph runtime across panes;
it is independent of physical position pins and keeping.

Reset advances the shared host-clock return. New arrangements, tile resizing,
play toggles, pointer interaction or reprojection supersede older returns.
The frame saves completed geometry; explicit save/close also flushes pending
graph views. The existing fold records remain separate curation.

## Gates and evidence

- `focused.log`: 19 passed, zero failed. The new cases cover all four layouts,
  canonical tagged membership, real provenance order, pause/play preservation,
  owner routing, exact saved seed, fresh-App Mosaic/region/pause adoption,
  graph-keyed save, refusal preservation and superseded returns.
- `supplier-scene-axes.log` and `supplier-bounded-scenes.log`: three and five
  shared tests pass. The regression uses the public tag mutation path, leaving
  raw Surface tags empty while reading canonical Resource assertions.
- `build.log`: native build passes.
- `workspace.log`: original process exit 101; 973 passed, one failed, ten
  ignored. The sole failure counted 14 radio choices, predating two shared
  built-in themes in the current supplier.
- `settings-corrective.log`: all nine settings-pane tests pass after commit
  `d5c198527acf2550afd4fccd2faeab04ccc818a4` changes only that expectation to 16
  and its explanatory comment. The verifier proves the runtime source ledger
  is otherwise unchanged. This is a corrective module rerun, not a second
  green full run.
- Native/reopen gate: blocked; zero accepted captures.

The first native attempt in `diagnostics/locked-console-r1` is excluded from
acceptance. It recorded zero successful presentations and no captures; the
read-only console check reported `CGSSessionScreenIsLocked=Yes`. Its generated
public custody helper and display-sleep assertion exited. The attempted
scenario never began. Profiles, vault files and binaries remain outside this
packet. The later diagnostic logging exposes unavailable acquisition reasons.

The native scenario creates six ordinary browser members at three sites and
navigation relations. It can qualify catalog/rendering, untagged Sets and
unrelated Provenance, Partition/Mosaic, attention, seed reset, folds, controls
and windows. Tagged membership and multistep provenance are qualified by the
CPU consumer tests, not by invented source data in the native fixture.

Portable SceneDocument viewing and remote/frozen projection expansion beyond
Spiral remain follow-ups. Windows/Linux, human accessibility and physical
recovery are not newly qualified by this packet.

Raw Cargo logs retain the upstream Servo lint-fixture manifest diagnostic and
existing supplier/deprecation warnings. Final process exit codes and test
summaries determine the software result; those logs are not edited.

## Reproduction and continuation

Run `python3 docs/receipts/graph_scenes_20261016/verify.py
docs/receipts/graph_scenes_20261016 --software-only` from the repository root
to verify the recorded software evidence. Without `--software-only`, the
verifier exits 2 while native acceptance is blocked. This distinction is
intentional.

After unlocking the Q-PC console, run the native driver with a fresh cohort,
for example `python3 docs/receipts/graph_scenes_20261016/run_native.py r2`. Its
default output is outside the repository under
`/Users/markik/Code/output/turnstone-scene-refresh-20261016/native-r2`;
`--output` may select another external scratch root. It refuses repository
output, preserves the failed attempt, freezes the binary/scenarios/source
ledger, and launches seed and fresh-process reopen against one new synthetic
profile. The generated public keeper and bounded sleep assertion exit in its
cleanup path.

Use the bundled Python with Pillow to run `verify_native.py` against the
accepted cohort, then inspect every readback. Copy only the evidence files
into this packet; exclude profiles, custody files and binaries. Bind the new
result to the recorded candidate, update the gate status and hash ledger, and
repeat packet verification. Publish the isolated Mere supplier branch before
Turnstone main so the receiving lockfile's revision is remotely reachable.
`publication-dry-run.json` records only remote/ref checks; no refs were written.
