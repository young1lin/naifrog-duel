import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import {validVersion,readVersion,checkVersion,bumpVersion,metadata} from './version.mjs';
async function fixture(run){
 const base=await fs.realpath(os.tmpdir()),root=await fs.mkdtemp(path.join(base,'naifrog-version-'));
 try{
  await fs.writeFile(path.join(root,'Cargo.toml'),'[package]\nname="naifrog-duel"\nversion="0.1.0"\n');
  await fs.writeFile(path.join(root,'Cargo.lock'),'[[package]]\nname="dependency"\nversion="0.1.0"\n\n[[package]]\nname="naifrog-duel"\nversion="0.1.0"\n');
  await fs.writeFile(path.join(root,'CHANGELOG.md'),'# Changes\n\n## [Unreleased]\n\n- New change\n\n## [0.1.0] - 2026-10-29\n\n- Initial release\n');
  await run(root);
 }finally{if(!root.startsWith(base+path.sep))throw new Error('Unsafe fixture path');await fs.rm(root,{recursive:true,force:true});}
}
test('version metadata distinguishes development builds from matching release tags',()=>{
 assert.equal(metadata('0.1.0','a'.repeat(40)).package_version,'0.1.0-dev.aaaaaaa');
 assert.equal(metadata('0.1.0','a'.repeat(40),'v0.1.0').package_version,'0.1.0');
 assert.throws(()=>metadata('0.1.0','a'.repeat(40),'v0.2.0'));
 for(const value of ['01.0.0','0.1','0.1.0-beta.01','v0.1.0','../version'])assert.equal(validVersion(value),false);
});
test('patch bump changes only the project lock entry and preserves release history',()=>fixture(async root=>{
 assert.equal(await bumpVersion(root,'patch','2026-10-29'),'0.1.1');
 assert.equal(await readVersion(root),'0.1.1');
 const lock=await fs.readFile(path.join(root,'Cargo.lock'),'utf8');
 assert.match(lock,/name="dependency"\nversion="0.1.0"/);
 const log=await fs.readFile(path.join(root,'CHANGELOG.md'),'utf8');
 assert.match(log,/## \[0.1.1\] - 2026-10-29/);assert.match(log,/## \[0.1.0\]/);
 assert.equal(await checkVersion(root,'v0.1.1'),'0.1.1');
 await assert.rejects(checkVersion(root,'v0.1.0'));
}));
test('mismatched lock versions and unfinished notes block release checks',()=>fixture(async root=>{
 await fs.writeFile(path.join(root,'Cargo.lock'),'[[package]]\nname="naifrog-duel"\nversion="0.2.0"\n');
 await assert.rejects(readVersion(root));
 await fs.writeFile(path.join(root,'Cargo.lock'),'[[package]]\nname="naifrog-duel"\nversion="0.1.0"\n');
 await fs.writeFile(path.join(root,'CHANGELOG.md'),'## [0.1.0]\n\n- TODO\n');
 await assert.rejects(checkVersion(root,'v0.1.0'));
}));
