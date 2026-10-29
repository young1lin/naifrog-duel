import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {parseArgs} from 'node:util';
import {zipSync,unzipSync} from 'fflate';
import {findPrivacyIssues} from './privacy-rules.mjs';
import {readVersion,validVersion} from './version.mjs';
const ROOT=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const webFiles=['web/index.html','web/main.js','web/network.js','web/style.css','web/pkg/naifrog_duel.js','web/pkg/naifrog_duel_bg.wasm','web/pkg/naifrog_duel_bg.wasm.gz'];
function safeName(name){
 if(!name||path.isAbsolute(name)||name.includes('\\')||name.split('/').some(x=>!x||x==='.'||x==='..'))throw new Error('Unsafe package path.');
 return name;
}
async function inventory(root,relative=''){
 const names=[];
 for(const item of await fs.readdir(path.join(root,relative),{withFileTypes:true})){
  const name=path.posix.join(relative,item.name);
  if(item.isSymbolicLink())throw new Error('Package symlinks are forbidden.');
  if(item.isDirectory())names.push(...await inventory(root,name));
  else if(item.isFile())names.push(name);
  else throw new Error('Unsupported package entry.');
 }
 return names.sort();
}
async function put(root,name,bytes){
 safeName(name);const at=path.join(root,name);await fs.mkdir(path.dirname(at),{recursive:true});await fs.writeFile(at,bytes);
}
async function copy(root,from,output,name=from){
 const source=path.join(root,from),item=await fs.lstat(source);
 if(!item.isFile()||item.isSymbolicLink())throw new Error('Missing or invalid package input: '+from);
 await put(output,name,await fs.readFile(source));
}
function noPrivateFile(name){
 if(/^(?:assets\/original|src|target|archive|artifacts|blender|\.git|\.agents)(?:\/|$)/.test(name)||/\.pdb$/i.test(name)||/(?:^|\/)\.env$/.test(name))throw new Error('Private/development file entered package: '+name);
}
export async function verifyBundle(folder){
 const record=JSON.parse(await fs.readFile(path.join(folder,'release-manifest.json'),'utf8'));
 const actual=(await inventory(folder)).filter(x=>x!=='release-manifest.json');
 if(JSON.stringify(actual)!==JSON.stringify(record.files.map(x=>x.path).sort()))throw new Error('Package inventory differs from its manifest.');
 for(const entry of record.files){
  safeName(entry.path);noPrivateFile(entry.path);
  const bytes=await fs.readFile(path.join(folder,entry.path));
  if(hash(bytes)!==entry.sha256)throw new Error('Package checksum mismatch: '+entry.path);
  const findings=findPrivacyIssues(entry.path,bytes);
  if(findings.length)throw new Error('Package privacy check: '+findings[0].rule+' in '+entry.path+' (value withheld)');
 }
 return record;
}
async function importWeb(archive,output,version,commit){
 const bytes=await fs.readFile(archive),expected=(await fs.readFile(archive+'.sha256','utf8')).split(/\s+/)[0];
 if(hash(bytes)!==expected)throw new Error('Web archive checksum mismatch.');
 const entries=unzipSync(bytes),manifests=Object.keys(entries).filter(x=>x.endsWith('/release-manifest.json'));
 if(manifests.length!==1)throw new Error('Web archive needs exactly one release manifest.');
 const manifestName=manifests[0],prefix=manifestName.slice(0,-'release-manifest.json'.length),record=JSON.parse(Buffer.from(entries[manifestName]).toString('utf8'));
 if(record.kind!=='web'||record.version!==version||record.source_commit!==commit)throw new Error('Server and web build version/commit differ.');
 for(const name of webFiles){
  const entry=record.files.find(x=>x.path===name),content=entries[prefix+name];
  if(!entry||!content||hash(content)!==entry.sha256)throw new Error('Missing or corrupt web runtime file: '+name);
  await put(output,name,content);
 }
}
export async function packageRelease({root=ROOT,kind,target,version,commit,webArchive,out=path.join(root,'dist'),archive=true}){
 if(!['desktop','web','server'].includes(kind)||!validVersion(version)||!/^[0-9a-f]{40}$/.test(commit))throw new Error('Invalid package kind, version or commit.');
 const expectedTarget=kind==='web'?'wasm32-unknown-unknown':target;
 if(kind==='desktop'&&target!=='x86_64-pc-windows-msvc')throw new Error('Desktop package needs the Windows MSVC target.');
 if(kind==='server'&&!['x86_64-pc-windows-msvc','x86_64-unknown-linux-musl'].includes(target))throw new Error('Unsupported server target.');
 const platform=kind==='web'?'':target.includes('windows')?'windows-x86_64':'linux-x86_64';
 const base='naifrog-'+(kind==='desktop'?'desktop':kind)+'-v'+version+(platform?'-'+platform:'');
 const folder=path.join(out,base);await fs.mkdir(out,{recursive:true});await fs.mkdir(folder);
 const policy=JSON.parse(await fs.readFile(path.join(root,'public-source.json'),'utf8'));
 for(const name of policy.files.filter(x=>x.startsWith('assets/processed/')))await copy(root,name,folder);
 for(const name of ['LICENSE','NOTICE.md','assets/UNICODE-LICENSE.txt','assets/ART_PROVENANCE.json','assets/README.md'])await copy(root,name,folder);
 for(const [language,name] of [['zh','README.md'],['en','README.en.md']]){
  let text=await fs.readFile(path.join(root,'packaging','README.'+kind+'.'+language+'.md'),'utf8');
  text=text.replaceAll('{{VERSION}}',version);await put(folder,name,Buffer.from(text));
 }
 if(kind==='desktop')await copy(root,'target/'+target+'/release/naifrog-duel.exe',folder,'naifrog-duel.exe');
 if(kind==='web'){
  for(const name of webFiles)await copy(root,name,folder);
  await copy(root,'scripts/serve-web.mjs',folder);
 }
 if(kind==='server'){
  if(!webArchive)throw new Error('A complete server package requires the matching web archive.');
  await importWeb(webArchive,folder,version,commit);
  const windows=target.includes('windows'),binary='naifrog-server'+(windows?'.exe':'');
  await copy(root,'target/'+target+'/release/'+binary,folder,binary);
  await copy(root,'packaging/start-server.'+(windows?'ps1':'sh'),folder,'start-server.'+(windows?'ps1':'sh'));
  await copy(root,'DEPLOY.md',folder);
  if(!windows){await fs.chmod(path.join(folder,binary),0o755);await fs.chmod(path.join(folder,'start-server.sh'),0o755);}
 }
 const record={schema_version:1,kind,version,source_version:await readVersion(root),source_commit:commit,target:expectedTarget,files:[]};
 for(const name of await inventory(folder)){
  noPrivateFile(name);const bytes=await fs.readFile(path.join(folder,name));
  record.files.push({path:name,bytes:bytes.length,sha256:hash(bytes)});
 }
 await put(folder,'release-manifest.json',Buffer.from(JSON.stringify(record,null,2)+'\n'));
 await verifyBundle(folder);
 if(!archive)return {folder,record};
 const archivePath=folder+(platform==='linux-x86_64'?'.tar.gz':'.zip');
 if(platform==='linux-x86_64')execFileSync('tar',['-czf',archivePath,'-C',out,base]);
 else{
  const files={};for(const name of await inventory(folder))files[base+'/'+name]=new Uint8Array(await fs.readFile(path.join(folder,name)));
  await fs.writeFile(archivePath,zipSync(files,{level:6}));
 }
 const checksum=hash(await fs.readFile(archivePath));
 await fs.writeFile(archivePath+'.sha256',checksum+'  '+path.basename(archivePath)+'\n');
 return {folder,archive:archivePath,sha256:checksum};
}
async function main(){
 const {values}=parseArgs({options:{kind:{type:'string'},target:{type:'string'},version:{type:'string'},commit:{type:'string'},'web-archive':{type:'string'},out:{type:'string'}}});
 const version=values.version??process.env.PACKAGE_VERSION,commit=values.commit??execFileSync('git',['rev-parse','HEAD'],{cwd:ROOT,encoding:'utf8'}).trim();
 console.log(JSON.stringify(await packageRelease({kind:values.kind,target:values.target,version,commit,webArchive:values['web-archive'],...(values.out?{out:path.resolve(values.out)}:{})}),null,2));
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url))main().catch(error=>{console.error(error.message);process.exitCode=1;});
