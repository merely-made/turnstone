#!/usr/bin/env python3
"""Additional sealed-plan and claim-slot verification after headed cold reopen.
This cannot replace the unchanged native/citation/semantic checks.
"""
from pathlib import Path
import argparse,json,re,sys
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
from verify_contract_inputs import check

def read(p):return json.loads(p.read_text())
def main():
 p=argparse.ArgumentParser();p.add_argument('run',type=Path);a=p.parse_args();assert read(a.run/'completion.json')['result']=='pass'
 source=(HERE/'answer_contract.rs').read_text();instructions=re.search(r'const INSTRUCTIONS_V5: &str = r#"(.*?)"#;',source,re.S).group(1)
 cases=read(HERE/'cases.json');checks={}
 for mode in read(a.run/'completion.json')['modes']:
  lane=a.run/mode;snapshots=[read(lane/'resident'/f'snapshot-{i}.json') for i in [0,1]];review={}
  for case in cases['asks']:
   key=case['id'];warm=read(lane/'resident'/f'mixed-0-{key}.json');cold=read(lane/'resident'/f'history-1-{key}.json');assert warm==cold
   bound=check(warm,instructions)
   for snapshot in snapshots:
    sealed=next(x for x in snapshot['evidence'] if x['record']['run']==warm['run'])
    assert sealed['requested_facts']==bound['facts'],'sealed fact set differs from original question plan'
    assert sealed['answer_support']['requested_facts']==bound['set_id']
    assert [c['fact'] for c in sealed['answer_support']['claims']]==bound['facts']['facts']
   review[key]={'retained_set':bound['set_id'],'sealed_warm_cold_equal':True,'fixed_count':len(bound['facts']['facts'])}
  fresh=read(lane/'resident/mixed-1-cross.json');bound=check(fresh,instructions);sealed=next(x for x in snapshots[1]['evidence'] if x['record']['run']==fresh['run']);assert sealed['requested_facts']==bound['facts'];assert sealed['answer_support']['requested_facts']==bound['set_id'];review['fresh_cold_cross']={'retained_set':bound['set_id'],'fixed_count':len(bound['facts']['facts']),'sealed_equal':True}
  checks[mode]=review
 result={'result':'pass','cases':checks,'boundary':'Additional question-plan, exact claim-slot and sealed-artifact checks. Native source absence, citations and semantics require the unchanged frozen checks and recorded visual review.'};(a.run/'requested-fact-native-review.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n');print(json.dumps(result,ensure_ascii=False,indent=2))
if __name__=='__main__':main()
