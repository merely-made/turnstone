#!/usr/bin/env python3
"""Independent question-only fact derivation, exact contract and raw slot checks.
No source values, expected statuses or answers participate in fact planning.
The original frozen semantic checker remains separate and unchanged.
"""
from pathlib import Path
import argparse,copy,json,re,hashlib
import blake3

def wire(v):return json.dumps(v,ensure_ascii=False,separators=(',',':'))
def digest(s):return 'blake3:'+blake3.blake3(s.encode()).hexdigest()
def derive(q):
 head,sep,tail=q.partition('?');assert '?' not in tail and not any(c in head for c in '"()[];\n')
 listed=tail.strip().lower()=='give one claim for each, in that order.'
 prefix='What are ' if listed else 'What is ' if head[:8].lower()=='what is ' else ''
 assert not listed or head[:len(prefix)].lower()==prefix.lower()
 body=head[len(prefix):].strip()
 if listed:
  if ', and ' in body:
   first,last=body.rsplit(', and ',1);parts=first.split(', ')+[last]
  else:parts=body.split(' and ');assert len(parts)==2
 else:parts=[body]
 assert 1<=len(parts)<=8 and (not listed or len(parts)>1)
 cursor=len(prefix);facts=[];qid=digest(q)
 for part in parts:
  assert part and part.strip()==part and len(part.encode())<=512 and ',' not in part and ' and ' not in part.lower() and ' or ' not in part.lower()
  start=q.index(part,cursor,len(head));end=start+len(part);selector={'start':start,'end':end};cursor=end
  facts.append({'id':digest(wire([qid,selector,part])),'selector':selector,'text':part})
 return {'method':'eidetic.explicit-question-list/v1','question':qid,'facts':facts}

def grammar(r,facts):
 def lit(value):return {'kind':'literal','value':value}
 def seq(items):return {'kind':'sequence','items':items}
 def choice(alternatives):return {'kind':'choice','alternatives':alternatives}
 ps=r['passages'];cs=[lit(wire({'handle':p['handle'],'quote':p['passage']['text']})) for p in ps]
 def tree(at,selected,fact):
  if at<len(ps):
   branches=[]
   for answer in [False,True]:
    row=wire({'handle':ps[at]['handle'],'answers':answer})
    branches.append(seq([lit((',' if at else '')+row),tree(at+1,selected+[at] if answer else selected,fact)]))
   return choice(branches)
  def body(agreement,status,links):
   text=lit(wire('The supplied evidence does not state '+fact['text']+'.')) if status=='insufficient' else {'kind':'json_string','max_bytes':384}
   return seq([lit('],"agreement":'+wire(agreement)+'},"status":'+wire(status)+',"text":'),text,lit(',"evidence":['),links,lit(']}')])
  if not selected:return body('none','insufficient',lit(''))
  supported=body('single' if len(selected)==1 else 'same','supported',choice([cs[i] for i in selected]))
  pairs=[seq([cs[i],lit(','),cs[j]]) for a,i in enumerate(selected) for j in selected[a+1:] if ps[i]['passage']['source']!=ps[j]['passage']['source']]
  return choice([supported,body('different','conflicting',choice(pairs))]) if pairs else supported
 items=[lit('{"claims":[')]
 for n,fact in enumerate(facts):
  if n:items.append(lit(','))
  items.append(seq([lit('{"fact":'+wire(fact)+',"review":{"candidates":['),tree(0,[],fact)]))
 items.append(lit(']}'));return seq(items)

def check(run,instructions):
 r=run['recall'];q=run['question'];facts=derive(q)
 candidates=[{'handle':p['handle'],'quote':p['passage']['text']} for p in r['passages']]
 context='EVIDENCE candidates (the entire available evidence set):\n'+wire(candidates)+'\n'
 prompt=instructions+'REQUESTED_FACTS (fixed count '+str(len(facts['facts']))+', in this order):\n'+wire(facts['facts'])+'\nQUESTION (JSON string): '+wire(q)+'\nReturn the complete minified JSON object.'
 expected={'method':'distillery.candidate-review/v6','question':digest(q),'prompt':digest(prompt),'context':digest(context),'max_claims':len(facts['facts']),'output_grammar':digest(wire(grammar(r,facts['facts']))),'requested_facts':facts}
 assert r['answer_contract']==expected,'contract input or fact identity mismatch'
 assert len(prompt)<=4096 and len(context)<=r['config']['max_context_chars']
 support=r.get('support')
 if support:
  assert support['requested_facts']==digest(wire(facts))
  assert len(support['claims'])==len(facts['facts'])
  assert [c['fact'] for c in support['claims']]==facts['facts']
  assert support['raw_output']==digest(run['text'])
 if run['state']=='finished':
  claims=json.loads(run['text'])['claims'];assert len(claims)==len(facts['facts']);assert [c['fact'] for c in claims]==facts['facts']
 return {'facts':facts,'set_id':digest(wire(facts)),'prompt_chars':len(prompt),'context_chars':len(context),'contract':expected}

def main():
 p=argparse.ArgumentParser();p.add_argument('probe',type=Path);a=p.parse_args();f=a.probe/'frozen';source=(f/'answer_contract.rs').read_text();instructions=re.search(r'const INSTRUCTIONS_V6: &str = r#"(.*?)"#;',source,re.S).group(1)
 contract=json.loads((f/'measurement-contract.json').read_text());assert hashlib.sha256((f/'answer_contract.rs').read_bytes()).hexdigest()==contract['prompt_module_sha256_before_measurement']
 cases=json.loads((f/'cases.json').read_text());assert hashlib.sha256((f/'cases.json').read_bytes()).hexdigest()==contract['unchanged_frozen_sha256']['cases.json'];checks={};controls={}
 for case in cases['asks']:
  key=case['id'];run=json.loads((a.probe/(key+'.json')).read_text());assert run['question']==case['question'];review=check(run,instructions);r=run['recall']
  allowed=['capture','control','knot','note'] if key=='conflict' else ['capture','knot'] if key=='reduced' else ['capture','knot','note'];assert sorted(s['source'].rsplit('/',1)[-1] for s in r['scope'])==sorted(allowed)
  for p in r['passages']:
   passage=p['passage'];label=passage['source']['source'].rsplit('/',1)[-1];assert label in allowed and passage['text']==cases['corpus'][label]['text']
  checks[key]={'input_identity':'pass','run_state':run['state'],'model_tokens':run['tokens'],**review}
  mutations={}
  bad=copy.deepcopy(run);bad['question']+=' changed';mutations['foreign_question']=bad
  bad=copy.deepcopy(run);bad['recall']['answer_contract']['requested_facts']['facts'][0]['text']+=' changed';mutations['altered_fact']=bad
  if run['state']=='finished':
   doc=json.loads(run['text'])
   for name,changed in [('missing',doc['claims'][:-1]),('extra',doc['claims']+[doc['claims'][0]])]+([('reordered',list(reversed(doc['claims'])))] if len(doc['claims'])>1 else []):
    bad=copy.deepcopy(run);bad['text']=wire({'claims':changed})
    # Keep the forged support/raw hash mutually consistent: rejection must
    # still follow from the prescribed count/order, not a stale output hash.
    if bad['recall'].get('support'):
     by_fact={c['fact']['id']:c for c in bad['recall']['support']['claims']}
     bad['recall']['support']['claims']=[copy.deepcopy(by_fact[c['fact']['id']]) for c in changed]
     bad['recall']['support']['raw_output']=digest(bad['text'])
    mutations[name]=bad
  for name,bad in mutations.items():
   try:check(bad,instructions)
   except (AssertionError,ValueError,KeyError,TypeError):controls[key+'_'+name]='rejected'
   else:raise AssertionError('Invalid requested-fact control passed: '+name)
 result={'result':'pass','cases':checks,'negative_controls':controls,'boundary':'Question-only fact plan, fixed count/order, input and raw/support identity. Semantic correctness uses the separate unchanged frozen rubric.'};(a.probe/'contract-input-review.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n');print(json.dumps(result,ensure_ascii=False,indent=2))
if __name__=='__main__':main()
