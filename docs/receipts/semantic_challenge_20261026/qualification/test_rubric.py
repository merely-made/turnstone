from pathlib import Path
import copy,importlib.util,json,sys
sys.dont_write_bytecode=True
O=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('verify_suite',O/'verify_suite.py');v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
suite=json.loads((O/'frozen/suite.json').read_text());checks=[]
for cohort in suite['cohorts']:
 for case in cohort['asks']:
  if case.get('expected_admission_error'):continue
  scope=['capture','control','knot','note'] if case['id']=='conflict' else ['capture','knot'] if case['id']=='reduced' else ['capture','knot','note']
  passages=[{'handle':n,'passage':{'source':{'source':'https://probe.test/'+key},'text':cohort['corpus'][key]['text']}} for n,key in enumerate(scope)]
  claims=[]
  for slot,g in enumerate(case['gold']):
   relevant=g['relevant'];agreement='none' if not relevant else 'single' if len(relevant)==1 else 'same' if len({g['values'][k] for k in relevant})==1 else 'different'
   status='insufficient' if not relevant else 'conflicting' if agreement=='different' else 'supported'
   links=[]
   for key in relevant:
    if status=='conflicting' and links and g['values'][key]==g['values'][scope[links[0]['handle']]]:continue
    links.append({'handle':scope.index(key),'quote':cohort['corpus'][key]['text']})
    if len(links)==(2 if status=='conflicting' else 1):break
   claims.append({'fact':{'text':case['question']+' slot '+str(slot)},'review':{'candidates':[{'handle':i,'answers':k in relevant} for i,k in enumerate(scope)],'agreement':agreement},'status':status,'evidence':links})
  base={'text':json.dumps({'claims':claims}),'recall':{'passages':passages}}
  assert all(r['result']=='pass' for r in v.semantic(case,base));checks.append(cohort['name']+'/'+case['id']+': ideal gold accepted')
  for kind in ['relevance','status','agreement']:
   bad=copy.deepcopy(claims)
   if kind=='relevance':bad[0]['review']['candidates'][0]['answers']=not bad[0]['review']['candidates'][0]['answers']
   elif kind=='status':bad[0]['status']='supported' if bad[0]['status']!='supported' else 'insufficient'
   else:bad[0]['review']['agreement']='none' if bad[0]['review']['agreement']!='none' else 'single'
   changed={**base,'text':json.dumps({'claims':bad})};assert any(r['result']=='fail' for r in v.semantic(case,changed));checks.append(cohort['name']+'/'+case['id']+': '+kind+' mutation rejected')
  if cohort['name']=='agreement' and case['id']=='conflict':
   bad=copy.deepcopy(claims);bad[0]['evidence']=[{'handle':scope.index(k),'quote':cohort['corpus'][k]['text']} for k in ['capture','knot']]
   changed={**base,'text':json.dumps({'claims':bad})};errors=v.semantic(case,changed)[0]['errors'];assert any(e['kind']=='conflict_does_not_show_distinct_values' for e in errors);checks.append('identical quotes from distinct sources rejected as conflict')
report={'result':'pass','count':len(checks),'checks':checks,'boundary':'Tests the external semantic rubric using synthetic ideal and mutated documents. These are not model successes or product tests.'}
(O/'rubric-tests.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:r for k,r in report.items() if k!='checks'},indent=2))
