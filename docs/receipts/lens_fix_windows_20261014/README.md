# Windows changed-source qualification after the lens-label fix, 2026-10-14

**Qualified source:** `b0b709a5` (lens accessibility-label fix `1a0bf3d1`), run
from `f14d2091`, whose only difference from `b0b709a5` is `design_docs/`.
Machine o-pc, Windows 11, toolchain 1.99.0, pins Mere `77fd235`, Genet
`b6bfbeb`, netrender `6a24ae0`, no local path override. Target
`C:/t/cargo-targets/turnstone` (cold). This is the Windows half; the macOS
half passed on q-pc against the same source.

## Results

| Gate | Result | Pre-fix reference |
|---|---|---|
| App producer, `cargo +1.99.0 build --bin turnstone --locked --offline -j1` | pass, 11m00s, exe `2ef88841…` | exe `8a129e07…` |
| Eight scripted native lib gates | 17/17 pass, 0 failed | 17/17 |
| Full lib, default | **944 / 0 / 10** | 927 / 0 / 10 |
| Full lib, `--features piccolo` | **977 / 0 / 10** | 960 / 0 / 10 |
| Ten published cases (14 scenarios) | 14/14 `RESULT ok`, 40 captures, 0 blank, 1,142 frames | 14/14, 40, 1,141 frames |
| Inferred original five | 5/5 `RESULT ok`, 11 captures, 0 blank | attempt2 failed at Lens |
| Raw/final pixel review (a4-scoped, alpha) | 4 + 4 pairs exact in scope | same |

`a11y::lens_window_tests` passes in both suites; no `place::` timing test failed,
so no re-runs were needed. The +17 over the pre-fix full-suite counts belongs to
the pre-fix baseline's older source, not to this run; per-gate filtered totals
show the lib grew from 953 to 954 tests between `1aa6663` and this source, which
is the new lens identity test.

## Notes

- **Receipt location (Mark, 2026-10-14):** a new dated folder, so the pre-fix
  `stations_windows_20261013` receipts are untouched. `run-published-gates.ps1`
  and `run-native-scenario.ps1` are verbatim copies; they resolve the repo as
  `$PSScriptRoot/../../..`. Build jobs are `-j1` exactly as scripted (Mark).
- **Build attempt 1** stopped after 653 crates: another session's cargo started
  at 23:13:03 and cargo's automatic cache GC removed
  `~/.cargo/git/checkouts/smolweb-…/882baeb` mid-build, so rustc could not start
  (`os error 267`). No source involvement; attempt 2 ran unchanged. Log kept as
  `published-native-build-attempt1-checkout-removed.log`.
- **Lock hash.** `published-native-executable.json` records `99a14fc3…` because
  this checkout has `core.autocrlf=true`; the committed LF blob hashes to
  `d418c6ca…`, the pre-fix receipt's lock. Content is identical.
- **Unused-patch warnings** (eight) and the servo tidy-fixture parse message
  match the pre-fix published build log line for line.
- **Original five.** *Reading, not ruled:* no doc names them. Identified as
  `document_a11y_gemini`, `_gopher`, `_micron`, `_lens` and `reader_appearance`:
  eleven captures, `document_a11y_lens` asserts `window: Turnstone lens 0` (the
  label the fix changed) and `serve.py` serves Reader on the port
  `reader_appearance` opens. Mark ruled to run them labelled as inferred. Each
  ran in its own case against `scenarios/fixtures/document_a11y/serve.py` with a
  throwaway local self-signed certificate (not committed); requests are in
  `original-five-fixture-server.log`. `run-original-five.ps1` and
  `run-published-ten.ps1` are the drivers as run; the latter's summary line
  prints `System.Object[]`, which is cosmetic; the runner's
  `*.execution.json` records are authoritative.
- **Run-local state** (`profile/`, `themes.json`) is omitted, as in the pre-fix
  receipt.

## Visual review

All 51 target captures were inspected whole. Against the pre-fix run, 26 of 40
published captures are byte-identical or differ by 1–3 pixels at delta 1. The
rest (a5, alpha, fonts/missing-font reopen) differ only in graph-node placement:
panes, text, chrome and lens content are pixel-identical. Rough edges, all
pre-existing or already recorded as gate-3 UI issues: graph nodes clipped at
the left edge; the floating roster cuts its last row; 2360×1600 frames draw
content small at the top left; narrow-font title truncates; Micron confirmation
caption overflows its box; the Reader inset's source pill overlaps article text;
the floating "workbench" title is occluded; Inspector lines clip at the right.

## Still open

Physical OS accessibility activation, human assistive technology, human
cross-window focus, monitor DPI transitions, early scrollbar-fade pixel timing,
GPU device-loss recovery and Windows producer rebinding.

Files: `final-native-qualification.json` (all 19 scenarios, capture and artifact
hashes), `lib-suites.json`, `published-native-*.json`, logs, `native/fixed-*`.
