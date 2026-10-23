# Retained requested-fact qualification — 2026-10-23

The retained fact-set implementation is pushed. **The real-model gate did not pass; headed answer and source-absent cold-reopen checks were not run.**

The 81 focused software checks passed: Eidetic models 18, resident inference 42, Djinn inference 8, resident routes 4, and Turnstone inference 9. They cover Unicode ranges, exact fact identity, refusal of missing/extra/reordered bindings, sealed plan storage before generation, replay and lock boundaries, and historical contracts. These software checks do not qualify a completed Qwen answer.

[CONTRACT.md](CONTRACT.md) describes the bounded question notation, explicit-span Rust API, fixed claim slots, and retained question-to-fact-to-passage support. [summary.json](summary.json) records the gate and supplier commits. Both resident and Turnstone source graphs contain one canonical Eidetic revision.

The same four frozen cases, source cuts, original semantic checkers, Qwen2.5-1.5B weights/template, 1,024-token cap, 900-second per-case deadline, 3 GiB host cache and 4 GiB device layer budget were retained. The frozen probe function is byte-identical; the surrounding native snapshot helper additionally exports sealed requested facts. [measurement-contract.json](measurement-contract.json) records hashes before measurement.

| Frozen case | State | Tokens | Prescribed claims |
| --- | --- | ---: | ---: |
| cross | failed | 106 | 3 |
| missing | failed | 125 | 1 |
| conflict | failed | 228 | 1 |
| reduced | finished | 112 | 1 |

The reduced-scope case passed raw shape and the unchanged semantic assertions under the declared omitted-citations wire default. The other three cases failed before producing complete answers. Input and retained-plan binding passed 4/4.

The cohort took 714.787 seconds. No session mismatch or deadline timeout occurred. The child exit was 101; the failed gate remains a failure. Failed or partial JSON is not a completed semantic result. Every case was attempted; no output was repaired or replaced.

The decoder reported `output grammar has no finite eligible token`. [diagnostics/](diagnostics/) retains the exact cross-source failed prefix, its independently reconstructed grammar, production ESP grammar source, replay source, and executed result. Replaying that byte state with finite synthetic zero scores admits exactly the handle tokens `1`, `2`, and `3`. This establishes a valid grammar continuation. It does not recover the live score row or establish why those eligible scores were non-finite. The previous and new probe compiler/profile fingerprints match. The score-validity boundary and forced-block inference path are the next useful diagnostic cues; a prompt change alone would not resolve the executed failure.

[candidate-01/](candidate-01/) contains raw final and partial records, progress, process result, original gate, independent input/plan review, declared wire-default semantic review and raw-shape review. The original checkers are unchanged. As in the prior receipt, the separate wire-default decoder supplies an omitted empty `citations` array only; it does not change model text or support. [checks/](checks/) contains build and test logs. Terminal empty log lines are removed for Git whitespace checks; [log-provenance.json](log-provenance.json) preserves original and published hashes. Raw JSON is unchanged.

The native harness and sealed-plan checker are prepared evidence machinery only. No native binary was built for this revision, no desktop window was acquired, and no headed or cold-reopen success is claimed. Model weights, compiled executables, keys and live profiles are excluded.

All eight remote source branch heads were read back with `git ls-remote` and matched the published pins before the receipt commit. The primary Turnstone checkout was unchanged. Primary Mere advanced concurrently from `d918f670c8328ac6544242b0b753992b5124df09` to `50afede99966eeb5859ff332931d6b0004813d67` and remained clean; qualification used the isolated, fixed `487b6648211800a613db39891bd7691b157b0d7a` supplier. Before/after snapshots are retained.

`verify_receipt.py --hashes` rechecks the archive, the unchanged frozen hashes, successful software logs, and the recorded failed gates without starting inference. The verifier succeeding means the receipt is internally consistent; the model gate remains failed.
