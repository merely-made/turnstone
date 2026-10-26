#!/usr/bin/env python3
"""Whole raw document and exact bounded-shape checks; no output rewriting."""
from pathlib import Path
import argparse,copy,json

def check(run):
 raw=run['text'];doc=json.loads(raw);r=run['recall']
 assert isinstance(doc,dict) and set(doc)=={'claims'}
 claims=doc['claims'];facts=r['answer_contract']['requested_facts']['facts'];assert isinstance(claims,list) and len(claims)==len(facts);assert [c['fact'] for c in claims]==facts
 quoted=escape=False
 for c in raw:
  if quoted:
   if escape:escape=False
   elif c=='\\':escape=True
   elif c=='"':quoted=False
  elif c=='"':quoted=True
  else:assert not c.isspace(),'whitespace outside a JSON string'
 candidates=[({'handle':p['handle'],'quote':p['passage']['text']},p['passage']['source']) for p in r['passages']]
 for claim in claims:
  assert isinstance(claim,dict) and set(claim)=={'fact','status','text','evidence'}
  assert isinstance(claim['text'],str) and claim['text'].strip()
  assert len(json.dumps(claim['text'],ensure_ascii=False,separators=(',',':')).encode())-2<=384
  links=claim['evidence'];assert isinstance(links,list)
  sources=[]
  for link in links:
   assert isinstance(link,dict) and set(link)=={'handle','quote'} and type(link['handle']) is int
   sources.append(next(source for value,source in candidates if value==link))
  if claim['status']=='supported':assert len(sources)==1
  elif claim['status']=='conflicting':assert len(sources)==2 and sources[0]!=sources[1]
  elif claim['status']=='insufficient':assert not sources
  else:raise AssertionError('unknown claim status')
 support=r['support'];assert support and support['error'] is None;assert [c['fact'] for c in support['claims']]==facts
 assert [(c['status'],c['text'],[(e['handle'],e['quote']) for e in c['evidence']]) for c in claims]==[(c['status'],c['text'],[(e['handle'],e['quote']) for e in c['evidence']]) for c in support['claims']]
 return {'claims':len(claims),'whole_raw_json':True,'exact_candidate_links':sum(len(c['evidence']) for c in claims)}

def main():
 p=argparse.ArgumentParser();p.add_argument('probe',type=Path);a=p.parse_args();cases=json.loads((a.probe/'frozen/cases.json').read_text());results={};controls={}
 for case in cases['asks']:
  key=case['id'];run=json.loads((a.probe/(key+'.json')).read_text())
  if run['state']!='finished' or (a.probe/(key+'.timeout.json')).exists():results[key]={'result':'incomplete'};continue
  try:results[key]={'result':'pass',**check(run)}
  except (AssertionError,ValueError,KeyError,TypeError,StopIteration) as e:results[key]={'result':'fail','error':repr(e)}
  if results[key]['result']=='pass':
   mutations={'fence':'```json\n'+run['text']+'\n```','suffix':run['text']+'x','extra_field':json.dumps({**json.loads(run['text']),'conflicting':[]},ensure_ascii=False)}
   for name,raw in mutations.items():
    invalid=copy.deepcopy(run);invalid['text']=raw
    try:check(invalid)
    except (AssertionError,ValueError,KeyError,TypeError,StopIteration):controls[key+'_'+name]='rejected'
    else:raise AssertionError('Invalid raw-output control passed')
 result={'result':'pass' if len(results)==4 and all(v['result']=='pass' for v in results.values()) else 'gate_not_passed','cases':results,'negative_controls':controls,'boundary':'Raw whole-document syntax and bounded quotation shape; semantic correctness remains the separate frozen rubric.'}
 (a.probe/'structured-output-review.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n');print(json.dumps(result,ensure_ascii=False,indent=2));raise SystemExit(0 if result['result']=='pass' else 1)
if __name__=='__main__':main()
