from pathlib import Path
import subprocess,json,os
O=Path(__file__).resolve().parent;W=Path('/Users/markik/Code/worktrees');pins=json.loads((O/'supplier-pins.json').read_text());result={}
for name in ['mere','turnstone']:
 d=W/(name+'-cross-source-answers-20261022')
 with (O/('order-'+name+'-source-graph.json')).open('w') as out,(O/('order-'+name+'-source-graph.stderr.log')).open('w') as err:subprocess.run(['/usr/local/bin/cargo','metadata','--locked','--offline','--format-version','1'],cwd=d,stdout=out,stderr=err,check=True,env={**os.environ,'RUSTC':'/usr/local/bin/rustc','RUSTDOC':'/usr/local/bin/rustdoc'})
 v=json.loads((O/('order-'+name+'-source-graph.json')).read_text());e=sorted({p['source'] for p in v['packages'] if p.get('source') and 'github.com/merely-made/eidetic.git' in p['source']});assert len(e)==1 and pins['eidetic'] in e[0],e;result[name]={'eidetic':e[0]}
 if name=='turnstone':
  m=sorted({p['source'] for p in v['packages'] if p.get('source') and p['name']=='distillery'});assert len(m)==1 and pins['mere'] in m[0],m;result[name]['resident_mere']=m[0]
  other={p['name']:p['source'] for p in v['packages'] if p.get('source') and 'mere.git?rev=' in p['source']};old=json.loads((O/'turnstone-source-graph.json').read_text());baseline={p['name']:p['source'] for p in old['packages'] if p.get('source') and 'mere.git?rev=' in p['source']};assert other==baseline;result[name]['other_mere_pins_unchanged']=other
(O/'order-canonical-sources.json').write_text(json.dumps({'result':'pass','sources':result},indent=2)+'\n');print(json.dumps(result,indent=2))
