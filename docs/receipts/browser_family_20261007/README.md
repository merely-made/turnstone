# Current browser family, 2026-10-07

The coordinated source family is published on Turnstone `main`: runtime adoption
`abb349cf7957e2b509f4cb7f492e3e0db4821964`, followed by the test-only portability
repair `8d1f907f4bd31b256737a27d3e63fe8cd8672efc`. Final Windows S0 qualification
passes 681 library tests, zero failures, nine ignored, with all three browser
features enabled. Final Linux default-feature qualification passes 650 library
tests, zero failures, nine ignored. Both locked all-targets checks, Cargo
verification (1,280 packages), and locked trees pass; Linux metadata also passes.
This folder preserves
exact-family commands, source identity and results. Historical October 6 supplier/native
receipts remain in their own folder and retain their original scope.

The Linux first full suite on `abb349cf` preserves a genuine negative control:
644 passed, five failed, nine ignored. Locked all-targets, Cargo verification,
tree and metadata passed with unchanged tracked inputs. One test still required
an install review to fit an ordinary one-line row, although the actual Confirm
row already wraps and reserves its height. Four tests assumed that fresh Sky
calculations reproduce a Windows receipt digest on every platform. Exact DTO
comparison finds only four angular result leaves differing by at most
`7.806255641895632e-17` radians; inputs, policies, metadata and TT bounds match.
No particular math-library cause or equivalence policy is inferred.

Published follow-up `8d1f907f4bd31b256737a27d3e63fe8cd8672efc` changes only
tests and fixed test fixtures. The real retained Confirm row now checks full
DOM/A11y disclosure, card containment and click geometry. The original two
Windows Sky DTOs retain exact-byte serialization and literal `caff8371` /
`74883b5d` golden checks; fresh calculations check their own exact byte hashes
and same-platform repetition. All numerical tolerances and receipt hashing
remain unchanged. The [full production-scope proof](test-repair-production-equivalence.json)
checks the entire prefixes before the inline test modules, including the
Sky receipt module beyond its early test-only import. Its original narrower
source audit remains historical. The actual denizen control and all 24 focused
Windows Sky tests pass; complete Windows/Linux reruns now pass.

The [final Windows source qualification](windows-final-source-qualification.json)
binds its four completed gates to the clean tracked checkout `c44199e8`, which
contains only receipt/documentation changes above `8d1f907f`. Its 260 recorded
inputs match Git objects after attributes, apart from the identified inactive
local configuration. The [final Linux packet](linux-final-fd-README.md) records
the exact clean published `8d1f907f` checkout and unchanged tracked inputs.
The first standard-parallel Linux run preserves 634 passes, 16 failures and
nine ignored: OS error 24 exhausted its descriptor budget. A fresh SSH session
measured soft/hard limits of 1,024 / 1,048,576; the ended failing process's own
limits were not captured. The successful rerun raises only the launched child
tree's soft limit to 65,536, retaining standard harness parallelism, source and
test deadlines. The test process's observed descriptor peak is 2,333, a sampled
lower bound. This host setting is not a production application or system change.

See the [original Linux packet](linux-current-README.md),
[exact Sky diagnostic](linux-sky-diagnostic-README.md), and
[cross-platform comparison](sky-cross-platform-comparison.json). The temporary
Windows diagnostic did not independently capture its working-source raw hash;
that limitation remains explicit in its coordination identity record. Native
receipts retain their original binary identity, supported by unchanged
production code; the old binary is not relabelled as a rebuilt follow-up.

Mere prerequisite 57b4893db6909d5ed9c4ccae30216f0d8164201a and
Genet 965b64e206a47d1c8808472de9aa461233638768 are published. The Mere
neutral all-targets check and seven adapter tests pass. Weld supplier
c4dd593b7a730acb4aa6ffab0fe2edf8a0584083 qualifies CEF154.5.0+154.0.34
with Windows sandboxed native import pixel controls in its own repository.
Knot 0096591a0af98bf4777d8bfe718c0423dea9fc90 is published with 579 passing
Windows workspace tests plus standalone document qualification. Redshank
82271df20927d41a4cbb79c56a09fef5bbb38cde is published with 201 passing tests,
desktop all-targets and wasm checks. Graft's qualified supplier source
7907ff295da2c20c2a791419e6658d1c05350345 is published with eleven passing
Windows typed rows and native GPU initial/click/resize controls on Servo
0.7.0 (aac43a3f31a259f04a574f5ec4e959c943ad7cc7). Its documentation-only
follow-up is f991ca181507dc08b7e904adcc607d73794d533b. Later opt-in resize
and shared-resource-descriptor diagnostics are published as
bb48281bd4a88a726a009e0518706881b74b7604; this consumer selects that source.
Servo/default synchronization is unchanged. Graft's later `36d097ef` changes
only the release plan and receipt documentation; this consumer retains its
qualified runtime source `bb48281b`.
The final CI observation confirms all six jobs pass on that exact `bb48281b`
source: Windows core, Windows integration, Windows Servo demos, Windows Iced
Servo demo, macOS Metal, and Linux check/tests. The [CI audit](graft-qualified-bb-ci-audit.json)
binds the workflow and raw job record. The documentation-only `36d097ef` run
was still in progress when observed; no result is inferred for it.

The final official release/API recheck still selects Servo 0.7.0, released
October 5, and CEF `154.5.0+154.0.34`, published October 7. Servo stable's
Mozilla family remains `mozjs 0.26.4` / `mozjs_sys 153.3.0-1`. The separately
published October 7 Mozilla security update (`mozjs 0.26.8` /
`mozjs_sys 153.4.0-0`, upstream commit `34ad101f`) is outside this selected
Servo stable source; it needs a separately qualified upstream adoption.
The recorded upstream freshness audit preserves that distinction.

The first consumer resolution retains a failed control for the old p2panda
0.7.4 root patch. The eight related patches now mirror Mere's published
0.7.5 tag. The next resolution exposes Servo's CAPI ICU ~2.1 dependency
against Genet's intentional ICU ^2.2 minimum. The exact official CAPI 2.1.2
Git release source isolates its C-facing family; Mozilla's private collator
and normalizer additionally need three production ranges widened. The
vendored candidate preserves all Rust and data bytes. `resolve-icu-compat`
metadata succeeds with the candidate. The combined locked build and library
suite pass (the earlier checkpoint had 679 passing tests; runtime adoption
had 680, and the final test-only repair has 681, zero failed, nine ignored).
Seven normal page-native
Intl/normalization probes pass, with zero active Servo views and exit zero at
teardown. This is not a lock-only conflict. See the
[source identity proof](icu-source-isolation/README.md) and
[candidate provenance](icu-manifest-compat/README.md).

Scry's selected published source is 2c3ebd24118851cf6125cfb193f55aab20e3ad67.
It adds macOS pending-image retention and a Linux WPE main-thread constructor
refusal. Windows native input and same-profile process-restart controls pass; this
repin does not establish macOS capture or Linux headed acceptance.

Root source adoption reports typed physics-law/overlay refusals, preserves
monitor-derived display budgets through graph-runtime replacement, and
rejects version-four projection scores before mutating version-five state.
The combined library suite covers these changes. The published Knot and
Redshank revisions use this same Mere/Genet family. The old cloud execution
sequence and its prerequisite hashes are historical; the user's subsequent
authorization was executed in Mere, Knot, Redshank, then Turnstone order.

`native-current-family-intl-intl` is deliberately retained as a failed pixel
control: the 509-pixel content surface contains only approximately 252 pixels
of green page content. `native-current-family-viewport` uses the same executable
and normal page JavaScript. The title reports width/client/body 509 and one
resize event, while the imported page still paints width 252 and zero resize
events. The immutable `all3-current` and `all3-current-viewport` input archives
preserve both generations and their completed evidence. Successful scenario
completion does not qualify these pixels.

The current-family CEF two-page input/find/zoom/teardown control passes and its
captures show independent typed input in both pages. A concurrent default Cargo
verification failed with Windows access denied while the CEF build script was
copying runtime files. The sequential rerun passes with 1,280 packages verified.
The Linux pre-repin locked all-targets baseline also passes on published
`aafa78df`, with all 1,950 tracked source hashes unchanged.

The same `36b8cd6e` executable compares existing, producer completion,
normalization completion and both-waits with fresh named Servo profiles.
Normalization-only fails both Intl and viewport green-body pixels at about
44.56% coverage; both-waits passes both. Existing and producer-only positives
typically have six handoffs, while the failing normalization and passing both
Intl runs each have five. Both viewport captures paint the new width and resize
count. These are diagnostics, not a production synchronization fix. Imported
source readiness/reuse, actual resource-state handback and fence/resource
lifetime remain Graft release gates.

Archive verification now reads selected source/helper bytes from their immutable
input ZIP, while native outputs remain checked against their original files.
All five historical current-family archives verify after later source edits.
`archive-negative-control` deliberately substitutes a false native-capture hash
and is correctly rejected with exit one. Its broken manifest is a named negative
control, not a qualified native receipt.

The historical fingerprint field `build_environment` records the environment
at fingerprint creation, not a captured build environment. New manifests call
it `freeze_environment`; actual build commands, exits, Cargo JSON native build
events and staged/loaded module hashes provide the build/runtime binding.
Windows PowerShell 5 also serialized the original input gate's annotated
`Get-Content` result as a `value`/`PSPath` object. Its value is `RESULT ok`, all
three original sentinels say `RESULT ok`, and each native exit is zero. The
runner now records a plain string and the archive reader accepts either exact
representation. Original completed receipts are preserved.

The final native audit qualifies seven completed runs on executable
`61a81cb0864635a337e4db7ca7ded22f77d8076059fe72378b17f230f713c39a`:
Scry input and profile restart, Weld input/find/zoom and permission denial,
Servo shared-profile independent views and reconstruction, mixed four-page
composition, and seven page-native Intl probes. All exit zero without timeout
or forced cleanup. Loaded CEF154 and build-selected ANGLE hashes match. Eight
child-process module-sampling races are preserved as observation limitations.
Servo orientation captures and the mixed resized panes were also inspected.
The Intl positive explicitly enables both diagnostic waits. A default-mode
pane-expansion control on this same executable still fails pixel coverage
(47.63%), so these positives do not establish a default synchronization fix.

The [native audit](final-native-family-audit.json) binds finished files and
loaded modules. The [Windows source qualification](windows-current-source-qualification.json)
binds all 258 unchanged compiled inputs to the published source: 163 exact
Git byte matches, 94 Git attribute-filtered matches, and one archived inactive
local Cargo configuration. That local file is not an activated patch source;
the locked graph has one published source per sibling. Fixture servers were
stopped only after PID, creation time and command-line ownership checks.
Their closure receipt's executable field is null after process exit; the
original owner record and command lines preserve the verified executable.

The final `all3-current-resource-descriptor` archive preserves 258 source
inputs and hashes 184 completed evidence files, including the seven native
runs, Windows S0 results, build/runtime binding and retained default pixel
failure. Its [manifest](artifact-manifest-all3-current-resource-descriptor.json)
verifies independently against the immutable source ZIP and original finished
outputs. Linux results have their own source-bound receipt; running logs are
never admitted to this frozen Windows/native generation.

The [final independent S0 audit](final-s0-audit.json) verifies all 74 indexed
Linux artifacts, 12 completed final Windows artifacts, and 246 Linux core
inputs directly against published Git bytes. The [closed record index](final-publication-artifacts.json)
binds the final platform packets, audits, canonical status and cleanup record.
The [cleanup result](owned-worktree-cleanup-result.json) records removal of
the two completed compatibility worktrees and their exact owned local and
provisional remote references, with primary-checkout guards preserved.
Existing reusable Cargo targets and the selected CEF SDK cache remain build
inputs; this lane retains no isolated Cargo home or compatibility worktree.
Two inactive Weld native-test profiles remain under
`wgpu-weld/docs/receipts/cef154_windows_20261007/.profiles/`: automatic approval
review rejected their deletion with "blocked by policy". No bypass was attempted.

Servo accessibility semantics,
advertised actions, host subtree admission, and human Windows/macOS/Linux AT
remain separate release gates. No crate registry release is claimed.
