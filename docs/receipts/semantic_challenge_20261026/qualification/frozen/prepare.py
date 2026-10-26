from pathlib import Path
import datetime,hashlib,json,re,shutil,subprocess
O=Path(__file__).resolve().parent
B=Path('/Users/markik/Code/output/quoted-review-20261024/order-replay/candidate-01')
M=Path('/Users/markik/Code/worktrees/mere-cross-source-answers-20261022')
def write(p,v):p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def slot(relevant,values=None):return {'relevant':relevant,'values':values or {k:k for k in relevant}}
def ask(id,q,slots,admission=None):return {'id':id,'question':q,'gold':slots,**({'expected_admission_error':admission} if admission else {})}
def cohort(name,corpus,asks):return {'name':name,'corpus':{k:{'text':v} for k,v in corpus.items()},'asks':asks}
old=json.loads((B/'frozen/cases.json').read_text())['corpus']; old={k:v['text'] for k,v in old.items()}
cs=[]
cs.append(cohort('paraphrases',old,[
 ask('cross',"What are the archive emergency phrase, the observatory entry word, and the listener's saved reminder? Give one claim for each, in that order.",[slot(['capture']),slot(['knot']),slot(['note'])]),
 ask('missing','How much money is allocated annually to the Juniper archive?',[slot([])]),
 ask('conflict','Which recovery phrase should I use for the Juniper archive? Report incompatible retained answers.',[slot(['capture','control'],{'capture':'violet lantern','control':'amber harbor'})]),
 ask('reduced','What did the saved listener ask me to remember?',[slot([])])]))
cs.append(cohort('qualifiers',{
 'capture':"The Pelican observatory's access word is café. This note gives no access word for the Cormorant observatory.",
 'control':"The Cormorant observatory's access word is cedar.",
 'knot':"The Juniper archive's former recovery phrase was violet lantern; its current phrase is not recorded here.",
 'note':"A volunteer guessed that the Fern reserve budget is $7000; no budget has been approved."
},[
 ask('cross','What is the Pelican observatory access word?',[slot(['capture'])]),
 ask('missing','What is the approved Fern reserve budget?',[slot([])]),
 ask('conflict','What is the Cormorant observatory access word?',[slot(['control'])]),
 ask('reduced','What is the current Juniper archive recovery phrase?',[slot([])])]))
cs.append(cohort('agreement',{
 'capture':'The Bracken ferry departure time is 09:30.',
 'control':'The Bracken ferry departure time is 10:15.',
 'knot':'The Bracken ferry departure time is 09:30.',
 'note':'The Pelican ferry departure time is 08:00.'
},[
 ask('cross','What is the Bracken ferry departure time?',[slot(['capture','knot'],{'capture':'09:30','knot':'09:30'})]),
 ask('missing','What is the Bracken ferry arrival time?',[slot([])]),
 ask('conflict','What is the Bracken ferry departure time?',[slot(['capture','control','knot'],{'capture':'09:30','control':'10:15','knot':'09:30'})]),
 ask('reduced','What is the Bracken ferry departure time?',[slot(['capture','knot'],{'capture':'09:30','knot':'09:30'})])]))
long="Broomgate station log: the access code is {code}. The lobby has two benches, a blue noticeboard, a clock, a rack of folded maps, a painted floor, a shelf of empty folders, three hooks for coats, a brass bell, a small table, a glass door, a mat, a lamp, and a bin."
lcorpus={'capture':long.format(code='maple-42'),'control':long.format(code='birch-73'),'knot':'The Broomgate quay is open.','note':'Broomgate has no recorded maintenance budget.'}
cs.append(cohort('long_passages',lcorpus,[
 ask('cross','What is the Broomgate station access code?',[slot(['capture'])]),
 ask('missing','What is the Broomgate annual maintenance budget?',[slot([])]),
 ask('reduced','What is the Broomgate quay access code?',[slot([])])]))
cs.append(cohort('long_conflict_admission',lcorpus,[ask('conflict','What is the Broomgate station access code?',[slot(['capture','control'],{'capture':'maple-42','control':'birch-73'})], 'extractive conflict text exceeds the 384-byte claim bound')]))
assert sum(len(c['asks']) for c in cs)==16
# Check gold and passage-size expectations before observing the model.
for c in cs:
 for a in c['asks']:
  scope=['capture','control','knot','note'] if a['id']=='conflict' else ['capture','knot'] if a['id']=='reduced' else ['capture','knot','note']
  assert all(set(s['relevant'])<=set(scope) for s in a['gold'])
  p=[c['corpus'][k]['text'] for k in scope]
  enc=lambda s:len(json.dumps(s,ensure_ascii=False,separators=(',',':')).encode())-2
  oversize=any(enc(t)>384 for t in p) or any(enc('The supplied sources disagree: '+u+' / '+v)>384 for i,u in enumerate(p) for v in p[i+1:])
  assert oversize==bool(a.get('expected_admission_error')),(c['name'],a['id'],[enc(t) for t in p])
F=O/'frozen';F.mkdir()
for name in ['answer_contract.rs','candidate_review.rs','recall.rs','recall_embedding.rs']:
 src=M/'ports/distillery/src/inference'/name
 shutil.copy2(src,F/name)
for name in ['verify_contract_inputs.py','verify_structured_output.py','verify_score_health.py']:
 shutil.copy2(B/'frozen'/name,F/name)
shutil.copy2(M/'ports/djinn/tests/inference_route.rs',F/'inference_route.rs')
shutil.copy2(Path(__file__),F/'prepare.py')
write(F/'suite.json',{'version':1,'kind':'synthetic lexical semantic challenge','cases':16,'model_requests':15,'expected_admission_refusals':1,'cohorts':cs,'rubric':'Every supplied candidate relevance flag, agreement, status and selected support must match the frozen property-specific gold. Conflict links must represent distinct gold values, not merely distinct sources. Missing required relevant passages is a retrieval failure. SemanticReview remains unreviewed. No prompt, grammar, model, token or timeout tuning on this challenge.'})
for c in cs:
 d=O/c['name'];d.mkdir();write(d/'cases.json',c)
model=Path('/Users/markik/Code/output/cross-source-answers-20261022/qwen2.5-1.5b-instruct');mi=json.loads((model/'verified.json').read_text())
for name,size,digest in mi['artifacts']:assert (model/name).stat().st_size==size and sha(model/name)==digest
binary=B/'frozen/inference_route';assert sha(binary)==json.loads((B/'inputs.json').read_text())['binary_sha256']
subprocess.run(['cp','-c','-p',str(binary),str(F/'inference_route')],check=True);assert sha(F/'inference_route')==sha(binary)
source={}
for repo in ['eidetic','esp','knot','cleromancy','gemot','murm','mere','turnstone']:
 p=Path('/Users/markik/Code/worktrees')/(repo+'-cross-source-answers-20261022')
 source[repo]={'path':str(p),'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=p,text=True).strip(),'status':subprocess.check_output(['git','status','--porcelain=v1'],cwd=p,text=True),'branch':subprocess.check_output(['git','branch','--show-current'],cwd=p,text=True).strip()}
write(O/'freeze.json',{'frozen_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'baseline':str(B),'baseline_binary_sha256':sha(binary),'generation_model':mi,'sources':source,'sha256':{p.name:sha(p) for p in F.iterdir() if p.is_file()},'limits':{'tokens':1024,'seconds_per_request':900,'host_weight_bytes':3*1024**3,'device_layer_bytes':4*1024**3},'preservation':'Existing baseline outputs untouched; executable APFS clone verified by SHA256; no Cargo compilation.'})
print(json.dumps({'frozen':str(O),'cases':16,'model_requests':15,'admission_refusals':1,'passage_lengths':{k:len(v.encode()) for k,v in lcorpus.items()},'dirty_sources':{k:v['status'] for k,v in source.items() if v['status']}},indent=2))
