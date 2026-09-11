# Native Micron and NomadNet acceptance, 2026-09-11

RESULT: clean-pinned tests/build and bounded native UI acceptance passed, with
the initial-request timeout and error-view recovery limitation recorded below.

## Pinned consumer

- Mere: `1777b19840a6478a0faf3ab060a3ff71d5532321`.
- Knot: `0c8cdefb17ffa86a4e6b2a508d9a0aeb8fd2ccac`.
- Retinue: `2a763c72ec7c7ff61b8cd7adb86d084a02d2c4d6`.
- All eight owned p2panda patches: `85f88345a1d6677c32099d3d306efcb9dafeb12d`.

The initial Rust 1.97.1 workspace run of
`cargo test --lib --no-default-features` passed **458 tests, 9 ignored, 0 failed**
in 102.98 seconds. This includes local-file subset rendering, native-address
admission, and exact Stop/stale-answer/Reload request identity. The desktop
`cargo build --bin turnstone --no-default-features` passed.

That workspace run inherited parent Cargo local overrides and is not the
clean-pinned receipt. The final gate ran from `C:/t/turnstone-clean-cwd` with
an absolute manifest path and `--locked`. Rust 1.97.1 completed
`cargo test --locked --manifest-path C:/Users/mark_/Code/repos/turnstone/Cargo.toml --lib --no-default-features`:
**458 passed, 9 ignored, 0 failed**, in 103.57 seconds. The matching
`cargo build --locked --manifest-path C:/Users/mark_/Code/repos/turnstone/Cargo.toml --bin turnstone --no-default-features`
passed in 1m 07s. Final logs are `C:/t/turnstone-nomadnet-clean-full-tests.log`
and `C:/t/turnstone-nomadnet-clean-headed-build.log`.

Initial logs: `C:/t/turnstone-nomadnet-full-tests.log` and
`C:/t/turnstone-nomadnet-headed-build.log`.

## Independent reference

Stock Python NomadNet 1.4.2 served a non-executable static file over its TCP
Reticulum interface at `127.0.0.1:42429`. The task configuration was
`/tmp/turnstone-nomadnet-20260911/{node,rns}` in WSL Ubuntu. No implementation
source from NomadNet, RNS, or LXMF was read to implement this consumer.

The public destination/path entered in the browser was:

```text
7fc8950ec4e50695be76eddc8e539ee8:/page/turnstone.mu
```

A separate stock RNS 1.5.3 client first fetched and compared the entire file:
134 bytes, SHA-256
`0ac184d9d224ad09862743de0ba43ae60c0ad93ea8ec6ed4d110468e5146ba9a`.
The literal fixture is [turnstone.mu](turnstone.mu). Independent reader log:
`C:/t/turnstone-stock-reader-20260911.log`.

## Initial Turnstone UI (workspace overrides)

The app used `TURNSTONE_ROOT=C:/t/turnstone-nomadnet-ui-state-20260911` and
`TURNSTONE_NOMADNET_TCP=127.0.0.1:42429`.

1. Opened a local `.mu` file in Workbench. The shared subset visibly rendered
   the heading, rule, plain text, bold, italic, partial-preview notice, and
   literal unsupported link-looking lines.
2. Entered the destination/path in the actual omnibar with physical keyboard
   events. The Go suggestion retained the native spelling. The input helper's
   unshifted colon first produced a semicolon; this was corrected before
   committing navigation. [Address entry](native-address.png).
3. Committed navigation. Turnstone logged a 134-byte response with absent media
   type, then opened the fetched page alongside the local file with engine
   `nematic.micron-subset`. [Both views](local-and-fetched.png).
4. Clicked the visible Reload control. A second 134-byte fetch completed and
   the same native engine reopened the page, preserving the subset rendering.
5. Entered `00000000000000000000000000000000:/page/index.mu` for an
   undiscovered destination. The UI displayed Requested/Stop and Windows
   showed the app's established connection from port 64591 to port 42429.
   Clicked Stop before the 30-second deadline. The UI displayed Stopped 0 bytes
   and Reload, and the app's connection disappeared. [Stopped view](native-stopped.png).

Relevant live log observations (`C:/t/turnstone-nomadnet-ui.out`, UTC):

```text
13:41:34 local file content session live, engine=nematic.micron-subset
13:44:00 native page fetched, content_type=None, bytes=134
13:44:37 native content session live, engine=nematic.micron-subset
13:45:01 Reload: native page fetched, bytes=134; native session live
13:46:21 cancelled completion without a pending requester was dropped
```

The test app and reference node were stopped after capture. Fixtures and logs
were retained. A Windows Security prompt initially covered the app; it
disappeared before acceptance without agent interaction with the prompt.

## Final clean-pinned Turnstone UI

The rebuilt binary repeated the checks against the freshly restarted stock
node using `/tmp/turnstone-nomadnet-final-20260911/{node,rns}`. A separate stock
reader again compared all 134 bytes successfully. The app restored the local
file and fetched-page tiles, using the same explicit launch configuration.

The initial native request timed out at 13:56:15 UTC. After the independent
stock reader succeeded, clicking Reload fetched 134 bytes at 13:57:14. The
error tile still showed its earlier error until Open node in Workbench was
selected. The resulting [local and fetched views](clean-local-and-fetched.jpg)
both visibly used the shared partial renderer. The cause of the first timeout
is unqualified; this receipt does not establish reliable cold discovery or
automatic recovery of an error tile.

Another visible Reload fetched 134 bytes and recreated the native session at
13:57:53. Physical [native address entry](clean-native-address.jpg), selecting
the open-address suggestion and pressing Enter fetched another 134 bytes at
13:58:24, with `engine=nematic.micron-subset`.

Entering the all-zero destination started a lookup at 13:58:41. Windows showed
app PID 28452 with an established connection from port 52363 to port 42429.
Clicking Stop at 13:58:56 produced [Stopped 0 bytes](clean-native-stopped.jpg)
and the app's TCP connection disappeared. Log: `C:/t/turnstone-nomadnet-clean-ui.out`.
The exact app executable was checked before stopping PID 28452. The task-local
stock daemon PID 436 was stopped with TERM. No recovery or HOME mutation was
performed during this final run.

## Remaining limits

This is an explicitly partial Micron renderer. Additional syntax and clickable
Micron links remain unqualified and inert. The canonical gap analysis lists
the individual constructs needing evidence. Transport configuration is through
process-launch settings, not a dedicated connection-settings pane. The accepted
body-size limit is checked after Resource reassembly, not during wire allocation.

Go `view-mu` large Resource interoperability remains failing despite independent
Python large-transfer receipts. Clipboard paste did not enter text in the
Turnstone omnibar; physical typing did. Knot's outer-canvas painting defect is
separate. Native address fetches also emit a harmless existing Gemini-identity
projection warning before the native route; no Gemini identity is supplied.
These defects are not claimed resolved by this acceptance.
