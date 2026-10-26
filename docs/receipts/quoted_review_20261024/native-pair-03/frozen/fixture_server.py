#!/usr/bin/env python3
"""Bounded local HTML capture origin with an append-only request observation."""
import argparse, html, json, threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

p=argparse.ArgumentParser();p.add_argument('--cases',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--ready-file',type=Path,required=True);a=p.parse_args()
a.output.mkdir();cases=json.loads(a.cases.read_text());requests=[];lock=threading.Lock()
pages={v['path']:v for v in cases['corpus'].values() if 'path' in v}
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  with lock:
   requests.append({'method':'GET','path':self.path});(a.output/'requests.json').write_text(json.dumps(requests,indent=2)+'\n')
  page=pages.get(self.path)
  body=(f"<!doctype html><meta charset=utf-8><title>{html.escape(page['title'])}</title><main><p>{html.escape(page['text'])}</p></main>" if page else 'not found').encode()
  self.send_response(200 if page else 404);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(body)));self.send_header('Cache-Control','no-store');self.end_headers();self.wfile.write(body)
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);server.daemon_threads=True
host,port=server.server_address;a.ready_file.write_text(json.dumps({'host':host,'port':port,'base_url':f'http://{host}:{port}'})+'\n')
try:server.serve_forever(poll_interval=.1)
finally:server.server_close()
