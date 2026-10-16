import fs from 'node:fs';
import assert from 'node:assert/strict';
const definitions=new Map();
globalThis.HTMLElement=class {};
globalThis.customElements={get:name=>definitions.get(name),define:(name,type)=>definitions.set(name,type)};
globalThis.location={search:'?render_backend=webgl2'};
const source=fs.readFileSync(process.env.MOUNT_SOURCE ?? 'ports/graphshell/web/mount.js','utf8');
const {defineElements}=await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));
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

// Route artifacts: the chooser honours explicit policy and probes for an
// actual WebGPU device under Auto; a WebGPU artifact's boot refusal remounts
// the element on WebGL2.
const { chooseArtifact } = await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));
const setNavigator=(value)=>Object.defineProperty(globalThis,'navigator',{value,configurable:true,writable:true});
setNavigator({});
assert.equal(await chooseArtifact('webgl2'),'webgl2');
assert.equal(await chooseArtifact('webgpu'),'webgpu');
assert.equal(await chooseArtifact('auto'),'webgl2');
let destroyed=0;
setNavigator({gpu:{requestAdapter:async()=>({requestDevice:async()=>({destroy:()=>destroyed++})})}});
assert.equal(await chooseArtifact('auto'),'webgpu');assert.equal(destroyed,1);
setNavigator({gpu:{requestAdapter:async()=>({requestDevice:async()=>{throw new Error('no device')}})}});
assert.equal(await chooseArtifact('auto'),'webgl2');
setNavigator({gpu:{requestAdapter:async()=>null}});
assert.equal(await chooseArtifact('auto'),'webgl2');
console.log('artifact choice: pass');

definitions.clear();
const observers=[];
globalThis.MutationObserver=class{constructor(callback){this.callback=callback;observers.push(this)}observe(target){this.target=target;this.live=true}disconnect(){this.live=false}};
const mounted=[];const disposed=[];
const fakeModule=(name)=>({mount_tree:(root)=>{mounted.push(name);if(name==='webgpu'){root.setAttribute('data-mount-error','WebGPU: no adapter; WebGL2: not compiled into this host');for(const o of observers)if(o.live&&o.target===root)o.callback();}},dispose_tree:()=>disposed.push(name)});
const requested=[];
defineElements(async(choice)=>{requested.push(choice);const artifact=choice==='webgl2'?'webgl2':'webgpu';return{artifact,module:fakeModule(artifact)};});
assert.equal(definitions.get('graphshell-view'),undefined);
const Tree=definitions.get('graphshell-tree');
const element=new Tree();element.dataset={};element.attrs=new Map();element.hasAttribute=k=>element.attrs.has(k);element.getAttribute=k=>element.attrs.get(k)??null;element.setAttribute=(k,v)=>element.attrs.set(k,v);element.removeAttribute=k=>element.attrs.delete(k);element.isConnected=true;
location.search='?render_backend=auto';
await element.connectedCallback();
for(let i=0;i<5;i++)await new Promise(r=>setTimeout(r,0));
assert.deepEqual(requested,['auto','webgl2']);
assert.deepEqual(mounted,['webgpu','webgl2']);
assert.deepEqual(disposed,['webgpu']);
assert.equal(element.getAttribute('data-viewer-artifact'),'webgl2');
assert.equal(element.getAttribute('data-mount-error'),null);
console.log('auto WebGPU boot refusal falls back to WebGL2: pass');

