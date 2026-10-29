import fs from 'node:fs/promises';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {parseArgs} from 'node:util';
const ROOT=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
export function validVersion(value){
 if(!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/.test(value))return false;
 return !value.split('-').slice(1).join('-').split('.').some(x=>/^\d+$/.test(x)&&x.length>1&&x[0]==='0');
}
export async function readVersion(root=ROOT){
 const toml=await fs.readFile(path.join(root,'Cargo.toml'),'utf8'),lock=await fs.readFile(path.join(root,'Cargo.lock'),'utf8');
 const version=toml.match(/\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m)?.[1];
 const locked=lock.match(/\[\[package\]\]\s+name\s*=\s*"naifrog-duel"\s+version\s*=\s*"([^"]+)"/)?.[1];
 if(!validVersion(version??'')||version!==locked)throw new Error('Cargo.toml and Cargo.lock must contain the same valid project version.');
 return version;
}
export async function releaseNotes(root,version){
 const text=await fs.readFile(path.join(root,'CHANGELOG.md'),'utf8'),heading='## ['+version+']';
 const start=text.indexOf(heading);if(start<0)throw new Error('CHANGELOG.md has no entry for '+version);
 const body=text.slice(start+heading.length).replace(/^[^\n]*\n/,'').split(/\n## \[/)[0].trim();
 if(!body||/\bTODO\b/.test(body))throw new Error('Complete the release notes before tagging '+version);
 return body+'\n';
}
export async function checkVersion(root=ROOT,tag=''){
 const version=await readVersion(root);
 await releaseNotes(root,version);
 if(tag&&tag!=='v'+version)throw new Error('Release tag must be v'+version+'; received '+tag);
 return version;
}
export function metadata(version,sha,tag=''){
 if(!validVersion(version)||!/^[a-f0-9]{40}$/.test(sha))throw new Error('Invalid build version or commit.');
 if(tag&&tag!=='v'+version)throw new Error('Tag/version mismatch.');
 return {version,package_version:tag?version:version+'-dev.'+sha.slice(0,7),source_commit:sha,prerelease:version.includes('-')?'true':'false'};
}
export async function bumpVersion(root,kind,date=new Date().toISOString().slice(0,10)){
 if(!['patch','minor','major'].includes(kind))throw new Error('Choose patch, minor or major.');
 const old=await readVersion(root),parts=old.split('-')[0].split('.').map(Number);
 if(kind==='major'){parts[0]++;parts[1]=parts[2]=0;}else if(kind==='minor'){parts[1]++;parts[2]=0;}else parts[2]++;
 const version=parts.join('.'),tomlPath=path.join(root,'Cargo.toml'),lockPath=path.join(root,'Cargo.lock'),logPath=path.join(root,'CHANGELOG.md');
 const toml=await fs.readFile(tomlPath,'utf8'),lock=await fs.readFile(lockPath,'utf8'),log=await fs.readFile(logPath,'utf8');
 const unreleased=log.match(/(## \[Unreleased\][^\n]*\n)([\s\S]*?)(?=\n## \[|$)/);
 if(!unreleased)throw new Error('CHANGELOG.md needs an [Unreleased] section.');
 const notes=unreleased[2].trim()||'- TODO: document this release / 补充本次变更。';
 await fs.writeFile(tomlPath,toml.replace(/(\[package\][\s\S]*?^version\s*=\s*")[^"]+/m,'$1'+version));
 await fs.writeFile(lockPath,lock.replace(/(\[\[package\]\]\s+name\s*=\s*"naifrog-duel"\s+version\s*=\s*")[^"]+/,'$1'+version));
 await fs.writeFile(logPath,log.replace(unreleased[0],unreleased[1]+'\n## ['+version+'] - '+date+'\n\n'+notes+'\n'));
 return version;
}
async function main(){
 const {positionals,values}=parseArgs({allowPositionals:true,options:{tag:{type:'string'},root:{type:'string'}}});
 const root=path.resolve(values.root??ROOT),command=positionals[0]??'check',tag=values.tag??process.env.RELEASE_TAG??'';
 if(command==='bump'){console.log(await bumpVersion(root,positionals[1]));return;}
 if(command==='print'){console.log(await readVersion(root));return;}
 if(command==='notes'){process.stdout.write(await releaseNotes(root,await checkVersion(root,tag)));return;}
 const version=await checkVersion(root,tag);
 if(command==='check'){console.log('Version OK: '+version);return;}
 if(command!=='metadata')throw new Error('Usage: version.mjs check|metadata|notes|bump patch|minor|major [--tag vX.Y.Z]');
 const sha=execFileSync('git',['-C',root,'rev-parse','HEAD'],{encoding:'utf8'}).trim(),record=metadata(version,sha,tag);
 if(process.env.GITHUB_OUTPUT)await fs.appendFile(process.env.GITHUB_OUTPUT,Object.entries(record).map(([key,value])=>key+'='+value+'\n').join(''));
 console.log(JSON.stringify(record,null,2));
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url))main().catch(error=>{console.error(error.message);process.exitCode=1;});
