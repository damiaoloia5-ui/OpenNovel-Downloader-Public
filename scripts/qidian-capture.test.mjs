// Controlled DOM-shaped objects only: no browser, window automation or network.
import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
const source=readFileSync('apps/desktop/src-tauri/src/capture_page.js','utf8').replace('__MODE__','chapter');
class Element {
  constructor(tag='DIV',attributes={}){this.nodeType=1;this.tagName=tag;this.attrs=attributes;this.childNodes=[];this.style={};this.innerText='';this.rect=true;}
  getAttribute(key){return this.attrs[key]??null;}
  getClientRects(){return this.rect?[{}]:[];}
  matches(){return ['BUTTON','SCRIPT','STYLE','SVG','CANVAS','I'].includes(this.tagName);}
  querySelectorAll(selector){return this.queries?.(selector)||[];}
  querySelector(selector){return this.querySelectorAll(selector)[0]||null;}
  add(child){child.parentElement=this;this.childNodes.push(child);return child;}
  content(value){this.childNodes.push({nodeType:3,textContent:value});return this;}
}
function fixture(count=3){
  let parent=new Element();for(let i=0;i<8;i++)parent=parent.add(new Element());
  const owner=parent.add(new Element('ARTICLE',{'data-cid':'2'}));
  const title=owner.add(new Element('H3').content('原创章节'));
  const metadata=owner.add(new Element().content(`${count*8}字`));
  const reader=owner.add(new Element());const paragraphs=[];
  for(let i=0;i<count;i++){const p=reader.add(new Element('P').content('原创正文'));p.add(new Element('SPAN').content('继续出发'));paragraphs.push(p);}
  const hidden=paragraphs[0].add(new Element('SPAN').content('不能保存的隐藏字'));hidden.style.display='none';
  reader.queries=s=>s==='p'?paragraphs:[];
  let gates=[];let buttons=[];let iframes=[];
  owner.queries=s=>s.includes('.read-content')?[reader]:s==='.j_chapterName'?[title]:s==='.text-info'?[metadata]:s.includes('.subscribe-btn')?gates:[];
  const document={readyState:'complete',getElementById:id=>id==='chapter-2'?owner:null,querySelectorAll:s=>s==='iframe'?iframes:s==='button'?buttons:[],querySelector:()=>null};
  let styleCalls=0;
  const context={document,location:{origin:'https://www.qidian.com',pathname:'/chapter/1/2/'},Node:{TEXT_NODE:3,ELEMENT_NODE:1},innerHeight:1000,innerWidth:1000,URL,getComputedStyle:node=>{styleCalls++;return {display:'block',visibility:'visible',opacity:'1',filter:'none',...node.style};}};
  return {owner,reader,paragraphs,hidden,document,context,get styleCalls(){return styleCalls;},reset(){styleCalls=0;},gate(){const e=owner.add(new Element('BUTTON'));e.innerText='订阅本章';buttons=[e];},verify(){const e=owner.add(new Element('IFRAME'));e.title='验证码';e.src='https://example.invalid/captcha';e.getBoundingClientRect=()=>({top:0,left:0,right:30,bottom:30});iframes=[e];},protect(){reader.queries=s=>s==='canvas,iframe'?[new Element('CANVAS')]:s==='p'?paragraphs:[];}};
}
function run(f,script=source){return JSON.parse(JSON.stringify(vm.runInNewContext(script,f.context)));}
test('capture retains visible complete text and excludes hidden descendants',()=>{
  const f=fixture();const r=run(f);assert.equal(r.title,'原创章节');assert.equal(r.paragraphs.length,3);assert.ok(r.paragraphs.every(p=>p==='原创正文继续出发'));
});
test('visibility cache is per extraction and detects newly hidden ancestor',()=>{
  const f=fixture();assert.ok(run(f).paragraphs);f.reader.style.opacity='0';assert.equal(run(f).error,'reader');f.reader.style.opacity='1';assert.ok(run(f).paragraphs);
  for(const style of [{visibility:'hidden'},{display:'none'},{filter:'blur(4px)'}]){f.owner.style=style;assert.equal(run(f).error,'reader');}
});
test('every observation still checks loading, subscription, verification and protected nodes',()=>{
  const f=fixture();f.document.readyState='interactive';assert.equal(run(f).error,'loading');f.document.readyState='complete';f.gate();assert.equal(run(f).error,'restricted');
  const v=fixture();v.verify();assert.equal(run(v).error,'verification');const p=fixture();p.protect();assert.equal(run(p).error,'protected');
});
test('long shared-ancestor DOM computes styles at most once per distinct node',()=>{
  const f=fixture(1000);assert.equal(run(f).paragraphs.length,1000);assert.ok(f.styleCalls<2100,`style calls: ${f.styleCalls}`);
});
test('old/new script controlled traversal comparison',{skip:!process.env.OPENNOVEL_CAPTURE_BASELINE},()=>{
  const previous=readFileSync(process.env.OPENNOVEL_CAPTURE_BASELINE,'utf8').replace('__MODE__','chapter');
  const a=fixture(1000),b=fixture(1000);const old=run(a,previous),current=run(b);assert.deepEqual(current,old);
  assert.ok(b.styleCalls<a.styleCalls);console.log(JSON.stringify({paragraphs:1000,oldStyleCalls:a.styleCalls,newStyleCalls:b.styleCalls,reduction:1-b.styleCalls/a.styleCalls}));
});
