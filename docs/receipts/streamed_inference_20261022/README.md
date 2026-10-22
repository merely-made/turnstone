# Streamed inference: execution qualified, answer gate failed

The runtime changes and supplier pins are pushed. All four real
Qwen2.5-1.5B frozen cases reached `finished` within their original 900-second
per-case limits. **Zero of four passed the unchanged answer contract.**
`headed_allowed` is false. Headed retained-answer and cold-reopen checks were
not run.

## Runtime changes

ESP retains verified immutable encoded layer buffers under an explicit host
capacity budget, and a first-fit subset of dense f32 layer tensors under an
explicit device payload budget. Cached layers can queue together; transient
layers drain before their weights drop. This removes repeated artifact reads,
verification, tensor conversion and upload for admitted layers. Limits are
rechecked, errors are not cached, and default settings retain uncached streaming.

The lane admits 3 GiB of host Vec capacity and 4 GiB of additional device
layer payload. The actual run retained 28 encoded layers in 2,620,715,168 bytes
of host capacity, with only 28 source misses across all four asks. It retained
22 tensor layers per ask in 4,118,208,512 bytes of device payload; six layers
continued to stream. These are logical capacity/payload counters. Shared
weights, KV, transient weights, workspace and allocator overhead are additional;
the counters are not total RSS or VRAM measurements.

The final build also optimizes the hot tensor/backend crates and SHA-256;
[the exact overrides](optimized-runtime.toml) are frozen with the run. The model
checkpoint, original f32 execution, prompt/template identity, cases, rubric,
1024-token cap and 900-second per-case deadlines were preserved. The
[measurement contract](measurement-contract.json) hashes the unchanged answer
checker and probe execution function.

## Completed frozen cases

The checkpoint is `Qwen/Qwen2.5-1.5B-Instruct` at
`989aa7980e4cf806f80c7fef2b1adb7bc71aa306`; the weight SHA-256 is
`dd924a11b4c220f385b51ffa522daea7c9f3d850e31b162bb5661df483c6d3ee`.
The [original cases](cases.json) and [premeasurement rubric](case-freeze.json)
retain SHA-256 `b91de4627aee873389066f4a8ed814fe4faff85cf1c8a3ae1d8e74f988d27ea2`.

| Case | Reported tokens | Case wall time | Decode tokens/s | Frozen answer result |
|---|---:|---:|---:|---|
| cross | 124 | 8:03 | 0.284 | Fail: Malformed JSON: later claims are outside the claims array/root object. No partial claims admitted. |
| missing | 175 | 11:35 | 0.267 | Fail: Three supported source facts replace the required single insufficient-budget statement. |
| conflict | 130 | 7:36 | 0.299 | Fail: Two single-source conflicting claims replace one disagreement claim with both sources; both are flagged for missing joint support. |
| reduced | 122 | 6:27 | 0.332 | Fail: Two in-scope source facts replace the required insufficient listener-reminder statement. The removed note was not supplied. |

Case wall time includes prefill; decode rate excludes time before the first
token. Verification/import took 157.4 seconds before the first ask. The owned
process exited 0 after about 36.4 minutes, with no timeout or model-session
mismatch. These are observed timings on a shared development Mac/Vega 56,
not a controlled attribution of speed gains to caching alone. Earlier stopped
windows used a different backend optimization profile and remain diagnostics:
[baseline](baseline-diagnostic.json), [host only](host-cache-diagnostic.json),
and [initial device cache](device-cache-diagnostic.json).

The libtest probe reports one passing execution test because it checks terminal
execution. The independent [answer gate](probe/gate.json) reports
`gate_not_passed`, and the [recorded rubric review](probe/rubric-review.json)
explains each failure. Correct quotation identity in the missing/reduced runs
did not make those answers responsive to their questions. The conflict
validator identified the missing cross-source support. The malformed cross
answer admitted zero claims. All raw outputs, support assessments, exact
passages and Unicode selectors are retained in the four `probe/*.json` case
records. `semantic_review` remains `unreviewed`; this is not general entailment
or corpus-completeness qualification.

The reduced case supplied only capture and Knot passages. Its café word comes
from the in-scope Knot source. It did not receive or return the removed note or
its harbor text. Its failure is the wrong requested fact/status, not evidence
of a note leaking through the scope cut.

## Software and consumer checks

- ESP optimized library: 189 passed, 0 failed, 1 ignored.
- Explicit native GPU walkthrough: passed all eight architecture fixtures,
  including partial caching, streaming, staged execution and cache disablement.
- Resident settings: seven checks passed, including legacy JSON compatibility
  and nonzero CPU-cache refusal before opening lanes.
- Canonical optimized resident route: four passed, three real-model fixtures
  ignored by the ordinary run. The four-case real-model probe was then invoked
  explicitly and independently gated as described above.
- Final-pin Turnstone inference tests: nine passed, one separate resident fixture
  ignored. The final canonical native build exited 0.

Build-log hashes and passing software logs are retained here. Cargo printed its
existing optional Servo tidy-fixture manifest diagnostic while discovering
packages; the final native build completed successfully and exited 0. Public
logs only remove terminal empty lines; [their provenance](public-log-provenance.json)
retains both original and published hashes. The full original logs, binaries
and diagnostic CPU samples remain outside Git under
`output/streamed-inference-20261022/`.

## Pins and qualification boundary

[Source pins](source-pins.json) identify ESP `b9f050b`, Knot `23b8f55`, Mere
`138fed73d`, and the Turnstone product pin commit `8bdfd9d`, on
`codex/cross-source-answers-20261022`. Eidetic remains `44f06647`. Canonical
metadata resolves one ESP and the active Mere integration to `138fed73d`;
the preexisting separate `mere-source-edit` leaf is preserved.

The headed harness is prepared for four historical answer reopens and one
fresh cold cross-source ask, with retained support and embedding envelope/index
checks. Those harness changes compiled, but were not executed because this
checkpoint failed its gate. The [product baseline](product-baseline.json)
records the built consumer; it is not headed or cold-reopen acceptance.
The prior `cross_source_answers_20261022` failure receipt remains unchanged.
