from pathlib import Path
import collections,hashlib,json,math,sys
from PIL import Image,ImageStat
base=Path(sys.argv[1])
scenes={p.stem:json.loads(p.read_text()) for mode in ['seed','reopen'] for p in (base/mode).glob('*.scene.json')}
expected_scenes={name+'.scene' for name in ['00_source','01_partition','02_attention','03_grid','04_reset','05_sets','06_provenance','07_mosaic','09_saved','10_lens_partition','11_lens_saved','12_reopened','15_recipe_restored']}
assert set(scenes)==expected_scenes and len(scenes)==13, sorted(scenes)
truth=scenes['00_source.scene']['graph']
assert len(truth['nodes'])==len(truth['resources'])==len(truth['shown_resources'])==6
assert len(truth['resource_edges'])==5
expected_images={'seed/'+name+'.png' for name in ['01_partition','02_attention','04_reset','05_sets','06_provenance','07_mosaic','09_controls','09_saved','10_lens_partition','11_lens_saved']} | {'reopen/12_reopened.png','reopen/15_recipe_restored.png'}
for name,s in scenes.items():
 assert s['graph']==truth, name+' changed graph truth'
 assert s['graph_id']==scenes['00_source.scene']['graph_id'], name+' changed graph owner'
 assert s['physics_paused'], name+' unexpectedly playing'
 source_members={node['node_id'] for node in truth['nodes']}
 assert len(s['geometry']['positions'])==6 and {member for member,_ in s['geometry']['positions']}==source_members, name+' incomplete geometry'
 assert s['facts']['grammar'], name+' grammar off'
 for _,at in s['geometry']['positions']:
  assert all(isinstance(x,(int,float)) and math.isfinite(x) and abs(x)<1e8 for x in at), name+' bad position'
 for region in s['geometry']['regions']:
  assert all(isinstance(x,(int,float)) and math.isfinite(x) for at in region['contour'] for x in at), name+' bad region'
for name,id in [('01_partition','partition.default'),('03_grid','grid.default'),('05_sets','sets.default'),('06_provenance','provenance.default'),('07_mosaic','mosaic.default'),('10_lens_partition','partition.default')]:
 assert scenes[name+'.scene']['facts']['arrangement']==id
assert scenes['01_partition.scene']['facts']['regions']>=3
assert scenes['03_grid.scene']['facts']['regions']==0
assert scenes['06_provenance.scene']['facts']['regions']==0
assert scenes['02_attention.scene']['facts']['foreground_pins']==1
assert scenes['12_reopened.scene']['facts']['foreground_pins']==0
asmap=lambda pairs: {str(k):v for k,v in pairs}
saved=scenes['11_lens_saved.scene']; reopened=scenes['12_reopened.scene']
assert saved['folds']==reopened['folds'] and len(saved['folds'])==1
assert saved['facts']['arrangement']==reopened['facts']['arrangement']=='mosaic.default'
assert asmap(saved['geometry']['seed'])==asmap(reopened['geometry']['seed'])
assert asmap(saved['geometry']['positions'])==asmap(reopened['geometry']['positions'])
for key in ['group_of','scale']:
 assert asmap(saved['geometry']['mosaic'][key])==asmap(reopened['geometry']['mosaic'][key])
for key in ['group_order','member_order']:
 assert saved['geometry']['mosaic'][key]==reopened['geometry']['mosaic'][key]
assert saved['geometry']['regions']==reopened['geometry']['regions']
members={member for member,_ in saved['geometry']['positions']}
scales=saved['geometry']['mosaic']['scale']
assert all(member in members and math.isfinite(scale) and scale>0 for member,scale in scales)
assert any(scale!=1.0 for _,scale in scales), 'nondefault Mosaic scale required'
assert asmap(scenes['01_partition.scene']['geometry']['positions'])==asmap(scenes['04_reset.scene']['geometry']['positions']), 'reset did not land at saved seed'
assert not scenes['15_recipe_restored.scene']['facts']['saved_seed']
assert scenes['15_recipe_restored.scene']['geometry']['seed'] is None
fold_checks=[]
for mode in ['seed','reopen']:
 for path in sorted((base/mode).glob('*.folds.json')):
  v=json.loads(path.read_text()); assert v['graph']==truth
  assert v['records']==v['stored']['records']
  key_by_member={m['id']:m['key'] for m in v['member_keys']}
  membership={key_by_member[m]:i for i,r in enumerate(v['records']) for m in r['members']}
  assert len(membership)==sum(len(r['members']) for r in v['records'])
  expected=collections.Counter()
  for r in v['relations']:
   a,b=r['from'],r['to']
   if a not in membership and b not in membership:continue
   a=f"Fold({membership[a]})" if a in membership else f"Node({a})"
   b=f"Fold({membership[b]})" if b in membership else f"Node({b})"
   if a!=b:expected[a,b,r['family']]+=1
  actual=collections.Counter({(r['from'],r['to'],r['family']):r['count'] for r in v['routes']})
  assert actual==expected,(path.name,actual,expected)
  fold_checks.append(str(path.relative_to(base)))
assert set(fold_checks)=={'seed/08_folded.folds.json','reopen/12_reopened.folds.json','reopen/13_expanded.folds.json','reopen/14_restored_fold.folds.json'} and len(fold_checks)==4
images=[]
for mode in ['seed','reopen']:
 p=json.loads((base/mode/'process.json').read_text())
 assert p['success'] and not p['timeout'], (mode,p)
 for path in sorted((base/mode).glob('*.png')):
  im=Image.open(path).convert('RGB'); assert min(im.size)>=400
  assert max(ImageStat.Stat(im).stddev)>5, path
  images.append({'path':str(path.relative_to(base)),'size':im.size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()})
assert {item['path'] for item in images}==expected_images and len(images)==12
keeper=json.loads((base/'keeper-process.json').read_text());assert keeper['original_files_unchanged'] and keeper['exited'] and keeper['bounded_display_sleep_assertion_exited']
result={'result':'pass','scene_checkpoints':len(scenes),'fold_route_checkpoints':fold_checks,'images':images,'source_graph_unchanged':True,'exact_reopen_positions_seed_regions_mosaic_fold':True,'saved_seed_reset':True,'transient_foreground_pin_cleared_on_reopen':True,'processes_and_helper_exited':True}
(base/'verification.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
