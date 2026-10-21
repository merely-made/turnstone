#!/usr/bin/env python3
"""Independent provenance, citation, fusion, cold and native-reader checks."""
import argparse, hashlib, json
from pathlib import Path
import blake3
HERE=Path(__file__).resolve().parent
def read(path):return json.loads(path.read_text())
def digest(value):return 'blake3:'+blake3.blake3(value if isinstance(value,bytes) else value.encode()).hexdigest()
def objects(value):
 if isinstance(value,dict):
  yield value
  for v in value.values():yield from objects(v)
 elif isinstance(value,list):
  for v in value:yield from objects(v)
def passage_id(source,text):
 source={k:source[k] for k in ['kind','owner','source','node']};version=digest(text)
 return digest(json.dumps({'source':source,'version':version,'text_hash':version,'selector':{'start':0,'end':len(text)},'chunker':'mere.unicode-chunks/v1/512','text':text},ensure_ascii=False,separators=(',',':')))
def rrf(lexical,vector):
 scores={}
 for lane in [lexical,vector]:
  for rank,key in enumerate(lane):scores[key]=scores.get(key,0.)+1./(60+rank+1)
 return sorted(scores,key=lambda key:(-scores[key],key))
def metrics(ranking,relevant):
 hits=[i+1 for i,key in enumerate(ranking) if key in relevant]
 return {'reciprocal_rank':1./min(hits) if hits else 0.,'top1':int(bool(ranking) and ranking[0] in relevant),'recall_at_3':len(set(ranking[:3])&set(relevant))/len(relevant)}
def main():
 p=argparse.ArgumentParser();p.add_argument('run',type=Path);base=p.parse_args().run;cases=read(HERE/'cases.json');inputs=read(base/'inputs.json');assert read(base/'completion.json')['result']=='pass'
 all_pids=[];results={};comparison={}
 for mode in ['lexical','fused']:
  lane=base/mode;assert read(lane/'completion.json')['result']=='pass'
  lane_inputs=read(lane/'inputs.json') if (lane/'inputs.json').exists() else inputs
  for field in ['binary_sha256','resident_sha256','source_sha256','supplier_source_sha256','generation_model','embedding_model']:assert lane_inputs[field]==inputs[field]
  if (lane/'recipe').exists():
   for file in (lane/'recipe').iterdir():assert hashlib.sha256(file.read_bytes()).hexdigest()==lane_inputs['frozen_sha256'][file.name]
  assert read(lane/'origin-stop.json')['tcp_refused']
  assert read(lane/'requests-at-stop.json')==read(lane/'requests-final.json')
  phases=read(lane/'phases.json');assert len(phases)==2 and phases[1]['transfer_absent']
  for phase,name in enumerate(['warm','cold']):
   state=phases[phase];process=read(lane/name/'process.json');resident=read(lane/'resident'/f'process-{phase}.json');launch=read(lane/name/'launch.json')
   assert process['success'] and not process['timeout'] and process['binary_sha256']==inputs['binary_sha256']
   assert process['scenario_sha256']==lane_inputs['frozen_sha256'][name+'.scn']
   assert state['product_pids']==launch['product_pids'] and len(state['product_pids'])==1
   assert state['resident_pid']==resident['pid'] and resident['clean_shutdown'] and state['clean_shutdown'] and state['endpoint_unreachable'] and state['origin_refused']
   assert state['producer_paths_absent']
   done=(lane/name/'scenario.done').read_text();assert done.startswith('RESULT ok') and 'blank=0' in done
   all_pids+=state['product_pids']+[resident['pid']]
  captures=read(lane/'warm/02_capture_B.source-captures.json');assert len(captures)==2
  assert all(c['keep'] and c['phase']=='saved' and c['error'] is None for c in captures)
  source_by_label={};scope=read(lane/'resident/mixed-0-capture.json')['recall']['scope'];assert len(scope)==4
  session=None
  for source in scope:
   assert source['kind']=='fleece';scheme,rest=source['source'].split('://',1);kind,path=rest.split('/',1);assert scheme=='mere'
   parts=path.split('/');assert len(parts)==5
   s,graph,node,manifest,text_hash=parts;session=session or s;assert session==s and node==source['node'] and source['owner']==f'turnstone/{s}/research/v1'
   if kind=='saved-source':
    c=next(c for c in captures if c['stored']['annotation_manifest']=='blake3:'+manifest)
    label='capture' if c['url'].endswith('/A') else 'control';assert c['url'].endswith('/A' if label=='capture' else '/B')
    assert (c['session'],c['graph'],c['node'])==(s,graph,node)
   else:
    assert kind=='retained-text'
    label=next(k for k in ['knot','note'] if read(lane/'completion.json')['addresses'][k]=='mere://clip/'+manifest)
   assert label not in source_by_label and digest(cases['corpus'][label]['text'])=='blake3:'+text_hash
   source_by_label[label]=source
  assert set(source_by_label)==set(cases['corpus'])
  if mode=='fused':
   acknowledgements=lane/'selection-acknowledgements' if (lane/'selection-acknowledgements').exists() else lane/'control'
   ordered=['capture','control','knot','note']
   for count in range(1,5):
    ack=read(acknowledgements/f'selected-{count}.json')['source'];assert ack['session']==session and len(ack['chosen'])==count
    assert {t['text_hash'] for t in ack['chosen']}=={digest(cases['corpus'][k]['text']) for k in ordered[:count]}
    assert {(t.get('key') or t['clip'])['node'] for t in ack['chosen']}=={source_by_label[k]['node'] for k in ordered[:count]}
  ids={label:passage_id(source,cases['corpus'][label]['text']) for label,source in source_by_label.items()};labels={v:k for k,v in ids.items()}
  snapshots=[read(lane/'resident'/f'snapshot-{phase}.json') for phase in [0,1]];answer_checks={}
  for case in cases['asks']:
   label=case['id'];text=cases['corpus'][label]['text'];warm=read(lane/'resident'/f'mixed-0-{label}.json');cold=read(lane/'resident'/f'mixed-1-{label}.json');history=read(lane/'resident'/f'history-1-{label}.json')
   assert history==warm and warm['run']!=cold['run']
   assert warm['recall']['passages']==cold['recall']['passages']
   for run in [warm,cold]:
    recall=run['recall'];assert run['state']=='finished' and run['question']==case['question'] and recall['scope']==scope and not recall['stale']
    assert recall['config']['max_passages']==3 and recall['config']['chunk_chars']==512 and recall['config']['min_quote_chars']==32
    assert ' '.join(text.split()) in ' '.join(run['text'].split())
    assert recall['citations'] and not recall.get('unknown_citations',[])
    supplied={v['handle']:v['passage'] for v in recall['passages']};assert 1<=len(supplied)<=3
    target=[]
    for cite in recall['citations']:
     passage=supplied[cite['handle']];key=passage_id(passage['source'],passage['text']);assert cite['passage']==key and key in labels
     expected=cases['corpus'][labels[key]]['text'];assert passage['text']==expected and passage['source']==source_by_label[labels[key]]
     assert passage['version']==passage['text_hash']==digest(expected) and passage['selector']=={'start':0,'end':len(expected)}
     selector=cite['answer_selector'];quote=run['text'][selector['start']:selector['end']]
     if cite['basis']=='verbatim_passage':assert quote==expected
     else:assert cite['basis']=='model_marker' and quote==f"[{cite['handle']}]"
     if key==ids[label]:target.append(cite)
    assert target,'the requested provenance must have an admitted citation'
    ranking=next(v for v in read(lane/'resident/mixed-rankings-0.json')['rows'] if v['id']==label)
    assert [passage_id(p['source'],p['text']) for p in supplied.values()]==ranking['fused'][:len(supplied)]
   answer_checks[label]={name:{'target_text_occurrences':run['text'].count(text),'citation_bases':[c['basis'] for c in run['recall']['citations']],'non_target_citation_sources':[labels[c['passage']] for c in run['recall']['citations'] if c['passage']!=ids[label]],'boundary':'Identity and range validation does not certify that every model-authored reference is semantically relevant.'} for name,run in [('warm',warm),('cold',cold)]}
   old=next(v for v in snapshots[0]['evidence'] if v['record']['run']==warm['run']);restored=next(v for v in snapshots[1]['evidence'] if v['record']['run']==warm['run']);assert old==restored
   documents=[]
   for relative in [f'warm/{label}_citation',f'cold/{label}_historical_citation',f'cold/{label}_citation']:
    observation=read(lane/(relative+'.saved-sources.json'));assert observation['session']==session and len(observation['admissions'])==2 and len(observation['retained_admissions'])==2
    if label=='capture':
     reader=next(r for r in observation['readers'] if r['phase']=='Ready' and r['target']['key']['node']==source_by_label[label]['node'] and r['target']['selector']=={'start':0,'end':len(text)});assert reader['error'] is None and reader['engine']=='turnstone.saved-source'
     assert reader['content']['text']==text and reader['target']['text_hash']==digest(text)
     assert reader['target']['selector']=={'start':0,'end':len(text)}
    else:
     reader=next(r for r in observation['retained_readers'] if r['node']==source_by_label[label]['node']);assert reader['engine']=='fleece.retained-clip'
    document=reader['rendered_document'];assert text in [v['Text'] for v in objects(document) if 'Text' in v]
    if label=='note':assert 'Source recording unavailable in this clip' in json.dumps(document)
    documents.append(document)
   assert documents[0]==documents[1]==documents[2]
  ranks=[read(lane/'resident'/f'mixed-rankings-{phase}.json') for phase in [0,1]];assert ranks[0]==ranks[1] and ranks[0]['scope']==scope
  rows={}
  for actual,expected in zip(ranks[0]['rows'],cases['rankings'],strict=True):
   assert actual['id']==expected['id'] and actual['question']==expected['question']
   for field in ['lexical','vector','fused']:assert len(actual[field])==len(set(actual[field])) and set(actual[field])<=set(labels)
   if mode=='lexical':assert not actual['vector'] and actual['fused']==actual['lexical']
   else:assert actual['fused']==rrf(actual['lexical'],actual['vector'])
   mapped={field:[labels[k] for k in actual[field]] for field in ['lexical','vector','fused']}
   rows[actual['id']]={'question':actual['question'],'relevant':expected['relevant'],'rankings':mapped,'metrics':{k:metrics(v,expected['relevant']) for k,v in mapped.items()}}
  comparison[mode]=rows
  assert snapshots[0]['embedding_projections']==snapshots[1]['embedding_projections']
  if mode=='fused':
   assert len(snapshots[0]['embedding_projections'])==1;projection=next(iter(snapshots[0]['embedding_projections'].values()));space=projection['envelope']['space'];assert space['dimensions']==384 and space['metric']=='cosine' and space['normalization']=='l2-f32' and space['pooling']=='attention-mask-mean'
   assert space['producer']['artifacts']==read(HERE/'embedding-artifact-blake3.json')['loaded_artifacts'] and len(projection['envelope']['corpus']['sources'])==4
   for case in cases['asks']:
    warm=read(lane/'resident'/f"mixed-0-{case['id']}.json")['recall']['embedding'];cold=read(lane/'resident'/f"mixed-1-{case['id']}.json")['recall']['embedding'];assert warm['cache']=='built' and cold['cache']=='reopened'
    for key in ['index','envelope']:assert warm[key]==cold[key]
    assert warm['lexical_weight']==warm['vector_weight']==1 and warm['rrf_k']==60 and warm['vector_bytes']<=8*1024*1024 and projection['persisted_bytes']<=8*1024*1024
  else:
   assert not snapshots[0]['embedding_projections']
   for case in cases['asks']:assert read(lane/'resident'/f"mixed-0-{case['id']}.json")['recall'].get('embedding') is None
  workspace_path=lane/'receiver-workspace.json'
  if (lane/'profile').exists():workspace_path.write_bytes((lane/'profile/sessions'/session/'workspace.json').read_bytes())
  choices=[v['chosen'] for v in objects(read(workspace_path)) if v.get('session')==session and 'view' in v and 'chosen' in v];assert len(choices)==1 and len(choices[0])==4 and sum('key' in v for v in choices[0])==sum('clip' in v for v in choices[0])==2
  for label,v in cases['corpus'].items():assert v['text'] not in workspace_path.read_text()
  for label in ['knot','note']:assert read(lane/(label+'-cold-export.mereclip'))==read(HERE/cases['corpus'][label]['package'])
  scan_path=lane/'private-store-scan.json'
  if (lane/'profile').exists():
   scan={}
   for label,files in [('clips',[f for f in (lane/'profile/retained-clips').rglob('*') if f.is_file()]),('journal',list((lane/'private-resident/data').rglob('inference.redb')))]:
    assert files;hits={key:sum(f.read_bytes().count(value['text'].encode()) for f in files) for key,value in cases['corpus'].items()};assert not any(hits.values());scan[label]={'bytes':sum(f.stat().st_size for f in files),'full_body_hits':hits}
   scan_path.write_text(json.dumps(scan,indent=2)+'\n')
  assert not any(n for group in read(scan_path).values() for n in group['full_body_hits'].values())
  images=sorted((lane/'warm').glob('*.png'))+sorted((lane/'cold').glob('*.png'));assert len(images)==26
  results[mode]={'scope':scope,'source_by_label':source_by_label,'passage_ids':ids,'cases':3,'answer_diagnostics':answer_checks,'historical_evidence_unchanged':True,'exact_local_readers_equal':True,'rankings_equal_after_cold':True,'captures':[{'path':str(f.relative_to(base)),'sha256':hashlib.sha256(f.read_bytes()).hexdigest()} for f in images]}
 assert len(all_pids)==len(set(all_pids))==8
 metrics_summary={mode:{field:{metric:sum(r['metrics'][field][metric] for r in rows.values())/len(rows) for metric in ['reciprocal_rank','top1','recall_at_3']} for field in ['lexical','vector','fused']} for mode,rows in comparison.items()}
 result={'result':'pass','process_ids':all_pids,'modes':results,'ranking_cases':comparison,'mean_metrics':metrics_summary,'generation_semantic_quality':'Not certified. Inspect answer_diagnostics: model-authored references can identify a supplied passage without semantically supporting the adjacent answer. Repeated note text and additional source markers are retained, not filtered.','scope':'Bounded native four-source publication, three provenance-specific asks, exact target citation activation, eight frozen ranking questions; real CPU MiniLM and GPU SmolLM2. Combined query measures retrieval, not a combined generated answer.'}
 (base/'verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n');print(json.dumps(metrics_summary,indent=2));print('Mixed-source native qualification verification passed')
if __name__=='__main__':main()
