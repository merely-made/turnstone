# Structured answer output and conflict qualification — 2026-10-23

The decoder now enforces a bounded byte grammar. All four final raw outputs pass
whole-document structure and exact candidate-quotation shape checks, and their
retained question/prompt/context/grammar identities pass independent checks.
The complete answer gate remains **not passed**: 3/4 cases pass the frozen
semantic assertions. **Headed answer and cold reopen were not run.**

| Frozen case | Runtime state | Declared statuses | Semantic rubric |
|---|---|---|---|
| Three requested facts | finished | supported | fail |
| Absent annual budget | finished | insufficient | pass |
| Conflicting recovery phrases | finished | conflicting | pass |
| Reminder excluded from scope | finished | insufficient | pass |

The cross-source run supplied all three sources but returned only the recovery
phrase claim, omitting the access word and listener reminder. The grammar allows
1–8 claims; it does not retain an exact requested-fact plan or enforce its count.
No absent claim was synthesized or repaired into the raw model output.

The final annual-budget and removed-reminder cases both return insufficient
with empty evidence. The genuine conflict returns one disagreement claim with
both exact recovery phrases and both distinct-source quotations, choosing no
winner. Their raw outputs are preserved verbatim in `probe/*.json`; see
`result.json` for the exact final statuses, raw text and checker results. A quoted passage proves
identity, not relevance, entailment or completeness. `semantic_review` stays
`unreviewed` even when these bounded synthetic rubric checks pass.

## Implementation and scope

ESP `aaae5d19365adcfa0b0292137eddd71edcb39337` adds `OutputGrammar` to generation
requests and checkpoints. A verified ByteLevel tokenizer maps token pieces to
bytes without lossy Unicode decoding. The decoder masks tokens that violate the
grammar. Literal spans are canonically tokenized and evaluated as causal blocks;
every emitted token is committed. These syntax positions are forced by policy,
while status, free claim text and candidate choices use model logits. Seeded
replay counts forced sampling positions. CPU equivalence tests compare warm
blocks, single-token decoding and full recompute. This is a different decoding
policy, so timing is not an unconstrained-model speed comparison.

Incomplete grammar at the token limit, caller stop strings and unsupported
tokenizers fail closed. Template text stops cannot truncate constrained JSON.
The async decoder and stub refuse constrained requests. Grammar acceptance ends
the document without waiting for EOS. Replay checks token validity and text,
and the resident seals the exact grammar hash with the original question,
prompt and context. A new journal semantics marker prevents older readers from
silently dropping the policy. The checkpoint grammar must match the retained
request. V1–v3 prompt bytes remain unchanged; v4 shares v3's output grammar and
makes relevance the first decision before comparing values.

This answer grammar bounds supplied passages to eight and claims to 1–8, with
nonempty claim strings up to 384 encoded content bytes, one exact candidate per
supported claim, no evidence for insufficient claims, and two distinct supplied
sources for a conflicting claim. It cannot establish that those sources discuss
the same requested fact. The frozen cases expose that separate semantic boundary.

## Preserved evidence

`probe/` is the full final v4 cohort. `diagnostics/v3/` preserves the earlier
complete v3 cohort: four valid documents, two semantic passes, a one-claim cross
answer and a false conflict over unrelated facts in the reduced scope. Its
`manifest-description-correction.json` records inherited descriptive manifest
fields that were stale; its frozen source/binary hashes, model/session, case
hashes, `inputs.json` and results already identify the actual run correctly and
were not rewritten. The final v4 manifest was corrected before execution.

The original cases, source cuts, expected statuses, checker assertions, runtime
profile, checkpoint weights, 1,024-token cap and 900-second per-case limit remain
unchanged. Cases SHA-256:
`b91de4627aee873389066f4a8ed814fe4faff85cf1c8a3ae1d8e74f988d27ea2`.
The same Qwen2.5-1.5B-Instruct revision, original layered f32 execution, 3 GiB
host encoded-layer budget and 4 GiB additional device-layer payload budget were
used. All four final runs Finished without a timeout or model-session mismatch;
whole-cohort elapsed time was 356.152 seconds. This is not a
resident memory or total VRAM qualification.

The original verifier has a known optional-array decoder bug: the protocol
omits an empty `recall.citations`, while the checker indexes it unconditionally.
`verify_wire_defaults.py` deep-copies the decoded view and applies only the
protocol's declared `[]` default before calling the **unchanged** frozen
`verify_answer` assertions. Nonempty/null citation controls must be rejected.
Raw text, support and source files are untouched. Both original and corrected
wire-decoder results remain in the receipt. The original result is not silently
replaced, and the corrected review still fails the complete answer gate.

Recheck from this directory with a Python environment containing `blake3`:

```sh
python verify_probe.py probe                    # original checker; expected nonzero
python verify_wire_defaults.py probe             # complete semantic gate; expected nonzero
python verify_contract_inputs.py probe            # exact inputs; expected zero
python verify_structured_output.py probe           # raw shape; expected zero
```

`validation/` records 103 ESP inference, 6 prompt-template, 41 Distillery,
8 Djinn, 4 resident-route and 9 Turnstone inference passes. ESP's focused test
uses the preserved compatibility lock (`esp-tested.Cargo.lock`) and restores its
checkout lock afterward; the canonical resident and Turnstone builds use their
committed locks with no source overlays. `TMPDIR=/tmp` keeps fixture Unix sockets
within macOS's path bound. Source graphs retain one ESP/Eidetic identity and the
independent older `mere-source-edit` leaf. This is focused software coverage,
not a full workspace, CI or headed acceptance claim.

Public evidence contains synthetic text, sources, scripts, source hashes and
logs. Model weights, executable binaries, live stores and persona keys remain
outside the repository. `public-log-provenance.json` records formatting-only
terminal-LF cleanup of published logs. Prepared native scripts are present for
the gated follow-up; `native/not-run.json` is the authoritative execution status.
