import {test} from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {PAGE_FILES,requestPath,createHandler} from './serve-web.mjs';

// HTTP routing tests use disposable fixtures instead of depending on game atlas content.
async function start(t){
 const root=fs.mkdtempSync(path.join(os.tmpdir(),'naifrog-static-'));
 let server;
 t.after(async()=>{
  if(server?.listening){
   server.closeAllConnections();
   await new Promise((resolve,reject)=>server.close(error=>error?reject(error):resolve()));
  }
  fs.rmSync(root,{recursive:true,force:true});
 });
 fs.mkdirSync(path.join(root,'web'),{recursive:true});
 fs.mkdirSync(path.join(root,'assets/processed'),{recursive:true});
 for(const file of PAGE_FILES)fs.copyFileSync(new URL('../web/'+file,import.meta.url),path.join(root,'web',file));
 fs.writeFileSync(path.join(root,'assets/processed/fighter_idle.webp'),'http-art-fixture');
 fs.writeFileSync(path.join(root,'assets/processed/name_font.otf'),'http-font-fixture');
 fs.writeFileSync(path.join(root,'Cargo.toml'),'private fixture');
 server=http.createServer(createHandler(root));
 await new Promise((resolve,reject)=>server.once('error',reject).listen(0,'127.0.0.1',resolve));
 return 'http://127.0.0.1:'+server.address().port;
}

test('only public game files are addressable',()=>{
 assert(requestPath('/'));
 assert(requestPath('/pkg/naifrog_duel_bg.wasm'));
 assert(requestPath('/assets/processed/fighter_idle.webp'));
 for(const p of ['/Cargo.toml','/assets/original/fight.png','/../Cargo.toml','/pkg/%2e%2e/Cargo.toml','/pkg/%5c..%5cCargo.toml','/pkg/%252e%252e/x','/pkg/%zz'])assert.equal(requestPath(p),null,p);
});

test('HTTP serves page/assets, rejects private files and writes',async t=>{
 const url=await start(t);
 const page=await fetch(url);
 assert.equal(page.status,200);
 assert.match(await page.text(),/id="game"/);
 const art=await fetch(url+'/assets/processed/fighter_idle.webp',{method:'HEAD'});
 assert.equal(art.headers.get('content-type'),'image/webp');
 assert.equal(art.status,200);
 assert.equal(art.headers.get('content-length'),String(Buffer.byteLength('http-art-fixture')));
 const font=await fetch(url+'/assets/processed/name_font.otf',{method:'HEAD'});
 assert.equal(font.status,200);
 assert.equal(font.headers.get('content-type'),'font/otf');
 assert.equal((await fetch(url+'/Cargo.toml')).status,404);
 assert.equal((await fetch(url,{method:'POST'})).status,405);
});

test('every page module the HTML imports is served, including /network.js',async t=>{
 const url=await start(t);
 for(const file of ['main.js','network.js','style.css']){
  const response=await fetch(url+'/'+file);
  assert.equal(response.status,200,file);
  assert.equal(response.headers.get('x-naifrog-server'),'offline-static');
  assert.equal(await response.text(),fs.readFileSync(new URL('../web/'+file,import.meta.url),'utf8'),file);
 }
 assert.match(fs.readFileSync(new URL('../web/main.js',import.meta.url),'utf8'),/\/network\.js/);
});
