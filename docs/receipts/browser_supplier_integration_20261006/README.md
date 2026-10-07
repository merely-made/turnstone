# Browser supplier integration, 2026-10-06

This documentation/receipt checkpoint preserves local candidate work; it does
not land its Cargo/Rust changes or the final U9 repin. Shared Weld input and
five scoped three-engine Windows controls are qualified. Servo's default
reopen still has a reproduced intermittent white-frame failure. Foreign
accessibility on all three platforms remains a release gate.

The [six-run synchronization comparison](sync-diagnostic/README.md) records
Existing pixels passing once and failing once, producer-only and
normalization-only passing once each, and Both passing twice on one executable.
All six scenario/exit guards pass; the independent pixel reviewer rejects the
white reopened A. Its 98 distinct browser tests pass. The source-qualified
ordering gap and observed association with waits do not prove visual causality.
Defaults stay unchanged. All experimental sources remain archived; the owned
temporary config is removed and all 175 prior candidate inputs restored exactly.

The restored Git candidate also compiles successfully in 1m05s at one job and
BelowNormal priority. Its [source review](sync-diagnostic/restored-candidate-source-review.json)
verifies the original 175 input hashes and 1,673 dependency records. Its fresh
binary is compile-only evidence, separate from both earlier native executables.

Four source-bearing compatibility worktrees remain owned by this browser lane:
`Code/worktrees/genet-browser-fonts` (`679d8314`),
`Code/worktrees/mere-browser-input-compat` (`edf175f9`),
`Code/worktrees/woodshed-browser-input-compat` (`24f196f4`), and
`Code/worktrees/knot-editor-browser-input-compat` (`211ff57a`). Their published
branches preserve the qualified older family while the current U9 set is held.
They are not integrated into current main. Existing stable targets under
`C:/t/cargo-targets/<repo>` are reused; no isolated Cargo home remains.

The first locked production build selects Mere `db4ee31258b23c86572c429388c5d10bd4de9dc3`,
Woodshed `b613fc55d2d59e865b6bb04e55a880ee4773b824`, Knot
`91cb44a2c00c088f0677956d31f3cdca6828c59e` and Graft
`01f3c9f3d68df3247e6b7ec7f65ffbca57e9b2d4`. The dependency compatibility
subdirectory records exact diffs and source equality. This preserves the
qualified Genet `69a2383b2ad777b884a72f31f8f8fb7ece275c0b`, one Mere/Inker family
and one wgpu 30.0.1 package. Existing registry package versions remain in the
lock; Servo adds parallel versions under existing package names, recorded in
the source-family comparison.

The source removes Turnstone's local mouse/CEF CHAR bridge. Mere owns native
mouse routing with held buttons/modifiers and separate actual-character-code
text dispatch. Its 11 Weld and four Graft tests pass at this exact compatible
source. Supplementary Unicode and OS IME remain open.

The first production command is `cargo build --locked --features scry,weld
--target-dir C:/t/cargo-targets/turnstone -j 2`. It passes with consumer Rust
1.98.1 and CEF SDK 151.3.24. Focused serial checks pass 60 shell tests, six
`document_find` matches and 18 chrome tests. One chrome test occurs in both
filters: 84 executions cover 83 distinct tests. These are focused browser
checks; the earlier broad-suite network failure retains its original scope.

Executable SHA-256 is
`b7bfd160ceab9710810b1993ac1a98eef04984fed670ffa9c9203ad91da861bb`.
All four native runs finish with `RESULT ok`, native exit zero and no timeout:

- `native-weld-input`: independent alpha/bravo text/clicks, actual native find,
  requested zoom geometry and cleared closed views.
- `native-scry-input`: current input/scroll pixels, independent native cookies,
  resize, Reader switching, reconstruction and zero producer/cache/importer close.
- `native-scry-restart`: exact application profile and loopback origin survive
  a separate process, retaining text, localStorage, cookies and saved engine pins.
- `native-weld-permission`: exact-origin retained permission card, actual Deny
  click, CEF callback/result-page navigation and teardown. The fixture server
  independently reports `RESULT ok`.

Composed captures were directly reviewed for these outcomes. Browser-reported
DPR differs between Scry and Weld; this phase does not establish cross-engine
DPI parity. Native input comes from the real producer through the scenario
harness, with physical keyboard and OS IME qualification still separate.

`source-manifest-scry-weld.json` freezes source/scenarios/runners, the actual
Cargo command/toolchain and executable. Fingerprints pass before and after
native execution. `source-inputs-scry-weld.zip` preserves all 172 frozen input
paths as raw bytes before the Servo followup changes. The first artifact
manifest verifies 95 finished evidence files and every archived source entry.
It excludes synthetic profile bytes and active logs. Later source changes and
replacement executables do not alter this earlier qualification.
`facts-scry-weld.json` collects actual native results and unique test counts.
The first runner records loaded root-process modules; child module observation
is added only for the later three-engine runtime gate.

The selected Servo policy is one explicitly named, configurable profile shared
by its views in the process. `TURNSTONE_SERVO_PROFILE` selects its name, with
`Default` when unset. `TURNSTONE_SERVO_PROFILE_DIR` optionally selects an
absolute directory. The factory binds it once on the UI thread and retains the
process root between view closures and session changes. This is independent
of Scry/Weld per-node profile storage.

The B3 source owns upstream Servo views and ordered callbacks, imports each
paint through Graft's pre-present hook on the host device/queue, validates
texture metadata/custody, and clears views and caches before process shutdown.
It explicitly denies unsupported permission requests, preserves logical key
releases and distinguishes basic mouse/navigation/zoom limitations. The exact
upstream source is `1d44e5dd6a8b64c02f9dbf7fcbdf4ebdd0740019`. Graft's typed
adapter check passes on supplier Rust 1.97.1; its native donor and the actual
Turnstone consumer have separate gates.

Windows B3 needs matching mozangle-built `libEGL.dll` and `libGLESv2.dll`,
actual loaded-module evidence and a bounded process exit. The three-engine
binary also requires fresh Weld/Scry runs because the ANGLE DLL namespace can
affect CEF even before a Servo view is selected. The later runner uses fresh
phase-prefixed profiles/outputs and records executable SHA before launch.
Its module observer includes only verified live descendants of its owned
process, records safe browser role flags and explicit observation limitations.

`turnstone-all3-build.log` is a failed diagnostic, not a compile receipt:
Genet enables fontsan's default `woff2`, while upstream Servo enables `wuff`.
The crate's exactly-one guard rejects both together. The inverse feature tree
is retained in `fontsan-conflict-tree.log`. The owned compatibility repair
aligns the supported decoder only after valid WOFF2, malformed-input and SFNT
checks; it does not silently absorb newer Genet source. Servo 0.7 has the same
fontsan edge and therefore does not resolve the collision by version alone.
Genet `679d8314aab4ec9f57a903c79dde244c3c565c1e` passes those three checks.
The valid WOFF2 fixture retains its NKo character-map glyph after decoding;
the sanitizer function itself is unchanged. Exact tests, fixture provenance
and the stopped duplicate cache-wait diagnostic are recorded in
[fontsan-compatibility/qualification-summary.json](fontsan-compatibility/qualification-summary.json).
The production consumer's locked Windows feature graph now resolves with
only `libz-sys,wuff`, one Mere/Inker/Genet family and one wgpu 30 device family.
[Resolved graph review](fontsan-compatibility/turnstone-resolved-feature-review.json)
records Bevy reflection's separate `wgpu-types` 27 package. Compilation and
native results remain separate gates.
The current production candidates are Genet `679d8314`, Mere `edf175f9`,
Redshank/Woodshed `24f196f4` and Knot `211ff57a`. Their adapter/runtime bodies
retain the qualified baseline. The independently parsed
[supplier lock review](fontsan-compatibility/root-lock-review.json) and
[Turnstone lock comparison](fontsan-compatibility/turnstone-lock-comparison.json)
identify only the named source revisions and decoder-provider changes.
Turnstone already had Servo's `wuff` packages: its lock removes the unused
`fontsan-woff2` table and edge, retaining all other normalized package records,
registry versions and checksums. `turnstone-before-fonts.Cargo.toml` and
`turnstone-before-fonts.Cargo.lock` retain the failed consumer's exact prior
resolver inputs. `turnstone-all3-fontsan-build.log` is a completed failure:
`quinn-proto`'s rustc reports a 2 MiB allocation failure, then exits with
`0xc0000409`; Cargo exits 101. The
[build result](fontsan-compatibility/turnstone-all3-fontsan-build-result.json)
preserves the actual command and log hash without assigning an unproved cause.
The serialized one-job retry passes in 57m24s. Its distinct
[result](fontsan-compatibility/turnstone-all3-serial-build-result.json), transcript
and pre-swap manifest/lock/input hashes preserve Graft `01f3c9f`'s exact
compilation scope. No speculative serde patch or browser source repair was needed.

The added mixed-runtime scenario will initialize two Servo views, then
WebView2 and CEF in one process, compose all four current surfaces, resize and
retire them. This has a separate fresh application/profile and native gate
from the individual scenarios. The later module observer also records actual
WebView2/CEF DLL versions when available; current Rust bindings alone do not
establish the installed runtime used by a native run.

Targets remain the stable repository targets under `C:/t/cargo-targets`.
Synthetic profiles are locally ignored and retained for exact restart proof.
The three supplier compatibility revisions are reachable from published main:
Mere `392630bb`, Woodshed `06c2b13c` and Knot `6cb57f10`. All owned
compatibility worktrees and temporary branches are removed; the exact
integration and preservation guards are in `supplier-main-integration/`.
That cleanup describes the first input-adapter phase. The subsequent decoder
repair uses approved Genet/Mere/Woodshed/Knot compatibility worktrees because
their current primary trees contain newer independent work. Their new pin
family and later cleanup are recorded separately in `fontsan-compatibility/`.
Unrelated primary work, including held graph-semantics work, stays with
its owner. Package/MSRV/feature-row, fresh claimed-host hardware and registry-only
release qualification remain separate. This task has made no package publication.

## Current-stack landing and accessibility checkpoint

The subsequent U9 ruling holds final Turnstone pins until WS4 is pushed and
the current tested set moves Knot, Redshank, then Turnstone. Published Mere
`5919dc64` has R0–R5; R6 `1d87808a` remains local in this snapshot. Identity's
Knot candidate `f68af0dc` pins Mere `ea74604b`, which also lacks R6. None of
these observations authorizes absorbing that session's dirty seed work.
The older Genet/Mere/Woodshed/Knot decoder candidates and their four approved
worktrees remain preserved; their main integration is held. An independently
qualified current Genet codec owner repair can land without repinning a
consumer. Current Mere's immutable Genet `d851a9db` still selects decoder
defaults, so merely advancing Mere does not close the three-engine graph gate.

U2 prioritizes accessibility on all three operating systems. Each foreign
surface must join the host tree with original tree/node identity, lifecycle,
bounds, focus and semantic actions. Current Turnstone routing ignores
`target_tree`; Servo's pinned source exports subtrees but lacks the public
action-forwarding method present in Servo 0.7. Scry/Weld's shared producers
currently supply pixels without semantic trees. These are explicit unfinished
integration and supplier gates. The scoped passing native pixel scenarios do
not qualify assistive technology. The Claude cloud plan owner is not available
through this task's connected thread tools; a repository checkpoint records
the hold without claiming a direct relay occurred.

A later remote refresh finds Mere `fde06dc0` published, still without R6
`1d87808a`, and Knot still at `6cb57f10`. The independent current-Genet codec
repair is now published as `965b64e2`; its exact six-file publication is
recorded in
[genet-main-publication.json](fontsan-compatibility/genet-main-publication.json).
Mere's existing `d851a9db` pin does not inherit that repair. These observations
update the landing checkpoint without changing the older frozen input set.

The current-stack consumer follow-up also includes four physics refusal
call sites (`src/app/mod.rs:1157,1168` and
`src/app/session_lifecycle.rs:1050,1062`). Published Mere's law setter applies
the new law and can return the overlays it removed; its overlay setter
refuses an incompatible nonempty set without changing that set. The host
must report those outcomes and verify restored law/overlay ordering, rather
than treating every `Err` as an unchanged law. Native hosts should pass
winit's optional monitor millihertz to `set_physics_display_rate`; Mere owns
the unknown-rate fallback. Peer acceptance must also exercise the score-4/5
mismatch already refused by `remote_projection.rs`. These follow-ups are
part of the coordinated S0 migration, not qualified by the older build.

The supplier's later bounded Servo control reproduces the preservation swap
failure without its importing context. A narrow `PreserveBuffer::No` change
then passes the native wgpu 30 GPU smoke with 49 imported frames and exact
initial/click colors and resized dimensions. Its fresh default wgpu 29 check
also passes. Tested source and native receipts are pushed at Graft
`dec11bbd2c9c8676e66987fb6ca32cd4ae6310eb`; documentation-only `79261674`
follows it with identical compiled source. Fresh required CI now passes all
six jobs at documentation-only `1fd08963`; [CI evidence](graft-ci-20261007.json)
retains its exact scope. Later uncommitted synchronization diagnostics are
outside that CI result. Turnstone's completed serial build selected older Git
`01f3c9f`. Its next qualification candidate selects the tested `dec11bbd`
triplet. A targeted Cargo update changed 17 unrelated dependency edge sets;
that attempt was stopped. The prior lock was retained with only the three
Graft Git source references replaced, and locked Windows metadata accepts it.
The [lock review](fontsan-compatibility/turnstone-graft-swap-lock-review.json)
and [resolved graph](fontsan-compatibility/turnstone-graft-swap-resolved-feature-review.json)
record this bounded change. The corrected candidate rebuild passes in 6m50s,
with no speculative serde patch or source API repair. Its first native run
exits zero without timeout but reports `RESULT fail`: `graft.servo` reaches
the document-session registry and creates no Servo views. The exact 174 source
inputs and 30 native evidence files are preserved in
[servo-registration-negative-control.json](servo-registration-negative-control.json)
and `native-servo-registration-failed/`; profiles remain at their original
paths. This is a failed control, not a multi-view, orientation, text, shutdown
or accessibility qualification. The next host-dispatch run uses fresh profiles
and a new source label.

The repaired dispatcher then passes the locked build and all 64 focused shell
tests, including pending/retry regressions. Native Servo exits zero without
timeout and all scenario assertions pass, with independent alpha/bravo input,
navigation, correctly oriented initial/result pages and bounded shutdown.
Direct image review nevertheless rejects its reopen gate: capture 08 shows
Servo A white despite positive title and frame/view counts. The
[failed visual control](servo-reopen-pixels-negative-control.json) preserves
all 174 source inputs and 30 native files. The new
[fixture pixel check](review-servo-pixels.py) passes five earlier captures and
rejects that same reopen image. Reader's switch capture is an honest refusal
of static extraction from the scripted result shell; it is not a Reader
extraction qualification.

An independent unchanged-binary control, `all3-pixel-control`, passes five
fresh native runs: simultaneous four-page Servo/WebView2/CEF composition and
resize, Weld input/find/requested zoom, Scry input/scroll/reconstruction,
same-profile Scry process restart, and the real Weld permission Deny callback.
All exit zero without timeout. Direct captures establish their stated current
pixel scope. The [mixed review](mixed-control-visual-review.json) records actual
root ANGLE, CEF 151.3.24 and WebView2 156.0.4314.8 loaded-module evidence and
zero final caches/views/importers. These controls do not close Servo reopen,
foreign accessibility, final U9 adoption or package publication.

The supplier source review identifies a separate explicit ordering gap:
shared GL writes are not completed before asynchronous wgpu normalization,
nor is that copy completed before the shared source can be reused. wgpu owns
the in-flight texture resource; no hidden lease is lost by Turnstone's output
extraction. The donor smoke's per-frame CPU pixel-readback wait provides
serialization that this host lacks. The gap is source-qualified; its role in
the white reopen is unproved. A default-off, fallible synchronization control
will test producer completion and normalization completion separately before
any production scheduling change.

The later [October 7 remote checkpoint](current-stack-reconciliation/README.md)
finds R6 and the shared AccessKit helper/E1a on Mere `a59e4c47`. WS4's Git
publication prerequisite is satisfied. Mere still selects Genet `d851a9db`,
which lacks decoder repair `965b64e2`; published Knot and nested Redshank
still select older Mere families. The identity seed branch remains separate
from Knot main. Final U9 adoption therefore still waits for the coordinated
same-family tested set. E1a's windowless checks do not close E1b's human
screen-reader walks or foreign-browser semantic exports/actions.
