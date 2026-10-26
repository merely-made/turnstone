#!/usr/bin/env python3
"""Review exact request identity even when output is cancelled or rejected.
This supplements the frozen checker; it cannot grant answer acceptance.
"""
from pathlib import Path
import argparse,json,re,hashlib
import blake3

def digest(s):return 'blake3:'+blake3.blake3(s.encode()).hexdigest()
def wire(v):return json.dumps(v,ensure_ascii=False,separators=(',',':'))
def grammar(r):
 def lit(value):return {'kind':'literal','value':value}
 def seq(items):return {'kind':'sequence','items':items}
 def choice(alternatives):return {'kind':'choice','alternatives':alternatives}
 def string():return {'kind':'json_string','max_bytes':384}
 ps=r['passages'];cs=[lit(wire({'handle':p['handle'],'quote':p['passage']['text']})) for p in ps]
 variants=[seq([lit('{"status":"insufficient","text":'),string(),lit(',"evidence":[]}')])]
 if cs:
  variants.append(seq([lit('{"status":"supported","text":'),string(),lit(',"evidence":['),choice(cs),lit(']}')]))
  pairs=[seq([cs[i],lit(','),cs[j]]) for i,a in enumerate(ps) for j,b in enumerate(ps) if i<j and a['passage']['source']!=b['passage']['source']]
  if pairs:variants.append(seq([lit('{"status":"conflicting","text":'),string(),lit(',"evidence":['),choice(pairs),lit(']}')]))
 claim=choice(variants);tail=lit(']}')
 for _ in range(1,8):tail=choice([lit(']}'),seq([lit(','),claim,tail])])
 return seq([lit('{"claims":['),claim,tail])

def main():
 p=argparse.ArgumentParser();p.add_argument('probe',type=Path);a=p.parse_args();f=a.probe/'frozen';source=(f/'answer_contract.rs').read_text();instructions=re.search(r'const INSTRUCTIONS_V4: &str = r#"(.*?)"#;',source,re.S).group(1)
 contract=json.loads((f/'measurement-contract.json').read_text());assert hashlib.sha256((f/'answer_contract.rs').read_bytes()).hexdigest()==contract['prompt_module_sha256_before_measurement']
 cases=json.loads((f/'cases.json').read_text());assert hashlib.sha256((f/'cases.json').read_bytes()).hexdigest()==contract['unchanged_frozen_sha256']['cases.json'];checks={}
 for case in cases['asks']:
  key=case['id'];run=json.loads((a.probe/(key+'.json')).read_text());r=run['recall'];assert run['question']==case['question']
  candidates=[{'handle':p['handle'],'quote':p['passage']['text']} for p in r['passages']];context='EVIDENCE candidates (the entire available evidence set):\n'+wire(candidates)+'\n';prompt=instructions+wire(run['question'])+'\nReturn the complete minified JSON object.'
  expected={'method':'distillery.requested-facts/v4','question':digest(run['question']),'prompt':digest(prompt),'context':digest(context),'max_claims':8,'output_grammar':digest(wire(grammar(r)))};assert r['answer_contract']==expected
  assert len(prompt)<=4096 and len(context)<=r['config']['max_context_chars']
  allowed=['capture','control','knot','note'] if key=='conflict' else ['capture','knot'] if key=='reduced' else ['capture','knot','note'];assert sorted(s['source'].rsplit('/',1)[-1] for s in r['scope'])==sorted(allowed)
  for p in r['passages']:
   passage=p['passage'];label=passage['source']['source'].rsplit('/',1)[-1];assert label in allowed and passage['text']==cases['corpus'][label]['text']
  support=r.get('support');assert support is None or len(support['claims'])<=8
  checks[key]={'input_identity':'pass','run_state':run['state'],'model_tokens':run['tokens'],'scope':allowed,'supplied_passages':len(candidates),'admitted_claims':len(support['claims']) if support else None,'support_error':support['error'] if support else None,'contract':expected}
 result={'result':'pass','cases':checks,'boundary':'Exact inputs, source cuts, retained task identity and admitted-claim bound only. A cancelled run or a rejected document remains unqualified; no semantic or cold-reopen acceptance is implied.'};(a.probe/'contract-input-review.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n');print(json.dumps(result,ensure_ascii=False,indent=2))
if __name__=='__main__':main()
