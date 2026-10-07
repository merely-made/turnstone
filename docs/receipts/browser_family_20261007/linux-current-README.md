# Linux current-family control at abb349c, 2026-10-07

The published current family compiles on Linux, but its default library suite is **not yet qualified**. Exact source is `abb349cf7957e2b509f4cb7f492e3e0db4821964` on `thinkpad-l14-f`, Rust 1.98.1. All 2,420 tracked file hashes remained unchanged throughout the original run and continuation.

| Gate | Actual result | Elapsed |
|---|---|---:|
| Locked workspace/all-targets check | PASS, exit 0 | 273.353 s |
| Full default workspace library suite, serial test harness | FAIL, exit 101; 644 passed, 5 failed, 9 ignored | 1483.210 s including compilation; 771.27 s test execution |
| `cargo_mode.py verify +1.98.1` | PASS, exit 0; 1,280 catalog packages | 4.705 s |
| Locked dependency tree | PASS, exit 0 | 1.369 s |
| Locked Linux-filtered default metadata | PASS, exit 0; 1,072 packages/resolve nodes | 1.421 s |

[Original summary](linux-current-summary.json) retains the failed library gate and original runner stop. [Continuation](linux-current-continuation-summary.json) records the remaining successful gates without replacing that failure. [Library output](linux-current-lib.stdout.log) and [compiler output](linux-current-lib.stderr.log) preserve full warnings, transport cleanup logs and captured assertions. The original suite used `--test-threads=1`; a later repair qualification must record its own source and command.

The failures are:

- `denizen::tests::staged_installs_are_content_derived_and_reviewable`: its single-line measurement is 549.5621px against 528px. [Exact no-build probe](linux-current-denizen-exact-control.result.json) reproduces exit 101 in 0.126 s using the same linked binary; [panic](linux-current-denizen-exact-control.stderr.log) preserves the full grant text. Production install review rows already wrap; a test-contract repair is separate from this control.
- `sky_surface::tests::reference_source_reproduces_the_p0_receipt` and `sky_timeline::tests::boston_eclipse_day_composes_an_analytical_sun_moon_receipt`: calculated `decef158176a2c11fcb5f65ef058e7cee96f43d05a2a88604598ed90ae210f03`, expected `caff8371d348ba141397a8185e291c533c6ab12d4fe85f9ce3be797707ce411d`.
- `sky_surface::tests::provider_admits_semantic_controls_and_provenance`: expected visible `Receipt caff8371`.
- `sky_surface::tests::next_day_replaces_one_retained_projection_without_rewriting_opening_provenance`: calculated `bb0941fe1f6c1dc59461bbf9d105bba7e6ff7d1b4f2077d2e46be3126fa4a8ff`, expected `74883b5db9fa959cccdeb69744da4a99baec5bfd55bade77426cfe6b0d9c450f`.

No numerical/serialization cause or equivalence policy is inferred from the digest mismatch. The separately reviewed [temporary output patch](linux-current-sky-diagnostic-overlay.patch) exposes exact JSON for cross-host comparison; its diagnostic runs are separate evidence, not accepted published-source gates.

[Input inventory](linux-current-inputs.json), [manifest](linux-current-Cargo.toml), [lock](linux-current-Cargo.lock), [remote artifact hashes](linux-current-remote-artifacts.json), [guard](linux-current-guard.json) and [post-state](linux-current-post-state.json) bind source/provenance. Portable mode verifies lock SHA256 `0e49603c02f3ef9d51cf986508c4c1bc540e1cdc5151b68268563463aa13bec0`. [Filtered metadata provenance](linux-current-family-provenance.json) contains one source each for Mere `57b4893d`, Genet `965b64e2`, Knot `0096591a`, Woodshed `82271df2` and neutral Welding `c4dd593b`. The full lock also retains the optional supplier/Servo sources; their presence is not Linux native execution proof.

The six baseline outputs were byte-identical to their newly published abb blobs and were reconciled without deletion before the fast-forward. All 24 original-current remote outputs were transferred and verified. Remote tracked source was clean after the original gates; owned new receipts remain untracked and preserved. The existing Cargo-marked `/home/markik/Code/target` is retained for follow-up; post-state observed no known compiler/app or lock holder. No other repository, isolated home or worktree was changed.

This is default Linux compile/test/portable-lock evidence. It does not qualify headed foreign producers, Windows-only three-engine registration, or accessibility. Original failures stay retained if later repairs pass. [Local receipt hashes](linux-current-hashes.json) cover these original controls and documentation.
