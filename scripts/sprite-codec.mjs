// Shared, dependency-free PNG/GIF codec. No filesystem side effects.
import zlib from 'node:zlib';
import assert from 'node:assert/strict';
export const CELL=512, PIVOT={x:256,y:460};
const TICKS=[3,3,2,2,2,3,3,4];
function decodePng(buffer) {
  assert.equal(buffer.subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
  let header, offset = 8; const chunks = [];
  while (offset < buffer.length) {
    const size = buffer.readUInt32BE(offset);
    const type = buffer.toString('ascii', offset + 4, offset + 8);
    const body = buffer.subarray(offset + 8, offset + 8 + size);
    if (type === 'IHDR') header = { width: body.readUInt32BE(0), height: body.readUInt32BE(4), depth: body[8], type: body[9], compression: body[10], filter: body[11], interlace: body[12] };
    if (type === 'IDAT') chunks.push(body);
    offset += size + 12;
  }
  assert(header && header.depth === 8 && header.type === 6 && header.interlace === 0 && header.compression === 0 && header.filter === 0, 'Expected non-interlaced 8-bit RGBA PNG');
  const stride = header.width * 4;
  const filtered = zlib.inflateSync(Buffer.concat(chunks));
  assert.equal(filtered.length, (stride + 1) * header.height);
  const pixels = Buffer.alloc(stride * header.height);
  const paeth = (a, b, c) => { const p = a + b - c, da = Math.abs(p-a), db = Math.abs(p-b), dc = Math.abs(p-c); return da <= db && da <= dc ? a : db <= dc ? b : c; };
  let input = 0;
  for (let y = 0; y < header.height; y++) {
    const filter = filtered[input++], start = y * stride;
    assert(filter <= 4, 'Invalid PNG filter');
    for (let x = 0; x < stride; x++) {
      const a = x >= 4 ? pixels[start+x-4] : 0;
      const b = y > 0 ? pixels[start+x-stride] : 0;
      const c = x >= 4 && y > 0 ? pixels[start+x-stride-4] : 0;
      const prediction = filter === 0 ? 0 : filter === 1 ? a : filter === 2 ? b : filter === 3 ? Math.floor((a+b)/2) : paeth(a,b,c);
      pixels[start+x] = (filtered[input++] + prediction) & 255;
    }
  }
  return { ...header, pixels };
}

const CRC_TABLE = Array.from({ length: 256 }, (_, i) => { let c = i; for (let k=0;k<8;k++) c = (c & 1) ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0; });
function pngChunk(type, payload) {
  const name = Buffer.from(type), chunk = Buffer.alloc(payload.length + 12);
  chunk.writeUInt32BE(payload.length, 0); name.copy(chunk, 4); payload.copy(chunk, 8);
  let crc = 0xffffffff;
  for (const byte of Buffer.concat([name, payload])) crc = CRC_TABLE[(crc ^ byte) & 255] ^ (crc >>> 8);
  chunk.writeUInt32BE((crc ^ 0xffffffff) >>> 0, chunk.length - 4);
  return chunk;
}
function encodePng(width, height, pixels) {
  const header = Buffer.alloc(13); header.writeUInt32BE(width,0); header.writeUInt32BE(height,4); header[8]=8; header[9]=6;
  const stride = width*4, scan = Buffer.alloc((stride+1)*height);
  for (let y=0;y<height;y++) pixels.copy(scan,y*(stride+1)+1,y*stride,(y+1)*stride);
  return Buffer.concat([Buffer.from('89504e470d0a1a0a','hex'), pngChunk('IHDR',header), pngChunk('IDAT',zlib.deflateSync(scan,{level:9})), pngChunk('IEND',Buffer.alloc(0))]);
}

function findCharacters(image) {
  const { width:w, height:h, pixels } = image;
  const labels = new Int32Array(w*h), queue = new Int32Array(w*h), found = [];
  let label=0;
  for (let seed=0;seed<labels.length;seed++) {
    if (labels[seed] || pixels[seed*4+3] < 96) continue;
    const id=++label; let read=0, count=1, minX=w,minY=h,maxX=0,maxY=0;
    queue[0]=seed; labels[seed]=id;
    while(read<count) {
      const index=queue[read++], x=index%w, y=Math.floor(index/w);
      minX=Math.min(minX,x); maxX=Math.max(maxX,x); minY=Math.min(minY,y); maxY=Math.max(maxY,y);
      const visit = n => { if (!labels[n] && pixels[n*4+3]>=96) { labels[n]=id; queue[count++]=n; } };
      if(x>0) visit(index-1); if(x+1<w) visit(index+1); if(y>0) visit(index-w); if(y+1<h) visit(index+w);
    }
    found.push({ id, count, minX,minY,maxX,maxY });
  }
  const characters=found.sort((a,b)=>b.count-a.count).slice(0,8);
  assert.equal(characters.length,8); assert(characters.every(c=>c.count>10000),'Cannot isolate eight complete characters');
  characters.forEach(c=>c.row=(c.minY+c.maxY)/2<h/2?0:1);
  assert.equal(characters.filter(c=>c.row===0).length,4); assert.equal(characters.filter(c=>c.row===1).length,4);
  characters.sort((a,b)=>a.row-b.row || a.minX-b.minX);
  for (const c of characters) {
    let left=w,right=0; const band=Math.max(12,Math.round((c.maxY-c.minY)*0.065));
    for(let y=c.maxY-band;y<=c.maxY;y++) for(let x=c.minX;x<=c.maxX;x++) if(labels[y*w+x]===c.id) { left=Math.min(left,x);right=Math.max(right,x); }
    c.rootX=(left+right+1)/2; c.rootY=c.maxY+1;
    c.cropX=Math.max(0,c.minX-3); c.cropY=Math.max(0,c.minY-3);
    c.cropW=Math.min(w,c.maxX+4)-c.cropX; c.cropH=Math.min(h,c.maxY+4)-c.cropY;
    let mask=new Uint8Array(c.cropW*c.cropH);
    for(let y=0;y<c.cropH;y++) for(let x=0;x<c.cropW;x++) mask[y*c.cropW+x]=labels[(y+c.cropY)*w+x+c.cropX]===c.id?1:0;
    // Retain only a narrow antialiased fringe around the body component.
    for(let pass=0;pass<3;pass++) {
      const next=mask.slice();
      for(let y=0;y<c.cropH;y++) for(let x=0;x<c.cropW;x++) if(mask[y*c.cropW+x]) {
        for(let dy=-1;dy<=1;dy++) for(let dx=-1;dx<=1;dx++) {
          const xx=x+dx,yy=y+dy; if(xx>=0 && yy>=0 && xx<c.cropW && yy<c.cropH) next[yy*c.cropW+xx]=1;
        }
      }
      mask=next;
    }
    c.clean=Buffer.alloc(c.cropW*c.cropH*4);
    for(let y=0;y<c.cropH;y++) for(let x=0;x<c.cropW;x++) {
      const dst=(y*c.cropW+x)*4,src=((y+c.cropY)*w+x+c.cropX)*4;
      if(mask[y*c.cropW+x] && pixels[src+3]>=24) pixels.copy(c.clean,dst,src,src+4);
    }
  }
  return characters;
}
function renderFrame(c,scale,pivot=PIVOT) {
  const frame=Buffer.alloc(CELL*CELL*4);
  for(let y=0;y<CELL;y++) for(let x=0;x<CELL;x++) {
    const sx=c.rootX-c.cropX+(x+0.5-pivot.x)/scale-0.5;
    const sy=c.rootY-c.cropY+(y+0.5-pivot.y)/scale-0.5;
    const ix=Math.floor(sx),iy=Math.floor(sy),fx=sx-ix,fy=sy-iy;
    let a=0,r=0,g=0,b=0;
    for(let dy=0;dy<=1;dy++) for(let dx=0;dx<=1;dx++) {
      const xx=ix+dx,yy=iy+dy; if(xx<0 || yy<0 || xx>=c.cropW || yy>=c.cropH) continue;
      const at=(yy*c.cropW+xx)*4,weight=(dx?fx:1-fx)*(dy?fy:1-fy),alpha=c.clean[at+3]*weight;
      a+=alpha;r+=c.clean[at]*alpha;g+=c.clean[at+1]*alpha;b+=c.clean[at+2]*alpha;
    }
    if(a>=8) {const at=(y*CELL+x)*4;frame[at]=Math.round(r/a);frame[at+1]=Math.round(g/a);frame[at+2]=Math.round(b/a);frame[at+3]=Math.round(a);}
  }
  return frame;
}
function bounds(pixels,w,h,threshold=24) {
  let minX=w,minY=h,maxX=-1,maxY=-1;
  for(let y=0;y<h;y++) for(let x=0;x<w;x++) if(pixels[(y*w+x)*4+3]>=threshold) {minX=Math.min(minX,x);minY=Math.min(minY,y);maxX=Math.max(maxX,x);maxY=Math.max(maxY,y);}
  return {minX,minY,maxX,maxY};
}

function previewRgb(frame) {
  const rgb=Buffer.alloc(CELL*CELL*3), bg=[238,242,247];
  for(let y=0;y<CELL;y++) for(let x=0;x<CELL;x++) {
    const p=(y*CELL+x),a=frame[p*4+3]/255;
    const base=y===PIVOT.y+2 && x>=48 && x<CELL-48?[190,198,208]:bg;
    for(let k=0;k<3;k++) rgb[p*3+k]=Math.round(frame[p*4+k]*a+base[k]*(1-a));
  }
  return rgb;
}
function gifPalette(frames) {
  const count=new Uint32Array(32768), red=new Uint32Array(32768),green=new Uint32Array(32768),blue=new Uint32Array(32768);
  for(const rgb of frames) for(let p=0;p<rgb.length;p+=3) {
    const r=rgb[p],g=rgb[p+1],b=rgb[p+2],key=((r>>3)<<10)|((g>>3)<<5)|(b>>3);
    count[key]++;red[key]+=r;green[key]+=g;blue[key]+=b;
  }
  const colors=[]; for(let key=0;key<count.length;key++) if(count[key]) colors.push({key,n:count[key],c:[red[key]/count[key],green[key]/count[key],blue[key]/count[key]]});
  const box=values=>{
    const low=[255,255,255],high=[0,0,0]; let n=0;
    for(const v of values) {n+=v.n;for(let k=0;k<3;k++) {low[k]=Math.min(low[k],v.c[k]);high[k]=Math.max(high[k],v.c[k]);}}
    const spans=high.map((v,k)=>v-low[k]); const axis=spans.indexOf(Math.max(...spans));
    return {values,n,axis,score:values.length>1?spans[axis]*Math.sqrt(n):0};
  };
  const boxes=[box(colors)];
  while(boxes.length<256) {
    boxes.sort((a,b)=>b.score-a.score);const current=boxes.shift();
    if(!current.score) {boxes.unshift(current);break;}
    const v=current.values.sort((a,b)=>a.c[current.axis]-b.c[current.axis]);
    let total=0,cut=1;for(let i=0;i<v.length-1;i++) {total+=v[i].n;cut=i+1;if(total>=current.n/2)break;}
    boxes.push(box(v.slice(0,cut)),box(v.slice(cut)));
  }
  const palette=boxes.map(b=>[0,1,2].map(k=>Math.round(b.values.reduce((sum,v)=>sum+v.c[k]*v.n,0)/b.n)));
  while(palette.length<256)palette.push([0,0,0]);
  const lookup=new Int16Array(32768);lookup.fill(-1);
  const indexed=frames.map(rgb=>{
    const result=Buffer.alloc(CELL*CELL);
    for(let i=0;i<result.length;i++) {
      const r=rgb[i*3],g=rgb[i*3+1],b=rgb[i*3+2],key=((r>>3)<<10)|((g>>3)<<5)|(b>>3);
      if(lookup[key]<0) {let best=0,dist=Infinity;for(let p=0;p<palette.length;p++){const c=palette[p],d=(r-c[0])**2+(g-c[1])**2+(b-c[2])**2;if(d<dist){best=p;dist=d;}}lookup[key]=best;}
      result[i]=lookup[key];
    }
    return result;
  });
  return {palette,indexed};
}
// Valid GIF LZW literal stream with frequent CLEAR codes: deliberately simple,
// bounded to 9-bit codes, and avoids dependencies for this small preview.
function gifLiteralStream(indices) {
  const data=Buffer.alloc(Math.ceil((indices.length+Math.ceil(indices.length/200)+3)*9/8));
  let bits=0,held=0,pos=0;
  const emit=code=>{held|=code<<bits;bits+=9;while(bits>=8){data[pos++]=held&255;held>>>=8;bits-=8;}};
  emit(256);
  for(let i=0;i<indices.length;i++){if(i>0 && i%200===0)emit(256);emit(indices[i]);}
  emit(257);if(bits)data[pos++]=held&255;
  const blocks=[Buffer.from([8])];
  for(let i=0;i<pos;i+=255){const n=Math.min(255,pos-i);blocks.push(Buffer.from([n]),data.subarray(i,i+n));}
  blocks.push(Buffer.from([0]));return Buffer.concat(blocks);
}
function encodeGif(frames,ticks=TICKS) {
  const {palette,indexed}=gifPalette(frames.map(previewRgb));
  const screen=Buffer.alloc(7);screen.writeUInt16LE(CELL,0);screen.writeUInt16LE(CELL,2);screen[4]=0xf7;
  const chunks=[Buffer.from('GIF89a'),screen,Buffer.from(palette.flat()),Buffer.from([0x21,0xff,0x0b]),Buffer.from('NETSCAPE2.0'),Buffer.from([3,1,0,0,0])];
  for(let i=0;i<indexed.length;i++) {
    const delay=Math.round(ticks[i]/60*100)+(i===indexed.length-1?80:0);
    const control=Buffer.from([0x21,0xf9,4,4,delay&255,delay>>8,0,0]);
    const descriptor=Buffer.alloc(10);descriptor[0]=0x2c;descriptor.writeUInt16LE(CELL,5);descriptor.writeUInt16LE(CELL,7);
    chunks.push(control,descriptor,gifLiteralStream(indexed[i]));
  }
  chunks.push(Buffer.from([0x3b]));return Buffer.concat(chunks);
}


export {decodePng,encodePng,findCharacters,renderFrame,bounds,encodeGif};
