#!/usr/bin/env python3
"""Independent headed source, model, projection and cold evidence checks.

Uses Python blake3; accepts a raw run or the public packet. Raw stores and
models stay outside the packet. Model SHA-256 verification occurs in the driver.
"""
import argparse, hashlib, json, runpy
from pathlib import Path
import blake3
HERE=Path(__file__).resolve().parent
helpers=runpy.run_path(str(HERE.parent/'r3_scoped_answers_20261017/verify_native.py'))
read,digest,identity,validate_document,values=[helpers[k] for k in ['read','digest','identity','validate_document','values']]
TEXT="The Pelican observatory's access word is café."
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('run',type=Path);base=p.parse_args().run
 assert read(base/'completion.json')['result']=='pass'
 assert read(base/'origin-stop.json')['tcp_refused']
 assert read(base/'requests-at-stop.json')==read(base/'requests-final.json')
 products=[];residents=[]
 for phase,name in enumerate(['seed','reopen']):
  process=read(base/name/'process.json');assert process['success'] and not process['timeout']
  done=(base/name/'scenario.done').read_text();assert done.startswith('RESULT ok') and 'blank=0' in done
  product=read(base/name/'launch.json')['product_pids'];assert len(product)==1;products+=product
  start=read(base/f'resident-start-{phase}.json');stop=read(base/f'resident-stop-{phase}.json');resident=read(base/'resident'/f'process-{phase}.json')
  assert start['origin_refused'] and stop['endpoint_unreachable'] and stop['exit_code']==0 and resident['clean_shutdown'];assert start['pid']==stop['pid']==resident['pid'];residents.append(start['pid'])
  path=base/('seed/03_read_origin_and_resident_absent.saved-sources.json' if phase==0 else 'reopen/08_cold_reader_before_resident.saved-sources.json')
  assert hashlib.sha256(path.read_bytes()).hexdigest()==start['source_read_sha256']
 assert len(set(products+residents))==4
 captures=read(base/'seed/02_B_saved.source-captures.json');assert len(captures)==2
 for c in captures:assert c['keep'] and c['phase']=='saved' and c['error'] is None
 a=next(c for c in captures if c['url'].endswith('/A'));b=next(c for c in captures if c['url'].endswith('/B'));assert a['session']==b['session']
 profile=base/'profile/sessions'/a['session'];facets=read(profile/'facets.json')['nodes'];workspace=read(profile/'workspace.json')
 for capture in captures:
  assert facets[capture['node']]['turnstone.controls.v1.keep']
  assert facets[capture['node']]['capture.source-annotations/v1']==[capture['stored']['annotation_manifest']]
 choices=[v for v in values(workspace) if v.get('session')==a['session'] and 'chosen' in v and 'view' in v]
 assert len(choices)==1 and len(choices[0]['chosen'])==2
 assert {v['key']['annotation_manifest'] for v in choices[0]['chosen']}=={c['stored']['annotation_manifest'] for c in captures}
 assert TEXT not in (profile/'workspace.json').read_text(), 'pane payload copied source text'

 warm=read(base/'resident/finished.json');cold=read(base/'resident/embedding-cold-ask.json');assert warm['state']==cold['state']=='finished' and warm['run']!=cold['run']
 assert warm['recall']['embedding']['cache']=='built' and cold['recall']['embedding']['cache']=='reopened'
 for field in ['envelope','index']:assert warm['recall']['embedding'][field]==cold['recall']['embedding'][field]
 assert warm['recall']['passages']==cold['recall']['passages']
 scope=warm['recall']['scope'];assert len(scope)==2 and cold['recall']['scope']==scope
 assert {identity(source)[0]['annotation_manifest'] for source in scope}=={c['stored']['annotation_manifest'] for c in captures}
 for run in [warm,cold]:
  recall=run['recall'];assert recall['citations'] and not recall['stale']
  for supplied in recall['passages']:
   passage=supplied['passage'];key,version=identity(passage['source']);assert key['annotation_manifest']==a['stored']['annotation_manifest']
   assert passage['source'] in scope and passage['text']==TEXT
   assert passage['version']==version==passage['text_hash']==digest(TEXT)
   assert passage['selector']=={'start':0,'end':len(TEXT)}
  for citation in recall['citations']:
   passage=next(p['passage'] for p in recall['passages'] if p['handle']==citation['handle'])
   encoded={k:passage[k] for k in ['source','version','text_hash','selector','chunker','text']}
   encoded['source']={k:passage['source'][k] for k in ['kind','owner','source','node']}
   assert citation['passage']==digest(json.dumps(encoded,ensure_ascii=False,separators=(',',':')))
   if citation['basis']=='verbatim_passage':
    selector=citation['answer_selector'];assert run['text'][selector['start']:selector['end']]==TEXT
 readers=[]
 for path in ['seed/07_exact_retained_citation.saved-sources.json','reopen/11_cold_historical_citation.saved-sources.json','reopen/13_new_cold_ask_exact_citation.saved-sources.json']:
  observation=read(base/path);r=next(r for r in observation['readers'] if r['phase']=='Ready' and r['target']['key']['annotation_manifest']==a['stored']['annotation_manifest'])
  assert r['error'] is None and r['engine']=='turnstone.saved-source';assert r['content']['text']==TEXT and r['target']['text_hash']==digest(TEXT);assert validate_document(r)==TEXT;readers.append(r)
 for r in readers[1:]:assert r['target']==readers[0]['target'] and r['content']==readers[0]['content']
 snapshots=[read(base/'resident'/f'snapshot-{i}.json') for i in [0,1]]
 assert snapshots[0]['embedding_projections']==snapshots[1]['embedding_projections']
 assert len(snapshots[0]['embedding_projections'])==1
 projection=next(iter(snapshots[0]['embedding_projections'].values()));envelope=projection['envelope'];space=envelope['space']
 assert envelope['version']==1 and space['dimensions']==384 and space['metric']=='cosine' and space['normalization']=='l2-f32'
 assert len(envelope['corpus']['sources'])==2 and {r['source']['source'] for r in envelope['corpus']['sources']}=={s['source'] for s in scope}
 assert space['producer']['artifacts']==read(base/'embedding-artifact-blake3.json')['loaded_artifacts']
 assert space['pooling']=='attention-mask-mean' and space['query_prefix']==space['document_prefix']==''
 for run in [warm,cold]:
  e=run['recall']['embedding'];assert e['vector_bytes']<=8*1024*1024 and e['persisted_bytes']==projection['persisted_bytes']<=8*1024*1024
 old=next(e for e in snapshots[0]['evidence'] if e['record']['run']==warm['run']);restored=next(e for e in snapshots[1]['evidence'] if e['record']['run']==warm['run']);assert old==restored
 # Independent raw journal scan is performed before copying a public packet.
 scan_path=base/'private-journal-scan.json'
 if not scan_path.exists():
  stores=list((base/'private-resident/data').rglob('inference.redb'));assert len(stores)==1;data=stores[0].read_bytes()
  needles=['"vectors"','"producer"',"The Pelican observatory's access word is café."]
  scan={'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),'needles':{n:data.count(n.encode()) for n in needles}};assert not any(scan['needles'].values());scan_path.write_text(json.dumps(scan,indent=2)+'\n')
 assert not any(read(scan_path)['needles'].values())
 images=sorted(base.glob('seed/*.png'))+sorted(base.glob('reopen/*.png'));assert len(images)==13
 result={'passed':True,'product_pids':products,'resident_pids':residents,'origin_requests_unchanged':True,'historical_sealed_evidence_unchanged':True,'exact_source_selectors_preserved':True,'keep_capture_refs_and_chosen_scope_preserved':True,'workspace_contains_no_source_body':True,'matching_private_projection_reopened':True,'fresh_cold_ask':True,'embedding':{'warm':warm['recall']['embedding'],'cold':cold['recall']['embedding'],'space':space},'source_quote':TEXT,'captures':[{'path':str(p.relative_to(base)),'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in images],'scope':'macOS headed native retained-source flow; real CPU MiniLM retrieval and GPU SmolLM2 generation; no second-device or broader benchmark claim'}
 (base/'verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n');print('headed embedding recall verification passed')
if __name__=='__main__':main()
