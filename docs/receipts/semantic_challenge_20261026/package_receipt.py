from pathlib import Path
import collections,hashlib,json,shutil,subprocess
O=Path(__file__).resolve().parent
T=Path('/Users/markik/Code/worktrees/turnstone-cross-source-answers-20261022')
R=T/'docs/receipts/semantic_challenge_20261026'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v):p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n')
summary=json.loads((O/'summary.json').read_text());suite=json.loads((O/'frozen/suite.json').read_text());assert (O/'process-summary.json').exists()
assert not R.exists(),R
R.mkdir();Q=R/'qualification';Q.mkdir();ledger=[]
def copy(src,dst):
 dst.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(src,dst);assert sha(src)==sha(dst);ledger.append({'source':str(src),'destination':str(dst.relative_to(R)),'sha256':sha(src)})
for name in ['freeze.json','harness-freeze.json','runner-freeze.json','process-summary.json','runner.log','verify_suite.py','run_suite.py','test_rubric.py','rubric-tests.json','summary.json','verification-amendment.json','verification-session-shape-error.stdout.log','verification-session-shape-error.stderr.log','verification.log']:
 copy(O/name,Q/name)
for p in sorted((O/'frozen').iterdir()):
 if p.is_file() and p.name!='inference_route':copy(p,Q/'frozen'/p.name)
for c in suite['cohorts']:
 for p in sorted((O/c['name']).iterdir()):
  if p.is_file():copy(p,Q/c['name']/p.name)
copy(O/'verify_receipt.py',R/'verify_receipt.py')
copy(O/'package_receipt.py',R/'package_receipt.py')
copy(O/'summary.json',R/'summary.json')
write(R/'copy-ledger.json',{'copies':ledger,'omitted_artifacts':[{'name':'frozen/inference_route','reason':'Executable retained in Code/output, not distributed in the public receipt. Its source and SHA256 join the preceding quoted_review_20261024 baseline.'}],'synthetic_only':True})
counts=summary['semantic_cases'];lines=[]
for c in suite['cohorts']:
 rows=[r for r in summary['cases'] if r['cohort']==c['name']]
 sem=[r for r in rows if r['kind']=='semantic']
 lines.append('| '+c['name']+' | '+str(len(sem))+' | '+str(sum(r['result']=='pass' for r in sem))+' | '+str(sum(r['result']!='pass' for r in sem))+' | '+str(sum(r['kind']=='admission_negative' and r['result']=='pass' for r in rows))+' |')
failures=[]
for r in summary['cases']:
 for s in r.get('slots',[]):
  if s['result']!='pass':failures.append('- `'+r['cohort']+'/'+r['case']+'`, fact '+str(s['slot']+1)+': expected **'+s['expected_status']+'** / `'+s['expected_agreement']+'`, observed **'+s['actual_status']+'** / `'+s['actual_agreement']+'`. '+', '.join(e['kind'] for e in s['errors'])+'.')
freeze=json.loads((O/'freeze.json').read_text());elapsed=json.loads((O/'process-summary.json').read_text())['elapsed_seconds']
readme=f'''# Semantic challenge, 2026-10-26

The broader semantic qualification gate is **{summary['result']}**. Of fifteen admitted model requests, **{counts['passed']} passed** the frozen semantic rubric and **{counts['failed']} failed**; {counts['incomplete']} were incomplete. A separate sixteenth case exercised the expected oversized-conflict admission refusal and is not counted as a semantic answer success.

This extends the four-case [quoted-review baseline](../quoted_review_20261024/README.md) with paraphrases, entity/time/certainty qualifiers, repeated agreement versus conflicting values, and longer passages. All gold labels were frozen before measuring. No model, prompt, grammar, sampling, token limit, or timeout was tuned on the challenge. The first whole-suite checker run failed because it treated session metadata as a scalar. [verification-amendment.json](qualification/verification-amendment.json) records the exact two replacements that compare the existing session object correctly; the original checker and abbreviated error transcription are preserved. Gold labels and semantic logic are unchanged.

| Cohort | Semantic requests | Passed | Failed/incomplete | Expected admission refusals observed |
|---|---:|---:|---:|---:|
'''+ '\n'.join(lines)+f'''

## What the measurement establishes

All {summary['complete_raw_documents']} completed raw documents passed whole-document JSON, fixed requested-fact count/order, exact candidate-review and selected-quotation shape, retained-support consistency, and citation-selector checks. All {summary['contract_input_bindings']} input contracts matched independently derived question facts, exact v7 instructions, context, and output-grammar hashes. These are structural and provenance checks; semantic correctness is assessed separately against the frozen gold.

Numerical telemetry joined all {summary['score_events']} generation requests: {summary['invalid_rows']} invalid score rows and {summary['prefix_recomputations']} prefix recomputations. The external rubric passed 61 synthetic positive/mutation checks, including rejection of two identical quotations from distinct sources as a claimed conflict. Rubric checks are not model successes or product tests.

## Observed failures

'''+ '\n'.join(failures)+f'''

The failure details include every supplied candidate's relevance decision, agreement, status and selected source keys in [summary.json](summary.json). A truthful quotation and an exact source link alone do not establish that the passage answers the requested property. Retained support remains `semantic_review: unreviewed` throughout this measurement.

## Frozen execution envelope

- Qwen/Qwen2.5-1.5B-Instruct revision `989aa7980e4cf806f80c7fef2b1adb7bc71aa306`, original f32 streamed execution, AMD Radeon Pro Vega 56 / Metal.
- Generation session `blake3:0960d64d68cf80b35a5781e3daeddfcf453f30579861e02af8d93a535db58d3d`.
- Existing resident probe SHA256 `{freeze['baseline_binary_sha256']}`; APFS clone verified before measurement. No Cargo compilation was launched.
- `distillery.quoted-review/v7`; 1024 output tokens and 900 seconds per request; 3 GiB host weight cache and 4 GiB device-layer payload budget.
- Lexical recall, maximum four passages, explicit scopes with two to four synthetic PageText sources. Source roles reuse the existing generic probe's capture/control/knot/note keys; this run does not exercise native producer ingestion.
- {elapsed:.3f} seconds total elapsed, including separate model/resident loading for each cohort. The probe's temporary stores are disposable synthetic fixtures; raw run documents, logs, progress, process outcomes and frozen inputs are retained here and in `Code/output/semantic-challenge-20261026`.

The longer-passage conflict case exceeds the existing 384-byte extractive conflict-text bound. Admission returned the precise refusal before generation. The generic test probe unwraps that result and exits nonzero; this is expected admission evidence, not a qualified application error screen or an application-crash finding.

## Qualification boundary

This is one deterministic synthetic lexical measurement on one model/backend. It does not establish broad semantic reliability, fused-recall results, new producer retention, headed behavior, or cold-reopen correctness. The prior four-case native receipt retains its original boundary. Because this broader gate did not pass, no new headed or cold-reopen cohort was run.

The exposed work is semantic relevance and agreement: handling paraphrases and qualifiers, comparing all relevant values, and selecting quotations that show the actual disagreement. No production code was changed by this qualification.

## Reproduce the receipt checks

Run `python3 verify_receipt.py` with Python's `blake3` package available. It verifies all included hashes, the original frozen question/gold/source bytes, raw contract/output consistency, telemetry joins, and the recorded semantic results, including failures. It does not rerun inference. The model and executable remain in the retained local evidence lane and are intentionally excluded from this public package.
'''
(R/'README.md').write_text(readme)
write(R/'hashes.json',{str(p.relative_to(R)):sha(p) for p in sorted(R.rglob('*')) if p.is_file() and p.name!='hashes.json' and '__pycache__' not in p.parts})
print(json.dumps({'receipt':str(R),'files':len(json.loads((R/'hashes.json').read_text())),'bytes':sum(p.stat().st_size for p in R.rglob('*') if p.is_file())},indent=2))
