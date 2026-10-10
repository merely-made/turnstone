# Turnstone application appearance qualification, October 10

The application appearance integration uses shared Tabard definitions, Genet
styles and rendering, the shared native WorkshopHost, and existing Mesquite and
Taproot acceptance paths. Turnstone's Settings provider remains the owner of
selection and persistence. Saving a definition is separate from applying it.

**Current status: native13 macOS production appearance acceptance passed.**
The final coherent candidate includes the shared Theme name/seed field styling
fix and Rootstock accessibility scroll-bounds fix. Four serialized LaunchServices
processes completed 521 successful presentations and nineteen nonblank captures
with no new GPU reset. All nineteen originals were inspected. Both repaired
fields paint at 34 CSS pixels (68 physical pixels at 2×), wide/narrow application
previews are populated, Save remains separate from ordinary application Apply,
and both initial and same-ID edited choices restore in fresh processes.
Native11 remains the completed historical qualification; native12 preserves
only its passed seed/reopen before the source-changing field fix arrived.
No temporary observer or profile/library/vault dump is in this production evidence.

## Current-origin field-fix qualification: native13

The production build passed in 197.217 seconds. Binary SHA256 is
`0b12ba1e6ad21153015c9e300ac087ce65196588bb50055aa0606aa7cd0c1dee`.
The [320-input source manifest](native-13-source-manifest.json), frozen on
Turnstone HEAD `42cc3b7d602e7e64f47362d6b6400eba051f8510` plus its scoped
dependency-pin changes, has digest
`1a657526ae830f27f1680a025d6be6ab4e62ee3a712f734c03d73a637aaa2d10`.
Source and binary hashes were unchanged after every lane. All six shipped
fixtures remain byte-identical; no native lane was retried.

The compiled family is Mere `e95326dc08446a9256d3e340c63e571684afe697`,
Knot `59db666b3186d0256dde87b27fa9ce128dbc0eae`, Woodshed/Redshank
`cf9b070b38c75377b037578d13bdba8208265cc6`, Genet
`7422e90613f9017e5bb790e3acb48f61776b2eda`, Dramatis
`c1d620764315b4c750fcffea30b8c898b8bb73bc`, and the maintained Vello
triple `10f01d6d88e94eac087daf033b24895cf97b8e82`. The older renderer
bridge `4354955e` stays intentionally separate. [Full source closure](logs/current-tabard-field-fix/source-closure.json)
contains exactly one Mere source. Online refresh and locked offline metadata
passed; only the three immutable Mere/Knot/Woodshed pins changed in the lock.
Later Mere commits through `71528a7a3` changed only documentation/attributes;
the product source qualified here remains the explicit e953 pin.

The final Turnstone test executable started normally and passed eleven
appearance, nine Settings pane and twelve provider tests: 32 passed, zero
failed or ignored, with no artifact workaround. Across the coherent siblings,
[74 targeted CPU tests passed](logs/current-tabard-field-fix/final-targeted-cpu.json),
including the real shared Workshop retained field-height test. Mere's port and
Graphshell web boundary checker passed in 3.256 seconds. Portable provenance
passed using bundled Python 3.12; the initial macOS Python 3.9 invocation lacked
`tomllib` and is retained separately as a tool-runtime failure. These targeted
results do not renew the older full 777-test serial suite or standalone sibling
native gates. [Build/check results](logs/current-tabard-field-fix/checks.json) and
the individual test logs preserve the exact boundaries.

| Process lane | Main presentations | Workshop presentations | Original captures | Seconds |
| --- | ---: | ---: | ---: | ---: |
| Seed | 199 | 51 | 9 | 36.356 |
| Reopen | 48 | — | 2 | 15.873 |
| Same-ID edit | 126 | 49 | 6 | 29.172 |
| Final reopen | 48 | — | 2 | 15.339 |

[Native13 qualification](native/native-13/qualification.json), the per-window
receipts, reset snapshots and [original-image manifest](native/native-13/image-manifest.json)
bind these counts. The actual Radeon Pro Vega 56/Metal lane remained serialized;
all owned processes exited and the GPU slot was returned before documentation
work. All captured frames were nonblank, and the previews contained the exact
background, surface, secondary surface, primary, heading and text colors.
The saved initial and edited sheets each match their fixture's 255 UTF-8 bytes
(hashes `a66fd88f…` and `f96e7541…`); both separate-process restorations retained
`theme:copy-1` / `dark`. The same-ID parent pixels stay `#142238` after Save,
switch to `#342214` only after ordinary Apply, and restore that edit wide/narrow.

The Theme name fix `545732000` supplies the shared `tabard-text-input` class;
the seed rule targets that same class inside `seed-value`. [Original-pixel geometry](native/native-13/field-geometry/field-geometry.json)
measures the name borders at physical y364–431 and seed input borders at
y1038–1105, each 68 pixels / 34 CSS pixels at scale2. Both full bordered fields,
their actual values and the seed monospace styling were inspected in the
original frame and [name](native/native-13/field-geometry/name.png) /
[seed](native/native-13/field-geometry/seed-hex.png) lossless crops.
The included Rootstock fix `6a10ad673` uses painted geometry for accessibility
scroll bounds; its shared/browser qualification remains separate from this
application appearance gate and physical assistive-technology acceptance.

One external verifier failed during the same-ID gate because its fixed
`(660,330)-(2290,968)` crop excluded the visible Explore button after scroll.
The original [failed result](native/native-13/same-id/fixed-roi-failure.json) is
preserved. Independent original-image review and [exact pixel diagnosis](native/native-13/same-id/verifier-geometry-diagnostic.json)
found the actual wide preview at `(644,208)-(2306,864)`, 112 pixels above seed's
`(644,320)-(2306,976)`. The old crop had zero primary pixels; the actual preview
contains 8,119 primary pixels plus all five other required exact colors.
The [external verifier](native/native-13/check_lane.py) now derives each painted
preview extent from long exact authored-background horizontal runs and records
the per-image rectangle and all six counts. Role thresholds remain unchanged.
Corrected checks passed on the existing originals before the single planned
final reopen. No product source change or native retry was used to close this
checker geometry failure.

Native12's older binary `940d484d…` passed seed (198 main +51 Workshop
presentations, nine captures) and reopen (48 presentations, two captures).
It was held before either same-ID lane when the field fix changed the source:
297 presentations /11 inspected captures /zero blank /no reset. Its
[partial qualification record](native/native-12-partial/qualification.json) and
[source manifest](native-12-source-manifest.json) are historical; they do not
qualify the newly changed workshop.

All current scenarios use fresh private profiles and the existing custody
endpoint environment seam. The actual migrated production identity is
absent/pending because djinn has no broker at that isolated endpoint. This
acceptance makes no key, custody, vault or broker-operation claim. Raw profiles,
the library dump and vault data are excluded. Reader/browser/SC rollout,
Linux/Windows, physical AT, dirty parent-quit Save/Discard/Cancel and every focus
transition retain their existing separate acceptance boundaries.

## Historical CPU and build evidence: native01–11

The initial parallel library run returned 770 passed, seven failed, and nine
existing ignores. Inspector target geometry and viewport-row/Sky expectations
were repaired against their production authorities. The three network cases
passed individually; no network implementation was changed. The subsequent
full serial run passed 777 tests with zero failures and nine existing ignores
in 1366.20 seconds. That full run predates the Settings selector geometry repair.

Additional isolated checks passed: eight Settings tests, fifteen Inspector
tests, the viewport-row test, the Sky test (143.22 seconds), and three network
tests. Their original logs, the parallel failure log, serial result, metadata
checks and production build log are retained in [logs](logs).

After the atlas pin changed, the current library test executable compiled in
1 minute 24 seconds, then stalled before libtest at `_dyld_start`, with a 12 KiB
footprint and no binary images in the owned sample. Signing and verifying only
that generated test executable did not resolve startup: bounded 60-second and
180-second launches returned timeouts without executing tests. A disposable
system strip control also stalled 60 seconds. These are environment startup
limits, with zero current tests executed; they are neither assertion failures
nor additional passes. [Small diagnostic evidence](logs/turnstone-atlas-fixed-test-dyld-sample.txt),
[signing scope](logs/turnstone-atlas-fixed-test-signing.json),
[bounded results](logs/turnstone-atlas-fixed-signed-tests.json) and
[retry outcome](logs/turnstone-atlas-fixed-tests-retry-result.json) are retained.
No generated binary or general OS log is included. The accepted original
production binary remained unchanged and passed all nineteen native captures.
Execution subsequently recovered after removing a provenance attribute only
from the owned generated ad hoc signed/verified test artifact. The current
source passed 32 focused CPU checks: eleven appearance (3.31 seconds), nine
Settings pane (1.74 seconds) and twelve provider (0.77 seconds), zero failures
or ignores. [Appearance execution](logs/turnstone-atlas-fixed-appearance-tests-local-artifact.log),
[Settings pane execution](logs/turnstone-atlas-fixed-settings-pane-tests-local-artifact.log)
and [provider execution](logs/turnstone-atlas-fixed-settings-provider-tests-local-artifact.log)
are retained. This artifact-only workaround changes no source, production
binary or system setting. The original bounded pre-execution failures remain
historical environment evidence, and this 32-test current result remains
separate from the older full 777-test serial run.

The initial production binary SHA256 was
`4df475a09c66a9c84351b8567567ede3a8cb25c753583b8477d64e7f1078cd5b`.
[Its source manifest](native-01-source-manifest.json) records the source inputs
for that candidate, excluding mutable plans and this ledger. It is historical
evidence, not a claim about a later rebuilt binary.

## Initial native failure control

The first isolated LaunchServices process exited cleanly after 73.32 seconds.
Its scenario returned `RESULT fail`: the Edit themes physical click did not
open the workshop. The initial route used Taproot's generic estimated layout
rather than the Settings pane's retained painted geometry. The workshop
receipt wait exhausted 1800 scenario frames and subsequent selection/mode
assertions failed. This process recorded 142 successful main presentations,
seven captures and zero blank captures. GPU restart reports were unchanged.
The remaining three process lanes were not run.

All seven original images were inspected. They show real graph and Settings
pixels, but the alleged canonical-mode captures have identical frame digests
and retain the default selection. They cannot qualify appearance adoption.
The [failed receipt](native/native-01/seed/scenario.done), original PNGs,
adapter/process logs, bounded process sample and reset snapshots are retained
under [native/native-01/seed](native/native-01/seed).

The repair makes Settings resolve unique visible targets from the same
RetainedLayout fragments used by paint and physical hit testing. The shell
checks the owning surface and rejects covered or ambiguous targets, then uses
the ordinary physical input route. It does not mutate provider/live settings
or bypass the production Apply control.

All nine current Settings tests passed in 1.56 seconds, including the new
retained physical-control test at both native receipt sizes. The old generic
point was `(54, 77.5)` and the retained point was `(44.75396, 70)`; both points
hit Edit themes in the isolated pane CPU fixture. The CPU test therefore does
not reproduce the original native miss or establish its cause. The fresh
native run must confirm the complete host route. Six current receipt/editor
tests also passed after this repair.

## Retained-target retry and event-loop diagnosis

The rebuilt retained-target binary SHA256 was
`d3000572241d4fa4859d7a92f53499f18cd1d30ace8c74a994eb7ba209043d9d`.
Its focused checks passed nine Settings, twelve provider and six receipt/editor
tests. Native attempt 02 produced 147 successful main presentations and seven
nonblank main captures. All four mode assertions passed and the visible
Settings modes changed. The child workshop opened and installed its 195-node
accessibility tree, but failed its ten-second presentation deadline with zero
presentations and two redraw attempts. Authored identity/CSS assertions failed;
the remaining process lanes were held. No GPU restart was recorded.

A third, explicitly diagnostic process used the same frozen binary and the
existing `CAMBIUM_HOST_FRAME_TRACE=1` option. The child was visible, key, on the
active space, and belonged to an active, unhidden application. A one-second
sample of the owned native process showed the main thread inside the custom
Turnstone `wait-file` verb's `wait_frames`, sleeping on the event-loop thread.
That synchronous wait prevents the child from receiving its native redraws.
The sample did not show a `nextDrawable` or GPU wait on the main thread. This
process also ended with 147 main presentations, seven nonblank main captures,
a failed child presentation receipt and unchanged GPU restart reports.

The original evidence for these failed attempts is retained under
[native/native-02](native/native-02) and [native/native-03](native/native-03).
These are diagnosis records and do not qualify the authored-theme workflow.

The repair replaces the two workshop waits with shared Taproot's cooperative
`wait 1800` and includes requested/open workshop work in Turnstone's existing
busy report. Seven receipt/editor tests passed, including a real shared-driver
regression which returns to its caller during both the request and live-child
phases, holds subsequent Apply work, then resumes once after the child closes.
The separate child outcome assertion and native receipt checks remain in place.
The final focused delta passed 28 checks: seven appearance/editor/receipt,
nine Settings (1.55 seconds), and twelve provider (0.89 seconds). The 777-test
full serial result predates the retained-selector and cooperative-wait changes;
it is recorded separately from these checks. The repaired production binary
completed before native attempt 04; the preview visual gate below remains open.

## Cooperative-wait native retry and preview pixel failure

The repaired production build passed in 31.38 seconds. Its binary SHA256 is
`d6cc8432d696addbc15fc5871579fb4549eff36b75aab886e4f5c3e1d1a893cb`.
[The native-04 source manifest](native-04-source-manifest.json) freezes 320
build inputs, excluding plans and receipt files, with canonical input digest
`43d90ca17a8f0fdf0bcd5204a499ad3e70eb4a7c92580db243f825076d71bcc7`.

The fresh isolated LaunchServices seed process completed in 34.183 seconds.
Both scenario receipts returned `RESULT ok`. The main window recorded 198
successful presentations, seven captures and zero blank captures. The child
workshop recorded 51 successful presentations, 54 redraws, two captures and
zero blank captures. Both owned windows/processes exited; GPU restart reports
were unchanged. The cooperative wait therefore allows the child to make
native presentation progress.

All nine original PNGs were inspected. The main images show four distinct
canonical Settings modes, the selected authored theme, and wide/narrow controls.
The library contains the exact 255 UTF-8 stylesheet bytes from the fixture,
with SHA256 `a66fd88f7099783f29e428056cff1e613adbe9d6b6421c004c4d848b8ce2746c`.
The persisted application choice is `theme:copy-1` in `dark` mode. These facts
are extracted in [authored-facts.json](native/native-04/seed/authored-facts.json)
without including a private profile, vault or library dump.

The workshop header, theme editor and derived palette paint, but its application
stylesheet preview is empty at both sizes. The wide interior ROI
`(660,330)-(2290,968)` contains 1,039,940 pixels of only RGBA
`(243,244,246,255)`, without application document controls or text. The narrow
interior has the same one-color result. [Pixel counts](native/native-04/seed/pixel-counts.json)
record exact original-image hashes, whole-frame counts and both preview ROIs.
This is a real visual failure despite the nonblank whole-window receipts.
No subsequent process lane was launched after this finding. The successful
parent-app workflow is preserved, but it does not qualify the complete workshop
preview or the unexecuted reopen and same-ID workflows.

## Preview producer diagnostic

Diagnostic attempt 05 used the same frozen binary and unchanged scenarios in a
fresh isolated profile, adding only the existing `CAMBIUM_HOST_PERF_TRACE=1`.
It completed in 34.722 seconds with main 198 presentations/seven captures and
workshop 51 presentations/two captures, zero blank captures and unchanged GPU
restart reports. Its two preview interiors remain one-color backgrounds.
The host trace records two producer calls and two staged outputs on each of
three changed frames (native log lines 63, 79 and 97). The preview registration
and staging path therefore executes; this result does not yet establish where
the visible texture is lost. Public logs and exact ROI counts are retained in
[native/native-05/seed](native/native-05/seed). This run is diagnostic only.

Diagnostic attempt 06 added only the existing `MESQUITE_CAPTURE_PAINT=1` to
that trace configuration. The same binary and fixtures completed in 34.292
seconds, again with main 198 presentations/seven captures and workshop 51
presentations/two captures, zero blank captures and unchanged restart reports.
Its original PNGs have the same empty preview ROIs. The exact presented-frame
binary PaintList envelopes are preserved beside both workshop images in
[native/native-06/seed/workshop](native/native-06/seed/workshop), with hashes in
[preview-roi-counts.json](native/native-06/seed/preview-roi-counts.json).
All owned native processes exited; no later lane ran. This supplies evidence
for the shared preview paint/staging investigation without a source change.

Diagnostic attempt 07 moved the parent chrome rendering identity out of the
shared native child host's low identity range and exposed the existing
StylesheetSpecimen diagnostics in the receipt snapshot. Seven appearance
and eleven exact surface tests passed; an intentionally terminated broader
substring test invocation is not counted as a pass. The diagnostic production
build passed in 32.70 seconds with SHA256
`e63c45974e3673b590b22ba82213a1a273c3a2678e024d2d702b5b3b35eaccc9`.
[Its source manifest](native-07-source-manifest.json) freezes 320 inputs with
digest `1516cd4b753127a0545e4bf6c097f96b5bbe9980bba5fec2647abf7111930891`.

Only the external diagnostic workshop fixture adds
`assert snap preview-errors == none` after both captures, before Save.
This intentionally failing assertion reports the actual diagnostic value;
its fixture SHA256 is
`1165b3a9ebc37064eb844dd4ecd47db6e69d3513616d7342c9bd42dbee52b80f`.
The run completed in 34.457 seconds with 199 main presentations/seven captures
and 52 workshop presentations/two captures, all nonblank and no new restart.
The assertion reports `got ''`: the stylesheet specimen has no diagnostics.
Both receipts fail through this intentional assertion and its propagated
child failure. Both preview interiors remain exactly the same one-color
background, so the named chrome identity change does not repair that visual
failure. The original workshop images were inspected; their paired PaintLists,
exact ROI counts and diagnostic fixture are retained under
[native/native-07/seed](native/native-07/seed). All owned processes exited.

Diagnostic attempt 08 observes the actual StylesheetSpecimen SceneProducer's
rendered output through a temporary opt-in module. The original source Rc and
first-binding raster key are retained; one initial producer registration is
replaced with a delegating observer, and readback timing can alter output.
This is diagnostic instrumentation, not a qualified production change.
The [temporary source patch](native-08-diagnostic-source.patch) is preserved.
[Its source manifest](native-08-source-manifest.json) records 321 inputs with
digest `0bdff804d3dd7852bfba2b5c8324baea23799b7cb85ae6eb67f1988dee8d1b16`.
The diagnostic build passed in 39.42 seconds with SHA256
`e3833cb4665d4d754dcdbee1c83a651a1107e65ee93c81eb75db3e788f797c1f`.
The six shipped fixtures are unchanged and the forced native07 assertion is
removed.

This single process completed in 35.212 seconds; both receipts return
`RESULT ok`, with main 198 presentations/seven captures and workshop 51
presentations/two captures, all nonblank. No new restart was recorded and all
owned processes exited. Four exact output textures were copied within bounded
readbacks (502, 201, 149 and 170 ms from arm to mapping). Every source scene
contains 24 operations and 13 glyph runs with no diagnostics. Only after the
process exited were raw RGBA files converted to PNG and scanned for pixels.
The authored wide and narrow textures have 1,875 distinct RGBA values each,
are fully opaque, and contain the exact authored background/surface/primary
and text/header role colors. Their images visibly contain the Garden notebook
heading, Explore button, address field, content heading and paragraph.

The composed workshop's wide/narrow preview ROIs still contain only the
background color. The healthy actual source texture and empty composed preview
therefore establish loss after source rendering. Source metadata, raw RGBA,
converted PNGs, [pixel counts](native/native-08/seed/source-diagnostic/pixel-stats.json),
and original workshop PNG/PaintList pairs are retained under
[native/native-08/seed](native/native-08/seed). Broader native acceptance remains
held until that producer-to-paint/composition boundary is repaired.

Diagnostic attempt 09 preserves the original capture first, then uses the same
live core and existing registered stylesheet texture in a fresh simple image
scene and a fresh full captured packet. It performs no producer/source calls,
registration, restaging or cache invalidation. Two probe copies are armed after
both rasters and both mappings complete before raw file writes. Its temporary
[patch](native-09-diagnostic-source.patch) and [321-input manifest](native-09-source-manifest.json)
record input digest `0df8c7f1c99fd9f9b5ccd39cd19bd76f86e71a37b297d5af99148824ba9d70bf`.
The diagnostic build passed in 37.68 seconds, binary SHA256
`9c5ab5222fc27713f90bb0e57ca67133a4bfca88b06930c03f419a9f1957a778`.

The process completed in 34.535 seconds, both receipts `RESULT ok`, with main
198 presentations/seven captures and workshop 51 presentations/two captures,
all nonblank. Both copies completed (514/512 ms), no restart was recorded, and
all owned processes exited. Pixel conversion/scans occurred after process exit.
The sampled image key is `0xc000000074616263`; live texture registrations remain
2 before, after sampler, and after full replay. The full packet contains 434
operations, four fonts and zero shadow masks. The observed original callback
is the legacy capture path, so runtime PresentedFrame host/sequence are
unavailable; its metadata explicitly records the assumed twofold scale rather
than claiming a stamped presentation measurement.

The direct staged CSS sampler is entirely transparent: all 1,090,272 pixels
are RGBA `(0,0,0,0)`. The fresh full packet paints the surrounding workshop, but
its preview ROI contains the same one-color background as the original, with
zero differences across all 1,039,940 ROI pixels. The fresh full image was
visually inspected. Against native08's healthy fully opaque actual producer
output, this isolates the registered image sampling/staging boundary; a fresh
full-scene raster alone does not repair it. Raw probe data, converted images,
metadata and [pixel comparisons](native/native-09/seed/stage-diagnostic/pixel-stats.json)
are preserved in [native/native-09/seed](native/native-09/seed).

Diagnostic attempt 10 compares five outputs on the same live core: the
original registered stylesheet image before any new registration, a fresh
stage key, direct registration of the actual source texture under another new
key, the original key after those new registrations, and the actual source
texture. The original child capture is armed first, and the original stylesheet
key is never restaged or unregistered by the diagnostic. A wrapper delegates
the real SceneProducer with the same source Rc; it performs no copies during
producer rendering/staging. New image identities deliberately trigger the
renderer's normal cache invalidation. All five copies are armed after the
rasters and all mappings finish before file I/O.

The build passed in 43.31 seconds with SHA256
`26a74ff0a0226a2ff5447f973ea4410915162beae69ba50d82b292e97217bf58`.
[Its 321-input source manifest](native-10-source-manifest.json) has digest
`913fedec6505bad763bfb2f747139eb7007b91f875f2cfd4390e6387d26d78f0`;
[the temporary source patch](native-10-diagnostic-source.patch) is preserved.
The single process completed in 34.479 seconds, both receipts `RESULT ok`,
with main 198 presentations/seven captures and workshop 51 presentations/two
captures, zero blank captures and no new restart. All owned processes exited
before PNG conversion and pixel scans. Copy times are 526, 523, 518, 515 and
512 ms. Live registrations remain two after sampling the original, then rise
to three/four for the two deliberately introduced image identities.

The original sampler before new identities is entirely transparent across
1,090,272 pixels. The fresh stage, direct registration, original sampler after
those registrations, and actual source are all byte-identical fully opaque
populated images with 1,875 colors, raw SHA256
`5cfb73d0a2ac38f45b9cdab7aa680ae8f03ccb334d8efec32b4fc02b9fe566c7`.
This is strong evidence of a renderer cache refresh defect: introducing a new
image identity makes the existing original identity sample correctly without
restaging it. The precise shared source repair remains under investigation.
The original wide/narrow workshop preview captures remain empty, so this
diagnostic does not qualify the production preview workflow. All nine original
PNGs and the actual-source PNG were visually inspected. Original callback
metadata is legacy, with no stamped host/sequence claim.

Original captures/PaintLists, five raw copies and converted images, metadata,
and [exact pixel comparisons](native/native-10/seed/registration-diagnostic/pixel-stats.json)
are retained in [native/native-10/seed](native/native-10/seed).

## Shared renderer repair and production confirmation

The shared investigation found that a patchless solid/empty frame returned
zero atlas dimensions while the encoding resolver retained resident image
entries. The renderer consequently replaced the persistent atlas, and the
next image reuse sampled an empty atlas without reuploading the clean resident
entry. The published [Vello repair](https://github.com/mark-ik/vello/commit/10f01d6d88e94eac087daf033b24895cf97b8e82)
preserves the resident atlas extent across those frames.

The [minimal GPU reproduction](shared-renderer-repair/minimal-gpu-repro.rs)
and independent [CPU negative control](shared-renderer-repair/patchless-atlas-negative.log)
are retained. The CPU regression fails before the repair because atlas extent
is `(0,0)` instead of `(2048,2048)`. All 33 encoding tests pass after the repair.
The published [13-frame regression](shared-renderer-repair/image_atlas.rs)
checks every pixel of the same retained image between solid and empty frames;
it [passes on AMD Radeon Pro Vega 56 / Metal](shared-renderer-repair/patchless-atlas-gpu.log)
in 0.81 seconds. The existing six-frame repeated preview regression also
[passes](shared-renderer-repair/patchless-repeat-regression.log) in 1.20 seconds.
The subsequent native11 production run below confirms this repair in the actual Turnstone workshop.

## Native11 production acceptance

The fixed production build passed in 1 minute 50 seconds. Its binary SHA256 is
`a402a258d751b2483a5161ce1ddd60911b48c1fe4b0ea085a5579b3854961426`.
[The final source manifest](native-11-source-manifest.json) freezes all 320
Cargo/source/scenario build inputs, excluding mutable plans and this ledger;
its canonical input digest is
`32f7ea1a01dbc180c3a62a36e72196e0d2fd5df0866a7731c6c9dd59cfa2789c`.
All inputs remained byte-identical across the four processes. Six external
scenario copies match the shipped fixtures. These runs use the existing
LaunchServices adapter, Mesquite/Taproot control paths and actual Settings
Edit themes and Apply controls; they contain no source observer or direct
product-state mutation.

| Process | Main presentations / captures | Workshop presentations / captures | Elapsed |
| --- | --- | --- | --- |
| [Seed](native/native-11/seed) | 199 / 7 | 51 / 2 | 36.518 s |
| [Reopen](native/native-11/reopen) | 48 / 2 | — | 15.048 s |
| [Same-ID Save then Apply](native/native-11/same-id) | 126 / 4 | 49 / 2 | 26.521 s |
| [Same-ID reopen](native/native-11/same-id-reopen) | 48 / 2 | — | 15.175 s |

Every main/child receipt returns `RESULT ok`; all nineteen captures are
nonblank. The child header, editor controls, graph and application document
preview are visible wide and narrow. Unlike attempts 04–10, the initial wide
preview ROI contains 1,875 RGBA colors: the Garden notebook, Explore button,
address field, heading and paragraph paint in the authored palette. Exact role
colors include background `#142238`, surface `#1B2E49`, second surface
`#243D5E`, primary `#2F7FFF`, heading `#9CD5FF` and body text `#EAF2FF`.
[Original ROI counts](native/native-11/seed/preview-roi-counts.json) preserve
this positive gate beside the earlier one-color negative captures.

The initial saved sheet equals all 255 fixture UTF-8 bytes, SHA256
`a66fd88f7099783f29e428056cff1e613adbe9d6b6421c004c4d848b8ce2746c`.
Save refreshes the catalog while Turnstone default stays selected; the actual
Apply control then selects `theme:copy-1` / `dark`. Four canonical mode
captures and a narrow authored capture pass. The first separate process
restores that exact choice and `#142238` pixels at both sizes.

The same-ID workshop edit changes only the background role to `#342214`.
Stored CSS again equals all 255 edited fixture bytes, SHA256
`f96e7541ae4bdce4bf4088efa1071e88a02456b6be68b83a52b6cce59c2a07a2`;
the identity and name remain `theme:copy-1` and `TurnstoneAcceptance`.
Main snapshots and exact background pixels remain `#142238` after Save before
Apply, then become `#342214` only after the ordinary persisted Apply action.
The final separate process restores the same identity, Dark mode and edited
appearance wide and narrow. Only definition/choice facts and hashes are
extracted; private profiles, vault files and raw library dumps are excluded.

The compiled candidate uses Mere `7019f07d36a9da8c1a3edbbcbabad606e9cb3278`
and Genet `7422e90613f9017e5bb790e3acb48f61776b2eda`, Knot `95849aea`,
Woodshed/Redshank `4aa68bfb`, and explicit root patches for all three maintained
Vello packages at `10f01d6d88e94eac087daf033b24895cf97b8e82`. The known
legacy Vello bridge remains separate. Later shared root-policy publications
Mere `3808c5a22` and Genet `7971ac91a1d` do not change the compiled shared
crate content: the orchestrator verified empty Mere `-- crates` and Genet
`-- components` diffs against the held hashes. This ledger records the actual
compiled pins; it does not claim a repin or a different rebuilt binary.

[Machine-readable qualification](native/native-11/qualification.json) binds
source, binary and scenario hashes, all presentation counts, authored facts,
reset snapshots and platform boundaries. All owned processes exited after each
lane, and no new GPU reset occurred. The full 777-test serial CPU result predates
the retained-selector/cooperative-wait delta; its 28 focused checks and the
later seven appearance/eleven exact surface tests are reported separately.
This receipt qualifies the macOS application appearance workflow; Linux,
Windows, physical assistive technology, browser rendering and independent
Reader/SC rollout gates remain unqualified by this run. Dirty parent-quit
Save/Discard/Cancel paths and every focus transition were not separately
exercised natively. The shared host preserves those decisions in source; these
receipts specifically qualify the four shipped appearance workflows.

The bundled Python/PIL final pixel check also stalled before process startup.
A responsive independent system-Python stdlib PNG/zlib scanline decoder
confirmed both exact final background pixels and stored choice; its
[restoration facts](native/native-11/same-id-reopen/final-restoration-pixels.json)
are retained. The unused owned PIL process was terminated. Earlier completed
PIL scans establish the seed/same-ID ROI counts, and all nineteen original
images were separately inspected. This startup limitation did not stall a
native application or create a GPU reset.
