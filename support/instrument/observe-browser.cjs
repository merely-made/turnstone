// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

// Passive observation of the driver's own Chrome child and HTTP listener.
// Load with node --require; this observer never closes another resource.
const fs=require('node:fs'),path=require('node:path'),os=require('node:os');
const cp=require('node:child_process'),http=require('node:http'),assert=require('node:assert/strict');
const output=process.argv[process.argv.indexOf('--output')+1];
assert.ok(process.argv.includes('--output')&&output);
const record={schema:'instrument-runtime/v1',spawned:[],ports:[]};
const owned=new Map();
function rows(){return cp.execFileSync('/bin/ps',['-axo','pid=,ppid=,comm='],{encoding:'utf8'}).split('\n').flatMap(s=>{
 const m=s.match(/^\s*(\d+)\s+(\d+)\s+(.*)$/);return m?[{pid:+m[1],ppid:+m[2],name:m[3]}]:[];
});}
record.existing_browser_pids=rows().filter(r=>r.name.includes('Google Chrome')).map(r=>r.pid);
const sampler=setInterval(()=>{const current=rows();let changed=true;
 while(changed){changed=false;for(const row of current)if((record.spawned.some(e=>e.pid===row.pid)||owned.has(row.ppid))&&!owned.has(row.pid)){owned.set(row.pid,row.name);changed=true;}}
},1000);sampler.unref();
const spawn=cp.spawn;
cp.spawn=function(file,args){const child=spawn.apply(this,arguments);
 if(String(file).endsWith('/Google Chrome')){
  const profile=args.find(s=>s.startsWith('--user-data-dir='))?.slice('--user-data-dir='.length);
  const entry={pid:child.pid,profile,executable:String(file),exit_observed:false};record.spawned.push(entry);
  child.once('exit',(code,signal)=>Object.assign(entry,{exit_observed:true,exit_code:code,exit_signal:signal}));
 }
 return child;
};
const create=http.createServer;
http.createServer=function(){const server=create.apply(this,arguments);
 server.once('listening',()=>{const address=server.address();record.ports.push(address.port);});return server;
};
let observed=false;
process.once('beforeExit',async()=>{
 if(observed)return;observed=true;
 clearInterval(sampler);
 try{
  assert.equal(record.spawned.length,1);assert.equal(record.ports.length,1);
  const entry=record.spawned[0];assert.ok(entry.profile);assert.ok(!record.existing_browser_pids.includes(entry.pid));
  assert.ok(entry.profile.startsWith(os.tmpdir())||entry.profile.startsWith('/private'+os.tmpdir()));
  assert.ok(path.basename(entry.profile).startsWith('playwright_chromiumdev_profile-'));
  const current=rows();record.owned_pids_observed=[...owned.keys()];assert.ok(owned.has(entry.pid));
  record.owned_pids_remaining=current.filter(r=>owned.get(r.pid)===r.name||r.pid===entry.pid||r.ppid===entry.pid).map(r=>r.pid);
  record.temporary_profile_removed=!fs.existsSync(entry.profile);
  assert.ok(entry.exit_observed);assert.deepEqual(record.owned_pids_remaining,[]);assert.ok(record.temporary_profile_removed);
  const probe=create();await new Promise((resolve,reject)=>{probe.once('error',reject);probe.listen(record.ports[0],'127.0.0.1',resolve);});
  await new Promise((resolve,reject)=>probe.close(e=>e?reject(e):resolve()));record.http_port_rebound=true;
  record.passed=true;
 }catch(e){record.failure=e.stack;record.passed=false;process.exitCode=1;}
 if(fs.existsSync(output))fs.writeFileSync(path.join(output,'runtime.json'),JSON.stringify(record,null,2));
});
