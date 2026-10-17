from pathlib import Path
import collections, copy, hashlib, json, subprocess, sys
from PIL import Image, ImageStat
base=Path(sys.argv[1])
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
# Run from the checkout root, or pass it as the second argument.
repo=Path(sys.argv[2]).resolve() if len(sys.argv)>2 else Path.cwd()
inputs=json.loads((base/'inputs.json').read_text())
for path,digest in inputs['source_sha256'].items():
 blob=subprocess.check_output(['git','show',inputs['turnstone_source']+':'+path],cwd=repo)
 assert hashlib.sha256(blob).hexdigest()==digest, ('source binding',path)
 if (repo/path).exists():assert sha(repo/path)==digest, ('working source changed',path)
checks=[]
for phase in ['seed','reopen']:
 process=json.loads((base/phase/'process.json').read_text())
 assert process['success'] and not process['timeout'] and process['launcher_exit']==0
 assert (base/phase/'scenario.done').read_text().splitlines()[0]=='RESULT ok'
 checks.extend((phase,p,json.loads(p.read_text())) for p in sorted((base/phase).glob('*.folds.json')))
assert len(checks)==12,len(checks)
reference=checks[0][2]
assert all(pos is not None for _,pos in reference['positions']), 'missing member geometry'
def graph(value):
 value=copy.deepcopy(value)
 assert value['timestamp_secs']==0 # explicit snapshot date, all source event clocks retained
 return value
for phase,p,v in checks:
 assert graph(v['graph'])==graph(reference['graph']),('graph changed',p.name)
 assert v['positions']==reference['positions'],('source positions changed',p.name)
 assert v['stored_graph.json_blake3']==reference['stored_graph.json_blake3'],('stored graph bytes changed',p.name)
 assert v['stored_facets.json_blake3']==reference['stored_facets.json_blake3'],('stored facets changed',p.name)
 assert v['records']==v['stored']['records']
 assert v['physics_paused'] and v['presented_frames']>0
 key_by_member={m['id']:m['key'] for m in v['member_keys']}
 membership={key_by_member[member]:index for index,r in enumerate(v['records']) for member in r['members']}
 assert len(membership)==sum(len(r['members']) for r in v['records'])
 expected=collections.Counter()
 for r in v['relations']:
  a,b=r['from'],r['to']
  if a not in membership and b not in membership:continue
  a=f"Fold({membership[a]})" if a in membership else f"Node({a})"
  b=f"Fold({membership[b]})" if b in membership else f"Node({b})"
  if a!=b:expected[a,b,r['family']]+=1
 actual=collections.Counter({(r['from'],r['to'],r['family']):r['count'] for r in v['routes']})
 assert actual==expected,('directed boundary accounting',p.name,actual,expected)
 for r in v['records']:
  assert r['version']==2 and r['source_scope']=='turnstone:graph:'+v['graph_id']
saved=next(v for _,p,v in checks if p.stem=='07_saved.folds')
reopened=next(v for _,p,v in checks if p.stem=='08_reopened.folds')
assert saved['records']==reopened['records']
assert checks[-1][2]['records']==[]
images=[]
for phase in ['seed','reopen']:
 for path in sorted((base/phase).glob('*.png')):
  with Image.open(path) as im:
   assert im.width>=1000 and im.height>=600
   stat=ImageStat.Stat(im.convert('RGB'));assert max(stat.var)>10
   images.append({'path':str(path.relative_to(base)),'size':im.size,'sha256':sha(path)})
assert len(images)==9,len(images)
helper=json.loads((base/'keeper-process.json').read_text())
assert helper['exit_code']==0 and helper['exited'] and helper['original_files_unchanged'] and helper['bounded_display_sleep_assertion_exited']
result={'result':'passed','checkpoints':len(checks),'presented_frames_at_final_checkpoint':{phase:max(v['presented_frames'] for ph,p,v in checks if ph==phase) for phase in ['seed','reopen']},'source_nodes':len(reference['positions']),'source_relation_cells':len(reference['relations']),'two_fold_boundary_bundles':len(saved['routes']),'two_fold_boundary_cells':sum(r['count'] for r in saved['routes']),'source_graph_and_facets_bytes_unchanged':True,'source_snapshot_at_fixed_date_unchanged':True,'every_member_position_unchanged':True,'exact_native_records_reopened':True,'all_directed_boundary_counts_recomputed':True,'images':images}
(base/'verification.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
