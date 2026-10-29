import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {spawnSync,execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const checker=fileURLToPath(new URL('./public-source.mjs',import.meta.url));
async function fixture(run){
 const temp=await fs.realpath(os.tmpdir()),root=await fs.mkdtemp(path.join(temp,'naifrog-public-check-'));
 try{
  await fs.writeFile(path.join(root,'public-source.json'),JSON.stringify({schema_version:1,asset_policy:'character-art-excluded-pending-redistribution-rights',files:['public-source.json','README.md'],directories:[],exclude:[]}));
  await fs.writeFile(path.join(root,'README.md'),'Public fixture\n');
  const git=args=>execFileSync('git',['-C',root,...args],{stdio:'pipe'});
  git(['init','--quiet','-b','main']);
  const hooks=path.join(root,'.empty-hooks');await fs.mkdir(hooks);
  git(['config','core.hooksPath',hooks]);git(['config','user.name','Privacy test']);git(['config','user.email','ci@users.noreply.github.com']);git(['config','commit.gpgsign','false']);
  git(['add','public-source.json','README.md']);git(['commit','--quiet','-m','Public fixture']);
  const check=()=>spawnSync(process.execPath,[checker,'check','--root',root],{encoding:'utf8'});
  await run({root,git,check});
 }finally{
  const resolved=await fs.realpath(root);
  if(!resolved.startsWith(temp+path.sep)||!path.basename(resolved).startsWith('naifrog-public-check-'))throw new Error('Unexpected test cleanup path');
  await fs.rm(resolved,{recursive:true,force:true});
 }
}
test('public history accepts its root tree but rejects removed private files',()=>fixture(async({root,git,check})=>{
 assert.equal(check().status,0);
 await fs.mkdir(path.join(root,'artifacts'));await fs.writeFile(path.join(root,'artifacts','private.txt'),'Private fixture');
 git(['add','-f','artifacts/private.txt']);git(['commit','--quiet','-m','Private fixture']);
 git(['rm','--quiet','artifacts/private.txt']);git(['commit','--quiet','-m','Remove private fixture']);
 const result=check();assert.equal(result.status,1);assert.match(result.stderr,/Non-public path remains in reachable Git history/);
}));
test('public checks reject synthetic credentials without printing their value',()=>fixture(async({root,check})=>{
 const token=['gh','p_','A'.repeat(36)].join('');await fs.writeFile(path.join(root,'README.md'),token);
 const result=check();assert.equal(result.status,1);assert.match(result.stderr,/github-token/);assert.ok(!(result.stdout+result.stderr).includes(token));
}));

test('annotated release tags are not paths and cannot hide a private file with the same name',()=>fixture(async({root,git,check})=>{
 git(['-c','tag.gpgSign=false','tag','-a','v0.1.0','-m','Release fixture']);git(['tag','release-lightweight']);
 assert.equal(check().status,0);
 await fs.writeFile(path.join(root,'v0.1.0'),'Private fixture');
 git(['add','v0.1.0']);git(['commit','--quiet','-m','Private tag-named fixture']);
 git(['rm','--quiet','v0.1.0']);git(['commit','--quiet','-m','Remove tag-named fixture']);
 const result=check();assert.equal(result.status,1);assert.match(result.stderr,/Non-public path remains in reachable Git history: v0\.1\.0/);
}));
