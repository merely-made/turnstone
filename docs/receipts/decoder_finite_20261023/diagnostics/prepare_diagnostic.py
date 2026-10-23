from pathlib import Path
import json,re,hashlib,subprocess
O=Path(__file__).resolve().parent;prior=O.parent/'requested-facts-20261023';E=Path('/Users/markik/Code/worktrees/esp-cross-source-answers-20261022');M=Path('/Users/markik/Code/worktrees/mere-cross-source-answers-20261022')
state={}
for n in ['eidetic','esp','knot','cleromancy','gemot','murm','mere','turnstone']:
 p=E.parent/(n+'-cross-source-answers-20261022');state[n]={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=p,text=True).strip(),'status':subprocess.check_output(['git','status','--porcelain=v1','--untracked-files=all'],cwd=p,text=True)};assert not state[n]['status']
(O/'preservation-before.json').write_text(json.dumps(state,indent=2)+'\n')
r=json.loads((prior/'candidate-01/cross.json').read_text());c=r['recall']['answer_contract'];source=(M/'ports/distillery/src/inference/answer_contract.rs').read_text();instructions=re.search(r'const INSTRUCTIONS_V5: &str = r#"(.*?)"#;',source,re.S).group(1)
def wire(x):return json.dumps(x,ensure_ascii=False,separators=(',',':'))
facts=c['requested_facts']['facts'];prompt=instructions+f'REQUESTED_FACTS (fixed count {len(facts)}, in this order):\n'+wire(facts)+'\nQUESTION (JSON string): '+wire(r['question'])+'\nReturn the complete minified JSON object.'
context='EVIDENCE candidates (the entire available evidence set):\n'+wire([{'handle':p['handle'],'quote':p['passage']['text']} for p in r['recall']['passages']])+'\n'
import sys;sys.path.insert(0,str(prior));import verify_contract_inputs as v
assert v.digest(prompt)==c['prompt'] and v.digest(context)==c['context']
t=json.loads((E/'crates/esp/templates/qwen2-instruct-v1.json').read_text());rendered=t['bos']+t['system']['prefix']+t['system_prompt']+t['system']['suffix']+t['user']['prefix']+t['context']['prefix']+context+t['context']['suffix']+prompt+t['user']['suffix']+t['assistant']['prefix']
(O/'rendered-prompt.txt').write_text(rendered);(O/'grammar.json').write_text(wire(v.grammar(r['recall'],facts)))
model=O.parent/'cross-source-answers-20261022/qwen2.5-1.5b-instruct';verified=json.loads((model/'verified.json').read_text())
for n,size,digest in verified['artifacts']:
 p=model/n;assert p.stat().st_size==size and hashlib.sha256(p.read_bytes()).hexdigest()==digest
(O/'model-verified.json').write_text(json.dumps(verified,indent=2)+'\n')
print('Diagnostic prompt and frozen grammar reconstructed; model artifact hashes verified')
