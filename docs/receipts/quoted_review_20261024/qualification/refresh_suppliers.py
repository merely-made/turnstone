from pathlib import Path
import json,subprocess
O=Path(__file__).resolve().parent;W=Path('/Users/markik/Code/worktrees');before=json.loads((O/'baseline-v6-pins.json').read_text());heads=dict(before)
def head(d):return subprocess.check_output(['git','rev-parse','HEAD'],cwd=d,text=True).strip()
heads['eidetic']=head(W/'eidetic-cross-source-answers-20261022')
for name in ['esp','knot','cleromancy','gemot','murm']:
 d=W/(name+'-cross-source-answers-20261022');assert not subprocess.check_output(['git','status','--short'],cwd=d,text=True)
 changed=[]
 for rel in ['Cargo.toml','Cargo.lock','core/Cargo.toml','core/Cargo.lock','native/Cargo.toml','native/Cargo.lock']:
  p=d/rel
  if not p.exists():continue
  s=p.read_text();after=s
  for key in ['eidetic','esp']:after=after.replace(before[key],heads[key])
  if after!=s:p.write_text(after);changed.append(rel)
 subprocess.run(['git','diff','--check'],cwd=d,check=True);subprocess.run(['git','add','--',*changed],cwd=d,check=True)
 subprocess.run(['git','commit','-m','Refresh exact quoted-review supplier pins'],cwd=d,check=True)
 subprocess.run(['git','push','origin','HEAD:codex/cross-source-answers-20261022'],cwd=d,check=True)
 heads[name]=head(d);(O/'supplier-pins.json').write_text(json.dumps(heads,indent=2)+'\n')
for name in ['mere','turnstone']:
 for rel in ['Cargo.toml','Cargo.lock']:
  p=W/(name+'-cross-source-answers-20261022')/rel;s=p.read_text()
  for key in ['eidetic','esp','knot','cleromancy','gemot','murm']:s=s.replace(before[key],heads[key])
  p.write_text(s)
print(json.dumps(heads,indent=2))
