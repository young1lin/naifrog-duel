// Security acceptance for a running naifrog-server, over raw HTTP requests and raw WebSocket
// frames so every header, frame size and message is under test control. Start a fresh, throwaway
// instance for it: the last step uses up the hourly transfer budget on purpose.
//
//   NAIFROG_PROXY_IPS=127.0.0.1  NAIFROG_MAX_ROOMS=2  NAIFROG_EGRESS_MB_PER_HOUR=64
//   $env:NAIFROG_ACCESS_KEY = <key>   # read from the environment, never printed
//   node scripts/security-smoke.mjs http://127.0.0.1:18091
//
// Every case sends its own X-Real-IP (trusted only because the request comes from the configured
// local proxy address), so per-IP quotas of one case do not leak into the next.
import http from 'node:http';
import net from 'node:net';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
// Must equal net::PROTOCOL_VERSION.
const PROTOCOL = 9;
const [base] = process.argv.slice(2);
const secret = process.env.NAIFROG_ACCESS_KEY ?? '';
assert(base, 'Usage: node scripts/security-smoke.mjs <url>');
assert(secret.length >= 32, 'Set NAIFROG_ACCESS_KEY to the server access key');
const { origin, hostname: host, port } = new URL(base);
const wait = ms => new Promise(r => setTimeout(r, ms));
async function until(what, check, ms = 3000, every = 20) {
  const end = Date.now() + ms;
  for (;;) { const v = await check(); if (v) return v; if (Date.now() > end) throw new Error('Timed out waiting for ' + what); await wait(every); }
}
const run = crypto.randomInt(1, 250); let nextIp = 0;
const freshIp = () => { nextIp++; return `10.${run}.${nextIp >> 8}.${nextIp & 255}`; };
const rows = [];
function expect(name, got, want) {
  rows.push(name + ' -> ' + (typeof got === 'string' ? got : JSON.stringify(got)));
  assert.deepEqual(got, want, name);
}

function request(path, { method = 'GET', ip, origin: from, cookie, headers = {}, body, keep = 4096 } = {}) {
  return new Promise((resolve, reject) => {
    const h = { ...headers };
    if (ip !== undefined) h['X-Real-IP'] = ip;
    if (from) h.Origin = from;
    if (cookie) h.Cookie = cookie;
    if (body !== undefined) h['Content-Length'] = Buffer.byteLength(body);
    const req = http.request({ host, port, path, method, headers: h, agent: false }, res => {
      let size = 0, kept = 0; const chunks = [];
      res.on('data', d => { size += d.length; if (kept < keep) { chunks.push(d); kept += d.length; } });
      res.on('end', () => resolve({ status: res.statusCode, headers: res.headers, size, text: Buffer.concat(chunks).toString('utf8') }));
      res.on('error', reject);
    });
    req.on('error', reject);
    if (body !== undefined) req.write(body);
    req.end();
  });
}
const status = async (path, options) => (await request(path, options)).status;
const json = 'application/json';
async function login(ip = freshIp()) {
  const r = await request('/api/auth', { method: 'POST', ip, origin, headers: { 'Content-Type': json }, body: JSON.stringify({ access_key: secret }) });
  assert.equal(r.status, 200, 'login from ' + ip);
  return { ip, cookie: r.headers['set-cookie'][0].split(';')[0], setCookie: r.headers['set-cookie'][0] };
}

// Minimal RFC 6455 client: masked client frames, unfragmented server frames.
function ws({ ip, cookie, origin: from = origin } = {}) {
  return new Promise(resolve => {
    const sock = net.connect(Number(port), host);
    const c = { status: 0, messages: [], last: null, frames: 0, closed: false };
    let buf = Buffer.alloc(0), upgraded = false;
    c.frame = (op, payload) => {
      const n = payload.length, mask = crypto.randomBytes(4);
      const head = n < 126 ? Buffer.from([0x80 | op, 0x80 | n]) : Buffer.from([0x80 | op, 0x80 | 126, n >> 8, n & 255]);
      const body = Buffer.from(payload); for (let i = 0; i < n; i++) body[i] ^= mask[i & 3];
      if (!sock.destroyed) sock.write(Buffer.concat([head, mask, body]));
    };
    c.send = message => c.frame(1, Buffer.from(typeof message === 'string' ? message : JSON.stringify(message)));
    c.close = () => { c.frame(8, Buffer.alloc(0)); sock.end(); };
    c.reply = (what, ms = 3000) => until(what, () => c.messages[0] ?? (c.closed && { type: 'socket-closed' }), ms);
    c.ended = (what, ms = 3000) => until(what, () => c.closed, ms);
    c.said = () => c.messages.filter(m => m.type === 'error' || m.type === 'closed').map(m => m.message ?? m.reason);
    sock.on('connect', () => {
      const lines = ['GET /api/ws HTTP/1.1', `Host: ${host}:${port}`, 'Upgrade: websocket', 'Connection: Upgrade', 'Sec-WebSocket-Version: 13', 'Sec-WebSocket-Key: ' + crypto.randomBytes(16).toString('base64')];
      if (ip !== undefined) lines.push('X-Real-IP: ' + ip);
      if (from) lines.push('Origin: ' + from);
      if (cookie) lines.push('Cookie: ' + cookie);
      sock.write(lines.join('\r\n') + '\r\n\r\n');
    });
    sock.on('data', d => {
      buf = Buffer.concat([buf, d]);
      if (!upgraded) {
        const end = buf.indexOf('\r\n\r\n'); if (end < 0) return;
        c.status = Number(buf.toString('latin1', 0, end).split(' ')[1]); buf = buf.subarray(end + 4); upgraded = true;
        if (c.status !== 101) sock.destroy();
        resolve(c);
      }
      for (;;) {
        if (buf.length < 2) return;
        const op = buf[0] & 15; let len = buf[1] & 127, at = 2;
        if (len === 126) { if (buf.length < 4) return; len = buf.readUInt16BE(2); at = 4; }
        else if (len === 127) { if (buf.length < 10) return; len = Number(buf.readBigUInt64BE(2)); at = 10; }
        if (buf.length < at + len) return;
        const payload = buf.subarray(at, at + len); buf = buf.subarray(at + len);
        if (op === 1) { const m = JSON.parse(payload.toString('utf8')); if (m.type === 'snapshot') { c.last = m; c.frames++; } else c.messages.push(m); }
        else if (op === 9) c.frame(10, payload);
        else if (op === 8) c.close(); // Answer the close handshake, as browsers do.
      }
    });
    sock.on('error', () => {});
    sock.on('close', () => { c.closed = true; if (!upgraded) resolve(c); });
  });
}
// Open a socket as `user`, send the first message, return the socket and its first non-snapshot reply.
async function enter(user, first) {
  const c = await ws(user); assert.equal(c.status, 101, 'upgrade for ' + JSON.stringify(first));
  c.send(first); return [c, await c.reply('reply to ' + (typeof first === 'string' ? 'raw frame' : first.type))];
}
const denied = async (c, what) => { await c.ended(what); return c.said().join(',') || 'closed without reason'; };

try {
  // 1. Client address: X-Real-IP is trusted only from the configured proxy, exactly once and well formed.
  expect('config, proxy peer without X-Real-IP', await status('/api/config'), 401);
  expect('config, two X-Real-IP headers', await status('/api/config', { headers: { 'X-Real-IP': ['10.0.0.1', '10.0.0.2'] } }), 401);
  expect('config, malformed X-Real-IP', await status('/api/config', { ip: 'not-an-ip' }), 401);
  const config = await request('/api/config', { ip: freshIp() });
  expect('config, one valid X-Real-IP', [config.status, JSON.parse(config.text)], [200, { multiplayer: true, authenticated: false, protocol: PROTOCOL }]);

  // 2. Files: only the login page is public; game files need a session; nothing else exists.
  const anon = freshIp();
  const page = await request('/', { ip: anon });
  expect('GET / headers', [page.status, page.headers['cache-control'], page.headers['x-content-type-options'], page.headers['x-frame-options'], page.headers['referrer-policy'], /frame-ancestors 'none'/.test(page.headers['content-security-policy']) && /connect-src 'self'/.test(page.headers['content-security-policy'])], [200, 'private, no-store', 'nosniff', 'DENY', 'no-referrer', true]);
  const pub = []; for (const p of ['/index.html', '/main.js', '/network.js', '/style.css']) pub.push(await status(p, { ip: anon }));
  expect('public page files', pub, [200, 200, 200, 200]);
  const prot = []; for (const p of ['/pkg/naifrog_duel.js', '/pkg/naifrog_duel_bg.wasm', '/assets/processed/game-assets.json', '/assets/processed/fighter_idle.webp', '/api/ws']) prot.push(await status(p, { ip: anon }));
  expect('game files and /api/ws without a session', prot, [401, 401, 401, 401, 401]);
  const hidden = []; for (const p of ['/assets/original/', '/Cargo.toml', '/web/index.html', '/.env', '/DEPLOY.md', '/target/release/naifrog-server.exe', '/scripts/server.test.mjs']) hidden.push(await status(p, { ip: anon }));
  expect('project files are never served', hidden, [404, 404, 404, 404, 404, 404, 404]);
  expect('POST / not allowed', await status('/', { method: 'POST', ip: anon, body: '' }), 405);

  // 3. Login: exact Origin, JSON only, no extra fields, small body, right key.
  const lip = freshIp(), auth = (headers, body) => status('/api/auth', { method: 'POST', ip: lip, headers, body });
  const key = JSON.stringify({ access_key: secret });
  const attempts = [
    await auth({ 'Content-Type': json }, key),
    await auth({ 'Content-Type': json, Origin: 'http://evil.example' }, key),
    await auth({ 'Content-Type': json, Origin: origin + '.evil.example' }, key),
    await auth({ 'Content-Type': 'text/plain', Origin: origin }, key),
    await auth({ 'Content-Type': json, Origin: origin }, JSON.stringify({ access_key: secret, admin: true })),
    await auth({ 'Content-Type': json, Origin: origin }, JSON.stringify({ access_key: secret + ' '.repeat(2000) })),
    await auth({ 'Content-Type': json, Origin: origin }, JSON.stringify({ access_key: crypto.randomBytes(32).toString('hex') })),
  ];
  expect('login: no Origin, foreign, look-alike, text/plain, extra field, 2 KB body, wrong key', attempts, [401, 401, 401, 401, 401, 401, 401]);
  const user = await login(lip);
  const flags = user.setCookie.split(';').map(s => s.trim());
  expect('session cookie', [flags[0].split('=')[0], flags[0].split('=')[1].length >= 43, flags.includes('HttpOnly'), flags.includes('SameSite=Strict'), flags.includes('Path=/'), flags.some(f => f.startsWith('Max-Age=')), flags.includes('Secure'), user.setCookie.includes(secret)], ['naifrog_session', true, true, true, true, true, false, false]);
  const authed = JSON.parse((await request('/api/config', { ip: lip, cookie: user.cookie })).text).authenticated;
  expect('logged in: config and game script', [authed, await status('/pkg/naifrog_duel.js', { ip: lip, cookie: user.cookie })], [true, 200]);
  // The manifest and versioned game files: kept by the browser, still behind the login;
  // refusals and API answers never stored.
  const anonManifest = await request('/api/manifest', { ip: lip });
  const manifest = await request('/api/manifest', { ip: lip, cookie: user.cookie });
  const listed = JSON.parse(manifest.text).files;
  const hash = listed['pkg/naifrog_duel.js'];
  expect('manifest: anonymous, logged in, entries', [anonManifest.status, anonManifest.headers['cache-control'], manifest.status, manifest.headers['cache-control'], ['pkg/naifrog_duel.js', 'pkg/naifrog_duel_bg.wasm', 'assets/processed/fighter_idle.webp'].every(name => /^[0-9a-f]{20}$/.test(listed[name])), Object.keys(listed).some(name => !/^(pkg|assets\/processed)\/[^/]+$/.test(name) || name.endsWith('.gz'))], [401, 'private, no-store', 200, 'private, no-store', true, false]);
  const kept = await request(`/v/${hash}/pkg/naifrog_duel.js`, { ip: lip, cookie: user.cookie });
  const plainJs = await request('/pkg/naifrog_duel.js', { ip: lip, cookie: user.cookie });
  const anonKept = await request(`/v/${hash}/pkg/naifrog_duel.js`, { ip: lip });
  const again = await request('/pkg/naifrog_duel.js', { ip: lip, cookie: user.cookie, headers: { 'If-None-Match': plainJs.headers.etag } });
  expect('cache: versioned, plain, anonymous versioned, revalidated', [kept.status, kept.headers['cache-control'], plainJs.headers['cache-control'], anonKept.status, anonKept.headers['cache-control'], again.status, again.text.length], [200, 'private, max-age=31536000, immutable', 'private, no-cache', 401, 'private, no-store', 304, 0]);
  const traversal = []; for (const p of ['/pkg/../../Cargo.toml', '/pkg/..%2f..%2fCargo.toml', '/assets/processed/../original/', '/assets/processed/..%5c..%5cCargo.toml', '/pkg/%2e%2e/%2e%2e/Cargo.toml']) traversal.push(await status(p, { ip: lip, cookie: user.cookie }));
  expect('path traversal with a session', traversal, [404, 404, 404, 404, 404]);

  // 4. Brute force: 10 attempts per IP per minute, then everything is refused, even the right key.
  const bip = freshIp(), wrong = [];
  for (let i = 0; i < 11; i++) wrong.push(await status('/api/auth', { method: 'POST', ip: bip, origin, headers: { 'Content-Type': json }, body: JSON.stringify({ access_key: crypto.randomBytes(32).toString('hex') }) }));
  const right = await status('/api/auth', { method: 'POST', ip: bip, origin, headers: { 'Content-Type': json }, body: key });
  expect('11 wrong keys then the right one', [wrong.slice(0, 10).every(s => s === 401), wrong[10], right], [true, 429, 429]);

  // 5. A session works only from the address that logged in; forged cookies do nothing.
  const other = freshIp(), forged = 'naifrog_session=' + crypto.randomBytes(32).toString('base64url');
  const moved = [JSON.parse((await request('/api/config', { ip: other, cookie: user.cookie })).text).authenticated, await status('/pkg/naifrog_duel.js', { ip: other, cookie: user.cookie }), (await ws({ ip: other, cookie: user.cookie })).status];
  expect('stolen cookie from another IP: config, file, ws', moved, [false, 401, 401]);
  expect('forged cookie: file, ws', [await status('/pkg/naifrog_duel.js', { ip: lip, cookie: forged }), (await ws({ ip: lip, cookie: forged })).status], [401, 401]);

  // 6. WebSocket upgrade needs the exact Origin and a session.
  const gate = [(await ws({ ip: lip })).status, (await ws({ ip: lip, cookie: user.cookie, origin: '' })).status, (await ws({ ip: lip, cookie: user.cookie, origin: 'http://evil.example' })).status, (await ws({ ip: lip, cookie: user.cookie, origin: origin + '.evil.example' })).status];
  expect('ws: no cookie, no Origin, foreign Origin, look-alike Origin', gate, [401, 401, 401, 401]);

  // 7. The first message must be a well-formed create / join of this protocol version.
  const firsts = [];
  for (const first of [{ type: 'input', seq: 1, round: 0, held: 0, pressed: 0 }, '{not json', { type: 'create', version: 0 }, { type: 'join', version: PROTOCOL, code: 'AAAAAAAA' }, { type: 'join', version: PROTOCOL, code: 'AAAAAAAA', player: 0 }, { type: 'create', version: PROTOCOL, player: 1 }]) {
    const [c, reply] = await enter(user, first); await c.ended('first message refusal'); firsts.push(reply.message ?? reply.type);
  }
  expect('first message: input, bad JSON, old version, unknown room, seat choice, extra field', firsts, ['request denied', 'request denied', 'client outdated, reload the page', 'room unavailable', 'request denied', 'request denied']);
  { const c = await ws(user); c.frame(2, Buffer.from(`{"type":"create","version":${PROTOCOL}}`)); await c.ended('binary refusal'); expect('first message as binary frame', c.said(), ['request denied']); }
  { const c = await ws(user); c.send(JSON.stringify({ type: 'create', version: PROTOCOL, pad: 'x'.repeat(1500) })); await c.ended('oversized refusal'); expect('first frame over 1024 bytes: closed, no seat', c.messages.some(m => m.type === 'welcome'), false); }
  { const c = await ws(user); const t = Date.now(); await c.ended('silent socket timeout', 8000); expect('silent socket closed after the first-message timeout', Math.round((Date.now() - t) / 1000) >= 4, true); }

  // 8. Rooms, seats and forged input. P creates, Q joins, R is an unrelated login.
  const P = await login(), Q = await login(), R = await login();
  const [pSock, pWelcome] = await enter(P, { type: 'create', version: PROTOCOL });
  expect('create', [pWelcome.type, pWelcome.player, /^[A-Z2-9]{8}$/.test(pWelcome.code), /^[a-f0-9]{64}$/.test(pWelcome.resume)], ['welcome', 0, true, true]);
  const code = pWelcome.code;
  { const [c, r] = await enter(P, { type: 'create', version: PROTOCOL }); await c.ended('busy'); expect('same login, second room while seated', r.message, 'already in another room'); }
  let [qSock, qWelcome] = await enter(Q, { type: 'join', version: PROTOCOL, code });
  expect('join by match code', [qWelcome.type, qWelcome.player], ['welcome', 1]);
  let qResume = qWelcome.resume;
  pSock.send({ type: 'input', seq: 1, round: 0, held: 0, pressed: 0 }); qSock.send({ type: 'input', seq: 1, round: 0, held: 0, pressed: 0 });
  await until('match running', () => pSock.last?.connected.every(Boolean) && pSock.last.arena.ticks > 5);
  const spawn = pSock.last.arena.fighters.map(f => [f.hp, f.x]);
  let qSeq = 1;
  // Each forged message ends Q's socket with a generic refusal; Q's seat survives and resumes.
  const resumeQ = async () => {
    const [c, r] = await enter(Q, { type: 'join', version: PROTOCOL, code, resume: qResume });
    assert.equal(r.player, 1, 'Q resumes seat P2'); qResume = r.resume; qSock = c; qSeq = 0; return c;
  };
  const forgedInputs = [
    ['unknown held bit', s => ({ type: 'input', seq: s, round: 0, held: 256, pressed: 0 })], // 128 is SHOT
    ['press of a held-only key', s => ({ type: 'input', seq: s, round: 0, held: 0, pressed: 1 })],
    ['extra hp field', s => ({ type: 'input', seq: s, round: 0, held: 0, pressed: 0, hp: 0 })],
    ['extra player field', s => ({ type: 'input', seq: s, round: 0, held: 2, pressed: 0, player: 0 })],
    ['position field', s => ({ type: 'input', seq: s, round: 0, held: 0, pressed: 0, x: 0 })],
    ['future round', s => ({ type: 'input', seq: s, round: 5, held: 0, pressed: 0 })],
    ['negative seq', () => ({ type: 'input', seq: -1, round: 0, held: 0, pressed: 0 })],
    ['create inside a room', () => ({ type: 'create', version: PROTOCOL })],
    ['join inside a room', () => ({ type: 'join', version: PROTOCOL, code })],
    ['winner message', () => ({ type: 'winner', player: 1 })],
    ['leave with extra field', () => ({ type: 'leave', code })],
  ];
  const forgedResults = {};
  for (const [name, make] of forgedInputs) {
    await resumeQ(); qSock.send(make(++qSeq)); forgedResults[name] = await denied(qSock, name);
  }
  await resumeQ(); qSock.send({ type: 'input', seq: 5, round: 0, held: 0, pressed: 0 }); await wait(100); qSock.send({ type: 'input', seq: 5, round: 0, held: 0, pressed: 0 });
  forgedResults['replayed seq'] = await denied(qSock, 'replay');
  await resumeQ(); qSock.send(JSON.stringify({ type: 'input', seq: 1, round: 0, held: 0, pressed: 0, pad: 'x'.repeat(1500) }));
  forgedResults['1.5 KB frame'] = await denied(qSock, 'oversized frame');
  await resumeQ(); qSock.frame(2, Buffer.from([1, 2, 3]));
  forgedResults['binary frame'] = await denied(qSock, 'binary frame');
  await resumeQ(); for (let i = 1; i <= 400; i++) qSock.send({ type: 'input', seq: i, round: 0, held: 0, pressed: 0 });
  forgedResults['400 inputs at once'] = await denied(qSock, 'flood');
  // A frame over the size limit is a protocol error: the socket closes before any reason can be sent.
  expect('forged / abusive messages each close the socket with a reason', Object.entries(forgedResults).every(([name, r]) => r === (name === '1.5 KB frame' ? 'closed without reason' : 'request denied')), true);
  rows.push('  ' + JSON.stringify(forgedResults));
  await resumeQ(); qSock.send({ type: 'input', seq: 1, round: 0, held: 0, pressed: 0 });
  await until('room running again', () => pSock.last?.connected.every(Boolean));
  await wait(300);
  expect('after all forged messages: health and positions unchanged', pSock.last.arena.fighters.map(f => [f.hp, f.x]), spawn);
  // Cross-session resume, and a third player for a full room.
  const before = qSock.frames;
  const [r1, steal] = await enter(R, { type: 'join', version: PROTOCOL, code, resume: qResume }); await r1.ended('cross-session');
  const [r2, third] = await enter(R, { type: 'join', version: PROTOCOL, code }); await r2.ended('full room');
  await wait(300);
  expect('other login with P2 resume token / without token', [steal.message, third.message], ['room unavailable', 'room unavailable']);
  expect('real P2 keeps the seat', [qSock.closed, qSock.frames > before], [false, true]);
  // The same login replacing its own socket fences the old one.
  const oldQ = qSock; await resumeQ();
  await oldQ.ended('old socket fenced'); expect('older socket of the same seat', oldQ.said(), ['connection replaced']);

  // 8b. Display names are sanitized server-side: hostile or oversized names fall back to the
  // seat default and are never echoed anywhere in a snapshot (trimming is covered by unit tests).
  const NP = await login(), NQ = await login();
  const hostile = '<script>alert(1)</script>';
  const [n1, n1w] = await enter(NP, { type: 'create', version: PROTOCOL, name: hostile });
  assert.equal(n1w.type, 'welcome', 'a hostile name still creates the room');
  const [n2, n2w] = await enter(NQ, { type: 'join', version: PROTOCOL, code: n1w.code, name: '奶蛙\u0007x' + 'A'.repeat(12) });
  assert.equal(n2w.type, 'welcome');
  await until('names room running', () => n1.last?.connected.every(Boolean));
  expect('hostile names become the seat defaults', n1.last.names, ['P1', 'P2']);
  expect('hostile name never echoed', JSON.stringify(n1.last).includes(hostile) || JSON.stringify(n1.last).includes('奶蛙'), false);
  n1.send({ type: 'leave' }); await n1.ended('names room closed'); await n2.ended('names room P2 socket closed');

  // 9. Concurrent sockets: at most 2 per login and 4 per IP.
  const wip = freshIp(), S1 = await login(wip), S2 = await login(wip), S3 = await login(wip);
  const idle = [await ws(S1), await ws(S1), await ws(S1), await ws(S2), await ws(S2), await ws(S3)];
  expect('sockets: login1 x3, login2 x2, login3 x1 on one IP', idle.map(c => c.status), [101, 101, 429, 101, 101, 429]);
  for (const c of idle) if (c.status === 101) c.close();

  // 10. Upgrade rate: 30 per IP per minute.
  const T = await login(), upgrades = [];
  for (let i = 0; i < 31; i++) { const c = await ws(T); upgrades.push(c.status); if (c.status === 101) { c.send({ type: 'leave' }); await c.ended('short socket'); } }
  expect('31 upgrades in a minute', [upgrades.slice(0, 30).every(s => s === 101), upgrades[30]], [true, 429]);

  // 11. Room limit (server started with NAIFROG_MAX_ROOMS=2; P/Q hold one room).
  const U1 = await login(), U2 = await login();
  const [u1, u1w] = await enter(U1, { type: 'create', version: PROTOCOL });
  const [u2, u2w] = await enter(U2, { type: 'create', version: PROTOCOL }); await u2.ended('server full');
  u1.send({ type: 'leave' }); await u1.ended('leave');
  const [u3, u3w] = await enter(U2, { type: 'create', version: PROTOCOL }); u3.send({ type: 'leave' }); await u3.ended('leave');
  expect('second room, third room, after one leaves', [u1w.type, u2w.message, u1.said()[0], u3w.type], ['welcome', 'server full', 'room closed', 'welcome']);

  // 12. File request quotas: 120 per IP per minute, of which 6 may be the WASM (WASM_PER_IP_PER_MINUTE).
  const V = await login(), heads = [];
  for (let i = 0; i < 121; i++) heads.push(await status('/assets/processed/game-assets.json', { method: 'HEAD', ip: V.ip, cookie: V.cookie }));
  expect('121 file requests in a minute', [heads.slice(0, 120).every(s => s === 200), heads[120]], [true, 429]);
  const W = await login(), wasm = [];
  for (let i = 0; i < 7; i++) wasm.push(await status('/pkg/naifrog_duel_bg.wasm', { method: 'HEAD', ip: W.ip, cookie: W.cookie }));
  expect('7 WASM requests in a minute', wasm, [200, 200, 200, 200, 200, 200, 429]);

  // 13. Transfer budget (server started with NAIFROG_EGRESS_MB_PER_HOUR=64). Logged-out traffic may use
  // only 1/16 of it; the rest is shared. When it is gone, files and sockets are refused with a reason.
  let ip = freshIp(), publicBytes = 0, publicCount = 0, last;
  for (;;) {
    last = await request('/network.js', { ip });
    if (last.status === 429) { ip = freshIp(); continue; }
    if (last.status !== 200) break;
    publicBytes += last.size; publicCount++;
  }
  expect('logged-out downloads stop near 4 MiB (64 MiB / 16)', [last.status, publicBytes > 3.8 * 2 ** 20 && publicBytes <= 4 * 2 ** 20], [503, true]);
  rows.push(`  ${publicCount} logged-out downloads, ${publicBytes} bytes`);
  let X = await login();
  expect('same file with a session still loads', await status('/network.js', { ip: X.ip, cookie: X.cookie }), 200);
  const big = await request('/pkg/naifrog_duel_bg.wasm', { ip: X.ip, cookie: X.cookie, keep: 0 });
  expect('first full WASM download', [big.status, big.size === Number(big.headers['content-length'])], [200, true]);
  // More full copies until one no longer fits; how many fit depends on the WASM's size.
  let fitted = 1, refused;
  for (;;) {
    refused = await request('/pkg/naifrog_duel_bg.wasm', { ip: X.ip, cookie: X.cookie, keep: 0 });
    if (refused.status !== 200 || fitted >= 5) break;
    fitted++;
  }
  expect('later WASM copies stop fitting the budget', refused.status, 503);
  rows.push(`  ${fitted} full WASM downloads fitted (${big.size} bytes each)`);
  const files = ['stage_garden.webp', 'fx_hit.webp', 'fighter_hurt.webp', 'fighter_heavy.webp', 'fighter_guard.webp', 'fighter_idle.webp', 'fighter_dash.webp', 'game-assets.json', 'shadow.webp'];
  let drained = 0;
  for (const f of files) for (;;) {
    const r = await request('/assets/processed/' + f, { ip: X.ip, cookie: X.cookie, keep: 0 });
    if (r.status === 429) { X = await login(); continue; }
    if (r.status !== 200) break;
    drained += r.size;
  }
  rows.push(`  drained ${drained} more bytes with a session`);
  await until('live room told the budget is gone', () => pSock.said().includes('server transfer budget exhausted') || qSock.said().includes('server transfer budget exhausted'), 10000);
  const Y = await login();
  // The last few bytes may still fit the new room's welcome; the socket then ends with the reason.
  const [y, yReply] = await enter(Y, { type: 'create', version: PROTOCOL }); await y.ended('budget refusal');
  expect('budget exhausted: live sockets, page, game file, new room', [pSock.said().concat(qSock.said()).includes('server transfer budget exhausted'), await status('/', { ip: Y.ip }), await status('/pkg/naifrog_duel.js', { ip: Y.ip, cookie: Y.cookie }), y.said()], [true, 503, 503, ['server transfer budget exhausted']]);
  rows.push(`  new room's first reply: ${yReply.type}`);
  pSock.close(); qSock.close();
  for (const r of rows) console.log('ok  ' + r);
  console.log('PASS: security acceptance');
} catch (e) {
  for (const r of rows) console.log('ok  ' + r);
  console.error(e);
  process.exitCode = 1;
}
setTimeout(() => process.exit(), 100).unref();
