import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {checkVersion,releaseNotes} from './version.mjs';
const root=process.cwd(),tag=process.env.RELEASE_TAG,repo=process.env.GH_REPO;
if(!repo||!tag)throw new Error('GH_REPO and RELEASE_TAG are required.');
const version=await checkVersion(root,tag),directory=path.resolve(process.argv[2]??'release-files');
const files=(await fs.readdir(directory)).filter(x=>/\.(?:zip|tar\.gz|sha256)$/.test(x)).sort();
const archives=files.filter(x=>!x.endsWith('.sha256'));
if(archives.length!==4||files.length!==8)throw new Error('Expected desktop, web, Linux server and Windows server archives with checksums.');
for(const file of archives){
 const bytes=await fs.readFile(path.join(directory,file)),expected=(await fs.readFile(path.join(directory,file+'.sha256'),'utf8')).split(/\s+/)[0];
 if(crypto.createHash('sha256').update(bytes).digest('hex')!==expected)throw new Error('Release archive checksum mismatch: '+file);
}
const gh=args=>execFileSync('gh',args,{encoding:'utf8',stdio:['ignore','pipe','pipe']});
let existing;try{existing=JSON.parse(gh(['release','view',tag,'--repo',repo,'--json','isDraft,tagName,url']));}catch{}
const notesFile=path.join(directory,'release-notes.md');await fs.writeFile(notesFile,await releaseNotes(root,version));
const assets=files.map(file=>path.join(directory,file));
if(existing){
 if(!existing.isDraft)throw new Error('Refusing to modify a published release; use a new version.');
 gh(['release','upload',tag,'--repo',repo,'--clobber',...assets]);
 gh(['release','edit',tag,'--repo',repo,'--draft','--notes-file',notesFile]);
}else{
 const args=['release','create',tag,'--repo',repo,'--draft','--verify-tag','--title','Naifrog Duel '+tag,'--notes-file',notesFile,...assets];
 if(version.includes('-'))args.push('--prerelease');gh(args);
}
const result=JSON.parse(gh(['release','view',tag,'--repo',repo,'--json','url,isDraft,assets']));
if(!result.isDraft||result.assets.length!==8)throw new Error('Draft release verification failed.');
console.log(result.url);
if(process.env.GITHUB_STEP_SUMMARY)await fs.appendFile(process.env.GITHUB_STEP_SUMMARY,'Draft release: '+result.url+'\n\nFour platform packages and four SHA256 files uploaded.\n');
