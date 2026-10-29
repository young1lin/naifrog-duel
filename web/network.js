const $=id=>document.getElementById(id);
const SESSION='naifrog-room-v1';
const NAME_STORE='naifrog-name-v1';
// Must equal net::PROTOCOL_VERSION; the server refuses other versions with 'client outdated'.
const PROTOCOL=9;
// Rust reads this view every frame. `active` is true only while 私人联机 is the chosen mode.
const view={active:false,player:null,room:null,transport:0,status:'',snapshot:null};
let socket=null,generation=0,inputSeq=0,lastSnapshot=null,lastServerSeq=-1,intentional=false,reconnectTimer,retries=0,session=null,awaitingRelogin=false;
let multiplayer=false,authenticated=false;
// Every text here comes as a [中文, English] pair, shown in the page's language: main.js sets
// <html lang> and calls setLanguage when the visitor switches.
const T=([zh,en])=>document.documentElement.lang==='en'?en:zh;
// The status pair on screen, so a language switch can reword it; null until the first status.
let said=null;
const status=(zh,en=zh)=>{said=[zh,en];const text=T(said);view.status=text;$('net-status-text').textContent=text;$('room-status').textContent=text;};
// Server reasons are English protocol strings; the page words them for players.
const REASONS={'room unavailable':['房间不存在、已满或已过期。','No such room, or it is full or expired.'],'server full':['服务器房间已满，请稍后再试。','The server has no free room; try again later.'],'already in another room':['你已在另一个房间里（可能是另一个标签页）。','You are already in another room (maybe in another tab).'],'client outdated, reload the page':['网页版本已更新，请刷新页面。','The page was updated; reload it.'],'request denied':['请求被拒绝，连接已断开。','Request denied; the connection was closed.'],'connection replaced':['这个席位已在另一个窗口或标签页打开，本页已断开；刷新本页可取回。','This seat was opened in another window or tab, so this page disconnected; reload it to take the seat back.'],'server transfer budget exhausted':['服务器本小时流量额度已用完，请稍后再试。',"The server used up this hour's traffic; try again later."],'room closed':['房间已关闭（对方离开或超时）。','The room closed (the other player left or it timed out).']};
const reason=(text,fallback)=>REASONS[text]??fallback;
// The display name typed in the room panel, kept in localStorage (never the access key).
// The game checks it with net::valid_name, the server's own rule, which main.js exposes once the
// game loads: letters, digits, space, _ and -, and the 8105 common Han characters, at most 10
// letters or 6 Han wide. Anything else sends no name and the server uses P1/P2. Until the game
// loads only the ASCII part applies; the room panel opens after it.
const NAME_PATTERN=/^[A-Za-z0-9 _-]{1,10}$/;
const playerName=()=>{const v=$('player-name').value.trim(),check=window.__naifrogValidName;return(check?check(v):NAME_PATTERN.test(v)?v:null)??null;};
const NAME_RULE=['可用字母、数字、空格、_、- 和常用汉字；最多 10 个字母或 6 个汉字。','Letters, digits, space, _, - and common Chinese characters; at most 10 letters or 6 Chinese characters.'];
// Say so while typing when the name cannot be shown, instead of silently playing as P1/P2.
const nameHint=()=>{const bad=$('player-name').value.trim()!==''&&!!window.__naifrogValidName&&playerName()===null,h=$('name-hint');h.textContent=(bad?T(['这个名字显示不了，开战时会用 P1 / P2。','This name cannot be shown; the fight will use P1 / P2. ']):'')+T(NAME_RULE);h.className=bad?'warn':'muted';};
const rememberName=()=>{try{localStorage.setItem(NAME_STORE,$('player-name').value.trim());}catch{}};
const save=()=>{try{session?sessionStorage.setItem(SESSION,JSON.stringify(session)):sessionStorage.removeItem(SESSION);}catch{}};
const load=()=>{try{const s=JSON.parse(sessionStorage.getItem(SESSION));if(s&&/^[A-Z2-9]{8}$/.test(s.code)&&/^[a-f0-9]{64}$/.test(s.resume))return s;}catch{}return null;};
const send=message=>{if(socket?.readyState===WebSocket.OPEN)socket.send(JSON.stringify(message));};
// Masks match net::INPUT_MASK (all eight bits) and controls::TRIG (jump, light, heavy, dash, shot).
function input(held,pressed){if(!view.active||view.player===null||!lastSnapshot||socket?.readyState!==WebSocket.OPEN)return;send({type:'input',seq:++inputSeq,round:lastSnapshot.round,held:held&255,pressed:pressed&220});}
// Defined before wasm init. In online mode Rust shows only server snapshots, even while disconnected.
window.__naifrogNetPoll=()=>{const s=JSON.stringify(view);view.snapshot=null;return s;};
window.__naifrogNetSend=(held,pressed)=>input(held,pressed);
window.__naifrogNetRestart=()=>{if(view.active&&view.player!==null&&lastSnapshot)send({type:'restart',round:lastSnapshot.round});};
window.addEventListener('blur',()=>input(0,0));
document.addEventListener('visibilitychange',()=>{if(document.hidden)input(0,0);});
const roomButtons=inRoom=>{$('network-leave').disabled=!inRoom;$('network-create').disabled=inRoom;$('join-button').disabled=inRoom;};
// Copy on every device. The Clipboard API exists only on secure pages (https://, localhost), so on
// http://IP:port and when it is refused, copy a selected off-screen textarea with execCommand (the
// clipboard.js technique; iOS Safari accepts it inside a tap). It runs before any await on http://,
// while the tap still counts as a user gesture. Returns false when nothing was copied.
async function copyText(text){
 if(window.isSecureContext&&navigator.clipboard?.writeText){try{await navigator.clipboard.writeText(text);return true;}catch{}}
 const area=document.createElement('textarea'),focused=document.activeElement;
 area.value=text;area.setAttribute('readonly','');
 // Fixed inside the viewport so focusing scrolls nothing; 16px so iOS does not zoom in.
 area.style.cssText='position:fixed;top:0;left:0;width:1px;height:1px;padding:0;border:0;opacity:0;font-size:16px';
 document.body.append(area);area.focus({preventScroll:true});area.select();area.setSelectionRange(0,text.length);
 let copied=false;try{copied=document.execCommand('copy');}catch{}
 area.remove();focused?.focus?.({preventScroll:true});
 return copied;
}
// Snapshots rewrite the room status 20 times a second, so copy feedback goes on the button itself.
let copyReset;
const COPY=['复制匹配码','Copy code'];
function copyFeedback(label,ms){const b=$('copy-code');b.textContent=T(label);clearTimeout(copyReset);copyReset=setTimeout(()=>{b.textContent=T(COPY);},ms);}
// Pasted text may carry spaces, lowercase or words around the code (chat apps): keep the code.
function typedCode(text){const v=text.toUpperCase();return(v.match(/[A-HJ-NP-Z2-9]{4}-[A-HJ-NP-Z2-9]{4}/)?.[0]??v).replace(/[\s-]/g,'');}
function snapshot(message){
 const {type,...frame}=message;
 if(!Number.isSafeInteger(frame.seq)||frame.seq<=lastServerSeq||!Number.isInteger(frame.round)||!frame.arena||!Array.isArray(frame.connected)||frame.connected.length!==2) return;
 lastServerSeq=frame.seq;lastSnapshot=frame;frame.hits=[...(view.snapshot?.hits??[]),...(frame.hits??[])].slice(-32);view.snapshot=frame;
 const you=view.player;
 const code=session?session.code.slice(0,4)+'-'+session.code.slice(4):'';
 const waiting=!frame.connected.every(Boolean);
 // A dropped seat is held five minutes; say how much is left (mm:ss) while it lasts.
 const held=frame.grace_left?.[1-you];
 const clock=Number.isInteger(held)&&held>=0?Math.floor(held/60)+':'+String(held%60).padStart(2,'0'):null;
 const hold=clock&&['对方已断线（席位保留 '+clock+'），对局暂停','Rival disconnected (seat kept '+clock+'), match paused'];
 const voted=frame.votes?.[you],otherVote=frame.votes?.[1-you];
 const state=hold||(waiting?['等待对方 / 断线暂停','Waiting for the other player / paused']:voted?['已请求重开，等对方确认','Rematch asked, waiting for the other player']:otherVote?['对方请求重开，按 R / Start 确认','The other player asks for a rematch: press R / Start']:['服务器对战中','Fighting on the server']);
 status('房间 '+code+' · 你是 P'+(you+1)+(you===0?'（金色）':'（蓝色）')+' · '+state[0],'Room '+code+' · you are P'+(you+1)+(you===0?' (gold)':' (blue)')+' · '+state[1]);
 // Only on change: rewriting the same text every snapshot would drop a selection made to copy it.
 if($('room-code-display').textContent!==code)$('room-code-display').textContent=code;
 roomButtons(true);
 // Read-only, sanitized inspection hook for local acceptance. No credentials/tokens exposed.
 window.__naifrogState={player:you,code:session?.code,seq:frame.seq,round:frame.round,arena:frame.arena,held:frame.held,connected:frame.connected,votes:frame.votes,names:frame.names,graceLeft:frame.grace_left??null};
}
// Every connect, leave and mode exit bumps `generation`: handlers of older sockets become no-ops,
// and Rust sees a new `transport` and drops held inputs, touch owners and old snapshots.
function drop(){
 clearTimeout(reconnectTimer);generation++;view.transport=generation;
 if(socket){const old=socket;socket=null;old.onopen=old.onmessage=old.onerror=old.onclose=null;try{old.close();}catch{}}
 lastServerSeq=-1;lastSnapshot=null;view.snapshot=null;view.player=null;view.room=null;window.__naifrogState=null;
 $('retry-now').hidden=true;
}
// `silent`: the relogin-wait probe dials without rewriting the prompt's status, so a
// refusal under the wrong address passes unheard and the form stays exactly as it was.
function connect(first,silent){
 drop();const own=generation;intentional=false;
 const url=new URL('/api/ws',location.href);url.protocol=location.protocol==='https:'?'wss:':'ws:';
 const ws=socket=new WebSocket(url);
 if(!silent)status(...(first.type==='create'?['正在创建房间……','Creating a room...']:['正在加入 / 恢复房间……','Joining / resuming the room...']));
 ws.onopen=()=>{if(own===generation)ws.send(JSON.stringify(first));};
 ws.onmessage=event=>{
  if(own!==generation)return;
  if(typeof event.data!=='string'||event.data.length>16384){ws.close(1009);return;}
  let message;try{message=JSON.parse(event.data);}catch{ws.close(1002);return;}
  if(message.type==='welcome'){
   if(![0,1].includes(message.player)||!/^[A-Z2-9]{8}$/.test(message.code)||!/^[a-f0-9]{64}$/.test(message.resume)){ws.close(1002);return;}
   view.player=message.player;inputSeq=0;retries=0;awaitingRelogin=false;
   session={code:message.code,resume:message.resume};view.room=message.code;save();
   // A silent resume while the key form was up: back to the room, prompt gone.
   $('network').hidden=true;$('auth-panel').hidden=true;$('room-panel').hidden=false;$('retry-now').hidden=true;$('game').focus({preventScroll:true});
  }else if(message.type==='snapshot')snapshot(message);
  else if(message.type==='error'){status(...reason(message.message,['无法加入房间。','Could not join the room.']));intentional=true;if(message.message==='room unavailable'){session=null;save();}$('network').hidden=false;$('retry-now').hidden=true;}
  else if(message.type==='closed'){status(...reason(message.reason,['房间已关闭。','The room closed.']));intentional=true;session=null;save();$('network').hidden=false;$('retry-now').hidden=true;}
 };
 ws.onerror=()=>{if(own===generation&&!silent)status('连接失败：请检查登录状态、服务器地址与网络。','Connection failed: check the login, the server address and the network.');};
 ws.onclose=()=>{
  if(own!==generation)return;view.player=null;roomButtons(false);
  if(awaitingRelogin||intentional||!session||!view.active)return;
  scheduleReconnect(own);
 };
 // About 5 minutes of attempts: as long as the server keeps a dropped seat. After a few
 // refusals, check whether the login itself is gone (a network switch changes the address
 // the session is bound to): retrying can never succeed then, so ask for the key instead.
 async function scheduleReconnect(own){
  if(own!==generation)return;
  if(retries>=80){$('network').hidden=false;$('retry-now').hidden=true;status('自动恢复失败，请重新登录 / 加入。断线席位保留 5 分钟。','Could not resume automatically; log in / join again. A dropped seat is kept for 5 minutes.');return;}
  retries++;$('retry-now').hidden=false;
  if(retries>=3){
   try{
    const r=await fetch('/api/config',{cache:'no-store',credentials:'same-origin'});
    if(own!==generation||!view.active)return;
    if(r.ok){
     const config=await r.json();
     if(own!==generation||!view.active)return;
     if(config.multiplayer&&!config.authenticated){showRelogin();return;}
    }
   }catch{} // The probe itself offline: keep retrying, same as before.
   if(own!==generation||!view.active)return;
  }
  status('连接中断，暂停操作；正在尝试恢复……（第 '+retries+' 次）','Connection lost, controls paused; trying to resume... (attempt '+retries+')');
  reconnectTimer=setTimeout(()=>{if(own===generation&&view.active&&session)connect({type:'join',version:PROTOCOL,code:session.code,resume:session.resume,name:playerName()});},Math.min(1000*retries,4000));
 }
}
function leaveRoom(){intentional=true;send({type:'leave'});drop();session=null;save();roomButtons(false);$('room-code-display').textContent='';}
/** Reconnect at once when the tab or link comes back, instead of waiting out the backoff.
 *  `auto` marks the browser's own wake events (pageshow / online / visible): they obey
 *  a stop the server asked for (error / closed set `intentional`), and while the page
 *  waits for the key again they make one silent attempt at most and never touch the
 *  screen. The retry button is a person clicking, so it keeps the full behavior. */
function reconnectNow(auto){
 if(!view.active||!session)return;
 if(auto&&intentional)return; // the server said stop; only a person may try again
 if(awaitingRelogin){
  // If the address came back, one silent dial quietly resumes the seat (the welcome
  // clears the wait); under any other address it fails unheard and the prompt stays.
  if(auto&&(!socket||socket.readyState===WebSocket.CLOSED))connect({type:'join',version:PROTOCOL,code:session.code,resume:session.resume,name:playerName()},true);
  return;
 }
 if(socket&&socket.readyState!==WebSocket.CLOSED)return; // healthy, or already dialing
 clearTimeout(reconnectTimer);retries=0;
 // Any login prompt on screen is moot once a dial succeeds; show the room side again.
 $('auth-panel').hidden=true;$('room-panel').hidden=false;
 connect({type:'join',version:PROTOCOL,code:session.code,resume:session.resume,name:playerName()});
}
window.addEventListener('pageshow',()=>reconnectNow(true));
window.addEventListener('online',()=>reconnectNow(true));
document.addEventListener('visibilitychange',()=>{if(!document.hidden)reconnectNow(true);});
/** The session died (e.g. the network changed address): stop retrying and ask for the key. */
function showRelogin(){
 awaitingRelogin=true;
 clearTimeout(reconnectTimer);$('retry-now').hidden=true;
 $('network').hidden=false;$('room-panel').hidden=true;$('auth-panel').hidden=false;
 status('登录已失效（可能切换了网络），请重新输入口令。','The login expired (maybe the network changed); type the key again.');
 wireLogin(async()=>{
  awaitingRelogin=false;authenticated=true;
  // The old seat is bound to the dead login and cannot be resumed from this one.
  session=null;save();$('auth-panel').hidden=true;$('room-panel').hidden=false;
  status('已重新登录。原座位随旧登录失效，请重新创建或加入房间。','Logged in again. The old seat ended with the old login; create or join a room again.');
 });
}
async function request(path,body){const r=await fetch(path,{method:'POST',credentials:'same-origin',headers:{'Content-Type':'application/json'},body:JSON.stringify(body),cache:'no-store'});if(!r.ok)throw Object.assign(new Error('HTTP '+r.status),{text:r.status===429?['请求过于频繁，请稍候。','Too many requests; wait a moment.']:r.status===401?['口令不正确或登录已过期。','Wrong key, or the login expired.']:['服务器拒绝请求（'+r.status+'）。','The server refused the request ('+r.status+').']});return r.json();}
export const onlineAvailable=()=>multiplayer&&authenticated;
export const onlineActive=()=>view.active;
export const onlineSummary=()=>view.active?(session?T(['房间 ','Room '])+session.code.slice(0,4)+'-'+session.code.slice(4)+(view.player===null?'':T([' · 你是 P',' · you are P'])+(view.player+1)+T(view.player===0?['（金色）',' (gold)']:['（蓝色）',' (blue)'])):T(['尚未进入房间','not in a room yet'])):'';
/** The visitor switched the language: reword the status, the name hint and the copy button. */
export function setLanguage(){if(said)status(...said);if(!$('room-panel').hidden)nameHint();clearTimeout(copyReset);$('copy-code').textContent=T(COPY);}
export const hasSavedRoom=()=>onlineAvailable()&&!!load();
/** Switch to 私人联机: Rust stops local play as soon as it sees `active`. Resumes a saved room after a reload. */
export function enterOnline(){
 if(!onlineAvailable())return false;
 if(!view.active){view.active=true;drop();}
 document.body.classList.add('online');$('network-toggle').hidden=false;$('network').hidden=false;$('auth-panel').hidden=true;$('room-panel').hidden=false;nameHint();
 const saved=load();
 if(saved&&!socket){session=saved;connect({type:'join',version:PROTOCOL,code:saved.code,resume:saved.resume,name:playerName()});}
 else if(!socket){roomButtons(false);status('请创建房间，或输入朋友发来的匹配码。','Create a room, or type the room code a friend sent you.');}
 return true;
}
/** Leave 私人联机 entirely: tell the server, close the socket, forget the room, hand control back to local modes. */
export function leaveOnline(){
 if(!view.active)return;
 leaveRoom();view.active=false;
 document.body.classList.remove('online');$('network-toggle').hidden=true;$('network').hidden=true;status('');
}
/** Wire the key form once; both the loading gate and a dead-login prompt reuse it. */
function wireLogin(onSuccess){
 $('login-form').onsubmit=async e=>{e.preventDefault();const button=$('login-button');button.disabled=true;
  try{await request('/api/auth',{access_key:$('access-key').value.trim()});$('access-key').value='';authenticated=true;await onSuccess();}
  catch(error){status(...(error.text??[error.message,error.message]));}
  finally{button.disabled=false;}};
}
/** Reads server config and, on naifrog-server, waits for login before the protected game files load. */
export async function prepareNetwork(startGame){
 let config;
 try{const r=await fetch('/api/config',{cache:'no-store',credentials:'same-origin'});
  if(r.status===404){await startGame();return;} // Offline static server: local modes only.
  if(!r.ok)throw new Error('HTTP '+r.status);config=await r.json();}
 catch(error){$('loading').textContent=T(['无法读取服务器配置：','Cannot read the server settings: '])+error.message;throw error;}
 multiplayer=!!config.multiplayer;authenticated=!!config.authenticated;
 if(!multiplayer){await startGame();return;}
 $('room-panel').hidden=true;$('auth-panel').hidden=authenticated;
 // A server without a domain may run on plain http://IP:port; say so before the key is typed.
 $('plain-http').hidden=window.isSecureContext;
 // The name box: prefill the remembered value and keep it in localStorage as it changes.
 try{$('player-name').value=localStorage.getItem(NAME_STORE)??'';}catch{}
 $('player-name').addEventListener('input',()=>{rememberName();nameHint();});
 $('network-create').onclick=()=>connect({type:'create',version:PROTOCOL,name:playerName()});
 $('retry-now').onclick=()=>reconnectNow();
 $('join-form').onsubmit=e=>{e.preventDefault();const code=typedCode($('join-code').value);if(!/^[A-HJ-NP-Z2-9]{8}$/.test(code)){status('匹配码为 8 位字母或数字，可带中间横杠。','A room code is 8 letters or digits, with or without the dash in the middle.');return;}session=null;save();connect({type:'join',version:PROTOCOL,code,name:playerName()});};
 $('network-leave').onclick=()=>{leaveRoom();$('network').hidden=false;status('已离开；可以创建或加入另一间房。','Left the room. You can create or join another one.');};
 $('network-toggle').onclick=()=>{$('network').hidden=!$('network').hidden;if($('network').hidden)$('game').focus({preventScroll:true});};
 $('close-network').onclick=()=>{$('network').hidden=true;$('game').focus({preventScroll:true});};
 // When nothing could copy (some browsers; a gamepad press is not a user gesture), select the code
 // on screen so a long press or Ctrl+C copies it.
 $('copy-code').onclick=async()=>{
  if(!session)return;const code=session.code.slice(0,4)+'-'+session.code.slice(4);
  if(await copyText(code)){copyFeedback(['已复制 ✓','Copied ✓'],3000);return;}
  const shown=$('room-code-display');if(shown.textContent!==code)shown.textContent=code;
  getSelection()?.selectAllChildren(shown);copyFeedback(['已选中，长按或 Ctrl+C 复制','Selected: long-press or Ctrl+C to copy'],8000);
 };
 if(!authenticated){
  $('loading').hidden=true;$('network').hidden=false;status('此服务器需要服务口令才能载入游戏。','This server needs the access key before it loads the game.');
  await new Promise(resolve=>wireLogin(()=>{$('auth-panel').hidden=true;$('network').hidden=true;status('');resolve();}));
 }
 await startGame();
}
