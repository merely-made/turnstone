"""Synthetic negative controls for the receiving driver, not product UI evidence."""
from pathlib import Path
import json
import os
import subprocess

out=Path('/home/markik/Code/output/cue-vulkan-thinkpad-20261023/guard-controls')
out.mkdir(exist_ok=False)
fake=out/'synthetic-not-turnstone'
fake.write_text('''#!/usr/bin/env python3
import json,os
from pathlib import Path
p=Path(os.environ['TURNSTONE_CAPTURE_DIR'])
(p/'scenario.done').write_text('RESULT ok\\nSynthetic guard control, not a product run\\n')
(p/'synthetic.png').write_bytes(b'NOT A PRODUCT CAPTURE')
entry={'sampled_layers':1,'scope':'1:1'}
(p/'primary.cue-frames.json').write_text(json.dumps({'entries':[entry]}))
if os.environ['CONTROL']=='blank-lens':
    (p/'synthetic_lens.cue-frames.json').write_text(json.dumps({'entries':[{'sampled_layers':0,'scope':None}]}))
adapter={'backend':'Metal' if os.environ['CONTROL']=='wrong-backend' else 'Vulkan','device_type':'Cpu' if os.environ['CONTROL']=='software-vulkan' else 'IntegratedGpu','name':'synthetic guard control'}
(p/'primary.sources.json').write_text(json.dumps([{'adapter':adapter}]))
''')
fake.chmod(0o700)
scenario=out/'synthetic.scn';scenario.write_text('# Synthetic control; never a real native UI scenario\n')
rows=[]
for name,expected in [('software-vulkan',"'device_type': 'Cpu'"),('wrong-backend',"'backend': 'Metal'"),('blank-lens','blank content in lens')]:
    env=os.environ.copy();env.update(CONTROL=name,WAYLAND_DISPLAY='wayland-0')
    args=['python3','/home/markik/Code/worktrees/turnstone-cue-vulkan-20261023/scripts/receive-cue-ui-linux.py',str(fake),str(scenario),str(out/name),'http://127.0.0.1:1']
    p=subprocess.run(args,env=env,capture_output=True,text=True)
    (out/(name+'.log')).write_text(p.stdout+p.stderr)
    assert p.returncode != 0 and expected in p.stdout+p.stderr,(name,p.returncode,p.stdout,p.stderr)
    assert json.loads((out/name/'run.json').read_text())['passed'] is False
    rows.append({'control':name,'expected_refusal':expected,'driver_exit':p.returncode,'refused':True})
(out/'summary.json').write_text(json.dumps({'scope':'Synthetic receiving-driver negative controls only; no native rendering or hardware qualification','controls':rows},indent=2)+'\n')
print(json.dumps(rows,indent=2))
