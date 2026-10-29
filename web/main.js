const $=id=>document.getElementById(id);
const canvas=$('game'),loading=$('loading'),help=$('help'),menu=$('menu'),room=$('network');
// Language: the page and the game canvas always show the same one. ?lang=zh|en for this load,
// else the visitor's last choice (the 中文 / English button in each panel), else the browser's
// first language: any zh is Chinese, everything else English. index.html carries the Chinese;
// elements with data-en (and data-en-placeholder / data-en-aria-label) carry the English.
// network.js reads <html lang>; Rust polls __naifrogLang every frame.
const LANG_STORE='naifrog-lang-v1';
const pickLang=tag=>/^zh(?:[-_]|$)/i.test(tag??'')?'zh':/^en(?:[-_]|$)/i.test(tag??'')?'en':null;
let lang=pickLang(new URLSearchParams(location.search).get('lang'));
if(!lang){try{lang=pickLang(localStorage.getItem(LANG_STORE));}catch{}}
lang??=pickLang(navigator.languages?.[0]??navigator.language)??'en';
const T=(zh,en)=>lang==='zh'?zh:en;
window.__naifrogLang=()=>lang;
const TITLE=['奶蛙对打 · 单机 / 本地 / 联机','Naifrog Duel · CPU / Local / Online'];
function applyLang(){
 document.documentElement.lang=T('zh-CN','en');document.title=T(...TITLE);
 for(const [attr,key] of [['textContent','en'],['placeholder','enPlaceholder'],['aria-label','enAriaLabel']]){
  for(const el of document.querySelectorAll('[data-'+key.replace(/[A-Z]/g,c=>'-'+c.toLowerCase())+']')){
   const zh=key+'Zh';el.dataset[zh]??=attr==='textContent'?el.textContent:el.getAttribute(attr);
   const text=T(el.dataset[zh],el.dataset[key]);
   if(attr==='textContent')el.textContent=text;else el.setAttribute(attr,text);
  }
 }
 for(const b of document.querySelectorAll('.lang-toggle')){b.textContent=T('English','中文');b.lang=T('en','zh-CN');b.setAttribute('aria-label',T('Switch to English','切换到中文'));}
}
applyLang();
$('loading-text').textContent=T('正在加载奶蛙对打……','Loading Naifrog Duel...');
for(const b of document.querySelectorAll('.lang-toggle'))b.onclick=()=>{
 lang=T('en','zh');try{localStorage.setItem(LANG_STORE,lang);}catch{}
 applyLang();net?.setLanguage();refresh();
};
canvas.addEventListener('pointerdown',()=>canvas.focus({preventScroll:true}));
canvas.addEventListener('contextmenu',e=>e.preventDefault());
window.__gameStarted=false;window.__gameError=null;
// Rust <-> page mode bridge. Requests: 0 pause (menu over a local match), 1 new CPU match,
// 2 new local 2P match, 3 resume; 255 none. Online is entered through network.js instead.
const MODES=['menu','cpu','local','online','demo'];
const NAMES={menu:['未选择模式','no mode yet'],cpu:['单机打人机','Vs CPU'],local:['本地同屏双人','Local 2 players'],online:['私人联机','Private online'],demo:['演示','Demo']};
let request=255,ready=false,net=null;
window.__naifrogMode={mode:'menu',paused:true};
window.__naifrogLocalPoll=()=>{const r=request;request=255;return r;};
window.__naifrogLocalChanged=(code,paused)=>{
 window.__naifrogMode={mode:MODES[code]??'menu',paused:!!paused};
 // A pad's Select pauses / resumes inside Rust: keep the page menu in step.
 if(ready&&local()){if(paused&&menu.hidden&&help.hidden)openMenu();else if(!paused&&!menu.hidden){menu.hidden=true;canvas.focus({preventScroll:true});}}
 refresh();
};
// Touch screens get the see-through stick and round buttons from the start; a desktop shows
// them only after its first touch. __naifrogTouch is where Rust drew them, for browser tests.
window.__naifrogTouchCapable=()=>navigator.maxTouchPoints>0||matchMedia('(any-pointer: coarse)').matches;
window.__naifrogTouch={visible:false,buttons:[],sticks:[]};
window.__naifrogTouchLayout=json=>{window.__naifrogTouch=JSON.parse(json);};
// Safe-area insets (notch, gesture bar) in CSS pixels, read from custom properties that env()
// fills on real devices (viewport-fit=cover) and a test can override. The game polls this.
window.__naifrogSafeArea=()=>{const cs=getComputedStyle(document.documentElement),n=k=>parseFloat(cs.getPropertyValue(k))||0;return n('--safe-l')+','+n('--safe-r')+','+n('--safe-b')+','+innerHeight;};
window.__naifrogGameReady=()=>{ready=true;$('menu-toggle').disabled=false;loading.hidden=true;if(net?.hasSavedRoom())net.enterOnline();else openMenu();};
const current=()=>net?.onlineActive()?'online':window.__naifrogMode.mode;
const local=()=>['cpu','local'].includes(current());
// Overlays pause local matches; online matches keep running on the server.
function syncPause(){if(!ready||!local())return;request=(menu.hidden&&help.hidden)?3:0;}
function refresh(){
 const mode=current(),paused=window.__naifrogMode.paused&&mode!=='online';
 $('menu-current').textContent=T('当前：','Now: ')+T(...NAMES[mode])+(mode==='online'?' · '+net.onlineSummary()+T('（联机不会暂停）',' (online play never pauses)'):mode!=='menu'&&paused?T('（已暂停）',' (paused)'):'');
 $('menu-resume').hidden=mode==='menu';
 for(const b of menu.querySelectorAll('.mode'))b.classList.toggle('current',b.dataset.mode===mode);
 const online=menu.querySelector('[data-mode="online"]');online.disabled=!net?.onlineAvailable();
 $('online-hint').textContent=online.disabled
  ?T('当前页面由离线静态服务器提供（只能单机 / 本地）。私人联机请打开 naifrog-server 的地址（https:// 域名或 http://IP:端口）并登录。','This page comes from the offline static server (CPU / local only). For private online, open the naifrog-server address (an https:// domain or http://IP:port) and log in.')
  :T('创建房间或输入朋友的匹配码。服务器决定你是 P1 还是 P2，加入者通常是蓝色 P2。',"Create a room or type a friend's room code. The server decides whether you are P1 or P2; whoever joins is usually blue P2.");
 $('menu-note').textContent=mode==='online'
  ?T('选择单机或本地会离开当前联机房间。数字键 1 / 2 / 3 直接选择。','Choosing CPU or local leaves the online room. Number keys 1 / 2 / 3 choose directly.')
  :T('数字键 1 / 2 / 3 直接选择。开始新模式会重开对局；Esc 或下方按钮返回当前对局。','Number keys 1 / 2 / 3 choose directly. A new mode starts a new match; Esc or the button below goes back to the current one.');
}
function openMenu(){if(!ready)return;room.hidden=true;help.hidden=true;menu.hidden=false;refresh();syncPause();(menu.querySelector('.mode.current:not(:disabled)')??menu.querySelector('.mode:not(:disabled)'))?.focus({preventScroll:true});}
function closeMenu(){if(current()==='menu')return;menu.hidden=true;syncPause();canvas.focus({preventScroll:true});}
function choose(mode){
 if(!ready)return;
 if(mode==='online'){if(!net?.onlineAvailable())return;menu.hidden=true;net.enterOnline();refresh();return;}
 if(net?.onlineActive())net.leaveOnline();
 request=mode==='cpu'?1:2;menu.hidden=true;help.hidden=true;canvas.focus({preventScroll:true});
}
for(const b of menu.querySelectorAll('.mode'))b.onclick=()=>choose(b.dataset.mode);
$('menu-resume').onclick=closeMenu;
$('menu-toggle').onclick=()=>menu.hidden?openMenu():closeMenu();
$('network-menu').onclick=openMenu;
$('help-toggle').onclick=()=>{help.hidden=!help.hidden;if(!help.hidden)menu.hidden=true;syncPause();if(help.hidden)canvas.focus();};
$('close-help').onclick=()=>{help.hidden=true;syncPause();canvas.focus();};
$('fullscreen').onclick=async()=>{try{if(!document.fullscreenElement)await document.documentElement.requestFullscreen();else await document.exitFullscreen();}catch(e){console.warn('This browser has no page full screen; use landscape.',e);}canvas.focus();};
function back(){
 if(!help.hidden){help.hidden=true;syncPause();canvas.focus();}
 else if(!room.hidden){room.hidden=true;canvas.focus();}
 else if(!menu.hidden)closeMenu();
 else openMenu();
}
window.addEventListener('keydown',e=>{
 if(!ready||e.repeat||e.target?.tagName==='INPUT')return;
 if(!menu.hidden&&/^Digit[123]$|^Numpad[123]$/.test(e.code)){e.preventDefault();choose(['cpu','local','online'][Number(e.code.at(-1))-1]);return;}
 if(e.key!=='Escape')return;
 e.preventDefault();back();
});
// Pads drive the page panels here: a focused panel blurs the canvas, winit then reports the window
// unfocused, and the game ignores pads until the canvas has focus again. D-pad / left stick moves
// focus, A or Start activates, B or Select goes back; in online play Select opens the room panel.
// Buttons act on release and only for presses that began on the same open panel, so no press
// leaks into the game once the canvas takes focus back.
const PAD={A:0,B:1,SELECT:8,START:9,UP:12,DOWN:13,LEFT:14,RIGHT:15};
let padPanel=null,padHeld=new Set(),padArmed=new Set(),padDir=0,padNext=Infinity;
const topPanel=()=>!help.hidden?help:!room.hidden?room:!menu.hidden?menu:null;
const padTargets=panel=>[...panel.querySelectorAll('button')].filter(b=>!b.disabled&&b.offsetParent!==null);
// In a room the safe default is 返回画面 (back to the game), never 离开房间 (leave room); never the
// language button either.
const padHome=panel=>panel===room&&!$('network-leave').disabled?$('close-network'):padTargets(panel).find(b=>!b.classList.contains('lang-toggle'));
function padStep(panel,step){const list=padTargets(panel),at=list.indexOf(document.activeElement);if(at<0)padHome(panel)?.focus({preventScroll:true});else list[(at+step+list.length)%list.length].focus({preventScroll:true});}
function padPoll(){
 requestAnimationFrame(padPoll);
 const held=new Set();
 // Some browsers hide or refuse the gamepad API on plain http:// pages; touch and keys still work.
 let pads=[];try{pads=navigator.getGamepads?.()??[];}catch{}
 for(const p of pads){if(!p?.connected)continue;p.buttons.forEach((b,i)=>{if(b.pressed)held.add(i);});const [x=0,y=0]=p.axes;if(y<-.5)held.add(PAD.UP);if(y>.5)held.add(PAD.DOWN);if(x<-.5)held.add(PAD.LEFT);if(x>.5)held.add(PAD.RIGHT);}
 const pressed=[...held].filter(i=>!padHeld.has(i)),released=[...padHeld].filter(i=>!held.has(i));padHeld=held;
 const dir=(held.has(PAD.DOWN)||held.has(PAD.RIGHT))-(held.has(PAD.UP)||held.has(PAD.LEFT));
 const panel=ready?topPanel():null;
 if(panel!==padPanel){
  // A new panel: buttons and directions already held wait for release; focus moves inside.
  padPanel=panel;padArmed.clear();padDir=dir;padNext=Infinity;
  if(panel&&!panel.contains(document.activeElement))padHome(panel)?.focus({preventScroll:true});
 }else if(panel){
  const now=performance.now();
  if(dir!==padDir){padDir=dir;padNext=now+400;if(dir)padStep(panel,dir);}
  else if(dir&&now>=padNext){padNext=now+140;padStep(panel,dir);}
  for(const i of pressed)if([PAD.A,PAD.B,PAD.SELECT,PAD.START].includes(i))padArmed.add(i);
  for(const i of released){
   if(!padArmed.delete(i))continue;
   if(i===PAD.B||i===PAD.SELECT)back();
   else if(panel.contains(document.activeElement)&&document.activeElement.tagName==='BUTTON')document.activeElement.click();
   else padStep(panel,0);
  }
 }else if(current()==='online'&&pressed.includes(PAD.SELECT))room.hidden=false;
}
requestAnimationFrame(padPoll);
// A local match pauses behind the menu while the tab is hidden.
document.addEventListener('visibilitychange',()=>{if(document.hidden&&local()&&menu.hidden)openMenu();});
// Loading screen. The first visit downloads the WASM (gzip-encoded when the server has a .gz copy;
// x-uncompressed-length then gives its decoded size) and the textures. Later visits load them from
// the browser cache: see gameUrl.
const loadText=$('loading-text'),loadBar=$('loading-bar'),loadDetail=$('loading-detail');
// wasm: bytes received and the expected total (0 when unknown); textures: one entry per texture fetch.
const load={wasm:null,textures:[],failed:null};window.__naifrogLoad=load;
const MB=n=>(n/1048576).toFixed(1)+' MB';
function showLoad(title,fraction,detail){
 if(load.failed)return;
 loading.hidden=false;loadText.textContent=title;loadDetail.textContent=detail;
 loadBar.hidden=fraction==null;if(fraction!=null)loadBar.value=Math.min(1,fraction);
}
function loadFailed(reason,error){
 if(load.failed)return;
 load.failed=reason;window.__gameError=reason+(error?' ('+error+')':'');
 loading.hidden=false;loadText.textContent=T('载入失败','Loading failed');loadBar.hidden=true;loadDetail.textContent=reason;
}
// What a player can do about a refused download.
const httpReason=status=>status===429?T('下载太频繁：同一网络 1 分钟内能载入的次数有限。请等 1 分钟再刷新，不要连续刷新。','Too many downloads: one network may load the game only a few times a minute. Wait a minute before reloading, and do not reload again and again.')
 :status===503?T('服务器正忙，或本小时的流量额度已用完。请过几分钟再刷新。',"The server is busy, or used up this hour's traffic. Reload in a few minutes.")
 :status===401?T('登录已过期。请刷新页面，重新输入服务口令。','The login expired. Reload the page and type the access key again.')
 :T('服务器返回 HTTP '+status+'。请稍后刷新重试。','The server answered HTTP '+status+'. Reload a little later.');
// Pass a body through, reporting each chunk's size and the end; a broken connection fails with `reason`.
function counted(response,progress,reason,headers){
 const reader=response.body.getReader();
 return new Response(new ReadableStream({
  async pull(c){let chunk;try{chunk=await reader.read();}catch(error){loadFailed(reason,error);c.error(error);return;}
   if(chunk.done){progress(0,true);c.close();}else{progress(chunk.value.byteLength,false);c.enqueue(chunk.value);}},
  cancel:r=>reader.cancel(r)}),{status:response.status,statusText:response.statusText,headers:headers??response.headers});
}
async function fetchWasm(url){
 const title=T('正在下载游戏程序（1/2）','Downloading the game (1/2)'),start=performance.now();
 showLoad(title,null,T('正在连接服务器……','Connecting to the server...'));
 let response;
 try{response=await fetch(url);}catch(error){loadFailed(T('连不上服务器。请检查网络后刷新。','Cannot reach the server. Check the network, then reload.'),error);throw error;}
 if(!response.ok){const reason=httpReason(response.status);loadFailed(reason);throw new Error(reason);}
 const gzip=response.headers.get('content-encoding')==='gzip',sent=Number(response.headers.get('content-length'))||0;
 const total=Number(response.headers.get('x-uncompressed-length'))||(gzip?0:sent);
 let got=0;load.wasm={got:0,total};
 return counted(response,n=>{
  got+=n;load.wasm.got=got;const s=Math.round((performance.now()-start)/1000),secs=T(' · 已用 '+s+' 秒',' · '+s+' s'),pct=Math.floor(got*100/total);
  showLoad(title,total?got/total:null,!total?T('已接收 '+MB(got),MB(got)+' received')+secs:gzip?pct+T('% · 压缩传输共 ','% · compressed: ')+MB(sent)+secs:MB(got)+' / '+MB(total)+T('（'+pct+'%）',' ('+pct+'%)')+secs);
 },T('游戏程序下载中断（网络太慢或连接断开）。请刷新重试。','The game download broke off (slow network or lost connection). Reload to try again.'),{'content-type':'application/wasm'});
}
// naifrog-server lists every game file with its content hash (/api/manifest), the way web game
// engines ship content-hashed files: the page asks for /v/<hash>/<file>, which the browser keeps for
// a year without asking again. An update changes the hashes of the changed files only, so only those
// download again. The offline server has no manifest: plain URLs, revalidated on every load.
const pageFetch=window.fetch.bind(window);
const manifest=new Map();
async function loadManifest(){
 try{
  const r=await pageFetch('/api/manifest',{cache:'no-store',credentials:'same-origin'});
  if(!r.ok)return;const files=(await r.json())?.files??{};
  for(const [name,hash] of Object.entries(files))if(/^[0-9a-f]{20}$/.test(hash))manifest.set(name,hash);
 }catch{}
}
// The versioned URL of a same-origin game file listed in the manifest; anything else is unchanged.
function gameUrl(input){
 if(typeof input!=='string'&&!(input instanceof URL))return input;
 const url=new URL(input,location.href),path=url.pathname.slice(1);
 return url.origin===location.origin&&!url.search&&manifest.has(path)?'/v/'+manifest.get(path)+'/'+path:input;
}
// Bevy's web asset reader fetches each texture with window.fetch: versioned, and its bytes counted.
// A failed texture makes the game exit, so the page says so instead of hanging on this screen.
window.fetch=async(input,init)=>{
 const texture=/assets\/processed\/[^?#]*\.webp$/.test(String(input?.url??input));
 input=gameUrl(input);
 if(ready||!texture)return pageFetch(input,init);
 const part={got:0,size:0,done:false};load.textures.push(part);textureProgress();
 let response;
 try{response=await pageFetch(input,init);}catch(error){loadFailed(T('贴图下载失败：连不上服务器。请检查网络后刷新。','A texture failed to download: cannot reach the server. Check the network, then reload.'),error);throw error;}
 if(!response.ok||!response.body){loadFailed(T('贴图下载失败。','A texture failed to download. ')+httpReason(response.status));return response;}
 part.size=Number(response.headers.get('content-length'))||0;
 return counted(response,(n,done)=>{part.got+=n;part.done=done;textureProgress();},T('贴图下载中断（网络太慢或连接断开）。请刷新重试。','A texture download broke off (slow network or lost connection). Reload to try again.'));
};
function textureProgress(){
 const t=load.textures,done=t.filter(p=>p.done).length,got=t.reduce((s,p)=>s+p.got,0);
 if(done===t.length){showLoad(T('正在准备画面……','Preparing the screen...'),1,T('贴图 '+done+' 张已下载',done+' textures downloaded')+' · '+MB(got));return;}
 const fraction=t.reduce((s,p)=>s+(p.done?1:p.size?Math.min(1,p.got/p.size):0),0)/t.length;
 showLoad(T('正在下载贴图（2/2）','Downloading textures (2/2)'),fraction,T('已完成 '+done+' / '+t.length+' 张 · 已接收 ',done+' / '+t.length+' textures done · received ')+MB(got));
}
// Dynamic import so a missing /network.js shows a visible reason instead of a stuck loading screen.
try{net=await import('/network.js');}
catch(error){let code='';try{const r=await fetch('/network.js',{method:'HEAD',cache:'no-store'});code=' HTTP '+r.status+(r.headers.get('x-naifrog-server')?' ('+r.headers.get('x-naifrog-server')+')':'');}catch{}
 loading.textContent=T('无法载入 /network.js'+code+'。可能是旧的服务器进程仍在运行：请停止它，再用 node scripts/serve-web.mjs（离线）或 naifrog-server（联机）重新启动。','Cannot load /network.js'+code+'. An old server process may still be running: stop it, then start node scripts/serve-web.mjs (offline) or naifrog-server (online) again.');window.__gameError=String(error);throw error;}
await net.prepareNetwork(async()=>{
 showLoad(T('正在加载奶蛙对打……','Loading Naifrog Duel...'),null,'');
 try{
  await loadManifest();
  const {default:init,valid_player_name}=await import(gameUrl('/pkg/naifrog_duel.js'));await init({module_or_path:fetchWasm(gameUrl('/pkg/naifrog_duel_bg.wasm'))});
  // The room panel checks names with the game's own rule (net::valid_name), the one the server applies.
  window.__naifrogValidName=valid_player_name;
  window.__gameStarted=true;if(!load.textures.length)showLoad(T('正在启动游戏……','Starting the game...'),null,'');canvas.focus({preventScroll:true});
 }catch(error){
  console.error(error);if(load.failed)return;window.__gameError=String(error);
  loadText.textContent=T('启动失败','Start failed');loadBar.hidden=true;loadDetail.textContent=error+T('\n请确认登录未过期、网页版本已构建，并通过 HTTP/HTTPS 打开。','\nCheck that the login has not expired, that the web build exists, and that the page was opened over HTTP/HTTPS.');
 }
});
