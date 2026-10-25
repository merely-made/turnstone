# Linux native Servo through Cue — 2026-10-23

The real Turnstone product receives completed Servo DMA-BUF snapshots through
Wgpu Cue on the ThinkPad's hardware RADV Renoir Vulkan device in a native GNOME
Wayland session (`DISPLAY` absent). Both native controls/lifecycle receiving and
simultaneous retained Genet/native Servo receiving pass on executable
`5b77504a3bc627d9b70b9de92ec5cbf6da75db0ee33f918bedd75827701a16aa`.

[Source identities](source.json) bind the clean Turnstone `a2655c6e` build,
Mere `114f4ea26`, Genet `2b5c0181`, Graft `db273681` and the published
Wgpu Cue 0.1.2 cohort. Frozen manifests, lock, commands, test results and
checksums accompany the captures. The two native runs use fresh synthetic
profiles and loopback HTTP fixtures; no production vault or executable is
published here. The private raw output retains the copied executables and
complete logs.

- **Native controls and lifecycle:** actual page text entry/deletion, button
  navigation with that value, reload, Back/Forward, effective 110% zoom, lens
  sizing, return/dock, and close/reopen pass. The primary/lens frame scope is
  shared, then changes after reopening. The native run has 488 primary
  presentations and 13 primary captures without a blank capture; a separate
  composed lens capture and its actual source raster are present.
- **Pixels and the actual adapter:** seven native source rasters pass exact
  red-top, white-centre, blue-bottom pixel checks through resizing, navigation,
  zoom, lens and reopen. Four native rasters pass in the mixed case too. Every
  observed native source names hardware Vulkan and device generation 1.
- **Both frame directions in one frame:** each mixed-card checkpoint matches
  the reported native and host-device surface IDs to two actual captured source
  rasters, from Servo and retained Genet on the same device. The mixed case has
  736 primary presentations and four nonblank primary captures. Its two global
  idle waits reach their 240-frame bounds; the following image, engine and
  lifecycle assertions pass. This is frame receiving evidence, not idle-cost
  acceptance.
- **A preserved failed receiving gate:** the earlier full controls scenario
  succeeds and its composed lens image is correct, but the driver rejects the
  missing lens source-raster diagnostic. [That failed gate](negative-lens-diagnostics)
  retains its executable identity, successful scenario outcome, image and
  rejecting driver output. The lens now uses the primary path's asynchronous
  diagnostic readback; the repeated full case passes the unchanged stronger
  guard. Unit guards also reject the wrong direction, sequential-only mixing
  and cached entries without corresponding actual source rasters.

The [canonical Graft implementation record](https://github.com/merely-made/wgpu-graft/blob/main/design_docs/2026-10-23_linux_native_cue_plan.md)
explains snapshot ownership, producer completion, Vulkan acquisition and the
shared Genet core. The lower provider and SDK receipts separately qualify
orientation/half alpha after producer destruction, descriptor refusal/closure,
registry receiving and the wgpu version matrix. Diagnostic CPU readback is
opt-in evidence collection, not the product's frame transport.

Physical IME/candidate windows, native physical keyboard projection, human AT,
physical device loss, transparent native colour/alpha, external semaphore
transport, X11/Xwayland and OS-compositor pixel correlation remain outside this
receipt. Servo's live report retains `Constructed` evidence and explicitly
unavailable native keyboard/IME/semantics. The fresh public-fixture profile's
identity remains pending/locked; identity-bound behavior is not qualified. The
retained runtime diagnostics also preserve the unavailable clipboard and the
single-GPU DRI_PRIME fallback advisory. Earlier sealed Linux retained and scroll
repair receipts remain immutable.

The broader Servo-feature library run records 1,182 passing tests, 17 ignored
and one failing Reader fixture assertion. That failure reproduces in isolation:
the fallback chain permits a Linux system face to cover the link arrow, so it
need not use the bundled face's exact bytes. The test-only follow-up `e527392`
explicitly requests the bundled family while preserving the byte-identity and
non-missing-glyph assertions. The affected content-pool group is rerun alongside
the native pending-frame, all-targets and Pelt receiving checks. The complete
1,200-test run is not repeated after that fixture-only correction. Its original
failure and isolated reproduction remain in the validation archive; the native
UI executable remains the separately qualified `a2655c6e` build. Cargo's
intentionally invalid Servo tidy fixture discovery diagnostic and existing
compiler warnings are retained with their command exit codes.

The final focused gates pass seven content-pool tests, one explicit hardware
Vulkan pending-frame test, both default and Servo all-targets checks, and all
nine Pelt receiving tests. Three receiving-guard unit tests pass. Every locked
gate retains unchanged lock bytes and a clean source status. Five local primary
checkouts and all 16 preexisting ThinkPad worktrees retain their original heads
and status; all 285 prior receipt files remain byte-identical. No task-owned
build or application process remains. The recorded CI snapshot separates
Wgpu Cue's green full workflow from any still-running Graft platform jobs.
