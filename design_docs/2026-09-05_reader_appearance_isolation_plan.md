# Reader appearance isolation

Status: bounded implementation, focused tests, and native Reader scroll/close,
idle-wait, and observed shutdown gates passed, 2026-09-06. CPU/FPS sampling remains open.

This is Turnstone's consumer slice of Mere's
`design_docs/mere_docs/implementation_strategy/2026-09-05_projection_refresh_and_surface_reuse_plan.md`,
authorized by Mark. Reader article content is shared; viewport, layout, scroll,
input focus, and raster identity belong to each appearance.

Done when a Reader document can appear in a workbench tile and graph inset at
different sizes; resizing/scrolling one preserves the other; removing a sibling
appearance preserves the survivor's identity; replacing the document discards
stale cached content. Stable pane/role identity must not depend on traversal
order. Other engines retain their existing single-session behavior.

Validation: focused shared-document and planner/input tests, followed by a
Turnstone compile check. Do not claim all engine lanes support independent
appearances. Preserve concurrent changes and record actual command outcomes.

## Findings

Reader and Smolweb retain the same `Arc<EngineDocument>` in each rendered
appearance. The focused Reader test passed and checks pointer identity of the
rendered packet, different viewport layouts, and independent scroll.

Turnstone assigns appearance identity from the document and stable pane role,
including separate lens namespaces and floated panes. Render, pointer, wheel,
and keyboard-scroll routing carry that identity. Overlapping copies consume
separate roles. Navigation and document close release old appearances; the
existing pane/window-close hook releases only the closed pane's retained state.

The host changes require the updated Mere dependency family. Validation uses a
temporary coherent local override; the committed Mere revision is unchanged.
Native compilation passed with 122 local Mere package overrides and the pinned
Distillery exception. The unrelated local Distillery working copy calls
scenomise without declaring that dependency; its source was preserved.

The actual pure surface modules passed 13 tests in a source-path harness using
a minimal numeric PaneId. They cover appearance routing, identical overlapping
rectangles, sibling-close identity, and pane-specific cleanup. The full
Reader/Smolweb library suite passed 20 tests. The
[combined receipt](../../mere/ports/graphshell/docs/receipts/projection_refresh_surface_reuse_receipt.json)
records logs, hashes, and commands. The headed native Reader interaction is
`scenarios/reader_appearance.scn`, run by
`scenarios/run_reader_appearance.ps1`. It drives the local article fixture,
selects Reader through Apparatus, opens the same node in Workbench, resolves
wheel targets from the live Shell plan, and records the sorted appearance ids,
shared source group, rects, viewports, and retained scroll positions before
and after each scroll and sibling close. Its runner rejects absent captures or
observations and checks that the closing receipt contains only the inset.
Graphshell's browser receipt does not stand in for this native gate.
The native executable build and headed runner passed on 2026-09-06. The
[native receipt](2026-09-06_reader_appearance_native_check_receipt.md) records
different viewports (Workbench 251x570, inset 305x600), shared source identity,
independent offsets (Workbench 240, inset 160), and unchanged inset identity
and offset after Workbench closes. The runner also rejects an extra appearance
or divergent source after either scroll. Screenshots were visually inspected.

The first run reached all three 240-frame global wait caps. Its explicit Reader
assertions passed; it does not prove whole-app quiescence or frame performance.
The follow-up below identifies the busy condition and verifies completion
without changing wait budgets. It supersedes the earlier hypothesis that
Canvas's 360-tick settling budget explained these 240-frame caps. An iroh endpoint-close
diagnostic also appeared during successful shutdown. Interactive resize and
navigation replacement remain outside this native scenario's coverage.

## Idle and shutdown follow-up, 2026-09-06

The diagnostic native run identified pending fetches as the sole busy condition
after all three capped waits: graph settling was false, content requests were
complete, and no document session was unsettled. The cause was a failed
best-effort favicon request: the fetch actor emitted only successful results,
leaving the failed request in the host's pending map.

Mere now emits a typed terminal favicon result with request identity. Turnstone
retires exactly that pending request on success or failure, while failures stay
UI-silent. Three focused favicon tests passed, including same-page out-of-order
completion and failed-request cleanup. The normal `busy()` poll remains
allocation-free; detailed diagnostics are collected only on explicit requests.

Share-reader and publishing services now own and join their worker threads,
signal shutdown, and await transport close. Reader shutdown cancels an in-flight
request without shortening normal request duration. Place lane teardown awaits
watcher cancellation and endpoint close before dropping its runtime. The first
diagnostic run no longer emitted the ungraceful endpoint-drop warning.

The combined native rerun passed the original appearance assertions, reported
`busy=false` at all three checkpoints without raising wait budgets, and emitted
neither of the rejected ungraceful-shutdown diagnostics. Waits completed in
103, 0, and 0 frames instead of reaching the three 240-frame caps. The runner
now enforces these idle and shutdown conditions. The native build passed;
three focused favicon tests passed. See the follow-up section of the
[native receipt](2026-09-06_reader_appearance_native_check_receipt.md).

This observes the native scenario's idle predicate and shutdown, not zero
background CPU or a frame-rate target. Active place membership and an ongoing
publishing transfer were not exercised by the fixture.
