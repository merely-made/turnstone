# Reader appearance native-check receipt

Date: 2026-09-06

## Result

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

## Headed status

The executable scenario is pending. Two attempted `cargo build` commands were
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
