// Export a fresh, allowlisted project without private working files or old Git history.
import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {parseArgs} from 'node:util';
import {findPrivacyIssues} from './privacy-rules.mjs';

const ROOT=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const MANIFEST='PUBLIC_SOURCE_MANIFEST.json';
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const slash=name=>name.split(path.sep).join('/');

async function exists(file){try{await fs.lstat(file);return true;}catch(error){if(error.code==='ENOENT')return false;throw error;}}

function privatePath(name,includeArt=false){
 const parts=name.split('/'),base=parts.at(-1);
 return ['archive','artifacts','target','dist','node_modules','.agents','.claude','.superpowers','review','blender'].includes(parts[0])
  || name==='AGENTS.md'||name==='CLAUDE.md'
  || (!includeArt&&(name.startsWith('assets/original/')||name==='assets/original'))
  || (!includeArt&&/^assets\/processed\/(?:fighter_.*|portrait\.webp|stage_garden\.webp|fx_hit\.webp)$/.test(name))
  || /^assets\/processed\/(?:naifrog_.*|.*\.preview\.gif)$/.test(name)
  || name.startsWith('web/pkg/')||name.startsWith('docs/prompts/')||name.startsWith('docs/superpowers/')
  || parts.includes('secrets')
  || (/^\.env(?:\.|$)/.test(base)&&!base.endsWith('.example'))
  || /\.(?:key|pem|p12|pfx|crt)$/i.test(base);
}

async function policyAt(root){
 const policy=JSON.parse(await fs.readFile(path.join(root,'public-source.json'),'utf8'));
 const includeArt=policy.asset_policy==='maintainer-declared-ai-art-mit';
 if(policy.schema_version!==1||(!includeArt&&policy.asset_policy!=='character-art-excluded-pending-redistribution-rights'))throw new Error('Unsupported public source policy.');
 for(const name of [...policy.files,...policy.directories,...policy.exclude]){
  if(path.isAbsolute(name)||name.includes('\\')||name.split('/').some(p=>!p||p==='.'||p==='..'))throw new Error('Invalid relative public path.');
 }
 const excluded=new Set(policy.exclude),files=new Set(policy.files);
 const allowed=name=>!privatePath(name,includeArt)&&!excluded.has(name)&&(files.has(name)||policy.directories.some(dir=>name.startsWith(dir+'/')));
 if(includeArt)await validateArtProvenance(root,policy,allowed);
 return {policy,allowed,includeArt};
}

async function validateArtProvenance(root,policy,allowed){
 if(policy.art_provenance!=='assets/ART_PROVENANCE.json'||!policy.files.includes(policy.art_provenance))throw new Error('Art publication requires its provenance record in the public allowlist.');
 const record=JSON.parse(await fs.readFile(path.join(root,policy.art_provenance),'utf8'));
 if(record.schema_version!==1||record.license!=='MIT'||record.source_declaration?.type!=='maintainer-statement')throw new Error('Invalid maintainer art declaration.');
 const assets=JSON.parse(await fs.readFile(path.join(root,'assets/processed/game-assets.json'),'utf8'));
 const rows=[...Object.values(assets.animations),assets.effect,assets.background,assets.portrait];
 const sources=new Map(rows.map(row=>[row.source,row.source_sha256]));
 const sourcePaths=record.sources.map(row=>row.path).sort();
 if(JSON.stringify(sourcePaths)!==JSON.stringify([...sources.keys()].sort()))throw new Error('Art source inventory differs from the game manifest.');
 for(const row of record.sources){
  if(!allowed(row.path)||row.sha256!==sources.get(row.path))throw new Error('Undeclared or mismatched art source: '+row.path);
  if(sha(await fs.readFile(path.join(root,row.path)))!==row.sha256)throw new Error('Art source checksum differs: '+row.path);
 }
 const derived=new Map(rows.map(row=>['assets/'+row.texture,row]));
 if(JSON.stringify(record.derived_files.map(row=>row.path).sort())!==JSON.stringify([...derived.keys()].sort()))throw new Error('Derived art inventory differs from the game manifest.');
 for(const row of record.derived_files){
  const game=derived.get(row.path);
  if(!allowed(row.path)||row.source!==game.source||row.source_sha256!==game.source_sha256)throw new Error('Derived art mapping differs: '+row.path);
 }
}

async function walk(root,relative='',skipGit=false){
 const rows=[];
 for(const entry of await fs.readdir(path.join(root,relative),{withFileTypes:true})){
  if(skipGit&&relative===''&&entry.name==='.git')continue;
  const name=slash(path.join(relative,entry.name));
  if(entry.isSymbolicLink())throw new Error('Symlink is not allowed in a public source export: '+name);
  if(entry.isDirectory())rows.push(...await walk(root,name,skipGit));
  else if(entry.isFile())rows.push(name);
  else throw new Error('Non-regular public file: '+name);
 }
 return rows.sort();
}

async function selectedFiles(root,policy,allowed){
 const selected=new Set();
 for(const name of policy.files){
  const item=await fs.lstat(path.join(root,name));
  if(!item.isFile()||item.isSymbolicLink()||!allowed(name))throw new Error('Invalid required public file: '+name);
  selected.add(name);
 }
 for(const directory of policy.directories){
  const item=await fs.lstat(path.join(root,directory));
  if(!item.isDirectory()||item.isSymbolicLink())throw new Error('Invalid public source directory: '+directory);
  for(const name of await walk(root,directory))if(!policy.exclude.includes(name)){
   if(!allowed(name))throw new Error('Private file found inside an exported directory: '+name);
   selected.add(name);
  }
 }
 return [...selected].sort();
}

function git(root,args){return execFileSync('git',['-C',root,...args],{encoding:'utf8',maxBuffer:32*1024*1024});}

async function check(root,verifyManifest=false){
 const {policy,allowed,includeArt}=await policyAt(root);
 await selectedFiles(root,policy,allowed);
 const hasGit=await exists(path.join(root,'.git'));
 const inventory=hasGit?git(root,['ls-files','-z']).split('\0').filter(Boolean):await walk(root,'',true);
 const failures=[];
 for(const name of inventory){
  if(name!==MANIFEST&&!allowed(name)){failures.push('Not in public allowlist: '+name);continue;}
  if(!await exists(path.join(root,name))){failures.push('Tracked file is missing: '+name);continue;}
  const item=await fs.lstat(path.join(root,name));
  if(!item.isFile()||item.isSymbolicLink()){failures.push('Not a regular public file: '+name);continue;}
  const bytes=await fs.readFile(path.join(root,name));
  for(const finding of findPrivacyIssues(name,bytes))failures.push(finding.rule+' in '+name+(finding.line?':'+finding.line:'')+' (value withheld)');
 }
 if(hasGit){
  const historicalPaths=git(root,['rev-list','--objects','--all']).split('\n').flatMap(line=>{
   const space=line.indexOf(' ');return space<0||space===line.length-1?[]:[line.slice(space+1)];
  });
  for(const name of new Set(historicalPaths)){
   const parentOfPublic=policy.files.some(file=>file.startsWith(name+'/'))||policy.directories.some(dir=>dir===name||dir.startsWith(name+'/'));
   if(name!==MANIFEST&&!allowed(name)&&!parentOfPublic)failures.push('Non-public path remains in reachable Git history: '+name);
  }
 }
 if(verifyManifest){
  const record=JSON.parse(await fs.readFile(path.join(root,MANIFEST),'utf8'));
  if(record.schema_version!==1||record.asset_policy!==policy.asset_policy)failures.push('Invalid export manifest.');
  const actual=inventory.filter(name=>name!==MANIFEST).sort();
  if(JSON.stringify(actual)!==JSON.stringify(record.files.map(file=>file.path).sort()))failures.push('Export file inventory differs from its manifest.');
  for(const entry of record.files){
   if(!allowed(entry.path)){failures.push('Manifest path is not allowed: '+entry.path);continue;}
   if(sha(await fs.readFile(path.join(root,entry.path)))!==entry.sha256)failures.push('Export checksum differs: '+entry.path);
  }
 }
 if(failures.length){
  const historical=failures.filter(message=>message.startsWith('Non-public path remains in reachable Git history:'));
  const current=failures.filter(message=>!message.startsWith('Non-public path remains in reachable Git history:'));
  const sample=[...current.slice(0,10),...historical.slice(0,10)];
  throw new Error(sample.join('\n')+(failures.length>sample.length?'\n... '+failures.length+' total findings':''));
 }
 return {mode:includeArt?'project-with-ai-art':'source-only',files:inventory.length,historyChecked:hasGit,knownSecretSignatures:0,characterArtIncluded:includeArt};
}

async function exportSource(destination){
 if(await exists(destination))throw new Error('Output already exists; choose a new directory.');
 const relation=path.relative(destination,ROOT);
 if(destination===ROOT||(!relation.startsWith('..'+path.sep)&&relation!=='..'&&!path.isAbsolute(relation)))throw new Error('Output cannot contain or replace the working project.');
 const {policy,allowed}=await policyAt(ROOT);
 for(const directory of ['src','scripts','deploy','.github','web','assets','docs','packaging']){
  const input=path.join(ROOT,directory);
  if(destination===input||destination.startsWith(input+path.sep))throw new Error('Output must be outside source/input directories.');
 }
 const selected=await selectedFiles(ROOT,policy,allowed);
 const record={schema_version:1,asset_policy:policy.asset_policy,files:[]};
 await fs.mkdir(destination,{recursive:true});
 for(const name of selected){
  const source=path.join(ROOT,name),target=path.join(destination,name);
  const item=await fs.lstat(source);
  if(!item.isFile()||item.isSymbolicLink())throw new Error('Source changed during export: '+name);
  const input=await fs.readFile(source);
  // Canonical line endings keep the SHA256 manifest valid after Git checkout
  // on either OS. Original art/font bytes are copied exactly.
  const binary=/\.(?:png|webp|gif|otf|wasm|gz|zip)$/i.test(name)||input.includes(0);
  const bytes=binary?input:Buffer.from(input.toString('utf8').replace(/\r\n/g,'\n'));
  await fs.mkdir(path.dirname(target),{recursive:true});
  await fs.writeFile(target,bytes,{flag:'wx'});
  record.files.push({path:name,bytes:bytes.length,sha256:sha(bytes)});
 }
 await fs.writeFile(path.join(destination,MANIFEST),JSON.stringify(record,null,2)+'\n',{flag:'wx'});
 const result=await check(destination,true);
 return {...result,output:destination,bytes:record.files.reduce((sum,file)=>sum+file.bytes,0)};
}

async function refreshManifest(root){
 const {policy,allowed}=await policyAt(root),selected=await selectedFiles(root,policy,allowed);
 const record={schema_version:1,asset_policy:policy.asset_policy,files:[]};
 for(const name of selected){
  const bytes=await fs.readFile(path.join(root,name));
  if(findPrivacyIssues(name,bytes).length)throw new Error('Privacy check failed in '+name+' (value withheld)');
  record.files.push({path:name,bytes:bytes.length,sha256:sha(bytes)});
 }
 await fs.writeFile(path.join(root,MANIFEST),JSON.stringify(record,null,2)+'\n');
 return {manifest:MANIFEST,files:selected.length+1};
}

try{
 const {values,positionals}=parseArgs({allowPositionals:true,options:{out:{type:'string'},root:{type:'string'},'verify-manifest':{type:'boolean',default:false}}});
 if(positionals.length!==1||!['export','check','manifest'].includes(positionals[0]))throw new Error('Usage: public-source.mjs export --out NEW_DIRECTORY | check [--root DIRECTORY] [--verify-manifest] | manifest');
 if(positionals[0]==='manifest'){
  if(values.out||values['verify-manifest'])throw new Error('Manifest does not accept export/check options.');
  console.log(JSON.stringify(await refreshManifest(path.resolve(values.root??ROOT)),null,2));
 }else
 if(positionals[0]==='export'){
  if(!values.out||values.root||values['verify-manifest'])throw new Error('Export requires --out and accepts no other options.');
  console.log(JSON.stringify(await exportSource(path.resolve(values.out)),null,2));
 }else{
  if(values.out)throw new Error('Check does not accept --out.');
  console.log(JSON.stringify(await check(path.resolve(values.root??ROOT),values['verify-manifest']),null,2));
 }
}catch(error){console.error(error.message);process.exitCode=1;}
