"""Verify recorded scene-refresh evidence; require all gates unless --software-only."""
from pathlib import Path
import argparse,hashlib,json,re,subprocess,sys
p=argparse.ArgumentParser();p.add_argument('receipt',type=Path);p.add_argument('--repo',type=Path);p.add_argument('--software-only',action='store_true');args=p.parse_args()
r=args.receipt.resolve();repo=args.repo or r.parents[2]
sha=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
manifest=json.loads((r/'sha256.json').read_text())
assert set(manifest)=={str(f.relative_to(r)) for f in r.rglob('*') if f.is_file() and f.name!='sha256.json'}, 'receipt file set changed'
for name,h in manifest.items():assert sha(r/name)==h,('receipt bytes changed',name)
source=json.loads((r/'candidate.json').read_text())
for path,h in source['source_sha256'].items():
 blob=subprocess.check_output(['git','show',source['source_commit']+':'+path],cwd=repo)
 assert hashlib.sha256(blob).hexdigest()==h,('recorded Git source differs',path)
software=json.loads((r/'software-result.json').read_text())
assert software['build']['exit_code']==0
for name in ['build-process.json','workspace-process.json']:
 process=json.loads((r/name).read_text())
 assert process['source_commit']==source['source_commit'] and process['exit_code']==0
summary=re.findall(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored;', (r/'workspace.log').read_text())
assert summary
passed=sum(int(x[1]) for x in summary);ignored=sum(int(x[3]) for x in summary)
assert passed==software['workspace']['passed'] and ignored==software['workspace']['ignored']
assert software['workspace']['exit_code']==0
assert all(state=='ok' and failed=='0' for state,passed,failed,ignored in summary)
assert '23 passed; 0 failed;' in (r/'focused.log').read_text()
assert '8 passed; 0 failed;' in (r/'session-persistence.log').read_text()
assert '8 passed; 0 failed;' in (r/'multiwindow-selfdrive-focused.log').read_text()
assert '3 passed; 0 failed;' in (r/'supplier-scene-axes.log').read_text()
assert '5 passed; 0 failed;' in (r/'supplier-bounded-scenes.log').read_text()
audit=json.loads((r/'supplier-source-audit.json').read_text());assert audit['result']=='pass' and audit['selected_package_manifest_heads_match_source'] and audit['selected_supplier_tracked_trees_clean']
native=software['native']
if native['status']=='qualified':
 inputs=json.loads((r/'inputs.json').read_text())
 assert inputs['turnstone_source']==source['source_commit']
 assert inputs['source_sha256']==source['source_sha256']
 assert inputs['binary_sha256']==source['native_binary_sha256']
 assert inputs['driver_sha256']==sha(r/'run_native.py')
 launcher=subprocess.check_output(['git','show',source['source_commit']+':docs/receipts/graph_folds_20261016/run_macos_scenario.py'],cwd=repo)
 assert hashlib.sha256(launcher).hexdigest()==inputs['launcher_sha256']
 for mode,scenario in [('seed','graph_scenes.scn'),('reopen','graph_scenes_reopen.scn')]:
  expected=source['source_sha256']['scenarios/'+scenario]
  assert json.loads((r/mode/'process.json').read_text())['scenario_sha256']==expected
  assert json.loads((r/mode/'process.json').read_text())['binary_sha256']==source['native_binary_sha256']
 review=json.loads((r/'visual-review.json').read_text())
 assert review['result']=='pass' and len(review['images'])==12
 for item in review['images']:assert sha(r/item['path'])==item['sha256']
 subprocess.run([sys.executable,str(r/'verify_native.py'),str(r)],check=True)
 captures=json.loads((r/'verification.json').read_text())['images']
 assert {(i['path'],i['sha256']) for i in review['images']}=={(i['path'],i['sha256']) for i in captures}
 acceptance='qualified'
else:
 assert native['status']=='blocked_locked_console'
 failure=r/'diagnostics/locked-console-r1'
 assert json.loads((failure/'blocker.json').read_text())['macos_screen_locked']
 assert not json.loads((failure/'process.json').read_text())['success']
 assert (failure/'scenario.done').read_text().startswith('RESULT fail')
 acceptance='blocked: headed and reopen acceptance pending an unlocked console'
result={'evidence_verification':'pass','acceptance':acceptance,'source_commit':source['source_commit'],'workspace_passed':passed,'workspace_ignored':ignored,'full_green_workspace_rerun':True,'focused_passed':23,'new_receiving_tests':24,'multiwindow_receipt_passed':8,'supplier_tests':8}
print(json.dumps(result,indent=2))
if acceptance!='qualified' and not args.software_only:sys.exit(2)
