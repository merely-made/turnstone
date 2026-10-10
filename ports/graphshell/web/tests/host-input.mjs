// Scenario-only bridge for the embedding fixture. Browser inputs use CDP Input;
// Runtime evaluation is limited to fixture reads and the request acknowledgement.
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
const [port, origin, output] = process.argv.slice(2);
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const started = Date.now(), records = [];
let page;
while (Date.now() - started < 30000) {
  try { page = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).find(p => p.type === 'page' && p.url.startsWith(origin)); } catch {}
  if (page) break;
  await sleep(100);
}
if (!page) throw new Error('Headed external-input bridge could not find its page');
const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve,reject) => { socket.addEventListener('open',resolve,{once:true});socket.addEventListener('error',reject,{once:true}); });
let sequence = 0;
const pending = new Map();
socket.addEventListener('message',event => {
  const value=JSON.parse(event.data), item=pending.get(value.id);
  if(!item)return;
  pending.delete(value.id);clearTimeout(item.timer);
  value.error?item.reject(new Error(JSON.stringify(value.error))):item.resolve(value.result);
});
function call(method,params={}) {
  return new Promise((resolve,reject) => {
    const id=++sequence,timer=setTimeout(()=>reject(new Error(`${method} timeout`)),5000);
    pending.set(id,{resolve,reject,timer});socket.send(JSON.stringify({id,method,params}));
  });
}
async function read(expression) {
  const value=await call('Runtime.evaluate',{expression,returnByValue:true});
  if(value.exceptionDetails)throw new Error(JSON.stringify(value.exceptionDetails));
  return value.result.value;
}
async function target(selector) {
  if(!['#search-input','#session-local'].includes(selector))throw new Error(`Unsupported host input selector ${selector}`);
  const value=await read(`(() => {const e=document.querySelector(${JSON.stringify(selector)});if(!e||e.closest('graphshell-view,graphshell-tree'))return null;const r=e.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2,width:r.width,height:r.height,disabled:!!e.disabled,hidden:!!e.hidden};})()`);
  if(!value||value.disabled||value.hidden||value.width<=0||value.height<=0)throw new Error(`External host target unavailable: ${selector}`);
  return value;
}
async function perform(request) {
  if(request.kind==='focus'||request.kind==='click') {
    const at=await target(request.target);
    if(request.kind==='focus'&&request.target!=='#search-input')throw new Error('Focus fixture supports the external input only');
    records.push({stage:'input',request,at,method:'CDP Input pointer'});
    await call('Input.dispatchMouseEvent',{type:'mouseMoved',x:at.x,y:at.y});
    await call('Input.dispatchMouseEvent',{type:'mousePressed',x:at.x,y:at.y,button:'left',clickCount:1});
    await call('Input.dispatchMouseEvent',{type:'mouseReleased',x:at.x,y:at.y,button:'left',clickCount:1});
    if(request.kind==='focus') {
      const id=await read('document.activeElement?.id');
      if(id!=='search-input')throw new Error(`External focus did not arrive: ${id}`);
    }
  } else if(request.kind==='key'&&request.target==='ArrowLeft') {
    if(await read('document.activeElement?.id')!=='search-input')throw new Error('External key target lost focus');
    records.push({stage:'input',request,method:'CDP Input keyboard'});
    await call('Input.dispatchKeyEvent',{type:'keyDown',key:'ArrowLeft',code:'ArrowLeft',windowsVirtualKeyCode:37});
    await call('Input.dispatchKeyEvent',{type:'keyUp',key:'ArrowLeft',code:'ArrowLeft',windowsVirtualKeyCode:37});
  } else throw new Error(`Unsupported host input request ${JSON.stringify(request)}`);
}
let last=0,error;
try {
  while(true) {
    const state=await read(`({scenario:document.body.dataset.scenario,request:document.querySelector('graphshell-view,graphshell-tree')?.dataset.hostInputRequest})`);
    if(['ok','fail'].includes(state.scenario))break;
    if(state.request) {
      if(state.scenario!=='running')throw new Error('Host input requested outside explicit running scenario');
      const request=JSON.parse(state.request);
      if(!Number.isSafeInteger(request.id)||request.id<=0)throw new Error('Invalid host input sequence');
      if(request.id!==last) {
        last=request.id;records.push({stage:'request',request,receivedAt:new Date().toISOString()});
        let failure;
        try {await perform(request);}catch(e){failure=String(e);}
        const ack={id:request.id,...(failure?{error:failure}:{ok:true})};
        // Acknowledge fixture transport only. Product state is never set here.
        await call('Runtime.evaluate',{expression:`document.querySelector('graphshell-view,graphshell-tree').setAttribute('data-host-input-result',${JSON.stringify(JSON.stringify(ack))})`,returnByValue:true});
        records.push({stage:'ack',ack});
        await writeFile(join(output,'host-input.json'),JSON.stringify({records},null,2));
        if(failure)throw new Error(failure);
      }
    }
    await sleep(50);
  }
}catch(e){error=String(e);process.exitCode=1;}
await writeFile(join(output,'host-input.json'),JSON.stringify({result:error?'fail':'ok',error,records},null,2));
socket.close();
