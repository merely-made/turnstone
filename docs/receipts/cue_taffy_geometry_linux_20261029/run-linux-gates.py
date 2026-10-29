from pathlib import Path
import subprocess,os,json,hashlib,time,re,resource,shutil
root=Path('<Code>');out=root/'output/cue-taffy-geometry-linux-20261029';sources=json.loads((out/'sources.json').read_text());repos={x['repo']:Path(x['checkout']) for x in sources};proof=json.loads((out/'fork-source-proof.json').read_text())
env=os.environ.copy();env.update(PATH='<home>/.cargo/bin:'+env['PATH'],CARGO_TARGET_DIR=str(root/'target'),CARGO_BUILD_BUILD_DIR=str(root/'repos/turnstone/target/build'),CARGO_BUILD_JOBS='2',CARGO_NET_GIT_FETCH_WITH_CLI='true',WGPU_BACKEND='vulkan',VK_DRIVER_FILES='/usr/share/vulkan/icd.d/radeon_icd.x86_64.json',XDG_RUNTIME_DIR='/run/user/1000',WAYLAND_DISPLAY='wayland-0',XDG_SESSION_TYPE='wayland',DBUS_SESSION_BUS_ADDRESS='unix:path=/run/user/1000/bus');env.pop('DISPLAY',None)
soft,hard=resource.getrlimit(resource.RLIMIT_NOFILE);resource.setrlimit(resource.RLIMIT_NOFILE,(min(65536,hard),hard))
state={'status':'starting','steps':[],'sources':sources,'storage':'existing stable Linux cache; no cleanup, cache move or policy change'}
def save(): (out/'gates-state.json').write_text(json.dumps(state,indent=2)+'\n')
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  for b in iter(lambda:f.read(1048576),b''):h.update(b)
 return h.hexdigest()
def audit(repo,packages):
 keys={(p['name'],p['version'],p.get('source')) for p in __import__('tomllib').loads((repo/'Cargo.lock').read_text())['package']};actual=set();heads={};fork=[]
 for p in packages:
  source=p['source']
  if source and source.startswith('git+'):
   checkout=next(d for d in Path(p['manifest_path']).parents if (d/'.git').exists());k=str(checkout)
   if k not in heads:heads[k]=subprocess.check_output(['git','rev-parse','HEAD'],cwd=checkout,text=True).strip()
   source=source.split('#')[0]+'#'+heads[k]
  actual.add((p['name'],p['version'],source))
  if p['name']=='genet-taffy':
   base=Path(p['manifest_path']).parent;assert p['version']=='0.16.0';assert all(sha(base/rel)==value for rel,value in proof['source_files'].items());fork.append({'version':p['version'],'source':source,'all_published_source_files_match':True,'source_files':len(proof['source_files'])})
 missing=sorted(actual-keys,key=str);result={'resolved_package_identities':len(actual),'lock_records':len(keys),'missing_source_aware_records':missing,'git_checkout_heads':heads,'fork':fork};assert not missing,missing;assert fork;return result
steps=[]
for repo in ['sdk','turnstone']:
 steps.append((repo+'-fetch',repo,['fetch','--locked','--target','x86_64-unknown-linux-gnu'],'fetch'))
 steps.append((repo+'-metadata',repo,['metadata','--locked','--offline','--format-version','1','--filter-platform','x86_64-unknown-linux-gnu'],'metadata'))
steps += [('sdk-vulkan-geometry','sdk',['test','--locked','--offline','-p','cue-genet','--test','text_input','-j2','--message-format','json-render-diagnostics','--','--ignored','--nocapture','--test-threads=1'],'tests'),('turnstone-primary-lens-geometry','turnstone',['test','--locked','--offline','--lib','document_text_input_geometry','-j2','--message-format','json-render-diagnostics','--','--nocapture','--test-threads=1'],'tests'),('turnstone-ime-admission','turnstone',['test','--locked','--offline','--lib','document_ime_admission','-j2','--message-format','json-render-diagnostics','--','--nocapture','--test-threads=1'],'tests'),('turnstone-vulkan-frame-custody','turnstone',['test','--locked','--offline','--lib','cue_content::tests::input_during_a_pending_frame','-j2','--message-format','json-render-diagnostics','--','--ignored','--nocapture','--test-threads=1'],'tests'),('turnstone-product-build','turnstone',['build','--locked','--offline','--bin','turnstone','-j2','--message-format','json-render-diagnostics'],'build')]
save()
for name,key,args,kind in steps:
 repo=repos[key];source=next(x for x in sources if x['repo']==key);assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()==source['commit'];assert not subprocess.check_output(['git','status','--porcelain'],cwd=repo,text=True).strip();assert sha(repo/'Cargo.lock')==source['lock_sha256'];dest=out/name;assert not dest.exists();dest.mkdir();cmd=['nice','-n','10','<home>/.cargo/bin/cargo','+1.99.1',*args];rec={'command':cmd,'cwd':str(repo),'source':source['commit'],'lock_sha256':source['lock_sha256'],'started':time.time(),'exit':None};state.update(status='running',stage=name);state['steps'].append(name);save();print('START',name,flush=True)
 with (dest/'stdout').open('w') as stdout,(dest/'stderr').open('w') as stderr:r=subprocess.run(cmd,cwd=repo,env=env,stdout=stdout,stderr=stderr)
 rec.update(exit=r.returncode,elapsed_seconds=round(time.time()-rec['started'],3));(dest/'run.json').write_text(json.dumps(rec,indent=2)+'\n');print('END',name,r.returncode,rec['elapsed_seconds'],flush=True)
 if r.returncode:
  state.update(status='failed',failure={'stage':name,'exit':r.returncode});save();print((dest/'stderr').read_text()[-5000:],flush=True);raise SystemExit(r.returncode)
 assert sha(repo/'Cargo.lock')==source['lock_sha256']
 if kind=='metadata':
  data=json.loads((dest/'stdout').read_text());result=audit(repo,data['packages']);(dest/'source-audit.json').write_text(json.dumps(result,indent=2)+'\n')
 elif kind in ['tests','build']:
  result=(dest/'stdout').read_text();messages=[]
  for line in result.splitlines():
   if not line.startswith('{'):continue
   try:messages.append(json.loads(line))
   except json.JSONDecodeError:pass
  assert any(m.get('reason')=='build-finished' and m.get('success') for m in messages)
  packages={m['package_id']:m for m in messages if m.get('reason')=='compiler-artifact'};parsed=[]
  for ident,m in packages.items():
   package=__import__('tomllib').loads(Path(m['manifest_path']).read_text())['package'];prefix,fragment=ident.rsplit('#',1);parsed.append({'name':package['name'],'version':fragment.rsplit('@',1)[-1],'source':prefix if prefix.startswith(('git+','registry+')) else None,'manifest_path':m['manifest_path']})
  checked=audit(repo,parsed);(dest/'source-audit.json').write_text(json.dumps(checked,indent=2)+'\n')
  if kind=='tests':
   counts=[tuple(map(int,m)) for m in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',result)];assert counts and sum(x[0] for x in counts)>0;assert not any(x[1] for x in counts);rec['test_counts']=counts;(dest/'run.json').write_text(json.dumps(rec,indent=2)+'\n')
 state.update(status='stage_passed');save()
exe=root/'target/debug/turnstone';frozen=out/'turnstone-linux';assert not frozen.exists();subprocess.run(['cp','--reflink=auto','--preserve=all',str(exe),str(frozen)],check=True);assert sha(exe)==sha(frozen);(out/'turnstone-executable.json').write_text(json.dumps({'source':next(x['commit'] for x in sources if x['repo']=='turnstone'),'path':str(frozen),'sha256':sha(frozen),'size':frozen.stat().st_size},indent=2)+'\n');state.update(status='passed');save();print('ALL GATES PASSED',flush=True)
