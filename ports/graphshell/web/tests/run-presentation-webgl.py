from pathlib import Path
from http.server import ThreadingHTTPServer, SimpleHTTPRequestHandler
import argparse,subprocess,signal,os,json,time,threading
ap=argparse.ArgumentParser();ap.add_argument('--out',dest='output',type=Path,required=True);ap.add_argument('--browser',choices=['chrome','firefox'],default='chrome');ap.add_argument('--headed',action='store_true');ap.add_argument('--angle',choices=['gl','vulkan','swiftshader']);ap.add_argument('--web',type=Path,required=True);ap.add_argument('--backend',choices=['auto','webgpu','webgl2'],default='webgl2');ap.add_argument('--disable-webgpu',action='store_true');ap.add_argument('--force-context-loss',action='store_true');ap.add_argument('--port',type=int,default=8975);a=ap.parse_args();a.output.mkdir(exist_ok=False)
root=a.web;browser=None;server=None
class Handler(SimpleHTTPRequestHandler):
 def __init__(self,*args,**kwargs):super().__init__(*args,directory=str(root),**kwargs)
 def do_POST(self):
  if self.path!='/result':self.send_error(404);return
  length=int(self.headers.get('Content-Length','0'));assert 0<length<1024*1024
  result=json.loads(self.rfile.read(length));(a.output/'result.json').write_text(json.dumps(result,indent=2)+'\n')
  self.send_response(204);self.end_headers()
 def log_message(self,*args):pass
try:
 server=ThreadingHTTPServer(('127.0.0.1',a.port),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
 angle=a.angle or ('vulkan' if a.backend=='webgpu' else 'gl')
 command=['/var/lib/flatpak/app/com.google.Chrome/x86_64/stable/active/files/extra/chrome','--headless=new','--no-first-run','--no-default-browser-check','--use-gl=angle','--use-angle='+angle,'--user-data-dir='+str(a.output/'profile'),f'http://127.0.0.1:{a.port}/?render_backend={a.backend}&force_context_loss={str(a.force_context_loss).lower()}']
 if angle=='vulkan':command[1:1]=['--enable-features=Vulkan','--use-vulkan=native']
 if angle=='swiftshader':command.insert(1,'--enable-unsafe-swiftshader')
 if a.disable_webgpu:command.insert(1,'--disable-features=WebGPU')
 if a.backend=='webgpu':command.insert(1,'--enable-unsafe-webgpu')
 if a.headed:command.remove('--headless=new')
 if a.browser=='firefox':
  (a.output/'profile').mkdir()
  if a.disable_webgpu:(a.output/'profile/user.js').write_text('user_pref("dom.webgpu.enabled", false);\n')
  command=['/usr/bin/firefox','--no-remote','--profile',str(a.output/'profile')]
  if not a.headed:command.append('--headless')
  command.append(f'http://127.0.0.1:{a.port}/?render_backend={a.backend}&force_context_loss={str(a.force_context_loss).lower()}')
 browser=subprocess.Popen(command,stdout=(a.output/'chrome.log').open('w'),stderr=subprocess.STDOUT,start_new_session=True)
 scope=Path(f'/proc/{browser.pid}/cgroup').read_text().strip();assert scope==Path('/proc/self/cgroup').read_text().strip(),scope
 (a.output/'launch.json').write_text(json.dumps({'command':command,'browser_pid':browser.pid,'cgroup':scope},indent=2)+'\n')
 deadline=time.monotonic()+90
 while not (a.output/'result.json').exists():
  assert time.monotonic()<deadline,'fixture wall-clock timeout';assert browser.poll() is None,'browser exited';time.sleep(.1)
 result=json.loads((a.output/'result.json').read_text())
 import base64,hashlib
 captures=[]
 for capture in result.pop('captures',[]):
  pixels=base64.b64decode(capture.pop('png').split(',',1)[1]);path=a.output/(capture['name']+'.png');path.write_bytes(pixels)
  capture['png_sha256']=hashlib.sha256(pixels).hexdigest();captures.append(capture)
 result['captures']=captures
 bundle=root/'pkg'/result.get('artifact','')/'graphshell_web_bg.wasm'
 result['bundle_sha256']=hashlib.sha256(bundle.read_bytes()).hexdigest() if bundle.is_file() else None
 result['policy']={'backend':a.backend,'disable_webgpu':a.disable_webgpu,'force_context_loss':a.force_context_loss,'angle':a.angle}
 (a.output/'summary.json').write_text(json.dumps(result,indent=2)+'\n')
 print(json.dumps({'result':result['result'],'controls':len(result['controls']),'errors':result.get('errors'), 'renderer':result.get('rendererDebug')}),flush=True)
 assert result['result']=='pass',result.get('errors')
finally:
 if browser is not None:
  try:os.killpg(browser.pid,signal.SIGTERM);browser.wait(timeout=3)
  except ProcessLookupError:pass
  except subprocess.TimeoutExpired:os.killpg(browser.pid,signal.SIGKILL);browser.wait(timeout=3)
 if server is not None:server.shutdown();server.server_close()
