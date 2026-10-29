import assert from 'node:assert/strict';
import test from 'node:test';
import {deflateSync,gzipSync} from 'node:zlib';
import {findPrivacyIssues} from './privacy-rules.mjs';

const token=['gh','p_','A'.repeat(36)].join('');
const localPath=String.fromCharCode(67,58,92)+['Users','private-person','project'].join(String.fromCharCode(92));
const scan=bytes=>findPrivacyIssues('fixture',Buffer.from(bytes));
const chunk=(type,data)=>{const length=Buffer.alloc(4);length.writeUInt32BE(data.length);return Buffer.concat([length,Buffer.from(type),data,Buffer.alloc(4)]);};
const png=(type,data)=>Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk(type,data),chunk('IEND',Buffer.alloc(0))]);

test('privacy findings redact secrets and machine paths',()=>{
 const findings=scan(token+'\n'+localPath);
 assert.deepEqual(findings.map(x=>[x.rule,x.line]),[['github-token',1],['absolute-windows-path',2]]);
 assert.ok(!JSON.stringify(findings).includes(token));
 assert.ok(!JSON.stringify(findings).includes(localPath));
});
test('loopback commands and example hosts are safe documentation',()=>{
 assert.deepEqual(scan('http://127.0.0.1:18080 https://game.example.com /etc/naifrog/server.env'),[]);
});
test('empty and short files are safe to scan',()=>{
 for(const value of ['','a','ab','abc'])assert.deepEqual(scan(value),[]);
});
test('binary path checks ignore short instruction-like prefixes but catch user directories',()=>{
 assert.deepEqual(findPrivacyIssues('client.exe',Buffer.from([87,58,47])),[]);
 assert.ok(findPrivacyIssues('client.exe',Buffer.from(localPath)).some(x=>x.rule==='absolute-windows-path'));
});
test('compressed WASM and extensionless ELF are scanned as executable data',()=>{
 const code=Buffer.from([0,97,115,109,0,87,58,47,0]);
 assert.deepEqual(findPrivacyIssues('client.wasm.gz',gzipSync(code,{level:0})),[]);
 assert.deepEqual(findPrivacyIssues('server',Buffer.from([127,69,76,70,0,87,58,47,0])),[]);
 const findings=findPrivacyIssues('client.wasm.gz',gzipSync(Buffer.concat([code,Buffer.from(localPath)])));
 assert.ok(findings.some(x=>x.rule==='absolute-windows-path'&&x.file==='client.wasm.gz'));
 assert.equal(findPrivacyIssues('bad.wasm.gz',Buffer.from('bad'))[0].rule,'unreadable-compressed-artifact');
});
test('font metadata is checked without interpreting glyph bytes as text',()=>{
 const name=Buffer.from(localPath,'utf16le');name.swap16();
 const metadata=Buffer.alloc(18);metadata.writeUInt16BE(1,2);metadata.writeUInt16BE(18,4);metadata.writeUInt16BE(3,6);metadata.writeUInt16BE(name.length,14);
 const header=Buffer.alloc(28);header.write('OTTO');header.writeUInt16BE(1,4);header.write('name',12);header.writeUInt32BE(28,20);header.writeUInt32BE(metadata.length+name.length,24);
 assert.ok(findPrivacyIssues('font.otf',Buffer.concat([header,metadata,name])).some(x=>x.rule==='absolute-windows-path'));
 const binary=Buffer.from(header);binary.write('CFFx',12);binary.writeUInt32BE(token.length,24);
 assert.deepEqual(findPrivacyIssues('font.otf',Buffer.concat([binary,Buffer.from(token)])),[]);
});
test('PNG text metadata is checked without scanning compressed pixels',()=>{
 assert.equal(findPrivacyIssues('image.png',png('tEXt',Buffer.from('Source\0'+localPath)))[0].rule,'absolute-windows-path');
 assert.deepEqual(findPrivacyIssues('image.png',png('IDAT',Buffer.from(token))),[]);
});
test('compressed PNG metadata cannot hide a token',()=>{
 const data=Buffer.concat([Buffer.from('Comment\0\0'),deflateSync(Buffer.from(token))]);
 assert.equal(findPrivacyIssues('image.png',png('zTXt',data))[0].rule,'github-token');
});
test('compressed international PNG metadata is checked',()=>{
 const data=Buffer.concat([Buffer.from('Comment\0\x01\0en\0Comment\0'),deflateSync(Buffer.from(localPath))]);
 assert.equal(findPrivacyIssues('image.png',png('iTXt',data))[0].rule,'absolute-windows-path');
});
test('UTF-16 image metadata is checked',()=>{
 assert.ok(findPrivacyIssues('image.png',png('eXIf',Buffer.from(localPath,'utf16le'))).some(x=>x.rule==='absolute-windows-path'));
});
test('WebP metadata is checked and corrupt metadata fails closed',()=>{
 const text=Buffer.from(token),length=Buffer.alloc(4);length.writeUInt32LE(text.length);
 const payload=Buffer.concat([Buffer.from('WEBPXMP '),length,text,text.length%2?Buffer.alloc(1):Buffer.alloc(0)]);
 const size=Buffer.alloc(4);size.writeUInt32LE(payload.length);
 assert.equal(findPrivacyIssues('image.webp',Buffer.concat([Buffer.from('RIFF'),size,payload]))[0].rule,'github-token');
 assert.equal(findPrivacyIssues('image.png',png('zTXt',Buffer.from('broken')))[0].rule,'unreadable-image-metadata');
});
