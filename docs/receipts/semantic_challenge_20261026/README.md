# Semantic challenge, 2026-10-26

The broader semantic qualification gate is **gate_not_passed**. Of fifteen admitted model requests, **8 passed** the frozen semantic rubric and **7 failed**; 0 were incomplete. A separate sixteenth case exercised the expected oversized-conflict admission refusal and is not counted as a semantic answer success.

This extends the four-case [quoted-review baseline](../quoted_review_20261024/README.md) with paraphrases, entity/time/certainty qualifiers, repeated agreement versus conflicting values, and longer passages. All gold labels were frozen before measuring. No model, prompt, grammar, sampling, token limit, or timeout was tuned on the challenge. The first whole-suite checker run failed because it treated session metadata as a scalar. [verification-amendment.json](qualification/verification-amendment.json) records the exact two replacements that compare the existing session object correctly; the original checker and abbreviated error transcription are preserved. Gold labels and semantic logic are unchanged.

| Cohort | Semantic requests | Passed | Failed/incomplete | Expected admission refusals observed |
|---|---:|---:|---:|---:|
| paraphrases | 4 | 2 | 2 | 0 |
| qualifiers | 4 | 2 | 2 | 0 |
| agreement | 4 | 2 | 2 | 0 |
| long_passages | 3 | 2 | 1 | 0 |
| long_conflict_admission | 0 | 0 | 0 | 1 |

## What the measurement establishes

All 15 completed raw documents passed whole-document JSON, fixed requested-fact count/order, exact candidate-review and selected-quotation shape, retained-support consistency, and citation-selector checks. All 15 input contracts matched independently derived question facts, exact v7 instructions, context, and output-grammar hashes. These are structural and provenance checks; semantic correctness is assessed separately against the frozen gold.

Numerical telemetry joined all 15 generation requests: 0 invalid score rows and 0 prefix recomputations. The external rubric passed 61 synthetic positive/mutation checks, including rejection of two identical quotations from distinct sources as a claimed conflict. Rubric checks are not model successes or product tests.

## Observed failures

- `paraphrases/cross`, fact 1: expected **supported** / `single`, observed **conflicting** / `different`. candidate_relevance, agreement, status, unsupported_selected_source.
- `paraphrases/cross`, fact 2: expected **supported** / `single`, observed **conflicting** / `different`. candidate_relevance, agreement, status, unsupported_selected_source.
- `paraphrases/cross`, fact 3: expected **supported** / `single`, observed **conflicting** / `different`. candidate_relevance, agreement, status, unsupported_selected_source.
- `paraphrases/conflict`, fact 1: expected **conflicting** / `different`, observed **supported** / `single`. candidate_relevance, agreement, status, conflict_does_not_show_distinct_values.
- `qualifiers/cross`, fact 1: expected **supported** / `single`, observed **insufficient** / `none`. candidate_relevance, agreement, status, unsupported_selected_source.
- `qualifiers/reduced`, fact 1: expected **insufficient** / `none`, observed **supported** / `single`. candidate_relevance, agreement, status, insufficient_has_links.
- `agreement/cross`, fact 1: expected **supported** / `same`, observed **conflicting** / `different`. agreement, status, unsupported_selected_source.
- `agreement/conflict`, fact 1: expected **conflicting** / `different`, observed **supported** / `single`. candidate_relevance, agreement, status, conflict_does_not_show_distinct_values.
- `long_passages/reduced`, fact 1: expected **insufficient** / `none`, observed **supported** / `single`. candidate_relevance, agreement, status, insufficient_has_links.

The failure details include every supplied candidate's relevance decision, agreement, status and selected source keys in [summary.json](summary.json). A truthful quotation and an exact source link alone do not establish that the passage answers the requested property. Retained support remains `semantic_review: unreviewed` throughout this measurement.

## Frozen execution envelope

- Qwen/Qwen2.5-1.5B-Instruct revision `989aa7980e4cf806f80c7fef2b1adb7bc71aa306`, original f32 streamed execution, AMD Radeon Pro Vega 56 / Metal.
- Generation session `blake3:0960d64d68cf80b35a5781e3daeddfcf453f30579861e02af8d93a535db58d3d`.
- Existing resident probe SHA256 `699c8ac6c911e3b7e0cf017cd300bb238e93102501168629d4bf9703a9130466`; APFS clone verified before measurement. No Cargo compilation was launched.
- `distillery.quoted-review/v7`; 1024 output tokens and 900 seconds per request; 3 GiB host weight cache and 4 GiB device-layer payload budget.
- Lexical recall, maximum four passages, explicit scopes with two to four synthetic PageText sources. Source roles reuse the existing generic probe's capture/control/knot/note keys; this run does not exercise native producer ingestion.
- 1028.315 seconds total elapsed, including separate model/resident loading for each cohort. The probe's temporary stores are disposable synthetic fixtures; raw run documents, logs, progress, process outcomes and frozen inputs are retained here and in `Code/output/semantic-challenge-20261026`.

The longer-passage conflict case exceeds the existing 384-byte extractive conflict-text bound. Admission returned the precise refusal before generation. The generic test probe unwraps that result and exits nonzero; this is expected admission evidence, not a qualified application error screen or an application-crash finding.

## Qualification boundary

This is one deterministic synthetic lexical measurement on one model/backend. It does not establish broad semantic reliability, fused-recall results, new producer retention, headed behavior, or cold-reopen correctness. The prior four-case native receipt retains its original boundary. Because this broader gate did not pass, no new headed or cold-reopen cohort was run.

The exposed work is semantic relevance and agreement: handling paraphrases and qualifiers, comparing all relevant values, and selecting quotations that show the actual disagreement. No production code was changed by this qualification.

## Reproduce the receipt checks

Run `python3 verify_receipt.py` with Python's `blake3` package available. It verifies all included hashes, the original frozen question/gold/source bytes, raw contract/output consistency, telemetry joins, and the recorded semantic results, including failures. It does not rerun inference. The model and executable remain in the retained local evidence lane and are intentionally excluded from this public package.
