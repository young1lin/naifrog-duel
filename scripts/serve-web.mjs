import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import {fileURLToPath} from 'node:url';
const ROOT=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
// OFFLINE debug server: no login, no multiplayer, never expose publicly. Production is naifrog-server.
export const PAGE_FILES=['index.html','main.js','network.js','style.css'];
const TYPES={'.html':'text/html; charset=utf-8','.css':'text/css; charset=utf-8','.js':'text/javascript; charset=utf-8','.wasm':'application/wasm','.webp':'image/webp','.png':'image/png','.gif':'image/gif','.json':'application/json','.otf':'font/otf'};
export function requestPath(raw,root=ROOT){
 let name;try{name=decodeURIComponent(raw.split('?')[0]);}catch{return null;}
 if(name.includes('\\')||name.split('/').some(s=>s==='.'||s==='..'))return null;
 if(name==='/'||name==='/index.html')return path.join(root,'web/index.html');
 if(['/main.js','/network.js','/style.css'].includes(name))return path.join(root,'web',name.slice(1));
 if(/^\/pkg\/(?:[a-zA-Z0-9_.-]+\/)*[a-zA-Z0-9_.-]+$/.test(name))return path.join(root,'web',name.slice(1));
 if(/^\/assets\/processed\/[a-zA-Z0-9_.-]+$/.test(name))return path.join(root,name.slice(1));
 return null;
}
export function createHandler(root=ROOT){
return function handler(req,res){
 if(!['GET','HEAD'].includes(req.method)){res.writeHead(405,{Allow:'GET, HEAD'}).end();return;}
 const file=requestPath(req.url??'/',root);if(!file){res.writeHead(404,{'X-Naifrog-Server':'offline-static'}).end('Not found');return;}
 fs.stat(file,(error,stat)=>{if(error||!stat.isFile()){res.writeHead(404,{'X-Naifrog-Server':'offline-static'}).end('Not found. Build web/pkg first using scripts/build-web.ps1.');return;}
 res.writeHead(200,{'Content-Type':TYPES[path.extname(file)]??'application/octet-stream','Content-Length':stat.size,'Cache-Control':'no-store','X-Content-Type-Options':'nosniff','X-Naifrog-Server':'offline-static'});if(req.method==='HEAD'){res.end();return;}fs.createReadStream(file).on('error',()=>res.destroy()).pipe(res);});
}
}
export const handler=createHandler();
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
 const arg=(key,fallback)=>{const i=process.argv.indexOf(key);return i<0?fallback:process.argv[i+1];};
 const port=Number(arg('--port','18080')),host=arg('--host','127.0.0.1');
 if(!Number.isInteger(port)||port<1||port>65535)throw new Error('Invalid port');
 const missing=PAGE_FILES.filter(f=>!fs.statSync(path.join(ROOT,'web',f),{throwIfNoEntry:false})?.isFile());
 if(missing.length)throw new Error('Missing page files in '+path.join(ROOT,'web')+': '+missing.join(', '));
 if(!fs.existsSync(path.join(ROOT,'web/pkg/naifrog_duel_bg.wasm')))console.warn('web/pkg is not built yet: run .\scripts\build-web.ps1 -Release');
 const server=http.createServer(handler);server.on('error',e=>{console.error(e);process.exitCode=1;});
 server.listen(port,host,()=>{console.log('Game: http://127.0.0.1:'+port);if(host==='0.0.0.0'){for(const addresses of Object.values(os.networkInterfaces()))for(const a of addresses??[])if(a.family==='IPv4'&&!a.internal)console.log('Tablet LAN: http://'+a.address+':'+port);}console.log('Offline static server (local play only, no login/online). Root: '+ROOT);console.log('Serving only web files and processed game assets. Ctrl+C to stop.');});
}
