# Servo accessibility admission, 2026-10-07

This lane joins real upstream Servo trees to Turnstone's existing AccessKit
host, using Mere's `uxtree::graft` composition and ownership helpers. It does
not change the pixel transport or supplier synchronization policy.

## Checkpoint

Mere `f1d169c755e082b5119c2485762fd28f4226d8fb` publishes four additive native
producer methods: activation, FIFO tree polling, resynchronization and typed
action delivery. The Windows library gate passed 123 Inker tests and 6 Graft
tests, at BelowNormal priority with one Cargo job and the existing Mere target.
Legacy producers retain explicit unsupported controls and actions.

Turnstone's all-three-feature consumer compilation, focused accessibility
tests, all-targets check and complete library suite pass. Two native functional
runs also pass real Servo lifecycle assertions. The narrower Windows heading
and ancestry observation passes. Raw qualification inputs and indexed
artifacts accompany the consumer source and scenarios.
The ordered Knot and Redshank stages are published. This is not full browser
accessibility or a release receipt.

Knot's ordered stage is published at
`14cd06e126c10df7f5506126b98af2f07ec87eb5`. Its all-targets check and metadata
pass. The original workspace run retains 589 passes, three ignored tests and
one fixed-sleep recovery-test failure; the source-identical same-binary
control passes. The test now uses its existing completion barrier, and all
three repaired recovery checks pass on the original unified workspace graph.
The production prefix is byte-identical. Standalone default and engine suites
pass 47 and 60 tests respectively, each with one ignored. The failed run and
the superseded package-only feature rebuild remain in Knot's committed
receipt. Turnstone has now compiled this graph with all three browser features.

Redshank's ordered stage is now published and integrated at Woodshed
`317968ba55092ee8f372b4aa2eb515da35833cc3`: 201 consumer tests pass with seven
existing ignores, three separate IPC tests pass, and desktop all-targets and
wasm checks pass. Its source and registry package records are unchanged;
only Mere identities move. Eleven concurrent transcript files were preserved
byte-for-byte, and the collision worktree and branch were removed.

Turnstone resolves this immutable family without registry changes: all 1,694
lock records retain their normalized package identities. Beyond the three
repository repins, the only lock edges added are Inker's AccessKit dependency
and Turnstone's three platform-consumer dev dependencies. The initial offline
lookup failure for the newly published Knot revision is retained; normal
network resolution succeeds. The all-three-feature foreign-tree suite passes
18 tests, including activation at every replay boundary against the actual
Windows, Unix and macOS consumer versions. The initial full library run passes
703 tests, fails one unrelated connected-writer test, and ignores nine. All
five new Servo accessibility provider tests pass in that run.

An independent audit confirms one selected Git family each for Mere, Genet,
Knot, Redshank, Scry, Weld and Graft, and one wgpu 30.0.1. The existing registry
`grafting` 0.6.0 used by Scry coexists with Git `grafting` 0.6.0 used by Graft;
that unchanged helper pair is not represented as a globally unique source.
Portable lock verification passes with unchanged source inputs after the
test readiness repair. Its earlier overlapping metadata command also exits
zero, but its source guard detects that test edit; both receipts are retained.

The connected-writer failure froze its baseline before the membership lane's
initial reconciliation completed: zero rounds became one while all expected
membership, delegation and graph updates arrived within 3.3 seconds. The same
original linked executable passes that exact test in isolation (13.58 seconds).
The failed aggregate and isolated control are retained. A test-only initial
readiness repair is being qualified: all ten lanes must complete their first
sync, and both initial certificates must be present before counters freeze.
Every byte outside that predicate is unchanged, including runtime code,
live-delivery assertions and deadlines; the raw original is archived.

The repaired default-concurrency aggregate passes 702 tests and times out in
two local worker reply checks (nine ignored). Both cases pass in the same
linked binary's serial isolated control. The complete same-binary run with
four test threads passes **704 tests, zero failures, nine ignored**; deadlines
and assertions are unchanged. `repaired-test-executable.json` binds that
binary. The locked all-three-feature all-targets check and native build pass
with unchanged input guards.

The native `a11y` and `a11y-admission` functional scenarios both exit zero:
two actual wrapper/document forests, DOM-only input updates, independent
navigation, retirement and fresh reopen. Their enclosing Windows observer
commands fail and remain distinct from that functional success. The first
observer loses exact timestamp ticks by reparsing a typed JSON date; its raw
helper and mechanical failure proof are retained. The second starts with the
safe host-only activation snapshot and spends 32 seconds traversing it; pages
navigate and close before it can retry, leaving stale provider elements.

The supplier-to-platform seam has an additional source-qualified limit:
the public fixture's banner text is `TextRun` beneath `GenericContainer`.
Windows consumer 0.35 filters both roles, and its text-range support requires
Label, Document, Terminal or text-input roles. Servo emits RootWebArea for the
body and generic containers for these controls, so ordinary body text and
form controls are not exposed by that combination. Consumer 0.38 retains
the same text-range predicate. Heading labels are explicitly exported by
Servo and can support a narrower real-platform subtree proof. No host role
rewrite or invented body-text capability is applied. The stable UIA probe
keeps the frame pump running while querying those original heading labels;
only after successful observation does a one-frame `wait-file` check its ACK.
Using `wait-file` to hold the UI thread before activation would block replay.

The fresh `native-a11y-uia-servo` stable probe passes its scenario and enclosing
observer command. Actual Windows headings have local node IDs 19 and 56 in
different nonzero consumer tree indices 2 and 4; each parent chain reaches
the verified owned window. UUID association is through the unique original
heading/local-ID witness in the exact composed checkpoint, not direct UUID
export by UIA. The observer completes in 0.73 seconds after the checkpoint.
Both actual Document ancestors report no TextPattern, and the two original
TOP-name probes find no match. This qualifies original supplier headings and
subtree ancestry only. General body text and form-control semantics remain
open; the raw supplier text also contains this fixture's script source.

The same executable's fresh `native-a11y-upstream-servo` negative control
retains `RESULT fail`, rejected absent/unreachable focus and missing-child
updates, one forest at the two-page checkpoint, and subsequent withdrawn
forests. Its assertions accumulate failures until scenario completion;
intermediate captures are not successes. The enclosing command exits one
with unchanged source inputs. The configured document-root policy is thus
qualified explicitly, rather than representing unmodified upstream focus.

Remote Turnstone advanced through three documentation-only commits to
`acc1793e586076524b56f170d952bd0ef0f0cb9a`. Their unusual-protocols rulings were
fast-forwarded without changing any compiled source, Cargo input, fixture or
scenario used by the existing gates.

## Implemented scope

- Preserve supplier TreeIds and tree-local NodeIds, including nested grafts.
- Validate each retained local graph and the foreign forest before publication.
  Missing declared children wait for their initial update; malformed known
  trees withdraw the surface. Unreferenced late callbacks cannot damage a
  current forest.
- Place each producer once in the host's tree, at its current physical pane
  bounds. A repeated visual appearance uses its focused placement, otherwise
  its first visible placement. It cannot give one foreign tree two parents.
- Publish semantic deltas on their UI-thread wake before acquiring pixels.
  Reactivating the platform adapter replays descendants before entering focus.
- Retire identities on navigation, close and replacement; navigation can also
  recover an incomplete or withdrawn tree. Validate foreign actions against
  current ownership, visibility, node presence and advertised action masks.
- Keep the exact successful composed foreign frames for bounded fixture
  observations. Persisted page labels and values require the explicit
  `TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT=1` flag; password nodes and their local
  descendants remain redacted.

## Qualification entry points

`cargo test --locked --lib foreign_a11y -j 1` exercises the host cache,
composition, diagnostics and fixture opt-in. Its recursive tests use the real
Windows 0.35, Unix 0.36 and macOS 0.38 consumer versions. The Servo delegate
FIFO/deactivation tests additionally require the `servo` feature.

Serve `scenarios/fixtures/browser_servo` at `http://127.0.0.1:43124`, then run
`run-native.ps1` against the newly qualified executable. The wrapper verifies
the served public fixture bytes, records raw source and executable hashes,
and invokes the existing native runner with a fresh process/application
profile. The qualified platform command uses `-PhasePrefix a11y-uia -Scenario
scenarios/browser_servo_a11y_uia_windows.scn`. `observe-owned-uia.ps1` binds
to that exact process's PID and creation time, activates Windows UI Automation,
and independently reads both original page headings and their actual parent
chains. It records the unsupported body-text observations separately.

`*.foreign-a11y.json` records in-process composition, even if the OS adapter
was inactive. `uia-tree.json` separately records actual Windows provider
traversal. Neither establishes a human screen-reader walk or another platform.
Existing outputs are preserved on failure; rerunning a completed generation
requires a separately admitted receipt/profile rather than overwriting it.
`run-native.ps1 -FocusPolicy upstream -PhasePrefix a11y-upstream` uses a fresh
named process/application profile for the raw-focus negative control.

## Supplier and release limits

Servo is the upstream 0.7 source `aac43a3f`. Its preferences disable
accessibility by default; this host explicitly enables it. Its public action
forwarder implements Click in the DOM handler, but the exported layout nodes
advertise no actions. Turnstone therefore refuses assistive actions rather
than inventing support. Focus, editing, selection and scrolling need supplier
implementation and native acceptance.

AAC activation targets the active top-level pipeline. The native fixture
qualifies two views' actual wrapper/document forests, not iframe export.
Deeper foreign nesting is a separate consumer-level test, not a claim about
Servo's supplier output.

The admitted supplier allocates process-wide node IDs but emits a hardcoded
focus ID of 1; later documents may not contain that node. The explicit
Servo-only `TURNSTONE_SERVO_A11Y_FOCUS=document-root` policy projects neutral
document-root focus, as AccessKit requires when no specific local focus is
represented. It preserves node IDs, graph data and action masks; debug
provenance records the raw supplier focus and projected focus without page
text. This is the default for this pinned supplier. `upstream` preserves the
raw focus for the negative control. Unknown policy values refuse Servo
initialization before upstream globals are touched.
This projection does not implement DOM focused-node reporting or actions and
must be reconsidered when repinning to genuine supplier focus support.

Upstream
reactivation resets its document epoch and can re-graft an already queued
document callback under the fresh wrapper. The host waits for a complete,
validated initial tree and rejects invalid retained updates. Its public
callback cannot identify the activation generation of an old full update.
The ordering race is a source finding awaiting the native run, not an observed
failure or a claim of a supplier fix. Successful composed snapshots describe
the declared focus projection, not unmodified upstream focus.

Windows, macOS and Linux human AT acceptance remains open. Servo body text,
hidden/script text, form roles, DOM focus and advertised assistive actions
need supplier/platform work. Scry/Weld semantic export and action delivery,
Servo resize/GPU ordering, and cross-process handle/fence transport remain
separate owner gates. Accessibility capability metadata is not promoted by
adding this transport seam.

## Retained resources and cleanup

This lane reuses the ordinary Mere, Knot, Woodshed and Turnstone Cargo targets.
It creates no isolated Cargo home. The Redshank collision worktree and branch
are removed after publication/integration, preserving the primary checkout's
eleven transcript files. The owned public fixture launcher and its verified
Python/console children are stopped; the cleanup receipt records their exact
creation times. No native app or observer from this lane remains running.

The four fresh application profiles under the older receipt's ignored
`profile/a11y*-servo-app` paths and Servo profiles under the stable Turnstone
target's `runtime/servo-a11y*` paths remain owned by this receipt. They retain
failed and successful native generations for review and are not source or
isolated build trees. Their source-bound captures and logs are indexed here.

An earlier Scry release worktree at
`C:/Users/mark_/Code/worktrees/wgpu-scry-release` remains with its release owner:
automatic approval review blocked its cleanup under policy in that earlier
lane. This lane does not retry or bypass that rejection. Earlier Weld native
profiles and Scry runtime caches remain with their prior evidence owners.
