// Values are never returned: findings contain only a rule, file and line.
import {inflateSync} from 'node:zlib';

const rules=[
 ['private-key',/-{5}BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-{5}/],
 ['github-token',/\b(?:github_pat_[A-Za-z0-9_]{22,}|gh[pousr]_[A-Za-z0-9]{30,})\b/],
 ['aws-key-id',/\b(?:AKIA|ASIA)[A-Z0-9]{16}\b/],
 ['api-token',/\b(?:sk-proj-[A-Za-z0-9_-]{30,}|sk-ant-[A-Za-z0-9_-]{30,}|sk-[A-Za-z0-9]{32,}|AIza[A-Za-z0-9_-]{35}|xox[baprs]-[A-Za-z0-9-]{20,})\b/],
 ['absolute-windows-path',/\b[A-Za-z]:[\\/]/],
 ['personal-home-path',/\/(?:Users|home)\/(?!<|\$|\{)[^\s/"'`<>]+/]
];

function scanText(file,text,context){
 const findings=[];
 for(const [index,line] of text.split(/\r?\n/).entries()){
  for(const [rule,pattern] of rules)if(pattern.test(line))findings.push({file,rule,line:index+1,...(context?{context}:{})});
 }
 return findings;
}

// Scan metadata, not compressed pixels. Original art bytes are never changed.
export function findPrivacyIssues(file,bytes){
 if(!Buffer.isBuffer(bytes))bytes=Buffer.from(bytes);
 const findings=[];
 const scan=(data,context)=>{
  findings.push(...scanText(file,data.toString('utf8'),context));
  if(data.includes(0))for(const start of [0,1]){
   const wide=data.subarray(start,start+Math.max(0,data.length-start-(data.length-start)%2));
   findings.push(...scanText(file,wide.toString('utf16le'),context));
   findings.push(...scanText(file,Buffer.from(wide).swap16().toString('utf16le'),context));
  }
 };
 if(bytes.subarray(0,8).equals(Buffer.from([137,80,78,71,13,10,26,10]))){
  for(let offset=8;offset+12<=bytes.length;){
   const length=bytes.readUInt32BE(offset),type=bytes.toString('ascii',offset+4,offset+8),end=offset+12+length;
   if(end>bytes.length){findings.push({file,rule:'invalid-image-metadata'});break;}
   const data=bytes.subarray(offset+8,offset+8+length);
   try{
    if(type==='tEXt'||type==='eXIf')scan(data,'PNG '+type);
    if(type==='zTXt'){
     const zero=data.indexOf(0);
     if(zero<0||data[zero+1]!==0)throw new Error('Invalid text chunk');
     scan(data.subarray(0,zero),'PNG zTXt keyword');
     scan(inflateSync(data.subarray(zero+2),{maxOutputLength:1024*1024}),'PNG zTXt');
    }
    if(type==='iTXt'){
     const zero=data.indexOf(0);if(zero<0)throw new Error('Invalid text chunk');
     const compressed=data[zero+1],method=data[zero+2];
     const languageEnd=data.indexOf(0,zero+3),translatedEnd=data.indexOf(0,languageEnd+1);
     if(languageEnd<0||translatedEnd<0||compressed>1||method!==0)throw new Error('Invalid text chunk');
     scan(data.subarray(0,zero),'PNG iTXt keyword');
     const text=data.subarray(translatedEnd+1);
     scan(compressed?inflateSync(text,{maxOutputLength:1024*1024}):text,'PNG iTXt');
    }
   }catch{findings.push({file,rule:'unreadable-image-metadata',context:'PNG '+type});}
   offset=end;
  }
 }else if(bytes.toString('ascii',0,4)==='RIFF'&&bytes.toString('ascii',8,12)==='WEBP'){
  for(let offset=12;offset+8<=bytes.length;){
   const type=bytes.toString('ascii',offset,offset+4),length=bytes.readUInt32LE(offset+4),end=offset+8+length;
   if(end>bytes.length){findings.push({file,rule:'invalid-image-metadata'});break;}
   if(type==='EXIF'||type==='XMP ')scan(bytes.subarray(offset+8,end),'WebP '+type.trim());
   offset=end+(length%2);
  }
 }else if(bytes.length>=4&&(bytes.toString('ascii',0,4)==='OTTO'||bytes.readUInt32BE(0)===0x00010000)){
  // OpenType glyph programs are binary and can resemble short path prefixes.
  // Inspect the name records and CFF string metadata instead.
  try{
   if(bytes.length<12)throw new Error('Invalid font');
   const tables=new Map();
   for(let i=0;i<bytes.readUInt16BE(4);i++){
    const at=12+i*16,tag=bytes.toString('ascii',at,at+4),offset=bytes.readUInt32BE(at+8),length=bytes.readUInt32BE(at+12);
    if(offset+length>bytes.length)throw new Error('Invalid font table');
    tables.set(tag,bytes.subarray(offset,offset+length));
   }
   const names=tables.get('name');
   if(names)for(let i=0;i<names.readUInt16BE(2);i++){
    const at=6+i*12,length=names.readUInt16BE(at+8),offset=names.readUInt16BE(4)+names.readUInt16BE(at+10);
    if(offset+length>names.length)throw new Error('Invalid font name');
    scan(names.subarray(offset,offset+length),'OpenType name');
   }
   const cff=tables.get('CFF ');
   if(cff){
    if(cff[0]!==1)throw new Error('Unsupported CFF metadata');
    let at=cff[2];
    for(let index=0;index<3;index++){
     const count=cff.readUInt16BE(at);if(!count){at+=2;continue;}
     const size=cff[at+2];if(size<1||size>4)throw new Error('Invalid CFF index');
     const start=at+3+(count+1)*size,end=start+cff.readUIntBE(at+3+count*size,size)-1;
     if(end>cff.length)throw new Error('Invalid CFF strings');
     if(index!==1)for(let i=0;i<count;i++){
      const a=start+cff.readUIntBE(at+3+i*size,size)-1,b=start+cff.readUIntBE(at+3+(i+1)*size,size)-1;
      scan(cff.subarray(a,b),'OpenType CFF string');
     }
     at=end;
    }
   }
  }catch{findings.push({file,rule:'unreadable-font-metadata'});}
 }else scan(bytes);
 return [...new Map(findings.map(item=>[JSON.stringify(item),item])).values()];
}
