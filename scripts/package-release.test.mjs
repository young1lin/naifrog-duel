import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import {unzipSync} from 'fflate';
import {packageRelease,verifyBundle} from './package-release.mjs';
async function fixture(run){
 const base=await fs.realpath(os.tmpdir()),root=await fs.mkdtemp(path.join(base,'naifrog-package-'));
 const put=async(name,content)=>{const at=path.join(root,name);await fs.mkdir(path.dirname(at),{recursive:true});await fs.writeFile(at,content);};
 try{
  await put('Cargo.toml','[package]\nname="naifrog-duel"\nversion="0.1.0"\n');
  await put('Cargo.lock','[[package]]\nname="naifrog-duel"\nversion="0.1.0"\n');
  await put('public-source.json',JSON.stringify({files:['assets/processed/runtime.webp']}));
  for(const file of ['LICENSE','NOTICE.md','assets/UNICODE-LICENSE.txt','assets/ART_PROVENANCE.json','assets/README.md','assets/processed/runtime.webp'])await put(file,'Public fixture');
  for(const kind of ['web','desktop','server'])for(const lang of ['zh','en'])await put('packaging/README.'+kind+'.'+lang+'.md','Version {{VERSION}}');
  for(const file of ['web/index.html','web/main.js','web/network.js','web/style.css','web/pkg/naifrog_duel.js','web/pkg/naifrog_duel_bg.wasm','web/pkg/naifrog_duel_bg.wasm.gz','scripts/serve-web.mjs','packaging/start-server.ps1','DEPLOY.md'])await put(file,'Public fixture');
  await put('target/x86_64-pc-windows-msvc/release/naifrog-duel.exe','Desktop fixture');
  await put('target/x86_64-pc-windows-msvc/release/naifrog-server.exe','Server fixture');
  await put('assets/original/private-draft.png','Never package this');
  await put('.env','Never package this');
  await run({root,put,commit:'a'.repeat(40)});
 }finally{if(!root.startsWith(base+path.sep))throw new Error('Unsafe fixture path');await fs.rm(root,{recursive:true,force:true});}
}
test('desktop archives contain runtime art, two languages and licenses, but no original drafts',()=>fixture(async({root,commit})=>{
 const result=await packageRelease({root,kind:'desktop',target:'x86_64-pc-windows-msvc',version:'0.1.0',commit});
 const entries=Object.keys(unzipSync(await fs.readFile(result.archive)));
 assert.ok(entries.some(x=>x.endsWith('/naifrog-duel.exe')));assert.ok(entries.some(x=>x.endsWith('/README.en.md')));
 assert.ok(entries.some(x=>x.endsWith('/assets/processed/runtime.webp')));
 assert.ok(!entries.some(x=>x.includes('/assets/original/')||x.endsWith('/.env')));
 assert.equal((await verifyBundle(result.folder)).source_commit,commit);
 await fs.writeFile(path.join(result.folder,'naifrog-duel.exe'),'Changed');
 await assert.rejects(verifyBundle(result.folder),/checksum mismatch/);
}));
test('complete server bundles require web files from the same version and commit',()=>fixture(async({root,commit})=>{
 const web=await packageRelease({root,kind:'web',version:'0.1.0',commit});
 const server=await packageRelease({root,kind:'server',target:'x86_64-pc-windows-msvc',version:'0.1.0',commit,webArchive:web.archive,out:path.join(root,'server'),archive:false});
 assert.equal(await fs.readFile(path.join(server.folder,'web/pkg/naifrog_duel_bg.wasm'),'utf8'),'Public fixture');
 await assert.rejects(packageRelease({root,kind:'server',target:'x86_64-pc-windows-msvc',version:'0.1.0',commit:'b'.repeat(40),webArchive:web.archive,out:path.join(root,'wrong'),archive:false}),/version\/commit differ/);
 await fs.appendFile(web.archive,'tamper');
 await assert.rejects(packageRelease({root,kind:'server',target:'x86_64-pc-windows-msvc',version:'0.1.0',commit,webArchive:web.archive,out:path.join(root,'tamper'),archive:false}),/checksum mismatch/);
}));
test('missing executables fail packaging instead of producing an empty download',()=>fixture(async({root,commit})=>{
 await fs.rename(path.join(root,'target/x86_64-pc-windows-msvc/release/naifrog-duel.exe'),path.join(root,'not-a-release.exe'));
 await assert.rejects(packageRelease({root,kind:'desktop',target:'x86_64-pc-windows-msvc',version:'0.1.0',commit,archive:false}));
}));
