#!/usr/bin/env python3
"""Independently compare R3 capture admission, ask scope, prompt and exact readers.

Requires Python blake3 (the isolated R2 verifier environment is reusable).
Accepts a raw run or the committed packet with public profile sidecars.
"""
import argparse
import hashlib
import json
from pathlib import Path
from urllib.parse import urlsplit
import blake3

def read(path): return json.loads(path.read_text())
def digest(text): return 'blake3:'+blake3.blake3(text.encode()).hexdigest()
def identity(source):
    parsed=urlsplit(source['source']); parts=parsed.path.strip('/').split('/')
    assert parsed.scheme=='mere' and parsed.netloc=='saved-source' and len(parts)==5
    session,graph,node,annotation,text_hash=parts
    assert source['kind']=='fleece' and source['node']==node
    assert source['owner']==f'turnstone/{session}/research/v1'
    return {'session':session,'graph':graph,'node':node,'annotation_manifest':'blake3:'+annotation},'blake3:'+text_hash

def values(value):
    if isinstance(value,dict):
        yield value
        for item in value.values(): yield from values(item)
    elif isinstance(value,list):
        for item in value: yield from values(item)

def validate_document(reader):
    document=reader['rendered_document'];assert document is not None
    assert document['address']==reader['url'] and document['provenance']['source_kind']=='turnstone.saved-source'
    blocks=document['blocks'];quote_index=next(i for i,b in enumerate(blocks) if 'Quote' in b)
    quote_parts=[span['Text'] for paragraph in blocks[quote_index]['Quote']['blocks'] for strong in paragraph['Paragraph']['spans'] for span in strong['Strong']]
    body_parts=[span['Text'] for paragraph in blocks[quote_index+1:-1] for span in paragraph['Paragraph']['spans']]
    assert all(len(part.encode())<=4096 for part in quote_parts+body_parts)
    assert ''.join(body_parts)==reader['content']['text'], 'display segmentation lost canonical characters'
    selector=reader['target']['selector'];assert ''.join(quote_parts)==reader['content']['text'][selector['start']:selector['end']]
    return ''.join(quote_parts)

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('run',type=Path);a=parser.parse_args();base=a.run
    probe=read(base/'wrong-owner-probe.json');assert probe['actual_admitted_route'] and probe['wrong_owner_publication_refused'] and probe['wrong_owner_ask_refused']
    stopped=read(base/'origin-stop.json');assert stopped['exited'] and stopped['tcp_refused']
    assert read(base/'requests-at-origin-stop.json')==read(base/'requests-final.json')
    for phase in ['seed','reopen']:
        process=read(base/phase/'process.json');assert process['success']
        done=(base/phase/'scenario.done').read_text();assert done.startswith('RESULT ok') and 'blank=0' in done
    captures=read(base/'seed/03_A2_saved.source-captures.json')
    a1,b,a2=sorted(captures,key=lambda item:item['ordinal'])
    for capture in [a1,b,a2]: assert capture['phase']=='saved' and capture['keep'] and capture['error'] is None
    assert a1['session']==b['session']==a2['session'] and a1['node']==a2['node']
    m1,mb,m2=[c['stored']['annotation_manifest'] for c in [a1,b,a2]]
    assert len({m1,mb,m2})==3
    session=a1['session'];profile=base/'profile/sessions'/session
    facets=read(profile/'facets.json')['nodes'];graph=read(profile/'graph.json')
    assert facets[a1['node']]['capture.source-annotations/v1']==[m1,m2]
    assert facets[b['node']]['capture.source-annotations/v1']==[mb]
    visited=[n for n in graph['nodes'] if n['url'].endswith('/X')];assert len(visited)==1
    assert not facets.get(visited[0]['node_id'],{}).get('capture.source-annotations/v1')
    choices=[v for v in values(read(profile/'workspace.json')) if v.get('session')==session and 'chosen' in v and 'view' in v]
    assert len(choices)==1 and len(choices[0]['chosen'])==1
    assert choices[0]['chosen'][0]['key']['annotation_manifest']==mb
    assert 'café' not in (profile/'workspace.json').read_text(), 'pane state must not copy the corpus'
    seed=read(base/'seed/09_exact_A1_citation_origin_gone.saved-sources.json')
    cold=read(base/'reopen/11_cold_reopened_exact_A1_citation.saved-sources.json')
    assert seed['session']==cold['session']==session and seed['generation']!=cold['generation']
    def reader(observation):
        available=[r for r in observation['readers'] if r['phase']=='Ready' and r['target']['key']['annotation_manifest']==m1]
        assert len(available)==1
        r=available[0];assert r['engine']=='turnstone.saved-source' and r['error'] is None
        assert digest(r['content']['text'])==r['target']['text_hash']==r['content']['text_hash']
        validate_document(r)
        selector=r['target']['selector'];quote=r['content']['text'][selector['start']:selector['end']]
        assert quote == r['content']['text'] == "The Pelican observatory's access word is café."
        assert 'Live A2 replacement' not in r['content']['text']
        return r,quote
    sr,quote=reader(seed);cr,cold_quote=reader(cold)
    assert sr['target']==cr['target'] and sr['content']==cr['content'] and quote==cold_quote
    receipt=read(base/'resident/resident-receipt.json');assert receipt['streaming_observed'] and receipt['pause_observed']
    first,second=receipt['snapshots'];assert first['evidence']==second['evidence'], 'sealed prompt/source evidence changed on reopen'
    runs=first['runs']['runs'];assert len(runs)==2, 'empty or oversize scope unexpectedly admitted generation'
    answered=next(r for r in runs if r['state']=='finished');cancelled=next(r for r in runs if r['state']=='cancelled')
    live=read(base/'resident/finished.json');live_cold=read(base/'resident/reopened.json')
    assert live==live_cold and live['run']==answered['run']
    assert 'café' in live['text'] and live['recall'].get('citations')
    assert any(c['reason']=='user' and c['at_token']>0 for c in live['continuations'])
    assert all(c['basis']=='verbatim_passage' for c in live['recall']['citations'])
    for citation in live['recall']['citations']:
        supplied=next(p for p in live['recall']['passages'] if p['handle']==citation['handle'])
        selector=citation['answer_selector'];assert 0<=selector['start']<selector['end']<=len(live['text'])
        assert live['text'][selector['start']:selector['end']]==supplied['passage']['text']
    assert {identity(s)[0]['annotation_manifest'] for s in answered['recall']['scope']}=={m1,mb}
    assert {identity(s)[0]['annotation_manifest'] for s in cancelled['recall']['scope']}=={mb}
    assert any(identity(s['passage']['source'])[0]['annotation_manifest']==m1 for s in answered['recall']['passages'])
    for run in runs:
        recall=live['recall'] if run['state']=='finished' else run['recall'];assert recall['owner']==f'turnstone/{session}/research/v1'
        allowed=recall['scope'];assert allowed
        record=next(e for e in first['evidence'] if e['record']['run']==run['run'])
        prompt=record['record']['request']['prompt']
        if run['state']=='cancelled': assert 'café' not in prompt, 'B-only ask leaked A1 text'
        for forbidden in ['unkept orchid-only sentinel','copperfield','Live A2 replacement']:
            assert forbidden not in prompt and forbidden not in json.dumps(recall,ensure_ascii=False)
        headers=[json.loads(line.split('] ',1)[1]) for line in prompt.splitlines() if line.startswith('[') and '] [' in line]
        assert len(headers)==len(recall['passages'])
        for source,version,selector in headers:
            assert source in allowed
            assert any(p['passage']['source']==source and p['passage']['version']==version and p['passage']['selector']==selector for p in recall['passages'])
        for supplied in recall['passages']:
            passage=supplied['passage'];assert passage['source'] in allowed
            key,version=identity(passage['source']);assert key['session']==session and version==passage['version']==passage['text_hash']
            full=sr['content']['text'] if key['annotation_manifest']==m1 else 'amber harbor main-session sentinel'
            assert digest(full)==version
            selector=passage['selector'];assert 0<=selector['start']<selector['end']<=len(full)
            assert full[selector['start']:selector['end']]==passage['text']
            # Reproduce Rust struct field order and UTF-8 passage serialization.
            source=passage['source'];encoded={'source':{k:source[k] for k in ['kind','owner','source','node']},'version':passage['version'],'text_hash':passage['text_hash'],'selector':{k:selector[k] for k in ['start','end']},'chunker':passage['chunker'],'text':passage['text']}
            passage_id=digest(json.dumps(encoded,ensure_ascii=False,separators=(',',':')))
            for citation in recall.get('citations',[]):
                if citation['handle']==supplied['handle']: assert citation['passage']==passage_id
    huge=read(base/'reopen/13_oversize_source_readable_offline.saved-sources.json')
    assert huge['session']!=session
    huge_reader=next(r for r in huge['readers'] if r['phase']=='Ready' and 'oversize saved marker' in r['content']['text'])
    validate_document(huge_reader)
    assert huge_reader['content']['text'].count('\\')==50000
    assert len(json.dumps(huge_reader['content']['text']).encode())>128*1024
    assert digest(huge_reader['content']['text'])==huge_reader['target']['text_hash']
    pngs=list(base.glob('seed/*.png'))+list(base.glob('reopen/*.png'));assert len(pngs)>=10
    result={'result':'pass','real_model':True,'citation_basis':'verbatim_passage','resumed_after_positive_token':True,'exact_scopes':[[m1,mb],[mb]],'old_A1_citation_survives_B_only_selection':True,'cold_generation_changed':True,'origin_process_gone':True,'post_stop_origin_requests':0,'generation_runs':2,'empty_and_oversize_new_runs':0,'unicode_quote':quote,'pngs':[{'path':str(p.relative_to(base)),'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted(pngs)]}
    (base/'verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n');print(json.dumps(result,ensure_ascii=False,indent=2))
if __name__=='__main__':main()
