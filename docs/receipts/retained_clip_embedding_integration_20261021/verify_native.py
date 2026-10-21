#!/usr/bin/env python3
"""Independently verify selected-only recall, native local readers and cold reuse."""
import argparse, hashlib, json
from pathlib import Path
import blake3
HERE=Path(__file__).resolve().parent
def read(path):return json.loads(path.read_text())
def digest(value):return 'blake3:'+blake3.blake3(value if isinstance(value,bytes) else value.encode()).hexdigest()
def objects(value):
 if isinstance(value,dict):
  yield value
  for item in value.values():yield from objects(item)
 elif isinstance(value,list):
  for item in value:yield from objects(item)
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('run',type=Path);base=p.parse_args().run
 inputs=read(base/'inputs.json')
 assert read(base/'completion.json')['result']=='pass'
 results={};all_pids=[]
 for kind in ['knot','redshank']:
  lane=base/kind;package=read(HERE/(kind+'.mereclip'));assert len(package['payloads'])==1
  text=bytes(package['payloads'][0]['bytes']).decode();version=digest(text)
  assert 'PRIVATE' not in text and 'UNSELECTED' not in text
  address='mere://clip/'+digest(json.dumps({'clip':package['clip'],'private_backing':[]},ensure_ascii=False,separators=(',',':'))).split(':')[1]
  assert read(lane/'completion.json')=={'result':'pass','source_and_transfer_absent_before_cold':True,'export_equal_original':True,'address':address}
  phases=read(lane/'phases.json');assert len(phases)==2 and phases[1]['transfer_absent']
  for phase,name in enumerate(['warm','cold']):
   state=phases[phase];process=read(lane/name/'process.json');resident=read(lane/'resident'/f'process-{phase}.json')
   assert state['clean_shutdown'] and state['endpoint_unreachable'] and process['success'] and not process['timeout']
   assert resident['clean_shutdown'] and state['resident_pid']==resident['pid']
   assert process['binary_sha256']==inputs['binary_sha256']
   assert process['scenario_sha256']==inputs['frozen_sha256'][name+'.scn']
   assert state['product_pids']==read(lane/name/'launch.json')['product_pids']
   assert len(state['product_pids'])==1
   all_pids+=state['product_pids']+[state['resident_pid']]
   assert (lane/name/'scenario.done').read_text().startswith('RESULT ok')
   assert state['producer_paths_absent']
  warm=read(lane/'resident/finished.json');cold=read(lane/'resident/embedding-cold-ask.json')
  assert warm['run']!=cold['run'] and warm['state']==cold['state']=='finished'
  assert warm['recall']['embedding']['cache']=='built' and cold['recall']['embedding']['cache']=='reopened'
  for field in ['envelope','index']:assert warm['recall']['embedding'][field]==cold['recall']['embedding'][field]
  assert warm['recall']['passages']==cold['recall']['passages']
  scope=warm['recall']['scope'];assert len(scope)==1 and cold['recall']['scope']==scope
  source=scope[0];assert source['kind']=='fleece' and source['source'].startswith('mere://retained-text/')
  parts=source['source'].removeprefix('mere://retained-text/').split('/');assert len(parts)==5
  session,graph,node,manifest,text_digest=parts;assert source['owner']==f'turnstone/{session}/research/v1' and source['node']==node
  assert manifest==address.rsplit('/',1)[1] and version=='blake3:'+text_digest
  for run in [warm,cold]:
   recall=run['recall'];assert recall['citations'] and not recall['stale']
   assert recall['config']['min_quote_chars']==32 and recall['config']['chunk_chars']==512 and recall['config']['max_passages']==1
   assert ' '.join(text.split()) in ' '.join(run['text'].split())
   for supplied in recall['passages']:
    passage=supplied['passage'];assert passage['source']==source and passage['version']==passage['text_hash']==version
    assert passage['selector']=={'start':0,'end':len(text)} and passage['text']==text
   for citation in recall['citations']:
    passage=next(v['passage'] for v in recall['passages'] if v['handle']==citation['handle'])
    record={k:passage[k] for k in ['source','version','text_hash','selector','chunker','text']};record['source']={k:source[k] for k in ['kind','owner','source','node']}
    assert citation['passage']==digest(json.dumps(record,ensure_ascii=False,separators=(',',':')))
    if citation['basis']=='verbatim_passage':
     selector=citation['answer_selector'];assert run['text'][selector['start']:selector['end']]==text
  observations=[]
  for name in ['warm/01_retained_body','warm/04_exact_clip_citation','cold/05_source_and_package_absent','cold/07_historical_citation','cold/09_new_cold_citation']:
   observation=read(lane/(name+'.saved-sources.json'));assert observation['session']==session
   assert observation['retained_admissions']==[{'session':session,'graph':graph,'node':node,'manifest':'blake3:'+manifest}]
   reader=next(r for r in observation['retained_readers'] if r['rendered_document']['address']==address)
   assert reader['node']==node and reader['engine']=='fleece.retained-clip'
   document=reader['rendered_document'];assert document['address']==address
   # The actual native reader session, not a second regenerated document.
   strings=[v['Text'] for v in objects(document) if 'Text' in v]
   assert text in strings
   observations.append(document)
  assert all(d==observations[0] for d in observations)
  if kind=='redshank':assert 'Source recording unavailable in this clip' in json.dumps(observations[0])
  snapshots=[read(lane/'resident'/f'snapshot-{phase}.json') for phase in [0,1]]
  assert snapshots[0]['embedding_projections']==snapshots[1]['embedding_projections'] and len(snapshots[0]['embedding_projections'])==1
  projection=next(iter(snapshots[0]['embedding_projections'].values()));envelope=projection['envelope'];space=envelope['space']
  assert space['dimensions']==384 and space['metric']=='cosine' and space['normalization']=='l2-f32' and space['pooling']=='attention-mask-mean'
  assert len(envelope['corpus']['sources'])==1 and envelope['corpus']['sources'][0]['source']==source
  assert space['producer']['artifacts']==read(base/'embedding-artifact-blake3.json')['loaded_artifacts']
  for run in [warm,cold]:
   e=run['recall']['embedding'];assert e['vector_bytes']<=8*1024*1024 and e['persisted_bytes']==projection['persisted_bytes']<=8*1024*1024
   assert e['lexical_weight']==e['vector_weight']==1
  old=next(e for e in snapshots[0]['evidence'] if e['record']['run']==warm['run']);restored=next(e for e in snapshots[1]['evidence'] if e['record']['run']==warm['run']);assert old==restored
  workspace_path=lane/'receiver-workspace.json'
  if (lane/'profile').exists():
   profile=lane/'profile/sessions'/session
   workspace_path.write_bytes((profile/'workspace.json').read_bytes())
  workspace=read(workspace_path)
  choices=[v['chosen'] for v in objects(workspace) if v.get('session')==session and 'view' in v and 'chosen' in v]
  assert len(choices)==1 and choices[0]==[{'clip':{'session':session,'graph':graph,'node':node,'manifest':'blake3:'+manifest},'text_hash':version,'selector':None}]
  assert text not in workspace_path.read_text()
  assert read(lane/'cold-export.mereclip')==package
  if (lane/'profile').exists():
   scan={}
   for group,files in [('clip',list((lane/'profile/retained-clips').rglob('*'))),('journal',list((lane/'private-resident/data').rglob('inference.redb')))]:
    files=[f for f in files if f.is_file()];assert files
    scan[group]={'bytes':sum(f.stat().st_size for f in files),'clear_text_hits':sum(f.read_bytes().count(text.encode()) for f in files)}
    assert scan[group]['clear_text_hits']==0
   (lane/'private-store-scan.json').write_text(json.dumps(scan,indent=2)+'\n')
  scan=read(lane/'private-store-scan.json');assert all(v['clear_text_hits']==0 for v in scan.values())
  images=sorted((lane/'warm').glob('*.png'))+sorted((lane/'cold').glob('*.png'));assert len(images)==10
  results[kind]={'selected_text':text,'source':source,'version':version,'original_anchors':package['clip']['anchors'],'warm':warm['recall']['embedding'],'cold':cold['recall']['embedding'],'historical_evidence_unchanged':True,'native_documents_equal':True,'only_selected_body_in_corpus':True,'cold_selected_export_equal':True,'private_store_scan':scan,'captures':[{'path':str(i.relative_to(base)),'sha256':hashlib.sha256(i.read_bytes()).hexdigest()} for i in images]}
 assert len(all_pids)==len(set(all_pids))==8
 result={'result':'pass','process_ids':all_pids,'lanes':results,'scope':'macOS native receiver imports of previously qualified embedded producer packages; real MiniLM fused recall, GPU generation and separate product/resident cold processes'}
 (base/'verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n');print('retained clip embedding verification passed')
if __name__=='__main__':main()
