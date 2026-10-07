from pathlib import Path
import subprocess,os,json,hashlib,datetime,time,base64,sys
r=Path('/home/markik/Code/repos/turnstone');out=r/'docs/receipts/browser_family_20261007';source=r/'src/sky_surface/tests.rs';sha='abb349cf7957e2b509f4cb7f492e3e0db4821964';prefix='linux-sky-diagnostic';patch=base64.b64decode('LS0tIGEvc3JjL3NreV9zdXJmYWNlL3Rlc3RzLnJzCisrKyBiL3NyYy9za3lfc3VyZmFjZS90ZXN0cy5ycwpAQCAtMzcsNyArMzcsMjcgQEAKIAogI1t0ZXN0XQogZm4gcmVmZXJlbmNlX3NvdXJjZV9yZXByb2R1Y2VzX3RoZV9wMF9yZWNlaXB0KCkgewotICAgIGxldCBwcm9qZWN0aW9uID0gY2FsY3VsYXRlX3NvdXJjZSgmU2t5UGFuZVNvdXJjZVYxOjpib3N0b25fZWNsaXBzZV9yZWZlcmVuY2UoKSkudW53cmFwKCk7CisgICAgbGV0IHNvdXJjZSA9IFNreVBhbmVTb3VyY2VWMTo6Ym9zdG9uX2VjbGlwc2VfcmVmZXJlbmNlKCk7CisgICAgbGV0IHByb2plY3Rpb24gPSBjYWxjdWxhdGVfc291cmNlKCZzb3VyY2UpLnVud3JhcCgpOworICAgIHByaW50bG4hKAorICAgICAgICAiU0tZX1JFQ0VJUFRfRElBR05PU1RJQyB7fSB7fSIsCisgICAgICAgIHNvdXJjZS5kYXRlLAorICAgICAgICBzZXJkZV9qc29uOjp0b19zdHJpbmcoCisgICAgICAgICAgICAmU3RyaW5nOjpmcm9tX3V0ZjgocHJvamVjdGlvbi5yZWNlaXB0LnRvX3ByZXR0eV9qc29uKCkudW53cmFwKCkpLnVud3JhcCgpCisgICAgICAgICkKKyAgICAgICAgLnVud3JhcCgpCisgICAgKTsKKyAgICBsZXQgbXV0IG5leHRfZGF5X3NvdXJjZSA9IHNvdXJjZS5jbG9uZSgpOworICAgIG5leHRfZGF5X3NvdXJjZS5kYXRlID0gIjIwMjQtMDQtMDkiLmludG8oKTsKKyAgICBsZXQgbmV4dF9kYXlfcHJvamVjdGlvbiA9IGNhbGN1bGF0ZV9zb3VyY2UoJm5leHRfZGF5X3NvdXJjZSkudW53cmFwKCk7CisgICAgcHJpbnRsbiEoCisgICAgICAgICJTS1lfUkVDRUlQVF9ESUFHTk9TVElDIHt9IHt9IiwKKyAgICAgICAgbmV4dF9kYXlfc291cmNlLmRhdGUsCisgICAgICAgIHNlcmRlX2pzb246OnRvX3N0cmluZygKKyAgICAgICAgICAgICZTdHJpbmc6OmZyb21fdXRmOChuZXh0X2RheV9wcm9qZWN0aW9uLnJlY2VpcHQudG9fcHJldHR5X2pzb24oKS51bndyYXAoKSkudW53cmFwKCkKKyAgICAgICAgKQorICAgICAgICAudW53cmFwKCkKKyAgICApOwogICAgIGFzc2VydF9lcSEocHJvamVjdGlvbi5yZWNlaXB0LmRheS5kYXRlLCAiMjAyNC0wNC0wOCIpOwogICAgIGFzc2VydF9lcSEocHJvamVjdGlvbi5yZWNlaXB0LmRheS50aW1lX3pvbmUsICJBbWVyaWNhL05ld19Zb3JrIik7CiAgICAgYXNzZXJ0X2VxIShwcm9qZWN0aW9uLnJlY2VpcHQuZmFjdHMubGVuKCksIDExKTsK')
def stamp():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def git(*a):return subprocess.check_output(['git','-C',str(r),*a],text=True).strip()
assert hashlib.sha256(patch).hexdigest()=='759bddb376364c1a7e89c0386f00b13b8b5b9b6cb0a8e1a421c95ff262fe7ee3'
for suffix in ['-overlay.patch','.stdout.log','.stderr.log','.result.json','-receipts.json']:assert not (out/(prefix+suffix)).exists()
assert git('rev-parse','HEAD')==sha and not git('diff','HEAD','--name-only')
original=json.loads((out/'linux-current-inputs.json').read_text());before=source.read_bytes()
for row in original['tracked_file_inventory']:assert hashlib.sha256((r/row['path']).read_bytes()).hexdigest()==row['sha256'],row['path']
assert before==subprocess.check_output(['git','-C',str(r),'show','HEAD:src/sky_surface/tests.rs'])
lock=subprocess.run(['fuser','/home/markik/Code/target/debug/.cargo-lock'],capture_output=True,text=True);assert lock.returncode==1 and not lock.stdout and not lock.stderr
owners=[];names={'cargo','rustc','rustdoc','cc','c++','clang','clang++','gcc','g++','ld','ld.lld','lld','cmake','ninja','make'}
for p in Path('/proc').iterdir():
 if p.name.isdigit():
  try:
   c=(p/'comm').read_text().strip()
   if c in names or c.startswith('turnstone'):owners.append({'pid':int(p.name),'comm':c})
  except OSError:pass
assert not owners,owners
subprocess.run(['git','-C',str(r),'apply','--check','-'],input=patch,check=True);subprocess.run(['git','-C',str(r),'apply','-'],input=patch,check=True)
applied=source.read_bytes();assert git('diff','HEAD','--name-only').splitlines()==['src/sky_surface/tests.rs'];(out/(prefix+'-overlay.patch')).write_bytes(patch)
args=['nice','-n','10','cargo','+1.98.1','test','--lib','--locked','-j1','sky_surface::tests::reference_source_reproduces_the_p0_receipt','--','--exact','--nocapture'];env=os.environ.copy();env['CARGO_TARGET_DIR']='/home/markik/Code/target';env['CARGO_BUILD_JOBS']='1'
result={'source_sha':sha,'scope':'Temporary cfg(test) Sky output overlay, not clean-published acceptance. Original golden assertions unchanged; production code untouched.','overlay_patch_sha256':hashlib.sha256(patch).hexdigest(),'test_source_before_sha256':hashlib.sha256(before).hexdigest(),'test_source_overlay_sha256':hashlib.sha256(applied).hexdigest(),'command':args,'cwd':str(r),'environment':{k:env.get(k) for k in ['CARGO_TARGET_DIR','CARGO_BUILD_JOBS','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS']},'started_utc':stamp(),'actual_exit_code':None};start=time.monotonic()
try:
 with (out/(prefix+'.stdout.log')).open('wb') as stdout,(out/(prefix+'.stderr.log')).open('wb') as stderr:
  p=subprocess.Popen(args,cwd=r,env=env,stdout=stdout,stderr=stderr);result['owned_pid']=p.pid;(out/(prefix+'.result.json')).write_bytes((json.dumps(result,indent=2)+'\n').encode());print('START Sky diagnostic PID '+str(p.pid),flush=True);result['actual_exit_code']=p.wait()
 result['finished_utc']=stamp();result['elapsed_seconds']=round(time.monotonic()-start,3);result['test_source_after_run_sha256']=hashlib.sha256(source.read_bytes()).hexdigest();assert source.read_bytes()==applied,'Overlay changed during execution'
 records=[]
 for line in (out/(prefix+'.stdout.log')).read_text().splitlines():
  if line.startswith('SKY_RECEIPT_DIAGNOSTIC '):
   _,date,payload=line.split(' ',2);pretty=json.loads(payload);records.append({'date':date,'pretty_json':pretty,'pretty_json_sha256':hashlib.sha256(pretty.encode()).hexdigest(),'receipt':json.loads(pretty)})
 result['diagnostic_record_dates']=[x['date'] for x in records];(out/(prefix+'-receipts.json')).write_bytes((json.dumps(records,indent=2)+'\n').encode())
except BaseException as error:
 result['runner_error']=repr(error);raise
finally:
 if source.read_bytes()==applied:source.write_bytes(before)
 result['final_source_sha']=git('rev-parse','HEAD');result['test_source_restored_sha256']=hashlib.sha256(source.read_bytes()).hexdigest();result['tracked_diff_after_restore']=git('diff','HEAD','--name-only').splitlines();result['restored_exact_original']=source.read_bytes()==before
 result['tracked_input_hash_changes_after_restore']=[]
 for row in original['tracked_file_inventory']:
  h=hashlib.sha256((r/row['path']).read_bytes()).hexdigest()
  if h!=row['sha256']:result['tracked_input_hash_changes_after_restore'].append({'path':row['path'],'before':row['sha256'],'after':h})
 result['output_hashes']={suffix:hashlib.sha256((out/(prefix+suffix)).read_bytes()).hexdigest() for suffix in ['-overlay.patch','.stdout.log','.stderr.log','-receipts.json'] if (out/(prefix+suffix)).is_file()};(out/(prefix+'.result.json')).write_bytes((json.dumps(result,indent=2)+'\n').encode());print(json.dumps(result,indent=2),flush=True)
