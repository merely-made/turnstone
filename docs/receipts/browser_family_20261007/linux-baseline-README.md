# Linux pre-repin baseline, 2026-10-07

PASS: `cargo +1.98.1 check --workspace --all-targets --locked -j1` on `thinkpad-l14-f`, Linux x86_64, exited **0** in **591.054 seconds** (10:09:02–10:18:53 UTC). [Actual result](linux-baseline-result.json) and [compiler log](linux-baseline.stderr.log) preserve the command, environment and warnings.

The clean remote `main` was fast-forwarded from `769a131786d3c8761fad7e8cdede26ea6a32a5ea` to exact published pre-repin source **`aafa78df71170c83df9838c6fc207117bedc45ec`** after fetching and verifying ancestry. [Guard](linux-baseline-guard.json), [inputs](linux-baseline-inputs.json), [manifest](linux-baseline-Cargo.toml) and [lock](linux-baseline-Cargo.lock) bind the source tree and every tracked file. All tracked file hashes and the source SHA remained unchanged during the check.

The command used `nice -n 10`, `CARGO_BUILD_JOBS=1`, and the approved existing `CARGO_TARGET_DIR=/home/markik/Code/target`; the running Cargo PID was independently observed at nice level 10. [Runner](linux-baseline-runner.py) records actual process completion. This is the pre-repin comparison, not qualification of the new coordinated family.

The existing Cargo-marked target is retained for the final published-family follow-up. [Post-state](linux-baseline-post-state.json) records clean tracked source and no observed named compiler/app or target-lock holder. The six owned remote receipt outputs remain untracked and preserved; other repositories were untouched. Ownership observations are instantaneous and must be repeated before another build.

This receipt qualifies default Linux workspace/all-targets compilation only. It ran no tests, headed browser, accessibility or foreign-engine native check. Linux default compilation does not qualify Windows-only three-engine producer registration. Final current-family Linux checks and tests remain pending after publication.

[Artifact hashes](linux-baseline-hashes.json) cover this new baseline receipt without changing earlier evidence.
