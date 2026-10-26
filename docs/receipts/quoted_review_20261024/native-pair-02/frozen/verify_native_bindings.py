#!/usr/bin/env python3
"""Independent bounded claim-support, provenance, process and cold verification."""
import argparse,hashlib,json,re
from pathlib import Path
import blake3
HERE=Path(__file__).resolve().parent

def read(p):return json.loads(p.read_text())
def digest(v):return 'blake3:'+blake3.blake3(v if isinstance(v,bytes) else v.encode()).hexdigest()
def wire(v):return json.dumps(v,ensure_ascii=False,separators=(',',':'))
def passage_id(p):
 source={k:p['source'][k] for k in ['kind','owner','source','node'] if k in p['source']}
 return digest(wire({'source':source,**{k:p[k] for k in ['version','text_hash','selector','chunker','text']}}))
def objects(v):
 if isinstance(v,dict):
  yield v
  for x in v.values():yield from objects(x)
 elif isinstance(v,list):
  for x in v:yield from objects(x)
def verify_answer(run,case,cases):
 assert run['state']=='finished' and run['question']==case['question']
 r=run['recall'];s=r['support'];assert r['answer_format']=='claim_support_v1'
 assert len(r['scope'])==case['scope_members'] and not r['stale'] and not r.get('unknown_citations')
 assert s['method']=='eidetic.claim-support/v1' and s['semantic_review']=='unreviewed' and s['raw_output']==digest(run['text']) and s['error'] is None
 claims=s['claims'];assert [c['status'] for c in claims]==case['expected_status']
 assert all(not c['issues'] for c in claims) and len({c['text'].strip() for c in claims})==len(claims)
 passages={v['handle']:v['passage'] for v in r['passages']};seen=[]
 for claim in claims:
  for link in claim['evidence']:
   p=passages[link['handle']];pid=passage_id(p)
   assert link['error'] is None and link['passage']==pid and digest(p['text'])==p['text_hash']
   assert p['selector']['end']-p['selector']['start']==len(p['text'])
   start=link['selector']['start']-p['selector']['start'];end=link['selector']['end']-p['selector']['start']
   assert 0<=start<end<=len(p['text']) and p['text'][start:end]==link['quote'] and len(link['quote'].strip())>=16
   label=next(k for k,v in cases['corpus'].items() if v['text']==p['text']);seen.append(label)
   assert any(c['handle']==link['handle'] and c['passage']==pid and c['basis'] in ['claim_quotation','model_marker'] for c in r['citations'])
 if case['id']=='cross':
  assert seen==['capture','knot','note']
  for claim in claims:
   p=passages[claim['evidence'][0]['handle']];text=claim['text'].lower()
   label=next(k for k,v in cases['corpus'].items() if v['text']==p['text'])
   assert all(word in text for word in {'capture':['juniper','recovery','violet lantern'],'knot':['pelican','access','café'],'note':['café','harbor']}[label]),(label,claim)
 elif case['id']=='conflict':
  assert set(seen)=={'capture','control'} and len(seen)==2
  assert any(w in claims[0]['text'].lower() for w in ['conflict','disagree','differ','both','inconsistent'])
 else:
  assert not seen and not r['citations']
  text=claims[0]['text'].lower()
  assert any(w in text for w in ['not','no ','unknown','unavailable','insufficient','missing','cannot']),claims[0]
  if case['id']=='missing':
   assert 'budget' in text and not re.search(r'[0-9$€£]',text),claims[0]
  if case['id']=='reduced':
   assert any(w in text for w in ['listener','reminder','note']),claims[0]
   assert 'café' not in text and 'harbor' not in text,claims[0]
   assert all(p['text']!=cases['corpus']['note']['text'] for p in passages.values())
 return {'claims':len(claims),'status':[c['status'] for c in claims],'checked_quotes':len(seen),'sources':seen,'semantic_review':'unreviewed','raw_output':s['raw_output']}
def reader_source_node(reader):
 # Saved-reader node is a view identity; its admitted source is target.key.node.
 return reader['target']['key']['node'] if 'target' in reader else reader['node']

def main():
 parser=argparse.ArgumentParser();parser.add_argument('run',type=Path);a=parser.parse_args();base=a.run
 cases=read(HERE/'cases.json');inputs=read(base/'inputs.json');assert read(base/'completion.json')['result']=='pass'
 pids=[];results={};frames=[]
 for mode in read(base/'completion.json')['modes']:
  lane=base/mode;phases=read(lane/'phases.json');assert len(phases)==2 and phases[1]['transfer_absent']
  assert read(lane/'origin-stop.json')['tcp_refused'] and read(lane/'requests-at-stop.json')==read(lane/'requests-final.json')
  snapshots=[read(lane/'resident'/f'snapshot-{i}.json') for i in [0,1]]
  for i,phase in enumerate(['warm','cold']):
   proc=read(lane/phase/'process.json');assert proc['success'] and not proc['timeout'] and proc['binary_sha256']==inputs['binary_sha256']
   assert proc['scenario_sha256']==inputs['frozen_sha256'][phase+'.scn']
   state=phases[i];assert state['clean_shutdown'] and state['endpoint_unreachable'] and state['origin_refused'] and state['producer_paths_absent']
   pids+=state['product_pids']+[state['resident_pid']]
   done=(lane/phase/'scenario.done').read_text();assert done.startswith('RESULT ok') and 'blank=0' in done
   frames += [str(p.relative_to(base)) for p in (lane/phase).glob('*.png')]
  checks={}
  for case in cases['asks']:
   ident=case['id'];warm=read(lane/'resident'/f'mixed-0-{ident}.json');history=read(lane/'resident'/f'history-1-{ident}.json');assert history==warm
   checks[ident]=verify_answer(warm,case,cases)
   old=next(v for v in snapshots[0]['evidence'] if v['record']['run']==warm['run']);cold=next(v for v in snapshots[1]['evidence'] if v['record']['run']==warm['run']);assert old==cold
   assert old['record']['output_blob']==digest(warm['text']) and old['answer_support']==warm['recall']['support']
   assert old['record']['question_blob']==digest(case['question'])
   assert set(old['record']['supplied_passages'])=={passage_id(p['passage']) for p in warm['recall']['passages']}
  fresh=read(lane/'resident/mixed-1-cross.json');checks['fresh_cold_cross']=verify_answer(fresh,cases['asks'][0],cases)
  warm_cross=read(lane/'resident/mixed-0-cross.json')
  assert fresh['run']!=warm_cross['run']
  if mode=='fused':
   assert warm_cross['recall']['embedding']['cache']=='built' and fresh['recall']['embedding']['cache']=='reopened'
   for field in ['envelope','index']:assert warm_cross['recall']['embedding'][field]==fresh['recall']['embedding'][field]
   assert snapshots[0]['embedding_projections']==snapshots[1]['embedding_projections']
  # Each claim's actual native citation must resolve its specific source, with
  # the same rendered document before and after restarting both processes.
  for n,claim in enumerate(read(lane/'resident/mixed-0-cross.json')['recall']['support']['claims'],1):
   link=claim['evidence'][0];warm=read(lane/'resident/mixed-0-cross.json');p=next(v['passage'] for v in warm['recall']['passages'] if v['handle']==link['handle']);documents=[]
   for phase in ['warm','cold']:
    observation=read(lane/phase/f'cross_{phase}_claim_{n}_citation.saved-sources.json')
    readers=observation['readers']+observation['retained_readers'];reader=next(v for v in readers if reader_source_node(v)==p['source']['node'])
    if 'target' in reader:assert reader['target']['selector']==link['selector'] and reader['target']['text_hash']==p['text_hash']
    document=reader['rendered_document'];assert p['text'] in [v['Text'] for v in objects(document) if 'Text' in v];documents.append(document)
   assert documents[0]==documents[1]
  results[mode]=checks
 assert len(pids)==len(set(pids)) and len(pids)>=4
 result={'result':'pass','cases':results,'distinct_processes':len(pids),'captures':frames,'scope':'Frozen fixture and exact quotation/provenance checks; semantic interpretations remain unreviewed until the separate recorded visual/rubric review.'}
 (base/'verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n');print('Cross-source answer and cold-reopen verification passed')
if __name__=='__main__':main()
