from pathlib import Path
import json,subprocess,tomllib
O=Path(__file__).resolve().parent;W=Path('/Users/markik/Code/worktrees');before=json.loads((O/'preservation-before.json').read_text())
heads={n:v['head'] for n,v in before.items()}
heads['eidetic']=subprocess.check_output(['git','rev-parse','HEAD'],cwd=W/'eidetic-cross-source-answers-20261022',text=True).strip()
def refs(obj):
 if isinstance(obj,dict):
  if obj.get('git')=='https://github.com/merely-made/esp.git' and 'rev' in obj:yield obj['rev']
  for v in obj.values():yield from refs(v)
for name in ['esp','knot','cleromancy','gemot']:
 d=W/(name+'-cross-source-answers-20261022')
 assert not subprocess.check_output(['git','status','--short'],cwd=d,text=True)
 s=(d/'Cargo.toml').read_text();changes={before['eidetic']['head']:heads['eidetic']}
 if name!='esp':changes.update({old:heads['esp'] for old in refs(tomllib.loads(s))})
 for file in ['Cargo.toml','Cargo.lock']:
  p=d/file;t=p.read_text()
  for old,new in changes.items():t=t.replace(old,new)
  p.write_text(t)
 subprocess.run(['git','diff','--check'],cwd=d,check=True)
 subprocess.run(['git','add','--','Cargo.toml','Cargo.lock'],cwd=d,check=True)
 subprocess.run(['git','commit','-m','Refresh retained candidate-review supplier pins'],cwd=d,check=True)
 subprocess.run(['git','push','origin','HEAD:codex/cross-source-answers-20261022'],cwd=d,check=True)
 heads[name]=subprocess.check_output(['git','rev-parse','HEAD'],cwd=d,text=True).strip()
 (O/'supplier-pins.json').write_text(json.dumps(heads,indent=2)+'\n')
for name in ['mere','turnstone']:
 d=W/(name+'-cross-source-answers-20261022')
 for file in ['Cargo.toml','Cargo.lock']:
  p=d/file;t=p.read_text()
  for key in ['eidetic','esp','knot','cleromancy','gemot']:
   t=t.replace(before[key]['head'],heads[key])
  p.write_text(t)
print(json.dumps(heads,indent=2))
