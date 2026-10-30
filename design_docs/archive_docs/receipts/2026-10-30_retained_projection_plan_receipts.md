# Retained projection plan receipts

## 2026-10-30: semantic projection reuse and settled control sync

- Source: Turnstone 22628cf9 plus commit 9b8c5fd3; Genet 5fdee500, Mere a4cbd4ba.
- Application library: 1239 passed, 0 failed, 8 ignored.
- Base: the frozen shared-frame/used-color release (sha256 1e648f12…).
  Final: optimized default build (sha256 of the frozen copy in
  `Code/output/turnstone-a11y-reuse-20261030/perf-final/binary.json`).
- Arrange, three alternating repetitions, last 120 presentations, no compiler
  overlap: base 15.18 / 14.12 / 14.42 fps (median 14.42, p95 80.5 ms);
  final 19.54 / 18.64 / 18.63 fps (median 18.64, p95 60.9 ms); peak sampled
  RSS 194 / 191 MiB. A memo-only first repetition read 13.95 vs 16.09 fps.
- Main-thread samples (3 s, Arrange settled): before, publication 59%,
  `OuterFrame::accessibility` 555, `FrameControls::sync` 291. Memo only:
  fresh projections 0, `OuterFrame::accessibility` 154, sync 455. Final:
  sync 1, publication 57%, `body_slots` 12.7%, `OuterFrame::accessibility`
  12.5%, `frame_with_guests` 9.9%.

## 2026-10-30: one surface plan per publication

- Source: commit after 41f3ba69 ("Share one surface plan across each
  accessibility publication"). Application library: 1259 passed, 0 failed,
  9 ignored.
- Arrange, three alternating repetitions (attempts 81–83 after two runs
  rejected for compiler overlap), last 120 presentations: prior final
  20.46 / 19.87 / 19.43 fps (median 19.87, p95 57.2 ms); plan sharing
  20.25 / 19.99 / 20.29 fps (median 20.25, p95 53.8 ms), +1.9%. Peak sampled
  RSS 199 / 206 MiB. Frozen bundles: `perf-final`, `perf-plan` in
  `Code/output/turnstone-a11y-reuse-20261030`.
