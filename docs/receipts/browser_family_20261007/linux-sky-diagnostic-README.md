# Linux temporary Sky receipt diagnostic, 2026-10-07

The exact reference test at `abb349cf7957e2b509f4cb7f492e3e0db4821964` plus the reviewed temporary test-only [overlay](linux-sky-diagnostic-overlay.patch) exited **101** in **60.825 seconds**, after emitting both exact pretty DTOs before its unchanged fixed-digest assertion. This is diagnostic evidence, not a clean published-source gate.

[Actual result](linux-sky-diagnostic.result.json), [stdout](linux-sky-diagnostic.stdout.log), [stderr](linux-sky-diagnostic.stderr.log), and [extracted receipts](linux-sky-diagnostic-receipts.json) bind the command, source/overlay hashes and raw output. Each `pretty_json` string retains exact emitted bytes; the parsed `receipt` object is a convenience. Dates are 2024-04-08 and 2024-04-09. Cargo ran at nice 10 with one build job and the existing shared target; no RUSTFLAGS override was active.

The overlay was removed after capture. The test file restored exactly to its original SHA256 `9a7dcc866c711516cf698e7b9d96e4020d0e695c9244ecb27e945f26d441678b`; tracked diff is empty and every original tracked input hash matches. Production code and digest semantics were unchanged.

[Mechanical cross-host comparison](sky-cross-platform-comparison.json) binds Windows exit 0 and Linux exit 101, both raw DTOs and source-overlay provenance. Only four angular f64 leaves differ, with maximum absolute difference `7.806255641895632e-17` radians. All other parsed DTO fields match, including inputs, metadata and TT interval bounds. No specific libm cause, rounding or accepted equivalence policy is inferred.

Windows provenance is separately qualified by [its source identity record](windows-sky-diagnostic-source-identity.json): the base and reviewed Sky overlay are recorded alongside other audited cfg(test) repairs, but temporary Windows raw-source SHA was not captured before restoration. That gap is retained. Neither instrumented run is described as clean published-source acceptance.

The original [current-family negative control](linux-current-README.md) remains immutable. Any repaired published-source full-suite result must have separate receipts. [Diagnostic hashes](linux-sky-diagnostic-hashes.json) cover this packet and the comparison.
