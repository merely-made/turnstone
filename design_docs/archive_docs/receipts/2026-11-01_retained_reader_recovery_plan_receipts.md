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
