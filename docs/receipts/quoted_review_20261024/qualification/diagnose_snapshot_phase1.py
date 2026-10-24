from pathlib import Path
import os,subprocess,shutil,json,time
O=Path(__file__).resolve().parent;N=O/'native-pair-02';D=O/'snapshot-diagnostic'
r=D/'resident'
env={**os.environ,'DJINN_CROSS_SOURCE_QWEN_DIR':'/Users/markik/Code/output/cross-source-answers-20261022/qwen2.5-1.5b-instruct','DJINN_CROSS_SOURCE_QWEN15':'1','ESP_SMOLLM2_DIR':'/Users/markik/Code/output/cross-source-answers-20261022/qwen2.5-1.5b-instruct','DJINN_HEADED_DIR':str(r),'DJINN_HEADED_ROOT':str(D/'private-resident'),'DJINN_HEADED_PHASE':'1','DJINN_HEADED_CLIP_CUSTODY':str(D/'wallet'),'DJINN_HEADED_MIXED_QUERIES':str(N/'frozen/questions.json'),'DJINN_HEADED_EMBEDDING_DIR':'/Users/markik/Code/output/embedding-envelope-20261020/all-MiniLM-L6-v2','TMPDIR':'/tmp','RUST_LOG':'warn'}
r=D/'resident'
with (D/'run-phase1.log').open('w') as f:
 p=subprocess.Popen([str(N/'frozen/inference_route'),'serve_trained_model_for_headed_turnstone','--ignored','--exact','--nocapture'],cwd='/Users/markik/Code/worktrees/mere-cross-source-answers-20261022',env=env,stdout=f,stderr=subprocess.STDOUT)
 (D/'process-phase1.json').write_text(json.dumps({'pid':p.pid,'binary':str(N/'frozen/inference_route'),'boundary':'Clone of failed warm fixture, no native product/window, no new inference; restart request already present.'},indent=2)+'\n')
 try:
  for i in range(24):
   if p.poll() is not None:break
   time.sleep(10)
   if i in [5,11,17]:subprocess.run(['/usr/bin/sample',str(p.pid),'3','1','-file',str(D/f'sample-phase1-{i}.txt')],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
  if p.poll() is None:p.terminate();p.wait(timeout=15)
  (D/'exit-phase1.json').write_text(json.dumps({'exit':p.returncode,'snapshot':(r/'snapshot-1.json').exists()})+'\n')
 finally:
  if p.poll() is None:p.kill();p.wait()
