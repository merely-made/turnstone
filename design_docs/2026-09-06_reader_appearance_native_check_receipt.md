# Reader appearance native verification receipt

Date: 2026-09-06

## Idle and shutdown follow-up

The follow-up native run passed, with all original appearance checks retained.
Its three waits completed in **103, 0, and 0 frames**, within the unchanged
240-frame budgets. At each checkpoint, pending fetches, requested content,
graph settling, and unsettled document sessions were all clear. The prior
iroh endpoint-drop diagnostic was absent. The runner now rejects capped waits,
busy snapshots, and that ungraceful-shutdown diagnostic.

The first diagnostic run established the cause: a failed favicon fetch stayed
pending because the actor emitted success only. Mere now emits a terminal
`FaviconOutcome` keyed by request ID. Turnstone clears exactly that request for
either result; failure remains UI-silent. The three favicon tests passed,
including same-page out-of-order completion and failed-request cleanup.
The idle-command parser test also passed using the same compiled test binary.

Native service owners now signal cancellation, await carrier close, and join
their worker threads. Reader cancellation preserves normal request duration.
Place teardown also awaits watcher cancellation and endpoint close on its
retained runtime. The native fixture does not exercise an active place join
or publishing transfer; these paths have compilation and ownership review,
not a transfer-in-progress receipt. A Gemini identity warning for the HTTP
fixture remains in the raw log and was not suppressed.

The [follow-up artifact receipt](receipts/reader_idle_shutdown/receipt.json)
contains hashes, the failed diagnostic observations, accepted observations and
captures, app logs, and unit-test/build logs. The accepted independent-scroll
capture was visually inspected. This is an idle-predicate and shutdown proof,
not an FPS, allocation, or background-worker CPU measurement. It uses the same
local dependency overlay and isolated Cargo cache as the original proof.

```powershell
$env:CARGO_HOME='C:/Users/mark_/Code/scratch/refresh-proof/cargo-home'
$env:CARGO_TARGET_DIR='C:/Users/mark_/Code/target-appearance-turnstone'
cargo test -p turnstone --lib favicon --offline --config C:/Users/mark_/Code/scratch/refresh-proof/turnstone-local.toml -j 1
cargo build -p turnstone --offline --config C:/Users/mark_/Code/scratch/refresh-proof/turnstone-local.toml -j 1
scenarios/run_reader_appearance.ps1 -TurnstoneBin C:/Users/mark_/Code/target-appearance-turnstone/debug/turnstone.exe -OutputRoot C:/Users/mark_/Code/scratch/refresh-proof/reader-idle-shutdown-run-02
```

## Native result

The executable build and headed scenario passed on 2026-09-06. The fixture,
scenario, app process, and runner all completed successfully. Four captures
and four state observations are retained in
[the artifact receipt](receipts/reader_appearance_native/receipt.json).

| Checkpoint | Workbench scroll | Inset scroll |
| --- | ---: | ---: |
| Before input | 0 | 0 |
| Inset input | 0 | 160 |
| Workbench input | 240 | 160 |
| Workbench closed | absent | 160 |

The Workbench viewport was 251x570 and the inset 305x600. Both reported the
same source group while present, backed by `Arc::ptr_eq` within each snapshot.
Workbench ID `0x7ff62e3ddce5f1e0` and inset ID `0x83646760589b1fb1` remained
stable. Closing Workbench preserved the inset ID and exact offset.
The [independent scroll capture](receipts/reader_appearance_native/03_independent_scrolls.png)
and [surviving inset capture](receipts/reader_appearance_native/04_inset_survives.png)
were visually inspected. Pixel dimensions and scroll positions are recorded
from the live host plan and retained Reader sessions, not fixture expectations.

The runner now also checks exactly two appearances and shared source after
each scroll. A separate scratch checker audit accepted a valid fractional
offset and rejected fractional drift, an extra appearance, and divergent
source. That audit is not native acceptance evidence.

Three global `wait 240` steps reached their frame caps. Explicit Reader state
assertions and all captures passed, but this receipt does not establish
whole-app idle behavior or frame performance. Interactive resize and document
replacement were not driven here. The process also emitted an iroh
endpoint-close diagnostic during shutdown; the successful exit does not close
that lifecycle concern.

The build used an isolated Cargo cache copied from local cached packages to
avoid a shared cache lock. Genet's copied checkout was verified at
`115d348deddc344d949754e63beaece47cf49f34` with no tracked differences. Local
Mere and existing sibling overrides remain in use, including pinned Distillery.
This is a local integration proof, not a published-dependency build receipt.
The native build completed in 49m34s with 76 Turnstone warnings and no errors.

```powershell
$env:CARGO_HOME='C:/Users/mark_/Code/scratch/refresh-proof/cargo-home'
$env:CARGO_TARGET_DIR='C:/Users/mark_/Code/target-appearance-turnstone'
cargo build -p turnstone --offline --config C:/Users/mark_/Code/scratch/refresh-proof/turnstone-local.toml -j 1
scenarios/run_reader_appearance.ps1 -TurnstoneBin C:/Users/mark_/Code/target-appearance-turnstone/debug/turnstone.exe -OutputRoot C:/Users/mark_/Code/scratch/refresh-proof/native-reader-run-01
```

Use a fresh output directory for each rerun. Binary, source, overlay, log, and
capture hashes are in the artifact receipt above. Raw logs are retained,
including capped waits and shutdown diagnostics.

## Earlier compile-only result

`cargo check` passed for the native Turnstone host. It completed in 1m45s
with 76 warnings and no errors. This checks the new role-resolved Reader
scenario seam against local Mere sources. It is not a headed-result claim.

```powershell
$env:CARGO_TARGET_DIR='C:/Users/mark_/Code/target-appearance-turnstone'
cargo check -p turnstone --offline --config C:/Users/mark_/Code/scratch/refresh-proof/turnstone-local.toml -j 1
```

Turnstone source base: `b70ba554d76f3b32bf722da7e5cab97abf70c0eb`.
Reader observation getters: Mere `1523c8287b419ae98e4122dec54668a445f2824d`.

The temporary overlay patches local Mere workspace packages. It deliberately
retains Turnstone's pinned Distillery: the local Distillery WIP has an
unrelated undeclared `scenomise` use in `chronicle.rs:324`.

## Earlier blocked build attempts

In the earlier compile-only pass, two attempted `cargo build` commands were
stopped while waiting on an external package-cache lock. At observation time
38 Cargo processes were active; that count does not identify a particular lock
owner. No further locked waiters were created.

When the cache is available, build and run from the Turnstone repository:

```powershell
$env:CARGO_TARGET_DIR='C:/Users/mark_/Code/target-appearance-turnstone'
cargo build -p turnstone --offline --config C:/Users/mark_/Code/scratch/refresh-proof/turnstone-local.toml -j 1
scenarios/run_reader_appearance.ps1 -TurnstoneBin C:/Users/mark_/Code/target-appearance-turnstone/debug/turnstone.exe
```

The runner parses observation files with invariant-culture numeric parsing and
verifies shared source, distinct IDs and viewport widths, one-role-only scroll
changes, and exact surviving inset ID and scroll offset after closing Workbench.

## Artifacts

| Artifact | SHA-256 |
| --- | --- |
| `scratch/refresh-proof/turnstone-reader-appearance-check.log` | `759bfe2232692993d4ca5b787599a1e0f655c6cf262e099a8a7c37f8002cd89f` |
| `scratch/refresh-proof/turnstone-local.toml` | `cf04500918b884423f03c22a149a14c2e5ea6897191934457c4db5284acc9357` |
| `src/scenario.rs` | `b617d0c882c302d523556d06483260bb42af5dacb68e649a938b26c246cacb17` |
| `src/shell/drive.rs` | `7cd3bfbbd5765da70ef6d44f5427cb88d9b888d307cfe399effec044887a1ff1` |
| `src/shell/reader_observe.rs` | `afe4ce4dcb4d89b611758fefa63357d60e2b5b6553e7e0534c442e3bb1fbc779` |
| `scenarios/reader_appearance.scn` | `583f7864ad84420400592d681ebfd63ff63e1863ed77c4c894af5d285bab5525` |
| `scenarios/run_reader_appearance.ps1` | `bce1cc1d74f84cb81c7a69150cef8fc07e95a1724605818aa8d9459af4493457` |

Static validation after the diagnostic text refinement passed: `rustfmt --emit
stdout` parsed the three Rust scenario/observer files without changing them,
and PowerShell's AST parser accepted the runner.
