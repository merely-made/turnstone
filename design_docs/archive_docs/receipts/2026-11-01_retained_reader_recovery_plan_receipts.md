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
