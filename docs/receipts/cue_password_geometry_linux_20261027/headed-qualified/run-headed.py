from pathlib import Path
import os,subprocess,json,time,hashlib,http.server,functools,threading,re,signal,shutil
base=Path('<home>/Code/output/genet-password-caret');root=base/'headed-qualified';assert not root.exists();root.mkdir();receiving=root/'receiving';receiving.mkdir()
repo=Path('<home>/Code/worktrees/turnstone-password-caret-20261027');exe=base/'turnstone-linux'
for name in ['geometry.html','geometry-wayland.scn','run-headed.py']:shutil.copy2(base/name,root/name)
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(http.server.SimpleHTTPRequestHandler,directory=str(root)));threading.Thread(target=server.serve_forever,daemon=True).start()
env=os.environ.copy();env.pop('DISPLAY',None);env.update(XDG_RUNTIME_DIR='/run/user/1000',WAYLAND_DISPLAY='wayland-0',XDG_SESSION_TYPE='wayland',DBUS_SESSION_BUS_ADDRESS='unix:path=/run/user/1000/bus',WGPU_BACKEND='vulkan',VK_DRIVER_FILES='/usr/share/vulkan/icd.d/radeon_icd.x86_64.json',TURNSTONE_ROOT=str(root/'profile'),MERE_ROOT=str(root/'mere'),TURNSTONE_SCENARIO=str(root/'geometry-wayland.scn'),TURNSTONE_CAPTURE_DIR=str(receiving),SERVO_FIXTURE_BASE=f'http://127.0.0.1:{server.server_port}',TURNSTONE_A4_DIAGNOSTICS='1',RUST_LOG='turnstone=info,turnstone::ime=debug',WAYLAND_DEBUG='client',NO_COLOR='1')
h=hashlib.sha256()
with exe.open('rb') as f:
 for b in iter(lambda:f.read(1048576),b''):h.update(b)
result={'source_head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(),'binary_sha256':h.hexdigest(),'inputs':{n:hashlib.sha256((root/n).read_bytes()).hexdigest() for n in ['geometry.html','geometry-wayland.scn','run-headed.py']},'requested_backend':'Vulkan','wayland_display':'wayland-0','qualified':False,'physical_input':False};started=time.time();child=None
try:
 with (root/'stdout.log').open('w') as stdout,(root/'stderr.log').open('w') as stderr:
  child=subprocess.Popen([str(exe)],cwd=repo,env=env,stdout=stdout,stderr=stderr,start_new_session=True)
  try:result['exit']=child.wait(timeout=360)
  except subprocess.TimeoutExpired:result['timed_out']=True
  finally:
   if child.poll() is None:
    os.killpg(child.pid,signal.SIGTERM)
    try:child.wait(timeout=5)
    except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
   result['exit']=child.returncode
finally:
 server.shutdown();server.server_close();result['elapsed_seconds']=round(time.time()-started,3)
 text=(root/'stdout.log').read_text();protocol=(root/'stderr.log').read_text();areas=[]
 for line in text.splitlines():
  if 'shared document caret geometry' in line:
   values={k:float(v) for k,v in re.findall(r'\b(window|x|y|width|height)=([0-9.eE+\-]+)',line)}
   if values:areas.append(values)
 result['shared_caret_observations']=areas;result['windows_with_geometry']=sorted({int(x['window']) for x in areas if 'window' in x});result['wayland_cursor_rectangles']=re.findall(r'zwp_text_input_v3[^\n]*set_cursor_rectangle[^\n]*',protocol)
 outcome=receiving/'scenario.done';result['scenario_outcome']=outcome.read_text() if outcome.exists() else None
 source_captures=[];scales={}
 for path in sorted(receiving.glob('*.sources.json')):
  for item in json.loads(path.read_text()):
   source_captures.append({'capture':path.name,**item});scales[int(item['window'])]=float(item['native_window_scale'])
 result['source_captures']=source_captures
 protocol_rects=[tuple(map(int,m)) for m in re.findall(r'zwp_text_input_v3[^\n]*set_cursor_rectangle\((-?\d+), (-?\d+), (-?\d+), (-?\d+)\)',protocol)]
 matches={}
 for window in [0,1]:
  scale=scales.get(window)
  matches[window]=bool(scale) and any(all(abs(a[k]/scale-b[i])<=1.0 for i,k in enumerate(['x','y','width','height'])) for a in areas if int(a.get('window',-1))==window for b in protocol_rects)
 result['window_rectangles_match_wayland']=matches
 hardware_windows={int(x['window']) for x in source_captures if x['adapter']['backend']=='Vulkan' and x['adapter']['device_type']!='Cpu'}
 result['qualified']=result.get('exit')==0 and (result.get('scenario_outcome') or '').startswith('RESULT ok') and {0,1}<=set(result['windows_with_geometry']) and {0,1}<=hardware_windows and all(matches.values())
 (root/'run.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if k not in ['shared_caret_observations','wayland_cursor_rectangles']},indent=2),flush=True)
 print('Caret observations',len(areas),'Wayland rectangles',len(result['wayland_cursor_rectangles']),flush=True)
raise SystemExit(0 if result['qualified'] else 1)
