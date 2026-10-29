// Build the complete game asset pack; never modify the original sheets.
import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {parseArgs} from 'node:util';
import {CELL,PIVOT,decodePng,findCharacters,renderFrame,bounds,encodeGif} from './sprite-codec.mjs';
import {decodeRgba,encodeWebp} from './webp-codec.mjs';
const ROOT=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const OUT=path.join(ROOT,'assets/processed');
const PREVIEWS=path.join(ROOT,'artifacts/previews');
const {values:{previews}}=parseArgs({options:{previews:{type:'boolean',default:false}}});
const PACK=[
 ['idle','fight-1.png'],['walk','n-1 (3).png'],['jump','fight-3.png'],
 ['guard','fight-4.png'],['hurt','fight-5.png'],['knockdown','fight-6.png'],
 ['getup','n-1 (2).png'],['light','fight.png'],['heavy','n-1 (1).png'],['dash','fight-8.png']
];
// Poses with no drawn sheet yet: stand-ins copied from another sheet, marked in the manifest.
// When the real sheet exists, move the name into PACK and delete it here; Rust does not change.
const PLACEHOLDERS=[['shot','light'],['air','dash']];
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const sheets=[];
for(const [name,file] of PACK){
 const source='assets/original/'+file,bytes=await fs.readFile(path.join(ROOT,source));
 const image=decodePng(bytes),characters=findCharacters(image);
 characters.forEach((c,i)=>{c.rootX=(i%4+0.5)*image.width/4;});
 sheets.push({name,source,sha256:hash(bytes),image,characters,mapping:name==='light'?[0,1,2,3,3,5,6,7]:[0,1,2,3,4,5,6,7]});
}
const all=sheets.flatMap(s=>s.mapping.map(i=>s.characters[i]));
const scale=Math.min(1,(PIVOT.x-22)/Math.max(...all.map(c=>c.rootX-c.cropX)),(CELL-PIVOT.x-22)/Math.max(...all.map(c=>c.cropX+c.cropW-c.rootX)),(PIVOT.y-22)/Math.max(...all.map(c=>c.rootY-c.cropY)));
assert(scale>0.65 && scale<=1,'Unexpected character scale');
// Game textures are lossless WebP; encodeWebp checks that the pixels survive the round trip.
function atlas(frames){const w=CELL*4,h=CELL*2,out=Buffer.alloc(w*h*4);frames.forEach((f,i)=>{for(let y=0;y<CELL;y++)f.copy(out,((Math.floor(i/4)*CELL+y)*w+(i%4)*CELL)*4,y*CELL*4,(y+1)*CELL*4);});return encodeWebp(w,h,out);}
await fs.mkdir(OUT,{recursive:true});
if(previews)await fs.mkdir(PREVIEWS,{recursive:true});
const manifest={schema_version:1,character:'奶蛙',cell:[CELL,CELL],layout:[4,2],pivot:PIVOT,bevy_anchor:[0,0.5-PIVOT.y/CELL],uniform_scale:scale,animations:{},notes:['All original files are preserved and SHA256 checked.','Character components are isolated before repacking, including poses that cross the original nominal grid.','Common scale and per-cell horizontal root; grounded feet aligned to the baseline.','Light attack holds original frame 4 instead of the hand-swapping frame 5.','Walk is an MVP gait, not a polished motion-captured alternating cycle.','Gameplay durations are defined in src/combat.rs; GIFs are only previews.','Textures are lossless WebP; the background keeps the original pixels.']};
for(const sheet of sheets){
 const frames=sheet.mapping.map(i=>renderFrame(sheet.characters[i],scale));
 sheet.frames=frames;
 const frameBounds=frames.map(f=>bounds(f,CELL,CELL));
 assert(frameBounds.every(b=>b.minX>=18&&b.maxX<CELL-18&&b.minY>=18&&b.maxY<CELL-18),'Clipping in '+sheet.name);
 assert(Math.max(...frameBounds.map(b=>b.maxY))-Math.min(...frameBounds.map(b=>b.maxY))<=1,'Baseline mismatch: '+sheet.name);
 const texture='processed/fighter_'+sheet.name+'.webp';
 await fs.writeFile(path.join(ROOT,'assets',texture),await atlas(frames));
 manifest.animations[sheet.name]={texture,source:sheet.source,source_sha256:sheet.sha256,source_frames:sheet.mapping.map(i=>i+1),bounds:frameBounds};
 if(previews&&['walk','getup','heavy','light'].includes(sheet.name))await fs.writeFile(path.join(PREVIEWS,'fighter_'+sheet.name+'.preview.gif'),encodeGif(frames,Array(8).fill(sheet.name==='walk'?6:5)));
 assert.equal(hash(await fs.readFile(path.join(ROOT,sheet.source))),sheet.sha256,'Original modified');
}
for(const [name,from] of PLACEHOLDERS){
 const sheet=sheets.find(s=>s.name===from),texture='processed/fighter_'+name+'.webp';
 await fs.writeFile(path.join(ROOT,'assets',texture),await atlas(sheet.frames));
 manifest.animations[name]={...manifest.animations[from],texture,placeholder:true,placeholder_of:from};
}
const fxSource='assets/original/fight-boom.png',fxBytes=await fs.readFile(path.join(ROOT,fxSource)),fx=decodePng(fxBytes);
const effectFrames=[];
for(let i=0;i<8;i++){
 const cropX=Math.floor(i%4*fx.width/4),cropY=Math.floor(Math.floor(i/4)*fx.height/2),cropW=Math.floor((i%4+1)*fx.width/4)-cropX,cropH=Math.floor((Math.floor(i/4)+1)*fx.height/2)-cropY,clean=Buffer.alloc(cropW*cropH*4);
 for(let y=0;y<cropH;y++)for(let x=0;x<cropW;x++){const a=((cropY+y)*fx.width+cropX+x)*4,d=(y*cropW+x)*4;if(fx.pixels[a+3]>=8)fx.pixels.copy(clean,d,a,a+4);}
 effectFrames.push(renderFrame({cropX,cropY,cropW,cropH,clean,rootX:(i%4+0.5)*fx.width/4,rootY:(Math.floor(i/4)+0.5)*fx.height/2},(CELL-32)/(fx.width/4),{x:256,y:256}));
}
await fs.writeFile(path.join(OUT,'fx_hit.webp'),await atlas(effectFrames));
manifest.effect={texture:'processed/fx_hit.webp',source:fxSource,source_sha256:hash(fxBytes),pivot:[256,256],alpha_threshold:8};
// Milk bubble, drawn here (no original): frames 0-3 fly to the right with a wobble and a
// trail of drops, frames 4-7 burst into a fading ring of drops.
const bubbleFrames=[];
for(let i=0;i<8;i++){
 const f=Buffer.alloc(CELL*CELL*4),c=CELL/2,wobble=Math.sin(i*Math.PI/2);
 const blobs=[];
 if(i<4){
  blobs.push({x:c,y:c,rx:135*(1+0.05*wobble),ry:135*(1-0.05*wobble),a:0.8,rim:true});
  [[-166,10,18],[-198,-12,12],[-220,6,8]].forEach(([dx,dy,r],k)=>blobs.push({x:c+dx+4*Math.sin(i*Math.PI/2+k),y:c+dy,rx:r,ry:r,a:0.75}));
 }else{
  const t=(i-3)/4;
  if(i===4)blobs.push({x:c,y:c,rx:125,ry:125,a:0.55,rim:true});
  for(let k=0;k<10;k++){const ang=k*Math.PI/5+0.3,R=60+150*t,r=26*(1-0.6*t);blobs.push({x:c+R*Math.cos(ang),y:c+R*Math.sin(ang),rx:r,ry:r,a:1-0.6*t,rim:true});}
 }
 for(let y=0;y<CELL;y++)for(let x=0;x<CELL;x++){
  let alpha=0,shade=1,rim=0;
  for(const b of blobs){
   const d=Math.hypot((x+0.5-b.x)/b.rx,(y+0.5-b.y)/b.ry);
   const edge=Math.min(1,Math.max(0,(1-d)*Math.min(b.rx,b.ry)/1.5));
   if(edge<=0)continue;
   let a=b.a*edge;
   if(b.rim){a=Math.min(1,a+0.15*d*d);rim=Math.max(rim,d>0.9?edge:0);shade=1-0.12*((y+0.5-b.y)/b.ry+1)/2;}
   alpha=Math.max(alpha,a);
  }
  if(alpha<=0)continue;
  const hl=i<4&&Math.hypot(x+0.5-(c-50),y+0.5-(c-54))<30;
  const o=(y*CELL+x)*4;
  const rgb=hl?[255,255,255]:rim?[226,204,160]:[255*shade,250*shade,236*shade];
  f[o]=Math.round(rgb[0]);f[o+1]=Math.round(rgb[1]);f[o+2]=Math.round(rgb[2]);f[o+3]=Math.round(255*(hl?1:alpha));
 }
 const b=bounds(f,CELL,CELL);
 assert(b.minX>=18&&b.maxX<CELL-18&&b.minY>=18&&b.maxY<CELL-18,'Clipping in bubble frame '+i);
 bubbleFrames.push(f);
}
await fs.writeFile(path.join(OUT,'fx_shot.webp'),await atlas(bubbleFrames));
manifest.shot={texture:'processed/fx_shot.webp',generated:'scripts/prepare-game-assets.mjs',frames:{fly:[0,1,2,3],burst:[4,5,6,7]},body_diameter_px:270};
const stageSource='assets/original/bg.png',stageBytes=await fs.readFile(path.join(ROOT,stageSource)),stage=await decodeRgba(stageBytes);
await fs.writeFile(path.join(OUT,'stage_garden.webp'),await encodeWebp(stage.width,stage.height,stage.pixels));
manifest.background={texture:'processed/stage_garden.webp',source:stageSource,source_sha256:hash(stageBytes)};
const sw=160,sh=48,shadow=Buffer.alloc(sw*sh*4);
for(let y=0;y<sh;y++)for(let x=0;x<sw;x++){const r=((x+0.5-sw/2)/(sw/2))**2+((y+0.5-sh/2)/(sh/2))**2;shadow[(y*sw+x)*4+3]=r<1?Math.round(125*(1-r)**2):0;}
await fs.writeFile(path.join(OUT,'shadow.webp'),await encodeWebp(sw,sh,shadow));
manifest.shadow={texture:'processed/shadow.webp',generated:'scripts/prepare-game-assets.mjs',size:[sw,sh]};
// HUD portrait: idle frame 1 cropped to the character and fitted into a small square
// (no new art). The plate draws it with a seat-colored border; out-of-frame reads stay
// transparent, so the padding never smears edge pixels.
{
 const idle=sheets.find(s=>s.name==='idle'),f=idle.frames[0],b=bounds(f,CELL,CELL);
 const P=128,content=Math.max(b.maxX-b.minX+1,b.maxY-b.minY+1),box=Math.round(content*1.28);
 const cx=Math.round((b.minX+b.maxX+1)/2),left=cx-(box>>1),top=b.maxY+Math.round(box*0.06)-box;
 const portrait=Buffer.alloc(P*P*4);
 for(let y=0;y<P;y++)for(let x=0;x<P;x++){
  const sx=left+Math.floor((x+0.5)*box/P),sy=top+Math.floor((y+0.5)*box/P);
  if(sx<0||sx>=CELL||sy<0||sy>=CELL)continue;
  f.copy(portrait,(y*P+x)*4,(sy*CELL+sx)*4,(sy*CELL+sx+1)*4);
 }
 assert(portrait.some((_,i)=>i%4===3&&portrait[i]>0),'Empty portrait');
 assert([0,P-1,P*(P-1),P*P-1].every(i=>portrait[i*4+3]===0),'Portrait corners must stay transparent');
 await fs.writeFile(path.join(OUT,'portrait.webp'),await encodeWebp(P,P,portrait));
 manifest.portrait={texture:'processed/portrait.webp',source:idle.source,source_sha256:idle.sha256,frame:1,size:P,generated:'scripts/prepare-game-assets.mjs'};
}
await fs.writeFile(path.join(OUT,'game-assets.json'),JSON.stringify(manifest,null,2)+'\n');
assert.equal(hash(await fs.readFile(path.join(ROOT,fxSource))),hash(fxBytes));
assert.equal(hash(await fs.readFile(path.join(ROOT,stageSource))),hash(stageBytes));
console.log(JSON.stringify({animations:PACK.length,placeholders:PLACEHOLDERS.map(p=>p[0]),scale,cell:CELL,pivot:PIVOT,originalsPreserved:true,manifest:'assets/processed/game-assets.json'},null,2));
