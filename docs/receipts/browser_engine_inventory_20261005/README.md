# Browser engine inventory qualification, 2026-10-05

This receipt qualifies the local B0 patch over Turnstone `74a46893d25e73e57787d31efd9d6850e4ad714f`.
It covers engine inventory, Inspector selection and refusal, saved-pin identity,
session adoption and failed-content recovery. Scry and upstream Servo are still
unconstructed here. It is not a hosted browser or trio publication receipt.

## Checks

| Check | Command / evidence | Result |
| --- | --- | --- |
| Default compilation | `cargo check --locked --all-targets --target-dir C:/t/cargo-targets/turnstone -j 2` | Initial and final source gates passed; `final-check.log` |
| Inspector regressions | `cargo test --locked --lib inspector_pane::tests --target-dir C:/t/cargo-targets/turnstone -j 2` | 14 passed; `inspector-tests.log` |
| Full library suite | `cargo test --locked --lib --target-dir C:/t/cargo-targets/turnstone -j 2` | 635 passed, 9 ignored; `library-tests.log` |
| Native build | `cargo build --locked --bin turnstone --target-dir C:/t/cargo-targets/turnstone -j 2` | Initial and final hook-repair builds passed; `native-build-final.log` |
| Native picker scenario | `scenarios/browser_engine_inventory.scn` | `native/scenario.done`: RESULT ok; all four PNGs generated, inventory/refusal/final Auto captures visually inspected |
| Optional Windows Weld controls | `cargo test --locked --features weld --lib shell::weld::contract_tests --target-dir C:/t/cargo-targets/turnstone -j 2` | 6 passed; `weld-contract-tests.log`; existing CEF 151.3.24 SDK reused through process-local `CEF_PATH` |
| Optional Windows Weld inventory | `cargo test --locked --features weld --lib inspector_pane::tests --target-dir C:/t/cargo-targets/turnstone -j 2` | 14 passed; `weld-inspector-tests.log` |
| Mere Scry adapter | `cargo test -p scrying-engine --lib --locked --offline --target-dir C:/t/cargo-targets/mere -j 2` | 21 passed at Mere `c36641d699c9fedbd17758380a2807b9ad9766ef` plus the local capability patch |
| Scry and Weld RADV preflight | Git Bash `bash -n scripts/assert-active-wayland-session.sh scripts/test-active-wayland-session.sh`, then `bash scripts/test-active-wayland-session.sh`, in each repository | Syntax passed; eight mocked cases passed per repository |

Existing warnings were observed. Cargo manifests, portable lock and dependency
pins are unchanged. The initial compilation and full suite precede a cosmetic
addition of the human label `Scroll` and the scoped scenario-hook repair;
final native and feature checks cover those changes.

## Native procedure

Use a fresh `TURNSTONE_ROOT` under this receipt's ignored `profile/`, set
`TURNSTONE_SCENARIO` to the absolute scenario path and `TURNSTONE_CAPTURE_DIR`
to this receipt's `native/`, then launch the stable-target `turnstone.exe`.
The scenario keeps its offline node at rest and requires no CEF runtime.
Four captures inspect inventory, Reader selection, disabled-row refusal and
Auto restoration. Unknown restored pins and failed-content recovery are
separate unit gates. This does not prove scrolling in a small Inspector pane,
keyboard/assistive acceptance or native hosted-page controls.

The first native run is preserved in `native-initial/`; its source manifest
is `source-manifest-initial.json` at this receipt's root. It failed final Auto restoration: host
hooks incorrectly rejected ordinary scoped pane clicks. Those hooks now defer
to the standard DOM probes, retaining Taproot's surface-scope checks. The
accepted scenario proves an enabled scoped choice before testing disabled
rows, and restores the ordinary pane before Auto to check a second geometry.
Its final capture maximizes Inspector to show Auto clear of the floating
toolbar; overlay/scroll acceptance in a small pane remains separate.
The first run's disabled-row assertions do not qualify actual click delivery.
`native-hook-repaired/` records the successful hook-repair run before the
scenario's final capture was adjusted to show Auto clearly. `native/` is the
final accepted and reviewed run.

## Stack and release evidence

`registry-status.json` records current crates.io package versions and
`workflow-status.json` records fresh GitHub results read during this pass.
Those reads do not rerun hardware. Exact source files are recorded separately
in `source-manifest.json` and `stack-source-manifest.json`. The former records
144 Turnstone inputs, the executable hash, Rust/Cargo versions and Windows
build. The latter records the exact adapter and workflow/helper inputs.
`native-artifact-manifest.json` records hashes of the accepted captures,
diagnostics and sentinel. Final verification found no source/executable
manifest mismatch and no portable Cargo manifest or lock change.

- Published baseline: Graft 0.6.0, Graft Frame 0.1.0, Scry 0.7.1, Weld 0.14.1.
- Weld `65d057d` parity run `37205146754` passed all four hosts. Hardware run
  `37197779933` passed both Macs and NVIDIA; RADV stopped at an expired logind
  session ID before pixel execution.
- Scry `99c0a9d` hardware run `37210994529` passed M4 and NVIDIA. Intel base
  cadence captured three complete frames against five required; resize passed.
  This failure does not establish its cause. RADV stopped at the same stale
  preflight. Local patches discover an active Wayland session owned by the
  runner UID, and execute mocked refusal checks before actual admission.
- Native RADV reruns, Scry Intel cadence acceptance and native capture-counter
  test execution remain open. Next public versions need qualification,
  exact-source packaging and a new registry-only consumer receipt.

The taxonomy plan owns the next Turnstone integration slices. The Graft
cross-repository release plan owns package qualification. No release was
published or workflow dispatched in this pass.

## Workspace

Unrelated Turnstone `.github/` work and other active Mere/Genet lanes are
preserved. No worktree or isolated Cargo home was created. The stable reusable
`C:/t/cargo-targets/turnstone` target is retained for Turnstone qualification;
the existing Mere target remains owned by its concurrent repository lanes.
The private native profile is excluded from Git; captures and receipts remain
source evidence. Automatic approval review rejected removal of the marked
synthetic profiles with the reason `blocked by policy`; those ignored generated
fixtures remain under `profile/`, owned by this qualification pass.
