# Bounded answer contract: software checks pass, answer gate remains closed

The supplier implementation and canonical Turnstone pin are complete. The final
Qwen2.5-1.5B run attempted all four original frozen cases: three finished and one
hit its original 900-second deadline. **The answer gate did not pass. Headed
answer and cold-reopen checks were not run.**

## Implementation and source cut

Distillery now prepares an explicit question-driven task: one claim per requested
fact, in order, with supplied evidence, insufficiency, or a single jointly
supported disagreement. Its two compact teaching examples use unrelated facts
and nonsequential evidence handles. Example text never enters the supplied
passage set. The prompt requests single-line minified JSON; model compliance is
measured rather than assumed.

`RecallEvidence.answer_contract` retains the task method, exact question,
prompt and context hashes, and an enforced eight-claim bound. The active
`distillery.requested-facts/v2` prompt has 1,416 instruction characters and a
4,096-character complete-prompt bound. The published v1 instruction bytes and
16,384-character bound remain available for replay. Assessments check the exact
question/evidence binding. The sealed journal also checks actual generation
inputs and marks the required checkpoint semantics. Legacy prose keeps its
previous wire shape. Eidetic's claim-to-passage artifact continues to report
semantic review as unreviewed; these bindings do not establish entailment.

Mere `5262ae5775344741a76545ba895a8dbf33256409` and Turnstone product commit
`e0fa597dd29f2d8722818dd560eca57995bf250c` identify the implementation. Turnstone's
lock changes are exactly 62 active Mere supplier replacements, plus one explicit
`None` in an existing client test fixture. Canonical metadata resolves one ESP
`b9f050b` and Eidetic `44f0664`. Knot stays at `23b8f55`; the separate existing
`mere-source-edit` leaf stays at `70deef2c8`. [Source pins](source-pins.json) and
[the built product baseline](product-baseline.json) retain exact identities.

## Final frozen cases

The cases, questions, source cuts, checkpoint/template, rubric and original
verifiers remain unchanged. Cases SHA-256 is
`b91de4627aee873389066f4a8ed814fe4faff85cf1c8a3ae1d8e74f988d27ea2`.
The 1,024-token cap, 900-second per-case deadline, optimized runtime profile,
3 GiB host cache budget and 4 GiB additional device-layer payload budget are
unchanged. The latter counters are not total physical memory measurements.

| Case | Tokens | Case time | Result |
|---|---:|---:|---|
| Cross-source facts | 122 | 15:07 including cancellation | Incomplete. Hit the 900-second generation deadline and cancelled cleanly. Partial output admits no answer support. |
| Absent annual budget | 29 | 3:32 | Pass under the declared protocol default described below. One insufficient-budget statement, no invented value, evidence or citations. |
| Conflicting recovery phrases | 48 | 4:26 | Fail. Parseable JSON chooses a supported single-source winner and adds an unsupported top-level `conflicting` key. Strict document validation admits zero claims. |
| Listener note removed from scope | 29 | 2:29 | Fail. Correct insufficiency wording is inside an unfinished code fence followed by a stray quote. Whole-document parsing fails; zero claims are admitted. |

The final process exited 101 because the cross-source case was incomplete,
after approximately 28.6 minutes including setup. Model/session identity matched
the previous baseline. Raw documents, exact supplied passages, task metadata,
timeout and all terminal states are retained in [probe/](probe/). The owned model
process exited and its recorded temporary source root disappeared. No native
application was launched for this qualification.

The removed note was not supplied in the reduced scope, and its reminder text
was not returned. That negative source-cut observation does not turn the
malformed output into an accepted answer. [The manual rubric review](probe/rubric-review.json)
records the distinction. [Metrics](probe/metrics.json) include prefill and observed
decode rates. This shared-Mac run is not a controlled attribution of runtime
performance to prompt size, caching, or other applications.

## Frozen checker and protocol-default review

The original checker reports zero passing cases. It rejects the three substantive
failures above and also raises `KeyError: citations` on the correct budget answer.
`RecallEvidence` declares an omitted empty citations array as `[]` using serde's
`default` and `skip_serializing_if`; the original checker indexes that optional
field directly. It had not reached that assertion in the earlier unsuccessful
budget result.

The original checker and [its result](probe/gate.json) are preserved unchanged.
[The supplemental wire-default review](probe/wire-default-review.json) applies
only that declared empty-array default to a decoded copy before calling the
unchanged rubric. It changes no raw files, model text or support. It rejects
both a nonempty citations array and `null` in negative controls. It records
**one pass, two failures and one incomplete case**; headed acceptance remains
blocked under both reviews.

The original additional binding checker also cannot complete on the cancelled
case because no assessed support exists. [The separate input-contract review](probe/contract-input-review.json)
checks all four retained question/prompt/context hashes, source cuts and admitted
claim bounds, including the cancelled run. It passes input identity only and
cannot grant answer or cold-reopen acceptance.

The public records can be rechecked without model weights or binaries:

```sh
python verify_probe.py probe
python verify_wire_defaults.py probe
python verify_contract_inputs.py probe
```

The first two commands deliberately return nonzero for this failed cohort. The
review scripts require `blake3`. The small text-only frozen inputs are included
under `probe/frozen`; executable/model payloads are excluded.

## Software checks and diagnostics

- Final Distillery inference tests: 40 passed.
- Final Djinn inference tests: 8 passed.
- Final ordinary resident-route checks: 4 passed, 3 real-model fixtures ignored.
  The real four-case probe was invoked separately and failed as reported above.
- Final Turnstone inference tests: 9 passed, 1 separate resident fixture ignored.
- Canonical Turnstone native build: passed. Its existing optional Servo
  tidy-fixture manifest diagnostic was nonfatal; Cargo finished and exited 0.

These are 61 focused passing unit/route/consumer checks, separate from the failed
real-model qualification. The sealed-journal check covers task retention,
reopen, lock/unlock and rejection of altered generation inputs. Tests also cover
question/evidence substitution, example-source rejection, claim bounds and v1
replay. Published logs remove only excess terminal newline bytes; [log provenance](public-log-provenance.json) records original and published hashes, and the original logs remain in the private qualification output.
The native build and focused test logs are retained here; these are local
checks, not a full-workspace or CI claim.

The initial 3,419-character v1 prompt attempt was deliberately stopped while
incomplete after it emitted formatted JSON at about 0.14 tokens/second. Its raw
partial, stop reason and frozen prompt remain in
[diagnostics/expanded-prompt/](diagnostics/expanded-prompt/); no semantic result is
claimed for that attempt. The compact final run above was allowed to execute all
four cases under their unchanged limits.

The first ordinary route invocation used macOS's long default temp directory,
producing a 117-byte Unix socket name and two admission failures. An isolated
short-temp control passed, followed by complete passing runs with `TMPDIR=/tmp`
(73-byte socket name). Failed and passing evidence remains in
[diagnostics/](diagnostics/). The model launcher uses the same short-temp setting.

The implementation is published on `codex/cross-source-answers-20261022`; this task did not merge a main
branch. The remaining observed gaps are multi-fact completion
within the frozen deadline, valid whole-document emission, and jointly supported
conflict handling. The headed harness is prepared but unexecuted.
