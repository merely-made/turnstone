#!/usr/bin/env python3
"""Verify R4 task custody, real prompts, process boundaries and exact readers."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy

HERE=Path(__file__).resolve().parent
r3=runpy.run_path(str(HERE.parent/'r3_scoped_answers_20261017/verify_native.py'))
read,digest,identity,values,validate_document=[r3[key] for key in ['read','digest','identity','values','validate_document']]
TEXT="The Pelican observatory's access word is café."
def write(path,value):path.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n')
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('run',type=Path);args=parser.parse_args();base=args.run
    assert read(base/'completion.json')['result']=='pass'
    stopped=[read(base/f'origin-stop-{i}.json') for i in range(2)]
    for record in stopped:assert record['exited'] and record['tcp_refused']
    origins=[read(base/f'origin-process-{i}.json') for i in range(2)]
    assert origins[0]['origin']==origins[1]['origin'] and origins[0]['pid']!=origins[1]['pid']
    assert read(base/'requests-at-stop-1.json')==read(base/'requests-final.json')
    restart_requests=read(base/'requests-at-stop-1.json')['requests'][len(read(base/'requests-at-stop-0.json')['requests']):]
    assert any(r['path']=='/swap-a' for r in restart_requests)
    for i in range(2):
        start=read(base/f'resident-start-{i}.json')
        assert start['prior_resident_exited'] and start['origin_refused_before_start'] and start['resident_endpoint_absent_before_start']
        assert hashlib.sha256((base/start['source_read_observation']).read_bytes()).hexdigest()==start['source_read_sha256']
        stop=read(base/f'resident-stop-{i}.json');proc=read(base/f'resident/process-{i}.json')
        assert stop['exited'] and stop['endpoint_unreachable'] and stop['exit_code']==0
        assert proc['pid']==stop['pid'] and proc['clean_shutdown']
    assert read(base/'resident-stop-0.json')['pid']!=read(base/'resident-stop-1.json')['pid']
    probe=read(base/'wrong-owner-probe.json');assert probe['actual_admitted_route'] and probe['wrong_owner_publication_refused'] and probe['wrong_owner_ask_refused']
    for phase in ['seed','reopen']:
        assert read(base/phase/'process.json')['success']
        assert (base/phase/'scenario.done').read_text().startswith('RESULT ok')
        inspection=read(base/(phase+'-store-inspection.json'));assert inspection['independent_after_app_exit'] and inspection['result']=='pass'
        assert all(s['page_text_references']==0 for s in inspection['sessions'])
    captures=read(base/'seed/11_A2_saved.source-captures.json');assert len(captures)==3
    initial_a=read(base/'seed/01_A1_saved.source-captures.json');assert len(initial_a)==1
    initial_a=initial_a[0]
    initial_b=next(c for c in read(base/'seed/02_B_saved.source-captures.json') if c['url'].endswith('/B'))
    # Recovery can renumber transient display ordinals. Immutable saved IDs,
    # owning members and the ordered facet series are the durable contract.
    a1=next(c for c in captures if c['stored']['annotation_manifest']==initial_a['stored']['annotation_manifest'])
    b=next(c for c in captures if c['stored']['annotation_manifest']==initial_b['stored']['annotation_manifest'])
    a2=next(c for c in captures if c not in [a1,b])
    assert a1['stored']==initial_a['stored'] and b['stored']==initial_b['stored']
    assert all(c['phase']=='saved' and c['keep'] and not c['error'] for c in captures+[initial_a,initial_b])
    assert a1['node']==a2['node'] and a1['session']==b['session']==a2['session']
    m1,mb,m2=[c['stored']['annotation_manifest'] for c in [a1,b,a2]];assert len({m1,mb,m2})==3
    session=a1['session'];profile=base/'profile/sessions'/session
    graph=read(profile/'graph.json');facets=read(profile/'facets.json')['nodes']
    assert facets[a1['node']]['capture.source-annotations/v1']==[m1,m2]
    assert facets[b['node']]['capture.source-annotations/v1']==[mb]
    assert all(facets[n]['turnstone.controls.v1.keep'] for n in [a1['node'],b['node']])
    x=next(n for n in graph['nodes'] if n['url'].endswith('/X'))
    assert x['title'] and x['body'] is None
    assert not facets.get(x['node_id'],{}).get('turnstone.controls.v1.keep')
    assert not facets.get(x['node_id'],{}).get('capture.source-annotations/v1')
    saved=read(base/'saved-before-cold'/session/'workspace.json');final=read(profile/'workspace.json')
    for w in [saved,final]:
        choices=[v for v in values(w) if v.get('session')==session and 'chosen' in v and 'view' in v]
        assert len(choices)==1 and len(choices[0]['chosen'])==1
        assert choices[0]['chosen'][0]['key']['annotation_manifest']==mb
        assert 'café' not in json.dumps(w,ensure_ascii=False)
    assert saved['layout']['root']['Workspace']['workspace']==final['layout']['root']['Workspace']['workspace'], 'arrangement changed on reentry/reopen'
    observations=[read(base/path) for path in ['seed/04_A1_search_read_no_origin_no_resident.saved-sources.json','seed/10_exact_A1_citation.saved-sources.json','seed/14_old_A1_after_A2_and_scope_change.saved-sources.json','reopen/15_cold_reader_before_resident_restart.saved-sources.json','reopen/18_cold_exact_A1_citation.saved-sources.json']]
    contents=[];targets=[]
    for i,o in enumerate(observations):
        expected='café' if i in [0,3] else TEXT
        selector={'start':TEXT.index(expected),'end':TEXT.index(expected)+len(expected)}
        matches=[r for r in o['readers'] if r['phase']=='Ready' and r['target']['key']['annotation_manifest']==m1 and r['target']['selector']==selector]
        assert len(matches)==1, 'expected exact retained reader, independent of other cached readers'
        ready=matches[0]
        assert ready['engine']=='turnstone.saved-source'
        assert ready['content']['text']==TEXT and digest(TEXT)==ready['target']['text_hash']==ready['content']['text_hash']
        quote=validate_document(ready);assert quote==expected
        contents.append(ready['content']);targets.append(ready['target'])
    assert all(c==contents[0] for c in contents) and targets[1]==targets[2]==targets[4]
    assert observations[0]['generation']!=observations[3]['generation']
    restored=next(r for r in observations[3]['readers'] if r['phase']=='Ready' and r['target']==targets[2])
    assert restored['engine']=='turnstone.saved-source' and validate_document(restored)==TEXT
    # Each packet's independent inspector verifies actual Fleece bytes through
    # the library, not merely copied observation JSON.
    for phase in ['seed','reopen']:
        owner=next(s for s in read(base/(phase+'-store-inspection.json'))['sessions'] if s['session']==session)
        entries=owner['validated_captures'];assert len(entries)==3
        assert {e['admission']['key']['annotation_manifest'] for e in entries}=={m1,mb,m2}
        assert all(e['content'] and not e['error'] for e in entries)
        assert next(e['content'] for e in entries if e['admission']['key']['annotation_manifest']==m1)==contents[0]
    receipt=read(base/'resident/resident-receipt.json');first,second=receipt['snapshots'];assert first['evidence']==second['evidence']
    live=read(base/'resident/finished.json');assert live==read(base/'resident/reopened.json')
    assert receipt['pause_observed'] and receipt['streaming_observed']
    assert any(c['reason']=='user' and c['at_token']>0 for c in live['continuations'])
    runs=first['runs']['runs'];assert len(runs)==2
    for run in runs:
        answered=run['state']=='finished';assert answered or run['state']=='cancelled'
        recall=live['recall'] if answered else run['recall'];scope=recall['scope']
        assert {identity(source)[0]['annotation_manifest'] for source in scope}==({m1,mb} if answered else {mb})
        record=next(e['record'] for e in first['evidence'] if e['record']['run']==run['run'])
        prompt=record['request']['prompt']
        for forbidden in ['unkept orchid-only sentinel','copperfield','Live A2 replacement']:
            assert forbidden not in prompt and forbidden not in json.dumps(recall,ensure_ascii=False)
        if not answered:assert 'café' not in prompt
        headers=[json.loads(line.split('] ',1)[1]) for line in prompt.splitlines() if line.startswith('[') and '] [' in line]
        assert len(headers)==len(recall['passages'])
        for source,version,selector in headers:
            assert source in scope and any(p['passage']['source']==source and p['passage']['version']==version and p['passage']['selector']==selector for p in recall['passages'])
        for supplied in recall['passages']:
            passage=supplied['passage'];assert passage['source'] in scope
            key,version=identity(passage['source']);assert key['session']==session
            full=TEXT if key['annotation_manifest']==m1 else 'amber harbor main-session sentinel'
            selector=passage['selector'];assert 0<=selector['start']<selector['end']<=len(full)
            assert passage['text']==full[selector['start']:selector['end']]
            assert version==passage['version']==passage['text_hash']==digest(full)
            source=passage['source'];encoded={'source':{k:source[k] for k in ['kind','owner','source','node']},'version':version,'text_hash':version,'selector':{k:selector[k] for k in ['start','end']},'chunker':passage['chunker'],'text':passage['text']}
            for citation in recall.get('citations',[]):
                if citation['handle']==supplied['handle']:assert citation['passage']==digest(json.dumps(encoded,ensure_ascii=False,separators=(',',':')))
    assert live['recall']['citations'] and all(c['basis']=='verbatim_passage' for c in live['recall']['citations'])
    for citation in live['recall']['citations']:
        passage=next(p['passage'] for p in live['recall']['passages'] if p['handle']==citation['handle'])
        selector=citation['answer_selector'];assert 0<=selector['start']<selector['end']<=len(live['text'])
        assert live['text'][selector['start']:selector['end']]==passage['text']

    huge=read(base/'reopen/20_oversize_source_readable_offline.saved-sources.json');hr=next(r for r in huge['readers'] if r['phase']=='Ready')
    validate_document(hr);assert hr['content']['text'].count('\\')==50000 and digest(hr['content']['text'])==hr['target']['text_hash']
    pngs=sorted(base.glob('seed/*.png'))+sorted(base.glob('reopen/*.png'));assert len(pngs)==19
    write(base/'verification.json',{'result':'pass','workflow':'collect-find-inspect-ask-verify-return','fresh_app_processes':2,'fresh_resident_processes':2,'source_read_before_resident_start_and_restart':True,'restored_reader_intent_ready_before_resident_restart':True,'same_origin_address_restarted_only_for_A2':True,'origin_gone_on_cold_reopen':True,'new_page_text_bodies':0,'keep_and_exact_capture_refs_preserved':True,'arrangement_and_single_answers_view_preserved':True,'exact_scopes':[[m1,mb],[mb]],'generation_runs':2,'exact_old_A1_after_A2_and_cold_reopen':True,'citation_basis':'verbatim_passage','non_ascii_search':'café','full_quote':TEXT,'oversize_refusal_preserves_full_reader':True,'pngs':[{'path':str(p.relative_to(base)),'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in pngs]})
    print('R4 independent verification passed')
if __name__=='__main__':main()
