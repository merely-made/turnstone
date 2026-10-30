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
