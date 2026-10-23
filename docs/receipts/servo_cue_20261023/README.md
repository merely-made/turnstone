# Concrete Servo and Genet Cue receiving — 2026-10-23

The finite macOS/Metal receiver passes offscreen with real Genet and Servo
producers. Both use `PeltSharedContent`, Webcue controls and the WgpuCue
presenter on one producer device. Genet renders onto the host device; Servo
hands over a retained native texture normalized by the existing Graft
pre-present hook. The visible-window run is pending the user's unlock signal.

This is a reusable native binding and receiving example. Full Turnstone UI
adoption, the product inspector and per-card/site policy UI are outside this
receipt. It does not qualify Linux/Vulkan, Windows runtime behavior, physical
keyboard/IME input, human assistive technology, transparent native content,
compositor capture or physical GPU reset.

## Source and reproduction

[`source.json`](source.json) records the source commit and SHA-256 of every
changed implementation/build file. [`type-identity.json`](type-identity.json)
records the final locked macOS graph: one canonical Webcue, WgpuCue,
`grafting-frame`, document-session API and Wgpu source. The published core is
0.1.1. Mere supplier `737fec2df` includes the hidden-input correction and
current main's scope/reading controls; Turnstone integrates `e5f2e97`.

Use Rust 1.99.1 and the committed lockfile:

```sh
cargo build --features servo --example cue_surfaces --locked
scripts/receive-cue-surfaces-mac.sh /absolute/path/to/cue_surfaces /fresh/absolute/run-root offscreen
scripts/receive-cue-surfaces-mac.sh /absolute/path/to/cue_surfaces /another/fresh/run-root headed
```

The driver launches an isolated `.app` in the logged-in GUI session with a
synthetic file fixture and separate Servo profile. It records the binary hash,
application output and acceptance JSON, waits for process exit, and only cleans
up its own executable. A headed pass additionally requires actual window
presentations. A live process, LaunchServices success or source pixels alone
does not pass that gate. `.app` binaries and profiles remain in the local run
roots; only synthetic fixtures, reports, pixels, hashes and logs are copied here.

## Executed receiving evidence

[`offscreen2/qualification.json`](offscreen2/qualification.json) passes all
phases with `window_presentations: 0`, explicitly classified as
`offscreen-native-receiving`:

- Both real producers yield the red page's expected RGBA pixels. Each complete
  capability report includes all 28 feature statuses and refusal/partial reasons.
- Synthetic pointer focus and text input edit the actual fields. Genet confirms
  the value through retained semantics; Servo reports the DOM input handler's
  changed title. Both return one correlated typed zoom result.
- Hidden input is refused; reload observations and completions progress through
  shared `advance` without any frame acquisition while hidden.
- Both resize to 300 × 200 and accept shared navigation, with ordered events and
  exactly one completion per accepted request. Genet's navigation result is a
  host acquisition request, as its report declares.
- Retirement settles pending requests once, refuses late input, removes Servo
  views, and replacement installs new scopes and blue-page pixels.
- A retained Servo allocation remains importable and readable after its producer
  is destroyed. A separate diagnostic device is refused before import. That
  device is a negative control, not a second producer device.
- Both final producers are dropped and the original process root shuts down
  with zero live Servo views.

The native envelope retains the actual Metal allocation while holding the Wgpu
HAL destruction guard. It adds no second normalization copy or CPU wait. The
paired importer requires the exact producing device and queue before relying
on the frame's no-fence queue ordering. Readbacks in this fixture are scheduled
and polled on later event-loop turns.

Genet's report advances to `Observed` through its existing renderer callback.
Servo's report remains `Constructed`; this receipt separately supplies the
observed native receiving evidence. The shared browser adapter currently has
no host-import feedback seam to promote its report. Raw handle acquisition
alone must not invent that feedback.

## Regression evidence and retained failures

The Mere focused Inker/Pelt/Graft cohort passes **254 tests**, with two GPU
tests ignored in that ordinary run and one ignored doctest. Both GPU tests
then pass explicitly on Metal. The new hidden-input regression verifies
unchanged real Genet editor contents, no result emitted for refused input,
hidden reload progress, resumed input and retirement precedence. Logs are in
[`tests/`](tests).

The first Turnstone suite, before integrating the concurrent scope/reading
work, ran 1,183 tests: **1,162 passed, five failed, 16 ignored**. Preserve that
failure, rather than substituting isolated reruns for its result. Three failures
were network/grant timeout cases; all three pass individually on the same
binary. Two were stale frame-header fixtures requesting a tab while the default
Map/Read switch hides that header. Their fixtures now explicitly show the tab;
all three frame-key tests pass.

The subsequent integrated serial run exposed three more stale fixtures from
the concurrent scope/reading changes: two expected retired Workbench/Orrery
labels, and the third globally replaced text in a modern saved layout, thereby
injecting a retired pane name into a retained display title. The fixtures now
expect Map/Read and supply an actual legacy tree. Product labels and title
preservation are unchanged. All **71 affected accessibility, pane/storage,
frame-routing and Servo host tests pass** after these test-only corrections.
The complete integrated serial run before these three fixture corrections
finished with **1,176 passed, three failed and 16 ignored** in 1,541.60 seconds.
All three earlier network failures pass in that integrated serial run. This
remains a failed broad run; the passing 71-test followup qualifies the corrected
fixtures and affected host boundaries separately.

Earlier native runs remain alongside the successful one:

- `attempt1`: window acquisition reported occlusion before transient handling
  was corrected; it did not qualify receiving.
- `attempt2`: real source pixels arrived, but the locked session never admitted
  window presentation; phase zero timed out.
- `attempt3`: explicit activation still could not present in the locked session.
  Its driver also became invalid after an in-flight script edit introduced an
  unset mode variable. Neither application timeout nor driver error is a pass.
- `offscreen1`: actual input/zoom receiving passed, then the strict hidden-input
  check exposed Genet still admitting input. Mere `737fec2df` corrects that
  boundary for both shared input and synchronous retained dispatch.

The locked final example build succeeds. Clippy completes for the library and
example with warnings, including existing repository warnings, five collapsible
`if` suggestions in the finite fixture, and its deliberately retained host
handles field; this is not a `-D warnings` gate. The complete output is retained.

[`primary-preservation.json`](primary-preservation.json) records all four clean
primary checkouts. Mere and Turnstone advanced through concurrent work outside
this receiving worktree; Cue and Graft remain at their initial commits. This
pass made source changes only in its isolated Mere and Turnstone worktrees.
The compiled receiver and isolated worktrees remain available for the pending
headed run.
