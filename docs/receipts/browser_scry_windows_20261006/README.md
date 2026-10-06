# Turnstone Windows browser follow-up, 2026-10-06

This receipt separates diagnostic runs, immutable dependency qualification,
and the final consumer runs. The prior 20261005 receipt remains frozen.

## Changes and ownership

Turnstone converts canonical Windows verbatim profile paths to ordinary DOS or
UNC paths before passing them to native browser constructors. It preserves the
resolved directory and refuses device paths. A controlled CookieManager probe
found native cookies present before reconstruction but absent afterward with
the verbatim path, and no Cookies database. With ordinary paths, both native
cookie databases appear and reload, reconstruction and process restart pass.
The diagnostic probe reads native metadata only; production contains no cookie
replay or fabricated fixture answers. `cookie-probe.patch` preserves the probe.

The root lock selects Scry queue-freshness repair `39818a7` directly. Weld's
package identity is aligned to the unchanged runtime source `4784d07` used by
Mere's opt-in `welding-0-15` adapter. Mere and Genet pins remain unchanged. `qualified-dependencies.json` records
the actual resolved sources and features: one wgpu 30.0.1 package, with both
registry and Git Graft 0.6 packages already present in this consumer graph.
The shared adapter now owns event and command translation, with a narrow host
mouse/character bridge for the pinned adapter's rejected mouse PointerEvent
and absent native CHAR delivery. `mere-weld-input-followup.md` records the
shared owner and removal gate. The native producer alias forwards all 60 trait
methods to the same instance; there is no second producer or event queue. Turnstone owns the
runtime, profiles, product capability limits and host-device import. Owned
Weld frame metadata is checked before import and every paint is handed to the
producer helper, including a reused epoch. A failure releases custody and
clears the cached host texture.

The direct native find fixture exposed a separate product bug: graph selection
B still controlled find, zoom offers and browser chrome after Workbench A
became the browser command target. Those paths now use the same browser target;
graph selection remains B. Regression coverage checks differing capabilities
in both directions, actual effect targets and retained chrome URLs/Keep intent.

## Existing qualification and controls

`native-repaired` preserves the freshness-repaired run that still failed four
cookie assertions. `native-cookie-probe` preserves the native profile-path
control. `native-normal-profile`, `native-complete` and `native-restart` pass
with the profile fix; they used the local diagnostic qualification build.

`native-production` and `native-production-restart` pass against immutable
locked Git dependencies and contain no probe. `production-source-manifest.json`
fingerprints that snapshot. They predate the browser-target corrections.
`native-weld-direct` and `native-weld-final` preserve the failures that led to
the honest partial zoom claim and the browser-target fix.

The combined library run before the final target correction passed 664 tests
with nine existing ignores. The shell subset passed 58 tests. These are retained
as `browser-production-lib-tests.log` and `browser-production-shell-tests.log`.
The existing environment-mutating Weld sandbox tests race under parallel test
execution; the retained serial Weld contract run passes four tests.

## Final qualification

The pre-input-bridge snapshot passes `native-current`,
`native-current-restart` and `native-weld-current`; its source/executable
fingerprints are in `current-source-manifest.json`. `native-weld-permission`
preserves the real mouse-pointer refusal that exposed the bridge requirement.
`native-current-harness-stderr` preserves a launch stopped by Windows
PowerShell's fatal treatment of redirected routine native stderr. The runner
now gates native exit status and `scenario.done` explicitly.

The pre-character-code locked production build passes with both features. The hashed test
binary records 665 passing tests, nine existing ignores and one failure in the
parallel broad run, with two sandbox environment tests filtered and then
passing serially. The network failure is
`a_writer_admission_carries_its_delegation_to_a_connected_joiner`: membership
converged, but an extra sync round meant reconciliation instead of the expected
live push. Its isolated exact retry passes. All 668 non-ignored cases therefore
pass across these runs, but the broad run itself is not green. The unchanged
network lane remains an explicit aggregate reliability limit. The earlier full
664-test source snapshot is a separate result.

`library-test-binary.json` records the exact binary hash, Cargo compilation and
actual direct invocations. Direct invocation avoids another shared package-cache
wait; the serialized partial run is preserved and is not counted as a completed
suite. The new mouse/character, find-target and chrome-target regressions pass.

`native-weld-permission-final` and its loopback server both report RESULT ok
on the same final executable as the Scry, restart and two-page Weld runs.
A granted request earlier reached `permission-answered` but waited for the OS
location service; `native-weld-permission-unbounded-location` preserves it.
The timeout experiment (`native-weld-permission-api-timeout`) navigated away
before the decision and is a failed control, not qualification. The final
fixture restores the real unbounded geolocation call and drives Deny: an actual
native callback, result-page navigation/render and zero-producer teardown pass.
It makes no claim about obtaining an actual location after Grant.

The final character-code correction then passes all six Weld contract/sandbox
tests serially and rebuilds with `--offline --locked --features scry,weld`.
`character-code-test-binary.json` records this newer test binary. A real fixture
had produced ALPHA/BRAVO from lowercase text because CHAR used a virtual key
code. CHAR now uses the actual character code; raw down/up keep their key codes.
`native-weld-qualified` preserves the uppercase failure. The corrected final
`native-weld-final-input` passes independent alpha/bravo input and clicks,
native find (one highlighted match on A while graph selection remains B),
requested 110% zoom only on A, reset and zero-producer teardown.

`native-scry-final` and `native-scry-final-restart` both report RESULT ok on
the final production executable. Two primary Workbench pages have independent
input, pointer clicks, scroll and site state. Native cookies survive reload,
producer reconstruction and separate-process restoration at the same loopback
origin. Resize, Reader/Scry switching, reopened current pixels and restored
engine pins pass. Closed state has zero producers, zero cached frames and no
per-producer importers; this does not claim the shared factory root is destroyed.
Visual captures show current alpha/bravo pages after input, reconstruction and
restart. Scry reports DPR 2 and Weld DPR 1 on this host; cross-engine DPI parity
is not claimed.

`native-final` preserves one failed Scry scroll-title assertion: the wheel title
arrived before the default scroll/title callback. The scenario allows 160
settling frames instead of 80 on the two upward scrolls, retaining the exact
required final offset and wheel counts. `native-scry-qualified` and its restart
also pass at the prior character-code snapshot.

`final-source-manifest.json` freezes all Rust source, Cargo files, scenarios,
fixture HTML/CSS/JS and the final executable SHA-256
`1e1893c4ac2a61f6af2c05e3ea2049028274ecf26ccbed50b8d5ff9507d5d967`.
Fingerprints were verified unchanged after all four runs. `facts.json` collects
their results and scope; `visual-review.json` records the inspected captures;
`artifact-manifest.json` hashes the retained evidence, excluding synthetic
profiles and the manifest itself. Earlier failed controls remain preserved.

## Hardware supplier gates

Scry `97b7579` passes RADV imported pixels, page input and the actual-constructor
worker-thread refusal regression in run 37533331164. Its native WPE tests run
on the process main thread. Both Macs pass the capture-size/DPI unit test but
fail native capture cadence with unchanged thresholds: Intel four versus five
base portrait frames; M4 three versus five base and two versus three final
portrait resize frames. Its NVIDIA job remains queued at this snapshot.
Weld `b5cf043`, run 37530088110, passes RADV, Intel and M4; NVIDIA is queued.
The final workflow refresh confirms the same statuses in
`scry-hardware-final-status.json` and `weld-hardware-final-status.json`.
Structured workflow snapshots and logs are retained here. No new package was
published. Fresh passing hardware, exact-source package/tag and registry-only
consumer proof remain required before the next release.

## Open application gates

Servo still needs a process-owned event loop and real browser producer on the
current host device, ordered events, explicit profile capability, frame-origin
handling and native input/resize/teardown proof. The Graft importer alone does
not supply these. Broader Mere integration remains held at its reviewed boundary.
Scry typed find/zoom and permission/auth forwarding remain unavailable through
the pinned adapter. Physical keyboard, supplementary Unicode, OS IME, lenses, sandbox-bootstrap
qualification, popup composition, downloads and browser lifecycle policy remain
separate owner slices. Weld requested zoom is partial because effective native
zoom readback is absent in its threaded host.

## Reproduction and retained state

Set `CEF_PATH` to `C:/Users/mark_/Code/cef-cache/wgpu-weld/151.3.24/cef_windows_x86_64`.
Use `cargo build --offline --locked --features scry,weld --bin turnstone
--target-dir C:/t/cargo-targets/turnstone`, start the static loopback fixture on
127.0.0.1:43123, then invoke `run-current.ps1`. Existing receipt directories
are refused to preserve evidence. CEF uses the existing 151.3.24 Windows SDK.
The direct executable uses the explicit trusted-content unsandboxed route.
All interactions are selfdrive mouse/CDP input, not physical keyboard claims.

The stable Turnstone target is reused. No isolated Cargo home or worktree was
created. Synthetic profiles under ignored `profile/` belong to this receipt and
are retained for exact-profile restart evidence. No source or historical
capture evidence was deleted. The `.github/` work in Turnstone and unrelated
supplier documentation changes are preserved.
