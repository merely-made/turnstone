# Turnstone lane handoff to q-pc, 2026-10-14

**Status, 2026-10-14:** the Turnstone lane moves from the ThinkPad to q-pc,
Mark's Mac. Windows-only work runs on o-pc, the ASUS Windows laptop. Remote `main` is the only live line: every branch and worktree on
the ThinkPad was reconciled into it, archived as a tag, or removed. Start from
a fresh pull; nothing needed is left behind.

## Start here

1. Pull `main` in `~/Code/repos/turnstone` on q-pc. On o-pc, the checkout is
   `C:/Users/mark_/Code/repos/turnstone`; check `git log origin/main..HEAD`
   and `git status` there first. The 2026-10-08 handoff recorded unpushed
   administrative commits and untracked `.github/` WIP on that checkout;
   reconcile them against current main before building, and commit only
   paths owned by the current slice.
2. Read [the index](DOC_README.md) and [the policy](DOC_POLICY.md). Its working
   principles hold: work directly on `main`, avoid branches, PRs and worktrees
   unless isolation is genuinely needed, and merge and prune any temporary
   lane before leaving it. Mark does not use pull requests, and commits carry
   no `Co-Authored-By` trailer.
3. Build per the [README](../README.md). On o-pc that needs MSVC tools,
   Windows SDK, CMake and Ninja (`CEF_PATH` reuses a matching distribution);
   approved Windows targets live under `C:/t/cargo-targets/<repo>`.

## Where main stands

- Pins: Mere `77fd235fe659c513dd0463ed57be32f02fb2e71a` (134 declarations) and
  Genet `b6bfbeb556a6a17eddf8d5ad8254d3fcac95b084` (32), Knot
  `8ed49d43f049921d56ab5a0c091a2c37c0970ee7`. Toolchain 1.99.0, MSRV 1.97.1.
- Latest published work: retained pane composition and native image
  apertures inside the shared window scene (`1aa6663`), with its exact receipt
  bytes (`c7a828e`). See the [pane registry plan](2026-08-08_pane_registry_and_graph_panes_plan.md)
  and `docs/receipts/stations_windows_20261013/published-final-native-qualification.json`.
- The lens accessibility-label fix landed with this handoff. Before it,
  `project_lens_with_contributions` labelled every lens window
  `Turnstone — lens` while the shell's bridge and actual windows use
  `Turnstone lens {ordinal}`; the original-five runtime run failed on exactly
  that assertion. The fix shares `a11y::lens_window_label` and adds
  `lens_window_tests`. On the ThinkPad against current main, the new test
  and all 59 `a11y` library tests pass (default features). It had been stranded on the ThinkPad's three-lanes
  branch after main moved past it; see
  [the qualification note](2026-10-13_current_consumer_qualification.md).
- Genet main has since gained `ValueEngine` (`f0a91f7`), an additive
  `script-engine-api` module for Mere's Scenograph rule runner. Turnstone needs
  no repin for it; take it with the next coordinated family repin.

## Open gates, in rough order

1. **Changed-source qualification after the lens fix.** The final App
   producer, the original five native compositions and the three
   station/lens-accessibility scenes are pending against the fixed source
   ([qualification note](2026-10-13_current_consumer_qualification.md)).
   Default and Piccolo native suites last passed 927/0/10 and 960/0/10 on the
   pre-fix source. On q-pc against `b0b709a`, the lib suites gave 938/2/10
   default and 970/3/10 Piccolo; all 59 `a11y` tests pass, and the failures
   are `place::` timing tests that pass alone. All 14 scenarios of the ten
   published cases ended `RESULT ok` with no blank captures. Evidence is
   outside the repo at q-pc `~/Code/testing/turnstone/macos-qualification-20261014`.
   The Windows App producer and Windows native runs remain for o-pc.
2. **Separate platform scopes** that no automated pass closes: physical GPU
   device-loss recovery, Windows producer rebinding, OS accessibility
   activation, human AT, human cross-window focus, monitor DPI transitions and
   early scrollbar-fade pixel timing. o-pc is the natural place for the
   Windows ones.
3. **Known UI issues** from the last visual review: floating-toolbar
   occlusion and confirmation-caption overflow. Recorded, not fixed.
4. **Resource-graph done-conditions** in the
   [2026-10-08 handoff](2026-10-08_browser_resource_handoff.md#remaining-done-conditions):
   per-view control migration, qualified placement through Canvas
   runtime/session/fork/save, behavior-journal attribution to the originating
   graph runtime. Several were applied after that list was written; verify
   each against the live tree before treating it as open.
5. **Plan ledger drift.** The [unusual-protocols plan](2026-10-06_unusual_protocols_browser_plan.md)
   still describes SC5/SC6 as open preparations although much is implemented;
   reconcile implemented versus qualified scope. SC7 store traits remain
   broader existing-plan work.
6. **Separate product work**, not integration: a general source-view
   consumer and named collections (taxonomy U2/U4), and the clip-system
   contracts the family thesis leaves unresolved (retention policy, first
   compound-clip profile, cross-domain edit/undo).

## What stays on the ThinkPad

- Evidence for the October 11–13 integration runs lives outside the repo at
  `/home/markik/Code/testing/turnstone/integration-completion-20261011`
  (94 GB; `progress.md` and `gap-audit.md` are the readable summaries). SSH
  alias `thinkpad-l14-f`, user `markik`; fall back to
  `-o HostName=thinkpad-l14-f.local` if the bare name stops resolving.
- **Mere's half of the three-lanes lane is unpublished there**:
  `/home/markik/Code/worktrees/mere-three-lanes` on
  `codex/three-lanes-upstream-20261010` holds two commits not on Mere main
  (`17f71d6` tenant-owned browser presentation and viewer acceptance,
  `abab46f` integration checkpoint) plus uncommitted Graphshell edits
  (`host_dataset_view.rs`, web `Cargo.lock`, `scenario-inventory.json`). That
  is Mere's to reconcile; Turnstone should not repin to its merge `71a1bbe`.

## Cleanup done on 2026-10-14

- Ported the lens-label fix to main (above).
- Removed worktrees `turnstone-graph-adoption` (merged) and
  `turnstone-resource-compat` (three-lanes; its obsolete repin to `71a1bbe`
  is kept in tag `archive/codex-three-lanes-upstream-20261010`).
- Deleted local `graph-semantics-adoption` (patch-equivalent on main) and
  `graph-semantics-adoption-integration` (merged), plus the remote copy.
- Deleted remote `codex/knot-editor-consumer-20260901`; main consumes
  knot-editor at a newer revision. Tag
  `archive/codex-knot-editor-consumer-20260901` keeps it.
- Removed the ThinkPad's untracked `thinkpad_checkout_sync_20261008`
  preservation copies after verifying all 56 files byte-identical to main.
