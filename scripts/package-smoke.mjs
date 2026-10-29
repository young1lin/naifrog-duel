import assert from 'node:assert/strict';
import path from 'node:path';
import http from 'node:http';
import net from 'node:net';
import {spawn} from 'node:child_process';
import crypto from 'node:crypto';
import {pathToFileURL} from 'node:url';
import {verifyBundle} from './package-release.mjs';
const folder=path.resolve(process.argv[2]??''),record=await verifyBundle(folder);
const isWasm=bytes=>Buffer.from(bytes).subarray(0,4).equals(Buffer.from([0,97,115,109]));
let server,child;
try{
 let origin,headers={};
 if(record.kind==='web'){
  const {createHandler}=await import(pathToFileURL(path.join(folder,'scripts/serve-web.mjs')).href);
  server=http.createServer(createHandler(folder));await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  origin='http://127.0.0.1:'+server.address().port;
 }else if(record.kind==='server'){
  const reserve=net.createServer();await new Promise(resolve=>reserve.listen(0,'127.0.0.1',resolve));
  const port=reserve.address().port;await new Promise(resolve=>reserve.close(resolve));origin='http://127.0.0.1:'+port;
  const key=crypto.randomBytes(32).toString('hex'),binary=path.join(folder,'naifrog-server'+(record.target.includes('windows')?'.exe':''));
  child=spawn(binary,[],{cwd:folder,env:{...process.env,NAIFROG_BIND:'127.0.0.1:'+port,NAIFROG_ORIGIN:origin,NAIFROG_ASSET_ROOT:folder,NAIFROG_ACCESS_KEY:key},stdio:'ignore'});
  let ready=false;
  for(let i=0;i<100;i++){
   if(child.exitCode!==null)throw new Error('Packaged server exited before accepting requests.');
   try{await fetch(origin+'/api/config',{signal:AbortSignal.timeout(500)});ready=true;break;}catch{await new Promise(resolve=>setTimeout(resolve,100));}
  }
  if(!ready)throw new Error('Packaged server did not become ready.');
  // The login page is public; the game runtime and manifest require a session.
  assert.equal((await fetch(origin+'/')).status,200);
  assert.equal((await (await fetch(origin+'/api/config')).json()).authenticated,false);
  for(const name of ['/pkg/naifrog_duel_bg.wasm','/assets/processed/game-assets.json','/api/manifest']){
   assert.equal((await fetch(origin+name)).status,401,name);
  }
  const login=await fetch(origin+'/api/auth',{method:'POST',headers:{Origin:origin,'Content-Type':'application/json'},body:JSON.stringify({access_key:key})});
  assert.equal(login.status,200);const cookie=login.headers.get('set-cookie');assert.ok(cookie?.includes('HttpOnly'));headers={Cookie:cookie.split(';')[0]};
  assert.equal((await (await fetch(origin+'/api/config',{headers})).json()).authenticated,true);
 }else throw new Error('Use the native --version check for desktop packages.');
 const page=await fetch(origin+'/',{headers});assert.equal(page.status,200);assert.match(await page.text(),/main\.js/);
 const wasm=await fetch(origin+'/pkg/naifrog_duel_bg.wasm',{headers});assert.equal(wasm.status,200);assert.ok(isWasm(await wasm.arrayBuffer()));
 const manifest=await fetch(origin+'/assets/processed/game-assets.json',{headers});assert.equal(manifest.status,200);
 for(const name of ['main.js','network.js','style.css'])assert.equal((await fetch(origin+'/'+name,{headers})).status,200);
 assert.equal((await fetch(origin+'/assets/original/fight-1.png',{headers})).status,404);
 console.log('PASS: packaged '+record.kind+' serves real WASM, page and assets; private inputs remain inaccessible.');
}finally{
 if(server)await new Promise(resolve=>server.close(resolve));
 if(child){child.kill();await Promise.race([new Promise(resolve=>child.once('exit',resolve)),new Promise(resolve=>setTimeout(resolve,3000))]);}
}
