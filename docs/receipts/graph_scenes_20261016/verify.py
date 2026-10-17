"""Verify recorded scene-refresh evidence; require all gates unless --software-only."""
from pathlib import Path
import argparse,hashlib,json,re,subprocess,sys
p=argparse.ArgumentParser();p.add_argument('receipt',type=Path);p.add_argument('--repo',type=Path);p.add_argument('--software-only',action='store_true');args=p.parse_args()
r=args.receipt.resolve();repo=args.repo or r.parents[2]
sha=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
for name,h in json.loads((r/'sha256.json').read_text()).items():assert sha(r/name)==h,('receipt bytes changed',name)
source=json.loads((r/'candidate.json').read_text())
for path,h in source['source_sha256'].items():
 blob=subprocess.check_output(['git','show',source['source_commit']+':'+path],cwd=repo)
 assert hashlib.sha256(blob).hexdigest()==h,('recorded Git source differs',path)
software=json.loads((r/'software-result.json').read_text())
assert software['build']['exit_code']==0
summary=re.findall(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored;', (r/'workspace.log').read_text())
assert summary
passed=sum(int(x[1]) for x in summary);ignored=sum(int(x[3]) for x in summary)
assert passed==software['workspace']['passed'] and ignored==software['workspace']['ignored']
assert software['workspace']['exit_code']==101 and not software['workspace']['full_green_rerun']
assert sum(int(x[2]) for x in summary)==software['workspace']['failed']==1
assert summary[0][0]=='FAILED' and all(x[0]=='ok' for x in summary[1:])
assert software['workspace']['failure']=='settings_pane::tests::pane_renders_controls_from_setting_control_not_setting_ids'
assert 'test '+software['workspace']['failure']+' ... FAILED' in (r/'workspace.log').read_text()
correction=software['corrective'];assert correction['exit_code']==0 and not correction['production_change']
assert '9 passed; 0 failed;' in (r/'settings-corrective.log').read_text()
# The full run already exercised all production code. Prove the corrective
# source differs only in this stale test fixture and its explanatory comment.
git=lambda *args:subprocess.check_output(['git',*args],cwd=repo)
changed=git('diff','--name-only',source['source_commit'],correction['source_commit'],'--','src','scenarios','Cargo.toml','Cargo.lock').decode().splitlines()
assert changed==['src/settings_pane.rs']
before=git('show',source['source_commit']+':src/settings_pane.rs')
after=git('show',correction['source_commit']+':src/settings_pane.rs')
old=b'// Four shared built-ins plus the Turnstone default, five mode choices,'
new=b'// Six shared built-ins plus the Turnstone default, five mode choices,'
assert before.count(old)==1
old_count=b'assert_eq!(dom.all_with_class(dom.document(), "radio").len(), 14);'
new_count=b'assert_eq!(dom.all_with_class(dom.document(), "radio").len(), 16);'
assert before.count(old_count)==1
assert after==before.replace(old,new).replace(old_count,new_count)
assert '19 passed; 0 failed;' in (r/'focused.log').read_text()
assert '3 passed; 0 failed;' in (r/'supplier-scene-axes.log').read_text()
assert '5 passed; 0 failed;' in (r/'supplier-bounded-scenes.log').read_text()
audit=json.loads((r/'supplier-source-audit.json').read_text());assert audit['result']=='pass' and audit['selected_package_manifest_heads_match_source'] and audit['selected_supplier_tracked_trees_clean']
native=software['native']
if native['status']=='qualified':
 subprocess.run([sys.executable,str(r/'verify_native.py'),str(r)],check=True)
 acceptance='qualified'
else:
 assert native['status']=='blocked_locked_console'
 failure=r/'diagnostics/locked-console-r1'
 assert json.loads((failure/'blocker.json').read_text())['macos_screen_locked']
 assert not json.loads((failure/'process.json').read_text())['success']
 assert (failure/'scenario.done').read_text().startswith('RESULT fail')
 acceptance='blocked: headed and reopen acceptance pending an unlocked console'
result={'evidence_verification':'pass','acceptance':acceptance,'source_commit':source['source_commit'],'workspace_passed':passed,'workspace_failed_stale_assertion':1,'workspace_ignored':ignored,'corrective_settings_module_passed':9,'test_only_correction_verified':True,'full_green_workspace_rerun':False,'focused_passed':19,'new_receiving_tests':16,'supplier_tests':8}
print(json.dumps(result,indent=2))
if acceptance!='qualified' and not args.software_only:sys.exit(2)
