# Scoped fix-round review

**Verdict: Needs fixes.** One Important issue remains in the native mouse-mask adoption. The other original fixes and the pointer-ID correction are accepted within this scope.

## Critical

None found.

## Important

1. **Native release does not clear the window button mask after capture retirement.** `/home/markik/Code/worktrees/turnstone-resource-compat/src/shell/input.rs:1074` reads and updates `window_buttons` only inside `content_mouse`. The actual lens release handler at `/home/markik/Code/worktrees/turnstone-resource-compat/src/shell/lens.rs:814` calls that adapter only while `captured_mouse` still exists. Reproduction: press Middle over lens content, replace its producer (or hide its appearance and synchronize placements), then release Middle without changing native focus. The pool correctly sends Cancel and removes the capture, so this release bypasses `content_mouse` and leaves AUXILIARY in `window_buttons`. Reopen/current content then receives a Left Down with PRIMARY|AUXILIARY and a Left Up with AUXILIARY, although Middle is physically up. Surface producers consequently observe a phantom held button in later gestures. The primary release handler has the same split: `/home/markik/Code/worktrees/turnstone-resource-compat/src/shell/input.rs:1148` clears only `surface_pointer_buttons`, while its capture guard at `:1163` can skip the authoritative map update; the next `content_mouse` overwrites that field from the stale map.

   Update physical button state at the native window mouse-event boundary, independently of whether content currently owns capture or receives the event, and let the content adapter consume that post-event state. Keep cancellation and actual button release distinct. Add a regression through the adopted host route: Middle Down -> pool cancellation by replacement/hide -> native Middle Up without capture -> next Left Down/Move/Up; assert exact masks, no old activation, and no old terminal delivery to the replacement. Retain the unrelated-release control. The current test at `/home/markik/Code/worktrees/turnstone-resource-compat/src/shell/window_input_tests.rs:414` calls `mouse_buttons` directly, so it cannot expose this skipped native call.

## Minor

None found within the requested scope.

## Complete fix-list assessment

- **Touch placement/lifetime: accepted.** Primary and lens Winit callers now enter `deliver_touch_in`/`deliver_touch_contact_in`, synchronize placements and use `TouchInput` with window/host-ID translation only. The pool owns appearance capture and delivery. Lost captures retire translations before late Move/Up; focus loss invokes the pool's window-wide cancellation. Second-appearance coordinates, sensor payloads, independent contacts, hide/reopen, replacement and focus loss have focused controls.
- **Source-window keys/IME: accepted.** Native primary/lens handlers pass the originating window. `select_input_window` requires native focus and validates remembered focus against that window's plan. Content keyboard, text, IME and focus traversal use `pool.input_at(window, ...)`; primary-owned editors reject lens key/IME delivery. The helper tests establish pool routing; actual caller adoption was checked separately by source inspection, not represented as a physical title-bar test.
- **SDK Keyboard completion: accepted.** Mere `ports/pelt/core/src/content.rs:332` lowers the document Keyboard event into `self.input`, retaining `after_document` and the ordinary route/lane completion path. The regression compares Session Enter and Keyboard Enter through the real content/controller/router/factory path, asserting the same completed surface route, consumed reroute and one spawn.
- **Lens auxiliary mouse and zoom: partly accepted.** Actual lens native dispatch accepts left/right/middle, uses pointer ID 1, sends movement and matching/unrelated releases through pool custody, and shares the primary page-zoom policy. Normal delivered-event masks are correct. The cancelled-capture release gap above prevents full acceptance of the post-event mask requirement.
- **Click jitter: accepted.** The product-local setting is documented, parsed as finite/nonnegative physical pixels with default 4, and read on pane construction. Small movement, recognized drag, Cancel and duplicate-release controls preserve release-only activation. Pure parser controls cover custom, zero and invalid values.
- **Related mouse pointer ID: accepted.** Capture queries now use the actual `mouse_pointer_event` ID 1 in primary and lens callers. The focused test constructs that real event and obtains its captured placement.

## Evidence and limits

Read all 2,315 lines of the supplied two-repository `source.diff`, including tests. SHA-256 is `6d2252cfdf38c4d65338fbe6509ce65d04761705e07ab3a9815bf386ac02166a`, matching metadata (89,603 bytes). Reviewed Turnstone `849b57d` -> `a40d217` and Mere `7b549347c` -> `0eea19951`; Genet `8939637d` is unchanged. Used the prior complete review, supplied reports, and targeted current-source reads for native callers and pool contracts. No fresh Git diff, source edits, Cargo/rustc, browser, tests or subagents were run. Only this ignored review artifact was written.

Read retained result lines: Turnstone input RED 1 passed/14 failed and GREEN 15/0; current content-pool covering 8/0 and native-foreign-lens covering 1/0. Mere Keyboard RED 0/1, focused GREEN 1/0 and covering content8 + host_surface13 = 21/0. These are existing receipts, not reviewer reruns. The instrumented producers and real Gemini/Livery/Reader/AccessKit consumer controls keep their documented evidence boundaries.

Broad/default/Piccolo/all-target/headed acceptance, Graphshell full runtime acceptance, final supplier pins/portable gates, native platform-global TreeId behavior and human/non-Linux AT remain separate pending qualifications. Their pending status is not the reason for this code verdict. Existing warnings and ordinary AccessKit structural-validity caller contracts were not expanded into new findings.
