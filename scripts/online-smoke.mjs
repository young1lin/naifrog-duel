// Two real WASM clients against a running naifrog-server, each in a fresh, isolated browser
// context. A PC client (keyboard, 1280x720) creates a private room; a tablet-size touch client
// (1180x820) joins with the match code as blue P2. Then: move, attack, guard, jump, dash, HP
// agreement, input timeout, disconnect pause + resume, stale-socket fencing, paired rematch,
// KO outcome, leaving through the mode menu, and switching to a new room.
//
//   $env:NAIFROG_ACCESS_KEY = <key>   # read from the environment, typed into the login field, never printed
//   node scripts/online-smoke.mjs <pc-cdp-port> <tablet-cdp-port> <url> [pc-ip tablet-ip]
//
// With the optional IPs each browser sends that X-Real-IP; start the server with
// NAIFROG_PROXY_IPS=127.0.0.1 so per-IP quotas and session binding see two different clients.
// Both browsers must be separately launched, debuggable, GPU-backed Chromium instances.
import fs from 'node:fs/promises';
import assert from 'node:assert/strict';
const [pcPort, tabletPort, url, pcIp, tabletIp] = process.argv.slice(2);
const secret = process.env.NAIFROG_ACCESS_KEY ?? '';
// Must equal net::PROTOCOL_VERSION and web/network.js; the raw-WebSocket steps dial with it.
const PROTOCOL = 9;
assert(pcPort && tabletPort && url, 'Usage: node scripts/online-smoke.mjs <pc-cdp-port> <tablet-cdp-port> <url> [pc-ip tablet-ip]');
assert(secret.length >= 32, 'Set NAIFROG_ACCESS_KEY to the server access key');
// Some checks read the page's Chinese texts; without ?lang=zh it follows the browser's language.
const zhPage = Object.assign(new URL(url), { search: '?lang=zh' }).href;
const wait = ms => new Promise(r => setTimeout(r, ms));
async function until(what, check, ms = 10000, every = 50) {
  const end = Date.now() + ms;
  for (;;) { const v = await check(); if (v) return v; if (Date.now() > end) throw new Error('Timed out waiting for ' + what); await wait(every); }
}
async function connect(wsUrl) {
  const ws = new WebSocket(wsUrl); let id = 0; const pending = new Map(), listeners = [];
  ws.onmessage = e => {
    const m = JSON.parse(e.data);
    if (m.id) { const p = pending.get(m.id); if (p) { pending.delete(m.id); m.error ? p.reject(new Error(m.error.message)) : p.resolve(m.result); } }
    else for (const l of listeners) l(m);
  };
  ws.onclose = () => { for (const p of pending.values()) p.reject(new Error('CDP connection closed')); pending.clear(); };
  await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
  return { send: (method, params = {}) => new Promise((resolve, reject) => { const n = ++id; pending.set(n, { resolve, reject }); ws.send(JSON.stringify({ id: n, method, params })); }), on: l => listeners.push(l), close: () => ws.close() };
}
const KEYS = { KeyA: ['a', 65], KeyD: ['d', 68], KeyF: ['f', 70], KeyG: ['g', 71], KeyH: ['h', 72], KeyR: ['r', 82] };
// Touch targets are aimed where the tablet last drew its controls (window.__naifrogTouch, canvas
// pixels): a stick direction is a drag from the stick's rest spot, a button (by name) a tap.
// REMATCH is the fixed canvas button (src/ui.rs `REMATCH`), as fractions of the window.
const STICK = { left: [-0.8, 0], right: [0.8, 0], guard: [0, 0.8], jump: [0, -0.8] };
const REMATCH = [0.5, 0.194]; // centre of ui::REMATCH (x 0.43 + 0.14 / 2, y 0.172 + 0.044 / 2)
async function client(name, port, { width, height, touch, ip }) {
  const version = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
  const browser = await connect(version.webSocketDebuggerUrl);
  // A new context has its own cookies and sessionStorage, so nothing leaks between runs or clients.
  const { browserContextId } = await browser.send('Target.createBrowserContext', { disposeOnDetach: true });
  const { targetId } = await browser.send('Target.createTarget', { url: 'about:blank', browserContextId });
  const page = await connect(`ws://127.0.0.1:${port}/devtools/page/${targetId}`);
  const c = { name, page, browser, contextId: browserContextId, touch, width, height, errors: [], onReady: null };
  page.on(m => {
    if (m.method === 'Runtime.consoleAPICalled') {
      const text = m.params.args.map(a => a.value ?? a.description ?? '').join(' ');
      if (text.includes('All 17 game textures ready')) c.onReady?.();
      if (m.params.type === 'error') c.errors.push(text);
    } else if (m.method === 'Runtime.exceptionThrown') c.errors.push(JSON.stringify(m.params.exceptionDetails));
  });
  await page.send('Runtime.enable'); await page.send('Page.enable'); await page.send('Network.enable');
  if (ip) await page.send('Network.setExtraHTTPHeaders', { headers: { 'X-Real-IP': ip } });
  await page.send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 0, mobile: false });
  await page.send('Emulation.setFocusEmulationEnabled', { enabled: true });
  if (touch) await page.send('Emulation.setTouchEmulationEnabled', { enabled: true, maxTouchPoints: 10 });
  c.eval = async expression => (await page.send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })).result.value;
  c.state = async () => JSON.parse(await c.eval('JSON.stringify(window.__naifrogState??null)'));
  c.mode = async () => JSON.parse(await c.eval('JSON.stringify(window.__naifrogMode)'));
  c.text = selector => c.eval(`document.querySelector(${JSON.stringify(selector)}).textContent`);
  c.visible = selector => c.eval(`!document.querySelector(${JSON.stringify(selector)}).hidden`);
  // Resolves on the readiness log of the navigation that follows, never an older one.
  // A page that fails to start (e.g. a debug-size wasm the server refuses) fails the run instead of hanging it.
  c.load = action => {
    const ready = new Promise((r, j) => { c.onReady = r; setTimeout(() => c.eval('window.__gameError').then(e => j(new Error(`${c.name} did not load the game: ${e}`)), j), 180000).unref(); });
    return action().then(() => ready);
  };
  c.press = async selector => {
    const r = JSON.parse(await c.eval(`JSON.stringify(document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect())`));
    const x = r.x + r.width / 2, y = r.y + r.height / 2;
    if (touch) { await page.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ id: 90, x, y }] }); await page.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] }); }
    else for (const type of ['mouseMoved', 'mousePressed', 'mouseReleased']) await page.send('Input.dispatchMouseEvent', { type, x, y, button: 'left', clickCount: 1 });
  };
  c.type = async (selector, text) => { await c.eval(`document.querySelector(${JSON.stringify(selector)}).focus()`); await page.send('Input.insertText', { text }); };
  c.key = async (code, type = 'keyDown') => { const [key, vk] = KEYS[code]; await page.send('Input.dispatchKeyEvent', { type, code, key, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk }); };
  c.tapKey = async (code, hold = 60) => { await c.key(code); await wait(hold); await c.key(code, 'keyUp'); };
  c.touchKeys = async names => {
    if (!names.length) return page.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    const t = JSON.parse(await c.eval('JSON.stringify(window.__naifrogTouch)'));
    const aim = (name, drag) => {
      if (name === 'rematch') return { x: REMATCH[0] * width, y: REMATCH[1] * height };
      assert(t.visible, c.name + ' shows no touch controls');
      const s = t.sticks[0], d = drag ? STICK[name] ?? [0, 0] : [0, 0];
      if (STICK[name]) return { x: s.x + d[0] * s.r, y: s.y + d[1] * s.r };
      const b = t.buttons.find(b => b.name === name.toUpperCase()); assert(b, 'No touch button ' + name);
      return { x: b.x, y: b.y };
    };
    const points = drag => names.map((n, i) => ({ id: i + 1, ...aim(n, drag) }));
    await page.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: points(false) });
    if (names.some(n => STICK[n])) await page.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: points(true) });
  };
  c.tapTouch = async (name, hold = 80) => { await c.touchKeys([name]); await wait(hold); await c.touchKeys([]); };
  c.focusGame = () => c.eval("document.getElementById('game').focus()");
  c.shot = async label => { const r = await page.send('Page.captureScreenshot', { format: 'png' }); await fs.mkdir('artifacts', { recursive: true }); await fs.writeFile(`artifacts/online-${label}.png`, Buffer.from(r.data, 'base64')); };
  c.close = async () => { try { await browser.send('Target.disposeBrowserContext', { browserContextId }); } catch {} page.close(); browser.close(); };
  return c;
}
const fighter = (s, i) => s.arena.fighters[i];
const stateName = f => typeof f.state === 'string' ? f.state : Object.entries(f.state).flat().join(':');
const units = x => Math.round(x / 1000);
async function both(fn) { return Promise.all([fn(pc), fn(tablet)]); }
const report = {};
let pc, tablet;
try {
  pc = await client('pc', pcPort, { width: 1280, height: 720, touch: false, ip: pcIp });
  tablet = await client('tablet', tabletPort, { width: 1180, height: 820, touch: true, ip: tabletIp });

  // 1. Login gate: nothing protected loads before the shared access key is accepted.
  await both(c => c.page.send('Page.navigate', { url: zhPage }));
  await both(c => until(c.name + ' login form', () => c.eval("!!document.getElementById('access-key')&&!document.getElementById('auth-panel').hidden&&!document.getElementById('network').hidden")));
  const gate = await both(c => c.eval("Promise.all(['/pkg/naifrog_duel.js','/pkg/naifrog_duel_bg.wasm','/assets/processed/game-assets.json'].map(p=>fetch(p,{cache:'no-store'}).then(r=>r.status))).then(s=>JSON.stringify({started:window.__gameStarted,status:s}))").then(JSON.parse));
  for (const g of gate) { assert.equal(g.started, false, 'Game must not start before login'); assert.deepEqual(g.status, [401, 401, 401], 'Protected files need a session'); }
  await pc.type('#access-key', 'wrong-key-' + 'x'.repeat(40)); await pc.press('#login-button');
  await until('wrong key message', async () => (await pc.text('#net-status-text')).includes('口令不正确'));
  await pc.eval("document.getElementById('access-key').value=''");
  // A plain http:// page on a non-local address (a server without a domain) warns before the key is typed.
  const context = await both(c => c.eval("JSON.stringify({secure:isSecureContext,warned:!document.getElementById('plain-http').hidden})").then(JSON.parse));
  for (const x of context) assert.equal(x.warned, !x.secure, 'Plain-HTTP warning shown exactly on insecure pages');
  report.loginGate = { protectedBeforeLogin: gate[0].status, wrongKeyRejected: true, secureContext: context[0].secure, plainHttpWarning: context[0].warned };
  await both(c => c.load(async () => { await c.type('#access-key', secret); await c.press('#login-button'); }));
  const storage = await both(c => c.eval(`JSON.stringify({local:Object.keys(localStorage),session:Object.keys(sessionStorage),cookieVisible:document.cookie.length>0,field:document.getElementById('access-key').value.length})`).then(JSON.parse));
  for (const s of storage) { assert.equal(s.cookieVisible, false, 'Session cookie must be HttpOnly'); assert.equal(s.field, 0, 'Key field is cleared'); assert.equal(s.local.length, 0, 'Nothing in localStorage'); }
  report.afterLogin = { storage: storage[0] };

  // 2. Both reach the mode menu; PC creates a private room, the tablet joins with the match code.
  await both(c => until(c.name + ' menu', () => c.visible('#menu')));
  for (const c of [pc, tablet]) assert.equal(await c.eval("document.querySelector('[data-mode=online]').disabled"), false, 'Online is available after login');
  await both(c => c.press('[data-mode=online]'));
  await both(c => until(c.name + ' room panel', async () => (await c.visible('#room-panel')) && (await c.mode()).mode === 'online'));
  await pc.shot('pc-lobby');
  // Display names: typed before create/join, they travel with them and both seats see both names.
  await pc.type('#player-name', 'PC-One'); await tablet.type('#player-name', 'Pad_Two');
  await pc.press('#network-create');
  const created = await until('PC seat', async () => { const s = await pc.state(); return s?.player === 0 && s.code && s; });
  const code = created.code, display = await pc.text('#room-code-display');
  assert.equal(display, code.slice(0, 4) + '-' + code.slice(4));
  // Loading / waiting pause: the server does not simulate while only one seat is ready.
  const t0 = (await pc.state()).arena.ticks; await wait(600); const waiting = await pc.state();
  assert.equal(waiting.arena.ticks, t0, 'No simulation before the second player joins');
  assert.deepEqual(waiting.connected, [true, false]);
  await tablet.type('#join-code', display); await tablet.press('#join-button');
  const joined = await until('tablet seat', async () => { const s = await tablet.state(); return s?.player === 1 && s; });
  assert.equal(joined.code, code, 'Tablet joined the same room');
  await both(c => until(c.name + ' sees both names', async () => { const s = await c.state(); return s?.names?.[0] === 'PC-One' && s.names[1] === 'Pad_Two' && s; }));
  assert.equal(await pc.eval("localStorage.getItem('naifrog-name-v1')"), 'PC-One', 'The name is remembered locally, never the key');
  // The versus screen opens the match: both names over the stage, before the round introduction.
  assert((await pc.state()).arena.intro > 90, 'Both seated while the versus screen is up');
  await both(c => c.shot(c.name + '-versus'));
  report.names = { pc: 'PC-One', tablet: 'Pad_Two', bothSeated: true, versusIntro: (await pc.state()).arena.intro };
  await both(c => until(c.name + ' fight running', async () => { const s = await c.state(); return s.connected.every(Boolean) && s.arena.ticks > 10; }));
  // The round introduction (90 ticks, ~1.5 s) freezes input and the clock: no movement test
  // may hold a key across it, or the release lands before the fight is live.
  await both(c => until(c.name + ' round introduction over', async () => (await c.state()).arena.intro === 0));
  assert.deepEqual([(await pc.mode()).mode, (await tablet.mode()).mode], ['online', 'online']);
  await both(c => c.shot(c.name + '-joined'));
  report.room = { codeLength: code.length, pcPlayer: 0, tabletPlayer: 1, ticksWhileWaiting: waiting.arena.ticks };

  // 3. Movement from each device moves only its own seat, and both clients see it.
  assert.equal(await pc.eval('window.__naifrogTouch.visible'), false, 'A PC without touch shows no on-screen controls');
  const pad = JSON.parse(await tablet.eval('JSON.stringify(window.__naifrogTouch)'));
  assert(pad.visible && pad.sticks.length === 1 && pad.buttons.length === 4 && !pad.buttons.some(b => b.name === 'JUMP'), 'The tablet gets one stick and four buttons, no JUMP');
  await pc.focusGame();
  let s = await tablet.state(); const p1x = fighter(s, 0).x, p2x = fighter(s, 1).x;
  await pc.key('KeyD'); await wait(500); await pc.key('KeyD', 'keyUp');
  await until('P1 moved right on the tablet', async () => fighter(await tablet.state(), 0).x > p1x + 50000);
  assert.equal(fighter(await tablet.state(), 1).x, p2x, 'PC keys never move P2');
  // Keys are applied when they reach the server: wait for the release before measuring.
  const settled = c => until(c.name + ' sees no held keys', async () => { const r = await c.state(); return !r.held[0] && !r.held[1] && r; });
  s = await settled(pc); await wait(150); const p1after = fighter(await pc.state(), 0).x;
  await tablet.touchKeys(['left']); await wait(500); await tablet.touchKeys([]);
  await until('P2 moved left on the PC', async () => fighter(await pc.state(), 1).x < p2x - 50000);
  assert.equal(fighter(await pc.state(), 0).x, p1after, 'Tablet touch never moves P1');
  report.movement = { p1: [units(p1x), units(p1after)], p2: [units(p2x), units(fighter(await pc.state(), 1).x)] };

  // Close in: both hold forward until they are in light range.
  await pc.key('KeyD'); await tablet.touchKeys(['left']);
  await until('fighters in range', async () => { const a = await pc.state(); return fighter(a, 1).x - fighter(a, 0).x < 170000; }, 6000, 20);
  await pc.key('KeyD', 'keyUp'); await tablet.touchKeys([]); await wait(300);
  const ready = c => until(c.name + ' both ready', async () => { const a = await c.state(); return !a.arena.fighters.some(f => f.y || stateName(f) !== 'Ready') && a; }, 6000);

  // 4. Light attack from the PC damages P2 on both screens. The hit's snapshot also
  // opens the combo count at 1 (P2 is in hitstun for 22 ticks, ~7 snapshots).
  let a = await ready(pc); const hp0 = fighter(a, 1).hp;
  await pc.tapKey('KeyF');
  await both(c => until(c.name + ' sees P2 hit with an open combo', async () => { const f = fighter(await c.state(), 1); return f.hp < hp0 && f.combo_taken === 1; }));
  const hit = await both(async c => fighter(await c.state(), 1).hp);
  assert.equal(hit[0], hit[1], 'Both clients show the same P2 health');
  report.attack = { p2hp: [hp0, hit[0]], combo: 1 };

  // 5. Front guard on the tablet: a PC light (8) chips a quarter, rounded up: 2.
  a = await ready(pc); if (fighter(a, 1).x - fighter(a, 0).x > 190000) { await pc.key('KeyD'); await until('back in range', async () => { const b = await pc.state(); return fighter(b, 1).x - fighter(b, 0).x < 170000; }, 4000, 20); await pc.key('KeyD', 'keyUp'); a = await ready(pc); }
  const hp1 = fighter(a, 1).hp;
  await tablet.touchKeys(['guard']); await until('P2 guarding', async () => stateName(fighter(await pc.state(), 1)) === 'Guard');
  await pc.tapKey('KeyF'); await wait(700);
  const guarded = await both(async c => fighter(await c.state(), 1).hp);
  await tablet.touchKeys([]);
  assert.deepEqual(guarded, [hp1 - 2, hp1 - 2], 'Front guard chips 2 off a light');
  report.guard = { p2hp: [hp1, ...guarded] };

  // 5b. Milk bubble from the tablet's SHOT button hits the idle PC fighter for 10 on both screens.
  a = await ready(pc); const hp2 = fighter(a, 0).hp;
  await tablet.tapTouch('shot');
  await both(c => until(c.name + ' sees the bubble hit P1', async () => fighter(await c.state(), 0).hp < hp2, 3000));
  const bubbled = await both(async c => fighter(await c.state(), 0).hp);
  assert.deepEqual(bubbled, [hp2 - 10, hp2 - 10], 'The bubble takes 10 from P1');
  report.shot = { p1hp: [hp2, ...bubbled] };

  // 6. Tablet jump (stick pushed up, there is no jump button) is seen by the PC.
  await ready(pc); await tablet.tapTouch('jump');
  const air = await until('P2 airborne on the PC', async () => { const f = fighter(await pc.state(), 1); return f.y > 0 && f.y; }, 3000, 15);
  report.jump = { p2PeakSeenUnits: units(air) };

  // 7. Dash from the PC.
  await ready(pc); await pc.tapKey('KeyH');
  const dash = await until('P1 dash on the tablet', async () => stateName(fighter(await tablet.state(), 0)).includes('Dash') || null, 3000, 15);
  report.dash = { seen: dash };

  // 8. Agreement after things settle.
  await both(ready); await wait(300);
  const [sa, sb] = await both(c => c.state());
  const view = st => st.arena.fighters.map(f => [f.hp, f.x, f.y]);
  assert.deepEqual(view(sa), view(sb), 'Both clients agree on HP and positions');
  report.agreement = { hp: sa.arena.fighters.map(f => f.hp), x: sa.arena.fighters.map(f => units(f.x)) };

  // 9. Input timeout: a client that stops sending loses its held keys within ~250 ms.
  await pc.focusGame(); await pc.key('KeyA');
  await until('P1 holding left', async () => (await tablet.state()).held[0] & 1);
  await pc.eval('window.__realNetSend=window.__naifrogNetSend;window.__naifrogNetSend=()=>{}');
  await until('server dropped P1 held keys', async () => (await tablet.state()).held[0] === 0, 2000, 20);
  const stall = fighter(await tablet.state(), 0).x; await wait(400);
  assert.equal(fighter(await tablet.state(), 0).x, stall, 'P1 stops after the input timeout');
  await pc.eval('window.__naifrogNetSend=window.__realNetSend');
  await until('P1 moving again', async () => fighter(await tablet.state(), 0).x < stall - 20000, 2000, 20);
  await pc.key('KeyA', 'keyUp');
  report.inputTimeout = { heldAfterTimeout: 0, frozenAt: units(stall) };

  // 10. Disconnect pauses the match: the PC leaves the page, the tablet's input does nothing and the
  // clock stops. Coming back to the page in the same tab resumes the same seat (session-bound token).
  await both(ready);
  const before = await tablet.state();
  await pc.page.send('Page.navigate', { url: 'about:blank' });
  await until('tablet sees P1 disconnected', async () => !(await tablet.state()).connected[0], 5000);
  // The dropped seat is held five minutes: the tablet sees the hold counting down, both
  // in the inspection hook and as a Chinese m:ss note in the status line.
  const grace = await until('tablet sees the rival hold counting down', async () => { const r = await tablet.state(); return Number.isInteger(r?.graceLeft?.[0]) && r.graceLeft[0] <= 300 && r; });
  const held = await tablet.text('#room-status');
  assert(held.includes('席位保留'), 'Status names the held seat: ' + held);
  assert(/[0-9]+:[0-9]{2}/.test(held), 'Status shows an m:ss countdown: ' + held);
  await wait(2600);
  assert((await tablet.state()).graceLeft[0] < grace.graceLeft[0], 'The hold keeps counting down');
  const frozen = (await tablet.state()).arena.ticks;
  await tablet.touchKeys(['left']); await wait(700);
  const paused = await tablet.state(); await tablet.touchKeys([]);
  assert.equal(paused.arena.ticks, frozen, 'No simulation while a seat is disconnected');
  assert.equal(fighter(paused, 1).x, fighter(before, 1).x, 'Tablet input is ignored while paused');
  assert((await tablet.text('#room-status')).includes('对局暂停'), 'Tablet status shows the pause');
  await tablet.shot('tablet-rival-disconnected');
  await pc.load(() => pc.page.send('Page.navigate', { url: zhPage }));
  // The tablet watches the interrupted round count back in while the PC reloads.
  const counted = await until('tablet sees the resumed round count back in', async () => { const r = await tablet.state(); return r?.arena?.resume > 0 && r; }, 25000);
  const resumed = await until('PC resumed its seat', async () => { const r = await pc.state(); return r?.player === 0 && r.code === code && r.connected.every(Boolean) && r; }, 20000);
  assert.deepEqual(resumed.arena.fighters.map(f => f.hp), before.arena.fighters.map(f => f.hp), 'Health survives the reconnect');
  // Ticks advance through the countdown too, so the clock proves the fight resumed.
  await until('the clock runs again after the 3-2-1', async () => { const r = await tablet.state(); return r.arena.resume === 0 && r.arena.remaining < paused.arena.remaining && r; }, 15000);
  report.resume = { ticksFrozenAt: frozen, resumeSeenAt: counted.arena.resume, hp: resumed.arena.fighters.map(f => f.hp), status: await tablet.text('#room-status') };

  // 11. Stale sockets are fenced. A second tab of the same login (like a duplicated tab) joins with the
  // page's resume token and takes the seat; the page is told and does not fight back on its own. A
  // reload of the page takes the seat back, and the other tab's socket is then refused.
  const token = await pc.eval("sessionStorage.getItem('naifrog-room-v1')");
  const { targetId: dupId } = await pc.browser.send('Target.createTarget', { url: 'about:blank', browserContextId: pc.contextId });
  const dup = await connect(`ws://127.0.0.1:${pcPort}/devtools/page/${dupId}`);
  await dup.send('Runtime.enable'); await dup.send('Page.enable');
  if (pcIp) await dup.send('Network.enable').then(() => dup.send('Network.setExtraHTTPHeaders', { headers: { 'X-Real-IP': pcIp } }));
  await dup.send('Page.navigate', { url: new URL('/style.css', url).href }); await wait(500);
  const dupEval = async expression => (await dup.send('Runtime.evaluate', { expression, returnByValue: true })).result.value;
  await dupEval(`(()=>{const s=${JSON.stringify(JSON.parse(token))};const u=new URL('/api/ws',location.href);u.protocol='ws:';
    const ws=new WebSocket(u);window.__raw={log:[]};ws.onopen=()=>ws.send(JSON.stringify({type:'join',version:${PROTOCOL},code:s.code,resume:s.resume}));
    ws.onmessage=e=>{const m=JSON.parse(e.data);if(m.type!=='snapshot'||!window.__raw.log.includes('snapshot'))window.__raw.log.push(m.type==='snapshot'?'snapshot':m.type+':'+(m.message??m.reason??m.player??''));};
    ws.onclose=()=>window.__raw.log.push('close');})()`);
  try {
    await until('second tab welcomed as P1', async () => (await dupEval('JSON.stringify(window.__raw.log)')).includes('welcome:0'));
  } catch (e) {
    throw new Error('second tab not welcomed; its log: ' + await dupEval('JSON.stringify(window.__raw.log)') + '; token: ' + JSON.stringify(token) + '; storage now: ' + await pc.eval("sessionStorage.getItem('naifrog-room-v1')") + '; page status: ' + await pc.text('#room-status'));
  }
  await until('page told it was replaced', async () => (await pc.text('#room-status')).includes('另一个窗口'), 5000);
  const replacedText = await pc.text('#room-status');
  await wait(1500);
  assert.equal(await pc.text('#room-status'), replacedText, 'Replaced page does not reconnect by itself');
  assert(!JSON.parse(await dupEval('JSON.stringify(window.__raw.log)')).includes('close'), 'Second tab keeps the seat');
  await pc.shot('pc-replaced');
  // A wake must not reconnect a page the server told to stop: the replaced page stays
  // silent — its resume token now lives in the other tab — and the other tab keeps the seat.
  await pc.eval("window.dispatchEvent(new Event('pageshow'));window.dispatchEvent(new Event('online'));document.dispatchEvent(new Event('visibilitychange'))");
  await wait(2500);
  assert.equal(JSON.parse(await pc.eval('window.__naifrogNetPoll()')).player, null, 'The replaced page does not reconnect on a wake');
  assert.equal(await pc.text('#room-status'), replacedText, 'The replaced status is unchanged by the wake');
  assert(!JSON.parse(await dupEval('JSON.stringify(window.__raw.log)')).includes('close'), 'The second tab keeps the seat through the wake');
  await pc.load(() => pc.page.send('Page.reload', { ignoreCache: true }));
  await until('page took the seat back', async () => { const r = await pc.state(); return r?.player === 0 && r.code === code && r.connected.every(Boolean); }, 20000);
  const dupLog = await until('second tab refused', async () => { const l = JSON.parse(await dupEval('JSON.stringify(window.__raw.log)')); return l.includes('close') && l; }, 5000);
  assert(dupLog.includes('error:connection replaced'), 'Second tab told it was replaced: ' + dupLog);
  await pc.browser.send('Target.closeTarget', { targetId: dupId }); dup.close();
  report.staleSocket = { pageMessage: replacedText, otherTab: dupLog, wakeStayedSilent: true };

  // 12. Rematch needs both votes.
  await pc.focusGame(); await both(ready);
  const round = (await pc.state()).round;
  await pc.tapKey('KeyR');
  await until('PC vote visible to tablet', async () => (await tablet.state()).votes[0]);
  await wait(400); assert.equal((await pc.state()).round, round, 'One vote does not restart');
  await tablet.tapTouch('rematch');
  await both(c => until(c.name + ' new round', async () => { const r = await c.state(); return r.round === round + 1 && r.arena.fighters.every(f => f.hp === 100); }));
  report.rematch = { round: [round, round + 1] };

  // 13. Fight to a KO: P1 (PC) uses heavies; both clients must show the same outcome.
  await pc.focusGame();
  const ko = await until('KO', async () => {
    const r = await pc.state(); if (r.arena.outcome) return r;
    const [f1, f2] = r.arena.fighters, gap = f2.x - f1.x;
    if (gap > 220000) await pc.key('KeyD'); else { await pc.key('KeyD', 'keyUp'); if (stateName(f1) === 'Ready' && !['Knockdown', 'Getup', 'Hurt'].includes(stateName(f2))) await pc.tapKey('KeyG'); }
    return null;
  }, 90000, 60);
  await pc.key('KeyD', 'keyUp');
  const outcomes = await both(async c => JSON.stringify((await c.state()).arena.outcome));
  assert.equal(outcomes[0], outcomes[1]); assert.equal(outcomes[0], '{"Winner":0}');
  await wait(300); await both(c => c.shot(c.name + '-ko'));
  report.ko = { outcome: JSON.parse(outcomes[0]), hp: ko.arena.fighters.map(f => f.hp), secondsLeft: Math.ceil(ko.arena.remaining / 60) };

  // 14. Choosing a local mode leaves the room; the other side is told it closed.
  await tablet.press('#menu-toggle'); await until('tablet menu', () => tablet.visible('#menu'));
  await tablet.press('[data-mode=cpu]');
  await until('tablet in CPU mode', async () => (await tablet.mode()).mode === 'cpu');
  await until('PC told the room closed', async () => (await pc.text('#room-status')).includes('房间已关闭'), 5000);
  assert(await pc.eval("document.getElementById('retry-now').hidden"), 'No retry button after the server closed the room');
  report.leave = { tabletMode: (await tablet.mode()).mode, pcStatus: await pc.text('#room-status') };

  // 15. Room switch: the tablet creates a new room and the PC joins it as blue P2.
  await tablet.press('#menu-toggle'); await until('tablet menu again', () => tablet.visible('#menu'));
  await tablet.press('[data-mode=online]'); await until('tablet room panel', () => tablet.visible('#room-panel'));
  await tablet.press('#network-create');
  const second = await until('second room', async () => { const r = await tablet.state(); return r?.player === 0 && r.code !== code && r; });
  await pc.eval("window.__codes=new Set();window.__codeWatch=setInterval(()=>{const s=window.__naifrogState;if(s)window.__codes.add(s.code+'/'+s.player)},16)");
  const joinText = second.code.slice(0, 4) + '-' + second.code.slice(4);
  await pc.eval("document.getElementById('join-code').value=''"); await pc.type('#join-code', joinText); await pc.press('#join-button');
  await until('PC is P2 in the new room', async () => { const r = await pc.state(); return r?.code === second.code && r.player === 1 && r.connected.every(Boolean); });
  await both(c => until(c.name + ' new room running', async () => (await c.state()).arena.ticks > 20));
  const seen = JSON.parse(await pc.eval('clearInterval(window.__codeWatch);JSON.stringify([...window.__codes])'));
  assert(seen.every(v => v === second.code + '/1' || v === code + '/0'), 'Only the old seat before and the new seat after: ' + seen);
  assert.equal(seen.at(-1), second.code + '/1');
  await both(c => c.shot(c.name + '-switched'));
  await pc.press('#network-toggle').catch(() => {});
  report.switch = { newRoomDiffers: second.code !== code, pcNowPlayer: 1, tabletNowPlayer: 0, statesSeen: seen.length };

  // 16. Leave explicitly.
  await until('PC room panel', async () => { if (!(await pc.visible('#network'))) await pc.press('#network-toggle'); return pc.visible('#network'); }, 3000);
  await pc.press('#network-leave');
  await until('tablet told the room closed', async () => (await tablet.text('#room-status')).includes('房间已关闭'), 5000);
  assert(await tablet.eval("document.getElementById('retry-now').hidden"), 'No retry button after the server closed the room');

  // 17. A network switch strands the login. The tablet's link dies (held offline long enough
  // for the server's pong timeout to close its side, ~20 s), then returns under a different
  // X-Real-IP: the page's next input send hits the dead socket, it sees the drop, and its
  // joins are refused — the session is bound to the login address. After a few the page says
  // the login died and asks for the key again instead of retrying blind for five minutes.
  // Once the address is back, the browser's `online` event reconnects at once and the same
  // seat resumes without a re-login.
  if (tabletIp) {
    await until('tablet room panel after close', async () => { if (!(await tablet.visible('#network'))) await tablet.press('#network-toggle'); return tablet.visible('#room-panel'); }, 3000);
    await tablet.press('#network-create');
    const third = await until('third room held by the tablet', async () => { const r = await tablet.state(); return r?.player === 0 && r.code !== code && r.code !== second.code && r; });
    await tablet.page.send('Network.setExtraHTTPHeaders', { headers: { 'X-Real-IP': tabletIp.replace(/\d+$/, m => String(Number(m) + 40)) } });
    await tablet.page.send('Network.emulateNetworkConditions', { offline: true, latency: 0, downloadThroughput: 0, uploadThroughput: 0 });
    await wait(25000);
    await tablet.page.send('Network.emulateNetworkConditions', { offline: false, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
    await until('tablet asked for the key again', async () => (await tablet.text('#room-status')).includes('登录已失效') && !(await tablet.eval("document.getElementById('auth-panel').hidden")), 30000);
    const quiet = await tablet.text('#room-status');
    await wait(4000);
    assert.equal(await tablet.text('#room-status'), quiet, 'No further retries after the login was declared dead');
    // A foreground wake (pageshow / online / the tab becoming visible) must not wipe the
    // key prompt: the form stays up, what was typed stays, and at most one silent attempt
    // happens — under the wrong address it cannot succeed, so nothing on screen may change.
    await tablet.eval("document.getElementById('access-key').value='not-the-key'");
    await tablet.eval("window.dispatchEvent(new Event('pageshow'));window.dispatchEvent(new Event('online'));document.dispatchEvent(new Event('visibilitychange'))");
    const woken = JSON.parse(await tablet.eval("JSON.stringify({auth:document.getElementById('auth-panel').hidden,room:document.getElementById('room-panel').hidden,retry:document.getElementById('retry-now').hidden,key:document.getElementById('access-key').value,status:document.getElementById('room-status').textContent})"));
    assert(!woken.auth && woken.room, 'The wake kept the login prompt up');
    assert(woken.retry, 'No retry button while waiting for the key');
    assert.equal(woken.key, 'not-the-key', 'Typed key text survives the wake');
    assert(woken.status.includes('登录已失效'), 'The status still explains the dead login');
    await wait(2000);
    const settled = JSON.parse(await tablet.eval("JSON.stringify({auth:document.getElementById('auth-panel').hidden,room:document.getElementById('room-panel').hidden,retry:document.getElementById('retry-now').hidden,status:document.getElementById('room-status').textContent})"));
    assert(!settled.auth && settled.room && settled.retry, 'The prompt is still the only thing on screen');
    assert.equal(settled.status, quiet, 'The silent attempt changed nothing the user sees');
    await tablet.page.send('Network.setExtraHTTPHeaders', { headers: { 'X-Real-IP': tabletIp } });
    await tablet.eval("window.dispatchEvent(new Event('online'))");
    await until('seat resumed without re-login', async () => { const r = await tablet.state(); return r?.player === 0 && r.code === third.code && r.connected[0] && r; }, 10000);
    report.networkSwitch = { prompt: quiet, resumedAfterOnlineEvent: true };
  }

  for (const c of [pc, tablet]) assert.equal(c.errors.length, 0, c.name + ' console errors: ' + c.errors.join('\n'));
  console.log(JSON.stringify({ pass: true, ...report }, null, 2));
} catch (e) {
  console.error(e);
  for (const c of [pc, tablet]) if (c) { console.error(c.name, c.errors); await c.shot(c.name + '-failure').catch(() => {}); }
  process.exitCode = 1;
} finally {
  for (const c of [pc, tablet]) if (c) await c.close();
}
