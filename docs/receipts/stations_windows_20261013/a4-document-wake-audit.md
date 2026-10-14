# A4 document wake audit

The final-supplier host control passed with local Mere paths, published Genet
`b6bfbeb556a6a17eddf8d5ad8254d3fcac95b084`, and published Netrender
`6a24ae08f55129b474282b8e4911564f875ce5fd`. Published Mere consumer adoption and
headed receipts remain separate gates.

The first audit found existing explicit wakes for Fetch completion, supplied
subresources, actor/physics updates, live settings and theme edits. Those paths
request the affected application windows, independently of the native producer
mailbox clock. Document work also owns an independent deadline while its last
Pelt pump reports pending work. Canvas image loading is synchronous in the
current host. Pending primary raster diagnostics now request another primary
redraw directly instead of relying on an unrelated surface producer.

Further inspection found a terminal-frame gap in the document deadline path.
Pelt's pending flag describes work remaining after the pump, rather than whether
that pump changed painted state. A due pump can mutate the document and settle,
clearing the flag before the host requests redraw. The previous statement that
all document completion wakes were sufficient was therefore incomplete.

`content_pool::pump_document_turn` now requests admitted pending windows both
before and after advancing the due turn. Winit paints the queued window after
the completed pump. Quiet component windows and hidden document appearances
receive neither a new mailbox timer nor an unconditional redraw. The regression
control uses an actual finite Livery opacity animation: its terminal pump
changes the emitted Scene and retires pending work, while only the visible
window keeps a final paint request. The restored previous after-only production
order failed this control with no requested windows instead of window 7.
Exact fixed source bytes were restored and the same control passed. See
`preliminary-b6-terminal-after-only-negative-control.log`,
`preliminary-b6-terminal-restored-tests.log`, and
`preliminary-b6-terminal-control.json` for outcomes and source hashes. The first
fixture used a `forwards` shorthand that did not enter pending work; that setup
failure remains in `preliminary-b6-terminal_document_turn-tests.log`. The final
control uses the same finite `linear` shorthand as the existing hidden-document
test, which also passed unchanged.

The narrower window polling also exposed a future-timer regression: a document
can report settled script-visible work while retaining a scheduled timer.
Pelt now exposes its existing session `pending_work` through a read-only pool
observer that preserves prepared routes and pump deduplication. Turnstone anchors
the supplied relative timer delay at the actual pump start, waits for that
deadline, and requests only the current admitted receiver's visible windows.
Runnable work uses the existing configurable clock cadence. Old due observations
are queued before replacing them, so a terminal timer turn retains its final
paint request even when another window's render performs the pump. Moves,
replacement sessions, hidden placements and withdrawal are checked against
current owner identity, without adding retention or destruction policy.

The affected shared gate passed all 25 Pelt pool tests, including the observational
future-timer and current-window controls. The host's two settled-timer tests
exercise deadline stability, actual receiver mutation and final paint custody.
The latter uses a typed hosted timer contract model rather than claiming optional
JavaScript interpreter conformance. See the shared
`pelt-b6-scheduling-source-qualification.json` and
`preliminary-b6-future-timer-repaired-tests.log`. Final source controls and native
consumer qualification are recorded separately. Physical window activation,
human AT, and monitor DPI transitions remain separate acceptance gates.

The timer-blind restored control failed both settled-timer tests: its observed
deadline was absent and its actual receiver no longer owned a wake. The candidate
bytes were restored exactly, and both controls passed again. See
`preliminary-b6-timer-blind-negative-control.log`,
`preliminary-b6-final-settled_timer-tests.log`, and
`preliminary-b6-observer-root-controls.json`.
