import {test} from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import vm from 'node:vm';

const source=(await fs.readFile(new URL('../web/network.js',import.meta.url),'utf8'))
 .replace(/\bexport (?=(?:async )?function|const)/g,'')
 +'\nglobalThis.api={prepareNetwork,enterOnline,leaveOnline};';
const flush=()=>new Promise(resolve=>setImmediate(resolve));
function deferred(){let resolve;const promise=new Promise(r=>resolve=r);return{promise,resolve};}

// Run the actual page module against controllable sockets, requests and timers.
// A vm context keeps every case's module state and browser globals isolated.
async function page(){
 const elements=new Map(),storage=new Map(),sockets=[],timers=new Map();let timerId=0;
 const el=id=>{if(!elements.has(id))elements.set(id,{hidden:false,disabled:false,value:'',textContent:'',focus(){},addEventListener(){}});return elements.get(id);};
 class Socket {
  static OPEN=1;static CLOSED=3;
  constructor(){this.readyState=0;sockets.push(this);}
  send(){}
  close(){this.readyState=3;}
  welcome(){this.readyState=1;this.onopen?.();this.onmessage?.({data:JSON.stringify({type:'welcome',code:'ABCDEFGH',player:0,resume:'a'.repeat(64)})});}
  drop(){this.readyState=3;this.onclose?.();}
 }
 const ctx=vm.createContext({
  document:{getElementById:el,documentElement:{lang:'zh'},body:{classList:{add(){},remove(){}}},addEventListener(){}},
  window:{addEventListener(){}},location:{href:'http://127.0.0.1:18080/',protocol:'http:'},navigator:{},URL,WebSocket:Socket,
  sessionStorage:{getItem:k=>storage.get(k)??null,setItem:(k,v)=>storage.set(k,v),removeItem:k=>storage.delete(k)},
  localStorage:{getItem:()=>null,setItem(){}},
  fetch:async()=>({ok:true,status:200,json:async()=>({multiplayer:true,authenticated:true})}),
  setTimeout:f=>{timers.set(++timerId,f);return timerId;},clearTimeout:id=>timers.delete(id),
 });
 vm.runInContext(source,ctx);
 await ctx.api.prepareNetwork(async()=>{});
 ctx.api.enterOnline();el('network-create').onclick();sockets.at(-1).welcome();
 const refusals=()=>{
  for(let n=0;n<2;n++){sockets.at(-1).drop();const[id,run]=[...timers.entries()].at(-1);timers.delete(id);run();}
  sockets.at(-1).drop();
 };
 return{ctx,el,sockets,refusals};
}

for(const stage of ['fetch','json'])test(`a stale reconnect ${stage} result cannot reopen login after a new welcome`,async()=>{
 const p=await page(),old=deferred();
 p.ctx.fetch=stage==='fetch'?()=>old.promise:async()=>({ok:true,json:()=>old.promise});
 p.refusals();await flush();
 p.el('retry-now').onclick();p.sockets.at(-1).welcome();
 assert.equal(p.el('network').hidden,true);
 const config={multiplayer:true,authenticated:false};
 old.resolve(stage==='fetch'?{ok:true,json:async()=>config}:config);await flush();
 assert.equal(p.sockets.at(-1).readyState,1);
 assert.equal(p.el('network').hidden,true);
 assert.equal(p.el('auth-panel').hidden,true);
});

test('a pending reconnect probe cannot open login after leaving online mode',async()=>{
 const p=await page(),old=deferred();p.ctx.fetch=()=>old.promise;p.refusals();
 p.ctx.api.leaveOnline();
 old.resolve({ok:true,json:async()=>({multiplayer:true,authenticated:false})});await flush();
 assert.equal(p.el('network').hidden,true);
});

test('the current reconnect probe still asks for login when its session expired',async()=>{
 const p=await page();p.ctx.fetch=async()=>({ok:true,json:async()=>({multiplayer:true,authenticated:false})});
 p.refusals();await flush();
 assert.equal(p.el('network').hidden,false);
 assert.equal(p.el('auth-panel').hidden,false);
 assert.equal(p.el('room-panel').hidden,true);
});
