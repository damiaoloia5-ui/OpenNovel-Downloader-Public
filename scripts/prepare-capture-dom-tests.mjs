import {readFile,mkdir,writeFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
const root=new URL('../',import.meta.url);
const source=await readFile(new URL('apps/desktop/src-tauri/src/capture_page.js',root),'utf8');
const target=new URL('.artifacts/capture-dom-tests/',root);
await mkdir(target,{recursive:true});
const serialized=JSON.stringify(source).replaceAll('<','\\u003c');
await writeFile(new URL('index.html',target),`<!doctype html><meta charset="utf-8"><title>OpenNovel capture DOM contracts</title>
<style>body{font:17px system-ui;margin:24px}iframe{width:1100px;height:800px;border:1px solid #ddd}.pass{color:#165c30}.fail{color:#b00}</style>
<h1>OpenNovel capture DOM contracts</h1><p id="summary">RUNNING</p><ol id="results"></ol><iframe id="fixture" title="原创受控页面"></iframe>
<script>
const source=${serialized};
const frame=document.getElementById('fixture');const results=[];
const catalog='<h1 class="book-title">原创测试书</h1><span class="writer">原创作者 著</span><h2>目录（3章）</h2><div class="volume-wrap"><a href="https://www.qidian.com/chapter/123/10/">第一章 出发</a><br><a href="https://www.qidian.com/chapter/123/30/">第二章 山路</a><br><a href="https://www.qidian.com/chapter/123/20/">第三章 归途</a></div>';
const body='<article class="main-text-wrap" data-cid="10"><header class="text-head"><h3 class="j_chapterName">第一章 出发</h3><div class="text-info">原创测试书　50字</div></header><div class="read-content"><p>这是一段原创测试文字。<span style="display:none">隐藏文字不能保存</span>旅人望着山顶的小屋，决定雨停后继续出发。<button>99条段评</button></p><p>第二段原创文字。</p></div></article>';
async function read(html,mode='chapter',path='/chapter/123/10/'){
  await new Promise(resolve=>{frame.onload=resolve;frame.srcdoc='<!doctype html><meta charset="utf-8">'+html;});
  const evaluate=new Function('document','location','getComputedStyle','Node','return ('+String.fromCharCode(10)+source.replace('__MODE__',mode).trim().replace(/;$/, '')+String.fromCharCode(10)+')');
  return evaluate(frame.contentDocument,{origin:'https://www.qidian.com',pathname:path,href:'https://www.qidian.com'+path},frame.contentWindow.getComputedStyle.bind(frame.contentWindow),frame.contentWindow.Node);
}
function assert(value,message){if(!value)throw new Error(message);}
async function test(name,work){try{await work();results.push({name,pass:true});}catch(error){results.push({name,pass:false,error:String(error)});}const item=document.createElement('li');const result=results.at(-1);item.className=result.pass?'pass':'fail';item.textContent=(result.pass?'PASS ':'FAIL ')+name+(result.error?' '+result.error:'');document.getElementById('results').append(item);}
(async()=>{
await test('catalog: declared total, identity and DOM order',async()=>{const r=await read(catalog,'catalog','/book/123/');assert(r.chapters.map(c=>c.id).join(',')==='10,30,20','order');assert(r.declared_total===3&&r.author==='原创作者','metadata');});
await test('catalog: observed current layout and dedicated catalog URL',async()=>{const html='<div class="book-info-top"><h1 id="bookName">原创新布局</h1><p class="book-meta"><span class="author">作者:原创作者</span></p></div><div class="book-catalog" id="bookCatalogSection"><p class="catalog-header-infos"><a class="catalog-header-title">目录</a><span class="catalog-header-desc">连载共3章</span></p><div id="allCatalog"><div class="catalog-volume"><ul class="volume-chapters"><li class="chapter-item"><a href="https://www.qidian.com/chapter/123/10/">第一章 出发</a></li><li class="chapter-item"><a href="https://www.qidian.com/chapter/123/30/">第二章 山路</a></li><li class="chapter-item"><a href="https://www.qidian.com/chapter/123/20/">第三章 归途</a></li></ul></div></div></div>';const r=await read(html,'catalog','/book/123/catalog/');assert(r.chapters.length===3&&r.author==='原创作者'&&r.title==='原创新布局','current layout');});
await test('catalog: hidden links are not accepted as complete',async()=>{const r=await read(catalog.replace('<a href="https://www.qidian.com/chapter/123/30/">','<a style="display:none" href="https://www.qidian.com/chapter/123/30/">'),'catalog','/book/123/');assert(r.error==='catalog','hidden missing chapter');});
await test('catalog: duplicate links deduplicate by remote ID',async()=>{const r=await read(catalog.replace('</div>','<a href="https://www.qidian.com/chapter/123/10/">第一章 出发</a></div>'),'catalog','/book/123/');assert(r.chapters.length===3,'duplicates');});
await test('catalog: wrong-book links cannot fill a missing chapter',async()=>{const r=await read(catalog.replace('/123/30/','/999/30/'),'catalog','/book/123/');assert(r.error==='catalog','wrong book');});
await test('catalog: missing total is refused',async()=>{const r=await read(catalog.replace('目录（3章）','目录'),'catalog','/book/123/');assert(r.error==='catalog','missing total');});
await test('chapter: normally rendered paragraphs and metadata',async()=>{const r=await read(body);assert(r.title==='第一章 出发'&&r.declared_words===50&&r.paragraphs.length===2,'metadata');assert(!r.paragraphs.join('').includes('隐藏')&&!r.paragraphs.join('').includes('段评'),'hidden content and annotations');});
await test('chapter: observed modern reader excludes title and paragraph reviews',async()=>{const html='<div class="print enable-review" data-id="10"><h1 class="title content-text">第一章 出发<span class="review">37</span></h1><div class="text-s-gray-500"><span><span class="num-word-cut"><span>1</span><span>2</span><span>3</span></span>50字</span></div><main class="content" id="c-10" data-type="cjk"><p><span class="content-text">原创文字，旅人在山路上前行。</span><span class="review">99</span></p></main></div>';const r=await read(html);assert(r.title==='第一章 出发'&&r.declared_words===50&&r.paragraphs[0]==='原创文字，旅人在山路上前行。','modern chapter');});
await test('chapter: visible subscription gate is refused',async()=>{const r=await read(body.replace('</article>','<div class="subscribe-wrap">订阅本章</div></article>'));assert(r.error==='restricted','visible gate');});
await test('chapter: visible purchase button outside reader is refused',async()=>{const r=await read(body+'<section><button>订阅本章</button></section>');assert(r.error==='restricted','outside paywall');});
await test('chapter: visible verification frame stops capture',async()=>{const r=await read('<iframe title="验证码" srcdoc="controlled verification"></iframe>'+body);assert(r.error==='verification','visible verification');});
await test('chapter: hidden subscription template is not a live gate',async()=>{const r=await read(body.replace('</article>','<div class="subscribe-wrap" style="display:none">订阅本章</div></article>'));assert(r.paragraphs?.length===2,'hidden template');});
await test('chapter: protected canvas is refused',async()=>{const r=await read(body.replace('<p>第二段原创文字。</p>','<canvas></canvas>'));assert(r.error==='protected','canvas');});
await test('chapter: blurred content is not normally readable text',async()=>{const r=await read(body.replace('class="read-content"','class="read-content" style="filter:blur(5px)"'));assert(r.error==='reader','blurred content');});
await test('chapter: missing word count is refused',async()=>{const r=await read(body.replace('50字','字数未知'));assert(r.error==='reader','word count');});
await test('chapter: wrong chapter container is refused',async()=>{const r=await read(body.replace('data-cid="10"','data-cid="99"'));assert(r.error==='reader','wrong chapter');});
await test('chapter: mixed readers without a matching owner are refused',async()=>{const r=await read(body.replace('data-cid="10"','')+body.replace('data-cid="10"',''));assert(r.error==='reader','mixed readers');});
await test('chapter: unknown markup never falls back to document body',async()=>{const r=await read('<h1>登录</h1><input value="not-read"><p>正文未知</p>');assert(r.error==='reader','unknown body');});
await test('page: catalog and chapter mode remain separate',async()=>{const r=await read(catalog,'chapter','/book/123/');assert(r.error==='chapter_page','wrong mode');});
const failed=results.filter(r=>!r.pass).length;document.getElementById('summary').textContent=(failed?'FAIL':'PASS')+' '+results.length+' contracts, '+failed+' failed';
})();
</script>`, 'utf8');
console.log('Prepared real-browser DOM contracts:',fileURLToPath(new URL('index.html',target)));
console.log('Serve .artifacts/capture-dom-tests on localhost and open index.html in a browser. Preparation is not a passing test result.');
