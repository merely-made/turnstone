// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const driver=fs.readFileSync(path.join(__dirname,'verify-browser.cjs'),'utf8');
const helper=fs.readFileSync(path.join(__dirname,'release.cjs'),'utf8');
const Module=require('node:module'),moduleValue=new Module('qualified-release-helper');
moduleValue._compile(helper,'qualified-release-helper.cjs');
const marker='.finally(async()=>{';
const start=driver.lastIndexOf(marker),end=driver.lastIndexOf('\n});');
if(start<0||end<start)throw Error('Maintained finalizer body not located');
const body=driver.slice(start+marker.length,end);
const AsyncFunction=Object.getPrototypeOf(async function(){}).constructor;
const invoke=new AsyncFunction('clearTimeout','timeout','release','context','browser','server','fs','path','out','process',body);
const cases=[];
async function run(name,resources,expected){
  const writes=[],processState={exitCode:0};
  await invoke(()=>{},null,resources=>moduleValue.exports.release(resources,10),resources.context,resources.browser,resources.server,
    {writeFileSync:(file,text)=>writes.push({file,text})},path,'unit-control-output',processState);
  if(writes.length!==1)throw Error('Expected one release receipt');
  const record=JSON.parse(writes[0].text);
  for(const [key,value]of Object.entries(expected))if(record[key]!==value)throw Error(name+': '+key);
  if(processState.exitCode!==(name==='successful-closes'?0:1))throw Error(name+': process exit did not follow close results');
  cases.push({name,record,driver_exit_code:processState.exitCode});
}
const goodServer=()=>({close:cb=>cb(),listening:false});
async function main(){
  await run('successful-closes',{context:{close:async()=>{}},browser:{close:async()=>{},isConnected:()=>false},server:goodServer()},
    {context_closed:true,browser_closed:true,http_closed:true});
  await run('both-close-calls-reject',{context:{close:async()=>{throw Error('context close rejected');}},
    browser:{close:async()=>{throw Error('browser close rejected');},isConnected:()=>true},server:goodServer()},
    {context_closed:false,browser_closed:false,http_closed:true});
  await run('http-close-rejects',{context:{close:async()=>{}},browser:{close:async()=>{},isConnected:()=>false},
    server:{close:cb=>cb(Error('HTTP close rejected')),listening:true}},
    {context_closed:true,browser_closed:true,http_closed:false});
  await run('browser-still-connected',{context:{close:async()=>{}},browser:{close:async()=>{},isConnected:()=>true},server:goodServer()},
    {context_closed:true,browser_closed:false,http_closed:true});
  await run('browser-close-times-out',{context:{close:async()=>{}},browser:{close:()=>new Promise(()=>{}),isConnected:()=>true},server:goodServer()},
    {context_closed:true,browser_closed:false,http_closed:true});
  console.log(JSON.stringify({driver_sha256:crypto.createHash('sha256').update(driver).digest('hex'),
    helper_sha256:crypto.createHash('sha256').update(helper).digest('hex'),body_sha256:crypto.createHash('sha256').update(body).digest('hex'),
    scope:'Exact maintained finally body and release helper with in-memory receipt sink and owned test doubles; no browser launched',cases,passed:true},null,2));
}
main().catch(error=>{console.error(error);process.exitCode=1;});
