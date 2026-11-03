import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
const script=readFileSync('apps/desktop/src-tauri/src/session_page.js','utf8');
function element(value='',tagName='DIV') {return {nodeType:1,tagName,childNodes:[{nodeType:3,textContent:value}],children:[],getClientRects:()=>[{}],getAttribute:()=>null,matches:()=>false,closest:()=>null,querySelectorAll:()=>[],classList:{contains:()=>false},dataset:{}};}
function fixture(platform){
 const paragraph=element('旅人沿着山路走进小屋。','P'),reader=element(),title=element('原创一章'),info=element('本章字数：11');reader.childNodes=[paragraph];paragraph.parentElement=reader;
  reader.querySelectorAll=s=>s===':scope > p'||s===':scope > p.chapter'?[paragraph]:[];
 const routes={zongheng:'https://read.zongheng.com/chapter/123/2.html','17k':'https://www.17k.com/chapter/123/2.html',qimao:'https://www.qimao.com/shuku/123-2/',shuqi:'https://www.shuqi.com/reader?bid=123&cid=2',jinjiang:'https://my.jjwxc.net/onebook_vip.php?novelid=123&chapterid=2',ciweimao:'https://www.ciweimao.com/chapter/2',faloo:'https://b.faloo.com/123_2.html'};
 const titles=['#Jcontent .title_txtbox','#readArea .readAreaBox > h1','.chapter-detail-wrap .chapter-title','.chapter-content h1','.noveltext h2','#J_BookCnt h1.chapter','.c_left h1'];
 const infos=['#Jcontent .bookinfo','#readArea .chapter_update_time','.chapter-tips','.noveltext .readsmall','#J_BookCnt .read-hd','.c_left .readInfo'];
 const bodies=['#Jcontent .reader-box .content','#readArea .readAreaBox > .p','.chapter-detail-article > .article','.chapter-content','.noveltext > #paragraph_comment_content','#J_BookCnt .read-bd','.c_left .noveContent'];
 const nodes=Object.fromEntries([...titles.map(k=>[k,title]),...infos.map(k=>[k,info]),...bodies.map(k=>[k,reader])]);
 nodes['input[name="novelid"]']={value:'123'};nodes['input[name="chapterid"]']={value:'2'};
 nodes['.js-dataChapters']={textContent:JSON.stringify({bookId:'123',chapterList:[{volumeList:[{chapterId:'2',chapterName:'原创一章',wordCount:11}]}]})};
 reader.querySelector=s=>s==='h3.chapter-title'?title:null;
 const ids={hidNovelID:{value:'123'},hidNodeID:{value:'2'}};
 const body=element();body.dataset={bookid:'123',chapterid:'2'};
 if(platform==='jinjiang')reader.childNodes=[{nodeType:3,textContent:'旅人沿着山路走进小屋。'}];
 const document={readyState:'complete',body,querySelector:s=>nodes[s]||null,getElementById:id=>ids[id]||null,querySelectorAll:s=>s==='.chapter-content'?[reader]:[]};
 const context={document,location:{href:routes[platform]},URL,HB:{book:{book_id:123,chapter_id:2}},$__BOOK:{bookId:123,chapterId:2},getComputedStyle:e=>({display:'block',visibility:'visible',opacity:'1',filter:'none',...e.style})};
  return {document,reader,paragraph,nodes,info,context,run:()=>JSON.parse(JSON.stringify(vm.runInNewContext(script.replaceAll('__PLATFORM__',platform).replaceAll('__BOOK__','123'),context)))};
}
test('seven platform rendered readers preserve visible paragraphs and identity-only URLs',()=>{
 for(const p of ['zongheng','17k','qimao','shuqi','jinjiang','ciweimao','faloo']){const f=fixture(p);const r=f.run();assert.equal(r.title,'原创一章',p);assert.deepEqual(r.paragraphs,['旅人沿着山路走进小屋。'],p);assert.equal(r.declared_words,11);}
 const f=fixture('shuqi');f.context.location.href+='&token=discarded#secret';assert.equal(f.run().url,'https://www.shuqi.com/reader?bid=123&cid=2');
});
test('unknown word counts, images, live challenges, hidden text and subscriptions never become saved prose',()=>{
 for(const p of ['zongheng','17k','qimao','ciweimao','faloo']){
  let f=fixture(p);f.info.childNodes[0].textContent='字数未知';assert.equal(f.run().error,'word_count',p);
   f=fixture(p);f.reader.querySelectorAll=s=>s==='img,canvas,iframe,svg'?[element('','IMG')]:s===':scope > p'?[f.paragraph]:[];assert.equal(f.run().error,'protected',p);
  f=fixture(p);f.paragraph.style={display:'none'};assert.equal(f.run().error,'reader',p);
  f=fixture(p);f.paragraph.childNodes[0].textContent='订阅本章后阅读';assert.equal(f.run().error,'restricted',p);
  f=fixture(p);f.document.querySelectorAll=()=>[{...element(),title:'安全验证'}];assert.equal(f.run().error,'verification',p);
 }
 const f=fixture('ciweimao');f.context.HB.book.book_id=999;assert.equal(f.run().error,'identity');
});
test('ciweimao excludes comment counters and the repeated inline marker but rejects unknown markup',()=>{
 const f=fixture('ciweimao');const marker=element('a1B2c3','SPAN'),counter=element('12','I');counter.matches=s=>s==='.J_Num';
 f.paragraph.children=[marker,counter];f.paragraph.childNodes.push(marker,counter);f.nodes['#J_BookCnt h1.chapter'].childNodes.push(counter);
 assert.equal(f.run().title,'原创一章');assert.deepEqual(f.run().paragraphs,['旅人沿着山路走进小屋。']);
 marker.childNodes[0].textContent='这里是真实文字';assert.equal(f.run().error,'protected');
});
test('ciweimao identified chapter illustrations become explicit TXT placeholders, never image prose',()=>{
 const f=fixture('ciweimao'),img=element('','IMG');img.src='https://e1.kuangxiangit.com/uploads/chapterimgsnew/1/2/3/999-123-2.jpg';
 f.paragraph.children=[img];f.paragraph.childNodes.push(img);
 f.reader.querySelectorAll=s=>s==='img,canvas,iframe,svg'?[img]:s===':scope > p'||s===':scope > p.chapter'?[f.paragraph]:[];
 assert.match(f.run().paragraphs[0],/TXT 未保存图片/);
 img.src='https://e1.kuangxiangit.com/uploads/chapterimgsnew/1/2/3/999-456-2.jpg';assert.equal(f.run().error,'protected');
});
