# Reader appearance isolation

Status: bounded implementation and focused validation complete; headed native
Reader appearance receipt added and pending its first executable run, 2026-09-06.

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
The native compile check passed; the headed runner remains pending while an
external Cargo package-cache holder is active, rather than adding another
locked build waiter.
