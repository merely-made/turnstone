from pathlib import Path
import json,importlib.util
O=Path(__file__).resolve().parent;N=O/'native-pair-04';spec=importlib.util.spec_from_file_location('score',O/'verify_score_health.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
checks={}
for phase,count in [(0,4),(1,1)]:
 events=m.parse((N/f'fused-resident-{phase}.log').read_text());assert len(events)==count,(phase,len(events))
 files=[f'mixed-{phase}-{key}.json' for key in (['cross','missing','conflict','reduced'] if phase==0 else ['cross'])]
 for name in files:
  run=json.loads((N/'fused/resident'/name).read_text());matches=[x for x in events if x['prompt']==run['recall']['answer_contract']['prompt']];assert len(matches)==1
  e=matches[0];assert e['checked_rows']>0 and e['invalid_rows']==e['prefix_recomputations']==e['failed_recoveries']==0
  checks[name]=e
v={'result':'pass','generations':checks,'observed_invalid_rows':0,'prefix_recomputations':0,'boundary':'Five logged native generations. Historical reopen does not generate; no recovery-required conclusion inferred from missing logs.'};(N/'score-health-review.json').write_text(json.dumps(v,indent=2)+'\n');print(json.dumps(v,indent=2))
