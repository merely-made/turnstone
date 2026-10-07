# Final Linux default-feature qualification, 2026-10-07

Exact published source `8d1f907f4bd31b256737a27d3e63fe8cd8672efc`, clean tracked `main` on `thinkpad-l14-f`. [Inputs](linux-final-fd-inputs.json) bind Rust 1.98.1, the tracked file inventory, selected Git packages, and the existing marker-identified `/home/markik/Code/target`. [Summary](linux-final-fd-summary.json) records unchanged source and every tracked input after all five gates. [Runner](linux-final-fd-runner.py) retains the actual command and observer implementation.

| Locked default-feature gate | Actual exit | Elapsed seconds |
| --- | ---: | ---: |
| Workspace library tests, standard parallel harness | 0 | 247.884 |
| Workspace all-targets check | 0 | 10.650 |
| cargo_mode verify | 0 | 3.261 |
| Dependency tree | 0 | 1.362 |
| Filtered Linux metadata | 0 | 1.413 |

The [library stdout](linux-final-fd-lib.stdout.log) records **650 passed, 0 failed, 9 ignored** (659 cases), with test time 246.15 seconds. The sandbox library records zero cases. [cargo_mode](linux-final-fd-cargo-mode.stdout.log) verified 1,280 lock packages and lock SHA256 `0e49603c02f3ef9d51cf986508c4c1bc540e1cdc5151b68268563463aa13bec0`. Raw stdout, stderr, and individual result JSONs are retained for every gate; [filtered metadata](linux-final-fd-metadata.stdout.log) and [family provenance](linux-final-fd-family-provenance.json) bind 1,072 default Linux packages/resolve nodes and the exact current Mere/Genet/Knot/Woodshed/Weld sources.

This rerun follows the preserved [original parallel negative](linux-final-README.md). Only each launched process's soft NOFILE limit was raised from 1,024 to 65,536, within the unchanged 1,048,576 hard limit. `nice 10`, Cargo `-j1`, and the normal parallel test harness remained unchanged; no `--test-threads` override was used. The actual test process `/proc` limit was observed as 65,536/1,048,576 and its sampled descriptor peak was **2,333**. Accessible owned-lineage `/proc` snapshots were sampled every 0.5 seconds, so observed peaks are lower bounds and inaccessible/exited processes can be missed. This confirms that the successful run needed a descriptor budget above the original soft limit; it does not establish a leak diagnosis or prescribe a permanent user/system setting.

[Post-state](linux-final-fd-post-state.json) confirms clean tracked source, no live compiler/test/application owner observed, and the fresh SSH observer still inherits 1,024/1,048,576. The approved shared target is retained for reuse. Remote baseline and diagnostic receipts, remote Knot receipts, and the unrelated Mere branch were preserved. This evidence qualifies portable default-feature compile/tests/lock coherence. Foreign Windows browser producers, headed coexistence, and assistive-technology acceptance remain separate gates.

[Remote artifact inventory](linux-final-fd-remote-artifacts.json) records the 20 exclusive remote outputs, all copied back and SHA256/size verified. [Packet hash index](linux-final-fd-hashes.json) binds these outputs and the local runner, inventory, post-state, and this note. Original controls were not overwritten.
