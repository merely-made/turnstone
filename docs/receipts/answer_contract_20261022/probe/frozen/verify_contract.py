#!/usr/bin/env python3
"""Independent retained task binding check; does not replace the frozen answer gate."""
import argparse,json,re
from pathlib import Path
import blake3

def wire(v):return json.dumps(v,ensure_ascii=False,separators=(',',':'))
def digest(v):return 'blake3:'+blake3.blake3(v.encode()).hexdigest()
def main():
 p=argparse.ArgumentParser();p.add_argument('probe',type=Path);a=p.parse_args()
 frozen=a.probe/'frozen';source=(frozen/'answer_contract.rs').read_text()
 instructions=re.search(r'const INSTRUCTIONS: &str = r#"(.*?)"#;',source,re.S).group(1)
 cases=json.loads((frozen/'cases.json').read_text());checks={}
 for case in cases['asks']:
  run=json.loads((a.probe/(case['id']+'.json')).read_text());r=run['recall'];c=r['answer_contract']
  prompt=instructions+wire(run['question'])+'\nReturn only the single-line minified JSON object.'
  candidates=[{'handle':p['handle'],'quote':p['passage']['text']} for p in r['passages']]
  context='EVIDENCE candidates (the entire available evidence set):\n'+wire(candidates)+'\n'
  assert run['question']==case['question']
  assert c=={'method':'distillery.requested-facts/v2','question':digest(run['question']),'prompt':digest(prompt),'context':digest(context),'max_claims':8},(case['id'],c)
  assert len(r['support']['claims'])<=8
  checks[case['id']]={'result':'pass','contract':c,'scope_members':len(r['scope']),'candidates':len(candidates)}
 result={'result':'pass','cases':checks,'boundary':'Exact task binding and bounds only; answer quality is decided by the unchanged frozen gate.'}
 (a.probe/'contract-verification.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
if __name__=='__main__':main()
