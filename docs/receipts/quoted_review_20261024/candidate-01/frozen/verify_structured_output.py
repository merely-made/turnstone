#!/usr/bin/env python3
"""Whole raw document and exact bounded-shape checks; no output rewriting."""
from pathlib import Path
import argparse,copy,json

def evidence_selectors(raw,claims):
 decoder=json.JSONDecoder();keys=[];at=0
 while at<len(raw):
  if raw[at]!='"':at+=1;continue
  value,end=decoder.raw_decode(raw,at)
  rest=end
  while rest<len(raw) and raw[rest].isspace():rest+=1
  if value=='evidence' and raw[rest:rest+1]==':':
   rest+=1
   while raw[rest].isspace():rest+=1
   assert raw[rest]=='[';keys.append(rest)
  at=end
 assert len(keys)==len(claims)
 expected={}
 for key,claim in zip(keys,claims):
  at=key+1
  for link in claim['evidence']:
   while raw[at].isspace() or raw[at]==',':at+=1
   obj,end=decoder.raw_decode(raw,at);assert obj==link
   cursor=at;spans=[]
   while cursor<end:
    if raw[cursor]!='"':cursor+=1;continue
    value,stop=decoder.raw_decode(raw,cursor)
    if value==link['quote']:spans.append({'start':cursor,'end':stop})
    cursor=stop
   assert len(spans)==1
   expected.setdefault(link['handle'],spans[0]);at=end
 return expected

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
  assert isinstance(claim,dict) and set(claim)=={'fact','review','status','text','evidence'}
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
  review=claim['review'];assert isinstance(review,dict) and set(review)=={'candidates','agreement'}
  decisions=review['candidates'];assert len(decisions)==len(candidates)
  assert [d['handle'] for d in decisions]==[p['handle'] for p in r['passages']]
  assert all(set(d)=={'handle','requested','quote','answers'} and type(d['handle']) is int and type(d['answers']) is bool for d in decisions)
  assert all(d['requested']==claim['fact']['text'] and d['quote']==p['passage']['text'] for d,p in zip(decisions,r['passages']))
  relevant=[d['handle'] for d in decisions if d['answers']]
  assert all(link['handle'] in relevant for link in links)
  if not relevant:assert review['agreement']=='none' and claim['status']=='insufficient'
  elif len(relevant)==1:assert review['agreement']=='single' and claim['status']=='supported'
  else:assert (review['agreement'],claim['status']) in [('same','supported'),('different','conflicting')]
  if claim['status']=='supported':assert claim['text']==links[0]['quote']
  elif claim['status']=='insufficient':assert claim['text']=='The supplied evidence does not state '+claim['fact']['text']+'.'
  else:assert claim['text']=='The supplied sources disagree: '+links[0]['quote']+' / '+links[1]['quote']
 support=r['support'];assert support and support['error'] is None;assert [c['fact'] for c in support['claims']]==facts
 assert [(c['status'],c['text'],[(e['handle'],e['quote']) for e in c['evidence']]) for c in claims]==[(c['status'],c['text'],[(e['handle'],e['quote']) for e in c['evidence']]) for c in support['claims']]
 assert [c['review'] for c in claims]==[c['review'] for c in support['claims']]
 expected=evidence_selectors(raw,claims)
 assert {c['handle'] for c in r.get('citations',[])}==set(expected)
 assert all(c['basis']=='claim_quotation' and c['answer_selector']==expected[c['handle']] for c in r.get('citations',[]))
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
   doc=json.loads(run['text']);bad=copy.deepcopy(doc);bad['claims'][0].pop('review');mutations['missing_review']=json.dumps(bad,ensure_ascii=False)
   if doc['claims'][0]['review']['candidates']:
    bad=copy.deepcopy(doc);bad['claims'][0]['review']['candidates'].pop();mutations['skipped_candidate']=json.dumps(bad,ensure_ascii=False)
    if len(doc['claims'][0]['review']['candidates'])>1:
     bad=copy.deepcopy(doc);bad['claims'][0]['review']['candidates'].reverse();mutations['reordered_candidates']=json.dumps(bad,ensure_ascii=False)
   bad=copy.deepcopy(doc);bad['claims'][0]['review']['agreement']='different' if bad['claims'][0]['status']!='conflicting' else 'same';mutations['contradictory_agreement']=json.dumps(bad,ensure_ascii=False)
   bad=copy.deepcopy(doc);bad['claims'][0]['text']='Another property is stated here.';mutations['prose_drift']=json.dumps(bad,ensure_ascii=False)
   if doc['claims'][0]['review']['candidates']:
    bad=copy.deepcopy(doc);bad['claims'][0]['review']['candidates'][0]['quote']+=' changed';mutations['foreign_review_quote']=json.dumps(bad,ensure_ascii=False)
    bad=copy.deepcopy(doc);bad['claims'][0]['review']['candidates'][0]['requested']+=' changed';mutations['foreign_review_property']=json.dumps(bad,ensure_ascii=False)
   for name,raw in mutations.items():
    invalid=copy.deepcopy(run);invalid['text']=raw
    try:check(invalid)
    except (AssertionError,ValueError,KeyError,TypeError,StopIteration):controls[key+'_'+name]='rejected'
    else:raise AssertionError('Invalid raw-output control passed')
 result={'result':'pass' if len(results)==4 and all(v['result']=='pass' for v in results.values()) else 'gate_not_passed','cases':results,'negative_controls':controls,'boundary':'Raw whole-document syntax and bounded quotation shape; semantic correctness remains the separate frozen rubric.'}
 (a.probe/'structured-output-review.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n');print(json.dumps(result,ensure_ascii=False,indent=2));raise SystemExit(0 if result['result']=='pass' else 1)
if __name__=='__main__':main()
