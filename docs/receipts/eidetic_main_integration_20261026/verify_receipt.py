#!/usr/bin/env python3
from pathlib import Path
import hashlib,json,re
R=Path(__file__).resolve().parent
hashes=json.loads((R/'hashes.json').read_text());files={str(p.relative_to(R)):p for p in R.rglob('*') if p.is_file() and p.name!='hashes.json' and '__pycache__' not in p.parts};assert set(files)==set(hashes)
for n,p in files.items():assert hashlib.sha256(p.read_bytes()).hexdigest()==hashes[n],n
s=json.loads((R/'summary.json').read_text());total=0
for name,c in s['software_checks'].items():
 log=(R/c['log']).read_text();m=re.search(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',log);assert m and [int(m[i]) for i in [1,2,3]]==[c['passed'],c['failed'],c['ignored']];assert c['passed']>0 and c['failed']==0;total+=c['passed']
assert total==s['total_passed'] and s['prior_semantic_gate']=={'result':'gate_not_passed','passed':8,'failed':7,'unchanged':True} and not s['new_model_or_headed_qualification']
for name,pin in s['supplier_main_revisions'].items():assert json.loads((R/'publications'/(name+'.json')).read_text())['commit']==pin
assert json.loads((R/'prompt-preservation.json').read_text())['whole_answer_contract_module_identical'] is True
assert all(n.endswith('.md') for n in json.loads((R/'mere-upstream-reconciliation.json').read_text())['upstream_delta_files'])
assert json.loads((R/'receiving-source-preservation.json').read_text())['upstream_source_and_fixtures_preserved'] is True
assert json.loads((R/'turnstone-product-build-result.json').read_text())['exit_code']==0
assert json.loads((R/'turnstone-product-freeze.json').read_text())['source_commit']==s['turnstone_validated_source_commit']
print(json.dumps({'receipt_integrity':'pass','software_tests_passed':total,'supplier_revisions':len(s['supplier_main_revisions']),'semantic_gate':'unchanged gate_not_passed','new_headed_qualification':False},indent=2))
