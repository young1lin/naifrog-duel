import fs from 'node:fs/promises';import path from 'node:path';import crypto from 'node:crypto';import assert from 'node:assert/strict';import {fileURLToPath} from 'node:url';import {bounds,CELL} from './sprite-codec.mjs';import {decodeRgba} from './webp-codec.mjs';import * as hb from 'harfbuzzjs';
const ROOT=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');const m=JSON.parse(await fs.readFile(path.join(ROOT,'assets/processed/game-assets.json'),'utf8'));const sha=b=>crypto.createHash('sha256').update(b).digest('hex');assert.equal(m.character,'奶蛙');assert.equal(Object.keys(m.animations).length,12);
for(const [name,a]of Object.entries(m.animations)){if(a.placeholder)assert(m.animations[a.placeholder_of]&&!m.animations[a.placeholder_of].placeholder,'Placeholder without a real sheet: '+name);assert.equal(sha(await fs.readFile(path.join(ROOT,a.source))),a.source_sha256,'Source changed: '+name);assert(a.texture.endsWith('.webp'),'Not a WebP texture: '+name);const p=await decodeRgba(await fs.readFile(path.join(ROOT,'assets',a.texture)));assert.equal(p.width,2048);assert.equal(p.height,1024);const bottom=[];for(let i=0;i<8;i++){const f=Buffer.alloc(CELL*CELL*4);for(let y=0;y<CELL;y++)p.pixels.copy(f,y*CELL*4,((Math.floor(i/4)*CELL+y)*p.width+i%4*CELL)*4,((Math.floor(i/4)*CELL+y)*p.width+i%4*CELL+CELL)*4);const b=bounds(f,CELL,CELL);assert(b.minX>=18&&b.maxX<494&&b.minY>=18&&b.maxY<494,'Unsafe padding: '+name);bottom.push(b.maxY);assert.equal(f[3],0);}assert(Math.max(...bottom)-Math.min(...bottom)<=1,'Baseline jitter: '+name);}
for(const a of [m.effect,m.background])assert.equal(sha(await fs.readFile(path.join(ROOT,a.source))),a.source_sha256);
// Check the runtime effect, not just its original: every atlas cell must decode,
// contain visible pixels and retain transparent padding.
{
 const p=await decodeRgba(await fs.readFile(path.join(ROOT,'assets',m.effect.texture)));
 assert.equal(p.width,2048,'Hit effect width');assert.equal(p.height,1024,'Hit effect height');
 for(let i=0;i<8;i++){
  const f=Buffer.alloc(CELL*CELL*4);
  for(let y=0;y<CELL;y++)p.pixels.copy(f,y*CELL*4,((Math.floor(i/4)*CELL+y)*p.width+i%4*CELL)*4,((Math.floor(i/4)*CELL+y)*p.width+i%4*CELL+CELL)*4);
  const b=bounds(f,CELL,CELL);
  assert(b.maxX>=b.minX&&b.maxY>=b.minY,'Empty hit effect frame '+i);
  assert(b.minX>=18&&b.maxX<494&&b.minY>=18&&b.maxY<494,'Unsafe hit effect padding: '+i);
  assert.equal(f[3],0,'Hit effect corner must be transparent: '+i);
 }
}
assert(m.shadow,'Shadow missing from the asset manifest');
{
 const p=await decodeRgba(await fs.readFile(path.join(ROOT,'assets',m.shadow.texture)));
 assert.deepEqual(m.shadow.size,[160,48],'Shadow manifest size');
 assert.equal(p.width,m.shadow.size[0],'Shadow width');assert.equal(p.height,m.shadow.size[1],'Shadow height');
 assert(p.pixels.some((_,i)=>i%4===3&&p.pixels[i]>0),'Empty shadow');
 assert([0,p.width-1,p.width*(p.height-1),p.width*p.height-1].every(i=>p.pixels[i*4+3]===0),'Shadow corners must stay transparent');
}
// The background is the original re-encoded: same size, same pixels.
{const bg=await decodeRgba(await fs.readFile(path.join(ROOT,'assets',m.background.texture))),source=await decodeRgba(await fs.readFile(path.join(ROOT,m.background.source)));assert.equal(bg.width,source.width);assert.equal(bg.height,source.height);assert(bg.pixels.equals(source.pixels),'Background pixels differ from the original');}
{const p=await decodeRgba(await fs.readFile(path.join(ROOT,'assets',m.shot.texture)));assert.equal(p.width,2048);assert.equal(p.height,1024);for(let i=0;i<8;i++){const f=Buffer.alloc(CELL*CELL*4);for(let y=0;y<CELL;y++)p.pixels.copy(f,y*CELL*4,((Math.floor(i/4)*CELL+y)*p.width+i%4*CELL)*4,((Math.floor(i/4)*CELL+y)*p.width+i%4*CELL+CELL)*4);const b=bounds(f,CELL,CELL);assert(b.maxX>b.minX&&b.minX>=18&&b.maxX<494&&b.minY>=18&&b.maxY<494,'Unsafe bubble frame '+i);assert.equal(f[3],0);}}
const stand=Object.entries(m.animations).filter(([,a])=>a.placeholder).map(([n,a])=>n+'<-'+a.placeholder_of).join(', ');
// The HUD portrait is a crop of the idle sheet's first frame: square, transparent
// corners, non-empty, and tracking the same original as the sheet it was cut from.
{
 const p=await decodeRgba(await fs.readFile(path.join(ROOT,'assets',m.portrait.texture)));
 assert.equal(p.width,m.portrait.size);assert.equal(p.height,m.portrait.size);
 assert([0,p.width-1,p.width*(p.height-1),p.width*p.height-1].every(i=>p.pixels[i*4+3]===0),'Portrait corners must stay transparent');
 assert(p.pixels.some((_,i)=>i%4===3&&p.pixels[i]>0),'Empty portrait');
 assert.equal(m.portrait.source,m.animations.idle.source,'Portrait must track the idle sheet');
 assert.equal(m.portrait.source_sha256,m.animations.idle.source_sha256,'Portrait source hash must match the idle sheet');
}
// The name font draws printable ASCII, every character of src/name_han.txt (the list
// net::valid_name accepts) and the punctuation of the game's Chinese texts (characters.extra, the
// ranges of i18n::FONT_EXTRA); all come from prepare-name-font.mjs and must not drift apart.
const nameFont=JSON.parse(await fs.readFile(path.join(ROOT,'assets/processed/name_font.json'),'utf8'));
{
 const f=nameFont,font=await fs.readFile(path.join(ROOT,'assets',f.font));
 assert.equal(sha(font),f.sha256,'Name font changed: rerun scripts/prepare-name-font.mjs');
 const han=[...(await fs.readFile(path.join(ROOT,f.characters.list),'utf8')).replace(/\s/g,'')].map(c=>c.codePointAt(0));
 assert.equal(han.length,f.characters.han);assert(han.every((c,i)=>c>0x7f&&(i===0||c>han[i-1])),'Name characters must be sorted, unique and non-ASCII');
 assert.equal(sha(String.fromCodePoint(...han)),f.characters.han_sha256,'Name list changed: rerun scripts/prepare-name-font.mjs');
 const drawn=new Set(new hb.Face(new hb.Blob(font)).collectUnicodes());
 for(let c=0x20;c<0x7f;c++)assert(drawn.has(c),'Name font lacks ASCII '+c);
 for(const c of han)assert(drawn.has(c),'Name font lacks '+String.fromCodePoint(c));
 let extra=0;
 for(const range of f.characters.extra.split(' ')){
  const [lo,hi=lo]=range.split('..').map(r=>parseInt(r.replace(/^U\+/,''),16));
  for(let c=lo;c<=hi;c++,extra++)assert(drawn.has(c),'Name font lacks U+'+c.toString(16).toUpperCase());
 }
 assert.equal(extra,f.characters.extra_count,'Punctuation count');
 assert((await fs.readFile(path.join(ROOT,'assets',f.license_file),'utf8')).includes('SIL OPEN FONT LICENSE Version 1.1'),'Name font license missing');
}
// The published directory contains runtime inputs only. Previews and historical
// exports belong in artifacts/ or the local archive, never in the server catalog.
const runtimeFiles=new Set([
 ...Object.values(m.animations).map(a=>a.texture),
 ...[m.effect,m.background,m.shot,m.shadow,m.portrait].map(a=>a.texture),
 'processed/game-assets.json','processed/name_font.json',nameFont.font,nameFont.license_file
].map(p=>p.replace(/^processed\//,'')));
for(const entry of await fs.readdir(path.join(ROOT,'assets/processed'),{withFileTypes:true})){
 assert(entry.isFile()&&runtimeFiles.has(entry.name),'Non-runtime asset in assets/processed: '+entry.name+'; move previews to artifacts/');
}
console.log('PASS: twelve 8-frame transparent atlases ('+(stand?'placeholders: '+stand:'no placeholders')+'), baseline/padding, original hashes, hit effect, shadow, background pixels, bubble atlas, HUD portrait; lossless WebP; name font (ASCII + '+nameFont.characters.han+' Han + '+nameFont.characters.extra_count+' punctuation, OFL); runtime-only published directory.');
