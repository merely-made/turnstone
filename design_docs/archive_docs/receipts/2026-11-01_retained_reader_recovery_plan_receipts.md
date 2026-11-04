# Retained reader recovery and readability receipts

## 2026-11-01: implementation and negative controls

Base: Turnstone `73565e33`; original M5 native receipt remains frozen to
`554f323f`. Supplier Woodshed `676d625dfdd13f55f454ac8d1ca81c061a03faca`
passed 112 shared-surface tests and was pushed before receiving it here.
The lock changes exactly six Woodshed source hashes. `cargo metadata --locked
--offline --format-version 1` succeeds; the ordinary targeted update failed on
an unrelated newer Mere `chirograph` version and did not rewrite the lock.

Runs use `Code/testing/custody-contract-20261029/m2-queued-gate.py`, shared
`machine-heavy.lock`, 3 GiB memory, zero scope swap, CPU 150 percent, one
build/test worker and a 600-second command cap. Logs and JSON remain outside
the repository under `Code/testing/custody-contract-20261029/`.

- `followups-fetch-20261101-1`: locked dependency fetch, exit 0.
- `followups-build-20261101-1`: initial host test compilation, exit 0;
  367.86 seconds command time, maximum RSS 2905488 KiB, swaps 0.
- `followups-recovery-red-20261101-1`: four helper negative controls fail at
  the deliberately unimplemented deferral method, as intended. This is a
  behavioral test baseline for the new owner, not a native reproduction.
  Log SHA-256 `f89792024fc263b08d31355d73c7c6499027826c797befc635e8d5eda2471c54`.
- `followups-layout-probe-20261101-1`: comparison row passes; initial tall
  research card, wrapped hint and tall hint tests fail. The card's height
  excluded its padding/border, and the hint needed an auto-height block
  inside its fixed-height scroll owner. Failed evidence is preserved.
  Log SHA-256 `dd2b158f78178319816e31e0cf708a94c0c17cdec82308bac5de7719e8fb1f5d`.

Source review checked the custody barrier, source currentness and pane lifetime,
and the shared note action identities. Subsequent software/native results are
recorded below, without extending the original M5 qualification.

## First full run and focused corrections

`followups-host-full-20261101-1`: 1420 passed, 2 failed, 21 ignored, exit 101.
The two new fixture failures were the Inspector's guessed column-width threshold
(actual visible rectangle 280 by 182 pixels) and the mounted workbench fixture's
unestablished nonzero offset. All seven recovery cases, the real Redshank consumer
layout and standalone error scrolling passed. The unchanged existing tests passed.
Log SHA-256 `1fd448ac8f87b31f487ccfebccf59f27757727022e77eefb468421b9a50b884e`.
Changed source/lock files for this run are preserved under
`Code/testing/retained-reader-followups-20261101/first-full-suite-source/`.

The Inspector regression now uses actual unclipped row/value geometry and bounded
scrolling to the real action. Research cards now clamp their width to narrow
windows; the standard-width row estimate remains a suggestion-count estimate,
while the measured card viewport and scroll owner enforce access to taller rows.

`followups-focused-20261101-1`: 14 selected cases passed, the mounted fixture's
offset assertion still failed. Source inspection showed that translating the
layout root does not establish a descendant offset. The corrected fixture uses
an unshifted outer window and a translated descendant host, preserving its
nonzero-offset and sibling-isolation assertions. The focused runner selects the
actual test binary from Cargo compiler-artifact metadata, checks every filter is
nonempty, and never selects executables by filesystem glob.

## Full host verification and final cancellation regression

`followups-focused-20261101-2`: all 15 selected cases passed, followed by the
complete host library suite from the same Cargo-selected artifact: 1423 passed,
0 failed, 21 ignored. The suite took 321.50 seconds; compilation and both test
passes together took 467.95 seconds, maximum RSS 2992816 KiB, swaps 0. The shared
lock wait is additional and is included in the outer JSON elapsed time.
Log SHA-256 `6f800de11df522b62cf2fee9d802fecb4c843eeafc830a4620bafcb242ad0ff5`.

A final source review found a cancellation gap beyond those passing cases:
pruning a closed tile removed its queued request but left `AwaitingCustody`,
preventing a later explicit reopen from spawning. A strengthened regression
checks close, prune, and explicit reopen. Its correction and focused results
are recorded separately below rather than attributed to the earlier full run.

`followups-reopen-20261101-1` exhausted its 600-second shared-lock acquisition
wait without executing the command (empty log, exit 1). It is a queue timeout,
not a failed test. The second attempt acquired the lock within the same limit.

The same review found that moving a waiting Workbench tile to a detached Tile
pane must preserve its request. Current reader ownership therefore follows the
same graph/member into either pane kind, while session, generation and exact URL
checks still govern replay. Closing every reader cancels the pending intent.

`followups-reopen-20261101-2`: the strengthened closed-tile test first fails
because explicit reopen emits no `SpawnContent`. After the correction, all 17
selected tests pass, including nine recovery cases. This adds detached-pane
continuity, replacement and newer-request preservation to the recovery coverage.
Command time 354.42 seconds, maximum RSS 2975688 KiB, swaps 0; exit 0.
Log SHA-256 `f5f310493c5007ef60ee3c157c90b9976eb37f49933fb19cf6927afd12199766`.

Review then identified a separate authority mismatch: `browser.content_on` is
a save-time sidecar, while an explicit graph toggle changes current content
state without saving first. Recovery must use current content state, not that
possibly stale sidecar. A regression through the real toggle action covers it.
An initially graph-owned reader remains independently open if a secondary
Workbench tile is later opened and closed; pane ownership is not latched from
that incidental projection.

## Final source verification and limits

`followups-sidecar-20261101-1`: the real graph-toggle regression first fails
because the stale saved flag suppresses replay. Removing that guard and using
the live content state resolves it. The test helper no longer prepopulates the
saved flag, and closing is exercised through current content state and the real
toggle action. All 18 final selected cases pass: 10 recovery and eight layout.
The complete host test binary compiles with locked offline dependencies; exit 0.
Command time 324.69 seconds, maximum RSS 2966560 KiB, swaps 0.
Log SHA-256 `aeba62ed0bb48da44a771aa7d75d55f01f3fe5649e43afa4183e74302c43491a`.

This final run permits a 3600-second lock-acquisition wait after earlier queue
contention; it acquired the lock promptly. The 600-second execution limit,
3 GiB memory limit, zero scope swap, CPU quota and single worker are unchanged.
Runner: `Code/testing/retained-reader-followups-20261101/queued-gate.py`.
Cargo-selected artifacts and source inventory are retained in that directory.

Source review found no further issue in the agreed scope. Source-size and
whitespace checks pass. The complete host suite was not repeated after the two
localized recovery corrections; the final run compiles the whole host and
rechecks all recovery and layout cases affected by this work.

These results establish software lifecycle and retained-layout behavior.
Fresh native startup/unlock, physical scrolling and screen-reader interaction
are **UNTESTED** for this source. They do not extend the original M5 native
receipt. Explicit custody locking still retires already-live readers; this
change resumes pending startup or explicitly requested reads after readiness.


## Native-window follow-up, 2026-11-03

Upstream was pulled before this run: Turnstone `4321a55482bcbe7d9bbc48e32c6a0c91e2dc82c5`,
Mere `f673d313`, Genet `ad362723`, Woodshed `676d625d`. The application was built
with locked dependencies and `authoring,inference`. Cargo compiler-artifact JSON
selected the executable; its frozen SHA-256 is
`ae402dd509306bf8d4a7f7f32768cf8d54ba39c4b0b17e3a9763f8d5dd7380dd`.
Only this plan's documentation was dirty during compilation. Source hashes and
artifact metadata are in `Code/testing/retained-reader-native-20261103/build-provenance.json`.

The fresh build passed in 294.97 seconds with maximum RSS 2645936 KiB and zero
scope swaps. An initial offline build lacked the locked Murm checkout; a locked
fetch supplied it without changing dependency selections. Native cases use the
shared machine lock, 3 GiB memory, zero scope swap, 150 percent CPU, 128 tasks,
a 360-second application deadline and 420-second scope backstop. They render
actual Wayland windows on T-PC using AMD Radeon Graphics (RADV RENOIR), Vulkan
and Mesa 26.2.5. System-wide swap already existed; zero swap refers to these scopes.

Each launched case gets disposable profile, custody and Mere copies from the preserved
`Code/testing/m5-intact-collection-20261030-w` fixture. Stale Unix sockets are
inventoried but excluded from copies. The frozen historical custody test host
serves that data; it is not a freshly built custody daemon qualification. The
application starts before custody, then the scenario starts the fixture and
verifies its Unix peer. The namespace exposes only loopback, the selected GPU,
Wayland socket and explicit fixture files; acquisition stays offline. Original
fixture inventories are compared after each run. Child cleanup and empty scope
verification are recorded separately.

Raw runs, frozen scripts, inputs, screenshots, structured observations and logs
are under `Code/testing/retained-reader-native-20261103/`.

- `recovery-1`: preparation rejected a stale copied Unix socket before launch.
  The corrected copier preserves its original identity while excluding sockets.
- `recovery-2`: automatic startup recovery and exact A1/A2 reopen reached five
  nonblank captures, but the scenario failed on Genet's native frame qualification
  deadline after 301 presented frames. This remains failed evidence. The error is
  `native frame qualification exceeded five seconds`; preceding accepted frames
  do not qualify the rejected frame.
- `startup-1`: a nonexistent host font-cache mount prevented application launch.
  The failed scope was absent before its own waiting supervisor was interrupted.
  No unrelated process was stopped. Subsequent cases mount the existing font
  configuration directory only.
- `startup-2`: **PASS**, 92 presented frames, two nonblank captures. The initial
  screenshot shows waiting readers with no custody principal. After custody
  becomes ready, captures, the retained selection and retained note render
  automatically without an open or toggle action. Saved-source diagnostics show
  A1 and A2 Ready, with exact retained text and no reader error. Startup clip
  selectors are not persisted in the in-memory citation map, so the clip claim
  here is partial visual recovery rather than a reconstructed selector observation.
  The old saved layout squeezes these readers into narrow, clipped columns;
  this run does not establish full-body readability. Exact A1/A2 bodies above
  are verified in structured observations, not both fully visible in the frame.
  Runtime records show zero generation requests and zero acquisition attempts;
  the restored Answers panel attempts the unavailable inference route five times.
  The fixture supplies no model service. Original data is unchanged and the scope
  is verified empty after cleanup.

The frame deadline begins at batch admission, before raster/seal, and includes
error-scope resolution and Classic curve-quality backlog clearance. Existing
logs do not identify which condition exceeded it or establish CPU scheduling as
the cause. A fresh Genet upstream fetch through `70a54f6f43c` found only lint
manifest changes in `genet-render-host` since the selected pin, with no fix to
this path. The gate and resource caps remain unchanged.

### Native findings after the startup pass

- `recovery-3`: **FAIL**, the five-second native frame qualification refusal
  recurs with font configuration mounted. Four nonblank captures and 268 frames
  precede it. Automatic recovery and A1 reopen reached observations; later steps
  did not. The timeout is reproducible across these two longer runs, without a
  demonstrated cause. Queue supervisors had earlier exited with signal 15 before
  these application runs; `queue-interruption.json` keeps that separate.
- `comparison-1`: scenario **FAIL** because the expected manifest omitted its
  `sealed-v1:blake3:` prefix. Actual identity was correct. `comparison-2` corrects
  that expectation and passes: 245 frames, seven nonblank captures, clean exit.
  Its observation records `eidetic.line-comparison/v1`, Explicit relationship,
  exact A1/A2 manifests and hashes, deletion of bytes 0..57 and insertion of
  bytes 0..59. The whole-left action reopens A1. Long identity rows visibly wrap.
  Wheel input inside the research card does not visibly expose its lower rows
  in this run, so research-card wheel access remains unqualified. Keyboard
  selection reaches the action. Inspector URL wrapping is visible; positive
  wheel delta reaches the lower metadata. The fixture labels for bottom/return
  assumed the opposite sign: the actual lower Inspector capture is
  `27_inspector_return.png`. A return-to-top was not verified.
- `note-1`: scenario **PASS**, 445 frames, five nonblank captures and clean exit;
  visual note layout **FAIL**. A real long note is saved, and its More actions /
  Make shareable control works. Controls occupy a separate row below the body,
  but the body paints as one horizontally clipped line in a roughly 510-pixel
  pane, leaving blank reserved height beneath. Supplier CSS requests wrapping;
  a prepared-text/layout/paint mismatch is a lead, not an established cause.
- `error-1`: scenario **PASS**, 332 frames and three nonblank captures, but process
  **FAIL**, SIGSEGV at exit. The long real file-load error wraps and its final
  sentinel is visible, but the entire error fits at the actual window size, so
  this is not an overflow-scroll test. The preserved core and symbol lookup
  show `ash::device::Device::destroy_command_pool` on the crashing thread while
  the main thread is in validation-layer device destruction through
  `ApplicationAtExit`, libc exit handlers and `std::process::exit`. This supports
  investigating GPU worker teardown versus process exit; it does not establish
  a validation-layer defect. Core SHA-256:
  `b65e31c02e5d973013ddd5286a766622a43d125a51a8ceb9df103c526715d05a`.

Every captured image above is 1024 by 600. The driver's `resize` command requests
an inner size without confirming a resize event, and these requests did not
produce the intended smaller window. Narrow-window qualification is therefore
**UNTESTED**. Source inspection also finds opposite delta conventions:
`PaneScroll::nudge` adds the delta, while element overflow scrolling subtracts it.
The recorded input directions must not be inferred from capture filenames.

A full Genet tree comparison from `ad362723` to upstream `70a54f6f43c`, including
merged changes, contains no Rust source or lockfile changes. It supplies no
runtime fix for these findings. The newer
[Vulkan SDK 1.4.363.1 release](https://vulkan.lunarg.com/doc/view/latest/windows/release_notes.html)
points to a [cross-queue semaphore synchronization fix](https://github.com/KhronosGroup/Vulkan-ValidationLayers/commit/317cff060893783f96d4174f50491e6374eb3714),
not a demonstrated fix for this exit crash. The still-open
[wgpu shutdown report](https://github.com/gfx-rs/wgpu/issues/8365) describes an
analogous process-exit/thread-destruction interaction on different hardware;
it is a diagnostic lead, not identification of the same defect. No system
package or dependency was changed on that basis.

- `error-2`: **FAIL**, 331 presented frames, three entirely blank captures,
  clean application exit. This uses a real missing file URL with 40 components
  instead of 14 to force more error text; each PNG has zero in every RGBA channel.
  The driver rejects the run as `blank main capture`. It does not establish error
  scrolling, nor does it identify which rendering path lost the frame.
- `clips-1`: **FAIL**, the same five-second qualification refusal after 62 frames,
  before any explicit clip observation or capture. Exact explicit selection/note
  reopening remains unqualified for this cohort. Startup partial visual recovery
  remains independently supported by `startup-2`.

`evidence-summary.json` hashes the raw observation/capture files, verifies the
exact comparison identities and spans, checks the recorded build-source hashes
against the final tree, and confirms original-fixture integrity and empty
native scopes for all nine application runs. Measured scope memory peaks across
the seven later runs range from 384647168 to 535437312 bytes, with no OOM events
or swap-limit events. Those counters were not collected for the first two runs.
All native execution was serialized; the earlier shared-lock wait overlapped
only with another job holding that lock. An idle S-PC check found about 1.4 GiB
available with nearly full swap, so no workload was transferred there.

The native-window check is complete with mixed results. It does **not** qualify
the entire baseline. Physical mouse-wheel hardware, screen-reader interaction,
a confirmed smaller native window and an interactive identity-unlock flow remain
**UNTESTED**. No production source or dependency change was made in this pass.

### Next checks and done-conditions

1. **Frame refusal and blank output:** instrument batch admission, seal, worker
   start and last observed scope/backlog state without weakening the deadline.
   Reproduce the 40-component missing-file case and explain its blank frames.
   Done when the underlying cause is corrected and the preserved failing native
   scenarios pass with nonblank frames under the same resource caps.
2. **Long note painting:** reproduce the wrapping flex-span in the supplier and
   actual Genet renderer; compare measured fragments, prepared line runs and
   fallback paint. Done when the whole saved note is visibly wrapped or reachable
   in its actual pane, with the existing working actions below it.
3. **GPU shutdown:** establish worker/device teardown completion before process
   exit. Done when the preserved error scenario exits cleanly repeatedly with
   validation enabled, alongside a regression for the identified lifetime defect.
4. **Input and sizing:** confirm actual dimensions before a narrow-window claim;
   resolve the opposite pane/element wheel conventions and missing research-card
   movement. Done when native top/bottom/return captures and the matching action
   demonstrate access to the full comparison and long error text.

After these corrections, rerun explicit capture, selection and note opens and
retain exact source diagnostics alongside native frames. Historical and passing
subset receipts must keep their recorded source boundaries.


## Rendering and shutdown repair, 2026-11-03 (in progress)

Genet `adfa1905d9e` adds explicit native completion-worker shutdown; its receipt
is `4fce4b5d7fc`. Shared recovery retains lightweight worker handles across device
generations, seals admission, waits for active publication and joins workers.
Reentrant closure refuses explicitly. Turnstone cancels queued frames and closes
all shared recovery owners during normal exit and Shell drop, before releasing
GPU hosts. The 32 Genet lock-source entries advance together; package versions
and the rest of the graph are preserved. Temporary paint diagnostics are kept
out of the receiving commit.

Supplier verification: five CPU tests and 16 actual-GPU native-frame tests pass;
changed-file line ceiling passes, and package-scoped Clippy exits zero with
warnings. Cargo-deny is unavailable. Exact commands, log hashes and source
inventory are in `genet/design_docs/2026-10-30_nonblocking_native_frames_plan.md`.
Independent review found and resolved a late-publication race before verification.
The tests were not observed failing against the original implementation.

Consumer artifacts are under `Code/testing/render-shutdown-fixes-20261103/`.
Locked fetch succeeds. The first native build passes in 163.6 seconds, maximum
RSS 2611944 KiB, zero scope swaps; executable SHA-256
`58f510c2d29159ccf3f7c2eb0b2b57cc5919cd7086a4894e7e25e20b9f90390b`.
It includes an opt-in temporary diagnostic at a paint entry point that did not
run for the note; that failed probe remains preserved. `note-probe-1` passes its
scenario and exits cleanly, while visual clipping remains. `shutdown-error-1`
reruns the original 14-component missing-file scenario: 332 frames, three
nonblank captures, scenario and process pass. This is one clean rerun of the
previously crashing case, not proof of repeated clean exits.

The corrected probe build passes in 78.56 seconds of execution (249.38 seconds
including lock wait), maximum RSS 2659200 KiB, zero scope swaps. Its SHA-256 is
`d5356d25f31f348e1bfc0afea93a22e295c87e27c6b976d46e2e97a87d67bdc5`.
Its temporary diagnostic captures the actual note and workbench paint paths.
`probe2/note-probe-2` again exits cleanly but clips text: computed wrap is enabled,
yet the note fragment is about 2686 pixels wide inside a 511-pixel pane.
`probe2/overflow-probe-2` produces three blank captures and fails its scenario;
its pre-raster paint data contains opaque background and correctly wrapped text.
Those rendering failures are still under investigation. Each native case uses
fresh disposable fixture copies and retains the original integrity/cleanup
checks. Resource caps and five-second frame qualification remain unchanged.

Release/source review found no direct upgrade remedy: wgpu is already at listed
release 30.0.1; the analogous [wgpu exit report](https://github.com/gfx-rs/wgpu/issues/8365)
remains open. New Netrender changes are source/test reorganization. The newer
Vello fork shader fix addresses Naga SPIR-V output in `vello_gpu_shaders`, which
is outside this Classic backend's locked graph. No provider upgrade was made
without a matching failure mechanism.


### Root causes and corrected probe cohort, 2026-11-03

The preserved probe cohort `probe3` receives Genet `cbedf7b5` and Woodshed
`e729fdad`. Its frozen executable SHA-256 is
`7a101dc6e3745e3ce9c08beb2e1ee7c912ab64ccf264846df6450c4efc3cc31b`.
It also includes temporary paint and Vello allocation diagnostics, so these
results qualify that diagnostic cohort. The build passes in 179.5 seconds,
maximum RSS 2679756 KiB, zero scope swaps. The temporary paint source patch is
preserved under `probe3/paint-snapshot-diagnostic.patch` and removed before the
final receiving build.

The note defect comes from the shared phone rule enabling flex wrapping on an
already vertical note row. Woodshed now keeps that row unwrapped while its
text and action controls wrap independently. The full-theme regression was
observed failing before the fix; all 113 surface tests pass. In the actual
511-pixel native pane, `note-fixed-1` shows the complete note in seven lines,
with its controls below it: text width 399 pixels and height 126, replacing the
2686-pixel single-line fragment. Its scenario passes with 445 frames, five
nonblank captures and clean exit. Captures remain 1024 by 600; the unacknowledged
smaller-window request still does not qualify a narrower native window.

The five-second refusal also has an identified cause. GPU completion could
qualify within its deadline, then be refused again when a delayed UI collected
or published the result. Genet `33b23ed6` separates worker deadline checks from
later live-device and publication checks; the original deadline remains enforced
through the worker's final local scope retirement. Loss, closure and failed
worker results still refuse publication. A regression was observed red before
the fix; five CPU tests and 17 actual-GPU tests pass. `cbedf7b5` merges current
upstream with identical render-host source. In `recovery-fixed-1`, 321 native
frames and seven nonblank captures pass, including delayed-custody startup,
exact A1/A2 opens, and explicit saved selection and note opens. `clips-fixed-1`
passes 127 frames and two nonblank captures. Recorded rendered documents retain
the exact manifests, hashes, selectors and text. Runtime records show offline
mode, zero acquisition attempts, zero generation requests and zero corpus
publications. The resident model-load counter is unavailable, not asserted zero.

`shutdown-error-2` repeats the previously crashing 14-component missing-file
scenario: 332 frames, three nonblank captures and clean exit. This is the second
clean rerun, across two repair-cohort builds. All five `probe3` cases exit the
application cleanly, including the blank-frame failure. The original fixture
hashes are unchanged and each native scope is confirmed empty. Native memory
peaks range from 438702080 to 789180416 bytes, with no OOM or swap-limit events.

The 40-component error still fails on the target: `overflow-bump-1` presents
331 frames and produces three entirely transparent captures. A4 evidence shows
opaque pre-raster paint data and a valid scene containing 6062 glyphs in 45 runs,
but both raw window raster and final composition are zero. Vello reports flatten
allocation failure `0x4`, requiring roughly 8.7 to 9.3 million line records. A
line record is 24 bytes; the requested 128 MiB storage binding permits at most
5592405 records. The tile lowering repeatedly encodes entire glyph runs for
intersecting 64-pixel tiles. A supplier regression confirms that a first tile
encodes all 100 glyphs of a run spanning many tiles. Reducing that duplication
and refusing unqualified allocation results are under verification; increasing
buffer limits or lowering curve quality is not the repair.

A bounded diagnostic on idle S-PC uses the identical frozen executable under
900 MiB, zero scope swap and 150 percent CPU. It reports the same allocation
failure, requiring about 15.17 million line records. Its later three captures
are nonblank and exit is clean, unlike the target's final blank captures; this
is supporting diagnostic evidence, not equivalent target acceptance. Peak
memory is 747937792 bytes, without OOM or swap-limit events; its fixture hashes
are unchanged and scope is empty. Raw evidence is copied to
`remote-evidence/`, retaining the failed and passing observations separately.

`probe3-evidence-summary.json` hashes the native observations and captures,
checks all four exact retained targets and rendered text, verifies offline
runtime counters, and checks unchanged originals and empty native scopes.
The remaining blank-frame fix still requires a clean receiving build and the
preserved native acceptance scenarios.


The release check was repeated before receiving the final renderer. Official
[wgpu 30.0.1](https://github.com/gfx-rs/wgpu/releases/tag/v30.0.1) remains the
latest listed release and is already selected. Its Vulkan change concerns the
acquire fence, not completion-worker lifetime. Official
[Vello Research 0.11.0](https://github.com/linebender/vello/releases/tag/v0.11.0)
updates wgpu/Naga and fixes bitmap-glyph placement. Its release commit
`f3000c8d9e7a9c7e6abb09b9587238bf8b4860ff` is already an ancestor of the original
`276b4664` fork pin, verified by fetching the release tag and checking ancestry.
Those release fixes were present when the preserved failures occurred.


### First clean receiving attempt

The receiving graph unifies Classic renderer, encoding and shader packages at
Vello `3d82d3b5`, with Netrender `1d37291c`, Genet `cbedf7b5` and Woodshed
`e729fdad`. Mere remains at its previously qualified `f673d313` cohort. Its
custom-paint package exposes `vello::Scene`, so merely updating Netrender would
leave two incompatible Scene providers. Root Git-source patches select the
same immutable Vello commit using the distinct `www.github.com` source URL;
Cargo forbids patching a Git source back to that identical source identifier.
The resolved graph contains exactly three Classic packages at one source and
no old Vello encoding or shader package. All unrelated package versions are
preserved. `final/graph-proof.json` records 43 changed package records plus the
two package renames. Locked offline metadata succeeds with `authoring,inference`.
The failed intermediate graph and same-source patch attempt are preserved.

The clean application build passes in 139.92 seconds of execution (218.09
seconds including lock wait), maximum RSS 2656596 KiB, zero swaps. Its frozen
executable SHA-256 is
`0e64bc7527b9b8930c5e6ddcb5fdc80076deec002c830a01914493b8a9eb34e6`.
No temporary paint or allocation diagnostic source or environment is included.
A sampled build cgroup reaches its 3 GiB cap and reclaims memory; OOM and
OOM-kill counters remain zero, and scope swap is zero. Raw build messages,
provenance, metadata and graph proof are under `final/`.

The culler passes 87 library tests, with one ignored. GPU parity controls check
opaque bytes, exact transparent alpha, and RGB wherever alpha is nonzero at
scales 0.75, 1 and 1.5. They include rotated variable glyphs; a modified font
with an understated glyph header exercises actual outline bounds through both
tile membership and lowering. Unknown/color outlines retain conservatively;
filtered layers disable glyph culling; scale changes invalidate paired caches.
RGB beneath zero alpha is excluded because unfiltered repeat controls vary
there too. The declared scoped Clippy command completes with warnings and the
staged line-ceiling check passes. A stricter warnings-as-errors attempt fails
on existing `netrender_device` warnings; that failed evidence is retained.

`final/overflow-final-1` now refuses an unqualified frame explicitly rather than
publishing a blank capture. It exits cleanly after 257 frames, before a capture,
with `AllocationShortfall`: failed bits 16, blend 1057792, lines 606525, binding
limit 134217728 bytes, and no line-binding exhaustion. Glyph work is reduced
from the earlier roughly nine million line records; the observed coarse-stage
shortfall is now below the binding cap. Its native memory peak is 818216960
bytes, with no OOM or swap-limit events. Original-fixture integrity and empty
scope cleanup are retained. This is still a failed acceptance run.

The next provider correction sizes blend spill storage before submission using
resolved clip/layer nesting and viewport tiles. Independent shader review
supports that conservative bound, including expanded color-glyph layers. The
estimate is capped at actual device limits; a loose upper bound does not itself
refuse an otherwise admissible scene. Actual GPU counter failures still refuse.
The preserved workload must pass again on the resulting immutable cohort.

During independent Mere manifest preparation, one Git cache pack briefly grew
to about 1.45 GiB outside the intended heavy scope. Its exact process tree was
stopped; an already cached object alternate supplies the missing source without
repacking. That exception is preserved in
`Code/testing/mere-vello-cohort-20261104/cache-incident.txt`. An earlier stopped
resolver left its scope child alive; the owned process group was identified and
retired, releasing the machine and package-cache locks. No unrelated process
was stopped. Build and native verification caps are unchanged.


## Qualified rendering and shutdown cohort, 2026-11-03

The final receiving cohort uses Netrender `6364e5d9`, Vello `b4a53486`, Genet
`cbedf7b5` and Woodshed `e729fdad`. Vello now pre-sizes blend spill storage after
scene resolution: maximum resolved clip nesting beyond the four local levels,
times the number of 16 by 16 viewport tiles, times 256 u32 entries. Saturating
arithmetic caps the estimate at the smaller device binding/buffer limit. Loose
estimates do not themselves refuse a scene; observed nonzero failure flags still
do. A fresh six-layer GPU scene reproduced the old shortfall before correction
and passes afterward, with pixel readback on the target Radeon. Ten CPU tests,
the GPU regression, no-default compilation and scoped Clippy pass. Netrender's
87 tests pass with one ignored; its all-features graph has one Vello revision.

The clean application build passes in 133.69 seconds, maximum RSS 2690072 KiB,
zero swaps. Frozen executable SHA-256:
`f44313c658f56ff6b4ebc0a2baa93ab3782d2c82ecc2e15b28d3b49e2faa2123`.
All seven native cases use this executable and fresh disposable fixture copies.

| Case | Presented frames | Captures | Result |
|---|---:|---:|---|
| `overflow-final-1` | 332 | 3 | PASS, nonblank captures, clean exit |
| `shutdown-final-1` | 333 | 3 | PASS, nonblank captures, clean exit |
| `note-final-1` | 446 | 5 | PASS, nonblank captures, clean exit |
| `comparison-final-1` | 246 | 7 | PASS, nonblank captures, clean exit |
| `recovery-final-1` | 321 | 7 | PASS, nonblank captures, clean exit |
| `clips-final-1` | 128 | 2 | PASS, nonblank captures, clean exit |
| `shutdown-final-2` | 333 | 3 | PASS, nonblank captures, clean exit |

That is 2139 presented frames and 30 nonblank captures. The original crashing
14-component error case passes twice on this final binary, alongside two clean
reruns on earlier repair builds. The formerly blank 40-component error now
renders; native top/bottom/return images show its first text, final path marker
and return to the first text. The long note shows all seven lines and working
controls below it in the actual 511-pixel pane. Delayed-custody startup, exact
A1/A2 opens and explicit selection/note opens pass. Comparison retains the exact
A1/A2 manifests and hashes, delete span 0..57 and insert span 0..59, and opens the
chosen left source. Inspector wraps the long exact source URL and scrolls.

`final2-evidence-summary.json` hashes observations/captures, checks all four
retained targets and rendered text, checks exact comparison identities/spans,
verifies all compiled root source hashes against the receiving tree, and verifies
the same frozen executable in all seven cases. Every recorded runtime is offline
with zero acquisition attempts, generation requests and corpus publications;
the unavailable resident-model-load counter is not asserted zero. Each case
confirms loopback-only networking, 3 GiB, zero scope swap, 150 percent CPU and
128 tasks. Native memory peaks range from 621514752 to 917430272 bytes, with
no OOM or swap-limit events. All originals remain unchanged and native scopes
are verified empty. Preserved earlier failures keep their original outcomes.

The full host test binary compiles and all 18 selected tests pass. Its first
outer runner exits 143 after the child records success, before writing its JSON
receipt; that incomplete supervision evidence remains. A frozen Cargo-selected
test executable rerun passes all 18 checks in 7.25 seconds, maximum RSS 370852
KiB, zero swaps, with a complete zero-exit receipt (`final-focused-2b`). The
repository's declared locked offline all-targets Clippy check also exits zero
with warnings (`final-clippy-2`); no warnings-as-errors claim is made. The
line-ceiling hook and whitespace check pass (`final-line-ceiling-2`).

This closes the observed note wrapping, false late-collection timeout, blank
raster and GPU-exit crash findings for this cohort. It does not close every
native follow-up: research-card wheel input still leaves later projected rows
unreachable by wheel, although keyboard actions work. That continuation defect
is being repaired separately with the existing bounded row window. Captures
remain 1024 by 600; physical wheel hardware, a confirmed smaller native window,
screen-reader interaction and interactive identity unlocking remain untested.

The optional Mere canonical pin preparation has an isolated Scene-identity
compile proof and 30 Sprigging unit-test passes. Full Mere offline lock resolution
is blocked by uncached Gemot `3766de48`; its three manifest edits remain
uncommitted/unpublished, with a preserved patch and inventory under
`Code/testing/mere-vello-cohort-20261104/`. No Mere source cohort intake is made
here; the receiving Git-source patch already guarantees the one Classic Scene
provider used by the qualified Turnstone executable.

## Research-card wheel completion, 2026-11-04

Turnstone `439e20c1` repairs continuation of the existing bounded row projection.
Pixel scrolling handles long rows first; at its boundary the shared input route
moves the row window only when another row is available. It cannot wrap at the
ends or advance a closed card. Ordinary and structured projections canonicalize
stored indices when results shrink. Query limits and row limits are preserved.
The actual omnibar is primary-only; the lens route returns Outside.

The production-route regression was observed red, then green. Four new controls
cover comparison continuation and pointer targeting, pixel offsets across normal
sync/redraw and projection replacement, closed-card state, and shrinking indices
in both row builders. All 63 selected host checks pass under a persistent
supervisor. The first outer runner again interrupted after the child succeeded;
its incomplete log remains. The full locked offline application test command
then passes: 1430 passed, zero failed, 21 ignored; binary targets and doctests
also succeed. Execution takes 425.86 seconds, maximum RSS 2625500 KiB, zero swaps.
Declared all-targets Clippy completes with warnings; no stricter claim is made.

The clean native build takes 78.75 seconds, maximum RSS 2678920 KiB, zero swaps.
Frozen executable SHA-256:
`aee4ff23559144410164b69d455240bf04679be05997f5331eea0359d180c870`.
Its six retained-source/rendering/shutdown scenarios and maximized pointer-open
acceptance case pass, alongside three supporting wheel/navigation observations:
ten runs, 2614 presented frames, 45 nonblank captures, all application exits zero.
One wheel reveals both change excerpts and whole-source actions; upward input
restores the headers and repeated downward input clamps. A real pointer click
at the visible left action creates a new A1 reader and address event. Its exact
manifest, whole-source selector, hash and 12-unit text are verified. Read document
and Maximize pane then visibly show A1 alongside the retained selection and note.

`final3-evidence-summary.json` verifies compiled source hashes, executable identity,
all exact retained targets and comparison spans, pointer navigation, offline
runtime counters, unchanged original fixtures, and empty native scopes. All
recorded acquisition, generation and corpus-publication counters remain zero;
the resident-model-load counter remains unavailable. No final application log
contains an allocation shortfall, capture refusal or failed worker retirement.
Native peaks range from 426590208 to 1671016448 bytes, with no OOM/swap-limit
events, under the same enforced 3 GiB/zero-swap/150-percent-CPU/128-task caps.
Physical wheel hardware, confirmed smaller native windows, screen readers and
interactive identity unlocking retain their untested status.
