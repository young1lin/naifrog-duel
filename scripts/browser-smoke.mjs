// Drives a separately launched, debuggable Chromium (CDP) through the real WASM page:
// mode menu, local 2P multi-touch, keyboard, pause, CPU mode with single-player touch.
// Usage: node scripts/browser-smoke.mjs <cdp-port> <url> [width height dpr|native]
// High-DPI: launch Chromium with --force-device-scale-factor=2 and pass `native`. CDP's emulated
// deviceScaleFactor reports devicePixelRatio 2 but CSS-sized ResizeObserver device pixels, which
// halves the WASM window; the canvas check below rejects that mismatch.
import fs from 'node:fs/promises';
import zlib from 'node:zlib';
import assert from 'node:assert/strict';
const port=Number(process.argv[2]??9223),url=process.argv[3]??'http://127.0.0.1:18080';
const width=Number(process.argv[4]??1280),height=Number(process.argv[5]??720),native=process.argv[6]==='native',dpr=native?0:Number(process.argv[6]??1);let suffix='';
const landing=await(await fetch(url)).text();assert(landing.includes('id="game"'),'This URL is not the game server');
const targets=await(await fetch('http://127.0.0.1:'+port+'/json/list')).json();const target=targets.find(t=>t.type==='page');assert(target,'No browser page target');
const ws=new WebSocket(target.webSocketDebuggerUrl);await new Promise((r,j)=>{ws.onopen=r;ws.onerror=j;});
let id=0,readinessArmed=false,readyResolve,readinessTimer;const pending=new Map(),errors=[];const ready=new Promise(r=>readyResolve=r);
ws.onclose=()=>{for(const p of pending.values())p.reject(new Error('Browser connection closed'));pending.clear();};
ws.onmessage=e=>{const m=JSON.parse(e.data);if(m.id){const p=pending.get(m.id);if(p){pending.delete(m.id);m.error?p.reject(new Error(JSON.stringify(m.error))):p.resolve(m.result);}}
 else if(m.method==='Runtime.consoleAPICalled'){const text=m.params.args.map(a=>a.value??a.description??'').join(' ');if(readinessArmed&&text.includes('All 17 game textures ready'))readyResolve();if(m.params.type==='error')errors.push(text);}
 else if(m.method==='Runtime.exceptionThrown')errors.push(JSON.stringify(m.params.exceptionDetails));};
const send=(method,params={})=>new Promise((resolve,reject)=>{const n=++id;pending.set(n,{resolve,reject});ws.send(JSON.stringify({id:n,method,params}));});
const wait=ms=>new Promise(r=>setTimeout(r,ms)); // Controlled input durations.
const evaluate=async expression=>(await send('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true})).result.value;
async function until(what,check,ms=8000){const end=Date.now()+ms;for(;;){const v=await check();if(v)return v;if(Date.now()>end)throw new Error('Timed out waiting for '+what);await wait(100);}}
const mode=()=>evaluate('JSON.stringify(window.__naifrogMode)').then(JSON.parse);
const inMode=(name,paused=false)=>until('mode '+name+(paused?' (paused)':''),async()=>{const m=await mode();return m.mode===name&&m.paused===paused;});
async function shot(name){const r=await send('Page.captureScreenshot',{format:'png'});const data=Buffer.from(r.data,'base64');await fs.mkdir('artifacts',{recursive:true});await fs.writeFile('artifacts/browser-'+name+suffix+'.png',data);return decode(data);}
function decode(b){assert.equal(b.subarray(1,4).toString(),'PNG');let at=8,w,h,type;const idat=[];while(at<b.length){const n=b.readUInt32BE(at),tag=b.subarray(at+4,at+8).toString(),d=b.subarray(at+8,at+8+n);if(tag==='IHDR'){w=d.readUInt32BE(0);h=d.readUInt32BE(4);assert.equal(d[8],8);type=d[9];assert.equal(d[12],0);}if(tag==='IDAT')idat.push(d);at+=n+12;}const channels=type===6?4:type===2?3:0;assert(channels,'Unexpected screenshot color type');const raw=zlib.inflateSync(Buffer.concat(idat)),out=Buffer.alloc(w*h*channels),stride=w*channels;let p=0;for(let y=0;y<h;y++){const filter=raw[p++];for(let x=0;x<stride;x++){const a=x>=channels?out[y*stride+x-channels]:0,c=y&&x>=channels?out[(y-1)*stride+x-channels]:0,d=y?out[(y-1)*stride+x]:0;let v=0;if(filter===1)v=a;if(filter===2)v=d;if(filter===3)v=Math.floor((a+d)/2);if(filter===4){const q=a+d-c,pa=Math.abs(q-a),pb=Math.abs(q-d),pc=Math.abs(q-c);v=pa<=pb&&pa<=pc?a:pb<=pc?d:c;}out[y*stride+x]=(raw[p++]+v)&255;}}return{w,h,channels,out};}
// Health bar fill pixels: P1 gold on the left, P2 blue on the right.
function bar(p,from,to,match){const y=Math.round(p.h*.075);let n=0;for(let x=Math.ceil(p.w*from);x<Math.floor(p.w*to);x++){const at=(y*p.w+x)*p.channels;if(match(...p.out.subarray(at,at+3)))n++;}return n;}
const blue=p=>bar(p,.56,.97,(r,g,b)=>r<130&&g>170&&b>200),gold=p=>bar(p,.03,.44,(r,g,b)=>r>200&&g>150&&g<215&&b<110);
const key=(code,key,virtualKey,type='keyDown')=>send('Input.dispatchKeyEvent',{type,code,key,windowsVirtualKeyCode:virtualKey,nativeVirtualKeyCode:virtualKey});
const tap=async(code,k,vk)=>{await key(code,k,vk);await key(code,k,vk,'keyUp');};
const touch=(type,touchPoints)=>send('Input.dispatchTouchEvent',{type,touchPoints});
// Scrolls the element into view first, as a finger would on a short screen where the menu scrolls.
async function tapElement(selector){const r=JSON.parse(await evaluate(`(e=>(e.scrollIntoView({block:'center'}),JSON.stringify(e.getBoundingClientRect())))(document.querySelector(${JSON.stringify(selector)}))`));const point={id:90,x:r.x+r.width/2,y:r.y+r.height/2};await touch('touchStart',[point]);await touch('touchEnd',[]);}
const visible=selector=>evaluate(`!document.querySelector(${JSON.stringify(selector)}).hidden`);
// On-screen controls sit where Rust last drew them (window.__naifrogTouch, canvas pixels): one
// floating stick per player that a finger grabs and drags, and round buttons to tap.
const controls=sticks=>until(sticks+' touch stick(s) drawn',async()=>{const t=JSON.parse(await evaluate('JSON.stringify(window.__naifrogTouch)'));return t.visible&&t.sticks.length===sticks&&t;});
let origin={x:0,y:0};
const button=(t,slot,name)=>{const b=t.buttons.find(b=>b.slot===slot&&b.name===name);assert(b,'No '+name+' button for slot '+slot);return{x:origin.x+b.x,y:origin.y+b.y};};
const stick=(t,slot,dx=0,dy=0)=>{const s=t.sticks.find(s=>s.slot===slot);assert(s,'No stick for slot '+slot);return{x:origin.x+s.x+dx*s.r,y:origin.y+s.y+dy*s.r};};
try{
 await send('Runtime.enable');await send('Runtime.discardConsoleEntries');await send('Page.enable');
 await send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:dpr,mobile:!native&&dpr>1});await send('Emulation.setTouchEmulationEnabled',{enabled:true,maxTouchPoints:10});
 readinessArmed=true;await send('Page.navigate',{url});
 await Promise.race([ready,new Promise((_,reject)=>{readinessTimer=setTimeout(()=>reject(new Error('Game texture readiness timeout; '+errors.join(';'))),120000);})]);clearTimeout(readinessTimer);
 // The canvas backing store must be CSS size x devicePixelRatio, or touch hit-tests and text scale are off.
 const canvasSize=()=>evaluate("(()=>{const c=document.getElementById('game');return JSON.stringify({w:c.width,h:c.height,cw:c.clientWidth,ch:c.clientHeight,dpr:devicePixelRatio})})()").then(JSON.parse);
 const fits=c=>Math.abs(c.w-c.cw*c.dpr)<=2&&Math.abs(c.h-c.ch*c.dpr)<=2;
 const canvas=await until('canvas backing store = CSS size x devicePixelRatio',async()=>{const c=await canvasSize();return fits(c)&&c;},5000).catch(async()=>{throw new Error('Canvas '+JSON.stringify(await canvasSize())+' does not match CSS x DPR (emulated DPR? use --force-device-scale-factor and `native`)');});
 suffix=canvas.dpr===1?'':'-dpr'+canvas.dpr;
 // Input durations below assume real time; a CPU rasterizer at high DPI can drop to ~1 fps.
 const gpu=JSON.parse(await evaluate("new Promise(r=>{const g=document.createElement('canvas').getContext('webgl2'),i=g?.getExtension('WEBGL_debug_renderer_info');let n=0;const t0=performance.now(),f=()=>{n++;if(performance.now()-t0<2000)requestAnimationFrame(f);else r(JSON.stringify({fps:Math.round(n/2),renderer:i?g.getParameter(i.UNMASKED_RENDERER_WEBGL):'unknown'}));};requestAnimationFrame(f);})"));
 assert(gpu.fps>=30,'Page runs at '+gpu.fps+' fps on '+gpu.renderer+'; use a GPU-backed browser (e.g. --use-angle=d3d11) or a smaller viewport');
 // 1. A fresh page starts at the mode menu with nothing simulated.
 await until('menu',()=>visible('#menu'));await inMode('menu',true);
 const offline=await evaluate("document.querySelector('[data-mode=online]').disabled");
 origin=JSON.parse(await evaluate("JSON.stringify(document.getElementById('game').getBoundingClientRect())"));
 assert.equal(await evaluate('window.__naifrogTouch.visible'),false,'No touch controls behind the menu');
 const menuShot=await shot('menu');assert(blue(menuShot)>menuShot.w*.32&&gold(menuShot)>menuShot.w*.32,'Both health bars are drawn behind the menu');
 // 2. Local 2P chosen by touch on the DOM menu.
 await tapElement('[data-mode=local]');await inMode('local');assert(!(await visible('#menu')),'Menu closes after choosing');
 await wait(250);const start=await shot('local-ready'),full=blue(start);assert(full>start.w*.32,'Initial HP bar missing');
 await wait(4000); // A match opens on the versus screen + round introduction: 210 dead ticks (3.5 s).
 // Each half has its own stick and buttons. P1 drags right while P2 drags left, then P2 pulls
 // down to guard while P1 taps LIGHT: front guard chips a quarter of the light (2 of 100 HP).
 const two=await controls(2);assert.equal(two.buttons.length,8,'Four buttons per player');assert(!two.buttons.some(b=>b.name==='JUMP'),'No jump button: the stick jumps');for(const slot of[0,1])button(two,slot,'SHOT');
 const points=[{id:1,...stick(two,0)},{id:2,...stick(two,1)}];
 await touch('touchStart',points);points[0]={id:1,...stick(two,0,.8)};points[1]={id:2,...stick(two,1,-.8)};
 await touch('touchMove',points);await wait(1100);
 points[1]={id:2,...stick(two,1,0,.8)};await touch('touchMove',points);
 points.push({id:3,...button(two,0,'LIGHT')});
 await touch('touchStart',points);await wait(220);const guarded=await shot('local-multitouch');{const chip=full-blue(guarded);assert(chip>full*.01&&chip<full*.05,'Front guard should chip about 2% (got '+chip+' of '+full+' px)');}
 await touch('touchEnd',[]);await wait(350);
 await evaluate("document.querySelector('#game').focus()");await tap('F1','F1',112);
 await key('KeyF','f',70);await wait(300);const hit=await shot('local-hit');assert(blue(hit)<full-20,'Keyboard light failed after touch movement');
 await wait(400);const held=await shot('local-held');assert(Math.abs(blue(held)-blue(hit))<4,'Holding attack must not repeat');
 await tap('KeyR','r',82);await wait(200);const restart=await shot('local-rematch');assert(Math.abs(blue(restart)-full)<4,'Rematch must reset health');await key('KeyF','f',70,'keyUp');
 // Milk bubbles from spawn distance: P1 on the keyboard (T), then P2 on its touch SHOT button. Each idle target loses 10 HP.
 await wait(4000); // Rematch replays the versus screen + round introduction (3.5 s); the T press must land on a live tick.
 await tap('KeyT','t',84);await wait(1500);const bubbled=await shot('local-shot-key');{const d=blue(restart)-blue(bubbled);assert(d>full*.06&&d<full*.14,'Keyboard bubble should take about 10% from P2 (got '+d+' px)');}
 const gold1=gold(bubbled);await touch('touchStart',[{id:4,...button(two,1,'SHOT')}]);await wait(80);await touch('touchEnd',[]);await wait(1500);
 const bubbled2=await shot('local-shot-touch');{const d=gold1-gold(bubbled2);assert(d>gold1*.06&&d<gold1*.14,'Touch bubble should take about 10% from P1 (got '+d+' px)');}
 // 3. Esc pauses behind the menu; nothing moves while paused.
 await tap('Escape','Escape',27);await inMode('local',true);assert(await visible('#menu'));
 await tap('Escape','Escape',27);await inMode('local');assert(!(await visible('#menu')),'Esc resumes');
 // 4. Digit 1 on the menu starts a CPU match. Idle P1 gets attacked by the CPU only.
 await tap('Escape','Escape',27);await inMode('local',true);await tap('Digit1','1',49);await inMode('cpu');
 const cpuStart=await shot('cpu-ready');const gold0=gold(cpuStart),blue0=blue(cpuStart);assert(gold0>cpuStart.w*.32&&blue0>cpuStart.w*.32,'CPU match starts at full health');
 let cpuHit;await until('CPU attacking idle P1',async()=>{cpuHit=await shot('cpu-attacks');return gold(cpuHit)<gold0-10;},15000);
 assert(Math.abs(blue(cpuHit)-blue0)<4,'Idle P1 must not damage the CPU');
 // Paused: health stays put for two seconds.
 await tap('Escape','Escape',27);await inMode('cpu',true);const p1=await shot('cpu-paused');await wait(2000);const p2=await shot('cpu-paused-later');
 assert(Math.abs(gold(p1)-gold(p2))<4&&Math.abs(blue(p1)-blue(p2))<4,'Paused CPU match must not advance');
 await tapElement('#menu-resume');await inMode('cpu');await tap('KeyR','r',82);await wait(200);
 // Single-player touch: one stick on the left, buttons on the right, all P1's (you), never the CPU seat's.
 const one=await controls(1);assert.equal(one.buttons.length,4,'Four buttons for one player');assert(one.buttons.every(b=>b.slot===0)&&one.sticks[0].slot===0);
 const before=blue(await shot('cpu-touch-before'));let landed;
 // Walk in on the stick, then jab a few times: a single tap rarely meets a CPU that is moving,
 // jumping or guarding, and screenshots at DPR 2 are slow enough for it to win meanwhile.
 await until('P1 landing a touch light on the CPU',async()=>{
  await touch('touchStart',[{id:6,...stick(one,0)}]);await touch('touchMove',[{id:6,...stick(one,0,.8)}]);await wait(400);await touch('touchEnd',[]);
  for(let i=0;i<3;i++){await touch('touchStart',[{id:7,...button(one,0,'LIGHT')}]);await wait(60);await touch('touchEnd',[]);await wait(200);}
  landed=await shot('cpu-touch-hit');if(blue(landed)<before-10)return true;
  // The CPU may knock P1 out meanwhile (it shoots and dives too): start a fresh round and retry.
  if(gold(landed)===0){await tap('KeyR','r',82);await wait(300);}return false;},30000);
 // A hidden tab (simulated visibilitychange) pauses the local match behind the menu.
 await evaluate("Object.defineProperty(document,'hidden',{configurable:true,get:()=>true});document.dispatchEvent(new Event('visibilitychange'))");
 await inMode('cpu',true);assert(await visible('#menu'),'Hidden tab must open the menu');
 await evaluate('delete document.hidden');await tapElement('#menu-resume');await inMode('cpu');
 // 5. A pressed page button looks pressed: :active lights it up (fingers get no hover).
 {
  await send('DOM.enable');await send('CSS.enable');const {root}=await send('DOM.getDocument');
  const {nodeId}=await send('DOM.querySelector',{nodeId:root.nodeId,selector:'[data-mode=cpu]'});
  const idle=await evaluate("getComputedStyle(document.querySelector('[data-mode=cpu]')).backgroundColor");
  await send('CSS.forcePseudoState',{nodeId,forcedPseudoClasses:['active'],force:true});
  const active=await evaluate("getComputedStyle(document.querySelector('[data-mode=cpu]')).backgroundColor");
  await send('CSS.forcePseudoState',{nodeId,forcedPseudoClasses:['active'],force:false});
  assert.notEqual(active,idle,'A pressed DOM button must look pressed ('+idle+' -> '+active+')');
 }
 // 6. Safe-area insets: the page feeds --safe-* custom properties (env() on real devices) to the
 //    game; controls and panels shift off the notch band, and no control ever reaches the HUD.
 {
  const plain=JSON.parse(await evaluate('JSON.stringify(window.__naifrogTouch)'));
  const size=await canvasSize();
  // __naifrogTouch is in CSS pixels (what touch dispatch uses); compare against CSS size.
  for(const c of [...plain.buttons,...plain.sticks])
   assert((c.y-c.r)/size.ch>0.5,'Controls live below the HUD band: '+JSON.stringify(c));
  const navBefore=parseFloat(await evaluate("getComputedStyle(document.querySelector('nav')).right"));
  await evaluate("for(const[k,v]of[['--safe-l','66px'],['--safe-r','59px'],['--safe-b','44px']])document.documentElement.style.setProperty(k,v)");
  const safe=await until('controls shift off the safe area',async()=>{const c=JSON.parse(await evaluate('JSON.stringify(window.__naifrogTouch)'));return c.visible&&Math.abs(c.sticks[0].x-plain.sticks[0].x)>20&&c;});
  assert(safe.buttons.every((b,i)=>b.x<plain.buttons[i].x-20),'Buttons move left of the right inset');
  assert(safe.sticks[0].y<plain.sticks[0].y-10&&safe.sticks[0].x>plain.sticks[0].x+20,'The stick moves in off the left and bottom insets');
  const navAfter=parseFloat(await evaluate("getComputedStyle(document.querySelector('nav')).right"));
  assert(navAfter>navBefore+10,'Page panels respect the insets too ('+navBefore+' -> '+navAfter+')');
  await shot('safe-area');
  await evaluate("for(const k of['--safe-l','--safe-r','--safe-b'])document.documentElement.style.removeProperty(k)");
  await until('controls return without insets',async()=>{const c=JSON.parse(await evaluate('JSON.stringify(window.__naifrogTouch)'));return c.visible&&Math.abs(c.sticks[0].x-plain.sticks[0].x)<2&&c;});
 }
 // 7. Rotation: a portrait window keeps the canvas matched and the controls on screen, off the HUD.
 {
  await send('Emulation.setDeviceMetricsOverride',{width:820,height:1180,deviceScaleFactor:dpr,mobile:!native&&dpr>1});
  const p=await until('portrait relayout',async()=>{const c=await canvasSize();return c.ch>c.cw&&fits(c)&&c;},10000);
  const c=JSON.parse(await evaluate('JSON.stringify(window.__naifrogTouch)'));
  assert(c.visible&&c.buttons.length===4,'Touch controls survive rotation');
  for(const k of [...c.buttons,...c.sticks])
   assert((k.y-k.r)/p.ch>0.5&&k.x-k.r>=0&&k.x+k.r<=p.cw+2,'Controls stay on screen and off the HUD in portrait: '+JSON.stringify(k));
  await shot('portrait');
  await send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:dpr,mobile:!native&&dpr>1});
  await until('landscape returns',async()=>{const c=await canvasSize();return c.cw>c.ch&&fits(c)&&c;},10000);
 }
 // The loading screen counted the WASM download, so its bar moved (it once stayed at 0%).
 const wasmLoad=JSON.parse(await evaluate('JSON.stringify(window.__naifrogLoad.wasm)'));
 assert(wasmLoad&&wasmLoad.got>0&&(!wasmLoad.total||wasmLoad.got===wasmLoad.total),'WASM download progress counts every byte: '+JSON.stringify(wasmLoad));
 const state=JSON.parse(await evaluate('JSON.stringify({started:window.__gameStarted,error:window.__gameError,width:innerWidth,height:innerHeight,mode:window.__naifrogMode,dpr:devicePixelRatio})'));
 assert.equal(errors.length,0,errors.join('\n'));
 console.log(JSON.stringify({pass:true,viewport:state,wasmLoad,canvas:[canvas.w,canvas.h],gpu,onlineButtonDisabled:offline,pressedButtonFeedback:true,safeArea:{controlsShift:true,panelsShift:true},rotationPortrait:true,local:{initialBluePixels:full,guardBluePixels:blue(guarded),hitBluePixels:blue(hit),heldBluePixels:blue(held),rematchBluePixels:blue(restart),keyBubbleBluePixels:blue(bubbled),touchBubbleGoldPixels:[gold1,gold(bubbled2)]},cpu:{gold0,blue0,goldAfterCpu:gold(cpuHit),blueAfterTouchLight:blue(landed)}},null,2));
}catch(e){console.error(e);console.error(errors);process.exitCode=1;}finally{clearTimeout(readinessTimer);ws.close();}
