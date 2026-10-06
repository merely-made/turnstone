# Windows Scry consumer qualification, 2026-10-05

Local B1 source over Turnstone `74a46893d25e73e57787d31efd9d6850e4ad714f`.
Source implementation and bounded native checks are complete. **Full B1
acceptance is gated on native cookie retention and current captured pixels.**
Both the full script and separate restart completed normally without external
repainting, and both returned `RESULT fail`. The failed assertions are retained.

## Current result

`source-manifest.json` binds 152 source/fixture inputs to executable SHA256
`a245130a4becccd35c81c78a5108a2a74d0e980fa52744937711f4e678421948`.
`native-clipped/` passes all full-script assertions except four cookie checks:
in-place B reload, A reconstruction, and reopening each closed page. Observed
DOM outcomes cover independent alpha/bravo text, pointer activation plus CDP
Enter (`clicks=2`), owner-correct wheel delivery and reset, sibling preservation,
capability refusal, pane resizing and engine switching. Closing both pages
records zero live producers, cached frames and importers. Reopening restores
their localStorage markers and text.

`native-restart/` uses that same executable, synthetic profile root and origin
in a separate process, without seeding URLs/storage or toggling content on.
Before tab input it records two restored Workbench cells, two live producers
and distinct fences. Saved engine pins, alpha/bravo text and localStorage
markers pass; its two cookie assertions fail. This qualifies those restored
facts, not full profile persistence or current presentation. The script's
final descriptive log line is unconditional; `RESULT` and failed assertions
govern acceptance.

Manual pixel review finds failures that the DOM/counter assertions do not
detect. In `02_text_pointer_and_keyboard.png`, B's tab title reports `clicks=2`
but its captured body still shows `clicks=1`. In `02e_both_scroll_reset.png`, A's
current DOM assertions pass `scroll=0,0`, `scrolled=no`, `wheels=2`, while its
captured body still shows `scroll=0,285`, `scrolled=yes`, `wheels=1`. The wheel
receipt has 435 and 502 frames/imports/waits on distinct fences; those counts
do not prove current pixels. Both reopened tiles are blank in
`08_ready_for_restart.png`; the first restart capture also has a blank B tile,
although both become visible in the later B Inspector capture. Initial
construction and later visibility do not close continued presentation.
`visual-review.json` records these observations. The capture, import, host
composition and event-delivery boundary needs correlated image evidence before
assigning a backend root cause.

Supplier review identifies a concrete Scry 0.7.1 freshness defect: its capture
notification consumer marks all arrivals seen, but dequeues one sample from a
two-slot pool. If no further frame arrives, a newer queued sample can remain
unread. A supplier-local fix must drain the bounded pool and retain its newest
sample while closing superseded frames. This finding is consistent with stale
pixels, but does not prove the cause of all blank captures. This receipt still
consumes published 0.7.1; a supplier source fix or pure queue test cannot change
its native acceptance result.
The bounded supplier fix is now implemented locally. All five focused Windows
capture tests pass, including the deliberately broken single-dequeue control;
`supplier-scry-freshness.json` records its source hash, executed command and
claim limits. Scry commit `39818a7` contains that source, the Windows plan and
only its scoped canonical-index hunks. The baseline Scry audit/preflight WIP
is preserved outside the commit because initial hunk ownership is uncertain.
The code awaits repaired-source native qualification. Stable target
`C:/t/cargo-targets/wgpu-scry` is retained for Scry repository reuse.

Latest locked all-target checks pass with default features and with `scry,weld`
(`default-check-final.log`, `combined-check-complete.log`). The current clipped
source passes all 18 Workbench tests, including independent graph selection
and long-title hit attribution (`workbench-tests-clipped.log`). Earlier targeted
Shell and Inspector runs pass 51 and 15 tests respectively; they predate only
the final tab-cell clipping regression. The historical full-suite failure
below has no new full-suite success receipt. The combined consumer still uses
Git `welding` 0.15.0 at `65d057d`; it is not an all-registry triplet proof.

## Source boundary

`--features scry` adds `scrying-engine` at the existing Mere `bd5912fb` pin.
It consumes registry `scrying` 0.7.1, `grafting` 0.6.0 and wgpu 30.0.1. The
20 added lock identities are recorded in `lock-delta.json`; no prior package
identity was removed. Existing repository pins are unchanged.

The factory constructs on the UI thread, shares an owned offscreen composition
root, and captures each page visual on the primary host's device/queue. Each
node keeps its explicit profile under `scry/webview2-profiles/<node>`. Each
producer has a dedicated fence because Scry allocates fence values locally.
The owned importer checks metadata and the exact fence, waits on every paint,
and clears unsafe caches on failure. Replacement clears old producer epochs.

The host corrects the pinned adapter's mouse translation: mouse events use
WebView2's mouse API, retaining buttons and Shift/Control; touch/pen keep their
pointer path. WebView2 mouse modifiers have no Alt/Meta representation. CDP
key/text dispatch is distinct from physical keyboard and OS IME acceptance.
Capture/resize/import errors retire the producer and report failed content.

## Procedure and evidence

Serve `scenarios/fixtures/browser_scry/` at `http://127.0.0.1:43123`.
Set `SCRY_FIXTURE_BASE` to that origin, `TURNSTONE_SCENARIO` to the absolute
scenario path, and `TURNSTONE_CAPTURE_DIR` to this receipt's run directory.
The geometry smoke uses `smoke.scn` and its own fresh `profile/smoke/` root.
The full script is `scenarios/browser_scry_windows.scn`; set its eight click
coordinate variables from the actual two-cell maximized geometry. All eight
variables are needed before parsing the full script.

Run the restart script `scenarios/fixtures/browser_scry/restart_verify.scn`
in a separate process using the exact same full-run `TURNSTONE_ROOT` and
loopback origin. Do not reseed storage or toggle content on to mask a failed
restart. `record-idle` writes current producer/cache counts and per-node
frames/imports/waits plus fence handles. Compare handles only while both
producers are live; operating systems may reuse them after close.

Current Scry WebView2 capture allocates a new texture per paint, so native
expectations are `frames == imports == waits`, with nonzero counts on both
pages. Reused-allocation waits receive a separate mocked regression; the
current native producer does not provide that shape.

`first-check.log` records successful initial all-target Scry compilation,
before the mouse, keyboard-host-shortcut and lazy-capture failure repairs.
`native-build.log` records the subsequent locked executable build.

`library-tests-scry.log` preserves the initial feature run: 644 passed,
2 failed and 9 ignored. The two Inspector fixtures assumed Scry was always
unavailable; they now distinguish enabled construction from explicit refusal.
`inspector-tests-scry.log` records all 14 corrected Inspector tests passing.
`combined-check.log` records the locked Scry/Weld all-target check passing.
`portable-verify.log` verifies the portable manifest and unchanged lock hash.

The initial smoke's `RESULT ok` was rejected on review: only one producer
existed and Workbench omitted its native surface. The Workbench admission fix
is followed by `native-workbench/`, whose stronger guards correctly fail B's
picker selection and the missing maximize target. `native-inspector/` records
the visible picker. `native-inspector-pointer/` selects B through a measured
physical point and proves two distinct fences with positive counters, but
its blank/stale pixels are not a completed two-page acceptance receipt.

The scoped Inspector probe now resolves the retained shaped layout instead
of estimating row centres independently. It still uses normal pointer
delivery and refuses ambiguous, clipped or covered surface targets. The
scenario reactivates Workbench after closing Inspector before maximizing.
The latest native, input, teardown and restart results are recorded above;
these earlier diagnostics remain historical.

`native/aborted.json` preserves the new-selector run's input stall. Its
initial two-page capture and count guards passed; the input-stage capture
never arrived. The published keyboard helper pumps Windows messages while
blocking for CDP completion inside a host event handler. That is a concrete
reentry seam consistent with the stall, not a captured native stack diagnosis.
The consumer repair uses an ordered callback queue with one request in flight
and dispatches from ordinary host polling. Direct `webview2-com` 0.39.1 is
the already-locked native binding; repository pins and package identities stay
unchanged. The callback run did not resolve progression: `native-nonblocking/aborted.json`
records another missing input checkpoint after more than six minutes beyond
initial two-page paint. `native-input-trace/` uses the same executable with
file checkpoints; opening A completes and `trace-before-input-click.txt`
arrives, while `trace-after-input-click.txt` does not. This narrows the stalled
phase to content press or the following frame, without yet identifying a
native call. `native-input-call-trace/` brackets focus and mouse delivery with
debug diagnostics on the next executable. They show native focus returning
in about three milliseconds and mouse down/up returning successfully in less
than one millisecond each. Two further paints and the after-click marker
complete, but the marker before typing does not. Neither a blocked focus call
nor a blocked mouse call is established. The next diagnostic brackets render,
accessibility event/update delivery and scenario steps; it logs verbs without
text arguments. `source-manifest-callback.json` and
`source-manifest-input-call-trace.json` preserve prior source/executable pairs.

`inspector-narrow-test.log` is the deliberately broken positive control:
the estimated point was inside the narrow pane but produced no viewer intent.
This reproduces the native selection failure independently of WebView2.

`native-geometry/` is a separate old-executable coordinate probe over the
existing Inspector diagnostic profile. It passed, and visual review of
`distinct-two-pages.png` found both current page summaries. The two nodes
recorded 168 and 169 frames/imports/waits on distinct fences. Both rectangles
are 509 by 570 physical pixels, with CSS viewport 255 by 285 at DPR 2.
`coordinates.json` records measured input and button points, avoiding the
floating caption. This establishes geometry, not the repaired selector or
the subsequent input/restart gates.

`source-manifest-initial.json` belongs to the first executable and scenario;
it does not describe the later Workbench and picker repairs.

`shell-tests-final.log` records 48 passing Shell tests, including five ordered
keyboard queue regressions and the owned-frame custody, per-paint wait and
mouse translation checks. `inspector-tests-final.log` records all 15 Inspector
tests passing, including selection after reuse and clipping/scrolling. The
full feature suite's final run preserved one test-fixture failure (646 passed,
1 failed, 9 ignored); that fixture was corrected and covered by the 15-test
Inspector rerun. It is not a fresh full-suite success.

`combined-check-trace.log` passes all targets with both Scry and Weld enabled.
The earlier `combined-check-callback.log` failed while copying CEF runtime
files used by the live native process; the successful rerun followed closing
that owned process. `portable-verify-final.log` verifies 1275 packages and lock
SHA256 `fb490421b31b10246c17a0dc70d5760e503fdb9c92761f40b342d0af42bd60e5`.

`native-loop-trace/` brackets the event loop. It completes native focus,
mouse down/up, the following render and accessibility update, and the
`trace-after-input-click` step. Further redraw callbacks then stop. The UI
thread wait-chain probe reports no cycle; it does not establish a native
stack. A repaint to the uniquely named owned **Turnstone** window resumes the
remaining steps, ordered CDP `alpha`, and `RESULT ok`. This is an intervened
diagnostic, not acceptance. The earlier repaint targeted the private capture
`Static` HWND returned by `Process.MainWindowHandle`; `owned-windows.txt` and
`redraw-product-window.json` record the correction.

The consumer now arms a timer independently of paint while frame producers or
a self-drive scenario are live, and requests a redraw on its deadline wake.
`TURNSTONE_SURFACE_POLL_MS` configures 1 through 60000 ms, default 16 ms; invalid input
uses that default. Idle application scheduling remains a minute. The deadline survives early event batches; late wakes poll once rather than
replaying missed ticks. `surface-poll-standalone-tests.log` records all three
pure timer regressions passing, with the exact module compiled through stdin
while Cargo was queued. `surface-poll-reset-positive-control.log` deliberately
resets the deadline on every event and fails the early-wake regression.
`surface-poll-test-manifest.json` records source, toolchain, mutation and binary
hashes. A native engine wake callback would replace this polling fallback.
The current full and restart runs complete without external repainting, with
the acceptance failures recorded above.

## Ownership and tab geometry diagnostics

`native-wake/` completes the entire script without external repaint, but
returns `RESULT fail`. Its source/executable pair is preserved in
`source-manifest-wake-first.json`; `native-wake-artifact-manifest.json` hashes
the diagnostic output. Enter lacked its carriage-return character event;
the fixture observed one activation instead of two. URL opening used to
select a sibling also fetched it again, invalidating the scroll-preservation
checks. Cookies disappeared on in-place reload while local storage persisted.

`native-owner/` and `native-context/` verify the repaired CDP Enter behavior:
both current page summaries show alpha/bravo and `clicks=2`. Their full runs
still fail; neither is B1 acceptance. `source-manifest-owner-first.json` and
`source-manifest-context-first.json` preserve those source/executable pairs.
The scripted selection now uses normal Workbench tab presses instead of
reopening URLs. Browser command/current-page observations retain their own
member independently of the graph snapshot. Reload and live-content toggles
use that member. Tab presses claim pane focus, and graph repaint updates its
context without displacing an active Workbench publisher. The regression in
`browser-command-repaint-tests.log` opens Inspector after repainting an
independently selected graph sibling and still targets the Workbench page.

Workbench also needs exposure through the shared probe's surface inventory.
The first resolver-only runs missed that surface, with downstream identity
assertions correctly failing. `native-probe/` exposes it and detects the next
real defect: an unbounded long tab title can extend its retained hit box into
the sibling's cell, causing a tab click to stack the wrong page. This is a
failed geometry diagnostic, preserved by `source-manifest-probe-first.json`.
Each cell now clips its furniture; both target resolution and physical tab
attribution use the painted visible rectangles. `long-tab-title-tests.log`
verifies that both long-title targets stay inside their owning cell and
resolve to the correct member, and that multiple matching tabs are refused.
The fixture disables scroll anchoring so changing diagnostic text cannot
move the measured viewport.

Read-only inspection of the synthetic `acceptance-wake` profiles finds other
browser storage but no `Cookies` database (`native-wake-cookie-store.json`).
The expected path is 197 characters. Turnstone does not inject or clear a
native cookie snapshot, and the first failing reload keeps the same producer;
those facts do not establish a native cookie-store cause. No speculative
cookie-custody workaround has been applied.

## Open gates

The next bounded owner slices are native CookieManager/profile-store
correlation across an in-place reload (Scry plus host profile binding), and
correlation of current DOM generations with captured/imported/composited
pixels on both pages (Scry capture plus the host adapter/compositor). Preserve
the existing cookie and visual assertions; neither gate is closed by positive
frame counters, a title update, or a host cookie replay workaround. The
producer/adapter also needs frame/event/completion wake notification so other
consumers do not require the host's timer fallback.

Find, typed page zoom, correlated capture/PDF, permission/auth answers,
DevTools opening, and accessibility-tree projection remain unavailable through
the pinned adapter. The host reports explicit reasons. Physical keyboard,
OS IME/preedit/candidate placement, actual OS resize/DPI, native rehosting and
multiple different-size appearances of one node require separate acceptance.
This two-distinct-page primary-window consumer does not qualify lenses or
cross-engine shared profile/history policy.

Trio hardware reruns, exact-source packaging and registry-only release
consumer gates remain owned by the cross-repository release plan. No release
or workflow dispatch is implied by this receipt.

## Workspace

Recorded builds reuse `C:/t/cargo-targets/turnstone`; no isolated Cargo home or
worktree was created. A verification invocation inherited the machine's old
`C:/t/graphshell-target` setting. Its owned process tree was stopped, then
metadata verification was rerun with the approved target explicitly set.
That interrupted build is not a qualification receipt; existing shared output
is retained because its other owners are unknown. The loopback fixture server
was run in a managed terminal session and stopped after the native runs.
Automatic approval review rejected the initial hidden background-server
launch with `blocked by policy`; the managed session serves the same fixture
with a tool-controlled lifetime. Synthetic profile state stays under the
receipt's ignored `profile/` as retained evidence of the failed cookie and
presentation gates. Turnstone owns these synthetic profiles. No isolated
target, Cargo home or worktree is retained.
