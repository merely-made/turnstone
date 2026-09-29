# Shared diagnostics adoption and Gloss / Inspector receipt

Date: 2026-09-29.
Status: sealed dependency graph resolves; focused diagnostics, UI and participant
tests, the normal native build and final initial/restarted pane scenarios pass.
The full workspace gate passes. Assistive-technology acceptance is open.

This records the consumer graph and evidence boundaries for the
[current Gloss / Inspector direction](2026-07-20_gloss_composite_pane.md#current-direction-2026-09-29).
The shared observation contract remains in
`mere/design_docs/mere_docs/implementation_strategy/2026-06-08_system_diagnostics_and_accessibility_plan.md`;
this document records Turnstone's application and acceptance evidence.

## Published dependency graph

| Owner | Sealed revision |
| --- | --- |
| Mere A | `ca2351b3fa21de1ca40a8553eed20fc503faf15c` |
| Genet | `19c206873ab08ae227217892d9e74d0df18b349a` |
| Knot K | `c92ad044103f8b3a97757a406e003c1b5381ac8d` |
| Woodshed / Redshank W | `752c920e713fa511d6e5d73385c57522d7a03233` |
| Netrender | `9607d16f1907f6c2085648ae96abcaa30d7c3d41` |

The root sweeps dependencies and registry patches onto these sealed sources.
Knot K and Redshank W consume Mere A, preserving retained-surface, Endpoint and
Fleece type identity. Acceptance uses the root Git graph and lock without
caller-local paths. Locked Windows-filtered metadata and a separate lock audit
find one source for each of fourteen shared seams. The lock has Mere 68, Genet 31,
Knot 4, Woodshed 6 and Netrender 4 packages; optional packages need not appear in
the default Windows-resolved graph. Registry changes add Ciborium's three 0.2.2
packages and move netrender-vello 0.10.0 to 0.10.1, with other existing registry
versions retained. Full all-platform offline metadata needs an unused Linux
ALSA package; the Windows-filtered gate avoids asserting that package is cached.

The source consolidations retain existing behavior. The separate Eidetic Fjall
dependency becomes `eidetic` with its `fjall` feature and
`eidetic::fjall::FjallStore`, preserving session storage paths. Endpoint's `local`
and `stdio` features supply Knot's local carrier and the endpoint binary through
`graphshell-endpoint`. Product storage and transport authority remain local.

The sealed settings API now holds an optional Tabard `ThemeChoice`, flattened
to the existing `theme_id` / `theme_mode` keys. Turnstone retains its setting
IDs and edits each value independently. Its product read adapter preserves old
mode-only records: an empty typed ID carries the mode and projects to an unset
chrome ID. This local representation is not an owner-wide compatibility claim.
Prepared regressions cover legacy load, unrelated saves and clearing an ID while
retaining the mode. Journal fixtures also follow typed kind/id/version/route
attribution; existing behavior string protocols retain the author's raw ID.

## Product diagnostic copy

[DiagnosticObservations](../src/diagnostic_observations.rs) copies fixed categories
at the AppEvent fanout, omitting URLs, titles, paths, prompts, credentials, hashes
and raw errors. Unclassified events become `other-app-event`; metadata supplies
no inferred operation, subject, worker timing or cause. Trail retains its authority.

Retention is configured per run by `TURNSTONE_DIAGNOSTIC_RECORDS`,
`TURNSTONE_DIAGNOSTIC_BYTES` and `TURNSTONE_DIAGNOSTIC_AGE_SECS`. Defaults are
128 records, 65,536 accounted bytes and 300 seconds. Any zero disables retention;
invalid settings make the requested receipt fail. The category's payload bytes
come from its actual JSON encoding, plus Apparatus envelope accounting. These
are store admission bytes, not a pretty-printed export or allocator size cap.
Fresh run UUIDs, elapsed `Instant` receipt times and independent cursors preserve
records, sequence gaps and cumulative loss.

The existing Taproot driver explicitly exports `diagnostics.json` beside
`scenario.done`; failed exports fail the sentinel. Ordinary runs do not export.
Scenario text, trace logs and screenshots retain their own privacy boundaries.

## Acceptance ledger

| Gate | Current evidence |
| --- | --- |
| Knot workspace at base `8a454fb7` with A / Genet pins | Passed 425 tests, 2 existing ignores |
| Knot after preserved Nomadnet merge `701d0f0b` and narrow cfg fixes | Desktop passed 210 tests, 1 ignore; document passed 34, 1 ignore; strict desktop clippy and optional Retinue check passed |
| Turnstone locked source graph | Windows-filtered locked/offline metadata and fourteen single-source seams pass |
| Turnstone diagnostic copy | Six focused tests pass, including redaction, accounting, disabled settings and export failure |
| Turnstone workspace tests | Final locked/offline workspace/all-target gate passes: 612 tests, 9 ignored; four other target harnesses run zero tests |
| Turnstone UI regression gate | 33 tests pass on the final source with Piccolo/Wasm enabled, including intact review wrapping, accessibility label and real click, plus migrated Inspector controls |
| Turnstone participant attribution gate | Five exact Piccolo/Wasm tests pass: script attribution, prior-author restoration, watch writes/wake and admitted component rings |
| Turnstone normal native binary | Final build passes on Rust 1.98.1, with default product features |
| Turnstone initial and restored native migration | Both final scenarios pass; all three 1024×600 captures inspected and nonblank |
| Turnstone native diagnostic loss wrapper | Initial retains sequences 9–10, 266 accounted bytes, 8 evictions and gap `[1,9)`; restart retains 1 record without loss. Both save Downloads, Recent in order plus Inspector |
| Physical assistive-technology pass | Not performed for this slice |

Knot's older native presentation captures retain their original pins. They do
not qualify the newly sealed consumer graph. Knot's detailed evidence lives at
`Code/testing/knot-editor/apparatus-adoption/receipt.md`.

The wrapper and evidence live at `Code/testing/turnstone/gloss-inspector/`.
The local initial scenario configures Gloss, activates Inspector's followed-member
viewer and saves; its twin restores the exact profile. Prepared persistence tests
cover legacy frame/lens conversion. The wrapper records binary/source/manifest/
lock/scenario hashes, enforces two-record retention and requires initial eviction
plus a gap. Missing counters/arrays or requested PNGs, retention overflow,
incorrect saved section order, failed sentinels and unsuccessful exits fail
acceptance. Baseline captures are preserved under `initial/` and `restored/`;
the final styled source is qualified under `final/initial/` and `final/restored/`.
The Inspector clip control now uses readable pane styles, wraps long labels and
reports unavailable state; its product text does not expose an environment setting.
The viewer assertion proves the requested override, not completed Reader rendering.

Final native binary SHA-256:
`D486EA8B7E66CEBC6DAA94A691E4CADB6DC516DA999AC762BCB0D0D4E6BD47B6`.
Lock SHA-256:
`A527EAFA3B6D0C885BF3CC6A697398C3217B15E4D86712C7F70B454050BAF690`.
Each launch records every source-file hash, manifest and scenario hashes. All
139 recorded source hashes still matched after the final captures. Capture
hashes, retained frame sidecars and diagnostic summary are saved alongside them.
These focused gates overlap full-workspace coverage and must not be summed as
unique test counts. Turnstone strict Clippy is not claimed: existing warnings
remain outside this migration.

Commands use the stable `C:/t/cargo-targets/turnstone`, two Cargo jobs and zero
dev/test debug level. The final workspace command is
`cargo +1.98.1 test --workspace --all-targets --locked --offline -- --test-threads=2`;
it finishes successfully with all nine Redshank guarded-host tests included.
The default native command is
`cargo +1.98.1 build -p turnstone --bin turnstone --locked --offline`.
Native wrappers use `-Run initial -Phase final` and then
`-Run restored -Phase final`. The earlier parallel log has 607 passing tests,
five failures and nine ignores; the completed serial log has 611 passing tests,
the one pre-caret card failure and nine ignores. These failed runs are not green
receipts and remain archived as failure provenance.

The first parallel failures exposed a guessed byte-quota fixture, a delegation
fixture that sampled the child's not-before time before founding its parent,
the actual Chrome card budget mismatch, and two elapsed setup/receive failures.
The byte test now measures the real admitted record footprint. The invitation
fixture samples after founding; issuer, scope, depth and expiry checks stay
unchanged. Both timing cases pass serially with their original budgets. Chrome
now measures its real padded/bordered card and shared caret projection, preserving
intact text, accessible label and click assertions. The serial run completed with
only the earlier card probe failure, before it included the caret's metrics. Failed and
serial logs remain preserved separately from the final gate.

Background work and sync controls, real worker causality, within-document
selection inspection, the separate TLS/download fixture, and broader contributed
pane semantic automation retain their own implementation and acceptance gates.
