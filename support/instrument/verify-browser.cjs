// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const {parseArgs}=require('node:util');
const {release}=require('./release.cjs');
const {values:args}=parseArgs({options:{repo:{type:'string'},package:{type:'string'},
  output:{type:'string'},playwright:{type:'string'},channel:{type:'string',default:'chrome'},
  'max-bytes':{type:'string',default:'100000000'},help:{type:'boolean'}}});
if(args.help){console.log('verify-browser.cjs --repo REPO --package FROZEN_PACKAGE --output NEW_DIR [--playwright MODULE] [--channel chrome]');process.exit(0);}
for(const name of ['repo','package','output'])assert.ok(args[name],'Required --'+name);
const repo=path.resolve(args.repo),base=path.resolve(args.package),out=path.resolve(args.output);
const maxBytes=Number(args['max-bytes']);assert.ok(Number.isSafeInteger(maxBytes)&&maxBytes>0);
function checkBudget(){let total=0;function visit(dir){for(const entry of fs.readdirSync(dir,{withFileTypes:true})){const p=path.join(dir,entry.name);if(entry.isDirectory())visit(p);else total+=fs.statSync(p).size;}}visit(out);assert.ok(total<=maxBytes,'Receipt budget exceeded');}
function inside(child,parent){const rel=path.relative(parent,child);return rel===''||(!rel.startsWith('..'+path.sep)&&rel!=='..'&&!path.isAbsolute(rel));}
for(const root of [repo,base])assert.ok(!inside(out,root)&&!inside(root,out),'Output must be separate from source and package');
const {chromium}=require(args.playwright||'playwright');
assert.equal(process.platform,'darwin','This existing browser qualification lane is macOS-only');
const {execFileSync}=require('node:child_process');
assert.ok(!/<key>CGSSessionScreenIsLocked<\/key>\s*<true\/>/.test(execFileSync('/usr/sbin/ioreg',['-n','Root','-d1','-a'],{encoding:'utf8'})),'Console must be unlocked');
const web=path.join(base,'web');
const candidate=JSON.parse(fs.readFileSync(path.join(base,'candidate.json')));
for(const suffix of ['.js','.wasm','.html'])assert.ok(Object.keys(candidate.package_sha256).some(p=>path.extname(p)===suffix),'Incomplete package manifest');
const hash=p=>crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
for(const [p,h]of Object.entries(candidate.source_sha256)){const source=path.resolve(repo,p);assert.ok(inside(source,repo));assert.equal(hash(source),h,'Source drift: '+p);}
for(const [p,h]of Object.entries(candidate.package_sha256)){const source=path.resolve(base,p);assert.ok(inside(source,base));assert.equal(hash(source),h,'Package drift: '+p);}
fs.mkdirSync(path.dirname(out),{recursive:true});fs.mkdirSync(out);
fs.writeFileSync(path.join(out,'input.json'),JSON.stringify({source:candidate.source,candidate_sha256:hash(path.join(base,'candidate.json'))},null,2));
// The caller owns an unlocked headed lane. This driver does not unlock consoles.
let browser,context,server;const runs=[],errors=[],messages=[];let timeout;
async function main(){
  server=http.createServer((req,res)=>{const rel=decodeURIComponent(new URL(req.url,'http://localhost').pathname).replace(/^\//,'');const p=path.resolve(web,rel||'workspace.html');if(!p.startsWith(web+path.sep)||!fs.existsSync(p)||!fs.statSync(p).isFile()){res.writeHead(404);res.end();return;}res.setHeader('Content-Type',({'.html':'text/html','.js':'text/javascript','.css':'text/css','.wasm':'application/wasm','.json':'application/json','.ttf':'font/ttf'})[path.extname(p)]||'application/octet-stream');fs.createReadStream(p).pipe(res);});
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const origin=`http://127.0.0.1:${server.address().port}`;
  browser=await chromium.launch({channel:args.channel,headless:false,args:['--enable-unsafe-webgpu']});
  context=await browser.newContext({viewport:{width:1280,height:900},deviceScaleFactor:1});
  const page=await context.newPage();page.setDefaultTimeout(20000);
  page.on('pageerror',e=>{errors.push(e.stack||e.message);fs.writeFileSync(path.join(out,'errors.json'),JSON.stringify(errors,null,2));});
  page.on('console',m=>{messages.push({type:m.type(),text:m.text()});fs.writeFileSync(path.join(out,'console.json'),JSON.stringify(messages,null,2));});
  async function mount(){await page.goto(origin+'/workspace.html?motion=normal');await page.bringToFront();await page.waitForFunction(()=>document.querySelector('graphshell-tree')?.dataset.ready==='true',null,{timeout:90000});assert.equal(new URL(page.url()).searchParams.get('app'),'local');}
  async function run(name,text){
    await page.evaluate(async text=>{const m=await import('./pkg/graphshell_web.js');m.run_scenario(text)},text);
    await page.waitForFunction(()=>['ok','fail'].includes(document.body.dataset.scenario),null,{timeout:90000});
    const receipt=await page.evaluate(()=>({scenario:window.graphshellScenario(),host:window.graphshellReceipt(),semantic:window.graphshellSemanticTree(),hidden:document.hidden}));
    fs.writeFileSync(path.join(out,name+'.json'),JSON.stringify({...receipt,errors},null,2));await page.screenshot({path:path.join(out,name+'.png')});checkBudget();
    assert.equal(receipt.scenario.result?.result,'ok',JSON.stringify(receipt.scenario));assert.equal(receipt.scenario.gate_failures.length,0);assert.equal(errors.length,0);assert.equal(receipt.hidden,false);runs.push(name);console.log('PASS',name);return receipt;
  }
  await mount();
  const adapter=await page.evaluate(async()=>{const a=await navigator.gpu.requestAdapter();return a?{vendor:a.info.vendor,architecture:a.info.architecture,device:a.info.device}:null;});
  assert.ok(adapter&&/amd|ati|radeon/i.test(JSON.stringify(adapter))&&!/swiftshader|llvmpipe/i.test(JSON.stringify(adapter)),'Real AMD adapter required');
  await page.getByRole('heading',{name:'Graphshell',exact:true}).waitFor({state:'attached'});
  const entry=await run('wide-entry','assert snap source == saved graph\nremember entry\nsettle 3\ncapture instrument_wide_entry\n');
  const scene=entry.scenario.result.saved_graph.session;assert.ok(scene);
  await run('forme-original',fs.readFileSync(path.join(web,'scenarios/p4_tree_forme.scn'),'utf8'));
  await mount();
  const reopen=await run('forme-reopen',fs.readFileSync(path.join(web,'scenarios/p4_tree_forme_reopen.scn'),'utf8'));
  assert.equal(reopen.scenario.result.saved_graph.session,scene,'Entry and reopen preserve the real session');
  await run('open-reading','settle 3\ncapture instrument_reading\n');
  await page.getByRole('heading',{name:'Workbench',exact:true}).waitFor({state:'attached'});
  const appearance=page.locator('.graphshell-appearance');await appearance.locator('summary').click();
  await appearance.getByRole('combobox',{name:'Appearance mode',exact:true}).selectOption('light');
  await run('light-workbench','settle 4\ncapture instrument_light_workbench\n');
  await appearance.locator('summary').click();
  await page.setViewportSize({width:520,height:860});
  await run('narrow-workbench','settle 4\ncapture instrument_narrow_workbench\n');
  const narrow=await run('narrow-graph','click role:button Return to Mere\nsettle 4\ncapture instrument_narrow_graph\n');
  assert.equal(narrow.scenario.result.saved_graph.session,scene);
  await page.getByRole('heading',{name:'Dataspace',exact:true}).waitFor({state:'attached'});
  await page.getByRole('button',{name:'Graph commands',exact:true}).press('Enter');
  const search=page.getByRole('textbox',{name:'Search graph commands',exact:true});await search.waitFor();await search.press('Escape');
  await run('commands-focus','assert snap commands-open == false\nassert snap focus-kind == graph\n');
  fs.writeFileSync(path.join(out,'summary.json'),JSON.stringify({source:candidate.source,runs,adapter,browser:browser.version(),headed:true,page_errors:errors.length,session:scene},null,2));
}
Promise.race([main(),new Promise((_,reject)=>{timeout=setTimeout(()=>reject(new Error('Owned browser run deadline exceeded')),8*60*1000);})])
.catch(e=>{fs.writeFileSync(path.join(out,'failure.json'),JSON.stringify({message:e.stack||e.message,runs,errors},null,2));console.error(e);process.exitCode=1;})
.finally(async()=>{clearTimeout(timeout);const record=await release({context,browser,server});
  fs.writeFileSync(path.join(out,'release.json'),JSON.stringify(record,null,2));
  if(record.close_errors.length||!record.browser_closed||!record.http_closed||!record.context_closed)process.exitCode=1;
});
