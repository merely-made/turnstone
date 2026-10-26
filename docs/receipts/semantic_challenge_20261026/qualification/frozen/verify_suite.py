"""External frozen-gold qualification. Never edits model output or retained support."""
from pathlib import Path
import collections,copy,hashlib,importlib.util,json,re,sys
O=Path(__file__).resolve().parent
F=O/'frozen'
def mod(name):
 spec=importlib.util.spec_from_file_location(name,F/(name+'.py'));m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
binding=mod('verify_contract_inputs');shape=mod('verify_structured_output');health=mod('verify_score_health')
instructions=re.search(r'const INSTRUCTIONS_V7: &str = r#"(.*?)"#;',(F/'answer_contract.rs').read_text(),re.S).group(1)
def label(p):return p['passage']['source']['source'].rsplit('/',1)[-1]
def semantic(case,run):
 claims=json.loads(run['text'])['claims'];assert len(claims)==len(case['gold']),'wrong fact count'
 passages=run['recall']['passages'];byhandle={p['handle']:label(p) for p in passages};available=set(byhandle.values());detail=[]
 for i,(claim,gold) in enumerate(zip(claims,case['gold'])):
  relevant=set(gold['relevant']);assert relevant<=available,('retrieval missing relevant sources',sorted(relevant-available))
  expected=[p['handle'] for p in passages if label(p) in relevant]
  actual=[d['handle'] for d in claim['review']['candidates'] if d['answers']]
  agreement='none' if not relevant else 'single' if len(relevant)==1 else 'same' if len({gold['values'][k] for k in relevant})==1 else 'different'
  status='insufficient' if not relevant else 'conflicting' if agreement=='different' else 'supported'
  selected=[byhandle[e['handle']] for e in claim['evidence']]
  errors=[]
  if actual!=expected:errors.append({'kind':'candidate_relevance','expected_handles':expected,'actual_handles':actual})
  if claim['review']['agreement']!=agreement:errors.append({'kind':'agreement','expected':agreement,'actual':claim['review']['agreement']})
  if claim['status']!=status:errors.append({'kind':'status','expected':status,'actual':claim['status']})
  if status=='insufficient' and selected:errors.append({'kind':'insufficient_has_links'})
  if status=='supported' and (len(selected)!=1 or not set(selected)<=relevant):errors.append({'kind':'unsupported_selected_source','selected':selected})
  if status=='conflicting' and (len(selected)!=2 or len(set(selected))!=2 or not set(selected)<=relevant or len({gold['values'].get(k) for k in selected})!=2):errors.append({'kind':'conflict_does_not_show_distinct_values','selected':selected})
  detail.append({'slot':i,'fact':claim['fact']['text'],'expected_status':status,'actual_status':claim['status'],'expected_agreement':agreement,'actual_agreement':claim['review']['agreement'],'supplied_sources':list(byhandle.values()),'selected_sources':selected,'errors':errors,'result':'fail' if errors else 'pass'})
 return detail

def check():
 suite=json.loads((F/'suite.json').read_text());freeze=json.loads((O/'freeze.json').read_text())
 for name,digest in freeze['sha256'].items():assert hashlib.sha256((F/name).read_bytes()).hexdigest()==digest,('freeze changed',name)
 report=[];controls=[];all_events=[];session_ids=set()
 for cohort in suite['cohorts']:
  d=O/cohort['name'];assert json.loads((d/'cases.json').read_text())==cohort,'external cases changed'
  proc=json.loads((d/'process-exit.json').read_text()) if (d/'process-exit.json').exists() else None
  log=(d/'run.log').read_text() if (d/'run.log').exists() else ''
  events=health.parse(log);all_events.extend(events);remaining=collections.defaultdict(list)
  for e in events:remaining[e['prompt']].append(e)
  for case in cohort['asks']:
   row={'cohort':cohort['name'],'case':case['id']};path=d/(case['id']+'.json')
   if case.get('expected_admission_error'):
    ok=bool(proc and proc['exit_code']!=0 and not proc['outer_timeout'] and case['expected_admission_error'] in log and not path.exists() and not events)
    row.update(kind='admission_negative',result='pass' if ok else 'fail',observed='probe panics on expected ask_recalled refusal' if ok else 'missing or unexpected refusal',generation_requests=len(events));report.append(row);continue
   row['kind']='semantic'
   if not path.exists():row.update(result='incomplete',process=proc);report.append(row);continue
   run=json.loads(path.read_text());session_ids.add(run['session']);row.update(state=run['state'],tokens=run['tokens'])
   if run['state']!='finished' or (d/(case['id']+'.timeout.json')).exists():row.update(result='incomplete',error=run.get('error'));report.append(row);continue
   try:
    assert run['question']==case['question'],'wrong question'
    allowed=['capture','control','knot','note'] if case['id']=='conflict' else ['capture','knot'] if case['id']=='reduced' else ['capture','knot','note']
    assert sorted(s['source'].rsplit('/',1)[-1] for s in run['recall']['scope'])==sorted(allowed),'scope mismatch'
    for p in run['recall']['passages']:assert label(p) in allowed and p['passage']['text']==cohort['corpus'][label(p)]['text'],'passage differs from frozen source'
    assert run['session']=='blake3:0960d64d68cf80b35a5781e3daeddfcf453f30579861e02af8d93a535db58d3d','different model session'
    b=binding.check(run,instructions);s=shape.check(run)
    row.update(input_identity='pass',structured_output='pass',facts=b['facts'],shape=s)
   except (AssertionError,ValueError,KeyError,TypeError,StopIteration) as e:row.update(result='integrity_fail',integrity_error=repr(e));report.append(row);continue
   rows=remaining[run['recall']['answer_contract']['prompt']]
   event=rows.pop(0) if rows else None
   healthy=bool(event and event['checked_rows']>0 and event['invalid_rows']==0 and event['prefix_recomputations']==0 and event['failed_recoveries']==0)
   row['score_health']={'result':'pass' if healthy else 'fail','event':event}
   try:details=semantic(case,run);row['slots']=details;row['result']='pass' if healthy and all(v['result']=='pass' for v in details) else 'fail'
   except (AssertionError,ValueError,KeyError,TypeError,StopIteration) as e:row.update(result='fail',semantic_error=repr(e))
   # Prove that the external rubric rejects a relevance decision mutation even
   # when the question, passages and all gold labels remain unchanged.
   if row['result']=='pass' and run['recall']['passages']:
    bad=copy.deepcopy(run);doc=json.loads(bad['text']);doc['claims'][0]['review']['candidates'][0]['answers']=not doc['claims'][0]['review']['candidates'][0]['answers'];bad['text']=json.dumps(doc,ensure_ascii=False,separators=(',',':'))
    assert any(v['result']=='fail' for v in semantic(case,bad)),'rubric accepted flipped relevance'
    controls.append(cohort['name']+'/'+case['id']+': flipped relevance rejected')
   report.append(row)
  assert not any(remaining.values()),('unjoined telemetry',cohort['name'])
 semantic_rows=[r for r in report if r['kind']=='semantic'];admission=[r for r in report if r['kind']=='admission_negative']
 summary={'result':'pass' if len(report)==16 and all(r['result']=='pass' for r in report) else 'gate_not_passed','semantic_cases':{'total':len(semantic_rows),'passed':sum(r['result']=='pass' for r in semantic_rows),'failed':sum(r['result'] in ['fail','integrity_fail'] for r in semantic_rows),'incomplete':sum(r['result']=='incomplete' for r in semantic_rows)},'admission_negatives':{'total':len(admission),'passed':sum(r['result']=='pass' for r in admission)},'complete_raw_documents':sum(r.get('structured_output')=='pass' for r in semantic_rows),'contract_input_bindings':sum(r.get('input_identity')=='pass' for r in semantic_rows),'score_events':len(all_events),'invalid_rows':sum(e['invalid_rows'] for e in all_events),'prefix_recomputations':sum(e['prefix_recomputations'] for e in all_events),'model_sessions':sorted(session_ids),'negative_controls':controls,'cases':report,'limits':'Lexical synthetic PageText probe only; no new producer, fused recall, headed or cold-reopen qualification. Retained review is model authored and unreviewed. Admission panic comes from the probe unwrap, not an application crash.'}
 (O/'summary.json').write_text(json.dumps(summary,ensure_ascii=False,indent=2)+'\n')
 print(json.dumps({k:v for k,v in summary.items() if k!='cases'},ensure_ascii=False,indent=2))
 for r in report:
  print(r['cohort'],r['case'],r['result'],[(s['slot'],s['errors']) for s in r.get('slots',[]) if s['errors']],r.get('semantic_error',''),r.get('integrity_error',''))
 return summary
if __name__=='__main__':check()
