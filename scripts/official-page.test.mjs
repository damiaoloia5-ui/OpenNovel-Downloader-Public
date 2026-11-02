import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
const source=readFileSync('apps/desktop/src-tauri/src/official_page.js','utf8');
function element(value='',tagName='DIV'){return {nodeType:1,tagName,childNodes:[{nodeType:3,textContent:value}],getClientRects:()=>[{}],querySelector:()=>null,querySelectorAll:()=>[]};}
function fixture(platform='faloo'){
  const text=element('原创第一段。','P'),reader=element();
  reader.childNodes=[text];text.parentElement=reader;
  reader.querySelectorAll=s=>s==='p'?[text]:[];
  const nodes={'#novelName':element('原创旅行'),'.c_left h1':element('原创旅行 原创第一章'),'.c_left .noveContent':reader,'#J_BookCnt .read-hd p span':element('原创旅行'),'#J_BookCnt h1.chapter':element('原创第一章'),'#J_BookCnt .read-hd':element('字数：7'),'#J_BookCnt .read-bd':reader,'h1':element('原创咪咕作品')};
  const ids={hidNovelID:{value:'123'},hidNodeID:{value:'10'}};
  const document={readyState:'complete',querySelector:s=>nodes[s]||null,querySelectorAll:()=>[],getElementById:id=>ids[id]||null};
  const origin=platform==='faloo'?'https://b.faloo.com':platform==='ciweimao'?'https://www.ciweimao.com':'https://www.cmread.com';
  const pathname=platform==='faloo'?'/123_10.html':platform==='ciweimao'?'/chapter/10':'/yuekan/novel/123.html';
  const context={document,location:{origin,hostname:new URL(origin).hostname,pathname},HB:{book:{book_id:123,chapter_id:10}},getComputedStyle:e=>({display:'block',visibility:'visible',opacity:'1',filter:'none',...e.style})};
  return {document,reader,text,ids,context,run(){return JSON.parse(JSON.stringify(vm.runInNewContext(source.replace('__PLATFORM__',platform),context)));}};
}
test('official text extraction is visible-only, identity-bound and never claims complete chapters',()=>{
  const f=fixture();assert.equal(f.run().status,'text');assert.equal(f.run().paragraphs[0],'原创第一段。');assert.equal(f.run().declared_words,null);
  f.text.style={display:'none'};assert.equal(f.run().status,'unsupported');f.text.style={};
  f.ids.hidNodeID.value='99';assert.equal(f.run().status,'unsupported');
});
test('verification, subscription, image and loading states carry no paragraphs',()=>{
  for(const status of ['loading','verification','restricted','protected']){
    const f=fixture();
    if(status==='loading')f.document.readyState='interactive';
    if(status==='verification')f.document.querySelectorAll=s=>s.startsWith('iframe')?[{...element(),title:'安全验证'}]:[];
    if(status==='restricted')f.text.childNodes[0].textContent='订阅本章';
    if(status==='protected')f.reader.querySelector=()=>element('','IMG');
    assert.equal(f.run().status,status);assert.deepEqual(f.run().paragraphs,[]);
  }
});
test('Ciwemo protected rendering is refused and Migu detail is metadata-only',()=>{
  const c=fixture('ciweimao');c.ids.J_BookImage=element();assert.equal(c.run().status,'protected');assert.deepEqual(c.run().paragraphs,[]);
  const m=fixture('migu');assert.equal(m.run().status,'book');assert.equal(m.run().book_id,'123');assert.deepEqual(m.run().paragraphs,[]);
});
test('Ciwemo standalone human verification is recognized without reading or solving it',()=>{
  const f=fixture('ciweimao');
  f.context.location.pathname='/signup/man_machine_verify';
  f.ids.J_ManMachineVerify=element();
  assert.equal(f.run().status,'verification');assert.deepEqual(f.run().paragraphs,[]);
  f.ids.J_ManMachineVerify.style={display:'none'};
  assert.equal(f.run().status,'unsupported');
  f.ids.J_ManMachineVerify.style={};f.context.location.pathname='/book/123';
  assert.notEqual(f.run().status,'verification');
});
test('visible text is not cut off by a two-hundred-thousand-character quota',()=>{
  const f=fixture();f.text.childNodes[0].textContent='山'.repeat(200001);
  const result=f.run();assert.equal(result.status,'text');assert.equal(result.paragraphs[0].length,200001);
});
