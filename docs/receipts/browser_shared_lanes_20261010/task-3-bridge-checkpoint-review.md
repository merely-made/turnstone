# Genet native accessibility bridge checkpoint review

**Verdict: Approved for this scoped checkpoint.** No Critical or Important findings. One nonblocking documentation finding remains below. This is neither full task 3 approval nor whole-branch or release approval.

## Review binding and evidence

- Base: `a0d95ebd6e20220e3430870365112a608b4f83ff`; head: frozen uncommitted checkpoint represented by `task-3-bridge-checkpoint-review.diff` and its adjacent JSON binding.
- Binding diff SHA-256: `427ba354d1dc76f3d0e6db379448e81819adc21eaa302c9100175dbb1eb890a3`.
- Reviewed `components/genet-winit-host/src/a11y.rs` and `components/genet-winit-host/Cargo.toml`. Direct source hashes matched the binding: `b3c14cc4d41a8e53e5f92346423c86520b4acbb50d520131deeabd6c5dc058d3` and `4c9b2ae76afbddd963353527a994508f439f612f05d2147a6641b7d482cb1b6a`, respectively. Changed-source reads recovered tool-truncated diff hunks.
- The current evidence is the report's **2026-10-10 resumed verification** checkpoint. Earlier pending/RED statements are historical.
- Inspected existing receipts: `/tmp/task3-resume-bridge-green.log` reports **15 passed, 0 failed**, comprising 11 accessibility and 4 wheel controls. `/tmp/task3-resume-epoch-red.log` contains the 15 intended missing-API/callback-shape errors. `/tmp/task3-resume-activation-guard-red-genet.log` reports the intended behavioral **0 passed, 1 failed** before repair. Durable receipt copies are recorded by the task report.
- No builds, tests, Git operations, or implementation edits were performed during this review. Only this requested report was written.

## Spec Compliance

- **Host boundary:** the bridge consumes AccessKit types, a winit window and callbacks. Its publication/action state is shared by the native adapters without introducing a Genet-to-Mere dependency.
- **Safe initial activation:** `a11y.rs:74` rejects missing tree metadata, non-ROOT identity, non-root focus, any guest-bearing node and missing root. Validation precedes publication replacement. Single-host compatibility clears guest references and refuses non-ROOT publications (`a11y.rs:102`). Ordinary AccessKit structural validity and completeness of the supplied initial tree remain host caller obligations, as clarified by the checkpoint owner.
- **Replay order and consumer validity:** the stored vector preserves caller order. The consumer tests exercise host installation, guest installation, focus, colliding NodeIds, replacement, removal and fresh-consumer replay prefixes. This establishes the tested publication protocol, not arbitrary vector validation.
- **Action custody:** `a11y.rs:186` captures action, TreeId, NodeId and data into the neutral request before calling the custom sink or enqueuing, and wakes afterward. Custom custody replaces default queue delivery.
- **Activation lifecycle:** checked monotonic increments occur on activation and Unix deactivation (`a11y.rs:118`, `134`, `157`). Bound replacement checks the expected epoch before mutation, again under publication custody and after replacement (`a11y.rs:281`).
- **Adapter callback boundary:** `a11y.rs:309` checks the epoch inside the supplier, substitutes the safe root on mismatch and returns false. All native adapters invoke that supplier inside `update_if_active`: Windows `a11y.rs:488`, macOS `635`, Unix `775`. Interrupted batches stop between entries; stale retained bound publications are refused before replay, including replay reached through drain. Successful completion requires every supplier to have been invoked and accepted in the same epoch.
- **Steady focus:** ordinary delivery sends only the ordered vector. It does not prepend the safe root on each frame. Bound publications may therefore preserve existing guest trees and focus within their activation epoch.
- **Bounds:** Unix `a11y.rs:784` reuses the existing physical-bounds helper. Windows and macOS no-op bounds methods are supported by installed primary dependency source: Windows 0.32.1 `src/util.rs:299` queries `ClientToScreen`; macOS 0.26.3 `src/util.rs:57` reads current backing scale/view bounds and converts through the current view/window screen transforms. These are source observations, not platform runtime receipts.

## Strengths

- Shared publication and lifecycle logic reduces differences between the three native adapters while preserving their native event handling.
- The supplier-based delivery contract addresses the actual activation-before-callback boundary, rather than relying solely on a check before calling the adapter.
- The tests use the real AccessKit consumer for guest ordering and identity boundaries, and distinguish missing-API RED from behavioral RED and subsequent GREEN.
- The recorded limitations correctly preserve the boundary between library tests and native/human accessibility acceptance.

## Findings

### Critical

None found.

### Important

None found within the stated caller contracts and checkpoint scope.

### Minor

**Explicitly document the reset-safe unbound replay contract.** Locations: `/home/markik/Code/worktrees/genet-three-lanes/components/genet-winit-host/src/a11y.rs:439`, `:586`, and `:726`.

The `update_batch` docs call the vector a “complete ordered publication,” but do not explicitly say that it must be valid starting from the supplied safe initial tree after every activation reset. The implementation intentionally retains and replays that vector across epochs. A caller mistaking “complete” for all updates needed by the currently active consumer could provide guest deltas that fail on a fresh consumer. State directly that the initial snapshot must be a complete, structurally valid ROOT tree and that the entire ordered vector must be reset-safe; direct epoch-dependent steady-state publications to `update_batch_at`. The latter's existing documentation already conveys the distinction in the opposite direction. This is a documentation clarification, not a request for general TreeUpdate validation or an identified failure with conforming inputs.

## Scope-unverifiable items

- Windows/macOS compilation and runtime behavior, real screen-reader operation and human accessibility receipts were not established by this Linux library GREEN or by dependency-source inspection.
- Arbitrary host-supplied tree/vector validity, consumer action-admission integration, complete Pelt/Ortet/Turnstone adoption and the remaining shared-input/pool task 3 gates are outside this early bridge review.
- No introduced actionable warning was identified in the inspected receipts. Previously disclosed unused-patch/dependency warning cleanup is outside this gate.

**Approved.** Continue the separately required full task 3 and whole-branch reviews and native acceptance gates.
