// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
const test=require('node:test'),assert=require('node:assert/strict');
const http=require('node:http');
const {release}=require('./release.cjs');

test('all owned closes must actually finish, including a real loopback server',async()=>{
  const server=http.createServer();await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  let connected=true;
  const result=await release({context:{close:async()=>{}},
    browser:{close:async()=>{connected=false;},isConnected:()=>connected},server});
  assert.equal(result.context_closed,true);assert.equal(result.browser_closed,true);
  assert.equal(result.http_closed,true);assert.deepEqual(result.close_errors,[]);
});
test('a rejected context close does not prevent browser/server close',async()=>{
  const result=await release({context:{close:async()=>{throw Error('context failed');}},
    browser:{close:async()=>{},isConnected:()=>false},
    server:{close:cb=>cb(),listening:false}});
  assert.equal(result.context_closed,false);assert.equal(result.browser_closed,true);
  assert.equal(result.http_closed,true);assert.equal(result.close_errors[0].resource,'context');
});
test('rejected browser and HTTP closes stay false',async()=>{
  const result=await release({browser:{close:async()=>{throw Error('browser failed');},isConnected:()=>true},
    server:{close:cb=>cb(Error('HTTP failed')),listening:true}});
  assert.equal(result.browser_closed,false);assert.equal(result.http_closed,false);
  assert.equal(result.close_errors.length,2);
});
test('a resolved close with an open connection is not success',async()=>{
  const result=await release({browser:{close:async()=>{},isConnected:()=>true}});
  assert.equal(result.browser_closed,false);assert.equal(result.close_errors.length,1);
});
test('a stuck close times out with no closure claim',async()=>{
  const result=await release({browser:{close:()=>new Promise(()=>{}),isConnected:()=>true}},10);
  assert.equal(result.browser_closed,false);assert.match(result.close_errors[0].error,/timed out/);
});
