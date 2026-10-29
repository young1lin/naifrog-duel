// Gamepad-only play in the real WASM page against a running naifrog-server. A simulated
// standard-mapping pad replaces navigator.getGamepads(), which both the page panels and the game
// (gilrs) poll every frame. With only that pad a PC player: walks the mode menu, creates a private
// room, fights a keyboard rival in a second isolated context, opens the room panel mid-match and
// returns to the game, votes for the rematch with A after a knockout, leaves for the CPU mode
// through the menu, pauses / resumes with Select and B, lands a hit on the CPU, and after the CPU
// knocks it out starts the next round with A. A simulated pad proves the page and game paths, not drivers or real
// hardware: see TESTING.md for the manual checklist.
//
//   $env:NAIFROG_ACCESS_KEY = <key>   # typed into the login field (a pad cannot type), never printed
//   node scripts/gamepad-smoke.mjs <cdp-port> <url>
//
// Each context sends a fresh X-Real-IP: start the server with NAIFROG_PROXY_IPS=127.0.0.1 so
// reruns do not hit the per-address WASM quota. The browser must be a debuggable, GPU-backed Chromium.
import fs from 'node:fs/promises';
import assert from 'node:assert/strict';
const [port, url] = process.argv.slice(2);
const secret = process.env.NAIFROG_ACCESS_KEY ?? '';
assert(port && url, 'Usage: node scripts/gamepad-smoke.mjs <cdp-port> <url>');
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
// A plain object is enough: gilrs reads Gamepad fields by name and never checks the type.
const PAD_STUB = `(() => {
  const pad = { id: 'Simulated pad (STANDARD GAMEPAD)', index: 0, connected: true, mapping: 'standard', timestamp: 0,
    buttons: Array.from({ length: 17 }, () => ({ pressed: false, touched: false, value: 0 })), axes: [0, 0, 0, 0] };
  window.__pad = { set(i, on) { pad.buttons[i] = { pressed: on, touched: on, value: on ? 1 : 0 }; pad.timestamp = performance.now(); } };
  Navigator.prototype.getGamepads = function () { return [pad, null, null, null]; };
})()`;
// Standard mapping (https://w3c.github.io/gamepad/#remapping).
const A = 0, B = 1, X = 2, RB = 5, SELECT = 8, UP = 12, DOWN = 13, LEFT = 14, RIGHT = 15;
const run = Date.now() % 250;
// Keyboard keys for the rival: [key, virtual key code].
const KEYS = { KeyA: ['a', 65], KeyG: ['g', 71], KeyR: ['r', 82] };
// Fixed ticks in a round (combat::TIME_LIMIT), minus the introduction's dead ticks
// (combat::INTRO_TICKS, plus combat::VS_TICKS on round one only): after the outcome
// `remaining` freezes, so ticks - (intro + TIME_LIMIT - remaining) is how long the
// result has shown (Arena::ticks_since_outcome).
const TIME_LIMIT = 99 * 60, REMATCH_DELAY = 60, INTRO_TICKS = 90, VS_TICKS = 120, ROUND_END_TICKS = 120;
// Mirrors combat::Arena::ticks_since_outcome, dead ticks (hit-stop, resume countdown) included.
const sinceOutcome = s => s.arena.outcome ? s.arena.ticks - (INTRO_TICKS + (s.arena.game === 0 ? VS_TICKS : 0) + (s.arena.dead ?? 0) + TIME_LIMIT - s.arena.remaining) : null;
const stateName = f => typeof f.state === 'string' ? f.state : Object.entries(f.state).flat().join(':');
async function client(name, { pad, ip }) {
  const version = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
  const browser = await connect(version.webSocketDebuggerUrl);
  const { browserContextId } = await browser.send('Target.createBrowserContext', { disposeOnDetach: true });
  const { targetId } = await browser.send('Target.createTarget', { url: 'about:blank', browserContextId });
  const page = await connect(`ws://127.0.0.1:${port}/devtools/page/${targetId}`);
  const c = { name, page, errors: [], onReady: null };
  page.on(m => {
    if (m.method === 'Runtime.consoleAPICalled') {
      const text = m.params.args.map(a => a.value ?? a.description ?? '').join(' ');
      if (text.includes('All 17 game textures ready')) c.onReady?.();
      if (m.params.type === 'error') c.errors.push(text);
    } else if (m.method === 'Runtime.exceptionThrown') c.errors.push(JSON.stringify(m.params.exceptionDetails));
  });
  await page.send('Runtime.enable'); await page.send('Page.enable'); await page.send('Network.enable');
  await page.send('Network.setExtraHTTPHeaders', { headers: { 'X-Real-IP': ip } });
  await page.send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 720, deviceScaleFactor: 0, mobile: false });
  await page.send('Emulation.setFocusEmulationEnabled', { enabled: true });
  if (pad) await page.send('Page.addScriptToEvaluateOnNewDocument', { source: PAD_STUB });
  c.eval = async expression => (await page.send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })).result.value;
  c.state = async () => JSON.parse(await c.eval('JSON.stringify(window.__naifrogState??null)'));
  c.mode = async () => JSON.parse(await c.eval('JSON.stringify(window.__naifrogMode)'));
  c.visible = selector => c.eval(`!document.querySelector(${JSON.stringify(selector)}).hidden`);
  // What has focus: a page control's id or mode, or 'game' for the canvas.
  c.focus = () => c.eval("(e=>e?.id||e?.dataset?.mode||e?.tagName||'none')(document.activeElement)");
  c.load = action => { const ready = new Promise((r, j) => { c.onReady = r; setTimeout(() => j(new Error(name + ' game never became ready')), 60000); }); return action().then(() => ready); };
  c.press = async selector => {
    const r = JSON.parse(await c.eval(`JSON.stringify(document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect())`));
    for (const type of ['mouseMoved', 'mousePressed', 'mouseReleased']) await page.send('Input.dispatchMouseEvent', { type, x: r.x + r.width / 2, y: r.y + r.height / 2, button: 'left', clickCount: 1 });
  };
  c.type = async (selector, text) => { await c.eval(`document.querySelector(${JSON.stringify(selector)}).focus()`); await page.send('Input.insertText', { text }); };
  c.hold = (button, on = true) => c.eval(`window.__pad.set(${button}, ${on})`);
  c.tap = async (button, ms = 90) => { await c.hold(button); await wait(ms); await c.hold(button, false); await wait(90); };
  c.key = async (code, type = 'keyDown') => { const [key, vk] = KEYS[code]; await page.send('Input.dispatchKeyEvent', { type, code, key, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk }); };
  c.tapKey = async (code, ms = 60) => { await c.key(code); await wait(ms); await c.key(code, 'keyUp'); };
  // Pad-only menu walking: step with the d-pad until `target` has focus.
  c.walkTo = async (target, step = DOWN) => { for (let i = 0; i < 12 && (await c.focus()) !== target; i++) await c.tap(step); assert.equal(await c.focus(), target, `pad focus reaches ${target}`); };
  c.shot = async label => { const r = await page.send('Page.captureScreenshot', { format: 'png' }); await fs.mkdir('artifacts', { recursive: true }); await fs.writeFile(`artifacts/gamepad-${label}.png`, Buffer.from(r.data, 'base64')); return Buffer.from(r.data, 'base64'); };
  c.close = async () => { try { await browser.send('Target.disposeBrowserContext', { browserContextId }); } catch {} page.close(); browser.close(); };
  return c;
}
// A screenshot's decoded pixels: size, stride and RGBA bytes.
async function pixels(png) {
  const zlib = await import('node:zlib');
  let at = 8, w, h, ch; const idat = [];
  while (at < png.length) { const n = png.readUInt32BE(at), tag = png.subarray(at + 4, at + 8).toString(), d = png.subarray(at + 8, at + 8 + n); if (tag === 'IHDR') { w = d.readUInt32BE(0); h = d.readUInt32BE(4); ch = { 2: 3, 6: 4 }[d[9]]; assert(ch && d[8] === 8 && d[12] === 0, 'unexpected screenshot format'); } if (tag === 'IDAT') idat.push(d); at += n + 12; }
  const raw = zlib.inflateSync(Buffer.concat(idat)), stride = w * ch, out = Buffer.alloc(h * stride); let p = 0;
  for (let y = 0; y < h; y++) { const f = raw[p++]; for (let x = 0; x < stride; x++) { const a = x >= ch ? out[y * stride + x - ch] : 0, c = y && x >= ch ? out[(y - 1) * stride + x - ch] : 0, d = y ? out[(y - 1) * stride + x] : 0; let v = 0; if (f === 1) v = a; if (f === 2) v = d; if (f === 3) v = (a + d) >> 1; if (f === 4) { const q = a + d - c, pa = Math.abs(q - a), pb = Math.abs(q - d), pc = Math.abs(q - c); v = pa <= pb && pa <= pc ? a : pb <= pc ? d : c; } out[y * stride + x] = (raw[p++] + v) & 255; } }
  return { w, h, out, stride, ch };
}
// Health-bar fill pixels in a screenshot, P1 gold and P2 blue (the same probes as browser-smoke.mjs).
async function bars(png) {
  const { w, h, out, stride, ch } = await pixels(png);
  const y = Math.round(h * 0.075);
  const count = (from, to, hit) => { let n = 0; for (let x = Math.ceil(w * from); x < Math.floor(w * to); x++) { const i = y * stride + x * ch; if (hit(out[i], out[i + 1], out[i + 2])) n++; } return n; };
  return { gold: count(0.03, 0.44, (r, g, b) => r > 200 && g > 150 && g < 215 && b < 110), blue: count(0.56, 0.97, (r, g, b) => r < 130 && g > 170 && b > 200) };
}
// Lit round-win dots under the bars' outer edges, P1 gold on the left and P2 blue on the
// right (a lit dot is ~64 px of seat color). A restart resets the whole match, leaving
// both dots dark for round 1; the automatic round advance keeps the wins lit.
async function wins(png) {
  const { w, h, out, stride, ch } = await pixels(png);
  const count = (from, to, hit) => { let n = 0; for (let y = Math.ceil(h * 0.103); y < Math.floor(h * 0.128); y++) for (let x = Math.ceil(w * from); x < Math.floor(w * to); x++) { const i = y * stride + x * ch; if (hit(out[i], out[i + 1], out[i + 2])) n++; } return n; };
  return { gold: count(0.0, 0.025, (r, g, b) => r > 200 && g > 150 && g < 215 && b < 110), blue: count(0.975, 1.0, (r, g, b) => r < 130 && g > 170 && b > 200) };
}
const fighter = (s, i) => s.arena.fighters[i];
let pc, rival; const report = {};
try {
  pc = await client('pc', { pad: true, ip: `10.99.${run}.2` });
  rival = await client('rival', { pad: false, ip: `10.99.${run}.3` });
  for (const c of [pc, rival]) {
    await c.page.send('Page.navigate', { url: zhPage });
    await until(c.name + ' login form', () => c.eval("!!document.getElementById('access-key')&&!document.getElementById('auth-panel').hidden&&!document.getElementById('network').hidden"));
    await c.load(async () => { await c.type('#access-key', secret); await c.press('#login-button'); });
    await until(c.name + ' mode menu', () => c.visible('#menu'));
  }

  // 1. The mode menu and the room panel with the pad alone: d-pad moves focus, A activates.
  assert.equal(await pc.focus(), 'cpu', 'menu opens with focus on the first mode');
  await pc.tap(DOWN); assert.equal(await pc.focus(), 'local', 'd-pad down moves focus');
  await pc.tap(UP); assert.equal(await pc.focus(), 'cpu', 'd-pad up moves focus back');
  await pc.walkTo('online');
  await pc.tap(A);
  await until('room panel from the pad', async () => (await pc.visible('#network')) && (await pc.visible('#room-panel')));
  await pc.walkTo('network-create');
  await pc.tap(A);
  const created = await until('pad-created room', async () => { const s = await pc.state(); return s?.player === 0 && s.code && s; });
  assert.equal(await pc.focus(), 'game', 'the game has focus after creating the room');
  await pc.shot('created');
  report.menu = { created: true, codeLength: created.code.length };

  // 2. A keyboard rival joins; the pad moves only its own seat on both screens.
  await rival.press('[data-mode=online]');
  await until('rival room panel', () => rival.visible('#room-panel'));
  await rival.type('#join-code', created.code); await rival.press('#join-button');
  await until('fight running', async () => { const s = await rival.state(); return s?.connected.every(Boolean) && s.arena.ticks > 10; });
  // The round introduction (90 ticks, ~1.5 s) freezes input: a 500 ms hold that straddles
  // it moves nobody, so wait until the fight is live before measuring movement.
  await until('round introduction over', async () => (await rival.state()).arena.intro === 0);
  let s = await rival.state(); const x0 = fighter(s, 0).x, p2x = fighter(s, 1).x;
  await pc.hold(RIGHT); await wait(500); await pc.hold(RIGHT, false);
  await until('pad moved P1 right on the rival screen', async () => fighter(await rival.state(), 0).x > x0 + 50000);
  assert.equal(fighter(await rival.state(), 1).x, p2x, 'the pad never moves P2');

  // 3. Select opens the room panel during an online match; B returns to the game and the pad still plays.
  await pc.tap(SELECT);
  await until('Select opened the room panel', () => pc.visible('#network'));
  assert.notEqual(await pc.focus(), 'game', 'the panel has focus');
  await pc.tap(B);
  await until('B closed the room panel', async () => !(await pc.visible('#network')) && (await pc.focus()) === 'game');
  s = await rival.state(); const x1 = fighter(s, 0).x;
  await pc.hold(LEFT); await wait(500); await pc.hold(LEFT, false);
  await until('pad moved P1 left after the panel', async () => fighter(await rival.state(), 0).x < x1 - 50000);
  report.online = { padMovedP1: true, selectOpensRoomPanel: true, backReturnsToGame: true };

  // 3b. RB throws a milk bubble at the idle keyboard rival: 10 off P2 on both screens.
  const hp2 = fighter(await rival.state(), 1).hp;
  await pc.tap(RB);
  for (const c of [rival, pc]) await until(c.name + ' sees the pad bubble hit P2', async () => fighter(await c.state(), 1).hp < hp2, 4000);
  const bubbled = [fighter(await rival.state(), 1).hp, fighter(await pc.state(), 1).hp];
  assert.deepEqual(bubbled, [hp2 - 10, hp2 - 10], 'the bubble takes 10 from P2 on both screens');
  report.online.rbBubble = [hp2, ...bubbled];

  // 4. The rival knocks out the idle pad player. An A tapped while the result is new does not vote,
  //    nor does one held past REMATCH_DELAY, nor a fresh A while a MID-MATCH round result still shows:
  //    the pad's A only restarts once the whole match is decided. The round then auto-advances with
  //    its wins kept. After the second KO the match is decided: rival R + a fresh pad A restart it,
  //    and the new match shows no extra jump. Tick counts come from the pad client's own snapshots,
  //    the same arena its game decides on.
  await rival.eval("document.getElementById('game').focus()");
  const ko = await until('rival KO of the idle pad player', async () => {
    const r = await rival.state(); if (r.arena.outcome) return r;
    const [f1, f2] = r.arena.fighters, gap = f2.x - f1.x;
    if (gap > 220000) await rival.key('KeyA'); else { await rival.key('KeyA', 'keyUp'); if (stateName(f2) === 'Ready' && !['Knockdown', 'Getup', 'Hurt'].includes(stateName(f1))) await rival.tapKey('KeyG'); }
    return null;
  }, 90000, 60);
  await rival.key('KeyA', 'keyUp');
  assert.deepEqual(ko.arena.outcome, { Winner: 1 }, 'the rival won by knockout');
  await pc.tap(A);
  const tappedAt = sinceOutcome(await pc.state());
  assert(tappedAt < REMATCH_DELAY, `A tapped and released while the result is new (${tappedAt} ticks)`);
  await wait(200);
  assert.deepEqual((await rival.state()).votes, [false, false], 'an early A does not vote');
  await pc.hold(A);
  const pressedAt = sinceOutcome(await pc.state());
  assert(pressedAt < REMATCH_DELAY, `A pressed while the result is new (${pressedAt} ticks)`);
  await until('result shown past the delay', async () => sinceOutcome(await pc.state()) >= REMATCH_DELAY, 5000);
  await pc.hold(A, false); await wait(300);
  s = await rival.state();
  assert.deepEqual([s.round, s.votes], [ko.round, [false, false]], 'an A held through the delay does not vote');
  // A fresh A inside the same result window must not vote either: the match is not decided.
  const windowAt = sinceOutcome(await pc.state());
  assert(windowAt >= REMATCH_DELAY && windowAt < ROUND_END_TICKS - 12,
    `the fresh-A window check landed at ${windowAt} ticks, needed [${REMATCH_DELAY}, ${ROUND_END_TICKS - 12})`);
  await pc.tap(A);
  await wait(250);
  s = await rival.state();
  assert.deepEqual(s.votes, [false, false], 'a fresh A on a mid-match round result does not vote');
  assert.equal(s.round, ko.round, 'no restart happened while the match was still open');
  const second = await until('the round auto-advanced past its result', async () => {
    const r = await pc.state(); return r.arena.game === 1 && r.arena.intro === 0 && r;
  }, 15000);
  assert.deepEqual(second.arena.wins, [0, 1], 'the auto-advance keeps the round wins');
  assert.equal(second.round, ko.round, 'the room round counter does not move on an auto-advance');
  // The rival takes the second round too: the match is decided, and only now does a fresh A vote.
  const ko2 = await until('rival KO of the idle pad player again', async () => {
    const r = await rival.state(); if (r.arena.outcome) return r;
    const [f1, f2] = r.arena.fighters, gap = f2.x - f1.x;
    if (gap > 220000) await rival.key('KeyA'); else { await rival.key('KeyA', 'keyUp'); if (stateName(f2) === 'Ready' && !['Knockdown', 'Getup', 'Hurt'].includes(stateName(f1))) await rival.tapKey('KeyG'); }
    return null;
  }, 90000, 60);
  await rival.key('KeyA', 'keyUp');
  assert.deepEqual(ko2.arena.outcome, { Winner: 1 }, 'the rival won the second round');
  assert.deepEqual(ko2.arena.match_outcome, { Winner: 1 }, 'two round wins decide the match');
  await until('match result shown past the delay', async () => { const r = await pc.state(); return r.arena.match_outcome && sinceOutcome(r) >= REMATCH_DELAY + 10; }, 15000);
  await rival.tapKey('KeyR');
  await until('rival voted', async () => (await pc.state()).votes[1]);
  await pc.tap(A);
  await until('pad A was the deciding vote', async () => (await pc.state()).round === ko2.round + 1);
  const fresh = await pc.state();
  assert.equal(fresh.arena.game, 0, 'the restart begins a whole new match');
  assert.deepEqual(fresh.arena.wins, [0, 0], 'the restart clears the round wins');
  const jumps = [];
  for (const end = Date.now() + 500; Date.now() < end; await wait(50)) { const f = fighter(await pc.state(), 0); jumps.push(f.y); }
  assert(jumps.every(y => y === 0), 'the A that started the match is not a jump: ' + jumps);
  await pc.shot('online-rematch');
  report.online.rematch = { koRound: ko.round, earlyTapTicks: tappedAt, earlyTapIgnored: true, heldThroughDelayIgnored: true, midMatchAIgnored: true, autoAdvanceKeepsWins: [0, 1], matchEndFreshAVotes: true, noJumpAfterRestart: true };

  // 5. Leave for the CPU mode through Select -> 模式菜单 -> 单机打人机, all on the pad.
  await pc.tap(SELECT);
  await until('room panel again', () => pc.visible('#network'));
  await pc.walkTo('network-menu');
  await pc.tap(A);
  await until('mode menu from the room panel', () => pc.visible('#menu'));
  await pc.walkTo('cpu', UP);
  await pc.tap(A);
  await until('CPU match running', async () => { const m = await pc.mode(); return m.mode === 'cpu' && !m.paused; });
  await until('rival told the room closed', () => rival.eval("document.getElementById('room-status').textContent.includes('房间已关闭')"));

  // 6. Select pauses (game) and resumes (page menu); B also resumes; no flicker back into the menu.
  await pc.tap(SELECT);
  await until('Select paused behind the menu', async () => (await pc.mode()).paused && (await pc.visible('#menu')));
  await pc.tap(SELECT);
  await until('Select resumed', async () => !(await pc.mode()).paused && !(await pc.visible('#menu')));
  await wait(400); assert.equal((await pc.mode()).paused, false, 'still running after Select resumed');
  await pc.tap(SELECT);
  await until('paused again', async () => (await pc.mode()).paused);
  await pc.tap(B);
  await until('B resumed', async () => !(await pc.mode()).paused && (await pc.focus()) === 'game');
  await wait(400); assert.equal((await pc.mode()).paused, false, 'still running after B resumed');
  report.cpu = { selectPauses: true, selectResumes: true, backResumes: true };

  // 7. The pad fights the CPU: X lands a light punch. Waiting for the CPU to come close is not
  //    enough (it can knock the idle player out first), so each try walks in, then jabs.
  await wait(300);
  const full = await bars(await pc.shot('cpu-before'));
  let after = full.blue;
  await until('pad light punch lands on the CPU', async () => {
    await pc.hold(RIGHT); await wait(400); await pc.hold(RIGHT, false);
    for (let i = 0; i < 3; i++) { await pc.tap(X, 60); await wait(200); }
    after = (await bars(await pc.shot('cpu-hit'))).blue; return after < full.blue - 10;
  }, 20000, 0);
  report.cpu.blueBar = [full.blue, after];

  // 8. The reported case: the CPU knocks out the idle pad player. A held from before the knockout
  //    must not restart on release (only a fresh press after ~1 s can), and with the best-of-three
  //    match a round result lasts ~2 s (120 ticks) before the next round starts by itself. The
  //    CPU's second win ends the match; that final result stays up, and a fresh A there resets the
  //    whole match at full health. Whether the stale-A check lands inside the 2 s window or after
  //    the auto-advance is timing (KO polling lags the KO by about a second), so it reads the
  //    round-win dots, not the bars: the ongoing match keeps the CPU's dot lit either way, while a
  //    restart resets the match and leaves both dots dark. Screenshots are the only signal offline.
  await pc.hold(A);
  const down = await until('CPU knocks out the pad player', async () => { const b = await bars(await pc.shot('cpu-ko')); return b.gold === 0 && b; }, 110000, 0);
  await wait(800);
  await pc.hold(A, false); await wait(300);
  const heldPng = await pc.shot('cpu-held-a');
  const held = await bars(heldPng), dots = await wins(heldPng);
  assert(dots.blue >= 40, `a stale A released in the result window does not restart the match (bars ${JSON.stringify(held)}, dots ${JSON.stringify(dots)})`);
  const auto = await until('the next round starts by itself', async () => { const p = await pc.shot('cpu-next-round'); const b = await bars(p), d = await wins(p); return b.gold >= full.gold - 4 && b.blue >= full.blue - 4 && d.blue >= 40 && { bars: b, dots: d }; }, 4000, 0);
  const down2 = await until('the CPU takes the match', async () => { const b = await bars(await pc.shot('cpu-match-end')); return b.gold === 0 && b; }, 110000, 0);
  await wait(1100);
  await pc.tap(A);
  const next = await until('pad A started a fresh match', async () => { const p = await pc.shot('cpu-rematch'); const b = await bars(p), d = await wins(p); return b.gold >= full.gold - 4 && b.blue >= full.blue - 4 && d.blue === 0 && d.gold === 0 && { bars: b, dots: d }; }, 4000, 0);
  report.cpu.rematch = { ko: down, heldThrough: { bars: held, dots }, autoAdvance: auto, matchEnd: down2, freshAMatch: next };

  for (const c of [pc, rival]) assert.equal(c.errors.length, 0, c.name + ' console errors: ' + c.errors.join('\n'));
  console.log(JSON.stringify({ pass: true, ...report }, null, 2));
} catch (e) {
  console.error(e); console.error(JSON.stringify({ pass: false, ...report, pcFocus: await pc?.focus().catch(() => '?'), pcErrors: pc?.errors, rivalErrors: rival?.errors }));
  process.exitCode = 1;
} finally { await pc?.close(); await rival?.close(); }
