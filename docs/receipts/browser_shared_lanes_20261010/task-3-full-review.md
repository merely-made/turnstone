## Spec compliance

❌ Issues found. Retained custody, placement admission, shared pointer payloads, Reader appearances and bridge replay are implemented, but native Turnstone adoption still has touch lifetime and window-routing gaps. The new document Keyboard envelope also bypasses Pelt's navigation completion path.

⚠️ Qualification remains separate: current Turnstone/default Piccolo/all-target/headed acceptance, Graphshell runtime, final supplier pins and portable gates remain pending. Windows/macOS runtime, native global TreeId behavior and human AT are not established by this review or the supplied automated results.

## Strengths

- `/home/markik/Code/worktrees/mere-three-lanes/ports/pelt/core/src/pool.rs:199` and `:341`: placements own generation/geometry/window observations; addressed preparation restores the correct viewport without polling surface resources. Content removal and appearance removal have distinct custody.
- `/home/markik/Code/worktrees/mere-three-lanes/ports/pelt/core/src/pool_a11y.rs:157`: dispatch checks captured publication before preparation, then proves full projection equality before translating a restored revision. This protects repeated viewports without renewing arbitrary stale actions.
- `/home/markik/Code/worktrees/mere-three-lanes/ports/pelt/core/src/workspace.rs:585` and `/home/markik/Code/worktrees/turnstone-resource-compat/src/shell/content_pool.rs:129`: both real consumers use the pool; Reader appearances share source while retaining independent mutable sessions.
- `/home/markik/Code/worktrees/genet-three-lanes/components/genet-winit-host/src/a11y.rs:74`, `:189`, `:309`: activation snapshots are ROOT/guest-free/root-focused, target TreeId survives action delivery, and every platform adapter invokes the epoch-checked supplier inside its update closure. Complete retained replay and stale epoch refusal are covered. No general malformed-tree validator is required by the controller ruling.

## Critical

None found.

## Important

1. **Turnstone touch still bypasses placement and lifetime custody.** `/home/markik/Code/worktrees/turnstone-resource-compat/src/shell/input.rs:1287` retains only pointer ID and node; `:1302` chooses the first current appearance of that node, and `:1310` sends directly to the current producer. Thus a touch on a second appearance uses the first appearance's coordinates; replacing the producer under the same node sends later Move/Up to a producer that never received Down. Hiding/removing an appearance sends no Cancel, and reopening can resume the stale contact. Focus loss at `:1051` only cancels pool pointer 0, leaving these touch contacts alive. **Fix:** retain host-ID translation only, route the full touch event through the pool using the actual appearance/window, and retire contacts through window focus loss and placement/content replacement. **Focused negatives:** second appearance coordinates, Down → hide/reopen → Up, Down → provider replacement → Up, and Down → focus loss → Up; expect Cancel on the original producer and no activation/new-producer delivery. Owner: Task1 consumer.

2. **Native keyboard/IME can reach a different window's document.** `/home/markik/Code/worktrees/turnstone-resource-compat/src/shell/lens.rs:601` forwards keys and `:612` forwards IME without the originating window. `Focused(true)` at `:583` leaves the global `app.focus` unchanged. `/home/markik/Code/worktrees/turnstone-resource-compat/src/shell/controller_input.rs:42` then selects that global appearance and `:68` uses explicit placement input, which requires visibility, not native focus/window ownership. Focus a lens through its title bar after editing a primary-window field: typing can still modify the primary field, despite its placement being unfocused. **Fix:** carry the source window through keyboard/IME routing and select/validate its focused placement, retaining per-window focus as needed. **Focused negative:** move OS focus without clicking content, then send text, Enter and IME commit; assert the other window's document is untouched. Owner: Task1 consumer.

3. **The document Keyboard variant skips lane transition completion.** `/home/markik/Code/worktrees/mere-three-lanes/ports/pelt/core/src/content.rs:331` calls `controller.input` directly. The ordinary Session path at `:309` runs `after_document`, whose `:582` implementation updates route metadata and performs document-to-surface swaps. Enter through `PeltInput::Keyboard` on a routed link can therefore leave the document lane in place with an unconsumed reroute, while equivalent Session input works. **Fix:** lower the keyboard event, then use the same `self.input`/`after_document` path. **Focused regression:** adapt the existing routed-content fixture to activate a surface-bound link with the Keyboard envelope and assert the lane and route match Session Enter. Owner: Task3 SDK.

4. **Lens mouse and modified-wheel routing remain incomplete.** `/home/markik/Code/worktrees/turnstone-resource-compat/src/shell/lens.rs:655` and `:788` admit only left-button content gestures; right/middle events fall through to graph-only routing. The new `content_mouse` calls at `:724` and `:803` also never update `surface_pointer_buttons`, so their shared pointer envelope does not reflect the pressed button. At `:817`, Ctrl+wheel is sent straight to document Wheel input, which declines Ctrl instead of invoking the primary window's page-zoom policy. **Fix:** use a shared window-aware content mouse/wheel adapter, maintain the real post-event button mask, and apply the established page-zoom policy consistently. **Focused regression:** lens auxiliary press/move/release, unrelated release, and Ctrl+wheel on a zoom-capable document, compared with the primary path. Owner: Task1 consumer.

## Minor

- `/home/markik/Code/worktrees/turnstone-resource-compat/src/contributed_surface.rs:525`: any nonzero motion permanently marks an ordinary click as a drag, so even one pixel of hand jitter suppresses activation at `:535` despite release over the same target. Prefer the host's configurable drag threshold or explicit drag recognition. Add a small-motion click control beside the existing drag/cancel controls.
- Supplied compiler warnings concern unused patches, upstream deprecations, existing platform-gated helpers and unchanged Pelt sites. They do not justify workspace-wide cleanup or invalidate the scoped passing tests.

## Checks and evidence boundary

- Read the three labelled diff blocks in ordered passes; packet SHA-256 matches `61973a5dcf9d8bc3eb69ad71b757383c16ecac51e3ccb215da75de3e65dd1f73`. No Git diff was rederived; no compiler, tests, browser or source mutations were run.
- Focused source reads were limited to the cut-off touch function and its lifetime references; window keyboard routing; the Keyboard/`after_document` cross-contract risk; and the drag adapter, which does use `PeltInput::Drag` at `/home/markik/Code/worktrees/turnstone-resource-compat/src/shell/gestures.rs:392`. These checks do not propose removing existing publishing, Tabard, Moot or readback integrations.
- Inspected durable result lines: `/home/markik/Code/worktrees/turnstone-resource-compat/docs/receipts/browser_shared_lanes_20261010/task3-native-postmerge-retry.log:191` reports native Pelt **72/0**; `task3-shared-postmerge.log:334` through `:593` report **433/0** across Cambium255/rootstock75/Mesquite28/PeltCore75. The explicit local-lock mismatch is retained in `task3-shared-postmerge-lock-check.txt:2`, consistent with the report's preserved solver output. Genet bridge **15/0** and Ortet **6/0** remain pre-integration evidence at `task3-resume-bridge-green.log:22` and `task3-resume-ortet-green.log:32`. Task1's lifecycle8 and native-foreign-lens1 receipts are acknowledged as controller-supplied evidence, not rerun here.
- Recommended tests above address concrete gaps in those controls. Existing passing suites need not be repeated merely to reproduce their evidence.

## Assessment

**Task quality: Needs fixes.** The shared pool and bridge are substantial, coherent improvements. The remaining blockers are specific input routes that evade or omit the new contracts, rather than missing broad qualification being treated as a source defect.
