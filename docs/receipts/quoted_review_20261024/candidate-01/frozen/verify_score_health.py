#!/usr/bin/env python3
"""Check numerical observations for each unchanged frozen request."""
from pathlib import Path
import json,re,argparse

def parse(log):
 pattern=r'prompt_hash=(blake3:[0-9a-f]{64}) score_health=ScoreHealth \{ checked_rows: (\d+), invalid_rows: (\d+), prefix_recomputations: (\d+), failed_recoveries: (\d+) \}'
 return [{'prompt':m[0],**dict(zip(['checked_rows','invalid_rows','prefix_recomputations','failed_recoveries'],map(int,m[1:])))} for m in re.findall(pattern,log)]
def main():
 p=argparse.ArgumentParser();p.add_argument('probe',type=Path);a=p.parse_args();cases=json.loads((a.probe/'frozen/cases.json').read_text());events=parse((a.probe/'run.log').read_text());checks={}
 for case in cases['asks']:
  run=json.loads((a.probe/(case['id']+'.json')).read_text());prompt=run['recall']['answer_contract']['prompt'];rows=[e for e in events if e['prompt']==prompt];assert len(rows)==1,(case['id'],rows)
  e=rows[0];assert e['checked_rows']>0 and e['failed_recoveries']==0 and e['prefix_recomputations']<=1
  assert e['invalid_rows']==e['prefix_recomputations'],e
  checks[case['id']]=e
 assert len(events)==4
 result={'result':'pass','cases':checks,'total_observed_invalid_rows':sum(e['invalid_rows'] for e in events),'total_prefix_recomputations':sum(e['prefix_recomputations'] for e in events),'boundary':'Content-free score observations for this cohort. No recovery-needed conclusion is drawn from suppressed logs; all four logged requests are required. Recomputed rows stay inside the same original token, deadline and model budgets. Frozen semantics remains separate.'};(a.probe/'score-health-review.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
if __name__=='__main__':main()
