// Build the canvas's Chinese font. Player names may use the 8105 characters of 通用规范汉字表, the
// Chinese standard list of common characters (Unihan field kTGH), and the game's Chinese texts use
// them with Chinese punctuation (EXTRA, the same ranges as i18n::FONT_EXTRA). Noto Sans SC Medium
// draws them, the CJK face matching the weight of Bevy's built-in Fira Mono Medium. This writes:
//   src/name_han.txt                    the characters; net::valid_name accepts them (server, game, page)
//   assets/processed/name_font.otf      the font cut down to printable ASCII, those characters and EXTRA
//   assets/processed/name_font.json     sources, SHA256 and counts, checked by check-assets.mjs
//   assets/processed/name_font-OFL.txt  the font's license (SIL OFL 1.1), shipped beside it
// Sources are pinned by URL and SHA256 and cached in target/name-font-cache. Downloads use fetch:
// behind a proxy run with NODE_USE_ENV_PROXY=1, or save the files there by hand.
import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {unzipSync,strFromU8} from 'fflate';
import subsetFont from 'subset-font';
import * as hb from 'harfbuzzjs';
const ROOT=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const CACHE=path.join(ROOT,'target/name-font-cache');
const SOURCES={
 font:{file:'NotoSansSC-Medium.otf',url:'https://raw.githubusercontent.com/notofonts/noto-cjk/Sans2.004/Sans/SubsetOTF/SC/NotoSansSC-Medium.otf',sha256:'7633f5a016d4dd95e685a69633d818aabc4644c4b08e26bd35b1b30c45ed5dda'},
 license:{file:'NotoSansCJK-LICENSE.txt',url:'https://raw.githubusercontent.com/notofonts/noto-cjk/Sans2.004/LICENSE',sha256:'6a73f9541c2de74158c0e7cf6b0a58ef774f5a780bf191f2d7ec9cc53efe2bf2'},
 unihan:{file:'Unihan-16.0.0.zip',url:'https://www.unicode.org/Public/16.0.0/ucd/Unihan.zip',sha256:'b8f000df69de7828d21326a2ffea462b04bc7560022989f7cc704f10521ef3e0'},
};
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const sorted=a=>[...a].sort((x,y)=>x-y);
async function source({file,url,sha256}){
 const at=path.join(CACHE,file);
 let bytes=await fs.readFile(at).catch(()=>null);
 if(!bytes){
  console.log('Downloading '+url);
  try{const r=await fetch(url);if(!r.ok)throw new Error('HTTP '+r.status);bytes=Buffer.from(await r.arrayBuffer());}
  catch(e){throw new Error(`Could not download ${url} (${e.message}). Behind a proxy, rerun with NODE_USE_ENV_PROXY=1, or save the file as ${at}.`);}
  await fs.mkdir(CACHE,{recursive:true});await fs.writeFile(at,bytes);
 }
 assert.equal(hash(bytes),sha256,'Unexpected content, delete it to download again: '+at);
 return bytes;
}
const font=await source(SOURCES.font),license=await source(SOURCES.license),unihan=await source(SOURCES.unihan);
// kTGH lines read "U+4E00<TAB>kTGH<TAB>2013:1"; the header names the field as "#<TAB>kTGH".
const table=strFromU8(unzipSync(unihan,{filter:f=>f.name==='Unihan_OtherMappings.txt'})['Unihan_OtherMappings.txt']);
const han=sorted(new Set(table.split('\n').filter(l=>/^U\+[0-9A-F]{4,5}\tkTGH\t/.test(l)).map(l=>parseInt(l.slice(2),16))));
assert.equal(han.length,8105,'通用规范汉字表 has 8105 characters');
const ascii=Array.from({length:0x7f-0x20},(_,i)=>0x20+i);
// Quotes, dashes, the ellipsis, arrows, CJK punctuation and full-width forms: src/i18n.rs FONT_EXTRA,
// which a Rust test compares with the "extra" field written below.
const EXTRA=[[0xb7,0xb7],[0xd7,0xd7],[0x2013,0x2014],[0x2018,0x2019],[0x201c,0x201d],[0x2026,0x2026],[0x2190,0x2193],[0x3000,0x303f],[0xff01,0xff5e]];
const extra=EXTRA.flatMap(([lo,hi])=>Array.from({length:hi-lo+1},(_,i)=>lo+i));
const u=c=>'U+'+c.toString(16).toUpperCase().padStart(4,'0');
const extraSpec=EXTRA.map(([lo,hi])=>lo===hi?u(lo):u(lo)+'..'+u(hi)).join(' ');
const drawn=new Set(new hb.Face(new hb.Blob(font)).collectUnicodes()),missing=[...han,...extra].filter(c=>!drawn.has(c));
assert.equal(missing.length,0,'The source font lacks '+String.fromCodePoint(...missing.slice(0,20)));
// Names are short horizontal runs of plain glyphs at 21-34 px: no layout features (closing over them
// would pull in variants), no vertical metrics, no hints (330 KB; unhinted CJK reads fine at that size).
// Name IDs 13 and 14 keep the license notice and its URL inside the font.
const subset=await subsetFont(font,String.fromCodePoint(...ascii,...han,...extra),{targetFormat:'sfnt',keepFeatures:[],noHinting:true,dropTables:['vhea','vmtx','VORG','BASE','GPOS','GSUB'],preserveNameIds:[13,14]});
assert.deepEqual(sorted(new hb.Face(new hb.Blob(subset)).collectUnicodes()),sorted([...ascii,...han,...extra]),'The subset must draw exactly ASCII, the name characters and EXTRA');
// 64 characters a line, so the list stays reviewable; Rust ignores the line breaks.
const lines=[];for(let i=0;i<han.length;i+=64)lines.push(String.fromCodePoint(...han.slice(i,i+64)));
await fs.writeFile(path.join(ROOT,'src/name_han.txt'),lines.join('\n')+'\n');
await fs.writeFile(path.join(ROOT,'assets/processed/name_font.otf'),subset);
await fs.writeFile(path.join(ROOT,'assets/processed/name_font-OFL.txt'),license);
const info={
 schema_version:1,font:'processed/name_font.otf',sha256:hash(subset),bytes:subset.length,
 source:SOURCES.font.url,source_sha256:SOURCES.font.sha256,
 license:'SIL Open Font License 1.1',license_file:'processed/name_font-OFL.txt',license_source:SOURCES.license.url,
 characters:{ascii:'U+0020..U+007E',han:han.length,list:'src/name_han.txt',han_sha256:hash(String.fromCodePoint(...han)),standard:'通用规范汉字表 (2013), Unihan kTGH',source:SOURCES.unihan.url,source_sha256:SOURCES.unihan.sha256,extra:extraSpec,extra_count:extra.length},
 generated:'scripts/prepare-name-font.mjs',harfbuzz:hb.versionString(),
};
await fs.writeFile(path.join(ROOT,'assets/processed/name_font.json'),JSON.stringify(info,null,2)+'\n');
console.log(JSON.stringify({font:'assets/processed/name_font.otf',bytes:subset.length,han:han.length,ascii:ascii.length,extra:extra.length,list:'src/name_han.txt'},null,2));
