// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import assert from 'node:assert/strict';
const definitions=new Map();
globalThis.HTMLElement=class {querySelector(){return null}};
globalThis.customElements={get:name=>definitions.get(name),define:(name,type)=>definitions.set(name,type)};
globalThis.location={search:'?render_backend=webgl2'};
const source=fs.readFileSync(process.env.MOUNT_SOURCE ?? 'ports/graphshell/web/mount.js','utf8');
// Import from a real file: mount.js resolves the physics Worker's URL against import.meta.url.
const sandbox=fs.mkdtempSync(path.join(os.tmpdir(),'mount-test-'));fs.writeFileSync(path.join(sandbox,'mount.js'),source);
globalThis.MutationObserver=class{observe(){}disconnect(){}};
const {defineElements,chooseArtifact}=await import(pathToFileURL(path.join(sandbox,'mount.js')).href);
let mounts=0,disposals=0;
defineElements({mount_tree:root=>{mounts++; assert.equal(root.attrs.get('data-render-backend'),'webgl2');},dispose_tree:()=>disposals++});
const Type=definitions.get('graphshell-tree');
const root=new Type(); root.dataset={};root.attrs=new Map();root.hasAttribute=k=>root.attrs.has(k);root.getAttribute=k=>root.attrs.get(k)??null;root.setAttribute=(k,v)=>root.attrs.set(k,v);root.removeAttribute=k=>root.attrs.delete(k);root.isConnected=true;
await root.connectedCallback();assert.equal(mounts,1);
root.isConnected=false;root.disconnectedCallback();assert.equal(disposals,1);
root.isConnected=true;await root.connectedCallback();assert.equal(mounts,2);
console.log('viewer policy and disconnect/remount: pass');
root.isConnected=false;root.disconnectedCallback();
location.search='?render_backend=bogus';root.isConnected=true;
await assert.rejects(root.connectedCallback(),/invalid render_backend/);assert.equal(mounts,2);
assert.match(root.attrs.get('data-mount-error'),/invalid render_backend/);
location.search='';root.attrs.set('data-render-backend','webgl2');
let release;globalThis.fetch=()=>new Promise(resolve=>release=()=>resolve({ok:true,text:async()=>'{"dataset":true}'}));
root.attrs.set('data-dataset-src','slow.json');
const connecting=root.connectedCallback();root.isConnected=false;root.disconnectedCallback();release();await connecting;assert.equal(mounts,2);
console.log('invalid policy and stale asynchronous fetch: pass');

// The viewer's one artifact: every policy, with or without a WebGPU device,
// loads the WebGL2 artifact; only an explicit `webgpu` names the opt-in
// comparison build. There is no probe and no remount.
const setNavigator=(value)=>Object.defineProperty(globalThis,'navigator',{value,configurable:true,writable:true});
let probed=0;
for(const nav of [{},{gpu:{requestAdapter:async()=>{probed++;return {requestDevice:async()=>({destroy(){}})}}}},{gpu:{requestAdapter:async()=>null}}]){
  setNavigator(nav);
  assert.equal(chooseArtifact('auto'),'webgl2');
  assert.equal(chooseArtifact('webgl2'),'webgl2');
  assert.equal(chooseArtifact('webgpu'),'webgpu');
}
assert.equal(probed,0,'no WebGPU probe');
console.log('artifact choice (one artifact, no probe): pass');

definitions.clear();
const mounted=[];const requested=[];
defineElements(async(choice)=>{requested.push(choice);const artifact=chooseArtifact(choice);return{artifact,module:{mount_tree:()=>mounted.push(artifact),dispose_tree:()=>{}}};});
assert.equal(definitions.get('graphshell-view'),undefined);
const Tree=definitions.get('graphshell-tree');
const element=new Tree();element.dataset={};element.attrs=new Map();element.hasAttribute=k=>element.attrs.has(k);element.getAttribute=k=>element.attrs.get(k)??null;element.setAttribute=(k,v)=>element.attrs.set(k,v);element.removeAttribute=k=>element.attrs.delete(k);element.isConnected=true;
location.search='?render_backend=auto';
await element.connectedCallback();
assert.deepEqual(requested,['auto']);
assert.deepEqual(mounted,['webgl2']);
assert.equal(element.getAttribute('data-viewer-artifact'),'webgl2');
console.log('auto mounts the one WebGL2 artifact: pass');
