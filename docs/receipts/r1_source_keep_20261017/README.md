# R1 source keep headed receipt

Final bounded macOS qualification passed. The [qualification record](qualification.json)
binds Turnstone base `dc630b7`, published Mere `1be0bc04f`, the final source/lock
hashes, and native binary SHA-256
`7b485620dbafe4dc4efcd310510fd9f7e0c4287b26372091f2be2aac1288301a`.
The final run is `turnstone-r1-native-final-04`: 953 bounded library tests,
three separate capture-library controls, 34 fetch + 12 PageLoad supplier tests,
Clippy correctness, and both fresh headed native processes pass.

The [native verification](native/verification.json) proves five Saved request
identities and A's exact ordered manifest references plus Keep after offline
reopen, with zero new fixture requests. It also checks two refused stale
controls, A-bound queued dispatch with cursor B, and uncaptured B/X/swap members.
All [12 captures were visually reviewed](native/visual-review.json). Lower
Inspector count/Keep rows are outside the captured viewport, so those values
rely on the ordinary observation assertions and independent persisted-ledger
checks. The cursor-B image shows B Ready/unkept; A's queued outcome is established
by its request ledger.

Cold reopen initially exposed expensive per-proxy-callback work starving
Winit's native event loop. Worker drains and accessibility publication now run
once at the end of each event batch, and actual unocclusion requests a redraw.
The final process independently encountered four occluded acquisitions, then
completed 26 successful presentations without intervention; the
[scheduling receipt](native/host-scheduling.json) retains that control. Earlier
harness failures and the stalled pre-fix process are
[diagnostics](diagnostics/summary.json), not acceptance passes.

The packet's verifier can also run without the original scratch directory:

```sh
python3 docs/receipts/r1_source_keep_20261017/verify_native.py \
  docs/receipts/r1_source_keep_20261017/native
```

Qualification is for app-native input/presentation on macOS with acknowledged
writes and tested process restarts. It does not qualify power loss, the
unrestricted shared-place/Sky test profile, other platforms, OS assistive
activation, or R2-R4's private reader/Answers workflow. Legacy PageText bytes and
Answers references remain readable; no purge or automatic corpus migration is
performed.

This packet drives the visible Inspector action with the native scenario lane,
records the current session's correlated source request facts, and verifies the
saved node facet ledger from disk. It uses a local HTTP fixture for A, B, and
visited-only X. A fresh second native process opens the same scratch profile,
selects A from graph truth without navigating to its URL, and checks Keep plus
the manifest count while the runner checks that the fixture saw no new request.

The runner does not build. After the parent-owned binary is ready, start it from
the repository root with a new receipt directory outside the checkout:

```sh
python3 docs/receipts/r1_source_keep_20261017/run_native.py \
  --binary target/debug/turnstone \
  --keeper-binary /path/to/tucket-custody-receipt-host \
  --seed scenarios/r1_source_keep_seed.scn \
  --reopen scenarios/r1_source_keep_reopen.scn \
  --timeout 600 \
  --output /Users/markik/Code/output/turnstone-r1-source-keep-<run-id>
python3 docs/receipts/r1_source_keep_20261017/verify_native.py \
  /Users/markik/Code/output/turnstone-r1-source-keep-<run-id>
```

The scenario screenshots are the human review surface. The receipt verifier
requires both native processes to report success against the same frozen
binary, the isolated no-wallet custody fixture to exit, five save requests to
be acknowledged, and two queued catalog actions
to fail after A/B/A retargeting and same-URL document replacement. All request
ids must belong to A's same session/graph/node and carry Keep intent. One queued
Inspector-offered A action is delivered after the graph cursor moves to
existing B; its saved request must still belong to A. The exact ordered
manifest references must survive a new process. The fixture replaces A's body
at the same address; the verifier requires a changed source identity only
after the fifth successful explicit save. The X recall query first finds its
address, then checks that an ordinary visit did not make its unique body-only
phrase searchable. The latter requires exactly the empty-result Hint, rather
than treating that Hint as a retained content match.

The runner freezes both supplied executables, serves fixture pages on loopback,
and launches an isolated no-wallet custody host for the app's cookie and
identity boundary. It uses a bounded display-awake assertion and separate
profile/XDG roots and an explicit scratch theme-library path. It stops after the first failed or timed-out phase and
retains the launch log and partial output; it does not retry a blocked or
unavailable GUI. A `RESULT ok` process receipt and the expected screenshot set
are required before this is called a headed acceptance; a launcher invocation
alone is not evidence.

The scenario explicitly queues the exact offered action and delivers it only
after the retarget or replacement step, exercising stale source refusal through
the regular action path. Wrong-session refusal remains covered by the focused
application tests rather than this visible UI lane.

The startup custodian is the existing immutable no-wallet fixture from the
[scene receipt](../graph_scenes_20261016/inputs.json), Mere `0eb2218a6`, binary
SHA-256 `8605534361a92f7b666a5681edfe38352f2f3d4ff538db57a78a293a174847a3`.
Its source file is unchanged against the current supplier. It provides only
synthetic startup identity/cookie isolation; the production Turnstone build and
its session-local source storage use the final locked supplier cohort recorded
in this receipt. No current Djinn release qualification is inferred.
