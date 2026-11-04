# Retained reader recovery and readable research text

**Status (2026-11-03):** rendering and GPU shutdown repairs pass seven fresh native runs, exact-source checks and focused host verification. Research-card wheel continuation remains in progress; physical input, a confirmed smaller native window, screen-reader interaction and interactive unlocking remain untested. Historical M5 receipts retain their recorded source boundaries.

## Scope

Repair the two usability follow-ups from the model-free retained-research milestone: restored private readers should resume once custody becomes ready, and research labels, comparison rows and reader errors should remain readable. Preserve exact source identity, session and custody generation checks and ordinary navigation behavior. Historical M5 receipts continue to qualify their recorded source cohort only.

## Phases and done-conditions

1. **Reader recovery.** Trace startup restoration through identity adoption and the trail custody readiness barrier. Add regressions for visible restored readers and pending work. Defer startup or explicit reader spawns until a ready custody transition; then resume their current admitted targets; exclude closed or navigated-away readers, withdrawn sources and unrelated failures. Verify stale responses cannot publish through a newer request or custody generation.
2. **Readable text.** Trace the shared layout rules used by research suggestions and reader failures. Allow wrapping and viewport-bounded access to full text without losing controls or source identity. Verify long labels, comparison text and long unbroken identifiers at narrow widths.
3. **Integration.** Run focused regression checks, the bounded application library suite and source-size checks. Review the combined change, publish the authorized fixes, and record native verification separately from software layout tests.

## Findings

- 2026-11-01: pulled Turnstone upstream through `73565e33` before implementation. Boot starts with no identity and restores the session (`src/app/session_lifecycle/app_boot.rs`). Identity adoption alone rebuilds participants and redraws (`app_fork_session_from.rs`); the shell installs private custody and queues a separate actor readiness barrier (`src/shell/shell_start_private_connect.rs`). Recovery must respect that barrier.
- 2026-11-01: captured reader errors and research rows use single-line presentation rules. Repairs belong in shared presentation and suggestion sizing, with existing source authority unchanged.

- 2026-11-01: a retained clip address persists its manifest, while research citation selectors are held only in the session's in-memory reader map. Recovery preserves existing persisted-address semantics; it does not reconstruct absent selectors or introduce a new durable citation format.
- 2026-11-01: closing a workbench tile does not necessarily clear the node's `content_on` state. Recovery must check actual live tile ownership as well as source currentness.
- 2026-11-01: chrome overflow CSS previously had neither retained element-scroll state nor a wheel route. Wrapping requires coherent paint, hit testing, accessibility and wheel handling for tall research rows. The initial wrapping-only patch is not sufficient.

- 2026-11-01: the startup fix holds only pending reader spawns. It does not automatically reload previously live private readers after an explicit custody lock; existing retirement policy still requires reopening those readers.
- 2026-11-01: Redshank's note row used an unshrinking actions column that squeezed text. The shared supplier now places wrapping controls below the note body (`woodshed` commit `676d625`). Inspector value rows also need wrapping inside their existing scroll owner.
- 2026-11-01: a targeted Cargo update attempted unrelated Mere branch resolution and failed on a newer `chirograph` version. Updating exactly the six Woodshed source hashes preserves the package graph; locked offline metadata validates that narrow receiving change.

## Progress

- 2026-11-03: began the authorized rendering and GPU shutdown repairs after fetching current upstream. Independent Genet worktrees isolate text-paint diagnosis and completion-worker lifecycle changes. The preserved native failures remain the acceptance cases; resource caps and frame qualification are unchanged.

- 2026-11-03: pulled upstream through `4321a554` before native verification. The current intake includes extracted compute and custody clients. Built a fresh native binary and checked disposable retained-profile/custody copies under the existing resource caps. Startup recovery and exact-version comparison pass. Longer runs reproduce the five-second frame refusal; a larger real error yields blank captures; long note text clips; one run crashes during process exit while GPU destruction is active. Smaller-window requests were not reflected in actual capture dimensions. The canonical [receipt](archive_docs/receipts/2026-11-01_retained_reader_recovery_plan_receipts.md#native-window-follow-up-2026-11-03) records passing subsets, failed evidence, unchanged originals and concrete follow-up done-conditions. Production source and dependencies are unchanged by this qualification pass.

- 2026-11-01: began the two authorized follow-ups in the existing isolated qualification worktree. Rust work remains serialized under the shared machine lock, 3 GiB memory, zero scope swap, 150 percent CPU and one build/test worker. Missing locked dependency sources are being fetched without changing dependency selections.

- 2026-11-01: reader deferral, currentness pruning, disconnected-barrier failure, wrapping and scoped scrolling are implemented. Supplier surface tests pass and the supplier is published. Current Turnstone full verification is queued under the shared machine lock.

- 2026-11-01: private startup readers now show a distinct waiting-for-profile state, clear stale live presentation facts, and fail explicitly if the custody actor disconnects before readiness. All seven focused recovery cases pass. Full-run follow-ups concern the new layout fixtures; source-size and whitespace checks pass.

- 2026-11-01: all 15 selected layout and recovery cases pass, followed by the full host library suite (1423 passed, 0 failed, 21 ignored). Final review found that cancelling a closed waiting tile must also release its waiting state so a later explicit reopen can spawn. That regression and its correction are being verified separately from the passing full run.

- 2026-11-01: final recovery review resolved closed-reader cancellation, detached-pane continuity and the distinction between live content state and its save-time sidecar. Both cancellation and stale-sidecar regressions failed before their corrections. The final build and all 18 focused tests pass (10 recovery, eight layout), with source-size and whitespace checks passing. The full suite above predates these localized recovery corrections; the canonical [receipt](archive_docs/receipts/2026-11-01_retained_reader_recovery_plan_receipts.md) preserves that boundary and the failed evidence.


- 2026-11-03: the final rendering/shutdown cohort uses Netrender `6364e5d9`,
  Vello `b4a53486`, Genet `cbedf7b5` and Woodshed `e729fdad`. Seven native cases
  pass, totaling 2139 frames and 30 nonblank captures, with clean exits, exact
  retained targets, offline counters, unchanged originals and empty scopes.
  All 18 focused host checks and declared Clippy complete successfully. The
  [qualified receipt](archive_docs/receipts/2026-11-01_retained_reader_recovery_plan_receipts.md#qualified-rendering-and-shutdown-cohort-2026-11-03)
  preserves source hashes, resource bounds, failed probes and remaining limits.
- 2026-11-03: native comparison exposes a separate continuation defect: the
  height-fitted research projection omits later rows, so pixel scrolling alone
  cannot reach them. Repair the wheel route using the existing bounded row
  window after pixel scrolling reaches its boundary. Done when the real
  comparison projection redraws to later visible, pointer-targetable actions,
  while long single rows, keyboard navigation and end clamps remain correct.
